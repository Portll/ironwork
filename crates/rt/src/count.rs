//! Fixed-point values held as an `i64` count of their last decimal place with their places, and
//! ADD, SUBTRACT, MULTIPLY, DIVIDE and FUNCTION MOD over them, carried and truncated exactly as
//! `Fixed` carries and truncates; None wherever a step leaves `i128` or the result an `i64`, for the
//! caller to take the `Fixed` path. The VM and a program's generated code both compute with these.

use crate::lir::Const;
use crate::vocab::BinOp;
use numeric::Arith;
use numeric::precision::{Fixed, Places, carried, product_places, quotient_places, sum_places};
use std::cmp::Ordering;

/// A constant as `operand_number` takes it while it fits an `i64` count of its last decimal place,
/// other than a negative zero.
#[inline]
pub fn const_number(c: &Const) -> Option<(i64, Places)> {
    match c {
        Const::Number(f) => match Number::of(*f) {
            Number::Int(n, places) => Some((n, places)),
            Number::Fixed(_) => None,
        },
        _ => None,
    }
}

/// A fixed-point value, held as a count of its last decimal place with its places while that fits
/// an `i64`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Number {
    Int(i64, Places),
    Fixed(Fixed),
}

impl Number {
    /// `f` as a count of its last decimal place where that fits an `i64`.
    #[inline]
    pub fn of(f: Fixed) -> Self {
        match f.to_i128().and_then(|n| i64::try_from(n).ok()) {
            Some(n) if !(f.negative && f.magnitude.is_zero()) => Self::Int(n, f.places),
            _ => Self::Fixed(f),
        }
    }

    #[inline]
    pub fn fixed(self) -> Fixed {
        match self {
            Self::Int(n, places) => Fixed::new(i128::from(n), places),
            Self::Fixed(f) => f,
        }
    }
}

/// ADD, SUBTRACT, MULTIPLY or DIVIDE of two counts as `Fixed` gives them: the exact result, or the
/// truncated quotient, kept to the places carried; None where a step leaves `i128`, the result does
/// not fit an `i64`, or the divisor is zero, which `Fixed` reports.
#[inline]
pub fn int_binop(x: Number, op: BinOp, y: Number, dmax: u32, arith: Arith) -> Option<Number> {
    let (Number::Int(x, px), Number::Int(y, py)) = (x, y) else { return None };
    let to = result_places(px, op, py, dmax, arith)?;
    binop(x, px, op, y, py, to).map(|n| Number::Int(n, to))
}

/// The places `x op y` carries at `dmax` under `arith`, from its operands' places alone; None for
/// an exponentiation.
#[inline]
pub fn result_places(px: Places, op: BinOp, py: Places, dmax: u32, arith: Arith) -> Option<Places> {
    Some(match op {
        BinOp::Add | BinOp::Sub => carried(sum_places(px, py), dmax, arith),
        BinOp::Mul => carried(product_places(px, py), dmax, arith),
        BinOp::Div => carried(quotient_places(px, py, dmax), dmax, arith),
        BinOp::Pow => return None,
    })
}

/// `int_binop` of counts `x` at `px` and `y` at `py`, its result kept to `to`, the places
/// `result_places` gives it.
#[inline]
pub fn binop(x: i64, px: Places, op: BinOp, y: i64, py: Places, to: Places) -> Option<i64> {
    let (x, y) = (i128::from(x), i128::from(y));
    let (exact, from) = match op {
        BinOp::Add | BinOp::Sub => {
            let (x, y) = aligned(x, px.dec, y, py.dec)?;
            let exact = if op == BinOp::Add { x.checked_add(y)? } else { x.checked_sub(y)? };
            (exact, sum_places(px, py))
        }
        BinOp::Mul => (x * y, product_places(px, py)),
        BinOp::Div if y != 0 => {
            let (numerator, denominator) = aligned(x, px.dec, y, py.dec + to.dec)?;
            return kept(numerator / denominator, to, to);
        }
        BinOp::Div | BinOp::Pow => return None,
    };
    kept(exact, from, to)
}

/// FUNCTION MOD of two counts as the intrinsic gives it: a - b * FLOOR(a / b) at the arguments'
/// greater decimal places, with the integer places of the shorter argument, high-order digits
/// dropped; None where either is not a count, or `b` is zero, which the intrinsic reports.
#[inline]
pub fn count_mod(a: Number, b: Number) -> Option<Number> {
    let (Number::Int(a, pa), Number::Int(b, pb)) = (a, b) else { return None };
    modulo(a, pa, b, pb).map(|n| Number::Int(n, mod_places(pa, pb)))
}

/// The places of FUNCTION MOD of arguments at `pa` and `pb`.
#[inline]
pub fn mod_places(pa: Places, pb: Places) -> Places {
    Places::new(pa.int.min(pb.int), pa.dec.max(pb.dec))
}

/// `count_mod` of counts `a` at `pa` and `b` at `pb`, at `mod_places`.
#[inline]
pub fn modulo(a: i64, pa: Places, b: i64, pb: Places) -> Option<i64> {
    let (a, b) = aligned(i128::from(a), pa.dec, i128::from(b), pb.dec)?;
    if b == 0 {
        return None;
    }
    let r = a % b;
    let r = if r != 0 && (r < 0) != (b < 0) { r + b } else { r };
    let places = mod_places(pa, pb);
    kept(r, places, places)
}

/// A count negated, as `Vm::eval_number_at` negates one; None for the one an `i64` cannot negate.
#[inline]
pub fn negated(n: i64) -> Option<i64> {
    (n != i64::MIN).then(|| -n)
}

