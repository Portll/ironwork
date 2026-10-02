//! The floating-point intrinsic functions over [`Real`]. Each result is within a few units of the
//! 128-bit working precision, so rounding it to long or extended HFP gives the nearest value except
//! in rare near-halfway cases (assumption C110). `None` is an argument outside the function's
//! domain.

use super::real::Real;
use std::cmp::Ordering;
use std::ops::{Add, Div, Mul, Neg, Sub};
use zarch::wide::U256;

/// π/2, ln 2 and ln 10 to 384 bits, most significant word first, each value = word₀word₁word₂ × 2^scale.
const HALF_PI: ([u128; 3], i32) = ([0xc90fdaa22168c234c4c6628b80dc1cd1, 0x29024e088a67cc74020bbea63b139b22, 0x514a08798e3404ddef9519b3cd3a431b], -383);
const LN_2: ([u128; 3], i32) = ([0xb17217f7d1cf79abc9e3b39803f2f6af, 0x40f343267298b62d8a0d175b8baafa2b, 0xe7b876206debac98559552fb4afa1b10], -384);
const LN_10: ([u128; 3], i32) = ([0x935d8dddaaa8ac16ea56d62b82d30a28, 0xe28fecf9da5df90e83c61e8201f02d72, 0x962f02d7b1a8105ccc70cbc02c5f0d68], -382);

/// Word `i` of a constant as a value.
fn word(c: ([u128; 3], i32), i: usize) -> Real {
    Real::new(false, U256::from_u128(c.0[i]), c.1 + 128 * (2 - i as i32))
}

fn constant(c: ([u128; 3], i32)) -> Real {
    word(c, 0).add(word(c, 1))
}

pub fn pi() -> Real {
    constant(HALF_PI).scaled(1)
}

pub fn half_pi() -> Real {
    constant(HALF_PI)
}

pub fn e() -> Real {
    exp(Real::ONE)
}

/// Adds terms from `next` until one no longer reaches the working precision of `sum`.
fn series(mut sum: Real, mut next: impl FnMut(u32) -> Real) -> Real {
    for n in 1..200 {
        let term = next(n);
        let negligible = match (term.binary_exponent(), sum.binary_exponent()) {
            (None, _) => true,
            (Some(t), Some(s)) => t < s - 132,
            (Some(_), None) => false,
        };
        if negligible {
            break;
        }
        sum = sum.add(term);
    }
    sum
}

/// k × a constant, carrying its second word so that k up to 2^20 loses nothing to rounding.
fn times(k: i128, c: ([u128; 3], i32)) -> Real {
    let k = Real::from_i128(k);
    k.mul(word(c, 0)).add(k.mul(word(c, 1)))
}

pub fn exp(x: Real) -> Real {
    if x.is_zero() {
        return Real::ONE;
    }
    let approx = x.to_f64();
    if approx > 4096.0 {
        return Real::huge();
    }
    if approx < -4096.0 {
        return Real::ZERO;
    }
    let k = (approx / std::f64::consts::LN_2).round() as i128;
    let r = x.sub(times(k, LN_2)).scaled(-8);
    let mut term = Real::ONE;
    let mut result = series(Real::ONE, |n| {
        term = term.mul(r).div(Real::from_u128(n as u128));
        term
    });
    for _ in 0..8 {
        result = result.mul(result);
    }
    result.scaled(k as i32)
}

pub fn ln(x: Real) -> Option<Real> {
    if x.is_zero() || x.is_negative() {
        return None;
    }
    let mut e = x.binary_exponent()?;
    let mut m = x.scaled(-e);
    if m.mul(m).compare(Real::from_u128(2)) == Ordering::Greater {
        m = m.scaled(-1);
        e += 1;
    }
    let z = m.sub(Real::ONE).div(m.add(Real::ONE));
    let z2 = z.mul(z);
    let mut power = z;
    let atanh = series(z, |n| {
        power = power.mul(z2);
        power.div(Real::from_u128(2 * n as u128 + 1))
    });
    Some(times(e as i128, LN_2).add(atanh.scaled(1)))
}

pub fn log10(x: Real) -> Option<Real> {
    Some(ln(x)?.div(constant(LN_10)))
}

pub fn exp10(x: Real) -> Real {
    exp(x.mul(constant(LN_10)))
}

pub fn sqrt(x: Real) -> Option<Real> {
    (!x.is_negative()).then(|| x.sqrt())
}

/// |x| - k·π/2 for the nearest integer k, and k mod 4; `None` when k passes 2^63. The first
/// word of π/2 is taken off exactly, in 256-bit integers.
fn reduce(x: Real) -> Option<(Real, u32)> {
    let x = x.abs();
    let k = (x.to_f64() / std::f64::consts::FRAC_PI_2).round();
    if k >= 9.2e18 {
        return None;
    }
    let k = k as u64;
    if k == 0 {
        return Some((x, 0));
    }
    let (significand, exponent) = x.parts();
    let whole = U256::from_u128(significand) << (exponent + 128) as u32;
    let taken = U256::widening_mul(k as u128, HALF_PI.0[0]) << 1;
    let first = if whole >= taken { Real::new(false, whole - taken, -128) } else { Real::new(true, taken - whole, -128) };
    let k_real = Real::from_u128(k as u128);
    let rest = k_real.mul(word(HALF_PI, 1)).add(k_real.mul(word(HALF_PI, 2)));
    Some((first.sub(rest), (k % 4) as u32))
}

fn sin_series(r: Real) -> Real {
    let r2 = r.mul(r);
    let mut term = r;
    series(r, |n| {
        term = term.mul(r2).div(Real::from_u128((2 * n * (2 * n + 1)) as u128)).neg();
        term
    })
}

