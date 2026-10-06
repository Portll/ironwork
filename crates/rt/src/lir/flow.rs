//! Control flow (lir.md §8): ops, terminators, ranges, and the VM's frames and return points
//! (assumption C99 PERFORM_RETURN_POINTS).

use super::{
    AbendId, ArithId, BlockId, CallId, CicsId, CondId, DisplayId, ExprId, FileOpId, InitId, InspectId, IntExpr,
    InvokeId, MarkupId, MovePlan, Odo, Operand, ParaId, PlaceId, RangeId, ReleaseId, ReportOp, ReturnId, ScreenInput, ScreenPlan, SearchAllId, SenderCheck, SortId, SqlId,
    StepPlan, StringId, SymId, TempId, UnstringId, UpDown,
};
use crate::abend::Ending;
use crate::vocab::AcceptFrom;
use crate::{codec_enum, codec_struct};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    /// `to` located, then `from`, which takes NUMCHECK's `check` before it is read.
    Move { from: Operand, to: PlaceId, plan: MovePlan, check: SenderCheck },
    /// SET TO, and PERFORM VARYING's FROM: as `Move`, but a data item sender is read as a number,
    /// its digits checked, where MOVE carries a zoned or packed sender's invalid digits (C260).
    Set { from: Operand, to: PlaceId, plan: MovePlan },
    Initialize { target: PlaceId, plan: InitId },
    Arith(ArithId),
    /// SET ADDRESS OF: `address` evaluated once, NULL or an address in run-unit memory, then
    /// each LINKAGE record given it in turn.
    SetAddress { records: Vec<u16>, address: Operand },
    /// SET UP BY or DOWN BY: `by` evaluated once, then each receiver read and moved in turn.
    SetUpDown { by: IntExpr, down: bool, targets: Vec<(PlaceId, UpDown)> },
    /// SET TO ENTRY: `entry` read as a program name and the program loaded, dynamically unless it
    /// is a literal under NODYNAM, then each function-pointer or procedure-pointer given the value
    /// naming it in the run unit's list of entries.
    SetEntry { entry: Operand, targets: Vec<PlaceId> },
    /// PERFORM VARYING's increment: `var` located, then each place of `prepass`, then `var + by`
    /// computed and stored.
    Step { var: PlaceId, by: ExprId, plan: StepPlan, prepass: Vec<PlaceId> },
    /// SEARCH's index steps, SORT-RETURN.
    SetInt { target: PlaceId, value: IntExpr },
    Inspect(InspectId),
    String(StringId),
    Unstring(UnstringId),
    SearchAll(SearchAllId),
    /// Raises the PERFORM and CALL depth, abending past 100.
    Nest,
    Unnest(u8),
    SetTemp(TempId, IntExpr),
    DecTemp(TempId),
    /// SEARCH's table count, evaluated once at the statement's start as `occurrences` evaluates it,
    /// abending as it abends, and held in the top frame's counter `temp`, which `Count::Temp` reads.
    SetCount(TempId, Odo),
    Display(DisplayId),
    Accept { target: PlaceId, from: AcceptFrom, plan: MovePlan },
    /// DISPLAY ... UPON ARGUMENT-NUMBER under `--compliance extended`: the next ACCEPT ... FROM
    /// ARGUMENT-VALUE takes the PARM argument word this numbers.
    ArgumentNumber(IntExpr),
    /// DISPLAY UPON ENVIRONMENT-NAME, or with `value` UPON ENVIRONMENT-VALUE, under `--compliance
    /// extended`: plan `display`'s text names the environment variable, or becomes its value.
    Environment { display: DisplayId, value: bool },
    /// DISPLAY on the screen under `--compliance extended`: plan `display`'s text written where
    /// `screen` puts it.
    ScreenDisplay { display: DisplayId, screen: ScreenPlan },
    /// ACCEPT from fields of the screen under `--compliance extended`, one for a positioned ACCEPT
    /// and one for each TO or USING field of a SCREEN SECTION's screen; with ON EXCEPTION
    /// phrases, `handled`, Arm(1) when a key other than ENTER ended it.
    ScreenAccept { inputs: Vec<ScreenInput>, handled: bool },
    File(FileOpId),
    Call(CallId),
    Cancel(Operand),
    Sort(SortId),
    Release(ReleaseId),
    Return(ReturnId),
    Report(ReportOp),
    Invoke(InvokeId),
    Cics(CicsId),
    Sql(SqlId),
    /// ALTER: from now on control reaching paragraph `para` goes to `to`. The alter table lives as
    /// long as the program's WORKING-STORAGE.
    Alter { para: ParaId, to: ParaId },
    /// Control reaching a paragraph of segment `priority`: when that differs from the segment
    /// register and is 50 or more, the alter entries of the segment's paragraphs are cleared. The
    /// register then holds `priority`.
    EnterSegment(u8),
    /// Under the DEBUG option: the line of the statement starting, which DEBUG-LINE gives.
    DebugLine(u32),
    /// Under the DEBUG option, after an ALTER of a paragraph a debugging section serves: that
    /// section, with DEBUG-NAME `name` and DEBUG-CONTENTS `contents`, the TO PROCEED TO name.
    DebugAlter { range: RangeId, name: SymId, contents: SymId },
    /// JSON GENERATE, JSON PARSE, XML GENERATE or XML PARSE (lir.md §9.13).
    Markup(MarkupId),
}

