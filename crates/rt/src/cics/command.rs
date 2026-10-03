//! An EXEC CICS command as ironwork runs it (lir.md §9.5): which command, and each option it reads
//! as a place, a value, text or a bare keyword. `P`, `O` and `S` are the executor's handles to a
//! data item, to any other operand and to text: the LIR's ids by default, the walker's own
//! references in the interpreter.

use super::Condition;
use crate::lir::{Operand, ParaId, PlaceId, SymId};
use crate::module::ModuleError;
use crate::module::codec::{Decode, Encode, Reader, Writer};
use crate::{codec_enum, codec_struct};

/// An option's argument: a data item, another operand (a literal, LENGTH OF and the like), text
/// the translator kept as written (a label, a name that is not a data item), or none at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Datum<P = PlaceId, O = Operand, S = SymId> {
    Place(P),
    Value(O),
    Text(S),
    Bare,
}

/// An option that may be absent.
pub type Opt<P = PlaceId, O = Operand, S = SymId> = Option<Datum<P, O, S>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CicsCommand<P = PlaceId, O = Operand, S = SymId> {
    /// The command as written, which messages name: SEND for SEND MAP written as SEND MAP(name).
    pub name: S,
    pub command: Cics<P, O, S>,
    pub resp: Resp<P, O, S>,
}

/// RESP, RESP2 and NOHANDLE, which decide what raising a condition does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resp<P = PlaceId, O = Operand, S = SymId> {
    pub resp: Opt<P, O, S>,
    pub resp2: Opt<P, O, S>,
    pub nohandle: bool,
}

/// One variant per command ironwork carries out; any other is `Unsupported`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cics<P = PlaceId, O = Operand, S = SymId> {
    File { verb: FileControl, file: Opt<P, O, S>, options: FileOptions<P, O, S> },
    /// CHANNEL and IMMEDIATE are kept only for the INVREQ they raise below the task's first level.
    Return { transid: Opt<P, O, S>, commarea: Opt<P, O, S>, length: Opt<P, O, S>, channel: Opt<P, O, S>, immediate: bool },
    Link(Transfer<P, O, S>),
    Xctl(Transfer<P, O, S>),
    Abend { abcode: Opt<P, O, S>, cancel: bool },
    /// Each condition with its paragraph, or None to remove its entry.
    HandleCondition(Vec<(Condition, Option<ParaId>)>),
    IgnoreCondition(Vec<Condition>),
    PushHandle,
    PopHandle,
    /// PROGRAM or LABEL sets the exit, RESET reactivates it, and with none of them it is CANCEL.
    HandleAbend { program: Opt<P, O, S>, label: Option<ParaId>, reset: bool },
    HandleAid,
    SendMap { map: Opt<P, O, S>, mapset: Opt<P, O, S>, from: Opt<P, O, S>, maponly: bool, dataonly: bool, cursor: Opt<P, O, S>, control: Control },
    ReceiveMap { map: Opt<P, O, S>, mapset: Opt<P, O, S>, into: Opt<P, O, S>, set: Opt<P, O, S> },
    SendControl { cursor: Opt<P, O, S>, control: Control },
    Receive(Record<P, O, S>),
    Asktime { abstime: Opt<P, O, S> },
    /// `outputs` are the options that name a date or time form, in the order written.
    Formattime { abstime: Opt<P, O, S>, datesep: Opt<P, O, S>, timesep: Opt<P, O, S>, outputs: Vec<(S, Datum<P, O, S>)> },
    Assign(Assign<P, O, S>),
    Getmain { flength: Opt<P, O, S>, length: Opt<P, O, S>, initimg: Opt<P, O, S>, set: Opt<P, O, S> },
    Freemain,
    Enq,
    Deq,
    Delay,
    /// Settles the SQL session's unit of work.
    Syncpoint { rollback: bool },
    Address { eib: Opt<P, O, S>, commarea: Opt<P, O, S>, cwa: Opt<P, O, S>, twa: Opt<P, O, S> },
    SendText { from: Opt<P, O, S>, length: Opt<P, O, S> },
    WriteOperator { text: Opt<P, O, S>, textlength: Opt<P, O, S> },
    WriteqTs { queue: Opt<P, O, S>, from: Opt<P, O, S>, length: Opt<P, O, S>, rewrite: bool, item: Opt<P, O, S>, numitems: Opt<P, O, S> },
    ReadqTs { queue: Opt<P, O, S>, next: bool, item: Opt<P, O, S>, numitems: Opt<P, O, S>, record: Record<P, O, S> },
    DeleteqTs { queue: Opt<P, O, S> },
    WriteqTd { queue: Opt<P, O, S>, from: Opt<P, O, S>, length: Opt<P, O, S> },
    ReadqTd { queue: Opt<P, O, S>, record: Record<P, O, S> },
    DeleteqTd { queue: Opt<P, O, S> },
    /// "EXEC CICS … is not supported yet", naming the command, when reached.
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileControl {
    Read,
    Write,
    Rewrite,
    Delete,
    Unlock,
    Startbr,
    Resetbr,
    Readnext,
    Readprev,
    Endbr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileOptions<P = PlaceId, O = Operand, S = SymId> {
    pub ridfld: Opt<P, O, S>,
    pub keylength: Opt<P, O, S>,
    pub reqid: Opt<P, O, S>,
    pub from: Opt<P, O, S>,
    pub numrec: Opt<P, O, S>,
    pub record: Record<P, O, S>,
    pub generic: bool,
    pub rrn: bool,
    pub gteq: bool,
    pub equal: bool,
    pub update: bool,
}

/// Where a record read arrives: INTO, or SET's pointer; LENGTH, which also limits it. WRITE's
/// LENGTH is the same option.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record<P = PlaceId, O = Operand, S = SymId> {
    pub into: Opt<P, O, S>,
    pub set: Opt<P, O, S>,
    pub length: Opt<P, O, S>,
}

