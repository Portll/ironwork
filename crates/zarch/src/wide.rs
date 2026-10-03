use std::ops::{Add, Shl, Shr, Sub};

/// Unsigned 256-bit integer: wide enough for the exact product of two 31-digit decimals and of two
/// extended-precision HFP fractions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct U256 {
    pub hi: u128,
    pub lo: u128,
}

const LOW64: u128 = u64::MAX as u128;

const fn times_ten(x: U256) -> U256 {
    let low = (x.lo & LOW64) * 10;
    let high = (x.lo >> 64) * 10 + (low >> 64);
    U256 { hi: x.hi * 10 + (high >> 64), lo: (low & LOW64) | ((high & LOW64) << 64) }
}

/// 10^0 to 10^77, the powers of ten below 2^256.
const POW10: [U256; 78] = {
    let mut table = [U256::from_u128(1); 78];
    let mut n = 1;
    while n < table.len() {
        table[n] = times_ten(table[n - 1]);
        n += 1;
    }
    table
};

impl U256 {
    pub const ZERO: Self = Self { hi: 0, lo: 0 };

    #[inline]
    pub const fn from_u128(lo: u128) -> Self {
        Self { hi: 0, lo }
    }

    pub fn widening_mul(a: u128, b: u128) -> Self {
        let (a0, a1, b0, b1) = (a & LOW64, a >> 64, b & LOW64, b >> 64);
        let (p00, p01, p10, p11) = (a0 * b0, a0 * b1, a1 * b0, a1 * b1);
        let mid = (p00 >> 64) + (p01 & LOW64) + (p10 & LOW64);
        Self { hi: p11 + (p01 >> 64) + (p10 >> 64) + (mid >> 64), lo: (p00 & LOW64) | ((mid & LOW64) << 64) }
    }

    #[inline]
    pub fn checked_mul(self, other: Self) -> Option<Self> {
        if (self.hi | other.hi | (self.lo >> 64) | (other.lo >> 64)) == 0 {
            return Some(Self::from_u128(self.lo * other.lo));
        }
        self.checked_mul_wide(other)
    }

    fn checked_mul_wide(self, other: Self) -> Option<Self> {
        if self.hi != 0 && other.hi != 0 {
            return None;
        }
        let low = Self::widening_mul(self.lo, other.lo);
        let cross = self.hi.checked_mul(other.lo)?.checked_add(other.hi.checked_mul(self.lo)?)?;
        Some(Self { hi: low.hi.checked_add(cross)?, lo: low.lo })
    }

    #[inline]
    pub fn checked_add(self, other: Self) -> Option<Self> {
        let (lo, carry) = self.lo.overflowing_add(other.lo);
        Some(Self { hi: self.hi.checked_add(other.hi)?.checked_add(carry as u128)?, lo })
    }

    #[inline]
    pub fn to_u128(self) -> Option<u128> {
        (self.hi == 0).then_some(self.lo)
    }

    #[inline]
    pub fn is_zero(self) -> bool {
        self == Self::ZERO
    }

    pub fn bits(self) -> u32 {
        if self.hi != 0 { 256 - self.hi.leading_zeros() } else { 128 - self.lo.leading_zeros() }
    }

    #[inline]
    pub fn pow10(n: u32) -> Self {
        *POW10.get(n as usize).expect("10^n beyond 256 bits")
    }

    #[inline]
    pub fn div_rem(self, divisor: Self) -> (Self, Self) {
        if (self.hi | divisor.hi) == 0 && (self.lo | divisor.lo) <= LOW64 && divisor.lo != 0 {
            let (a, d) = (self.lo as u64, divisor.lo as u64);
            return (Self::from_u128(u128::from(a / d)), Self::from_u128(u128::from(a % d)));
        }
        self.div_rem_wide(divisor)
    }

