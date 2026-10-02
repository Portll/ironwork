//! Hexadecimal floating point: COMP-1 is short, COMP-2 long, and extended carries ARITH(EXTEND)
//! intermediates. Value = (-1)^sign × 0.fraction × 16^(characteristic − 64). Arithmetic keeps one
//! guard digit and truncates; only LOAD ROUNDED rounds.

use crate::check::{Cc, ProgramCheck, ProgramMask};
use crate::wide::{signed_i128, U256};
use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Precision {
    Short,
    Long,
    Extended,
}

impl Precision {
    pub const fn digits(self) -> u32 {
        match self {
            Self::Short => 6,
            Self::Long => 14,
            Self::Extended => 28,
        }
    }

    pub const fn bytes(self) -> usize {
        match self {
            Self::Short => 4,
            Self::Long => 8,
            Self::Extended => 16,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Hfp {
    pub precision: Precision,
    pub negative: bool,
    pub characteristic: u8,
    pub fraction: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rounding {
    TowardZero,
    HalfAwayFromZero,
}

const FRACTION_56: u128 = (1 << 56) - 1;

const fn digit_bits(n: u32) -> u32 {
    4 * n
}

fn normalize(mut fraction: u128, mut expo: i32, digits: u32) -> (u128, i32) {
    debug_assert!(fraction != 0);
    while fraction >> digit_bits(digits - 1) == 0 {
        fraction <<= 4;
        expo -= 1;
    }
    (fraction, expo)
}

fn sign_order(negative: bool, fraction: u128) -> Ordering {
    match (fraction == 0, negative) {
        (true, _) => Ordering::Equal,
        (false, true) => Ordering::Less,
        (false, false) => Ordering::Greater,
    }
}

fn word(bytes: &[u8]) -> u128 {
    bytes.iter().fold(0, |acc, &b| acc << 8 | b as u128)
}

impl Hfp {
    pub const fn zero(precision: Precision) -> Self {
        Self { precision, negative: false, characteristic: 0, fraction: 0 }
    }

    pub fn from_bytes(precision: Precision, bytes: &[u8]) -> Self {
        assert_eq!(bytes.len(), precision.bytes(), "{precision:?} is {} bytes", precision.bytes());
        let fraction = match precision {
            Precision::Short => word(bytes) & 0xFF_FFFF,
            Precision::Long => word(bytes) & FRACTION_56,
            Precision::Extended => (word(&bytes[..8]) & FRACTION_56) << 56 | (word(&bytes[8..]) & FRACTION_56),
        };
        Self { precision, negative: bytes[0] & 0x80 != 0, characteristic: bytes[0] & 0x7F, fraction }
    }

    /// Storage image. An extended value's low-order half repeats the sign and carries a
    /// characteristic 14 less, modulo 128, unless every bit of the value is zero.
    pub fn to_bytes(self) -> Vec<u8> {
        let lead = (self.negative as u8) << 7 | self.characteristic;
        match self.precision {
            Precision::Short => ((lead as u32) << 24 | self.fraction as u32).to_be_bytes().to_vec(),
            Precision::Long => ((lead as u64) << 56 | self.fraction as u64).to_be_bytes().to_vec(),
            Precision::Extended => {
                let high = (lead as u64) << 56 | (self.fraction >> 56) as u64;
                let mut low = (self.fraction & FRACTION_56) as u64;
                if high != 0 || low != 0 {
                    let low_char = self.characteristic.wrapping_sub(14) & 0x7F;
                    low |= ((self.negative as u64) << 7 | low_char as u64) << 56;
                }
                [high.to_be_bytes(), low.to_be_bytes()].concat()
            }
        }
    }

    fn is_true_zero(self) -> bool {
        self.fraction == 0 && self.characteristic == 0
    }

    fn digits(self) -> u32 {
        self.precision.digits()
    }

    fn finish(precision: Precision, negative: bool, expo: i32, fraction: u128, mask: ProgramMask) -> Result<Self, ProgramCheck> {
        if expo > 127 {
            return Err(ProgramCheck::HfpExponentOverflow);
        }
        if expo < 0 {
            return if mask.hfp_exponent_underflow { Err(ProgramCheck::HfpExponentUnderflow) } else { Ok(Self::zero(precision)) };
        }
        Ok(Self { precision, negative, characteristic: expo as u8, fraction })
    }

    fn significance(precision: Precision, mask: ProgramMask) -> Result<Self, ProgramCheck> {
        if mask.hfp_significance { Err(ProgramCheck::HfpSignificance) } else { Ok(Self::zero(precision)) }
    }

    fn same_precision(self, other: Self) -> Precision {
        assert_eq!(self.precision, other.precision, "HFP operands of different precisions");
        self.precision
    }

    /// Both fractions with a guard digit, the one with the smaller exponent shifted right to align:
    /// the sign and magnitude of the sum, and the common exponent.
    fn aligned_sum(self, other: Self) -> (bool, u128, i32) {
        let (ea, eb) = (self.characteristic as i32, other.characteristic as i32);
        let align = |f: u128, by: i32| if by >= 32 { 0 } else { (f << 4) >> digit_bits(by as u32) };
        let (fa, fb) = (align(self.fraction, (eb - ea).max(0)), align(other.fraction, (ea - eb).max(0)));
        let expo = ea.max(eb);
        match (self.negative == other.negative, fa.cmp(&fb)) {
            (true, _) => (self.negative, fa + fb, expo),
            (false, Ordering::Less) => (other.negative, fb - fa, expo),
            (false, _) => (self.negative, fa - fb, expo),
        }
    }

    fn add_general(self, other: Self, normalized: bool, mask: ProgramMask) -> Result<Self, ProgramCheck> {
        let p = self.same_precision(other);
        let d = p.digits();
        let lone = match (self.is_true_zero(), other.is_true_zero()) {
            (_, true) => Some(self),
            (true, false) => Some(other),
            (false, false) => None,
        };
        if let Some(v) = lone {
            if v.fraction == 0 {
                return Self::significance(p, mask);
            }
            if !normalized {
                return Ok(v);
            }
            let (f, e) = normalize(v.fraction, v.characteristic as i32, d);
            return Self::finish(p, v.negative, e, f, mask);
        }
        let (negative, sum, expo) = self.aligned_sum(other);
        if sum == 0 {
            return Self::significance(p, mask);
        }
        if sum >> digit_bits(d + 1) != 0 {
            return Self::finish(p, negative, expo + 1, sum >> 8, mask);
        }
        if !normalized {
            return match sum >> 4 {
                0 => Self::significance(p, mask),
                f => Self::finish(p, negative, expo, f, mask),
            };
        }
        if sum >> digit_bits(d) != 0 {
            return Self::finish(p, negative, expo, sum >> 4, mask);
        }
        let (f, e) = normalize(sum, expo - 1, d);
        Self::finish(p, negative, e, f, mask)
    }

    pub fn add(self, other: Self, mask: ProgramMask) -> Result<Self, ProgramCheck> {
        self.add_general(other, true, mask)
    }

    pub fn sub(self, other: Self, mask: ProgramMask) -> Result<Self, ProgramCheck> {
        self.add_general(Self { negative: !other.negative, ..other }, true, mask)
    }

    pub fn add_unnormalized(self, other: Self, mask: ProgramMask) -> Result<Self, ProgramCheck> {
        self.add_general(other, false, mask)
    }

    /// MULTIPLY with the product in `target` precision: MEER short to short, MDER short to long,
    /// MDR long to long, MXDR long to extended, MXR extended to extended.
    pub fn mul(self, other: Self, target: Precision, mask: ProgramMask) -> Result<Self, ProgramCheck> {
        let d = self.same_precision(other).digits();
        assert!(target.digits() >= d, "a product is never narrower than its operands");
        if self.fraction == 0 || other.fraction == 0 {
            return Ok(Self::zero(target));
        }
        let (fa, ea) = normalize(self.fraction, self.characteristic as i32, d);
        let (fb, eb) = normalize(other.fraction, other.characteristic as i32, d);
        let product = U256::widening_mul(fa, fb);
        let lead_zero = (product >> digit_bits(2 * d - 1)).is_zero();
        let available = 2 * d - lead_zero as u32;
        let want = target.digits();
        let fraction = if available >= want {
            (product >> digit_bits(available - want)).lo
        } else {
            product.lo << digit_bits(want - available)
        };
        Self::finish(target, self.negative != other.negative, ea + eb - 64 - lead_zero as i32, fraction, mask)
    }

    pub fn div(self, divisor: Self, mask: ProgramMask) -> Result<Self, ProgramCheck> {
        let p = self.same_precision(divisor);
        let d = p.digits();
        if divisor.fraction == 0 {
            return Err(ProgramCheck::HfpDivide);
        }
        if self.fraction == 0 {
            return Ok(Self::zero(p));
        }
        let (fa, ea) = normalize(self.fraction, self.characteristic as i32, d);
        let (fb, eb) = normalize(divisor.fraction, divisor.characteristic as i32, d);
        let (fb, expo) = if fa < fb { (fb, ea - eb + 64) } else { (fb << 4, ea - eb + 65) };
        let (mut rem, mut quotient) = (fa, 0u128);
        for _ in 0..d {
            rem <<= 4;
            quotient = (quotient << 4) | (rem / fb);
            rem %= fb;
        }
        Self::finish(p, self.negative != divisor.negative, expo, quotient, mask)
    }

    /// COMPARE: algebraic, through the same guard-digit alignment as subtraction, so digits shifted
    /// past the guard digit take no part.
    pub fn compare(self, other: Self) -> Ordering {
        self.same_precision(other);
        match (self.is_true_zero(), other.is_true_zero()) {
            (_, true) => sign_order(self.negative, self.fraction),
            (true, false) => sign_order(other.negative, other.fraction).reverse(),
            (false, false) => {
                let (negative, diff, _) = self.aligned_sum(Self { negative: !other.negative, ..other });
                sign_order(negative, diff)
            }
        }
    }

    /// The condition code an arithmetic result sets: zero, negative or positive.
    pub fn sign_cc(self) -> Cc {
        sign_order(self.negative, self.fraction).into()
    }

    /// HALVE: the fraction shifted right one bit through a guard digit, then normalized.
    pub fn halve(self, mask: ProgramMask) -> Result<Self, ProgramCheck> {
        let d = self.digits();
        if self.fraction == 0 {
            return Ok(Self::zero(self.precision));
        }
        let wide = self.fraction << 3;
        let (fraction, expo) = if wide >> digit_bits(d) != 0 { (wide >> 4, self.characteristic as i32) } else { (wide, self.characteristic as i32 - 1) };
        let (fraction, expo) = normalize(fraction, expo, d);
        Self::finish(self.precision, self.negative, expo, fraction, mask)
    }

    /// LOAD LENGTHENED: exact.
    pub fn lengthen(self, target: Precision) -> Self {
        assert!(target.digits() >= self.digits());
        Self { precision: target, fraction: self.fraction << digit_bits(target.digits() - self.digits()), ..self }
    }

    /// The high-order part of a longer value, as storing its leading bytes leaves it.
    pub fn truncate(self, target: Precision) -> Self {
        assert!(target.digits() <= self.digits());
        Self { precision: target, fraction: self.fraction >> digit_bits(self.digits() - target.digits()), ..self }
    }

    /// LOAD ROUNDED: adds one at the first bit dropped, then truncates.
    pub fn round(self, target: Precision) -> Result<Self, ProgramCheck> {
        assert!(target.digits() < self.digits());
        let dropped = digit_bits(self.digits() - target.digits());
        let fraction = (self.fraction >> dropped) + (self.fraction >> (dropped - 1) & 1);
        if fraction >> digit_bits(target.digits()) != 0 {
            return Self::finish(target, self.negative, self.characteristic as i32 + 1, fraction >> 4, ProgramMask::default());
        }
        Ok(Self { precision: target, fraction, ..self })
    }

    /// CONVERT FROM FIXED: normalized, truncating digits the precision cannot hold.
    pub fn from_integer(value: i128, precision: Precision) -> Self {
        let magnitude = value.unsigned_abs();
        if magnitude == 0 {
            return Self::zero(precision);
        }
        let hex_digits = (128 - magnitude.leading_zeros()).div_ceil(4);
        let d = precision.digits();
        let fraction = if hex_digits <= d { magnitude << digit_bits(d - hex_digits) } else { magnitude >> digit_bits(hex_digits - d) };
        Self { precision, negative: value < 0, characteristic: 64 + hex_digits as u8, fraction }
    }

    /// Exponent of the fraction's last digit: value = fraction × 16^scale.
    fn scale(self) -> i32 {
        self.characteristic as i32 - 64 - self.digits() as i32
    }

    /// The value × 10^decimals as a sign and integer magnitude, or `None` past 256 bits.
    pub fn to_scaled_integer(self, decimals: u32, rounding: Rounding) -> Option<(bool, U256)> {
        let numerator = U256::from_u128(self.fraction).checked_mul(U256::pow10(decimals))?;
        let scale = self.scale();
        let magnitude = if scale >= 0 {
            let shift = digit_bits(scale as u32);
            match numerator.is_zero() {
                true => U256::ZERO,
                false if numerator.bits() + shift > 256 => return None,
                false => numerator << shift,
            }
        } else {
            let shift = digit_bits(scale.unsigned_abs());
            let half_or_more = (numerator >> (shift - 1)).lo & 1 == 1;
            let kept = numerator >> shift;
            if rounding == Rounding::HalfAwayFromZero && half_or_more { kept + U256::from_u128(1) } else { kept }
        };
        Some((self.negative && !magnitude.is_zero(), magnitude))
    }

    /// The value with its fractional digits dropped, toward zero, at any magnitude.
    pub fn integer_part(self) -> Self {
        let scale = self.scale();
        if scale >= 0 {
            return self;
        }
        let dropped = digit_bits(scale.unsigned_abs());
        let fraction = if dropped >= 128 { 0 } else { self.fraction >> dropped << dropped };
        if fraction == 0 { Self::zero(self.precision) } else { Self { fraction, ..self } }
    }

    /// CONVERT TO FIXED, or `None` when the result does not fit.
    pub fn to_integer(self, rounding: Rounding) -> Option<i128> {
        let (negative, magnitude) = self.to_scaled_integer(0, rounding)?;
        signed_i128(negative, magnitude)
    }

    pub fn approx(self) -> f64 {
        let v = self.fraction as f64 * 16f64.powi(self.scale());
        if self.negative { -v } else { v }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_MASK: ProgramMask =
        ProgramMask { fixed_point_overflow: false, decimal_overflow: false, hfp_exponent_underflow: false, hfp_significance: false };

    fn short(bits: u32) -> Hfp {
        Hfp::from_bytes(Precision::Short, &bits.to_be_bytes())
    }

    fn long(bits: u64) -> Hfp {
        Hfp::from_bytes(Precision::Long, &bits.to_be_bytes())
    }

    fn bits_short(v: Hfp) -> u32 {
        u32::from_be_bytes(v.to_bytes().try_into().unwrap())
    }

    fn bits_long(v: Hfp) -> u64 {
        u64::from_be_bytes(v.to_bytes().try_into().unwrap())
    }

    #[test]
    fn one_third_truncates_and_times_three_is_not_one() {
        let third = long(0x4110_0000_0000_0000).div(long(0x4130_0000_0000_0000), NO_MASK).unwrap();
        assert_eq!(bits_long(third), 0x4055_5555_5555_5555);
        let back = third.mul(long(0x4130_0000_0000_0000), Precision::Long, NO_MASK).unwrap();
        assert_eq!(bits_long(back), 0x40FF_FFFF_FFFF_FFFF);
    }

    #[test]
    fn carry_out_of_the_fraction_bumps_the_characteristic() {
        assert_eq!(bits_short(short(0x4180_0000).add(short(0x4180_0000), NO_MASK).unwrap()), 0x4210_0000);
    }

    #[test]
    fn the_guard_digit_keeps_a_difference_exact() {
        assert_eq!(bits_short(short(0x4110_0000).sub(short(0x40FF_FFFF), NO_MASK).unwrap()), 0x3B10_0000);
    }

    #[test]
    fn equal_operands_subtract_to_a_true_zero() {
        assert_eq!(bits_short(short(0x4110_0000).sub(short(0x4110_0000), NO_MASK).unwrap()), 0);
        let mask = ProgramMask { hfp_significance: true, ..NO_MASK };
        assert_eq!(short(0x4110_0000).sub(short(0x4110_0000), mask), Err(ProgramCheck::HfpSignificance));
    }

    #[test]
    fn an_unnormalized_zero_with_a_large_exponent_swamps_the_other_operand() {
        assert_eq!(bits_short(short(0x4C00_0000).add(short(0x4110_0000), NO_MASK).unwrap()), 0);
    }

    #[test]
    fn underflow_gives_a_true_zero_unless_the_mask_enables_it() {
        let tiny = short(0x0110_0000);
        assert_eq!(bits_short(tiny.mul(tiny, Precision::Short, NO_MASK).unwrap()), 0);
        let mask = ProgramMask { hfp_exponent_underflow: true, ..NO_MASK };
        assert_eq!(tiny.mul(tiny, Precision::Short, mask), Err(ProgramCheck::HfpExponentUnderflow));
    }

    #[test]
    fn halve_shifts_one_bit_and_normalizes() {
        assert_eq!(bits_short(short(0x4110_0000).halve(NO_MASK).unwrap()), 0x4080_0000);
        assert_eq!(bits_short(short(0xC120_0000).halve(NO_MASK).unwrap()), 0xC110_0000);
        assert_eq!(bits_short(short(0x4180_0000).halve(NO_MASK).unwrap()), 0x4140_0000);
        assert_eq!(bits_short(short(0x4100_0000).halve(NO_MASK).unwrap()), 0);
    }

    #[test]
    fn halve_underflow_gives_a_true_zero_unless_the_mask_enables_it() {
        let smallest = short(0x0010_0000);
        assert_eq!(bits_short(smallest.halve(NO_MASK).unwrap()), 0);
        let mask = ProgramMask { hfp_exponent_underflow: true, ..NO_MASK };
        assert_eq!(smallest.halve(mask), Err(ProgramCheck::HfpExponentUnderflow));
        assert_eq!(bits_short(short(0x0120_0000).halve(mask).unwrap()), 0x0110_0000);
    }

    #[test]
    fn overflow_and_divide_by_zero_are_program_checks() {
        let huge = short(0x7F10_0000);
        assert_eq!(huge.mul(huge, Precision::Short, NO_MASK), Err(ProgramCheck::HfpExponentOverflow));
        assert_eq!(huge.div(Hfp::zero(Precision::Short), NO_MASK), Err(ProgramCheck::HfpDivide));
    }

    #[test]
    fn short_operands_multiply_exactly_into_long() {
        let p = short(0x4130_0000).mul(short(0x4130_0000), Precision::Long, NO_MASK).unwrap();
        assert_eq!(bits_long(p), 0x4190_0000_0000_0000);
    }

    #[test]
    fn load_rounded_adds_half_and_carries() {
        assert_eq!(bits_short(long(0x4112_3456_789A_BCDE).round(Precision::Short).unwrap()), 0x4112_3456);
        assert_eq!(bits_short(long(0x4112_3456_8000_0000).round(Precision::Short).unwrap()), 0x4112_3457);
        assert_eq!(bits_short(long(0x41FF_FFFF_8000_0000).round(Precision::Short).unwrap()), 0x4210_0000);
        assert_eq!(bits_short(long(0x4112_3456_8000_0000).truncate(Precision::Short)), 0x4112_3456);
    }

    #[test]
    fn extended_storage_repeats_the_sign_and_offsets_the_low_characteristic() {
        let third = long(0x4110_0000_0000_0000)
            .lengthen(Precision::Extended)
            .div(long(0x4130_0000_0000_0000).lengthen(Precision::Extended), NO_MASK)
            .unwrap();
        let bytes = third.to_bytes();
        assert_eq!(u64::from_be_bytes(bytes[..8].try_into().unwrap()), 0x4055_5555_5555_5555);
        assert_eq!(u64::from_be_bytes(bytes[8..].try_into().unwrap()), 0x3255_5555_5555_5555);
        assert_eq!(Hfp::zero(Precision::Extended).to_bytes(), [0u8; 16]);
        assert_eq!(Hfp::from_bytes(Precision::Extended, &bytes), third);
    }

    #[test]
    fn integers_convert_both_ways() {
        assert_eq!(bits_long(Hfp::from_integer(10, Precision::Long)), 0x41A0_0000_0000_0000);
        assert_eq!(bits_long(Hfp::from_integer(256, Precision::Long)), 0x4310_0000_0000_0000);
        assert_eq!(bits_long(Hfp::from_integer(-1, Precision::Long)), 0xC110_0000_0000_0000);
        let one_and_half = long(0x4118_0000_0000_0000);
        assert_eq!(one_and_half.to_integer(Rounding::TowardZero), Some(1));
        assert_eq!(one_and_half.to_integer(Rounding::HalfAwayFromZero), Some(2));
        assert_eq!(long(0xC118_0000_0000_0000).to_integer(Rounding::HalfAwayFromZero), Some(-2));
    }

    #[test]
    fn compare_is_algebraic_across_representations() {
        assert_eq!(short(0x4110_0000).compare(short(0x4201_0000)), Ordering::Equal);
        assert_eq!(short(0x4110_0000).compare(short(0x40FF_FFFF)), Ordering::Greater);
        assert_eq!(short(0x8000_0000).compare(short(0x0000_0000)), Ordering::Equal);
        assert_eq!(short(0xC110_0000).compare(Hfp::zero(Precision::Short)), Ordering::Less);
    }

    #[test]
    fn scaled_integer_of_one_third() {
        let third = long(0x4055_5555_5555_5555);
        let (negative, m) = third.to_scaled_integer(5, Rounding::TowardZero).unwrap();
        assert!(!negative);
        assert_eq!(m.to_u128(), Some(33333));
        assert!((third.approx() - 1.0 / 3.0).abs() < 1e-15);
    }

    #[test]
    fn no_operation_panics_on_arbitrary_bits() {
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for p in [Precision::Short, Precision::Long, Precision::Extended] {
            for _ in 0..5_000 {
                let a: Vec<u8> = (0..p.bytes()).map(|_| next() as u8).collect();
                let b: Vec<u8> = (0..p.bytes()).map(|_| next() as u8).collect();
                let (a, b) = (Hfp::from_bytes(p, &a), Hfp::from_bytes(p, &b));
                let mask = ProgramMask { hfp_exponent_underflow: next() & 1 == 1, hfp_significance: next() & 1 == 1, ..NO_MASK };
                let _ = a.add(b, mask);
                let _ = a.sub(b, mask);
                let _ = a.add_unnormalized(b, mask);
                let _ = a.mul(b, p, mask);
                let _ = a.div(b, mask);
                let _ = a.compare(b);
                let _ = a.to_integer(Rounding::HalfAwayFromZero);
                let _ = a.to_scaled_integer(31, Rounding::TowardZero);
                if p != Precision::Short {
                    let _ = a.round(Precision::Short);
                }
                assert_eq!(Hfp::from_bytes(p, &a.to_bytes()).fraction, a.fraction);
            }
        }
    }
}
