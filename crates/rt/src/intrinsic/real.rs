//! Binary floating point with a 128-bit significand, rounded to nearest with ties away from zero:
//! the working precision of the floating-point intrinsic functions, 16 bits beyond extended HFP.

use std::cmp::Ordering;
use std::ops::{Add, Div, Mul, Neg, Sub};
use zarch::check::ProgramCheck;
use zarch::hfp::{Hfp, Precision};
use zarch::wide::U256;

/// (-1)^negative × significand × 2^exponent, the significand zero or with its top bit set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Real {
    negative: bool,
    significand: u128,
    exponent: i32,
}

const TOP: u128 = 1 << 127;

impl Real {
    pub const ZERO: Self = Self { negative: false, significand: 0, exponent: 0 };
    pub const ONE: Self = Self { negative: false, significand: TOP, exponent: -127 };

    /// magnitude × 2^exponent, rounded to 128 bits.
    pub fn new(negative: bool, magnitude: U256, exponent: i32) -> Self {
        if magnitude.is_zero() {
            return Self::ZERO;
        }
        let bits = magnitude.bits();
        if bits <= 128 {
            let shift = 128 - bits;
            return Self { negative, significand: magnitude.lo << shift, exponent: exponent - shift as i32 };
        }
        let drop = bits - 128;
        let kept = (magnitude >> drop).lo;
        let up = (magnitude >> (drop - 1)).lo & 1;
        match kept.checked_add(up) {
            Some(significand) => Self { negative, significand, exponent: exponent + drop as i32 },
            None => Self { negative, significand: TOP, exponent: exponent + drop as i32 + 1 },
        }
    }

    pub fn from_u128(n: u128) -> Self {
        Self::new(false, U256::from_u128(n), 0)
    }

    pub fn from_i128(n: i128) -> Self {
        Self::new(n < 0, U256::from_u128(n.unsigned_abs()), 0)
    }

    /// Beyond every HFP value, so that storing it is an exponent overflow.
    pub fn huge() -> Self {
        Self { negative: false, significand: TOP, exponent: 1 << 20 }
    }

    /// Exact: an HFP fraction holds at most 112 bits.
    pub fn from_hfp(h: Hfp) -> Self {
        let scale = h.characteristic as i32 - 64 - h.precision.digits() as i32;
        Self::new(h.negative, U256::from_u128(h.fraction), 4 * scale)
    }

    /// The nearest HFP value of `precision`, ties away from zero; below the smallest, zero, as
    /// Language Environment masks exponent underflow (assumption C8).
    pub fn to_hfp(self, precision: Precision) -> Result<Hfp, ProgramCheck> {
        if self.is_zero() {
            return Ok(Hfp::zero(precision));
        }
        let digits = precision.digits() as i32;
        let top = self.exponent + 128;
        let mut hex_exponent = top.div_euclid(4) + (top.rem_euclid(4) != 0) as i32;
        let shift = (4 * (hex_exponent - digits) - self.exponent) as u32;
        let mut fraction = (self.significand >> shift) + ((self.significand >> (shift - 1)) & 1);
        if fraction >> (4 * digits) != 0 {
            fraction >>= 4;
            hex_exponent += 1;
        }
        let characteristic = 64 + hex_exponent;
        if characteristic > 127 {
            return Err(ProgramCheck::HfpExponentOverflow);
        }
        if characteristic < 0 {
            return Ok(Hfp::zero(precision));
        }
        Ok(Hfp { precision, negative: self.negative, characteristic: characteristic as u8, fraction })
    }

    pub fn to_f64(self) -> f64 {
        let v = (self.significand >> 64) as f64 * 2f64.powi(self.exponent + 64);
        if self.negative { -v } else { v }
    }

    pub fn from_f64(x: f64) -> Self {
        if x == 0.0 || !x.is_finite() {
            return Self::ZERO;
        }
        let bits = x.to_bits();
        let biased = ((bits >> 52) & 0x7ff) as i32;
        let mantissa = (bits & ((1 << 52) - 1)) | if biased == 0 { 0 } else { 1 << 52 };
        Self::new(x < 0.0, U256::from_u128(mantissa as u128), biased.max(1) - 1075)
    }

