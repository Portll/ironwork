//! EXEC CICS (lir.md §9.5): each block bound by the walker's own `cics_bind::bind`, its handles
//! lowered, as one `Op::Cics`. HANDLE CONDITION keeps the paragraphs its labels name, which the op
//! returns as `Step::GoTo` when a condition takes one; HANDLE ABEND keeps its LABEL's.

use super::{Lower, LowerError, R, push, unsupported};
use crate::machine::cics_bind;
use rt::cics::Handles;
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
        // The walker abends at a label that names no procedure only once the block is in a task.
        let Ok(bound) = cics_bind::bind(block, &|text| cics_bind::label(program, block, text, para)) else {
            return unsupported("a HANDLE label that names no procedure", pos);
        };
        let command = bound.map(&mut Lowering { l: self, pos })?;
        let id = push(&mut self.services.cics, command, "EXEC CICS commands")?;
        self.op(Op::Cics(id), pos)
    }
}
