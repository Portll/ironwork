//! Running a command against the task: what it asks of the executor (`CicsHost`), reading and
//! storing its options, the EXEC interface block, raising conditions, and dispatch to the services.

use super::command::{Cics, CicsCommand, Datum, Record, Resp};
use super::{Condition, Task, file_control, maps, program, services};
use crate::abend::{Abend, AbendCode, Ending};
use crate::bms::Mapset;
use crate::host::Host;
use crate::lir::{ParaId, Step};
use crate::storage::{Loc, Val};
use crate::store::{self, ProgramFacts};
use crate::unit::{ADDRESS_BASE, Loader, RunUnit, UnitHost};
use crate::vocab::Pos;
use numeric::precision::{Fixed, Places};
use std::collections::HashMap;
use zarch::decimal::{self, Decimal};
use zarch::ebcdic::{self, CodePage};

pub(super) type R<T> = Result<T, Abend>;

pub const EIBTIME: usize = 0x00;
pub const EIBDATE: usize = 0x04;
pub const EIBTRNID: usize = 0x08;
pub const EIBTASKN: usize = 0x0C;
pub const EIBTRMID: usize = 0x10;
pub const EIBCPOSN: usize = 0x16;
pub const EIBCALEN: usize = 0x18;
pub const EIBAID: usize = 0x1A;
pub const EIBFN: usize = 0x1B;
pub const EIBRSRCE: usize = 0x33;
pub const EIBRESP: usize = 0x4C;
pub const EIBRESP2: usize = 0x50;

/// What a command asks of the executor running it beyond `Host` and the run unit: the logical
/// level's handlers, operands that are not data items, names only the executor can resolve, and
/// running a program for LINK and XCTL.
pub trait CicsHost<'w, P: Copy, O, S>: Host<P> + UnitHost<'w> {
    fn handlers(&mut self) -> &mut Handlers;
    /// An operand's bytes as CALL BY CONTENT passes them.
    fn content(&mut self, operand: &O, pos: Pos) -> R<Vec<u8>>;
    /// An operand's value as a subscript takes it.
    fn integer_of(&mut self, operand: &O, pos: Pos) -> R<i64>;
    fn text(&self, text: &S) -> String;
    /// This activation of the program, which a HANDLE ABEND LABEL it sets belongs to.
    fn activation(&self) -> u64;
    fn program_id(&self) -> String;
    /// Where DFHCOMMAREA is, when the program has one with an address.
    fn commarea(&self) -> Option<usize>;
    /// A mapset from the copy libraries; None when no library holds it.
    fn mapset(&mut self, name: &str) -> Option<Result<Mapset, String>>;
    /// The data item a name alone refers to, when it names one.
    fn item_named(&mut self, name: &str, pos: Pos) -> R<Option<Loc>>;
    /// The data item a name alone refers to, or the abend a reference to it gives.
    fn locate_named(&mut self, name: &str, pos: Pos) -> R<Loc>;
    /// Runs program `index` from its start with DFHEIBLK and `commarea` as its USING items; for
    /// XCTL, in this program's place at its logical level, with the level's HANDLE ABEND exit
    /// (C239).
    fn run_program(&mut self, program: Self::Program, index: usize, commarea: Option<usize>, xctl: bool) -> R<Ending>;
}

/// HANDLE CONDITION, IGNORE CONDITION and HANDLE ABEND, which belong to the logical level: every
/// program CALLed at it shares them (C234).
#[derive(Clone, Debug, Default)]
pub struct Handlers {
    pub conditions: HashMap<Condition, Handler>,
    /// What each PUSH HANDLE at this logical level suspended.
    pub stack: Vec<(HashMap<Condition, Handler>, Option<AbendExit>)>,
    pub abend: Option<AbendExit>,
}

impl Handlers {
    /// PUSH HANDLE: suspends HANDLE CONDITION, IGNORE CONDITION and HANDLE ABEND until `pop`.
    pub fn push(&mut self) {
        let suspended = (std::mem::take(&mut self.conditions), self.abend.take());
        self.stack.push(suspended);
    }

