//! NUMCHECK's test of a MOVE's sender (lir.md §9.14), decided here from the two kinds as
//! `Machine::move_source` decides it on each execution.

use super::Lower;
use rt::lir::{Operand, PlaceId, SenderCheck};

impl Lower<'_> {
    /// The test of a MOVE, WRITE, REWRITE or RELEASE FROM sender moved to `to` (`move_source`).
    pub(super) fn move_check(&self, from: Operand, to: PlaceId) -> SenderCheck {
        match from {
            Operand::Load(p) => rt::store::move_check(&self.c.options, self.kind_of(p), self.kind_of(to)),
            _ => SenderCheck::None,
        }
    }
}
