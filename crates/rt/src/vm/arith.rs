//! COMPUTE, ADD, SUBTRACT, MULTIPLY and DIVIDE as `Machine::arithmetic` runs them (lir.md §7.4):
//! the dmax pre-pass's locates, every step evaluated with an abend held until its step stores,
//! the REMAINDER's operands, then each receiver located again and stored.

use super::{Code, Halt, R, Vm};
use crate::abend::Abend;
use crate::arith;
use crate::fixed::places_of;
use crate::lir::{ArithPlan, ArithStep, Expr, ExprId, Mode, Operand, Step};
use crate::storage::{Loc, Val};
use crate::store;
use crate::unit::Loader;
use crate::vocab::{BinOp, Pos};
use std::rc::Rc;

/// A step's operation with its receiver under `per_receiver`, whether the receiver is its first
/// operand, and the receiver's operand.
type Own = Option<(BinOp, bool, ExprId)>;

impl<L: Loader<Rc<Code>>> Vm<'_, '_, '_, L> {
    pub(super) fn arith(&mut self, plan: &ArithPlan, pos: Pos) -> R<Step> {
        for &q in &plan.prepass {
            self.loc(q)?;
        }
        let mut results: Vec<(&ArithStep, ExprId, Own, Result<Val, Abend>)> = Vec::with_capacity(plan.steps.len());
        for step in &plan.steps {
            for &q in &step.probe {
                self.loc(q)?;
            }
            let (shared, own) = self.shared(plan, step);
            let outcome = match step.mode {
                Mode::Float(p) => self.eval_float(shared, p, pos).map(Val::Float),
                Mode::Fixed => {
                    let last = if own.is_some() { plan.inner_dmax } else { plan.dmax };
                    self.eval_fixed_at(shared, last, plan.inner_dmax, pos).map(Val::Num)
                }
            };
            let outcome = match outcome {
                Err(Halt::Unimplemented(what)) => return Err(Halt::Unimplemented(what)),
                Err(Halt::Abend(a)) => Err(a),
                Ok(v) => Ok(v),
            };
            results.push((step, shared, own, outcome));
        }
        let operands = match &plan.remainder {
            Some(r) => Some((self.eval_fixed(r.dividend, plan.dmax, pos)?, self.eval_fixed(r.divisor, plan.dmax, pos)?)),
            None => None,
        };
        let mut size_error = false;
        let mut quotient: Option<Loc> = None;
        for (step, shared, own, outcome) in results {
            let loc = self.loc(step.target)?;
            quotient.get_or_insert(loc);
            let outcome = match (own, outcome) {
                (Some((op, receiver_first, receiver)), Ok(Val::Num(value))) => {
                    let current = self.value(Operand::Load(step.target))?;
                    let current = arith::fixed_operand(current, plan.dmax, pos)?;
                    let (x, y) = if receiver_first { (current, value) } else { (value, current) };
                    if arith::divides_by_zero(op, &y) {
                        let binary = self.binary_division(receiver, shared)?;
                        Err(arith::zero_divide(binary, pos))
                    } else {
                        arith::fixed_binop(x, op, y, plan.dmax, plan.arith, pos).map(Val::Num)
                    }
                }
                (Some((op, receiver_first, _)), Ok(Val::Float(value))) => {
                    let p = plan.arith.float_intermediate();
                    let current = self.value(Operand::Load(step.target))?;
                    let current = arith::float_operand(current, p, pos)?;
                    let (x, y) = if receiver_first { (current, value) } else { (value, current) };
                    arith::float_binop(x, op, y, p, pos).map(Val::Float)
                }
                (_, outcome) => outcome,
            };
            let Some(value) = arith::size_error(outcome, plan.handled)? else {
                size_error = true;
                continue;
            };
            size_error |= store::store_value(&self.facts(), self.unit, loc, value, step.rounded, plan.handled, pos)?;
        }
        if let (Some(r), Some((x, y)), Some(q)) = (&plan.remainder, operands, quotient)
            && let Some(rest) = arith::remainder(x, y, places_of(q.kind).dec, plan.dmax, plan.arith, pos)?
        {
            let loc = self.loc(r.target)?;
            size_error |= store::store_value(&self.facts(), self.unit, loc, Val::Num(rest), false, plan.handled, pos)?;
        }
        Ok(if plan.handled { Step::Arm(u8::from(size_error)) } else { Step::Next })
    }

    /// What a step evaluates before any is stored: under `per_receiver`, the operand beside its
    /// own receiver; otherwise its whole expression.
    fn shared(&self, plan: &ArithPlan, step: &ArithStep) -> (ExprId, Own) {
        let own = |e: ExprId| plan.per_receiver && self.p.exprs[e as usize] == Expr::Operand(Operand::Load(step.target));
        match self.p.exprs[step.expr as usize] {
            Expr::Bin(a, op, b) if own(a) => (b, Some((op, true, a))),
            Expr::Bin(a, op, b) if own(b) => (a, Some((op, false, b))),
            _ => (step.expr, None),
        }
    }
}