    /// POP HANDLE: false when no PUSH HANDLE at this logical level is left to undo.
    pub fn pop(&mut self) -> bool {
        let Some((conditions, abend)) = self.stack.pop() else { return false };
        self.conditions = conditions;
        self.abend = abend;
        true
    }

    /// The handlers a CALLed program starts with: the level's, which a dynamic CALL first
    /// suspends with the PUSH HANDLE of CBLPSHPOP(ON) (C234).
    pub fn lend(&mut self, pushes: bool) -> Handlers {
        let mut level = std::mem::take(self);
        if pushes {
            level.push();
        }
        level
    }

    /// Takes the level's handlers back from a CALLed program that has ended, as it left them; with
    /// `pops`, for a dynamic CALL it returned from, the POP HANDLE that undoes the CALL's PUSH, which
    /// changes nothing when no PUSH is left (C234).
    pub fn take_back(&mut self, callee: &mut Handlers, pops: bool) {
        *self = std::mem::take(callee);
        if pops {
            self.pop();
        }
    }
}

/// The program level's HANDLE ABEND exit, which CANCEL and entering it deactivate and RESET
/// reactivates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AbendExit {
    pub target: ExitTarget,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExitTarget {
    /// A paragraph of the program activation `owner` that set the exit, entered as by a GO TO at
    /// the HANDLE ABEND command `at`.
    Label { paragraph: ParaId, owner: u64, at: Pos },
    /// A program, entered as by LINK with the COMMAREA and EIBCALEN of the program that set the
    /// exit, when it had a COMMAREA.
    Program { name: String, commarea: Option<(usize, i16)> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handler {
    /// A paragraph of the program activation `owner` that issued the HANDLE CONDITION (C235).
    Label { paragraph: ParaId, owner: u64 },
    Ignore,
}

/// Where control goes after a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Next,
    /// A handled condition.
    GoTo(ParaId),
    /// RETURN, XCTL, or a LINKed program's STOP RUN.
    End(Ending),
}

impl From<Flow> for Step {
    fn from(flow: Flow) -> Self {
        match flow {
            Flow::Next => Step::Next,
            Flow::GoTo(p) => Step::GoTo(p),
            Flow::End(e) => Step::End(e),
        }
    }
}

/// A command's name as written, which messages give, its RESP, RESP2 and NOHANDLE, and where it is.
pub struct At<'c, P, O, S> {
    pub name: &'c str,
    pub resp: &'c Resp<P, O, S>,
    pub pos: Pos,
}

/// EXEC CICS is refused outside a CICS task.
pub fn in_task<H, L: Loader<H>>(unit: &RunUnit<'_, H, L>, name: &str, pos: Pos) -> R<()> {
    match unit.cics {
        Some(_) => Ok(()),
        None => Err(Abend::ironwork(format!("EXEC CICS {name} was reached outside a CICS task: run the program with `ironwork cics`"), pos)),
    }
}

/// Every command starts with EIBRESP and EIBRESP2 zero.
pub fn begin_command<H, L: Loader<H>>(unit: &mut RunUnit<'_, H, L>) {
    eib_fullword(unit, EIBRESP, 0);
    eib_fullword(unit, EIBRESP2, 0);
}