    fn div_rem_wide(self, divisor: Self) -> (Self, Self) {
        assert!(!divisor.is_zero(), "division by zero");
        if self < divisor {
            return (Self::ZERO, self);
        }
        if self.hi == 0 {
            return (Self::from_u128(self.lo / divisor.lo), Self::from_u128(self.lo % divisor.lo));
        }
        if divisor.hi == 0 && divisor.lo <= LOW64 {
            return self.div_rem_short(divisor.lo);
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

    /// Schoolbook division by a divisor below 2^64, a 64-bit limb at a time, most significant first.
    fn div_rem_short(self, divisor: u128) -> (Self, Self) {
        let mut rem = 0u128;
        let mut limbs = [self.hi >> 64, self.hi & LOW64, self.lo >> 64, self.lo & LOW64];
        for limb in &mut limbs {
            let current = (rem << 64) | *limb;
            *limb = current / divisor;
            rem = current % divisor;
        }
        (Self { hi: (limbs[0] << 64) | limbs[1], lo: (limbs[2] << 64) | limbs[3] }, Self::from_u128(rem))
    }
}

impl Add for U256 {
    type Output = Self;
    #[inline]
    fn add(self, other: Self) -> Self {
        self.checked_add(other).expect("U256 overflow")
    }
}

impl Sub for U256 {
    type Output = Self;
    #[inline]
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

    fn bitwise_div_rem(a: U256, d: U256) -> (U256, U256) {
        let (mut quotient, mut rem) = (U256::ZERO, U256::ZERO);
        for bit in (0..a.bits()).rev() {
            rem = rem << 1;
            rem.lo |= (a >> bit).lo & 1;
            quotient = quotient << 1;
            if rem >= d {
                rem = rem - d;
                quotient.lo |= 1;
            }
        }
        (quotient, rem)
    }

    fn widening_checked_mul(a: U256, b: U256) -> Option<U256> {
        if a.hi != 0 && b.hi != 0 {
            return None;
        }
        let low = U256::widening_mul(a.lo, b.lo);
        let cross = a.hi.checked_mul(b.lo)?.checked_add(b.hi.checked_mul(a.lo)?)?;
        Some(U256 { hi: low.hi.checked_add(cross)?, lo: low.lo })
    }

    /// Values of every width, from xorshift64*, each kept to a random number of low bits.
    fn samples(count: usize) -> Vec<U256> {
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            u128::from(state.wrapping_mul(0x2545_F491_4F6C_DD1D))
        };
        (0..count)
            .map(|_| {
                let full = U256 { hi: next() << 64 | next(), lo: next() << 64 | next() };
                let bits = (next() % 257) as u32;
                if bits == 0 { U256::ZERO } else { (full << (256 - bits)) >> (256 - bits) }
            })
            .chain((0..78).map(U256::pow10))
            .chain([U256::from_u128(u128::MAX), U256 { hi: 1, lo: 0 }, U256 { hi: u128::MAX, lo: u128::MAX }, U256::from_u128(LOW64), U256::from_u128(LOW64 + 1)])
            .collect()
    }

    #[test]
    fn pow10_is_ten_multiplied_n_times() {
        let mut p = U256::from_u128(1);
        for n in 0..78 {
            assert_eq!(U256::pow10(n), p, "10^{n}");
            p = widening_checked_mul(p, U256::from_u128(10)).unwrap_or(U256::ZERO);
        }
    }

    #[test]
    #[should_panic(expected = "10^n beyond 256 bits")]
    fn pow10_refuses_78() {
        U256::pow10(78);
    }

    #[test]
    fn div_rem_and_checked_mul_agree_with_the_bitwise_forms() {
        let values = samples(600);
        for &a in &values {
            for &d in values.iter().step_by(7) {
                if !d.is_zero() {
                    assert_eq!(a.div_rem(d), bitwise_div_rem(a, d), "{a:?} / {d:?}");
                }
                assert_eq!(a.checked_mul(d), widening_checked_mul(a, d), "{a:?} * {d:?}");
            }
        }
    }

    #[test]
    fn shifts_cross_the_halves() {
        let one = U256::from_u128(1);
        assert_eq!(one << 130, U256 { hi: 4, lo: 0 });
        assert_eq!((one << 130) >> 129, U256::from_u128(2));
    }
}
