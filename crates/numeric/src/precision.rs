//! Fixed-point intermediate results: how many integer and decimal places IBM carries for each
//! operation, and exact arithmetic that drops exactly the digits it drops. See
//! [`crate::assumptions::INTERMEDIATE_TABLE`].

use crate::options::{Arith, ExtraPlace};
use std::fmt;
use zarch::wide::U256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Places {
    pub int: u32,
    pub dec: u32,
}

impl Places {
    #[inline]
    pub const fn new(int: u32, dec: u32) -> Self {
        Self { int, dec }
    }

    #[inline]
    pub const fn total(self) -> u32 {
        self.int + self.dec
    }
}

#[inline]
pub fn sum_places(a: Places, b: Places) -> Places {
    Places::new(a.int.max(b.int) + 1, a.dec.max(b.dec))
}

#[inline]
pub fn product_places(a: Places, b: Places) -> Places {
    Places::new(a.int + b.int, a.dec + b.dec)
}

pub fn quotient_places(dividend: Places, divisor: Places, dmax: u32) -> Places {
    Places::new(dividend.int + divisor.dec, dividend.dec.saturating_sub(divisor.dec).max(dmax))
}

/// A statement's dmax for its last operation, the one whose result the receivers take, and for
/// every operation below it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Dmax {
    pub last: u32,
    pub inner: u32,
}

impl Dmax {
    /// The decimal places a receiver of `scale` counts for: under ROUNDED one more, the digit
    /// rounding reads, in every operation, in the last alone, or in none, as `place` says. See
    /// [`crate::assumptions::ROUNDED_EXTRA_PLACE`].
    pub const fn receiver(scale: u32, rounded: bool, place: ExtraPlace) -> Self {
        let last = scale + rounded as u32;
        match place {
            ExtraPlace::Every => Self { last, inner: last },
            ExtraPlace::Last => Self { last, inner: scale },
            ExtraPlace::Off => Self { last: scale, inner: scale },
        }
    }

    pub fn max(self, other: Self) -> Self {
        Self { last: self.last.max(other.last), inner: self.inner.max(other.inner) }
    }

    /// Raised to the decimal places of an operand, which count in every operation.
    pub fn with(self, places: u32) -> Self {
        Self { last: self.last.max(places), inner: self.inner.max(places) }
    }
}

/// The places carried for an intermediate result `ir`. `dmax` is the most decimal places among
/// the statement's receivers and its operands other than divisors and exponents.
pub fn carried(ir: Places, dmax: u32, arith: Arith) -> Places {
    let n = arith.intermediate_digits();
    if ir.total() <= n {
        ir
    } else if ir.dec <= dmax {
        Places::new(n.saturating_sub(ir.dec), ir.dec)
    } else if ir.int + dmax <= n {
        Places::new(ir.int, n - ir.int)
    } else {
        Places::new(n.saturating_sub(dmax), dmax)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArithError {
    DivideByZero,
    BeyondModel,
}

impl fmt::Display for ArithError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DivideByZero => write!(f, "division by zero"),
            Self::BeyondModel => write!(f, "an intermediate wider than 256 bits"),
        }
    }
}

impl std::error::Error for ArithError {}

/// A fixed-point value: magnitude × 10^-places.dec, with the places it is declared to hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fixed {
    pub negative: bool,
    pub magnitude: U256,
    pub places: Places,
}

#[inline]
fn pow10(n: u32) -> U256 {
    U256::pow10(n)
}

impl Fixed {
    #[inline]
    pub fn new(value: i128, places: Places) -> Self {
        Self::signed(value < 0, U256::from_u128(value.unsigned_abs()), places)
    }

    #[inline]
    fn signed(negative: bool, magnitude: U256, places: Places) -> Self {
        Self { negative: negative && !magnitude.is_zero(), magnitude, places }
    }

    pub fn to_i128(self) -> Option<i128> {
        zarch::wide::signed_i128(self.negative, self.magnitude)
    }

    /// Keeps `to.dec` decimal places, truncating the rest, and `to.int` integer places, dropping
    /// high-order digits.
    pub fn fit(self, to: Places) -> Self {
        let from = self.places.dec;
        let magnitude = if to.dec < from {
            (self.magnitude.div_rem(pow10(from - to.dec)).0).div_rem(pow10(to.total())).1
        } else {
            self.magnitude.div_rem(pow10(to.int + from)).1.checked_mul(pow10(to.dec - from)).expect("fits by construction")
        };
        Self::signed(self.negative, magnitude, to)
    }

    fn aligned(self, dec: u32) -> Result<U256, ArithError> {
        self.magnitude.checked_mul(pow10(dec - self.places.dec)).ok_or(ArithError::BeyondModel)
    }

    fn negated(self) -> Self {
        Self::signed(!self.negative, self.magnitude, self.places)
    }