/// Runs a command.
pub fn run<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, command: &CicsCommand<P, O, S>, pos: Pos) -> R<Flow> {
    let name = x.text(&command.name);
    in_task(x.unit(), &name, pos)?;
    begin_command(x.unit());
    let at = At { name: &name, resp: &command.resp, pos };
    match &command.command {
        Cics::File { verb, file, options } => file_control::run(x, &at, *verb, file.as_ref(), options),
        Cics::Return { transid, commarea, length, channel, immediate } => program::cics_return(x, &at, transid.as_ref(), commarea.as_ref(), length.as_ref(), channel.is_some() || *immediate),
        Cics::Link(t) => program::link(x, &at, t, false),
        Cics::Xctl(t) => program::link(x, &at, t, true),
        Cics::Abend { abcode, cancel } => program::abend(x, &at, abcode.as_ref(), *cancel),
        Cics::HandleCondition(labels) => program::handle_condition(x, &at, labels),
        Cics::IgnoreCondition(conditions) => program::ignore_condition(x, &at, conditions),
        Cics::PushHandle => program::push_handle(x, &at),
        Cics::PopHandle => program::pop_handle(x, &at),
        Cics::HandleAbend { program, label, reset } => program::handle_abend(x, &at, program.as_ref(), *label, *reset),
        Cics::HandleAid | Cics::Freemain | Cics::Enq | Cics::Deq | Cics::Delay => ok(x, &at),
        Cics::SendMap { map, mapset, from, maponly, dataonly, cursor, control } => {
            maps::send_map(x, &at, maps::MapNames { map: map.as_ref(), mapset: mapset.as_ref() }, from.as_ref(), (*maponly, *dataonly), cursor.as_ref(), *control)
        }
        Cics::ReceiveMap { map, mapset, into, set } => maps::receive_map(x, &at, maps::MapNames { map: map.as_ref(), mapset: mapset.as_ref() }, into.as_ref(), set.as_ref()),
        Cics::SendControl { cursor, control } => maps::send_control(x, &at, cursor.as_ref(), *control),
        Cics::Receive(record) => maps::receive_raw(x, &at, record),
        Cics::Asktime { abstime } => services::asktime(x, &at, abstime.as_ref()),
        Cics::Formattime { abstime, datesep, timesep, outputs } => services::formattime(x, &at, abstime.as_ref(), datesep.as_ref(), timesep.as_ref(), outputs),
        Cics::Assign(assign) => services::assign(x, &at, assign),
        Cics::Getmain { flength, length, initimg, set } => services::getmain(x, &at, flength.as_ref(), length.as_ref(), initimg.as_ref(), set.as_ref()),
        Cics::Syncpoint { rollback } => services::syncpoint(x, &at, *rollback),
        Cics::Address { eib, commarea, cwa, twa } => services::address(x, &at, eib.as_ref(), commarea.as_ref(), cwa.as_ref(), twa.as_ref()),
        Cics::SendText { from, length } => services::send_text(x, &at, from.as_ref(), length.as_ref()),
        Cics::WriteOperator { text, textlength } => services::write_operator(x, &at, text.as_ref(), textlength.as_ref()),
        Cics::WriteqTs { queue, from, length, rewrite, item, numitems } => {
            services::writeq_ts(x, &at, queue.as_ref(), (from.as_ref(), length.as_ref()), *rewrite, item.as_ref(), numitems.as_ref())
        }
        Cics::ReadqTs { queue, next, item, numitems, record } => services::readq_ts(x, &at, queue.as_ref(), *next, item.as_ref(), numitems.as_ref(), record),
        Cics::DeleteqTs { queue } => services::deleteq_ts(x, &at, queue.as_ref()),
        Cics::WriteqTd { queue, from, length } => services::writeq_td(x, &at, queue.as_ref(), from.as_ref(), length.as_ref()),
        Cics::ReadqTd { queue, record } => services::readq_td(x, &at, queue.as_ref(), record),
        Cics::DeleteqTd { queue } => services::deleteq_td(x, &at, queue.as_ref()),
        Cics::Unsupported => Err(Abend::ironwork(unsupported(&name), pos)),
    }
}

/// The abend message for a command ironwork does not carry out.
pub fn unsupported(name: &str) -> String {
    format!("EXEC CICS {name} is not supported yet")
}

/// The task; `run` has already refused to run a command outside one.
pub(super) fn task<'a, 'w: 'a, P: Copy + 'a, O: 'a, S: 'a>(x: &'a mut impl CicsHost<'w, P, O, S>) -> &'a mut Task {
    x.unit().cics.as_mut().expect("EXEC CICS runs only in a task")
}

