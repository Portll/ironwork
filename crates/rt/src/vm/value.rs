//! Operands, expressions and FUNCTION, evaluated as the walker's `operand`, `expr_value`,
//! `eval_fixed`, `eval_float` and `function` evaluate them (lir.md §6, §9.9).

use super::{Code, Facts, Halt, R, Vm, not_yet};
use crate::abend::Abend;
use crate::arith;
use crate::intrinsic::function::{self as intrinsic, Evaluator};
use crate::lir::{AbendId, Argument, Base, Comparand, Const, Count, Expr, ExprId, Func, FunctionId, FunctionPlan, IntExpr, Mode, Operand, PlaceId, RefMod};
use crate::storage::{Kind, Loc, Val};
use crate::store::{self, ProgramFacts};
use crate::unit::{ADDRESS_BASE, Loader};
use crate::vocab::{BinOp, Figurative, Pos};
use numeric::precision::{Fixed, Places};
use std::rc::Rc;
use zarch::hfp::{Hfp, Precision};

/// A constant's value, or the abend reading a refused one gives.
pub(super) fn constant(c: &Const) -> Result<Val, AbendId> {
    Ok(match c {
        Const::Bytes(b) => Val::Bytes(b.clone()),
        Const::National(b) => Val::National(b.clone()),
        Const::Number(f) => Val::Num(*f),
        Const::Figurative(f) => Val::Fig(*f),
        Const::All(b) => Val::All(b.clone()),
        Const::Refused(abend) => return Err(*abend),
    })
}

impl<'p, L: Loader<Rc<Code>>> Vm<'p, '_, '_, L> {
    pub(super) fn read(&self, loc: Loc, pos: Pos) -> R<Val> {
        Ok(store::read(&self.facts(), &self.unit.mem, loc, pos)?)
    }

    /// `Machine::operand`: a data item located and read, a literal, LENGTH OF, ADDRESS OF or FUNCTION.
    pub(super) fn value(&mut self, o: Operand) -> R<Val> {
        match o {
            Operand::Load(p) => {
                let loc = self.loc(p)?;
                self.read(loc, self.pos(self.p.places[p as usize].at))
            }
            Operand::Const(c) => constant(&self.p.consts[c as usize]).map_err(|a| self.abend(a, None).into()),
            Operand::LengthOf(p) => {
                let loc = self.loc(p)?;
                Ok(Val::Num(Fixed::new(loc.len as i128, Places::new(9, 0))))
            }
            Operand::AddressOf(p) => {
                if let Base::Linkage(record) = self.p.places[p as usize].base
                    && self.linkage[record as usize].is_none()
                {
                    return Ok(Val::Address(0));
                }
                let loc = self.loc(p)?;
                Ok(Val::Address(ADDRESS_BASE + loc.offset as u32))
            }
            Operand::Function(f) => self.function(f),
            Operand::UserFunction(f) => self.user_function(f),
        }
    }

    /// `Machine::operand_with_loc`: a data item's value and its `Loc`, anything else's value.
    pub(super) fn value_with_loc(&mut self, o: Operand) -> R<(Val, Option<Loc>)> {
        if let Operand::Load(p) = o {
            let loc = self.loc(p)?;
            return Ok((self.read(loc, self.pos(self.p.places[p as usize].at))?, Some(loc)));
        }
        Ok((self.value(o)?, None))
    }

    /// `Machine::expr_value`: an operand as it reads, an expression after its locate passes.
    pub(super) fn comparand(&mut self, c: &Comparand, pos: Pos) -> R<Val> {
        match c {
            Comparand::Operand(o) => self.value(*o),
            Comparand::Expr { expr, dmax, mode, prepass } => {
                for &q in prepass {
                    self.loc(q)?;
                }
                Ok(match mode {
                    Mode::Float(p) => Val::Float(self.eval_float(*expr, *p, pos)?),
                    Mode::Fixed => Val::Num(self.eval_fixed(*expr, *dmax, pos)?),
                })
            }
        }
    }

    /// `Machine::comparand`: an operand keeps its location for a comparison; an expression does not.
    pub(super) fn comparand_with_loc(&mut self, c: &Comparand, pos: Pos) -> R<(Val, Option<Loc>)> {
        match c {
            Comparand::Operand(o) => self.value_with_loc(*o),
            Comparand::Expr { .. } => Ok((self.comparand(c, pos)?, None)),
        }
    }

    pub(super) fn eval_fixed(&mut self, e: ExprId, dmax: u32, pos: Pos) -> R<Fixed> {
        let arith = self.p.options.options.arith;
        match &self.p.exprs[e as usize] {
            Expr::Operand(o) => {
                let val = self.value(*o)?;
                Ok(arith::fixed_operand(val, dmax, pos)?)
            }
            Expr::Neg(inner) => Ok(arith::fixed_neg(self.eval_fixed(*inner, dmax, pos)?)),
            Expr::Bin(a, op, b) => {
                let x = self.eval_fixed(*a, dmax, pos)?;
                let y = self.eval_fixed(*b, dmax, pos)?;
                if arith::divides_by_zero(*op, &y) {
                    let binary = self.binary_division(*a, *b)?;
                    return Err(arith::zero_divide(binary, pos).into());
                }
                Ok(arith::fixed_binop(x, *op, y, dmax, arith, pos)?)
            }
            Expr::Pow(base, exponent) => {
                let x = self.eval_fixed(*base, dmax, pos)?;
                let n = self.int(exponent, pos)?;
                Ok(arith::pow(x, n, dmax, arith, pos)?)
            }
        }
    }

