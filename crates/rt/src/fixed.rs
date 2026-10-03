//! Fixed-point helpers the storage semantics share: building, aligning and comparing `Fixed`
//! values, an item's places, and zoned digits.

use crate::storage::Kind;
use numeric::precision::{Fixed, Places};
use std::cmp::Ordering;
use zarch::wide::U256;

/// The most digits a numeric item holds, under ARITH(EXTEND): all an alphanumeric sender can give
/// one (LONG_ZONED_BY_PACKS in numeric::assumptions).
pub const MAX_DIGITS: usize = 31;

pub fn fixed(negative: bool, magnitude: U256, places: Places) -> Fixed {
    Fixed { negative: negative && !magnitude.is_zero(), magnitude, places }
}

pub fn pow10(n: u32) -> U256 {
    U256::pow10(n)
}

/// The value's magnitude at `scale` decimal places, truncated or rounded half away from zero.
pub fn align(value: &Fixed, scale: u32, rounded: bool) -> Option<U256> {
    let from = value.places.dec;
    if scale >= from {
        return value.magnitude.checked_mul(pow10(scale - from));
    }
    let (q, r) = value.magnitude.div_rem(pow10(from - scale));
    let half = pow10(from - scale - 1).checked_mul(U256::from_u128(5))?;
    Some(if rounded && r >= half { q + U256::from_u128(1) } else { q })
}

pub fn compare_fixed(a: &Fixed, b: &Fixed) -> Ordering {
    let dec = a.places.dec.max(b.places.dec);
    let (ma, mb) = (align(a, dec, false).unwrap_or_default(), align(b, dec, false).unwrap_or_default());
    match (a.negative, b.negative) {
        (false, false) => ma.cmp(&mb),
        (true, true) => mb.cmp(&ma),
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
    }
}

/// The places of an item's stored digits; PICTURE Ps to the left of the digits make `scale`
/// exceed `digits`.
pub fn places_of(kind: Kind) -> Places {
    let (digits, scale) = kind.digits_scale().unwrap_or((0, 0));
    Places::new(digits.saturating_sub(scale), scale)
}

/// A value times ten to the `k`: the algebraic value of digits that PICTURE Ps follow.
pub fn scaled_up(f: Fixed, k: u32) -> Fixed {
    if k == 0 {
        return f;
    }
    fixed(f.negative, f.magnitude.checked_mul(pow10(k)).unwrap_or_default(), Places::new(f.places.int + k, f.places.dec))
}

/// A value divided by ten to the `k`, exactly: the same digits with `k` more decimal places.
pub fn scaled_down(f: Fixed, k: u32) -> Fixed {
    Fixed { places: Places::new(f.places.int.saturating_sub(k), f.places.dec + k), ..f }
}

pub fn zoned_digits(magnitude: u128, digits: usize, sign_zone: u8) -> Vec<u8> {
    let mut out = vec![0xF0u8; digits];
    zoned_digits_into(&mut out, magnitude, sign_zone);
    out
}

/// `zoned_digits` written over `out`, as many digits as it has.
pub fn zoned_digits_into(out: &mut [u8], magnitude: u128, sign_zone: u8) {
    let mut m = magnitude;
    let mut digits = out.iter_mut().rev();
    while m > u128::from(u64::MAX) {
        let Some(b) = digits.next() else { break };
        *b = 0xF0 | (m % 10) as u8;
        m /= 10;
    }
    let mut m = m as u64;
    for b in digits {
        *b = 0xF0 | (m % 10) as u8;
        m /= 10;
    }
    if let Some(last) = out.last_mut() {
        *last = (sign_zone << 4) | (*last & 0x0F);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoned_digits_keep_the_low_order_digits_of_any_magnitude() {
        let mut m = 1u128;
        let mut magnitudes = vec![0, u128::from(u64::MAX), u128::from(u64::MAX) + 1, u128::MAX];
        while let Some(next) = m.checked_mul(7) {
            magnitudes.extend([next, next - 1, next + 1]);
            m = next;
        }
        for &magnitude in &magnitudes {
            for digits in 0..42 {
                for zone in [0xC, 0xD, 0xF] {
                    let mut expected = vec![0xF0u8; digits];
                    let mut rest = magnitude;
                    for b in expected.iter_mut().rev() {
                        *b = 0xF0 | (rest % 10) as u8;
                        rest /= 10;
                    }
                    if let Some(last) = expected.last_mut() {
                        *last = (zone << 4) | (*last & 0x0F);
                    }
                    assert_eq!(zoned_digits(magnitude, digits, zone), expected, "{magnitude} in {digits} digits");
                }
            }
        }
    }
}