pub(super) fn page<'w, P: Copy, O, S>(x: &impl CicsHost<'w, P, O, S>) -> &'static CodePage {
    x.facts().page()
}

/// Text in the program's code page, a character it cannot encode as a space.
pub(super) fn encoded(page: &CodePage, text: &str) -> Vec<u8> {
    text.chars().map(|c| page.encode_char(c).unwrap_or(ebcdic::SPACE)).collect()
}

/// An option's bytes: a data item's storage, or another operand's as CALL BY CONTENT passes it.
pub fn bytes<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, d: Option<&Datum<P, O, S>>, pos: Pos) -> R<Option<Vec<u8>>> {
    Ok(match d {
        Some(Datum::Place(p)) => {
            let loc = x.locate(*p, false)?;
            Some(store::bytes(x.mem(), loc).to_vec())
        }
        Some(Datum::Value(o)) => Some(x.content(o, pos)?),
        _ => None,
    })
}

/// An option's bytes, cut to a length option when that is shorter.
pub(super) fn bytes_cut<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, data: Option<&Datum<P, O, S>>, length: Option<&Datum<P, O, S>>, pos: Pos) -> R<Option<Vec<u8>>> {
    let Some(mut bytes) = bytes(x, data, pos)? else { return Ok(None) };
    if let Some(n) = int(x, length, pos)? {
        bytes.truncate(n.max(0) as usize);
    }
    Ok(Some(bytes))
}

/// FROM's bytes cut to LENGTH (or TEXT and TEXTLENGTH for WRITE OPERATOR); `from` names it.
pub(super) fn sent<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, data: Option<&Datum<P, O, S>>, length: Option<&Datum<P, O, S>>, from: &str) -> R<Vec<u8>> {
    bytes_cut(x, data, length, at.pos)?.ok_or_else(|| Abend::ironwork(format!("EXEC CICS {} needs {from}", at.name), at.pos))
}

pub(super) fn int<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, d: Option<&Datum<P, O, S>>, pos: Pos) -> R<Option<i64>> {
    match d {
        Some(Datum::Place(p)) => x.integer(*p, pos).map(Some),
        Some(Datum::Value(o)) => x.integer_of(o, pos).map(Some),
        _ => Ok(None),
    }
}

/// A resource name (PROGRAM, TRANSID, QUEUE, FILE, ABCODE) with trailing spaces removed; text as
/// written loses its quotes.
pub(super) fn text<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, d: Option<&Datum<P, O, S>>, pos: Pos) -> R<Option<String>> {
    if let Some(Datum::Text(t)) = d {
        return Ok(Some(x.text(t).trim().trim_matches(|c| c == '\'' || c == '"').to_owned()));
    }
    let page = page(x);
    Ok(bytes(x, d, pos)?.map(|b| page.decode(&b).trim_end().to_owned()))
}

/// Locates a receiver the command only writes: its old bytes reach nothing, so they are not read.
fn receiver<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, p: P) -> R<Loc> {
    let was = x.unit().writing(true);
    let loc = x.locate(p, false);
    x.unit().writing(was);
    loc
}

/// MOVEs bytes into the data item an option names, as INTO and the like receive them; `name` is
/// the option's.
pub(super) fn store_bytes<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, d: Option<&Datum<P, O, S>>, name: &str, bytes: &[u8]) -> R<()> {
    match d {
        Some(Datum::Place(p)) => {
            let loc = receiver(x, *p)?;
            x.assign(loc, Val::Bytes(bytes.to_vec()), None, at.pos)
        }
        Some(Datum::Value(_)) => Err(Abend::ironwork(format!("EXEC CICS {}: {name} must name a data item", at.name), at.pos)),
        _ => Ok(()),
    }
}