/// LINK's and XCTL's options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transfer<P = PlaceId, O = Operand, S = SymId> {
    pub program: Opt<P, O, S>,
    pub commarea: Opt<P, O, S>,
    pub length: Opt<P, O, S>,
}

/// SEND MAP's and SEND CONTROL's write command and write control character options.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Control {
    pub erase: bool,
    pub freekb: bool,
    pub alarm: bool,
    pub frset: bool,
}

/// The ASSIGN options ironwork answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assign<P = PlaceId, O = Operand, S = SymId> {
    pub applid: Opt<P, O, S>,
    pub sysid: Opt<P, O, S>,
    pub userid: Opt<P, O, S>,
    pub netname: Opt<P, O, S>,
    pub facility: Opt<P, O, S>,
    pub startcode: Opt<P, O, S>,
    pub abcode: Opt<P, O, S>,
    pub program: Opt<P, O, S>,
    pub cwaleng: Opt<P, O, S>,
    pub twaleng: Opt<P, O, S>,
}

impl<P, O, S> Cics<P, O, S> {
    /// The variant's name, as the command it carries out is spelt.
    pub fn name(&self) -> &'static str {
        match self {
            Self::File { verb, .. } => verb.name(),
            Self::Return { .. } => "RETURN",
            Self::Link(_) => "LINK",
            Self::Xctl(_) => "XCTL",
            Self::Abend { .. } => "ABEND",
            Self::HandleCondition(_) => "HANDLE CONDITION",
            Self::IgnoreCondition(_) => "IGNORE CONDITION",
            Self::PushHandle => "PUSH HANDLE",
            Self::PopHandle => "POP HANDLE",
            Self::HandleAbend { .. } => "HANDLE ABEND",
            Self::HandleAid => "HANDLE AID",
            Self::SendMap { .. } => "SEND MAP",
            Self::ReceiveMap { .. } => "RECEIVE MAP",
            Self::SendControl { .. } => "SEND CONTROL",
            Self::Receive(_) => "RECEIVE",
            Self::Asktime { .. } => "ASKTIME",
            Self::Formattime { .. } => "FORMATTIME",
            Self::Assign(_) => "ASSIGN",
            Self::Getmain { .. } => "GETMAIN",
            Self::Freemain => "FREEMAIN",
            Self::Enq => "ENQ",
            Self::Deq => "DEQ",
            Self::Delay => "DELAY",
            Self::Syncpoint { .. } => "SYNCPOINT",
            Self::Address { .. } => "ADDRESS",
            Self::SendText { .. } => "SEND TEXT",
            Self::WriteOperator { .. } => "WRITE OPERATOR",
            Self::WriteqTs { .. } => "WRITEQ TS",
            Self::ReadqTs { .. } => "READQ TS",
            Self::DeleteqTs { .. } => "DELETEQ TS",
            Self::WriteqTd { .. } => "WRITEQ TD",
            Self::ReadqTd { .. } => "READQ TD",
            Self::DeleteqTd { .. } => "DELETEQ TD",
            Self::Unsupported => "unsupported",
        }
    }
}

