//! Program control and the handler table: RETURN, LINK, XCTL, ABEND, HANDLE CONDITION, IGNORE
//! CONDITION, PUSH and POP HANDLE, and HANDLE ABEND.

use super::Condition;
use super::command::{Datum, Transfer};
use super::run::{AbendExit, At, CicsHost, EIBCALEN, EIBFN, EIBRSRCE, ExitTarget, Flow, Handler, Handlers, R};
use super::run::{bytes_cut, eib_bytes, eib_calen, eib_halfword, eib_text, int, kept_calen, ok, page, raise, restore_calen, task, text};
use crate::abend::{Abend, AbendCode, Ending};
use crate::callee::{self, By, Callee};
use crate::lir::ParaId;
use crate::unit::{LoadError, Loader, RunUnit};
use crate::vocab::Pos;

/// RETURN ends this program's logical level, CALLed programs and all: the program that LINKed to
/// it goes on, or the task ends (C233). At the task's first level TRANSID and COMMAREA name the
/// next task and what it starts with; below it they raise INVREQ.
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
    if task(x).links > 0 && (transid.is_some() || commarea.is_some()) {
        return raise(x, at, Condition::INVREQ, 0);
    }
    let task = task(x);
    if let Some(t) = transid {
        task.next_transid = Some(t.to_ascii_uppercase());
    }
    if commarea.is_some() {
        task.returned_commarea = commarea;
    }
    task.ending_level = true;
    Ok(Flow::End(Ending::Goback))
}

/// RETURN or XCTL has ended the logical level: the program whose CALL has just come back ends too
/// (C233).
pub fn level_ended<H, L: Loader<H>>(unit: &RunUnit<'_, H, L>) -> bool {
    unit.cics.as_ref().is_some_and(|t| t.ending_level)
}

/// LINK runs a program and comes back; XCTL runs it in place of the program running this logical
/// level, with the level's HANDLE ABEND exit (C239), and the level ends when it does (C233). A
/// LINKed program gets the COMMAREA item itself; XCTL passes a copy, since this program's storage
/// goes away. Each LINK or XCTL starts the program with fresh WORKING-STORAGE, as CICS gives it.
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
    let saved = kept_calen(x.unit());
    eib_halfword(x.unit(), EIBCALEN, length as i16);
    let ending = enter(x, program, index, area, xctl, at.pos);
    let unit = x.unit();
    unit.release_temporaries(mark);
    restore_calen(unit, saved);
    match ending? {
        Ending::StopRun => Ok(Flow::End(Ending::StopRun)),
        _ if xctl => {
            task(x).ending_level = true;
            Ok(Flow::End(Ending::Goback))
        }
        _ => ok(x, at),
    }
}

/// Runs program `index` with fresh WORKING-STORAGE at the next logical level, or for XCTL in this
/// program's place; the level it ran has ended when it comes back.
fn enter<'w, P: Copy, O, S, X: CicsHost<'w, P, O, S>>(x: &mut X, program: X::Program, index: usize, area: Option<usize>, xctl: bool, pos: Pos) -> R<Ending> {
    let below = u32::from(!xctl);
    task(x).links += below;
    let callee = Callee { index, by: By::Link, mark: None, pos };
    let ran = callee::run(x, &callee, |x| {
        x.unit().enter(pos)?;
        let ending = x.run_program(program, index, area, xctl);
        x.unit().depth -= 1;
        Ok::<_, Abend>((ending, ()))
    });
    let task = task(x);
    task.links -= below;
    task.ending_level = false;
    ran?.0
}

/// ABEND ends the task with ABCODE; a HANDLE ABEND exit can intercept it unless CANCEL is given.
pub(super) fn abend<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, abcode: Option<&Datum<P, O, S>>, cancel: bool) -> R<Flow> {
    let code = text(x, abcode, at.pos)?.unwrap_or_else(|| "????".into());
    if cancel && let Some(task) = x.unit().cics.as_mut() {
        task.cancelling = true;
    }
    Err(Abend { message: format!("EXEC CICS ABEND ABCODE({code})"), code: AbendCode::Cics(code), pos: at.pos, file: None })
}

/// The transaction abend code an abend is in a CICS task, when a HANDLE ABEND exit can intercept
/// it: a program check is ASRA; ASPx and APSJ, and ironwork's own refusals, cannot be.
fn interceptable(abend: &Abend) -> Option<String> {
    match &abend.code {
        AbendCode::Check(_) | AbendCode::Protection => Some("ASRA".into()),
        AbendCode::Cics(code) if !code.starts_with("ASP") && code != "APSJ" => Some(code.clone()),
        _ => None,
    }
}