pub(super) fn store_int<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, d: Option<&Datum<P, O, S>>, value: i64, pos: Pos) -> R<()> {
    match d {
        Some(Datum::Place(p)) => {
            let dest = receiver(x, *p)?;
            x.store_fixed(dest, &Fixed::new(i128::from(value), Places::new(19, 0)), pos)
        }
        _ => Ok(()),
    }
}

/// Stores an address, or NULL for None, into the POINTER an option names.
pub(super) fn store_pointer<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, d: Option<&Datum<P, O, S>>, offset: Option<usize>, pos: Pos) -> R<()> {
    if let Some(Datum::Place(p)) = d {
        let loc = receiver(x, *p)?;
        let address = offset.map_or(0, |o| ADDRESS_BASE + o as u32);
        x.assign(loc, Val::Address(address), None, pos)?;
    }
    Ok(())
}

pub(super) fn eib_bytes<H, L: Loader<H>>(unit: &mut RunUnit<'_, H, L>, offset: usize, bytes: &[u8]) {
    let at = unit.eib + offset;
    unit.write(at, bytes);
}

pub(super) fn eib_halfword<H, L: Loader<H>>(unit: &mut RunUnit<'_, H, L>, offset: usize, value: i16) {
    eib_bytes(unit, offset, &value.to_be_bytes());
}

pub(super) fn eib_fullword<H, L: Loader<H>>(unit: &mut RunUnit<'_, H, L>, offset: usize, value: i32) {
    eib_bytes(unit, offset, &value.to_be_bytes());
}

/// A 4-byte packed EIB field: seven digits and a sign.
pub(super) fn eib_packed<H, L: Loader<H>>(unit: &mut RunUnit<'_, H, L>, offset: usize, value: i64) {
    let mut field = [0u8; 4];
    let _ = decimal::encode(&mut field, Decimal { negative: value < 0, magnitude: value.unsigned_abs() as u128 });
    eib_bytes(unit, offset, &field);
}

/// An EIB text field, in EBCDIC, padded with spaces or cut to `len`.
pub(super) fn eib_text<H, L: Loader<H>>(unit: &mut RunUnit<'_, H, L>, page: &CodePage, offset: usize, len: usize, text: &str) {
    let mut bytes = encoded(page, text);
    bytes.resize(len, ebcdic::SPACE);
    eib_bytes(unit, offset, &bytes);
}

pub(super) fn eib_calen<H, L: Loader<H>>(unit: &RunUnit<'_, H, L>) -> i16 {
    let at = unit.eib + EIBCALEN;
    i16::from_be_bytes([unit.mem[at], unit.mem[at + 1]])
}

/// EIBCALEN and whether it may hold input, to put back as they were when a program run inside
/// a command returns.
pub(super) fn kept_calen<H, L: Loader<H>>(unit: &RunUnit<'_, H, L>) -> (i16, bool) {
    (eib_calen(unit), unit.holds_input(unit.eib + EIBCALEN, 2))
}

pub(super) fn restore_calen<H, L: Loader<H>>(unit: &mut RunUnit<'_, H, L>, (value, input): (i16, bool)) {
    eib_halfword(unit, EIBCALEN, value);
    unit.mark_input(unit.eib + EIBCALEN, 2, input);
}

/// Fills the EXEC interface block for the task's first program: time, date, transaction, task
/// number, terminal, COMMAREA length and the AID that started it.
pub fn begin_task<H: Clone, L: Loader<H>>(unit: &mut RunUnit<'_, H, L>, page: &CodePage, length: usize) {
    let (seconds, hundredths) = unit.now();
    let abstime = super::abstime(seconds, hundredths);
    let (transid, termid, number) = unit.cics.as_ref().map(|t| (t.transid.clone(), t.termid.clone(), t.number)).unwrap_or_default();
    eib_packed(unit, EIBTIME, super::eib_time(abstime));
    eib_packed(unit, EIBDATE, super::eib_date(abstime));
    eib_text(unit, page, EIBTRNID, 4, &transid);
    eib_packed(unit, EIBTASKN, i64::from(number));
    eib_text(unit, page, EIBTRMID, 4, &termid);
    eib_halfword(unit, EIBCALEN, length as i16);
    // The caller chose the COMMAREA, and the operator the key that started the task: both input.
    unit.mark_input(unit.eib + EIBCALEN, 2, length > 0);
    if let Some(aid) = unit.cics.as_ref().and_then(|t| t.initial_aid) {
        eib_bytes(unit, EIBAID, &[aid]);
        unit.mark_input(unit.eib + EIBAID, 1, true);
    }
}