    pub fn is_zero(self) -> bool {
        self.significand == 0
    }

    pub fn is_negative(self) -> bool {
        self.negative && !self.is_zero()
    }

    pub fn abs(self) -> Self {
        Self { negative: false, ..self }
    }

    /// self × 2^by.
    pub fn scaled(self, by: i32) -> Self {
        if self.is_zero() { self } else { Self { exponent: self.exponent + by, ..self } }
    }

    /// The exponent e with 2^e <= |self| < 2^(e+1); `None` for zero.
    pub fn binary_exponent(self) -> Option<i32> {
        (!self.is_zero()).then_some(self.exponent + 127)
    }

    fn cmp_magnitude(self, other: Self) -> Ordering {
        match (self.is_zero(), other.is_zero()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => self.exponent.cmp(&other.exponent).then(self.significand.cmp(&other.significand)),
        }
    }

    pub fn compare(self, other: Self) -> Ordering {
        match (self.is_negative(), other.is_negative()) {
            (false, false) => self.cmp_magnitude(other),
            (true, true) => other.cmp_magnitude(self),
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
        }
    }

    pub fn sqrt(self) -> Self {
        if self.is_zero() {
            return Self::ZERO;
        }
        let mut x = match self.binary_exponent() {
            Some(e) => Self::from_f64((self.scaled(-(e & !1)).to_f64()).sqrt()).scaled(e >> 1),
            None => Self::ZERO,
        };
        for _ in 0..3 {
            x = x.add(self.div(x)).scaled(-1);
        }
        x
    }

    /// The nearest integer, ties away from zero, or `None` beyond 2^100.
    pub fn round_to_integer(self) -> Option<i128> {
        let Some(e) = self.binary_exponent() else { return Some(0) };
        if e > 100 {
            return None;
        }
        let shift = (-self.exponent) as u32;
        let magnitude = match shift {
            0..=127 => (self.significand >> shift) + ((self.significand >> (shift - 1)) & 1),
            128 => 1,
            _ => 0,
        };
        let m = magnitude as i128;
        Some(if self.negative { -m } else { m })
    }

    /// The significand and exponent: self = ±significand × 2^exponent.
    pub fn parts(self) -> (u128, i32) {
        (self.significand, self.exponent)
    }
}

impl Add for Real {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        if self.is_zero() {
            return other;
        }
        if other.is_zero() {
            return self;
        }
        let (big, small) = if self.cmp_magnitude(other) == Ordering::Less { (other, self) } else { (self, other) };
        let shift = (big.exponent - small.exponent) as u32 + 1;
        let a = U256 { hi: big.significand, lo: 0 } >> 1;
        let b = if shift >= 256 { U256::ZERO } else { U256 { hi: small.significand, lo: 0 } >> shift };
        let magnitude = if big.negative == small.negative { a + b } else { a - b };
        Self::new(big.negative, magnitude, big.exponent - 127)
    }
}

impl Sub for Real {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        self + -other
    }
}

impl Mul for Real {
    type Output = Self;

    fn mul(self, other: Self) -> Self {
        if self.is_zero() || other.is_zero() {
            return Self::ZERO;
        }
        Self::new(self.negative != other.negative, U256::widening_mul(self.significand, other.significand), self.exponent + other.exponent)
    }
}

impl Div for Real {
    type Output = Self;

    /// Panics on a zero divisor: callers test for it, since each function names its own error.
    fn div(self, divisor: Self) -> Self {
        assert!(!divisor.is_zero(), "Real division by zero");
        if self.is_zero() {
            return Self::ZERO;
        }
        let d = U256::from_u128(divisor.significand);
        let (mut q, r) = U256 { hi: self.significand, lo: 0 }.div_rem(d);
        let mut exponent = self.exponent - divisor.exponent - 128;
        if q.bits() == 128 {
            q = (q << 1) + U256::from_u128((r << 1 >= d) as u128);
            exponent -= 1;
        }
        Self::new(self.negative != divisor.negative, q, exponent)
    }
}

impl Neg for Real {
    type Output = Self;

    fn neg(self) -> Self {
        if self.is_zero() { self } else { Self { negative: !self.negative, ..self } }
    }
}
