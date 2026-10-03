//! EXEC CICS (lir.md §9.5): each block bound by the walker's own `cics_bind::bind`, its handles
//! lowered, as one `Op::Cics`. HANDLE CONDITION keeps the paragraphs its labels name, which the op
//! returns as `Step::GoTo` when a condition takes one; HANDLE ABEND keeps its LABEL's. A block the
//! walker refuses as it binds it lowers to `Cics::Refused` with the walker's message.

use super::{Lower, LowerError, R, push, unsupported};
use crate::Abend;
use crate::machine::cics_bind;
use rt::abend::AbendCode;
use rt::cics::{Cics, CicsCommand, Handles, Resp};
use rt::lir::{self, Op, PlaceId, SymId};
use syntax::Pos;
use syntax::ast::{ExecBlock, Operand, Ref};

/// The walker's references as the LIR's ids: a data item as a place located as the walker locates
/// it, not as a receiving item; any other operand as its value; text as a symbol.
struct Lowering<'l, 'c> {
    l: &'l mut Lower<'c>,
    pos: Pos,
}

impl<'b> Handles<&'b Ref, &'b Operand, &'b str> for Lowering<'_, '_> {
    type Place = PlaceId;
    type Value = lir::Operand;
    type Text = SymId;
    type Error = LowerError;

    fn place(&mut self, r: &'b Ref) -> R<PlaceId> {
        self.l.place(r, false)
    }

    fn value(&mut self, op: &'b Operand) -> R<lir::Operand> {
        Ok(self.l.operand(op, self.pos)?.operand)
    }

    fn text(&mut self, text: &'b str) -> R<SymId> {
        Ok(self.l.sym(text))
    }
}

impl Lower<'_> {
    pub(super) fn cics(&mut self, block: &ExecBlock, pos: Pos, para: usize) -> R<()> {
        let program = self.program;
        let command = match cics_bind::bind(block, &|text| cics_bind::label(program, block, text, para)) {
            Ok(bound) => bound.map(&mut Lowering { l: self, pos })?,
            Err(abend) => self.refused_cics(block, abend, pos)?,
        };
        let id = push(&mut self.services.cics, command, "EXEC CICS commands")?;
        self.op(Op::Cics(id), pos)
    }

    /// A block whose binding the walker refuses, which it abends at only once the task check and
    /// the observer's sinks have passed.
    fn refused_cics(&mut self, block: &ExecBlock, abend: Abend, pos: Pos) -> R<CicsCommand> {
        if abend.code != AbendCode::Ironwork || abend.pos != pos {
            return unsupported("an EXEC CICS block the walker refuses with another abend than IRONWORK at the block", pos);
        }
        let mut sinks = Vec::new();
        for (r, sink) in cics_bind::sinks(block) {
            sinks.push((self.place(r, false)?, sink));
        }
        let (name, why) = (self.sym(&block.command), self.sym(&abend.message));
        Ok(CicsCommand { name, command: Cics::Refused(why), resp: Resp { resp: None, resp2: None, nohandle: false }, sinks })
    }
}
