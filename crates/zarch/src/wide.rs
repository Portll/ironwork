use std::ops::{Add, Shl, Shr, Sub};

/// Unsigned 256-bit integer: wide enough for the exact product of two 31-digit decimals and of two
/// extended-precision HFP fractions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct U256 {
    pub hi: u128,
    pub lo: u128,
}

const LOW64: u128 = u64::MAX as u128;

impl U256 {
    pub const ZERO: Self = Self { hi: 0, lo: 0 };

    pub const fn from_u128(lo: u128) -> Self {
        Self { hi: 0, lo }
    }

    pub fn widening_mul(a: u128, b: u128) -> Self {
        let (a0, a1, b0, b1) = (a & LOW64, a >> 64, b & LOW64, b >> 64);
        let (p00, p01, p10, p11) = (a0 * b0, a0 * b1, a1 * b0, a1 * b1);
        let mid = (p00 >> 64) + (p01 & LOW64) + (p10 & LOW64);
        Self { hi: p11 + (p01 >> 64) + (p10 >> 64) + (mid >> 64), lo: (p00 & LOW64) | ((mid & LOW64) << 64) }
    }

    pub fn checked_mul(self, other: Self) -> Option<Self> {
        if self.hi != 0 && other.hi != 0 {
            return None;
        }
        let low = Self::widening_mul(self.lo, other.lo);
        let cross = self.hi.checked_mul(other.lo)?.checked_add(other.hi.checked_mul(self.lo)?)?;
        Some(Self { hi: low.hi.checked_add(cross)?, lo: low.lo })
    }

    pub fn checked_add(self, other: Self) -> Option<Self> {
        let (lo, carry) = self.lo.overflowing_add(other.lo);
        Some(Self { hi: self.hi.checked_add(other.hi)?.checked_add(carry as u128)?, lo })
    }

    pub fn to_u128(self) -> Option<u128> {
        (self.hi == 0).then_some(self.lo)
    }

    pub fn is_zero(self) -> bool {
        self == Self::ZERO
    }

    pub fn bits(self) -> u32 {
        if self.hi != 0 { 256 - self.hi.leading_zeros() } else { 128 - self.lo.leading_zeros() }
    }

    pub fn pow10(n: u32) -> Self {
        (0..n).fold(Self::from_u128(1), |acc, _| acc.checked_mul(Self::from_u128(10)).expect("10^n beyond 256 bits"))
    }

    pub fn div_rem(self, divisor: Self) -> (Self, Self) {
        assert!(!divisor.is_zero(), "division by zero");
        if self < divisor {
            return (Self::ZERO, self);
        }
        let mut quotient = Self::ZERO;
        let mut rem = Self::ZERO;
        for bit in (0..self.bits()).rev() {
            rem = rem << 1;
            rem.lo |= (self >> bit).lo & 1;
            quotient = quotient << 1;
            if rem >= divisor {
                rem = rem - divisor;
                quotient.lo |= 1;
            }
        }
        (quotient, rem)
    }
}

impl Add for U256 {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        self.checked_add(other).expect("U256 overflow")
    }
}

impl Sub for U256 {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        let (lo, borrow) = self.lo.overflowing_sub(other.lo);
        Self { hi: self.hi - other.hi - borrow as u128, lo }
    }
}

impl Shl<u32> for U256 {
    type Output = Self;
    fn shl(self, n: u32) -> Self {
        match n {
            0 => self,
            1..128 => Self { hi: (self.hi << n) | (self.lo >> (128 - n)), lo: self.lo << n },
            128..256 => Self { hi: self.lo << (n - 128), lo: 0 },
            _ => Self::ZERO,
        }
    }
}

impl Shr<u32> for U256 {
    type Output = Self;
    fn shr(self, n: u32) -> Self {
        match n {
            0 => self,
            1..128 => Self { hi: self.hi >> n, lo: (self.lo >> n) | (self.hi << (128 - n)) },
            128..256 => Self { hi: 0, lo: self.hi >> (n - 128) },
            _ => Self::ZERO,
        }
    }
}

/// A sign and magnitude as an `i128`, or `None` when the magnitude does not fit.
pub fn signed_i128(negative: bool, magnitude: U256) -> Option<i128> {
    let m = i128::try_from(magnitude.to_u128()?).ok()?;
    Some(if negative { -m } else { m })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widening_mul_of_max_values() {
        let p = U256::widening_mul(u128::MAX, u128::MAX);
        assert_eq!(p, U256 { hi: u128::MAX - 1, lo: 1 });
    }

    #[test]
    fn div_rem_recovers_the_operands() {
        let a = U256::pow10(62) + U256::from_u128(12345);
        let d = U256::pow10(31) + U256::from_u128(7);
        let (q, r) = a.div_rem(d);
        assert!(r < d);
        assert_eq!(q.checked_mul(d).unwrap() + r, a);
    }

    #[test]
    fn shifts_cross_the_halves() {
        let one = U256::from_u128(1);
        assert_eq!(one << 130, U256 { hi: 4, lo: 0 });
        assert_eq!((one << 130) >> 129, U256::from_u128(2));
    }
}