/// Turns a command's handles into another executor's, as lowering turns the walker's references
/// into the LIR's ids.
pub trait Handles<P, O, S> {
    type Place;
    type Value;
    type Text;
    type Error;
    fn place(&mut self, place: P) -> Result<Self::Place, Self::Error>;
    fn value(&mut self, value: O) -> Result<Self::Value, Self::Error>;
    fn text(&mut self, text: S) -> Result<Self::Text, Self::Error>;
}

type Mapped<H, P, O, S, T> = Result<T, <H as Handles<P, O, S>>::Error>;
type DatumOf<H, P, O, S> = Datum<<H as Handles<P, O, S>>::Place, <H as Handles<P, O, S>>::Value, <H as Handles<P, O, S>>::Text>;
type OptOf<H, P, O, S> = Option<DatumOf<H, P, O, S>>;
type CommandOf<H, P, O, S> = CicsCommand<<H as Handles<P, O, S>>::Place, <H as Handles<P, O, S>>::Value, <H as Handles<P, O, S>>::Text>;
type CicsOf<H, P, O, S> = Cics<<H as Handles<P, O, S>>::Place, <H as Handles<P, O, S>>::Value, <H as Handles<P, O, S>>::Text>;
type RecordOf<H, P, O, S> = Record<<H as Handles<P, O, S>>::Place, <H as Handles<P, O, S>>::Value, <H as Handles<P, O, S>>::Text>;
type TransferOf<H, P, O, S> = Transfer<<H as Handles<P, O, S>>::Place, <H as Handles<P, O, S>>::Value, <H as Handles<P, O, S>>::Text>;

impl<P, O, S> Datum<P, O, S> {
    pub fn map<H: Handles<P, O, S>>(self, h: &mut H) -> Mapped<H, P, O, S, DatumOf<H, P, O, S>> {
        Ok(match self {
            Self::Place(p) => Datum::Place(h.place(p)?),
            Self::Value(o) => Datum::Value(h.value(o)?),
            Self::Text(s) => Datum::Text(h.text(s)?),
            Self::Bare => Datum::Bare,
        })
    }
}

fn opt<P, O, S, H: Handles<P, O, S>>(o: Opt<P, O, S>, h: &mut H) -> Mapped<H, P, O, S, OptOf<H, P, O, S>> {
    o.map(|d| d.map(h)).transpose()
}

impl<P, O, S> CicsCommand<P, O, S> {
    /// The same command over `h`'s handles, each mapped in the order the fields are written.
    pub fn map<H: Handles<P, O, S>>(self, h: &mut H) -> Mapped<H, P, O, S, CommandOf<H, P, O, S>> {
        let name = h.text(self.name)?;
        let command = self.command.map(h)?;
        let resp = Resp { resp: opt(self.resp.resp, h)?, resp2: opt(self.resp.resp2, h)?, nohandle: self.resp.nohandle };
        Ok(CicsCommand { name, command, resp })
    }
}

impl<P, O, S> Record<P, O, S> {
    fn map<H: Handles<P, O, S>>(self, h: &mut H) -> Mapped<H, P, O, S, RecordOf<H, P, O, S>> {
        Ok(Record { into: opt(self.into, h)?, set: opt(self.set, h)?, length: opt(self.length, h)? })
    }
}

impl<P, O, S> Transfer<P, O, S> {
    fn map<H: Handles<P, O, S>>(self, h: &mut H) -> Mapped<H, P, O, S, TransferOf<H, P, O, S>> {
        Ok(Transfer { program: opt(self.program, h)?, commarea: opt(self.commarea, h)?, length: opt(self.length, h)? })
    }
}