/// Gives a record to INTO or SET and its length to LENGTH. A record longer than LENGTH (or the
/// INTO item) arrives cut short, with LENGERR.
pub(super) fn deliver<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, record: &Record<P, O, S>, data: &[u8]) -> R<Flow> {
    let limit = match int(x, record.length.as_ref(), at.pos)? {
        Some(n) => n.max(0) as usize,
        None => match &record.into {
            Some(Datum::Place(p)) => x.locate(*p, false)?.len,
            _ => data.len(),
        },
    };
    if record.set.is_some() {
        let area = x.unit().push_temporary(data);
        store_pointer(x, record.set.as_ref(), Some(area), at.pos)?;
    } else {
        store_bytes(x, at, record.into.as_ref(), "INTO", &data[..data.len().min(limit)])?;
    }
    store_int(x, record.length.as_ref(), data.len() as i64, at.pos)?;
    if data.len() > limit && record.set.is_none() {
        return raise(x, at, Condition::LENGERR, 0);
    }
    ok(x, at)
}

/// A command that succeeded: RESP and RESP2, when asked for, are zero.
pub fn ok<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>) -> R<Flow> {
    store_int(x, at.resp.resp.as_ref(), 0, at.pos)?;
    store_int(x, at.resp.resp2.as_ref(), 0, at.pos)?;
    Ok(Flow::Next)
}

/// Raises a condition. RESP or NOHANDLE take it; otherwise HANDLE CONDITION (the condition's own
/// entry, else ERROR) or IGNORE CONDITION decides, a label set by another activation abending
/// APC2 (C235); otherwise the task abends with the condition's AEIx code. A HANDLE ABEND exit can
/// intercept either abend.
pub fn raise<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, condition: Condition, resp2: i32) -> R<Flow> {
    let resp = condition.resp();
    eib_fullword(x.unit(), EIBRESP, resp);
    eib_fullword(x.unit(), EIBRESP2, resp2);
    if at.resp.resp.is_some() {
        store_int(x, at.resp.resp.as_ref(), i64::from(resp), at.pos)?;
        store_int(x, at.resp.resp2.as_ref(), i64::from(resp2), at.pos)?;
        return Ok(Flow::Next);
    }
    if at.resp.nohandle {
        return Ok(Flow::Next);
    }
    let me = x.activation();
    let handlers = x.handlers();
    match handlers.conditions.get(&condition).or_else(|| handlers.conditions.get(&Condition::ERROR)).copied() {
        Some(Handler::Ignore) => Ok(Flow::Next),
        Some(Handler::Label { paragraph, owner }) if owner == me => Ok(Flow::GoTo(paragraph)),
        Some(Handler::Label { .. }) => Err(Abend {
            code: AbendCode::Cics("APC2".into()),
            message: format!("EXEC CICS {}: {} was raised; its HANDLE CONDITION label is in a program that is not running there, which CICS cannot branch to", at.name, condition.name()),
            pos: at.pos,
            file: None,
        }),
        None => Err(Abend {
            code: AbendCode::Cics(condition.default_abend().into()),
            message: format!("EXEC CICS {}: {} was raised with no RESP, HANDLE CONDITION or IGNORE CONDITION", at.name, condition.name()),
            pos: at.pos,
            file: None,
        }),
    }
}