/// What an op tells the VM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Next,
    /// The handler a service selected, for the block's `Select`.
    Arm(u8),
    /// A transfer a service chose at run time (HANDLE CONDITION), or a procedure it
    /// ran left by.
    GoTo(ParaId),
    End(Ending),
    /// A procedure the op ran passed the return point of active frame `frame`, which completes.
    Return(u64),
    /// A procedure the op ran passed the return point of a PERFORM control had left.
    Resume(Resume),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Terminator {
    Jump(BlockId),
    Branch { cond: CondId, then: BlockId, otherwise: BlockId },
    /// On the `Arm` the block's last op returned.
    Select(Vec<BlockId>),
    /// Control passes the end of paragraph `next` − 1 for `next`, which may be one past the last:
    /// the return point armed there, if any, is taken.
    ParagraphEnd { next: ParaId },
    GoTo(ParaId),
    /// GO TO … DEPENDING ON: `targets[k − 1]` for k in range, taken as a `GoTo`; else `otherwise`.
    Switch { value: IntExpr, targets: Vec<ParaId>, otherwise: BlockId },
    /// An out-of-line PERFORM: push a frame, arm its return point and enter the range; `ret` runs
    /// when it completes.
    PerformEnter { range: RangeId, ret: BlockId, resume: Option<Resume> },
    /// Nothing in the run unit's first program, GOBACK in any other.
    ExitProgram { next: BlockId },
    End(Ending),
    Abend(AbendId),
    /// The entry of a paragraph an ALTER names: a `GoTo` of the target the alter table holds for
    /// `para`, or `Jump(otherwise)` while it holds none.
    AlteredGoTo { para: ParaId, otherwise: BlockId },
    /// Under the DEBUG option, the entry of a paragraph a debugging section serves: the section
    /// runs with DEBUG-NAME `name`, then `next`; a section that leaves takes the top frame with it.
    Debug { range: RangeId, name: SymId, next: BlockId },
}

/// The statement after an out-of-line PERFORM that runs once and is a statement of paragraph
/// `para`: `block`, where control resumes when it passes that PERFORM's return point after
/// leaving its range. `block` also keys what the PERFORM displaced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resume {
    pub para: ParaId,
    pub block: BlockId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    pub first: ParaId,
    pub last: ParaId,
    pub kind: RangeKind,
}