    pub fn add(self, other: Self, dmax: u32, arith: Arith) -> Result<Self, ArithError> {
        let dec = self.places.dec.max(other.places.dec);
        let (a, b) = (self.aligned(dec)?, other.aligned(dec)?);
        let (negative, magnitude) = match (self.negative == other.negative, a >= b) {
            (true, _) => (self.negative, a.checked_add(b).ok_or(ArithError::BeyondModel)?),
            (false, true) => (self.negative, a - b),
            (false, false) => (other.negative, b - a),
        };
        let ir = sum_places(self.places, other.places);
        Ok(Self::signed(negative, magnitude, Places::new(ir.int, dec)).fit(carried(ir, dmax, arith)))
    }

    pub fn sub(self, other: Self, dmax: u32, arith: Arith) -> Result<Self, ArithError> {
        self.add(other.negated(), dmax, arith)
    }

    pub fn mul(self, other: Self, dmax: u32, arith: Arith) -> Result<Self, ArithError> {
        let ir = product_places(self.places, other.places);
        let magnitude = self.magnitude.checked_mul(other.magnitude).ok_or(ArithError::BeyondModel)?;
        Ok(Self::signed(self.negative != other.negative, magnitude, ir).fit(carried(ir, dmax, arith)))
    }

    pub fn div(self, divisor: Self, dmax: u32, arith: Arith) -> Result<Self, ArithError> {
        if divisor.magnitude.is_zero() {
            return Err(ArithError::DivideByZero);
        }
        let to = carried(quotient_places(self.places, divisor.places, dmax), dmax, arith);
        let shift = (divisor.places.dec + to.dec) as i64 - self.places.dec as i64;
        let (numerator, denominator) = if shift >= 0 {
            (self.magnitude.checked_mul(pow10(shift as u32)), Some(divisor.magnitude))
        } else {
            (Some(self.magnitude), divisor.magnitude.checked_mul(pow10(shift.unsigned_abs() as u32)))
        };
        let (numerator, denominator) = (numerator.ok_or(ArithError::BeyondModel)?, denominator.ok_or(ArithError::BeyondModel)?);
        let quotient = numerator.div_rem(denominator).0;
        let exact = Self::signed(self.negative != divisor.negative, quotient, Places::new(u32::MAX / 2, to.dec));
        Ok(exact.fit(to))
    }

