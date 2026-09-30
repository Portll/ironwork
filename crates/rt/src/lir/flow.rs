//! Control flow (lir.md §8): ops, terminators, PERFORM ranges and the VM's frames.

use super::{
    AbendId, ArithId, BlockId, CallId, CicsId, CondId, DebugId, DisplayId, ExprId, FileOpId, InitId, InspectId, IntExpr,
    InvokeId, MovePlan, Operand, ParaId, PlaceId, RangeId, ReleaseId, ReportOp, ReturnId, SearchAllId, SortId, SqlId,
    StepPlan, StringId, TempId, UnstringId,
};
use crate::abend::Ending;
use crate::vocab::AcceptFrom;
use crate::{codec_enum, codec_struct};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    Move { from: Operand, to: PlaceId, plan: MovePlan },
    Initialize { target: PlaceId, plan: InitId },
    Arith(ArithId),
    SetAddress { record: u16, address: Operand },
    SetUpDown { target: PlaceId, by: IntExpr, down: bool, plan: StepPlan },
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
    Display(DisplayId),
    Accept { target: PlaceId, from: AcceptFrom, plan: MovePlan },
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
    /// A PERFORM range completed: the register goes back to the PERFORM's own segment, clearing
    /// nothing.
    SetSegment(u8),
}

/// What an op tells the VM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Next,
    /// The handler a service selected, for the block's `Select`.
    Arm(u8),
    /// A transfer a service chose at run time: HANDLE CONDITION, HANDLE ABEND.
    GoTo(ParaId),
    End(Ending),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Terminator {
    Jump(BlockId),
    Branch { cond: CondId, then: BlockId, otherwise: BlockId },
    /// On the `Arm` the block's last op returned.
    Select(Vec<BlockId>),
    /// Leaves a paragraph for `next`, which may be one past the last; a PERFORM range may complete here.
    ParagraphEnd { next: ParaId },
    GoTo(ParaId),
    /// GO TO … DEPENDING ON: `targets[k − 1]` for k in range, taken as a `GoTo`; else `otherwise`.
    Switch { value: IntExpr, targets: Vec<ParaId>, otherwise: BlockId },
    /// An out-of-line PERFORM: push a frame and enter the range; `ret` runs when it completes.
    PerformEnter { range: RangeId, ret: BlockId },
    /// Nothing in the run unit's first program, GOBACK in any other.
    ExitProgram { next: BlockId },
    End(Ending),
    Abend(AbendId),
    /// The entry of a paragraph an ALTER names: a `GoTo` of the target the alter table holds for
    /// `para`, or `Jump(otherwise)` while it holds none.
    AlteredGoTo { para: ParaId, otherwise: BlockId },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    pub first: ParaId,
    pub last: ParaId,
    pub kind: RangeKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RangeKind {
    Perform,
    SortProcedure,
    UseBeforeReporting,
}

/// An active range: `ret` runs when it completes, and its paragraphs run at `depth`. `temps` are the
/// TIMES counters of the statements that run under it, by `TempId`: a paragraph can PERFORM itself,
/// and each activation counts its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub first: ParaId,
    pub last: ParaId,
    pub kind: FrameKind,
    pub ret: BlockId,
    pub depth: u32,
    pub temps: Vec<i64>,
}

/// Main is the program's own run and holds every paragraph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    Main,
    Perform,
    SortProcedure,
    UseBeforeReporting { at: DebugId },
}

codec_enum!(Op {
    Move { from, to, plan } = 0,
    Initialize { target, plan } = 1,
    Arith(id) = 2,
    SetAddress { record, address } = 3,
    SetUpDown { target, by, down, plan } = 4,
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
    SetSegment(priority) = 29,
});
codec_enum!(Step { Next = 0, Arm(arm) = 1, GoTo(para) = 2, End(ending) = 3 });
codec_enum!(Terminator {
    Jump(block) = 0,
    Branch { cond, then, otherwise } = 1,
    Select(arms) = 2,
    ParagraphEnd { next } = 3,
    GoTo(para) = 4,
    Switch { value, targets, otherwise } = 5,
    PerformEnter { range, ret } = 6,
    ExitProgram { next } = 7,
    End(ending) = 8,
    Abend(abend) = 9,
    AlteredGoTo { para, otherwise } = 10,
});
codec_struct!(Range { first, last, kind });
codec_enum!(RangeKind { Perform = 0, SortProcedure = 1, UseBeforeReporting = 2 });
codec_struct!(Frame { first, last, kind, ret, depth, temps });
codec_enum!(FrameKind { Main = 0, Perform = 1, SortProcedure = 2, UseBeforeReporting { at } = 3 });
