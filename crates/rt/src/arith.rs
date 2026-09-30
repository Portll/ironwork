//! The arithmetic core (lir.md §7): one fixed-point or floating-point operation, exponentiation,
//! DIVIDE's remainder and the size-error decision. The executor walks the expression and makes the
//! locate passes; dmax, the choice of floating point and ARITH are fixed by the plan.

use crate::abend::Abend;
use crate::fixed::{MAX_DIGITS, align, fixed};
use crate::storage::Val;
use crate::vocab::{BinOp, Figurative, Pos};
use numeric::Arith;
use numeric::float;
use numeric::precision::{ArithError, Fixed, Places};
use zarch::check::{ProgramCheck, ProgramMask};
use zarch::hfp::{Hfp, Precision};

type R<T> = Result<T, Abend>;

fn fixed_error(e: ArithError, pos: Pos) -> Abend {
    match e {
        ArithError::DivideByZero => Abend::check(ProgramCheck::DecimalDivide, pos),
        ArithError::BeyondModel => Abend::ironwork("an intermediate result wider than 256 bits", pos),
    }
}

/// An operand's value in fixed point; a floating-point one converted at `dmax` places.
pub fn fixed_operand(val: Val, dmax: u32, pos: Pos) -> R<Fixed> {
    match val {
        Val::Num(f) => Ok(f),
        Val::Float(h) => Ok(float::to_fixed(h, Places::new(MAX_DIGITS as u32 - dmax.min(MAX_DIGITS as u32), dmax), false).0),
        Val::Fig(Figurative::Zero) => Ok(Fixed::new(0, Places::new(1, 0))),
        _ => Err(Abend::ironwork("a non-numeric operand in arithmetic", pos)),
    }
}

pub fn fixed_neg(v: Fixed) -> Fixed {
    fixed(!v.negative, v.magnitude, v.places)
}

/// Whether the operation divides by zero, which the executor turns into `zero_divide`'s abend.
pub fn divides_by_zero(op: BinOp, y: &Fixed) -> bool {
    op == BinOp::Div && y.magnitude.is_zero()
}

/// A division by zero: S0C9 when the compiler divides with the fixed-point divide instruction
/// (assumption C55), S0CB otherwise.
pub fn zero_divide(binary: bool, pos: Pos) -> Abend {
    Abend::check(if binary { ProgramCheck::FixedPointDivide } else { ProgramCheck::DecimalDivide }, pos)
}

/// ADD, SUBTRACT, MULTIPLY or DIVIDE at `dmax` places. Exponentiation is `pow`.
pub fn fixed_binop(x: Fixed, op: BinOp, y: Fixed, dmax: u32, arith: Arith, pos: Pos) -> R<Fixed> {
    let result = match op {
        BinOp::Add => x.add(y, dmax, arith),
        BinOp::Sub => x.sub(y, dmax, arith),
        BinOp::Mul => x.mul(y, dmax, arith),
        BinOp::Div => x.div(y, dmax, arith),
        BinOp::Pow => return Err(Abend::ironwork("exponentiation of a receiver by a shared result", pos)),
    };
    result.map_err(|e| fixed_error(e, pos))
}

/// `x` to the power `n`, an integer from 0 to 31, by repeated multiplication at `dmax` places.
pub fn pow(x: Fixed, n: i64, dmax: u32, arith: Arith, pos: Pos) -> R<Fixed> {
    if !(0..=31).contains(&n) {
        return Err(Abend::ironwork("exponentiation other than by an integer from 0 to 31 is not supported yet", pos));
    }
    let mut acc = Fixed::new(1, Places::new(1, 0));
    for _ in 0..n {
        acc = acc.mul(x, dmax, arith).map_err(|e| fixed_error(e, pos))?;
    }
    Ok(acc)
}

/// An operand's value in floating point of precision `p`.
pub fn float_operand(val: Val, p: Precision, pos: Pos) -> R<Hfp> {
    match val {
        Val::Float(h) if h.precision.digits() <= p.digits() => Ok(h.lengthen(p)),
        Val::Float(h) => Ok(float::narrow(h, p)),
        Val::Num(f) => float::from_fixed(f, p, ProgramMask::default()).map_err(|c| Abend::check(c, pos)),
        Val::Fig(Figurative::Zero) => Ok(Hfp::zero(p)),
        _ => Err(Abend::ironwork("a non-numeric operand in arithmetic", pos)),
    }
}

/// Negation; zero keeps its sign.
pub fn float_neg(v: Hfp) -> Hfp {
    if v.fraction == 0 { v } else { Hfp { negative: !v.negative, ..v } }
}

pub fn float_binop(x: Hfp, op: BinOp, y: Hfp, p: Precision, pos: Pos) -> R<Hfp> {
    let mask = ProgramMask::default();
    let result = match op {
        BinOp::Add => x.add(y, mask),
        BinOp::Sub => x.sub(y, mask),
        BinOp::Mul => x.mul(y, p, mask),
        BinOp::Div => x.div(y, mask),
        BinOp::Pow => return Err(Abend::ironwork("floating-point exponentiation is not supported yet", pos)),
    };
    result.map_err(|c| Abend::check(c, pos))
}

/// DIVIDE's REMAINDER: the dividend less the product of the divisor and the quotient cut to the
/// quotient receiver's `quotient_scale`. None for a zero divisor, which leaves the remainder as it was.
pub fn remainder(x: Fixed, y: Fixed, quotient_scale: u32, dmax: u32, arith: Arith, pos: Pos) -> R<Option<Fixed>> {
    if y.magnitude.is_zero() {
        return Ok(None);
    }
    let q = x.div(y, dmax, arith).map_err(|_| Abend::ironwork("remainder", pos))?;
    let q = fixed(q.negative, align(&q, quotient_scale, false).unwrap_or_default(), Places::new(q.places.int, quotient_scale));
    let r = q.mul(y, dmax, arith).and_then(|p| x.sub(p, dmax, arith)).map_err(|_| Abend::ironwork("remainder", pos))?;
    Ok(Some(r))
}

/// A receiver's result when `handled` (ON or NOT ON SIZE ERROR is written): a zero divisor is then
/// a size error, None, and the receiver keeps its value.
pub fn size_error(outcome: R<Val>, handled: bool) -> R<Option<Val>> {
    match outcome {
        Err(a) if handled && a.code.zero_divisor() => Ok(None),
        other => other.map(Some),
    }
}