impl Range {
    /// The paragraphs a GO TO stays in the range for, of a program of `paragraphs`: `first` to
    /// `last`, or to the program's last paragraph when `last` comes before `first`; every one for
    /// a SORT or MERGE procedure.
    pub fn region(&self, paragraphs: u32) -> (ParaId, ParaId) {
        let end = paragraphs.saturating_sub(1);
        match self.kind {
            RangeKind::SortProcedure => (0, end),
            _ if self.last < self.first => (self.first, end),
            _ => (self.first, self.last),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RangeKind {
    Perform,
    SortProcedure,
    UseBeforeReporting,
    /// A USE AFTER EXCEPTION/ERROR procedure.
    UseProcedure,
    /// A USE FOR DEBUGGING section.
    Debugging,
    /// An XML PARSE processing procedure.
    Processing,
}

/// What the declaratives need at run time besides each file's own procedure (`FileDesc.error`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Declaratives {
    /// The EXCEPTION/ERROR procedures for files open, or being opened, INPUT, OUTPUT, I-O and
    /// EXTEND, in that order.
    pub modes: [Option<RangeId>; 4],
    /// DEBUG-ITEM's offset and length in the slab, when a debugging section can run.
    pub debug_item: Option<(u32, u32)>,
}

/// A return point armed at the end of a range's last paragraph by frame `frame`, with the PERFORM's
/// resume.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReturnPoint {
    pub frame: u64,
    pub resume: Option<Resume>,
}

/// An active range. `displaced` is the return point it replaced at its range's end, put back when
/// it completes; `segment` the segment register when it was pushed, back when it completes. Its
/// paragraphs run at `depth`, and `temps` are the TIMES counters of the statements that run under
/// it, by `TempId`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub id: u64,
    pub kind: FrameKind,
    pub displaced: Option<ReturnPoint>,
    pub segment: u8,
    pub depth: u32,
    pub temps: Vec<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    /// The program's own run: every paragraph, and no return point.
    Main,
    Perform { range: RangeId, ret: BlockId, resume: Option<Resume> },
    /// A procedure a service or a `Debug` runs in a dispatch loop of its own, which ends when
    /// the frame completes or is left.
    Procedure { range: RangeId },
}

/// The return points of one activation: the one armed at each paragraph's end, what each
/// resumable PERFORM control left displaced (by its `Resume.block`), and the frames, Main first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Returns {
    pub armed: Vec<Option<ReturnPoint>>,
    pub saved: BTreeMap<BlockId, Option<ReturnPoint>>,
    pub frames: Vec<Frame>,
    pub next_frame: u64,
}

codec_enum!(Op {
    Move { from, to, plan, check } = 0,
    Initialize { target, plan } = 1,
    Arith(id) = 2,
    SetAddress { records, address } = 3,
    SetUpDown { by, down, targets } = 4,
    Step { var, by, plan, prepass } = 5,
    SetInt { target, value } = 6,
    Inspect(id) = 7,
    String(id) = 8,
    Unstring(id) = 9,
    SearchAll(id) = 10,
    Nest = 11,
    Unnest(n) = 12,
    SetTemp(temp, value) = 13,
    DecTemp(temp) = 14,
    Display(id) = 15,
    Accept { target, from, plan } = 16,
    File(id) = 17,
    Call(id) = 18,
    Cancel(name) = 19,
    Sort(id) = 20,
    Release(id) = 21,
    Return(id) = 22,
    Report(op) = 23,
    Invoke(id) = 24,
    Cics(id) = 25,
    Sql(id) = 26,
    Alter { para, to } = 27,
    EnterSegment(priority) = 28,
    DebugLine(line) = 30,
    DebugAlter { range, name, contents } = 31,
    Markup(id) = 32,
    Set { from, to, plan } = 33,
    SetCount(temp, odo) = 34,
    SetEntry { entry, targets } = 35,
    ArgumentNumber(value) = 36,
    ScreenDisplay { display, screen } = 37,
    ScreenAccept { inputs, handled } = 38,
    Environment { display, value } = 39,
});
codec_enum!(Step { Next = 0, Arm(arm) = 1, GoTo(para) = 2, End(ending) = 3, Return(frame) = 4, Resume(resume) = 5 });
codec_enum!(Terminator {
    Jump(block) = 0,
    Branch { cond, then, otherwise } = 1,
    Select(arms) = 2,
    ParagraphEnd { next } = 3,
    GoTo(para) = 4,
    Switch { value, targets, otherwise } = 5,
    ExitProgram { next } = 7,
    End(ending) = 8,
    Abend(abend) = 9,
    AlteredGoTo { para, otherwise } = 10,
    Debug { range, name, next } = 11,
    PerformEnter { range, ret, resume } = 12,
});
codec_struct!(Resume { para, block });
codec_struct!(Range { first, last, kind });
codec_enum!(RangeKind { Perform = 0, SortProcedure = 1, UseBeforeReporting = 2, UseProcedure = 3, Debugging = 4, Processing = 5 });
codec_struct!(Declaratives { modes, debug_item });
codec_struct!(ReturnPoint { frame, resume });
codec_struct!(Frame { id, kind, displaced, segment, depth, temps });
codec_enum!(FrameKind { Main = 0, Perform { range, ret, resume } = 1, Procedure { range } = 2 });
codec_struct!(Returns { armed, saved, frames, next_frame });
