//! Operands, expressions and FUNCTION, evaluated as the walker's `operand`, `expr_value`,
//! `eval_fixed`, `eval_float` and `function` evaluate them (lir.md §6, §9.9).

use super::{Code, Facts, R, Vm, not_yet};
use crate::abend::Abend;
use crate::arith;
pub(super) use crate::count::{Number, aligned, const_number, count_mod, int_binop};
use crate::fixed::places_of;
use crate::intrinsic::function::{self as intrinsic, Evaluator};
use crate::lir::{AbendId, Argument, Base, Comparand, Const, Count, Expr, ExprId, Func, FunctionId, FunctionPlan, IntExpr, Mode, Operand, PlaceId, RefMod, SenderCheck};
use crate::storage::{Kind, Loc, Val};
use crate::store::{self, ProgramFacts};
use crate::unit::{ADDRESS_BASE, Loader};
use crate::vocab::{BinOp, Figurative, Pos};
use numeric::precision::{Fixed, Places};
use std::rc::Rc;
use zarch::hfp::{Hfp, Precision};

/// A floating-point constant's value from its 8 or 16 bytes.
pub fn float_constant(bytes: &[u8]) -> zarch::hfp::Hfp {
    let precision = if bytes.len() == 16 { zarch::hfp::Precision::Extended } else { zarch::hfp::Precision::Long };
    zarch::hfp::Hfp::from_bytes(precision, bytes)
}

/// A constant's value, or the abend reading a refused one gives.
pub(super) fn constant(c: &Const) -> Result<Val, AbendId> {
    Ok(match c {
        Const::Bytes(b) => Val::Bytes(b.clone()),
        Const::National(b) => Val::National(b.clone()),
        Const::Number(f) => Val::Num(*f),
        Const::Figurative(f) => Val::Fig(*f),
        Const::All(b) => Val::All(b.clone()),
        Const::AllNational(b) => Val::AllNational(b.clone()),
        Const::Dbcs(b) => Val::Dbcs(b.clone()),
        Const::Float(b) => Val::Float(float_constant(b)),
        Const::Refused(abend) => return Err(*abend),
    })
}

impl<'p, L: Loader<Rc<Code>>> Vm<'p, '_, '_, L> {
    pub(super) fn read(&self, loc: Loc, pos: Pos) -> R<Val> {
        Ok(store::read(&self.facts(), &self.unit.mem, loc, pos)?)
    }

    /// NUMCHECK's test of an item once located and before it is read, under the option.
    pub(super) fn numcheck(&mut self, loc: Loc, check: SenderCheck, pos: Pos) -> R<()> {
        if self.p.options.options.numcheck.is_none() {
            return Ok(());
        }
        let facts = self.facts();
        Ok(store::numcheck_sender(&facts, self.unit, loc, check, self.sym(self.p.id), pos)?)
    }

    /// Place `p`, located at `loc`, tested and read as `Machine::operand` reads a data item.
    pub(super) fn read_tested(&mut self, p: PlaceId, loc: Loc) -> R<Val> {
        let pos = self.pos(self.p.places[p as usize].at);
        self.numcheck(loc, SenderCheck::Item, pos)?;
        self.read(loc, pos)
    }

