//! Declaratives at run time: a file's EXCEPTION/ERROR procedure after its statement fails, and a
//! debugging section before each procedure it serves, with DEBUG-ITEM saying how control came
//! there (Language Reference SC27-8713-03, pp. 19-20, 714-716 and 771-772).

use super::*;
use crate::declaratives::{DEBUG_CONTENTS, DEBUG_LINE, DEBUG_NAME, Span, mode_index};
use syntax::report::ReportStmt;

/// How control comes to a procedure, as DEBUG-CONTENTS reports it (p. 20).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Arrival {
    #[default]
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

/// What the declaratives keep while a program runs.
#[derive(Default)]
pub(super) struct State {
    /// How control comes to the procedure `run_paragraphs` starts at next.
    pub(super) arrival: Arrival,
    /// Where the statement last started is, for DEBUG-LINE.
    pub(super) line: Pos,
    /// A debugging section is running, and its statements start no other.
    pub(super) debugging: bool,
    /// How an EXCEPTION/ERROR procedure left the statement it ran for: GO TO, STOP RUN or GOBACK.
    pub(super) leaving: Option<Flow>,
    /// The file whose statement failed last.
    pub(super) failed: Option<usize>,
}

/// Where a statement is, for DEBUG-LINE; None for those that carry no position.
pub(super) fn statement_pos(s: &Stmt) -> Option<Pos> {
    Some(match s {
        Stmt::Move { pos, .. }
        | Stmt::Compute { pos, .. }
        | Stmt::If { pos, .. }
        | Stmt::PerformInline { pos, .. }
        | Stmt::PerformProc { pos, .. }
        | Stmt::Evaluate { pos, .. }
        | Stmt::Display { pos, .. }
        | Stmt::Open { pos, .. }
        | Stmt::Close { pos, .. }
        | Stmt::Write { pos, .. }
        | Stmt::Rewrite { pos, .. }
        | Stmt::Delete { pos, .. }
        | Stmt::Start { pos, .. }
        | Stmt::Initialize { pos, .. }
        | Stmt::GoTo { pos, .. }
        | Stmt::GoToDepending { pos, .. }
        | Stmt::Alter { pos, .. }
        | Stmt::Entry { pos, .. }
        | Stmt::Goback { pos }
        | Stmt::ExitProgram { pos }
        | Stmt::Cancel { pos, .. }
        | Stmt::Set { pos, .. }
        | Stmt::Accept { pos, .. }
        | Stmt::ExitMethod { pos }
        | Stmt::StopRun { pos } => *pos,
        Stmt::Arith(a) => a.pos,
        Stmt::Read(r) => r.pos,
        Stmt::Call(c) => c.pos,
        Stmt::String(st) => st.pos,
        Stmt::Unstring(u) => u.pos,
        Stmt::Inspect(i) => i.pos,
        Stmt::Search(se) => se.pos,
        Stmt::Exec(block) => block.pos,
        Stmt::Invoke(i) => i.pos,
        Stmt::Report(r) => match &**r {
            ReportStmt::Initiate { pos, .. } | ReportStmt::Generate { pos, .. } | ReportStmt::Terminate { pos, .. } | ReportStmt::Suppress { pos } => *pos,
        },
        Stmt::Sorting(so) => match &**so {
            Sorting::Sort(st) => st.pos,
            Sorting::Release { pos, .. } | Sorting::Return { pos, .. } => *pos,
        },
        Stmt::NextSentence | Stmt::SentenceEnd | Stmt::Continue | Stmt::Exit(_) => return None,
    })
}

impl<'p> Machine<'p, '_, '_> {
    /// File k's EXCEPTION/ERROR procedure: its own, else the one for the mode it is open in or
    /// being opened in (p. 714; [`numeric::assumptions::ERROR_DECLARATIVE_MODE`]).
    pub(super) fn error_declarative(&self, k: usize, mode: Option<OpenMode>) -> Option<Span> {
        let table = self.declaratives;
        table.files.get(k).copied().flatten().or_else(|| mode.and_then(|m| table.modes[mode_index(m)]))
    }

    /// Runs an EXCEPTION/ERROR procedure for the statement at `pos`. Control comes back to the
    /// statement, unless the procedure leaves by GO TO, STOP RUN or GOBACK, which the statement's
    /// `exec` then carries out.
    pub(super) fn run_error_declarative(&mut self, (first, last): Span, pos: Pos) -> R<()> {
        self.nest(pos)?;
        self.uses.arrival = Arrival::Use;
        let flow = self.run_paragraphs(first, last);
        self.unit.depth -= 1;
        match flow? {
            leaving @ (Flow::GoTo(_) | Flow::End(_)) => {
                self.uses.leaving = Some(leaving);
                Err(Abend { code: AbendCode::Signal(Signal::DeclarativeExit), message: String::new(), pos })
            }
            _ => Ok(()),
        }
    }

    /// Where an EXCEPTION/ERROR procedure sent control when it left its statement.
    pub(super) fn declarative_exit(&mut self) -> Flow {
        self.uses.leaving.take().unwrap_or(Flow::Next)
    }

    /// Under the DEBUG runtime option, runs the debugging section for paragraph i, if one serves
    /// it, with DEBUG-ITEM saying how control came to i (DEBUG_LINE_NUMBER, DEBUG_LINE_STATEMENT and
    /// DEBUG_NAME_FORM in numeric::assumptions). A flow when the section leaves by GO TO, STOP RUN
    /// or GOBACK.
    pub(super) fn debug_before(&mut self, i: usize, arrival: Arrival) -> R<Option<Flow>> {
        let table = self.declaratives;
        let Some(Some(((first, last), name))) = table.triggers.get(i) else { return Ok(None) };
        if self.uses.debugging {
            return Ok(None);
        }
        let pos = self.program.paragraphs[i].pos;
        let line = if arrival == Arrival::Start { pos } else { self.uses.line };
        if let Some(item) = table.debug_item {
            let (at, size) = (self.base + self.layout.items[item].offset as usize, self.layout.items[item].size as usize);
            self.unit.mem[at..at + size].fill(ebcdic::SPACE);
            for ((offset, len), text) in [(DEBUG_LINE, format!("{:06}", line.line)), (DEBUG_NAME, name.clone()), (DEBUG_CONTENTS, arrival.contents().to_owned())] {
                let bytes = self.page.encode(&text).map_err(|e| Abend::ironwork(e.to_string(), pos))?;
                let n = bytes.len().min(len);
                self.unit.mem[at + offset..at + offset + n].copy_from_slice(&bytes[..n]);
            }
        }
        let saved = self.uses.line;
        self.uses.debugging = true;
        let flow = self.nest(pos).and_then(|()| {
            let flow = self.run_paragraphs(*first, *last);
            self.unit.depth -= 1;
            flow
        });
        self.uses.debugging = false;
        self.uses.line = saved;
        Ok(match flow? {
            leaving @ (Flow::GoTo(_) | Flow::End(_)) => Some(leaving),
            _ => None,
        })
    }
}