    /// `Machine::eval_float`: an exponent is evaluated as a float, without its dmax pass, and then
    /// refused.
    pub(super) fn eval_float(&mut self, e: ExprId, p: Precision, pos: Pos) -> R<Hfp> {
        match &self.p.exprs[e as usize] {
            Expr::Operand(o) => {
                let val = self.value(*o)?;
                Ok(arith::float_operand(val, p, pos)?)
            }
            Expr::Neg(inner) => Ok(arith::float_neg(self.eval_float(*inner, p, pos)?)),
            Expr::Bin(a, op, b) => {
                let x = self.eval_float(*a, p, pos)?;
                let y = self.eval_float(*b, p, pos)?;
                Ok(arith::float_binop(x, *op, y, p, pos)?)
            }
            Expr::Pow(base, exponent) => {
                let x = self.eval_float(*base, p, pos)?;
                let y = match exponent {
                    IntExpr::Const(_) => x,
                    IntExpr::Item(q) => {
                        let val = self.value(Operand::Load(*q))?;
                        arith::float_operand(val, p, pos)?
                    }
                    IntExpr::Fixed { expr, .. } => self.eval_float(*expr, p, pos)?,
                    IntExpr::Walk(_) => return Err(not_yet("a JSON walk subscript as an exponent")),
                };
                Ok(arith::float_binop(x, BinOp::Pow, y, p, pos)?)
            }
        }
    }

    /// `Machine::binary_division`: whether a zero divisor is the fixed-point divide's (S0C9),
    /// every operand of both sides an integer binary item or integer literal and one an item.
    pub(super) fn binary_division(&self, a: ExprId, b: ExprId) -> R<bool> {
        let mut items = 0;
        let (x, y) = (self.binary_operands(a, &mut items)?, self.binary_operands(b, &mut items)?);
        match (x, y) {
            (Some(false), _) | (_, Some(false)) => Ok(false),
            _ if items == 0 => Ok(false),
            (Some(true), Some(true)) => Ok(true),
            _ => Err(not_yet("a zero divisor beside ZERO or an integer exponent, which may have been written ALL ZERO or with a decimal point")),
        }
    }

    /// Whether every operand is one the fixed-point divide takes, counting the items; None where
    /// the LIR does not keep what decides it: ZERO, which may have been written ALL ZERO, and an
    /// integer exponent, which may have been written with a decimal point.
    fn binary_operands(&self, e: ExprId, items: &mut usize) -> R<Option<bool>> {
        let binary_item = |items: &mut usize, place: PlaceId| {
            let binary = matches!(self.p.places[place as usize].kind, Kind::Binary { scale: 0, .. } | Kind::Index);
            *items += usize::from(binary);
            binary
        };
        Ok(match &self.p.exprs[e as usize] {
            Expr::Operand(Operand::Const(c)) => match &self.p.consts[*c as usize] {
                Const::Number(f) => Some(f.places.dec == 0),
                Const::Figurative(Figurative::Zero) => None,
                _ => Some(false),
            },
            Expr::Operand(Operand::LengthOf(_)) => {
                *items += 1;
                Some(true)
            }
            Expr::Operand(Operand::Load(place)) => Some(binary_item(items, *place)),
            Expr::Operand(_) => Some(false),
            Expr::Neg(inner) => self.binary_operands(*inner, items)?,
            Expr::Bin(x, _, y) => both(self.binary_operands(*x, items)?, self.binary_operands(*y, items)?),
            Expr::Pow(x, exponent) => {
                let base = self.binary_operands(*x, items)?;
                let exponent = match exponent {
                    IntExpr::Const(_) => None,
                    IntExpr::Item(place) => Some(binary_item(items, *place)),
                    IntExpr::Fixed { expr, .. } => self.binary_operands(*expr, items)?,
                    IntExpr::Walk(_) => return Err(not_yet("a JSON walk subscript as an exponent")),
                };
                both(base, exponent)
            }
        })
    }