    /// `Machine::operand`: a data item located and read, a literal, LENGTH OF, ADDRESS OF or FUNCTION.
    pub(super) fn value(&mut self, o: Operand) -> R<Val> {
        match o {
            Operand::Load(p) => {
                let loc = self.loc(p)?;
                self.read_tested(p, loc)
            }
            Operand::Const(c) => constant(&self.p.consts[c as usize]).map_err(|a| self.abend(a, None).into()),
            Operand::LengthOf(p) => {
                // A LINKAGE item's length is the compile's, less what its OCCURS DEPENDING ON
                // tables do not hold, read without its address (Language Reference, LENGTH OF).
                let program = self.p;
                let q = &program.places[p as usize];
                let len = if matches!(q.base, Base::Linkage(_)) && q.refmod.is_none() {
                    let pos = self.pos(q.at);
                    let mut len = i64::from(q.len);
                    for odo in &q.odo {
                        let current = self.occurrences(odo, pos)?;
                        len = crate::loc::odo_len(len, odo.max, current, odo.element);
                    }
                    len as usize
                } else {
                    self.loc(p)?.len
                };
                Ok(Val::Num(Fixed::new(len as i128, Places::new(9, 0))))
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
            return Ok((self.read_tested(p, loc)?, Some(loc)));
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
        self.eval_fixed_at(e, dmax, dmax, pos)
    }

    /// `Machine::eval_fixed_at`: `e`'s top operation at `last` places and every operation below
    /// it at `inner`.
    pub(super) fn eval_fixed_at(&mut self, e: ExprId, last: u32, inner: u32, pos: Pos) -> R<Fixed> {
        Ok(self.eval_number_at(e, last, inner, pos)?.fixed())
    }

    /// `eval_fixed`, its value held as an integer while it is one.
    pub(super) fn eval_number(&mut self, e: ExprId, dmax: u32, pos: Pos) -> R<Number> {
        self.eval_number_at(e, dmax, dmax, pos)
    }

    /// The digits an intermediate result keeps under `arith`: ARITH's, or cobc's.
    pub(super) fn carry(&self, arith: numeric::Arith) -> numeric::precision::Carry {
        numeric::precision::Carry::of(arith, self.p.options.options.emulates_cobc())
    }

    /// `eval_fixed_at`, its value held as an integer while it is one.
    pub(super) fn eval_number_at(&mut self, e: ExprId, last: u32, inner: u32, pos: Pos) -> R<Number> {
        let arith = self.p.options.options.arith;
        match &self.p.exprs[e as usize] {
            Expr::Operand(o) => self.operand_number(*o, last, pos),
            Expr::Neg(operand) => Ok(match self.eval_number_at(*operand, inner, inner, pos)? {
                Number::Int(n, places) if n != i64::MIN => Number::Int(-n, places),
                v => Number::Fixed(arith::fixed_neg(v.fixed())),
            }),
            Expr::Bin(a, op, b) => {
                let x = self.eval_number_at(*a, inner, inner, pos)?;
                let y = self.eval_number_at(*b, inner, inner, pos)?;
                if let Some(r) = int_binop(x, *op, y, last, self.carry(arith)) {
                    return Ok(r);
                }
                let (x, y) = (x.fixed(), y.fixed());
                if arith::divides_by_zero(*op, &y) {
                    let binary = self.binary_division(*a, *b)?;
                    return Err(arith::zero_divide(binary, pos).into());
                }
                Ok(Number::Fixed(arith::fixed_binop(x, *op, y, last, self.carry(arith), pos)?))
            }
            Expr::Pow(base, exponent) => {
                let x = self.eval_number_at(*base, inner, inner, pos)?.fixed();
                let n = self.int(exponent, pos)?;
                Ok(Number::Fixed(arith::pow(x, n, last, arith, self.p.options.options.emulates_cobc(), pos)?))
            }
        }
    }

    /// An operand as `value` reads it, then `fixed_operand` takes it.
    pub(super) fn operand_number(&mut self, o: Operand, dmax: u32, pos: Pos) -> R<Number> {
        match o {
            Operand::Load(p) => {
                if let Some(n) = self.static_number(p) {
                    return Ok(n);
                }
                let place = &self.p.places[p as usize];
                let loc = self.loc(p)?;
                let at = self.pos(place.at);
                self.numcheck(loc, SenderCheck::Item, at)?;
                if super::place::plain(place)
                    && let Some(n) = store::read_digits(&self.facts(), &self.unit.mem, loc)
                {
                    return Ok(Number::Int(n, places_of(loc.kind)));
                }
                let val = self.read(loc, at)?;
                Ok(Number::Fixed(arith::fixed_operand(val, dmax, pos)?))
            }
            Operand::Function(f) => match self.function_count(f)? {
                Some(n) => Ok(n),
                None => {
                    let val = self.value(o)?;
                    Ok(Number::Fixed(arith::fixed_operand(val, dmax, pos)?))
                }
            },
            Operand::Const(c) => match self.code.literals[c as usize] {
                Some((n, places)) => Ok(Number::Int(n, places)),
                None => {
                    let val = self.value(o)?;
                    Ok(Number::Fixed(arith::fixed_operand(val, dmax, pos)?))
                }
            },
            _ => {
                let val = self.value(o)?;
                Ok(Number::Fixed(arith::fixed_operand(val, dmax, pos)?))
            }
        }
    }

    /// FUNCTION MOD of two fixed-point arguments, each evaluated as `comparand` evaluates it and
    /// held as a count, the intrinsic taking the two values only where `count_mod` cannot; None,
    /// with nothing evaluated, for any other function or argument.
    fn function_count(&mut self, id: FunctionId) -> R<Option<Number>> {
        let plan = &self.p.plans.function[id as usize];
        let (Func::Mod, None, [Argument::Value(a), Argument::Value(b)]) = (plan.func, &plan.refmod, plan.args.as_slice()) else { return Ok(None) };
        if !(self.countable(a) && self.countable(b)) {
            return Ok(None);
        }
        let pos = self.pos(plan.at);
        let x = self.comparand_number(a, pos)?;
        let y = self.comparand_number(b, pos)?;
        if let Some(n) = count_mod(x, y) {
            return Ok(Some(n));
        }
        let mut args = vec![Val::Num(x.fixed()), Val::Num(y.fixed())];
        let result = intrinsic::evaluate(&mut Call { vm: self, plan }, plan.func.name(), plan.side, &mut args, pos);
        match self.settle(result)? {
            Val::Num(f) => Ok(Some(Number::of(f))),
            _ => Err(not_yet("FUNCTION MOD of numbers that is not a number")),
        }
    }

    /// A function argument `comparand` reads as a fixed-point number: an arithmetic expression, a
    /// numeric literal or a binary, packed or zoned item.
    fn countable(&self, c: &Comparand) -> bool {
        match c {
            Comparand::Expr { mode: Mode::Fixed, .. } => true,
            Comparand::Expr { .. } => false,
            Comparand::Operand(Operand::Const(k)) => self.code.literals[*k as usize].is_some(),
            Comparand::Operand(Operand::Load(p)) => matches!(self.p.places[*p as usize].kind, Kind::Binary { .. } | Kind::Packed { .. } | Kind::Zoned { .. }),
            Comparand::Operand(_) => false,
        }
    }

    /// `comparand` of an argument `countable` takes, held as a `Number`.
    fn comparand_number(&mut self, c: &Comparand, pos: Pos) -> R<Number> {
        match c {
            Comparand::Expr { expr, dmax, prepass, .. } => {
                for &q in prepass {
                    self.loc(q)?;
                }
                self.eval_number(*expr, *dmax, pos)
            }
            Comparand::Operand(o) => self.operand_number(*o, 0, pos),
        }
    }

    /// `Machine::eval_float`: an exponent is evaluated as a float, without its dmax pass.
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
                Ok(arith::float_binop(x, *op, y, p, self.p.options.options.emulates_cobc(), pos)?)
            }
            Expr::Pow(base, exponent) => {
                let x = self.eval_float(*base, p, pos)?;
                let y = match exponent {
                    IntExpr::Const(n) => Hfp::from_integer(i128::from(*n), p),
                    IntExpr::Item(q) => {
                        let val = self.value(Operand::Load(*q))?;
                        arith::float_operand(val, p, pos)?
                    }
                    IntExpr::Fixed { expr, .. } => self.eval_float(*expr, p, pos)?,
                    IntExpr::Walk(_) => return Err(not_yet("a JSON walk subscript as an exponent")),
                };
                Ok(arith::float_binop(x, BinOp::Pow, y, p, self.p.options.options.emulates_cobc(), pos)?)
            }
        }
    }

    /// `Machine::binary_division`: whether a zero divisor is the fixed-point divide's (S0C9),
    /// every operand of both sides an integer binary item or integer literal and one an item.
    pub(super) fn binary_division(&mut self, a: ExprId, b: ExprId) -> R<bool> {
        let mut walk = Division::default();
        let all = self.binary_operands(a, &mut walk)? && self.binary_operands(b, &mut walk)?;
        let undecided = || not_yet("a zero divisor beside ZERO or an integer exponent, which may have been written ALL ZERO or with a decimal point");
        match all {
            false if walk.undecided && self.p.options.options.numcheck.is_some() => Err(undecided()),
            false => Ok(false),
            true if walk.items == 0 => Ok(false),
            true if walk.undecided => Err(undecided()),
            true => Ok(true),
        }
    }

    /// Whether each operand, left to right, is one the fixed-point divide takes, each item located
    /// again, until one is not. An operand the LIR does not keep enough of to decide is taken to be
    /// one and noted: ZERO, which may have been written ALL ZERO, and an integer exponent, which
    /// may have been written with a decimal point.
    fn binary_operands(&mut self, e: ExprId, walk: &mut Division) -> R<bool> {
        Ok(match &self.p.exprs[e as usize] {
            Expr::Operand(Operand::Const(c)) => match &self.p.consts[*c as usize] {
                Const::Number(f) => f.places.dec == 0,
                Const::Figurative(Figurative::Zero) => {
                    walk.undecided = true;
                    true
                }
                _ => false,
            },
            Expr::Operand(Operand::LengthOf(_)) => {
                walk.items += 1;
                true
            }
            Expr::Operand(Operand::Load(place)) => self.binary_item(*place, walk)?,
            Expr::Operand(_) => false,
            Expr::Neg(inner) => self.binary_operands(*inner, walk)?,
            Expr::Bin(x, _, y) => self.binary_operands(*x, walk)? && self.binary_operands(*y, walk)?,
            Expr::Pow(x, exponent) => {
                self.binary_operands(*x, walk)?
                    && match exponent {
                        IntExpr::Const(_) => {
                            walk.undecided = true;
                            true
                        }
                        IntExpr::Item(place) => self.binary_item(*place, walk)?,
                        IntExpr::Fixed { expr, .. } => self.binary_operands(*expr, walk)?,
                        IntExpr::Walk(_) => return Err(not_yet("a JSON walk subscript as an exponent")),
                    }
            }
        })
    }

    fn binary_item(&mut self, place: PlaceId, walk: &mut Division) -> R<bool> {
        let binary = matches!(self.loc(place)?.kind, Kind::Binary { scale: 0, .. } | Kind::Index);
        walk.items += usize::from(binary);
        Ok(binary)
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
            let mut args = std::mem::take(&mut self.spare.args);
            args.clear();
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
            let counted = match (plan.func, args.as_slice()) {
                (Func::Mod, [Val::Num(a), Val::Num(b)]) => count_mod(Number::of(*a), Number::of(*b)),
                _ => None,
            };
            let result = match counted {
                Some(n) => Ok(Val::Num(n.fixed())),
                None => intrinsic::evaluate(&mut Call { vm: self, plan }, plan.func.name(), plan.side, &mut args, pos),
            };
            args.clear();
            self.spare.args = args;
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
            out.push(self.read_tested(element, loc)?);
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

/// What `binary_operands` has found so far: the items, and whether an operand it took to be one
/// the fixed-point divide takes may not have been.
#[derive(Default)]
struct Division {
    items: usize,
    undecided: bool,
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
            None => Err(not_yet("a FUNCTION argument read again without its plan")),
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

    fn caller(&mut self) -> Option<String> {
        let unit = &self.vm.unit;
        unit.caller_of(self.vm.me).map(|p| unit.programs[p].name.clone())
    }

    fn crt_status(&mut self) -> u16 {
        self.vm.unit.crt.as_ref().and_then(|c| c.borrow().last_key).map_or(0, crate::crt::crt_status)
    }

    fn run_argument(&mut self, position: usize) -> Option<String> {
        position.checked_sub(1).and_then(|k| self.vm.unit.arguments.words.get(k)).cloned()
    }

    fn heap_allocate(&mut self, size: usize) -> u32 {
        self.vm.unit.heap_allocate(size)
    }

    fn heap_free(&mut self, address: u32) -> u32 {
        self.vm.unit.heap_free(address)
    }

    fn argument_length(&mut self, position: usize) -> usize {
        self.vm.unit.argument_length_of(self.vm.me, position)
    }

    fn argument_count(&mut self) -> usize {
        self.vm.unit.argument_count_of(self.vm.me)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy)]
    struct Plain;

    impl ProgramFacts for Plain {
        fn options(&self) -> numeric::Options {
            numeric::Options::default()
        }
        fn page(&self) -> &'static zarch::ebcdic::CodePage {
            numeric::Options::default().code_page()
        }
        fn figurative(&self, _: Figurative) -> u8 {
            0
        }
        fn collation(&self) -> &zarch::ebcdic::Collation {
            &zarch::ebcdic::Collation::Native
        }
        fn ordinal(&self, byte: u8) -> u16 {
            u16::from(byte) + 1
        }
        fn character(&self, _: i64) -> Option<u8> {
            None
        }
        fn characters(&self) -> usize {
            256
        }
        fn decimal_point(&self) -> char {
            '.'
        }
        fn edit(&self, _: u32) -> (&[crate::picture::Sym], &str) {
            (&[], "")
        }
        fn scaling(&self, _: usize) -> u32 {
            0
        }
        fn item_name(&self, _: usize) -> String {
            String::new()
        }
    }

    struct Nothing(Option<u32>);

    impl Evaluator for Nothing {
        type Facts = Plain;
        fn facts(&self) -> Plain {
            Plain
        }
        fn integer(&mut self, _: usize, pos: Pos) -> Result<i64, Abend> {
            Err(Abend::ironwork("no integer argument", pos))
        }
        fn written(&self) -> usize {
            2
        }
        fn now(&self) -> (i64, u32) {
            (0, 0)
        }
        fn compiled(&self) -> (i64, u32) {
            (0, 0)
        }
        fn random(&mut self) -> &mut Option<u32> {
            &mut self.0
        }
        fn currency(&self) -> String {
            String::new()
        }
        fn caller(&mut self) -> Option<String> {
            None
        }
        fn argument_length(&mut self, _: usize) -> usize {
            0
        }

        fn argument_count(&mut self) -> usize {
            0
        }

        fn run_argument(&mut self, _: usize) -> Option<String> {
            None
        }

        fn crt_status(&mut self) -> u16 {
            0
        }

        fn heap_allocate(&mut self, _: usize) -> u32 {
            0
        }

        fn heap_free(&mut self, address: u32) -> u32 {
            address
        }
    }

    #[test]
    fn mod_of_two_counts_is_the_intrinsic_s() {
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            state.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        };
        let mut fast = 0;
        for _ in 0..40_000 {
            let mut operand = || {
                let digits = 1 + (next() % 18) as u32;
                let dec = (next() % u64::from(digits + 1)) as u32;
                let n = (next() % 10u64.pow(digits)) as i64;
                let n = if next() % 4 == 0 { n % 10 } else { n };
                (if next() % 2 == 0 { -n } else { n }, Places::new(digits - dec, dec))
            };
            let ((x, px), (y, py)) = (operand(), operand());
            let (a, b) = (Number::Int(x, px), Number::Int(y, py));
            let mut args = vec![Val::Num(a.fixed()), Val::Num(b.fixed())];
            let general = intrinsic::evaluate(&mut Nothing(None), "MOD", None, &mut args, Pos::default());
            match (count_mod(a, b), general) {
                (Some(r), Ok(Val::Num(general))) => {
                    fast += 1;
                    assert_eq!(r.fixed(), general, "MOD({x} at {px:?}, {y} at {py:?})");
                }
                (Some(r), other) => panic!("MOD({x} at {px:?}, {y} at {py:?}) gave {r:?} where the intrinsic gives {other:?}"),
                (None, _) => assert!(y == 0, "MOD({x} at {px:?}, {y} at {py:?}) left the counts"),
            }
        }
        assert!(fast > 35_000, "only {fast} MODs taken as counts");
    }
}