impl<P, O, S> Cics<P, O, S> {
    fn map<H: Handles<P, O, S>>(self, h: &mut H) -> Mapped<H, P, O, S, CicsOf<H, P, O, S>> {
        Ok(match self {
            Self::File { verb, file, options: o } => Cics::File {
                verb,
                file: opt(file, h)?,
                options: FileOptions {
                    ridfld: opt(o.ridfld, h)?,
                    keylength: opt(o.keylength, h)?,
                    reqid: opt(o.reqid, h)?,
                    from: opt(o.from, h)?,
                    numrec: opt(o.numrec, h)?,
                    record: o.record.map(h)?,
                    generic: o.generic,
                    rrn: o.rrn,
                    gteq: o.gteq,
                    equal: o.equal,
                    update: o.update,
                },
            },
            Self::Return { transid, commarea, length, channel, immediate } => {
                Cics::Return { transid: opt(transid, h)?, commarea: opt(commarea, h)?, length: opt(length, h)?, channel: opt(channel, h)?, immediate }
            }
            Self::Link(t) => Cics::Link(t.map(h)?),
            Self::Xctl(t) => Cics::Xctl(t.map(h)?),
            Self::Abend { abcode, cancel } => Cics::Abend { abcode: opt(abcode, h)?, cancel },
            Self::HandleCondition(labels) => Cics::HandleCondition(labels),
            Self::IgnoreCondition(conditions) => Cics::IgnoreCondition(conditions),
            Self::PushHandle => Cics::PushHandle,
            Self::PopHandle => Cics::PopHandle,
            Self::HandleAbend { program, label, reset } => Cics::HandleAbend { program: opt(program, h)?, label, reset },
            Self::HandleAid => Cics::HandleAid,
            Self::SendMap { map, mapset, from, maponly, dataonly, cursor, control } => {
                Cics::SendMap { map: opt(map, h)?, mapset: opt(mapset, h)?, from: opt(from, h)?, maponly, dataonly, cursor: opt(cursor, h)?, control }
            }
            Self::ReceiveMap { map, mapset, into, set } => Cics::ReceiveMap { map: opt(map, h)?, mapset: opt(mapset, h)?, into: opt(into, h)?, set: opt(set, h)? },
            Self::SendControl { cursor, control } => Cics::SendControl { cursor: opt(cursor, h)?, control },
            Self::Receive(record) => Cics::Receive(record.map(h)?),
            Self::Asktime { abstime } => Cics::Asktime { abstime: opt(abstime, h)? },
            Self::Formattime { abstime, datesep, timesep, outputs } => {
                let (abstime, datesep, timesep) = (opt(abstime, h)?, opt(datesep, h)?, opt(timesep, h)?);
                let outputs = outputs.into_iter().map(|(name, d)| Ok((h.text(name)?, d.map(h)?))).collect::<Result<_, _>>()?;
                Cics::Formattime { abstime, datesep, timesep, outputs }
            }
            Self::Assign(a) => Cics::Assign(Assign {
                applid: opt(a.applid, h)?,
                sysid: opt(a.sysid, h)?,
                userid: opt(a.userid, h)?,
                netname: opt(a.netname, h)?,
                facility: opt(a.facility, h)?,
                startcode: opt(a.startcode, h)?,
                abcode: opt(a.abcode, h)?,
                program: opt(a.program, h)?,
                cwaleng: opt(a.cwaleng, h)?,
                twaleng: opt(a.twaleng, h)?,
            }),
            Self::Getmain { flength, length, initimg, set } => Cics::Getmain { flength: opt(flength, h)?, length: opt(length, h)?, initimg: opt(initimg, h)?, set: opt(set, h)? },
            Self::Freemain => Cics::Freemain,
            Self::Enq => Cics::Enq,
            Self::Deq => Cics::Deq,
            Self::Delay => Cics::Delay,
            Self::Syncpoint { rollback } => Cics::Syncpoint { rollback },
            Self::Address { eib, commarea, cwa, twa } => Cics::Address { eib: opt(eib, h)?, commarea: opt(commarea, h)?, cwa: opt(cwa, h)?, twa: opt(twa, h)? },
            Self::SendText { from, length } => Cics::SendText { from: opt(from, h)?, length: opt(length, h)? },
            Self::WriteOperator { text, textlength } => Cics::WriteOperator { text: opt(text, h)?, textlength: opt(textlength, h)? },
            Self::WriteqTs { queue, from, length, rewrite, item, numitems } => {
                Cics::WriteqTs { queue: opt(queue, h)?, from: opt(from, h)?, length: opt(length, h)?, rewrite, item: opt(item, h)?, numitems: opt(numitems, h)? }
            }
            Self::ReadqTs { queue, next, item, numitems, record } => {
                Cics::ReadqTs { queue: opt(queue, h)?, next, item: opt(item, h)?, numitems: opt(numitems, h)?, record: record.map(h)? }
            }
            Self::DeleteqTs { queue } => Cics::DeleteqTs { queue: opt(queue, h)? },
            Self::WriteqTd { queue, from, length } => Cics::WriteqTd { queue: opt(queue, h)?, from: opt(from, h)?, length: opt(length, h)? },
            Self::ReadqTd { queue, record } => Cics::ReadqTd { queue: opt(queue, h)?, record: record.map(h)? },
            Self::DeleteqTd { queue } => Cics::DeleteqTd { queue: opt(queue, h)? },
            Self::Unsupported => Cics::Unsupported,
        })
    }

