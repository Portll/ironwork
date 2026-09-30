//! Program control and the handler table: RETURN, LINK, XCTL, ABEND, HANDLE CONDITION, IGNORE
//! CONDITION, PUSH and POP HANDLE, and HANDLE ABEND.

use super::Condition;
use super::command::{Datum, Transfer};
use super::run::{At, CicsHost, EIBCALEN, EIBFN, EIBRSRCE, Flow, Handler, R};
use super::run::{bytes_cut, eib_bytes, eib_calen, eib_halfword, eib_text, int, ok, page, raise, text};
use crate::abend::{Abend, AbendCode, Ending};
use crate::lir::ParaId;
use crate::unit::LoadError;

/// RETURN ends this program. At the task's top level TRANSID and COMMAREA name the next task and
/// what it starts with; in a LINKed program they raise INVREQ.
pub(super) fn cics_return<'w, P: Copy, O, S>(
    x: &mut impl CicsHost<'w, P, O, S>,
    at: &At<P, O, S>,
    transid: Option<&Datum<P, O, S>>,
    commarea: Option<&Datum<P, O, S>>,
    length: Option<&Datum<P, O, S>>,
) -> R<Flow> {
    eib_bytes(x.unit(), EIBFN, &[0x0E, 0x08]);
    let transid = text(x, transid, at.pos)?;
    let commarea = bytes_cut(x, commarea, length, at.pos)?;
    if !x.main() && (transid.is_some() || commarea.is_some()) {
        return raise(x, at, Condition::INVREQ, 0);
    }
    if let Some(task) = x.unit().cics.as_mut() {
        if let Some(t) = transid {
            task.next_transid = Some(t.to_ascii_uppercase());
        }
        if commarea.is_some() {
            task.returned_commarea = commarea;
        }
    }
    Ok(Flow::End(Ending::Goback))
}

/// LINK runs a program and comes back; XCTL runs it in this program's place. A LINKed program gets
/// the COMMAREA item itself; XCTL passes a copy, since this program's storage goes away. Each LINK
/// or XCTL starts the program with fresh WORKING-STORAGE, as CICS gives it.
pub(super) fn link<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, t: &Transfer<P, O, S>, xctl: bool) -> R<Flow> {
    eib_bytes(x.unit(), EIBFN, if xctl { &[0x0E, 0x04] } else { &[0x0E, 0x02] });
    let Some(name) = text(x, t.program.as_ref(), at.pos)?.map(|n| n.to_ascii_uppercase()) else {
        return Err(Abend::ironwork(format!("EXEC CICS {} needs PROGRAM", at.name), at.pos));
    };
    let page = page(x);
    eib_text(x.unit(), page, EIBRSRCE, 8, &name);
    let index = match x.unit().load(&name) {
        Ok(i) => i,
        Err(LoadError::NotFound) => return raise(x, at, Condition::PGMIDERR, 0),
        Err(LoadError::Compile(m)) => return Err(Abend::ironwork(format!("EXEC CICS {} PROGRAM({name}): {m}", at.name), at.pos)),
    };
    let Some(program) = x.unit().programs[index].compiled.clone() else {
        return Err(Abend::ironwork(format!("EXEC CICS {} PROGRAM({name}): the task's first program is already running", at.name), at.pos));
    };
    let mark = x.unit().mem.len();
    let (area, item_len) = match &t.commarea {
        Some(Datum::Place(p)) if !xctl => {
            let loc = x.locate(*p, false)?;
            (Some(loc.offset), loc.len)
        }
        Some(Datum::Place(_) | Datum::Value(_)) => {
            let bytes = bytes_cut(x, t.commarea.as_ref(), t.length.as_ref(), at.pos)?.unwrap_or_default();
            (Some(x.unit().push_temporary(&bytes)), bytes.len())
        }
        _ => (None, 0),
    };
    let length = int(x, t.length.as_ref(), at.pos)?.map_or(item_len, |n| n.max(0) as usize);
    let saved = eib_calen(x.unit());
    eib_halfword(x.unit(), EIBCALEN, length as i16);
    x.unit().programs[index].initialized = false;
    x.unit().enter(at.pos)?;
    let ending = x.run_program(program, index, area, xctl);
    let unit = x.unit();
    unit.depth -= 1;
    unit.programs[index].active = false;
    unit.release_temporaries(mark);
    eib_halfword(unit, EIBCALEN, saved);
    match ending? {
        Ending::StopRun => Ok(Flow::End(Ending::StopRun)),
        _ if xctl => Ok(Flow::End(Ending::Goback)),
        _ => ok(x, at),
    }
}

/// ABEND ends the task with ABCODE, unless HANDLE ABEND LABEL is active and CANCEL is absent.
pub(super) fn abend<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, abcode: Option<&Datum<P, O, S>>, cancel: bool) -> R<Flow> {
    let code = text(x, abcode, at.pos)?.unwrap_or_else(|| "????".into());
    if !cancel && let Some(p) = x.handlers().abend.take() {
        return Ok(Flow::GoTo(p));
    }
    Err(Abend { message: format!("EXEC CICS ABEND ABCODE({code})"), code: AbendCode::Cics(code), pos: at.pos, file: None })
}

pub(super) fn handle_condition<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, labels: &[(Condition, Option<ParaId>)]) -> R<Flow> {
    let conditions = &mut x.handlers().conditions;
    for &(condition, label) in labels {
        match label {
            Some(p) => conditions.insert(condition, Handler::Label(p)),
            None => conditions.remove(&condition),
        };
    }
    ok(x, at)
}

pub(super) fn ignore_condition<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, ignored: &[Condition]) -> R<Flow> {
    let conditions = &mut x.handlers().conditions;
    for &condition in ignored {
        conditions.insert(condition, Handler::Ignore);
    }
    ok(x, at)
}

pub(super) fn push_handle<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>) -> R<Flow> {
    let handlers = x.handlers();
    let saved = std::mem::take(&mut handlers.conditions);
    handlers.stack.push(saved);
    ok(x, at)
}

pub(super) fn pop_handle<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>) -> R<Flow> {
    let handlers = x.handlers();
    match handlers.stack.pop() {
        Some(saved) => {
            handlers.conditions = saved;
            ok(x, at)
        }
        None => raise(x, at, Condition::INVREQ, 0),
    }
}

/// `reset` is CANCEL or RESET.
pub(super) fn handle_abend<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, program: bool, label: Option<ParaId>, reset: bool) -> R<Flow> {
    if program {
        return Err(Abend::ironwork("EXEC CICS HANDLE ABEND PROGRAM is not supported yet; use LABEL", at.pos));
    }
    if let Some(p) = label {
        x.handlers().abend = Some(p);
    } else if reset {
        x.handlers().abend = None;
    }
    ok(x, at)
}