    /// The value a receiver of `places` gets: truncated, or rounded half away from zero, and whether
    /// the integer part overflowed it (ON SIZE ERROR).
    pub fn to_receiver(self, places: Places, rounded: bool) -> (Self, bool) {
        let from = self.places.dec;
        let magnitude = if places.dec >= from {
            self.magnitude.checked_mul(pow10(places.dec - from))
        } else {
            let (kept, dropped) = self.magnitude.div_rem(pow10(from - places.dec));
            let half = pow10(from - places.dec - 1).checked_mul(U256::from_u128(5)).unwrap();
            Some(if rounded && dropped >= half { kept + U256::from_u128(1) } else { kept })
        };
        let cap = pow10(places.total());
        match magnitude {
            Some(m) if m < cap => (Self::signed(self.negative, m, places), false),
            Some(m) => (Self::signed(self.negative, m.div_rem(cap).1, places), true),
            None => (Self::signed(self.negative, U256::ZERO, places), true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S18: Places = Places::new(18, 0);

    #[test]
    fn places_for_each_operation() {
        assert_eq!(sum_places(Places::new(5, 2), Places::new(3, 4)), Places::new(6, 4));
        assert_eq!(product_places(Places::new(5, 2), Places::new(3, 4)), Places::new(8, 6));
        assert_eq!(quotient_places(Places::new(5, 2), Places::new(3, 4), 3), Places::new(9, 3));
    }

    /// Programming Guide SC27-8714-03, p. 795: a quotient's decimal places are d2 - d1, the
    /// dividend's less the divisor's, or dmax, whichever is greater.
    #[test]
    fn a_quotient_carries_the_dividend_s_places_less_the_divisor_s_or_dmax() {
        assert_eq!(quotient_places(Places::new(2, 4), Places::new(1, 1), 2), Places::new(3, 3));
        assert_eq!(quotient_places(Places::new(2, 4), Places::new(1, 3), 2), Places::new(5, 2));
        let product = Fixed::new(12321, Places::new(2, 4));
        let q = product.div(Fixed::new(7, Places::new(1, 1)), 2, Arith::Compat).unwrap();
        assert_eq!((q.to_i128(), q.places.dec), (Some(1760), 3));
    }

    #[test]
    fn the_carried_places_table() {
        assert_eq!(carried(Places::new(20, 5), 5, Arith::Compat), Places::new(20, 5));
        assert_eq!(carried(Places::new(28, 6), 6, Arith::Compat), Places::new(24, 6));
        assert_eq!(carried(Places::new(20, 12), 4, Arith::Compat), Places::new(20, 10));
        assert_eq!(carried(Places::new(28, 12), 4, Arith::Compat), Places::new(26, 4));
        assert_eq!(carried(Places::new(28, 6), 6, Arith::Extend), Places::new(25, 6));
    }

    #[test]
    fn an_18_by_18_digit_product_loses_high_order_digits_under_compat_but_fewer_under_extend() {
        let big = Fixed::new(999_999_999_999_999_999, S18);
        let compat = big.mul(big, 0, Arith::Compat).unwrap();
        let extend = big.mul(big, 0, Arith::Extend).unwrap();
        assert_eq!(compat.places, Places::new(30, 0));
        assert_eq!(extend.places, Places::new(31, 0));
        let exact = U256::widening_mul(999_999_999_999_999_999, 999_999_999_999_999_999);
        assert_eq!(compat.magnitude, exact.div_rem(U256::pow10(30)).1);
        assert_eq!(extend.magnitude, exact.div_rem(U256::pow10(31)).1);
    }

    #[test]
    fn division_carries_dmax_decimal_places_and_truncates() {
        let q = Fixed::new(10, Places::new(2, 0)).div(Fixed::new(3, Places::new(1, 0)), 2, Arith::Compat).unwrap();
        assert_eq!((q.to_i128(), q.places.dec), (Some(333), 2));
        let n = Fixed::new(-10, Places::new(2, 0)).div(Fixed::new(3, Places::new(1, 0)), 2, Arith::Compat).unwrap();
        assert_eq!(n.to_i128(), Some(-333));
        assert_eq!(Fixed::new(1, S18).div(Fixed::new(0, S18), 0, Arith::Compat), Err(ArithError::DivideByZero));
    }

    #[test]
    fn a_rounded_quotient_carries_one_place_more_than_its_receiver() {
        let (dividend, divisor) = (Fixed::new(16617, Places::new(4, 1)), Fixed::new(441, Places::new(2, 1)));
        let receiver = Places::new(4, 1);
        let truncated = dividend.div(divisor, Dmax::receiver(receiver.dec, false, ExtraPlace::Every).last, Arith::Compat).unwrap();
        assert_eq!(truncated.to_receiver(receiver, true).0.to_i128(), Some(376));
        let rounded = dividend.div(divisor, Dmax::receiver(receiver.dec, true, ExtraPlace::Every).last, Arith::Compat).unwrap();
        assert_eq!((rounded.to_i128(), rounded.places.dec), (Some(3768), 2));
        assert_eq!(rounded.to_receiver(receiver, true).0.to_i128(), Some(377));
    }

    #[test]
    fn gnucobol_counts_the_rounded_place_in_the_last_operation_alone() {
        assert_eq!(Dmax::receiver(2, true, ExtraPlace::Every), Dmax { last: 3, inner: 3 });
        assert_eq!(Dmax::receiver(2, true, ExtraPlace::Last), Dmax { last: 3, inner: 2 });
        assert_eq!(Dmax::receiver(2, false, ExtraPlace::Last), Dmax { last: 2, inner: 2 });
        let statement = Dmax::receiver(2, true, ExtraPlace::Last).max(Dmax::receiver(1, false, ExtraPlace::Last)).with(1);
        assert_eq!(statement, Dmax { last: 3, inner: 2 });
        assert_eq!(statement.with(4), Dmax { last: 4, inner: 4 });
    }

    #[test]
    fn off_counts_no_rounded_place() {
        assert_eq!(Dmax::receiver(2, true, ExtraPlace::Off), Dmax { last: 2, inner: 2 });
        assert_eq!(Dmax::receiver(2, true, ExtraPlace::Off).with(3), Dmax { last: 3, inner: 3 });
    }

    #[test]
    fn addition_aligns_decimal_points() {
        let s = Fixed::new(125, Places::new(1, 2)).add(Fixed::new(-3, Places::new(1, 0)), 2, Arith::Compat).unwrap();
        assert_eq!((s.to_i128(), s.places), (Some(-175), Places::new(2, 2)));
    }

    #[test]
    fn receivers_truncate_or_round_half_away_and_flag_size_errors() {
        let v = Fixed::new(-12345, Places::new(3, 2));
        assert_eq!(v.to_receiver(Places::new(3, 1), false).0.to_i128(), Some(-1234));
        assert_eq!(v.to_receiver(Places::new(3, 1), true).0.to_i128(), Some(-1235));
        let (wrapped, size_error) = v.to_receiver(Places::new(2, 0), false);
        assert_eq!((wrapped.to_i128(), size_error), (Some(-23), true));
        let (rounded_over, size_error) = Fixed::new(999, Places::new(1, 2)).to_receiver(Places::new(1, 1), true);
        assert_eq!((rounded_over.to_i128(), size_error), (Some(0), true));
    }

    #[test]
    fn negative_zero_is_normalized() {
        let z = Fixed::new(5, Places::new(1, 0)).sub(Fixed::new(5, Places::new(1, 0)), 0, Arith::Compat).unwrap();
        assert!(!z.negative);
    }
}
