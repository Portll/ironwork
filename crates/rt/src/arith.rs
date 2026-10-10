//! The arithmetic core (lir.md §7): one fixed-point or floating-point operation, exponentiation,
//! DIVIDE's remainder and the size-error decision. The executor walks the expression and makes the
//! locate passes; dmax, the choice of floating point and ARITH are fixed by the plan.

use crate::abend::Abend;
use crate::fixed::{MAX_DIGITS, align, fixed};
use crate::intrinsic::math;
use crate::intrinsic::real::Real;
use crate::storage::Val;
use crate::vocab::{BinOp, Figurative, Pos};
use numeric::Arith;
use numeric::float;
use numeric::precision::{ArithError, Carry, Fixed, Places};
use std::cmp::Ordering;
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

/// ADD, SUBTRACT, MULTIPLY or DIVIDE at `dmax` places, the intermediate result keeping `carry`'s
/// digits. Exponentiation is `pow`.
pub fn fixed_binop(x: Fixed, op: BinOp, y: Fixed, dmax: u32, carry: Carry, pos: Pos) -> R<Fixed> {
    let result = match op {
        BinOp::Add => x.add(y, dmax, carry),
        BinOp::Sub => x.sub(y, dmax, carry),
        BinOp::Mul => x.mul(y, dmax, carry),
        BinOp::Div => x.div(y, dmax, carry),
        BinOp::Pow => return Err(Abend::ironwork("exponentiation of a receiver by a shared result", pos)),
    };
    result.map_err(|e| fixed_error(e, pos))
}

/// `x` to the integer power `n` at `dmax` places, as Enterprise COBOL takes an integral exponent in
/// fixed point (assumption C334): `x` multiplied by itself |n| - 1 times, 1 for an `n` of 0, and for
/// a negative `n` 1 divided by that power. Zero to a negative power is IGZ0050S, and zero compiled
/// for GnuCOBOL (`cobc`), as cobc gives it; a power that truncates to zero under a negative `n` is
/// IGZ0222S. An exponent of more than nine digits keeps its last nine. Past 31 the power is taken by
/// squaring, so an exponent costs its bits, not its value.
pub fn pow(x: Fixed, n: i64, dmax: u32, arith: Arith, cobc: bool, pos: Pos) -> R<Fixed> {
    let n = n % 1_000_000_000;
    if n < 0 && x.magnitude.is_zero() {
        return if cobc { Ok(Fixed::new(0, Places::new(1, 0))) } else { Err(Abend::zero_power(pos)) };
    }
    let carry = Carry::of(arith, cobc);
    let mul = |a: Fixed, b: Fixed| a.mul(b, dmax, carry).map_err(|e| fixed_error(e, pos));
    let one = Fixed::new(1, Places::new(1, 0));
    let magnitude = n.unsigned_abs();
    let power = if magnitude <= 31 {
        (0..magnitude).try_fold(one, |acc, _| mul(acc, x))?
    } else {
        let (mut acc, mut square, mut k) = (one, x, magnitude);
        while k > 0 {
            if k & 1 == 1 {
                acc = mul(acc, square)?;
            }
            k >>= 1;
            if k > 0 {
                square = mul(square, square)?;
            }
        }
        acc
    };
    if n >= 0 {
        return Ok(power);
    }
    if power.magnitude.is_zero() {
        let message = "IGZ0222S No significant digits remain in a fixed-point exponentiation operation due to excessive decimal positions specified in the operands or receivers.";
        return Err(Abend { code: crate::abend::AbendCode::user(4038), message: message.into(), pos, file: None });
    }
    one.div(power, dmax, carry).map_err(|e| fixed_error(e, pos))
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

/// `cobc`: compiled for GnuCOBOL, where zero to a negative power is zero.
pub fn float_binop(x: Hfp, op: BinOp, y: Hfp, p: Precision, cobc: bool, pos: Pos) -> R<Hfp> {
    let mask = ProgramMask::default();
    let result = match op {
        BinOp::Add => x.add(y, mask),
        BinOp::Sub => x.sub(y, mask),
        BinOp::Mul => x.mul(y, p, mask),
        BinOp::Div => x.div(y, mask),
        BinOp::Pow => return float_pow(x, y, p, cobc, pos),
    };
    result.map_err(|c| Abend::check(c, pos))
}

/// x ** y in floating point of precision `p`, nearest to the exact power (assumption C334). Zero
/// to a positive power is zero, to the power zero 1, and to a negative power IGZ0050S, which ON SIZE
/// ERROR takes as a size error, or zero compiled for GnuCOBOL (`cobc`); a negative base to a power
/// that is not an integer is taken as its absolute value (Language Reference SC27-8713-03,
/// pp. 296-297, Table 32).
pub fn float_pow(x: Hfp, y: Hfp, p: Precision, cobc: bool, pos: Pos) -> R<Hfp> {
    let (base, power) = (Real::from_hfp(x), Real::from_hfp(y));
    let value = if base.is_zero() {
        match power.compare(Real::ZERO) {
            Ordering::Greater => Real::ZERO,
            Ordering::Equal => Real::ONE,
            Ordering::Less if cobc => Real::ZERO,
            Ordering::Less => return Err(Abend::zero_power(pos)),
        }
    } else {
        math::pow(base, power)
    };
    value.to_hfp(p).map_err(|c| Abend::check(c, pos))
}

/// DIVIDE's REMAINDER: the dividend less the product of the divisor and the quotient cut to the
/// quotient receiver's `quotient_scale`. None for a zero divisor, which leaves the remainder as it was.
pub fn remainder(x: Fixed, y: Fixed, quotient_scale: u32, dmax: u32, carry: Carry, pos: Pos) -> R<Option<Fixed>> {
    if y.magnitude.is_zero() {
        return Ok(None);
    }
    let q = x.div(y, dmax, carry).map_err(|_| Abend::ironwork("remainder", pos))?;
    let q = fixed(q.negative, align(&q, quotient_scale, false).unwrap_or_default(), Places::new(q.places.int, quotient_scale));
    let r = q.mul(y, dmax, carry).and_then(|p| x.sub(p, dmax, carry)).map_err(|_| Abend::ironwork("remainder", pos))?;
    Ok(Some(r))
}

/// A receiver's result when `handled` (ON or NOT ON SIZE ERROR is written): a zero divisor or zero
/// to a negative power is then a size error, None, and the receiver keeps its value. Compiled for
/// GnuCOBOL (`cobc`), a zero divisor leaves the receiver as it was without the phrase too, as cobc does.
pub fn size_error(outcome: R<Val>, handled: bool, cobc: bool) -> R<Option<Val>> {
    match outcome {
        Err(a) if handled && a.size_error() || cobc && a.divides_by_zero() => Ok(None),
        other => other.map(Some),
    }
}