/// Counts `x` at `dx` and `y` at `dy` decimal places compared by value; None past `i128`.
#[inline]
pub fn order(x: i64, dx: u32, y: i64, dy: u32) -> Option<Ordering> {
    let (x, y) = aligned(i128::from(x), dx, i128::from(y), dy)?;
    Some(x.cmp(&y))
}

/// Ten to each power an `i128` holds.
const POW10: [i128; 39] = {
    let mut table = [1; 39];
    let mut k = 1;
    while k < table.len() {
        table[k] = table[k - 1] * 10;
        k += 1;
    }
    table
};

#[inline]
fn pow10(n: u32) -> Option<i128> {
    POW10.get(n as usize).copied()
}

/// Counts `x` of `dx` and `y` of `dy` decimal places, held at the greater; None past `i128`.
#[inline]
pub fn aligned(x: i128, dx: u32, y: i128, dy: u32) -> Option<(i128, i128)> {
    match dx.cmp(&dy) {
        Ordering::Equal => Some((x, y)),
        Ordering::Less => Some((x.checked_mul(pow10(dy - dx)?)?, y)),
        Ordering::Greater => Some((x, y.checked_mul(pow10(dx - dy)?)?)),
    }
}

/// `Fixed::fit`: `exact`, held at `from`'s decimal places, kept to `to.dec` decimal places, the rest
/// truncated, and `to.int` integer places, the high-order digits dropped.
#[inline]
fn kept(exact: i128, from: Places, to: Places) -> Option<i64> {
    let mut magnitude = exact.unsigned_abs();
    if to.dec < from.dec {
        magnitude = pow10(from.dec - to.dec).map_or(0, |d| quotient(magnitude, d.unsigned_abs()));
    }
    if let Some(cap) = pow10(to.int + from.dec.min(to.dec)).map(i128::unsigned_abs)
        && magnitude >= cap
    {
        magnitude -= quotient(magnitude, cap) * cap;
    }
    let kept = i64::try_from(magnitude).ok()?.checked_mul(i64::try_from(pow10(to.dec.saturating_sub(from.dec))?).ok()?)?;
    Some(if exact < 0 { -kept } else { kept })
}

/// `m / d`, by the 64-bit divide where both fit it.
#[inline]
fn quotient(m: u128, d: u128) -> u128 {
    match (u64::try_from(m), u64::try_from(d)) {
        (Ok(m), Ok(d)) => u128::from(m / d),
        _ => m / d,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arith;
    use crate::vocab::Pos;

    #[test]
    fn integers_add_subtract_and_multiply_as_fixed_point_does() {
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            state.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        };
        let mut fast = 0;
        for _ in 0..20_000 {
            let mut operand = || {
                let digits = 1 + (next() % 31) as u32;
                let bits = next() % 64;
                let n = (next() >> (63 - bits)) as i64;
                (if next() % 2 == 0 { -n } else { n }, Places::new(digits, 0))
            };
            let ((x, px), (y, py)) = (operand(), operand());
            let (dmax, arith) = ((next() % 4) as u32, if next() % 2 == 0 { Arith::Compat } else { Arith::Extend });
            for op in [BinOp::Add, BinOp::Sub, BinOp::Mul] {
                let (a, b) = (Number::Int(x, px), Number::Int(y, py));
                let expected = arith::fixed_binop(a.fixed(), op, b.fixed(), dmax, arith, Pos::default()).unwrap();
                if let Some(r) = int_binop(a, op, b, dmax, arith) {
                    fast += 1;
                    assert_eq!(r.fixed(), expected, "{x} {op:?} {y} at {px:?} {py:?}, dmax {dmax}, {arith:?}");
                }
            }
        }
        assert!(fast > 40_000, "only {fast} operations were integers");
    }

    #[test]
    fn scaled_operands_add_subtract_multiply_and_divide_as_fixed_point_does() {
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            state.wrapping_mul(0x2545_F491_4F6C_DD1D)
        };
        let mut fast = [0; 4];
        for _ in 0..40_000 {
            let mut operand = || {
                let digits = 1 + (next() % 18) as u32;
                let dec = (next() % u64::from(digits + 1)) as u32;
                let n = (next() % 10u64.pow(digits)) as i64;
                (if next() % 2 == 0 { -n } else { n }, Places::new(digits - dec, dec))
            };
            let ((x, px), (y, py)) = (operand(), operand());
            let (dmax, arith) = ((next() % 10) as u32, if next() % 2 == 0 { Arith::Compat } else { Arith::Extend });
            for (k, op) in [BinOp::Add, BinOp::Sub, BinOp::Mul, BinOp::Div].into_iter().enumerate() {
                let (a, b) = (Number::Int(x, px), Number::Int(y, py));
                let expected = arith::fixed_binop(a.fixed(), op, b.fixed(), dmax, arith, Pos::default());
                match (int_binop(a, op, b, dmax, arith), expected) {
                    (Some(r), Ok(expected)) => {
                        fast[k] += 1;
                        assert_eq!(r.fixed(), expected, "{x} {op:?} {y} at {px:?} {py:?}, dmax {dmax}, {arith:?}");
                    }
                    (Some(r), Err(e)) => panic!("{x} {op:?} {y} gave {r:?} where fixed point gives {e:?}"),
                    (None, _) => {}
                }
            }
        }
        assert!(fast.iter().all(|&n| n > 20_000), "operations taken as counts: {fast:?}");
    }
}
