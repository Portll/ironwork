//! Control flow (lir.md §8.4, §9.10): the dispatch loop over blocks, the frames and return points
//! of assumption C99, transfers, and a procedure a statement or a `Debug` runs in a loop of its own.

use super::{Code, Halt, R, Vm, not_yet};
use crate::abend::{Abend, AbendCode, Ending, Signal};
use crate::lir::{BlockId, DebugId, Frame, FrameKind, ParaId, RangeId, ReturnPoint, Step, Terminator};
use crate::unit::{Event, Loader};
use crate::vocab::Pos;
use std::rc::Rc;
use zarch::ebcdic;

/// How control came to a paragraph's entry, which DEBUG-CONTENTS shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Arrival {
    Perform,
    Start,
    GoTo,
    FallThrough,
    Use,
    /// An input or output procedure: SORT INPUT, SORT OUTPUT or MERGE OUTPUT.
    Sort(&'static str),
}

impl Arrival {
    fn contents(self) -> &'static str {
        match self {
            Self::Perform => "PERFORM LOOP",
            Self::Start => "START PROGRAM",
            Self::GoTo => "",
            Self::FallThrough => "FALL THROUGH",
            Self::Use => "USE PROCEDURE",
            Self::Sort(procedure) => procedure,
        }
    }
}

/// How a dispatch loop ends: the frame it runs under completed, or was left by a transfer, or the
/// run ended.
pub(super) enum Exit {
    Completed,
    Left(Step),
    End(Ending),
}

enum Next {
    Block(BlockId),
    Exit(Exit),
}

/// STOP RUN in a user-defined function an op or a condition ran: the run ends there, as the
/// walker's `exec` ends it at the statement holding the invocation.
fn stops_run(halt: &Halt) -> bool {
    matches!(halt, Halt::Abend(Abend { code: AbendCode::Signal(Signal::StopRun), .. }))
}

/// DEBUG-ITEM's DEBUG-LINE, DEBUG-NAME and DEBUG-CONTENTS: offset and length.
const DEBUG_FIELDS: [(usize, usize); 3] = [(0, 6), (7, 30), (56, 30)];

