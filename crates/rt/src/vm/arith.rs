//! COMPUTE, ADD, SUBTRACT, MULTIPLY and DIVIDE as `Machine::arithmetic` runs them (lir.md §7.4):
//! the dmax pre-pass's locates, every step evaluated with an abend held until its step stores,
//! the REMAINDER's operands, then each receiver located again and stored.

use super::value::{Number, int_binop};
use super::{Code, Halt, R, Stop, Vm};
use crate::abend::Abend;
use crate::arith;
use crate::fixed::places_of;
use crate::lir::{ArithPlan, ArithStep, Expr, ExprId, Mode, Operand, Step};
use crate::storage::{Kind, Loc, Val};
use crate::store;
use crate::unit::Loader;
use crate::vocab::{BinOp, Pos};
use std::rc::Rc;

/// A step's operation with its receiver under `per_receiver`, whether the receiver is its first
/// operand, and the receiver's operand.
type Own = Option<(BinOp, bool, ExprId)>;

/// A step evaluated, its abend held until it stores.
type Evaluated<'s> = (&'s ArithStep, ExprId, Own, Result<Val, Abend>);

impl<L: Loader<Rc<Code>>> Vm<'_, '_, '_, L> {
    pub(super) fn arith(&mut self, plan: &ArithPlan, pos: Pos) -> R<Step> {
        for &q in &plan.prepass {
            self.loc(q)?;
        }
        if let [step] = plan.steps.as_slice() {
            if step.mode == Mode::Fixed && plan.remainder.is_none() {
                return self.counted(plan, step, pos);
            }
            let evaluated = self.evaluated(plan, step, pos)?;
            return self.stored(plan, [evaluated], pos);
        }
        let mut results = Vec::with_capacity(plan.steps.len());
        for step in &plan.steps {
            results.push(self.evaluated(plan, step, pos)?);
        }
        self.stored(plan, results, pos)
    }

    /// `evaluated` then `stored` of a plan's one fixed-point step with no REMAINDER, in the same
    /// order of locates, reads and held abend, its value held as a `Number` until it stores.
    fn counted(&mut self, plan: &ArithPlan, step: &ArithStep, pos: Pos) -> R<Step> {
        for &q in &step.probe {
            self.loc(q)?;
        }
        let (shared, own) = self.shared(plan, step);
        let last = if own.is_some() { plan.inner_dmax } else { plan.dmax };
        let outcome = match self.eval_number_at(shared, last, plan.inner_dmax, pos).map_err(Stop::halt) {
            Err(Halt::Unimplemented(what)) => return Err(Halt::Unimplemented(what).into()),
            Err(Halt::Abend(a)) => Err(a),
            Ok(Number::Fixed(f)) => Ok(Number::of(f)),
            Ok(n) => Ok(n),
        };
        let loc = self.loc(step.target)?;
        let outcome = match (own, outcome) {
            (Some((op, receiver_first, receiver)), Ok(value)) => {
                let current = self.operand_number(Operand::Load(step.target), plan.dmax, pos)?;
                let (x, y) = if receiver_first { (current, value) } else { (value, current) };
                match int_binop(x, op, y, plan.dmax, plan.arith) {
                    Some(r) => Ok(r),
                    None => {
                        let (x, y) = (x.fixed(), y.fixed());
                        if arith::divides_by_zero(op, &y) {
                            let binary = self.binary_division(receiver, shared)?;
                            Err(arith::zero_divide(binary, pos))
                        } else {
                            arith::fixed_binop(x, op, y, plan.dmax, plan.arith, pos).map(Number::Fixed)
                        }
                    }
                }
            }
            (_, outcome) => outcome,
        };
        let value = match outcome {
            Ok(value) => value,
            Err(a) if plan.handled && a.size_error() => return Ok(Step::Arm(1)),
            // Compiled for GnuCOBOL, a zero divisor leaves the receiver as it was, as cobc does.
            Err(a) if self.p.options.options.emulates_cobc() && a.divides_by_zero() => return Ok(Step::Next),
            Err(a) => return Err(a.into()),
        };
        let counted = match value {
            Number::Int(n, places) => store::store_count(&self.facts(), self.unit, loc, (n, places), step.rounded, plan.handled, pos),
            Number::Fixed(_) => None,
        };
        let size_error = match counted {
            Some(stored) => stored?,
            None => store::store_value(&self.facts(), self.unit, loc, Val::Num(value.fixed()), step.rounded, plan.handled, pos)?,
        };
        Ok(if plan.handled { Step::Arm(u8::from(size_error)) } else { Step::Next })
    }

    fn evaluated<'s>(&mut self, plan: &ArithPlan, step: &'s ArithStep, pos: Pos) -> R<Evaluated<'s>> {
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
        let outcome = match outcome.map_err(Stop::halt) {
            Err(Halt::Unimplemented(what)) => return Err(Halt::Unimplemented(what).into()),
            Err(Halt::Abend(a)) => Err(a),
            Ok(v) => Ok(v),
        };
        Ok((step, shared, own, outcome))
    }

    /// The REMAINDER's operands, then each step's receiver located again and stored.
    fn stored<'s>(&mut self, plan: &ArithPlan, results: impl IntoIterator<Item = Evaluated<'s>>, pos: Pos) -> R<Step> {
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
                    let current = self.operand_number(Operand::Load(step.target), plan.dmax, pos)?;
                    let value = Number::of(value);
                    let (x, y) = if receiver_first { (current, value) } else { (value, current) };
                    match int_binop(x, op, y, plan.dmax, plan.arith) {
                        Some(r) => Ok(Val::Num(r.fixed())),
                        None => {
                            let (x, y) = (x.fixed(), y.fixed());
                            if arith::divides_by_zero(op, &y) {
                                let binary = self.binary_division(receiver, shared)?;
                                Err(arith::zero_divide(binary, pos))
                            } else {
                                arith::fixed_binop(x, op, y, plan.dmax, plan.arith, pos).map(Val::Num)
                            }
                        }
                    }
                }
                (Some((op, receiver_first, _)), Ok(Val::Float(value))) => {
                    let p = plan.arith.float_intermediate();
                    let current = self.value(Operand::Load(step.target))?;
                    let current = arith::float_operand(current, p, pos)?;
                    let (x, y) = if receiver_first { (current, value) } else { (value, current) };
                    arith::float_binop(x, op, y, p, self.p.options.options.emulates_cobc(), pos).map(Val::Float)
                }
                (_, outcome) => outcome,
            };
            let Some(value) = arith::size_error(outcome, plan.handled, self.p.options.options.emulates_cobc())? else {
                size_error = true;
                continue;
            };
            size_error |= self.store_result(loc, value, step.rounded, plan.handled, pos)?;
        }
        if let (Some(r), Some((x, y)), Some(q)) = (&plan.remainder, operands, quotient)
            && let Some(rest) = arith::remainder(x, y, places_of(q.kind).dec, plan.dmax, plan.arith, pos)?
        {
            let loc = self.loc(r.target)?;
            size_error |= store::store_value(&self.facts(), self.unit, loc, Val::Num(rest), false, plan.handled, pos)?;
        }
        Ok(if plan.handled { Step::Arm(u8::from(size_error)) } else { Step::Next })
    }

    /// `store::store_value`, a number held as a count stored into a binary, packed or zoned item
    /// without a `Fixed`.
    fn store_result(&mut self, loc: Loc, value: Val, rounded: bool, handled: bool, pos: Pos) -> R<bool> {
        if let (Kind::Binary { .. } | Kind::Packed { .. } | Kind::Zoned { .. }, Val::Num(f)) = (loc.kind, &value)
            && let Number::Int(n, places) = Number::of(*f)
            && let Some(stored) = store::store_count(&self.facts(), self.unit, loc, (n, places), rounded, handled, pos)
        {
            return Ok(stored?);
        }
        Ok(store::store_value(&self.facts(), self.unit, loc, value, rounded, handled, pos)?)
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
