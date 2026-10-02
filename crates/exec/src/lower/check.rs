//! NUMCHECK's test of a MOVE's sender (lir.md §9.14), decided here from the two kinds as
//! `Machine::move_source` decides it on each execution, and whether evaluating a part of the LIR
//! may run the test at all.

use super::Lower;
use rt::lir::{Expr, ExprId, IntExpr, Operand, PlaceId, SenderCheck};

impl Lower<'_> {
    /// The test of a MOVE, WRITE, REWRITE or RELEASE FROM sender moved to `to` (`move_source`).
    pub(super) fn move_check(&self, from: Operand, to: PlaceId) -> SenderCheck {
        match from {
            Operand::Load(p) => rt::store::move_check(&self.c.options, self.kind_of(p), self.kind_of(to)),
            _ => SenderCheck::None,
        }
    }

    /// Whether evaluating `e` may run NUMCHECK's test on an item it reads or reads to locate one.
    pub(super) fn int_tested(&self, e: &IntExpr) -> bool {
        match e {
            IntExpr::Const(_) | IntExpr::Walk(_) => false,
            IntExpr::Item(p) => self.read_tested(*p),
            IntExpr::Fixed { expr, prepass, .. } => prepass.iter().any(|&q| self.locate_tested(q)) || self.expr_tested(*expr),
        }
    }

    /// Whether reading the item at `p` may run NUMCHECK's test, on the item or as it is located.
    pub(super) fn read_tested(&self, p: PlaceId) -> bool {
        rt::store::numcheck_tests(&self.c.options, self.kind_of(p), false) || self.locate_tested(p)
    }

    fn locate_tested(&self, p: PlaceId) -> bool {
        let place = &self.places[p as usize];
        place.subscripts.iter().any(|s| self.int_tested(&s.value))
            || place.odo.as_ref().is_some_and(|o| self.int_tested(&o.object))
            || place.refmod.as_ref().is_some_and(|r| self.int_tested(&r.start) || r.length.as_ref().is_some_and(|l| self.int_tested(l)))
    }

    fn expr_tested(&self, e: ExprId) -> bool {
        match &self.exprs[e as usize] {
            Expr::Operand(Operand::Load(p)) => self.read_tested(*p),
            Expr::Operand(Operand::LengthOf(p) | Operand::AddressOf(p)) => self.locate_tested(*p),
            Expr::Operand(Operand::Const(_)) => false,
            Expr::Operand(Operand::Function(_)) => self.c.options.numcheck.is_some(),
            Expr::Neg(a) => self.expr_tested(*a),
            Expr::Bin(a, _, b) => self.expr_tested(*a) || self.expr_tested(*b),
            Expr::Pow(a, n) => self.expr_tested(*a) || self.int_tested(n),
        }
    }
}