fn cos_series(r: Real) -> Real {
    let r2 = r.mul(r);
    let mut term = Real::ONE;
    series(Real::ONE, |n| {
        term = term.mul(r2).div(Real::from_u128(((2 * n - 1) * 2 * n) as u128)).neg();
        term
    })
}

/// sin and cos of |x|.
fn sin_cos(x: Real) -> Option<(Real, Real)> {
    let (r, quadrant) = reduce(x)?;
    let (s, c) = (sin_series(r), cos_series(r));
    Some(match quadrant {
        0 => (s, c),
        1 => (c, s.neg()),
        2 => (s.neg(), c.neg()),
        _ => (c.neg(), s),
    })
}

pub fn sin(x: Real) -> Option<Real> {
    let (s, _) = sin_cos(x)?;
    Some(if x.is_negative() { s.neg() } else { s })
}

pub fn cos(x: Real) -> Option<Real> {
    Some(sin_cos(x)?.1)
}

pub fn tan(x: Real) -> Option<Real> {
    let (s, c) = sin_cos(x)?;
    if c.is_zero() {
        return None;
    }
    let t = s.div(c);
    Some(if x.is_negative() { t.neg() } else { t })
}

pub fn atan(x: Real) -> Real {
    if x.is_zero() {
        return Real::ZERO;
    }
    let a = x.abs();
    let inverted = a.compare(Real::ONE) == Ordering::Greater;
    let mut y = if inverted { Real::ONE.div(a) } else { a };
    for _ in 0..3 {
        y = y.div(Real::ONE.add(Real::ONE.add(y.mul(y)).sqrt()));
    }
    let y2 = y.mul(y);
    let mut power = y;
    let mut result = series(y, |n| {
        power = power.mul(y2).neg();
        power.div(Real::from_u128(2 * n as u128 + 1))
    })
    .scaled(3);
    if inverted {
        result = half_pi().sub(result);
    }
    if x.is_negative() { result.neg() } else { result }
}

fn within_one(x: Real) -> bool {
    x.abs().compare(Real::ONE) != Ordering::Greater
}

pub fn asin(x: Real) -> Option<Real> {
    if !within_one(x) {
        return None;
    }
    let rest = Real::ONE.sub(x).mul(Real::ONE.add(x));
    if rest.is_zero() {
        return Some(if x.is_negative() { half_pi().neg() } else { half_pi() });
    }
    Some(atan(x.div(rest.sqrt())))
}

pub fn acos(x: Real) -> Option<Real> {
    if !within_one(x) {
        return None;
    }
    let above = Real::ONE.add(x);
    if above.is_zero() {
        return Some(pi());
    }
    Some(atan(Real::ONE.sub(x).div(above).sqrt()).scaled(1))
}

/// base^n by squaring; `None` once the exponent passes any HFP value's.
fn power(base: Real, mut n: u128) -> Option<Real> {
    let (mut result, mut square) = (Real::ONE, base);
    while n > 0 {
        if n & 1 == 1 {
            result = result.mul(square);
        }
        n >>= 1;
        if n > 0 {
            square = square.mul(square);
        }
        if result.binary_exponent().is_some_and(|e| e.abs() > 1 << 16) || square.binary_exponent().is_some_and(|e| e.abs() > 1 << 16) {
            return None;
        }
    }
    Some(result)
}

/// ANNUITY: rate / (1 - (1 + rate)^-periods), or 1 / periods at a zero rate or one too small to
/// move 1 + rate (Language Reference SC27-8713-03, p. 521).
pub fn annuity(rate: Real, periods: u128) -> Option<Real> {
    if rate.is_negative() || periods == 0 {
        return None;
    }
    let discount = match power(Real::ONE.add(rate), periods) {
        Some(p) => Real::ONE.div(p),
        None => Real::ZERO,
    };
    let denominator = Real::ONE.sub(discount);
    if denominator.is_zero() {
        return Some(Real::ONE.div(Real::from_u128(periods)));
    }
    Some(rate.div(denominator))
}

/// PRESENT-VALUE: the sum of each amount / (1 + rate)^n, n from 1 (p. 619).
pub fn present_value(rate: Real, amounts: &[Real]) -> Option<Real> {
    let base = Real::ONE.add(rate);
    if base.is_zero() || base.is_negative() {
        return None;
    }
    let mut factor = Real::ONE;
    let mut sum = Real::ZERO;
    for a in amounts {
        factor = factor.mul(base);
        sum = sum.add(a.div(factor));
    }
    Some(sum)
}

pub fn mean(values: &[Real]) -> Option<Real> {
    let sum = values.iter().fold(Real::ZERO, |s, v| s.add(*v));
    (!values.is_empty()).then(|| sum.div(Real::from_u128(values.len() as u128)))
}

/// The mean of the squared differences from the mean (p. 677).
pub fn variance(values: &[Real]) -> Option<Real> {
    let m = mean(values)?;
    let squares: Vec<Real> = values.iter().map(|v| v.sub(m).mul(v.sub(m))).collect();
    mean(&squares)
}

/// The middle value, or the mean of the two middle values of an even count (p. 595).
pub fn median(values: &[Real]) -> Option<Real> {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.compare(*b));
    let n = sorted.len();
    match n {
        0 => None,
        _ if n % 2 == 1 => Some(sorted[n / 2]),
        _ => Some(sorted[n / 2 - 1].add(sorted[n / 2]).scaled(-1)),
    }
}

pub fn midrange(values: &[Real]) -> Option<Real> {
    let max = values.iter().copied().max_by(|a, b| a.compare(*b))?;
    let min = values.iter().copied().min_by(|a, b| a.compare(*b))?;
    Some(max.add(min).scaled(-1))
}

#[cfg(test)]
mod tests;