/// Where an abend that reaches activation `me` goes: to the level's HANDLE ABEND exit when it is
/// active, which CICS deactivates on the way in (C142). A LABEL `me` set is a GO TO in it, and one
/// another activation set abends APC2 (C238). A PROGRAM is entered only where `me` runs the level,
/// as the task, LINK or XCTL started it; in a CALLed program the abend goes on with the exit still
/// active. None passes the abend on.
pub fn abend_exit<H, L: Loader<H>>(unit: &mut RunUnit<'_, H, L>, handlers: &mut Handlers, abend: &Abend, me: u64, runs_level: bool) -> R<Option<ExitTarget>> {
    let Some(code) = interceptable(abend) else { return Ok(None) };
    let Some(task) = unit.cics.as_mut().filter(|t| !t.cancelling) else { return Ok(None) };
    let Some(exit) = handlers.abend.as_mut().filter(|e| e.active && (runs_level || matches!(e.target, ExitTarget::Label { .. }))) else {
        return Ok(None);
    };
    exit.active = false;
    task.abcode = Some(code);
    task.ending_level = false;
    match &exit.target {
        ExitTarget::Label { owner, .. } if *owner != me => Err(Abend {
            message: format!("{}; the HANDLE ABEND LABEL is in a program that is not running there, which CICS cannot branch to", abend.message),
            code: AbendCode::Cics("APC2".into()),
            pos: abend.pos,
            file: abend.file.clone(),
        }),
        target => Ok(Some(target.clone())),
    }
}

/// A HANDLE ABEND PROGRAM exit, entered as by LINK with the COMMAREA and EIBCALEN of the program
/// that set it, else of the program running the level. One that cannot be loaded abends APCT,
/// which passes to the next higher level.
pub fn enter_exit_program<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, name: &str, commarea: Option<(usize, i16)>, pos: Pos) -> R<Ending> {
    let apct = |message: String| Abend { code: AbendCode::Cics("APCT".into()), message, pos, file: None };
    let index = match x.unit().load(name) {
        Ok(i) => i,
        Err(LoadError::NotFound) => return Err(apct(format!("HANDLE ABEND PROGRAM({name}): no program of the name"))),
        Err(LoadError::Compile(m)) => return Err(Abend::ironwork(format!("HANDLE ABEND PROGRAM({name}): {m}"), pos)),
    };
    let Some(program) = x.unit().programs[index].compiled.clone() else {
        return Err(apct(format!("HANDLE ABEND PROGRAM({name}): the task's first program is already running")));
    };
    let page = page(x);
    eib_text(x.unit(), page, EIBRSRCE, 8, name);
    let saved = kept_calen(x.unit());
    let (area, length) = match commarea {
        Some((area, length)) => (Some(area), length),
        None => (x.commarea(), saved.0),
    };
    eib_halfword(x.unit(), EIBCALEN, length);
    let ending = enter(x, program, index, area, false, pos);
    restore_calen(x.unit(), saved);
    ending
}

pub(super) fn handle_condition<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, labels: &[(Condition, Option<ParaId>)]) -> R<Flow> {
    let owner = x.activation();
    let conditions = &mut x.handlers().conditions;
    for &(condition, label) in labels {
        match label {
            Some(paragraph) => conditions.insert(condition, Handler::Label { paragraph, owner }),
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

/// PUSH HANDLE suspends HANDLE CONDITION, IGNORE CONDITION and HANDLE ABEND until POP HANDLE.
pub(super) fn push_handle<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>) -> R<Flow> {
    x.handlers().push();
    ok(x, at)
}

pub(super) fn pop_handle<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>) -> R<Flow> {
    if x.handlers().pop() { ok(x, at) } else { raise(x, at, Condition::INVREQ, 0) }
}

/// HANDLE ABEND PROGRAM or LABEL replaces the program level's exit, active; RESET reactivates it
/// and CANCEL, the default, deactivates it (API Reference SC34-7402-00, pp. 314-315). PROGRAM
/// names a program a LINK could find, else PGMIDERR, and keeps this program's COMMAREA for it.
pub(super) fn handle_abend<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, program: Option<&Datum<P, O, S>>, label: Option<ParaId>, reset: bool) -> R<Flow> {
    let target = match (text(x, program, at.pos)?, label) {
        (Some(name), _) => {
            let name = name.to_ascii_uppercase();
            match x.unit().load(&name) {
                Ok(_) => Some(ExitTarget::Program { name, commarea: x.commarea().map(|area| (area, eib_calen(x.unit()))) }),
                Err(LoadError::NotFound) => return raise(x, at, Condition::PGMIDERR, 1),
                Err(LoadError::Compile(m)) => return Err(Abend::ironwork(format!("EXEC CICS HANDLE ABEND PROGRAM({name}): {m}"), at.pos)),
            }
        }
        (None, Some(p)) => Some(ExitTarget::Label { paragraph: p, owner: x.activation(), at: at.pos }),
        (None, None) => None,
    };
    let handlers = x.handlers();
    match (target, &mut handlers.abend) {
        (Some(target), exit) => *exit = Some(AbendExit { target, active: true }),
        (None, Some(exit)) => exit.active = reset,
        (None, None) => {}
    }
    ok(x, at)
}