impl<L: Loader<Rc<Code>>> Vm<'_, '_, '_, L> {
    /// Runs the procedure from its start, or from an ENTRY's paragraph and block.
    pub(super) fn run_from(&mut self, at: Option<(ParaId, BlockId)>) -> R<Ending> {
        let p = self.p;
        let (start, block) = match at {
            Some(at) => at,
            None => match p.paragraphs.get(p.procedure_start as usize) {
                Some(para) => (p.procedure_start, para.entry),
                None => return Ok(Ending::EndOfProgram),
            },
        };
        let Some(paragraph) = p.paragraphs.get(start as usize) else { return Ok(Ending::EndOfProgram) };
        self.segment = paragraph.priority;
        self.arrival = Arrival::Start;
        if at.is_some() && paragraph.is_section {
            self.line = self.pos(paragraph.at).line;
        }
        match self.dispatch(block, 0)? {
            Exit::End(e) => Ok(e),
            Exit::Completed | Exit::Left(_) => Ok(Ending::EndOfProgram),
        }
    }

    /// `Machine::go_to`: after an abend, the procedure from paragraph `p` as a GO TO at `from`
    /// reaches it. The frames the abend passed through are left as a GO TO leaves them, their points
    /// still armed, and the depth is the activation's (C236).
    pub(super) fn go_to(&mut self, p: ParaId, from: Pos) -> R<Ending> {
        while self.returns.frames.len() > 1 {
            self.leave();
        }
        if let Some(main) = self.returns.frames.first_mut() {
            main.temps.clear();
            self.unit.depth = main.depth as usize;
        }
        self.line = from.line;
        self.segment = self.p.paragraphs[p as usize].priority;
        self.arrival = Arrival::GoTo;
        match self.dispatch(self.entry(p), 0)? {
            Exit::End(e) => Ok(e),
            Exit::Completed | Exit::Left(_) => Ok(Ending::EndOfProgram),
        }
    }

    /// Runs blocks from `block` until the frame at `floor`, which this loop runs under, completes
    /// or is left, or the run ends. Control reaching a paragraph's entry tells the observer.
    fn dispatch(&mut self, mut block: BlockId, floor: usize) -> R<Exit> {
        let p = self.p;
        loop {
            if let Some(i) = self.code.entry_of[block as usize] {
                let paragraph = &p.paragraphs[i as usize];
                self.unit.notify(Event::Paragraph { program: self.sym(p.id), name: self.sym(paragraph.name), index: i as usize });
                self.paragraph_reached();
            }
            let b = &p.blocks[block as usize];
            let at = &p.debug.ops[block as usize];
            let starts = &p.debug.statements[block as usize];
            let tracing = (self.unit.statements.is_some() || self.unit.taint.is_some() || self.unit.statement_limit.is_some()) && !starts.is_empty();
            let mut told = 0;
            let mut arm = None;
            let mut transfer = None;
            for (k, (op, &id)) in b.ops.iter().zip(at).enumerate() {
                if tracing {
                    told = self.statements_before(starts, told, k)?;
                }
                match self.op(op, id) {
                    Ok(Step::Next) => {}
                    Ok(Step::Arm(a)) => arm = Some(a),
                    Ok(step) => {
                        transfer = Some(step);
                        break;
                    }
                    Err(halt) if stops_run(&halt) => {
                        transfer = Some(Step::End(Ending::StopRun));
                        break;
                    }
                    Err(halt) => return Err(halt),
                }
            }
            if tracing && transfer.is_none() {
                self.statements_before(starts, told, b.ops.len())?;
            }
            let next = match transfer {
                Some(step) => self.transfer(step, floor)?,
                None => match self.terminator(&b.end, at[b.ops.len()], arm, floor) {
                    Err(halt) if stops_run(&halt) => Next::Exit(Exit::End(Ending::StopRun)),
                    next => next?,
                },
            };
            match next {
                Next::Block(b) => block = b,
                Next::Exit(exit) => return Ok(exit),
            }
        }
    }

    /// Starts each statement of `starts`, from the `told`th, that starts before op `k`, as the
    /// walker's `exec` does: counted against the statement limit, taint's statement starts, and the
    /// observer told; how many of `starts` are started after.
    fn statements_before(&mut self, starts: &[(u32, DebugId)], mut told: usize, k: usize) -> R<usize> {
        while let Some(&(op, id)) = starts.get(told)
            && op as usize <= k
        {
            let pos = self.pos(id);
            self.unit.start_statement(pos)?;
            self.unit.statement_starts();
            if self.unit.traces(pos.line) {
                let file = self.event_file(pos);
                self.unit.notify(Event::Statement { file: &file, line: pos.line });
            }
            told += 1;
        }
        Ok(told)
    }

    fn terminator(&mut self, end: &Terminator, at: DebugId, arm: Option<u8>, floor: usize) -> R<Next> {
        let pos = self.pos(at);
        Ok(match end {
            Terminator::Jump(b) => Next::Block(*b),
            Terminator::Branch { cond, then, otherwise } => Next::Block(if self.cond(*cond, pos)? { *then } else { *otherwise }),
            Terminator::Select(arms) => match arm.and_then(|a| arms.get(usize::from(a))) {
                Some(&b) => Next::Block(b),
                None => return Err(not_yet("a Select with no arm for its op")),
            },
            Terminator::ParagraphEnd { next } => self.paragraph_end(*next, floor)?,
            Terminator::GoTo(t) => self.transfer(Step::GoTo(*t), floor)?,
            Terminator::Switch { value, targets, otherwise } => {
                let n = self.int(value, pos)?;
                match usize::try_from(n).ok().and_then(|n| targets.get(n.wrapping_sub(1))) {
                    Some(&t) => self.transfer(Step::GoTo(t), floor)?,
                    None => Next::Block(*otherwise),
                }
            }
            Terminator::PerformEnter { range, ret, resume } => {
                let r = self.p.ranges[*range as usize];
                let resume = self.resumable(*resume);
                self.push(FrameKind::Perform { range: *range, ret: *ret, resume }, r.last, resume);
                self.arrival = Arrival::Perform;
                Next::Block(self.entry(r.first))
            }
            Terminator::ExitProgram { next } if self.main => Next::Block(*next),
            Terminator::ExitProgram { .. } => Next::Exit(Exit::End(Ending::Goback)),
            Terminator::End(e) => Next::Exit(Exit::End(*e)),
            Terminator::Abend(id) => return Err(self.abend(*id, Some(at)).into()),
            Terminator::AlteredGoTo { para, otherwise } => match self.unit.programs[self.me].altered.get(*para as usize).copied().flatten() {
                Some(t) => self.transfer(Step::GoTo(t as ParaId), floor)?,
                None => Next::Block(*otherwise),
            },
            Terminator::Debug { range, name, next } => {
                if self.debugging {
                    return Ok(Next::Block(*next));
                }
                let line = if self.arrival == Arrival::Start { pos.line } else { self.line };
                match self.run_debugging(*range, self.sym(*name), line, self.arrival.contents(), pos)? {
                    None => Next::Block(*next),
                    Some(step) => self.leave_with(step, floor)?,
                }
            }
        })
    }

    fn entry(&self, paragraph: ParaId) -> BlockId {
        self.p.paragraphs[paragraph as usize].entry
    }

    /// A new frame arming the end of paragraph `last` to return to it.
    fn push(&mut self, kind: FrameKind, last: ParaId, resume: Option<crate::lir::Resume>) {
        let id = self.returns.next_frame;
        self.returns.next_frame += 1;
        let displaced = self.returns.armed[last as usize].replace(ReturnPoint { frame: id, resume });
        self.returns.frames.push(Frame { id, kind, displaced, segment: self.segment, depth: self.unit.depth as u32, temps: Vec::new() });
    }

    fn holds(&self, frame: &Frame, paragraph: ParaId) -> bool {
        let range = match frame.kind {
            FrameKind::Main => return true,
            FrameKind::Perform { range, .. } | FrameKind::Procedure { range } => range,
        };
        let (lo, hi) = self.p.ranges[range as usize].region(self.p.paragraphs.len() as u32);
        (lo..=hi).contains(&paragraph)
    }

    fn top(&self) -> &Frame {
        self.returns.frames.last().expect("an activation's Main frame")
    }

    /// Leaves the top frame: its point stays armed, and a PERFORM that can resume keeps what it
    /// displaced for that resume.
    fn leave(&mut self) {
        if let Some(frame) = self.returns.frames.pop()
            && let FrameKind::Perform { resume: Some(r), .. } = frame.kind
        {
            self.returns.saved.insert(r.block, frame.displaced);
        }
    }

    /// Completes the top frame: what it displaced is armed again, and the segment register and
    /// depth are as they were at its start.
    fn complete(&mut self) -> R<Next> {
        let Some(frame) = self.returns.frames.pop() else { return Err(not_yet("a frame completed with none running")) };
        let range = match frame.kind {
            FrameKind::Main => return Err(not_yet("the Main frame completed")),
            FrameKind::Perform { range, .. } | FrameKind::Procedure { range } => range,
        };
        let last = self.p.ranges[range as usize].last;
        self.returns.armed[last as usize] = frame.displaced;
        self.segment = frame.segment;
        self.unit.depth = frame.depth as usize;
        Ok(match frame.kind {
            FrameKind::Perform { ret, .. } => Next::Block(ret),
            _ => Next::Exit(Exit::Completed),
        })
    }

    /// Takes a transfer from the top frame (lir.md §8.4), leaving each frame that does not hold its
    /// target; a loop whose own frame is left ends with the transfer.
    fn transfer(&mut self, step: Step, floor: usize) -> R<Next> {
        match step {
            Step::GoTo(t) | Step::Resume(crate::lir::Resume { para: t, .. }) => {
                while !self.holds(self.top(), t) {
                    if self.returns.frames.len() - 1 == floor {
                        self.leave();
                        return Ok(Next::Exit(Exit::Left(step)));
                    }
                    self.leave();
                }
                self.unit.depth = self.top().depth as usize;
                Ok(Next::Block(match step {
                    Step::Resume(r) => {
                        self.segment = self.p.paragraphs[r.para as usize].priority;
                        self.paragraph_reached();
                        r.block
                    }
                    _ => {
                        self.arrival = Arrival::GoTo;
                        self.entry(t)
                    }
                }))
            }
            Step::Return(f) => {
                let Some(i) = self.returns.frames.iter().rposition(|frame| frame.id == f) else { return Err(not_yet("a return to a frame not running")) };
                if i < floor {
                    while self.returns.frames.len() > floor {
                        self.leave();
                    }
                    return Ok(Next::Exit(Exit::Left(step)));
                }
                while self.returns.frames.len() - 1 > i {
                    self.leave();
                }
                self.complete()
            }
            Step::End(e) => Ok(Next::Exit(Exit::End(e))),
            Step::Next | Step::Arm(_) => Err(not_yet("a transfer that is not one")),
        }
    }

    /// Control passing the end of paragraph `next` − 1: the return point armed there decides.
    fn paragraph_end(&mut self, next: ParaId, floor: usize) -> R<Next> {
        let end = next as usize - 1;
        let (top, depth, holds) = (self.top().id, self.top().depth, self.holds(self.top(), next));
        match self.returns.armed[end] {
            None if next as usize == self.p.paragraphs.len() => Ok(Next::Exit(Exit::End(Ending::EndOfProgram))),
            None if holds => {
                self.unit.depth = depth as usize;
                self.arrival = Arrival::FallThrough;
                Ok(Next::Block(self.entry(next)))
            }
            None => self.transfer(Step::GoTo(next), floor),
            Some(r) if r.frame == top => self.complete(),
            Some(r) if self.returns.frames.iter().any(|f| f.id == r.frame) => self.transfer(Step::Return(r.frame), floor),
            Some(ReturnPoint { resume: Some(r), .. }) => {
                self.returns.armed[end] = self.returns.saved.get(&r.block).copied().flatten();
                self.transfer(Step::Resume(r), floor)
            }
            Some(_) => match self.p.paragraphs[end].abandoned {
                Some(abend) => Err(self.abend(abend, None).into()),
                None => Err(not_yet("an abandoned return point with no abend")),
            },
        }
    }

    /// A procedure's leaving taken by the frame running it, as `run_region` breaks with it: a
    /// return to that frame completes it; any other transfer leaves it and carries on from the
    /// frame below, and from the Main frame ends the activation.
    fn leave_with(&mut self, step: Step, floor: usize) -> R<Next> {
        let top = self.returns.frames.len() - 1;
        if let Step::Return(f) = step
            && f == self.returns.frames[top].id
        {
            return self.complete();
        }
        if top == 0 {
            return Ok(Next::Exit(Exit::End(match step {
                Step::End(e) => e,
                _ => Ending::EndOfProgram,
            })));
        }
        self.leave();
        if top == floor {
            return Ok(Next::Exit(Exit::Left(step)));
        }
        self.transfer(step, floor)
    }

    /// Runs `range` as a procedure under a frame of its own, in a dispatch loop of its own. An
    /// abend that unwinds it leaves its frames as a GO TO does, their points armed, as the walker's
    /// `perform_range` does when one passes through it.
    pub(super) fn run_procedure(&mut self, range: RangeId, arrival: Arrival) -> R<Exit> {
        let r = self.p.ranges[range as usize];
        self.push(FrameKind::Procedure { range }, r.last, None);
        let floor = self.returns.frames.len() - 1;
        self.arrival = arrival;
        let exit = self.dispatch(self.entry(r.first), floor);
        if exit.is_err() {
            while self.returns.frames.len() > floor {
                self.leave();
            }
        }
        exit
    }

    /// A debugging section, unless one is running: DEBUG-ITEM filled with the line, name and
    /// contents, the depth raised, the section run. Some transfer when it leaves.
    pub(super) fn run_debugging(&mut self, range: RangeId, name: &str, line: u32, contents: &str, pos: Pos) -> R<Option<Step>> {
        if let Some((offset, len)) = self.p.services.declaratives.debug_item {
            let at = self.base + offset as usize;
            self.unit.mem[at..at + len as usize].fill(ebcdic::SPACE);
            let page = self.p.options.options.code_page();
            for ((field, width), text) in DEBUG_FIELDS.into_iter().zip([&format!("{line:06}")[..], name, contents]) {
                let bytes = page.encode(text).map_err(|e| crate::abend::Abend::ironwork(e.to_string(), pos))?;
                let n = bytes.len().min(width);
                self.unit.mem[at + field..at + field + n].copy_from_slice(&bytes[..n]);
            }
            self.unit.mark(at, len as usize);
        }
        let saved = self.line;
        self.debugging = true;
        let ran = match self.unit.enter(pos) {
            Ok(()) => {
                let ran = self.run_procedure(range, Arrival::Perform);
                self.unit.depth = self.unit.depth.saturating_sub(1);
                ran
            }
            Err(abend) => Err(abend.into()),
        };
        self.debugging = false;
        self.line = saved;
        Ok(match ran? {
            Exit::Completed => None,
            Exit::Left(step) => Some(step),
            Exit::End(e) => Some(Step::End(e)),
        })
    }
}
