//! Moves between fixed-point decimal and COMP-1/COMP-2, and floating-point intermediates. Each
//! conversion here is an assumption: see [`crate::assumptions::FLOAT_FROM_DECIMAL`],
//! [`crate::assumptions::FLOAT_TO_DECIMAL`] and [`crate::assumptions::FLOAT_NARROWING_ROUNDS`].

use crate::precision::{Fixed, Places};
use zarch::check::{ProgramCheck, ProgramMask};
use zarch::hfp::{Hfp, Precision, Rounding};
use zarch::wide::U256;

pub fn from_fixed(value: Fixed, precision: Precision, mask: ProgramMask) -> Result<Hfp, ProgramCheck> {
    let magnitude = value.magnitude.to_u128().and_then(|m| i128::try_from(m).ok()).expect("a fixed-point operand fits 31 digits");
    let integer = Hfp::from_integer(if value.negative { -magnitude } else { magnitude }, precision);
    if value.places.dec == 0 {
        return Ok(integer);
    }
    let scale = U256::pow10(value.places.dec).to_u128().expect("31 decimal places at most") as i128;
    integer.div(Hfp::from_integer(scale, precision), mask)
}

/// The fixed-point value a receiver of `places` gets, and whether it overflowed (ON SIZE ERROR).
pub fn to_fixed(value: Hfp, places: Places, rounded: bool) -> (Fixed, bool) {
    let rounding = if rounded { Rounding::HalfAwayFromZero } else { Rounding::TowardZero };
    match value.to_scaled_integer(places.dec, rounding) {
        Some((negative, magnitude)) => {
            let exact = Fixed { negative, magnitude, places: Places::new(u32::MAX / 2, places.dec) };
            exact.to_receiver(places, false)
        }
        None => (Fixed::new(0, places), true),
    }
}

pub fn narrow(value: Hfp, target: Precision) -> Hfp {
    value.truncate(target)
}

/// A floating-point value moved or stored into a fixed-point receiver of `places`, and whether it
/// overflowed (ON SIZE ERROR): rounded in the receiver's low-order position, short precision giving
/// at most 9 significant digits and long at most 18, the rest zero (Programming Guide SC27-8714-03,
/// p. 52).
pub fn to_receiver(value: Hfp, places: Places) -> (Fixed, bool) {
    let Some((negative, mut magnitude)) = value.to_scaled_integer(places.dec, Rounding::HalfAwayFromZero) else {
        return (Fixed::new(0, places), true);
    };
    let significant = match value.precision {
        Precision::Short => Some(9),
        Precision::Long => Some(18),
        Precision::Extended => None,
    };
    if let Some(limit) = significant
        && let Some(drop) = decimal_digits(magnitude).checked_sub(limit).filter(|&d| d > 0)
        && let Some((_, truncated)) = value.to_scaled_integer(places.dec, Rounding::TowardZero)
    {
        let unit = U256::pow10(drop);
        let (kept, rest) = truncated.div_rem(unit);
        let up = rest.checked_add(rest).is_some_and(|twice| twice >= unit);
        magnitude = (if up { kept + U256::from_u128(1) } else { kept }).checked_mul(unit).unwrap_or(magnitude);
    }
    let exact = Fixed { negative: negative && !magnitude.is_zero(), magnitude, places: Places::new(u32::MAX / 2, places.dec) };
    exact.to_receiver(places, false)
}

/// A floating-point value stored into a narrower COMP-1 or COMP-2: LOAD ROUNDED (p. 52).
pub fn narrow_rounded(value: Hfp, target: Precision) -> Result<Hfp, ProgramCheck> {
    value.round(target)
}

fn decimal_digits(n: U256) -> u32 {
    (1..=77).find(|&d| n < U256::pow10(d)).unwrap_or(78)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_decimal_fraction_becomes_a_truncated_quotient() {
        let tenth = from_fixed(Fixed::new(1, Places::new(0, 1)), Precision::Long, ProgramMask::default()).unwrap();
        assert_eq!(tenth.to_bytes(), 0x4019_9999_9999_9999u64.to_be_bytes());
    }

    #[test]
    fn a_truncated_tenth_moves_back_as_zero_point_zero_nine() {
        let tenth = Hfp::from_bytes(Precision::Long, &0x4019_9999_9999_9999u64.to_be_bytes());
        assert_eq!(to_fixed(tenth, Places::new(1, 2), false).0.to_i128(), Some(9));
        assert_eq!(to_fixed(tenth, Places::new(1, 2), true).0.to_i128(), Some(10));
    }

    #[test]
    fn a_receiver_gets_the_value_rounded_and_at_most_eighteen_digits_from_long_precision() {
        let tenth = Hfp::from_bytes(Precision::Long, &0x4019_9999_9999_9999u64.to_be_bytes());
        assert_eq!(to_receiver(tenth, Places::new(1, 2)).0.to_i128(), Some(10));
        let third = Hfp::from_integer(1, Precision::Long).div(Hfp::from_integer(3, Precision::Long), ProgramMask::default()).unwrap();
        assert_eq!(to_receiver(third, Places::new(0, 25)).0.to_i128(), Some(3_333_333_333_333_333_290_000_000));
        assert_eq!(to_receiver(third.round(Precision::Short).unwrap(), Places::new(0, 12)).0.to_i128(), Some(333_333_313_000));
    }

    #[test]
    fn overflow_is_a_size_error() {
        let big = Hfp::from_integer(123_456, Precision::Long);
        assert!(to_fixed(big, Places::new(3, 0), false).1);
    }
}