    /// The paragraphs HANDLE CONDITION and HANDLE ABEND name.
    pub fn labels(&self) -> Vec<ParaId> {
        match self {
            Self::HandleCondition(labels) => labels.iter().filter_map(|&(_, p)| p).collect(),
            Self::HandleAbend { label, .. } => label.iter().copied().collect(),
            _ => Vec::new(),
        }
    }
}

impl FileControl {
    pub fn name(self) -> &'static str {
        match self {
            Self::Read => "READ",
            Self::Write => "WRITE",
            Self::Rewrite => "REWRITE",
            Self::Delete => "DELETE",
            Self::Unlock => "UNLOCK",
            Self::Startbr => "STARTBR",
            Self::Resetbr => "RESETBR",
            Self::Readnext => "READNEXT",
            Self::Readprev => "READPREV",
            Self::Endbr => "ENDBR",
        }
    }
}

/// A condition's tag is its place in DFHRESP's order.
impl Encode for Condition {
    fn encode(&self, w: &mut Writer) {
        w.leb(*self as u64);
    }
}

impl Decode for Condition {
    fn decode(r: &mut Reader<'_>) -> Result<Self, ModuleError> {
        let at = r.position();
        let tag = r.leb()?;
        usize::try_from(tag).ok().and_then(|i| Condition::ALL.get(i).copied()).ok_or_else(|| r.malformed(at, format!("Condition has no tag {tag}")))
    }
}

codec_enum!(Datum { Place(place) = 0, Value(value) = 1, Text(text) = 2, Bare = 3 });
codec_struct!(CicsCommand { name, command, resp });
codec_struct!(Resp { resp, resp2, nohandle });
// Tags 1 and 9 are retired; HandleAbend, whose PROGRAM is a datum, is 34, and Return, with CHANNEL
// and IMMEDIATE, is 35.
codec_enum!(Cics {
    File { verb, file, options } = 0,
    Link(transfer) = 2,
    Xctl(transfer) = 3,
    Abend { abcode, cancel } = 4,
    HandleCondition(handlers) = 5,
    IgnoreCondition(conditions) = 6,
    PushHandle = 7,
    PopHandle = 8,
    HandleAid = 10,
    SendMap { map, mapset, from, maponly, dataonly, cursor, control } = 11,
    ReceiveMap { map, mapset, into, set } = 12,
    SendControl { cursor, control } = 13,
    Receive(record) = 14,
    Asktime { abstime } = 15,
    Formattime { abstime, datesep, timesep, outputs } = 16,
    Assign(assign) = 17,
    Getmain { flength, length, initimg, set } = 18,
    Freemain = 19,
    Enq = 20,
    Deq = 21,
    Delay = 22,
    Syncpoint { rollback } = 23,
    Address { eib, commarea, cwa, twa } = 24,
    SendText { from, length } = 25,
    WriteOperator { text, textlength } = 26,
    WriteqTs { queue, from, length, rewrite, item, numitems } = 27,
    ReadqTs { queue, next, item, numitems, record } = 28,
    DeleteqTs { queue } = 29,
    WriteqTd { queue, from, length } = 30,
    ReadqTd { queue, record } = 31,
    DeleteqTd { queue } = 32,
    Unsupported = 33,
    HandleAbend { program, label, reset } = 34,
    Return { transid, commarea, length, channel, immediate } = 35,
});
codec_enum!(FileControl {
    Read = 0,
    Write = 1,
    Rewrite = 2,
    Delete = 3,
    Unlock = 4,
    Startbr = 5,
    Resetbr = 6,
    Readnext = 7,
    Readprev = 8,
    Endbr = 9,
});
codec_struct!(FileOptions { ridfld, keylength, reqid, from, numrec, record, generic, rrn, gteq, equal, update });
codec_struct!(Record { into, set, length });
codec_struct!(Transfer { program, commarea, length });
codec_struct!(Control { erase, freekb, alarm, frset });
codec_struct!(Assign { applid, sysid, userid, netname, facility, startcode, abcode, program, cwaleng, twaleng });