    /// `Machine::function`: HEX-OF, BIT-OF and BYTE-LENGTH read an argument's storage; any other
    /// evaluates its arguments, then the function; then reference modification of the result.
    pub(super) fn function(&mut self, id: FunctionId) -> R<Val> {
        let plan = &self.p.plans.function[id as usize];
        let pos = self.pos(plan.at);
        let facts = self.facts();
        let value = if matches!(plan.func, Func::HexOf | Func::BitOf | Func::ByteLength) {
            if let Some(abend) = plan.arity {
                return Err(self.abend(abend, Some(plan.at)).into());
            }
            let [Argument::Value(arg)] = plan.args.as_slice() else { return Err(not_yet("a storage FUNCTION without its one argument")) };
            let bytes = match arg {
                Comparand::Operand(Operand::Load(p)) => {
                    let loc = self.loc(*p)?;
                    store::bytes(&self.unit.mem, loc).to_vec()
                }
                other => {
                    let val = self.comparand(other, pos)?;
                    intrinsic::stored_bytes(&facts, val, pos)?
                }
            };
            intrinsic::storage(&facts, plan.func.name(), &bytes, pos)?
        } else {
            let mut args = Vec::with_capacity(plan.args.len());
            for a in &plan.args {
                match a {
                    Argument::Value(c) => args.push(self.comparand(c, pos)?),
                    Argument::All { element, all } => self.all_elements(*element, all, &mut args)?,
                }
            }
            match plan.func {
                Func::Uuid4 => return Err(not_yet("FUNCTION UUID4, which gives another value on every run")),
                Func::Random if self.locating > 0 => return Err(not_yet("FUNCTION RANDOM in a subscript, reference modification or OCCURS DEPENDING ON")),
                _ => {}
            }
            let result = intrinsic::evaluate(&mut Call { vm: self, plan }, plan.func.name(), plan.side, args, pos);
            self.settle(result)?
        };
        self.refmodded(value, plan.refmod.as_ref(), pos)
    }

    /// A function's value reference-modified by `refmod`, its start and length evaluated now.
    pub(super) fn refmodded(&mut self, value: Val, refmod: Option<&RefMod>, pos: Pos) -> R<Val> {
        let Some(rm) = refmod else { return Ok(value) };
        let result = intrinsic::refmod(value, pos, || {
            let start = self.int(&rm.start, pos);
            let start = self.lift(start, pos)?;
            let length = match &rm.length {
                Some(l) => {
                    let length = self.int(l, pos);
                    Some(self.lift(length, pos)?)
                }
                None => None,
            };
            Ok((start, length))
        });
        self.settle(result)
    }

    /// `Machine::all_elements`: the counts of the ALL dimensions, left to right, then each
    /// element, the rightmost ALL subscript varying fastest, located and read.
    fn all_elements(&mut self, element: PlaceId, all: &[(u32, Count)], out: &mut Vec<Val>) -> R<()> {
        let pos = self.pos(self.p.places[element as usize].at);
        let mut counts = Vec::with_capacity(all.len());
        for (_, count) in all {
            counts.push(i64::from(self.count(count, pos)?));
        }
        if counts.contains(&0) {
            return Ok(());
        }
        let mut current: Vec<(u32, i64)> = all.iter().map(|&(at, _)| (at, 1)).collect();
        loop {
            let loc = self.loc_with(element, &current)?;
            out.push(self.read(loc, pos)?);
            let mut k = current.len();
            loop {
                if k == 0 {
                    return Ok(());
                }
                k -= 1;
                if current[k].1 < counts[k] {
                    current[k].1 += 1;
                    break;
                }
                current[k].1 = 1;
            }
        }
    }

    /// `Machine::program_name`: an alphanumeric value, decoded, trimmed and upper-cased.
    pub(super) fn program_name(&mut self, o: Operand, pos: Pos) -> R<String> {
        match self.value(o)? {
            Val::Bytes(b) => Ok(self.facts().page().decode(&b).trim().to_ascii_uppercase()),
            _ => Err(Abend::ironwork("a program name must be alphanumeric", pos).into()),
        }
    }
}

fn both(x: Option<bool>, y: Option<bool>) -> Option<bool> {
    match (x, y) {
        (Some(false), _) | (_, Some(false)) => Some(false),
        (Some(true), Some(true)) => Some(true),
        _ => None,
    }
}

/// What a FUNCTION reads beyond its arguments' values: its plan's integer argument again, the run
/// unit's clock and RANDOM state, and the program's compile time and NUMVAL-C currency.
struct Call<'a, 'p, 'u, 'w, L: Loader<Rc<Code>>> {
    vm: &'a mut Vm<'p, 'u, 'w, L>,
    plan: &'p FunctionPlan,
}

impl<'p, L: Loader<Rc<Code>>> Evaluator for Call<'_, 'p, '_, '_, L> {
    type Facts = Facts<'p>;

    fn facts(&self) -> Facts<'p> {
        self.vm.facts()
    }

    fn integer(&mut self, _k: usize, pos: Pos) -> Result<i64, Abend> {
        let value = match &self.plan.integer {
            Some(e) => self.vm.int(e, pos),
            None => Err(Halt::Unimplemented("a FUNCTION argument read again without its plan".into())),
        };
        self.vm.lift(value, pos)
    }

    fn written(&self) -> usize {
        self.plan.args.len()
    }

    fn now(&self) -> (i64, u32) {
        self.vm.unit.now()
    }

    fn compiled(&self) -> (i64, u32) {
        self.vm.p.options.when_compiled.map_or((0, 0), |t| (t.seconds, t.hundredths))
    }

    fn random(&mut self) -> &mut Option<u32> {
        &mut self.vm.unit.random
    }

    fn currency(&self) -> String {
        self.vm.p.options.numval_currency.clone()
    }
}
