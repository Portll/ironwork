//! Packed and zoned decimal, and the decimal instructions a COBOL compiler emits for them.
//! Operands are storage images, big-endian, 1 to 16 bytes. Overlapping operands are the caller's
//! concern: each instruction here reads both operands before writing the first.

use crate::check::{Cc, ProgramCheck};
use std::cmp::Ordering;

pub const PLUS: u8 = 0xC;
pub const MINUS: u8 = 0xD;
pub const UNSIGNED: u8 = 0xF;

pub const fn is_sign(nibble: u8) -> bool {
    nibble >= 0xA
}

pub const fn is_minus(nibble: u8) -> bool {
    nibble == 0xB || nibble == 0xD
}

/// A packed field's value, with the sign it was stored with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decimal {
    pub negative: bool,
    pub magnitude: u128,
}

impl Decimal {
    /// The signed value; `decode` bounds the magnitude to 31 digits, which fits.
    pub fn to_i128(self) -> i128 {
        if self.negative { -(self.magnitude as i128) } else { self.magnitude as i128 }
    }

    fn neg(self) -> Self {
        Self { negative: !self.negative, ..self }
    }

    fn plus(self, other: Self) -> Self {
        match (self.negative == other.negative, self.magnitude.cmp(&other.magnitude)) {
            (true, _) => Self { negative: self.negative, magnitude: self.magnitude + other.magnitude },
            (false, Ordering::Less) => Self { negative: other.negative, magnitude: other.magnitude - self.magnitude },
            (false, _) => Self { negative: self.negative, magnitude: self.magnitude - other.magnitude },
        }
    }
}

pub const fn digits_in(len: usize) -> u32 {
    2 * len as u32 - 1
}

fn check_len(op: &[u8]) -> Result<(), ProgramCheck> {
    if (1..=16).contains(&op.len()) { Ok(()) } else { Err(ProgramCheck::Specification) }
}

/// TEST DECIMAL: 0 valid, 1 invalid sign, 2 invalid digit, 3 both.
pub fn tp(op: &[u8]) -> Result<Cc, ProgramCheck> {
    check_len(op)?;
    let (last, body) = op.split_last().unwrap();
    let sign_bad = !is_sign(last & 0xF);
    let digit_bad = last >> 4 > 9 || body.iter().any(|b| b >> 4 > 9 || b & 0xF > 9);
    Ok(Cc(sign_bad as u8 | (digit_bad as u8) << 1))
}

pub fn decode(op: &[u8]) -> Result<Decimal, ProgramCheck> {
    if tp(op)?.0 != 0 {
        return Err(ProgramCheck::Data);
    }
    let (last, body) = op.split_last().unwrap();
    let magnitude = body.iter().fold(0u128, |m, b| (m * 10 + (b >> 4) as u128) * 10 + (b & 0xF) as u128);
    Ok(Decimal { negative: is_minus(last & 0xF), magnitude: magnitude * 10 + (last >> 4) as u128 })
}

fn write_digits(op: &mut [u8], negative: bool, mut magnitude: u128) {
    let (last, body) = op.split_last_mut().unwrap();
    *last = ((magnitude % 10) as u8) << 4 | if negative { MINUS } else { PLUS };
    magnitude /= 10;
    for byte in body.iter_mut().rev() {
        let lo = (magnitude % 10) as u8;
        let hi = (magnitude / 10 % 10) as u8;
        magnitude /= 100;
        *byte = hi << 4 | lo;
    }
}

/// Stores a decimal result. A zero result is positive unless high-order digits were lost, when it
/// keeps the sign of the true result.
fn set_result(op: &mut [u8], negative: bool, kept: u128, overflow: bool) -> Cc {
    let negative = negative && (kept != 0 || overflow);
    write_digits(op, negative, kept);
    Cc(match (overflow, kept == 0, negative) {
        (true, _, _) => 3,
        (false, true, _) => 0,
        (false, false, true) => 1,
        (false, false, false) => 2,
    })
}

fn set_value(op: &mut [u8], v: Decimal) -> Cc {
    let cap = 10u128.pow(digits_in(op.len()));
    set_result(op, v.negative && v.magnitude != 0, v.magnitude % cap, v.magnitude >= cap)
}

/// Stores `value` with a preferred sign, keeping the low-order digits that fit, as ZAP does.
pub fn encode(op: &mut [u8], value: Decimal) -> Result<Cc, ProgramCheck> {
    check_len(op)?;
    Ok(set_value(op, value))
}

/// PACK: zoned to packed. Nothing is validated: every zone but the last is discarded, and the last
/// becomes the sign.
pub fn pack(op1: &mut [u8], op2: &[u8]) -> Result<(), ProgramCheck> {
    check_len(op1)?;
    check_len(op2)?;
    let (last2, body2) = op2.split_last().unwrap();
    let (last1, body1) = op1.split_last_mut().unwrap();
    *last1 = last2.rotate_left(4);
    let mut digits = body2.iter().rev().map(|b| b & 0xF);
    for byte in body1.iter_mut().rev() {
        let lo = digits.next().unwrap_or(0);
        *byte = digits.next().unwrap_or(0) << 4 | lo;
    }
    Ok(())
}

/// UNPK: packed to zoned with EBCDIC zones (X'F'). Nothing is validated; the sign becomes the last
/// zone.
pub fn unpk(op1: &mut [u8], op2: &[u8]) -> Result<(), ProgramCheck> {
    check_len(op1)?;
    check_len(op2)?;
    let (last2, body2) = op2.split_last().unwrap();
    let (last1, body1) = op1.split_last_mut().unwrap();
    *last1 = last2.rotate_left(4);
    let mut digits = body2.iter().rev().flat_map(|b| [b & 0xF, b >> 4]);
    for byte in body1.iter_mut().rev() {
        *byte = 0xF0 | digits.next().unwrap_or(0);
    }
    Ok(())
}

/// ZERO AND ADD: only the second operand is validated.
pub fn zap(op1: &mut [u8], op2: &[u8]) -> Result<Cc, ProgramCheck> {
    check_len(op1)?;
    let b = decode(op2)?;
    Ok(set_value(op1, b))
}

pub fn ap(op1: &mut [u8], op2: &[u8]) -> Result<Cc, ProgramCheck> {
    let (a, b) = (decode(op1)?, decode(op2)?);
    Ok(set_value(op1, a.plus(b)))
}

pub fn sp(op1: &mut [u8], op2: &[u8]) -> Result<Cc, ProgramCheck> {
    let (a, b) = (decode(op1)?, decode(op2)?);
    Ok(set_value(op1, a.plus(b.neg())))
}

/// COMPARE DECIMAL: algebraic, so +0 equals -0 and X'1F' equals X'1C'.
pub fn cp(op1: &[u8], op2: &[u8]) -> Result<Cc, ProgramCheck> {
    let (a, b) = (decode(op1)?, decode(op2)?);
    Ok(Cc::from(a.to_i128().cmp(&b.to_i128())))
}

fn check_multiplier_len(op1: &[u8], op2: &[u8]) -> Result<(), ProgramCheck> {
    check_len(op1)?;
    check_len(op2)?;
    if op2.len() > 8 || op2.len() >= op1.len() { Err(ProgramCheck::Specification) } else { Ok(()) }
}

/// MULTIPLY DECIMAL. The multiplicand needs as many leftmost zero bytes as the multiplier has bytes.
/// The product's sign follows algebra even when it is zero.
pub fn mp(op1: &mut [u8], op2: &[u8]) -> Result<(), ProgramCheck> {
    check_multiplier_len(op1, op2)?;
    let (a, b) = (decode(op1)?, decode(op2)?);
    if op1[..op2.len()].iter().any(|&x| x != 0) {
        return Err(ProgramCheck::Data);
    }
    write_digits(op1, a.negative != b.negative, a.magnitude * b.magnitude);
    Ok(())
}

/// DIVIDE DECIMAL: quotient on the left, remainder in the rightmost `op2.len()` bytes. The quotient's
/// sign follows algebra and the remainder takes the dividend's, both even when zero.
pub fn dp(op1: &mut [u8], op2: &[u8]) -> Result<(), ProgramCheck> {
    check_multiplier_len(op1, op2)?;
    let (a, b) = (decode(op1)?, decode(op2)?);
    let quotient_len = op1.len() - op2.len();
    if b.magnitude == 0 || a.magnitude / b.magnitude >= 10u128.pow(digits_in(quotient_len)) {
        return Err(ProgramCheck::DecimalDivide);
    }
    let (q, r) = op1.split_at_mut(quotient_len);
    write_digits(q, a.negative != b.negative, a.magnitude / b.magnitude);
    write_digits(r, a.negative, a.magnitude % b.magnitude);
    Ok(())
}

/// SHIFT AND ROUND DECIMAL. `shift` is the low six bits of the second-operand address, read as
/// two's complement: positive shifts left, negative shifts right and adds `rounding` to the first
/// digit shifted out.
pub fn srp(op1: &mut [u8], shift: u8, rounding: u8) -> Result<Cc, ProgramCheck> {
    let a = decode(op1)?;
    if rounding > 9 {
        return Err(ProgramCheck::Data);
    }
    let shift = (((shift & 0x3F) << 2) as i8 >> 2) as i32;
    let capacity = digits_in(op1.len());
    if shift >= 0 {
        let shift = shift as u32;
        let kept = if shift >= capacity { 0 } else { a.magnitude % 10u128.pow(capacity - shift) * 10u128.pow(shift) };
        let overflow = a.magnitude != 0 && (shift >= capacity || a.magnitude >= 10u128.pow(capacity - shift));
        return Ok(set_result(op1, a.negative, kept, overflow));
    }
    let n = shift.unsigned_abs();
    let dropped = a.magnitude / 10u128.pow(n - 1) % 10;
    let kept = a.magnitude / 10u128.pow(n) + (dropped + rounding as u128 >= 10) as u128;
    Ok(set_result(op1, a.negative, kept, false))
}

/// CONVERT TO BINARY (32-bit).
pub fn cvb(op2: &[u8; 8]) -> Result<i32, ProgramCheck> {
    i32::try_from(decode(op2)?.to_i128()).map_err(|_| ProgramCheck::FixedPointDivide)
}

/// CONVERT TO BINARY (64-bit).
pub fn cvbg(op2: &[u8; 16]) -> Result<i64, ProgramCheck> {
    i64::try_from(decode(op2)?.to_i128()).map_err(|_| ProgramCheck::FixedPointDivide)
}

pub fn cvd(value: i32) -> [u8; 8] {
    let mut out = [0; 8];
    write_digits(&mut out, value < 0, value.unsigned_abs() as u128);
    out
}

pub fn cvdg(value: i64) -> [u8; 16] {
    let mut out = [0; 16];
    write_digits(&mut out, value < 0, value.unsigned_abs() as u128);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packed(hex: &str) -> Vec<u8> {
        (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn pack_takes_the_digit_of_an_embedded_space() {
        let mut out = [0u8; 2];
        pack(&mut out, &[0xF1, 0x40, 0xF3]).unwrap();
        assert_eq!(out, [0x10, 0x3F]);
        assert_eq!(decode(&out).unwrap(), Decimal { negative: false, magnitude: 103 });
    }

    #[test]
    fn pack_of_a_trailing_space_leaves_an_invalid_sign() {
        let mut out = [0u8; 2];
        pack(&mut out, &[0xF1, 0xF2, 0x40]).unwrap();
        assert_eq!(out, [0x12, 0x04]);
        assert_eq!(ap(&mut out, &[0x1C]), Err(ProgramCheck::Data));
    }

    #[test]
    fn ascii_digits_pack_to_an_invalid_sign() {
        let mut out = [0u8; 2];
        pack(&mut out, b"123").unwrap();
        assert_eq!(out, [0x12, 0x33]);
        assert_eq!(tp(&out), Ok(Cc(1)));
    }

    #[test]
    fn overpunched_sign_packs_and_unpacks() {
        let mut p = [0u8; 2];
        pack(&mut p, &[0xF1, 0xF2, 0xD3]).unwrap();
        assert_eq!(p, [0x12, 0x3D]);
        let mut z = [0u8; 3];
        unpk(&mut z, &p).unwrap();
        assert_eq!(z, [0xF1, 0xF2, 0xD3]);
    }

    #[test]
    fn unpk_pads_with_zoned_zeros_and_truncates_on_the_left() {
        let mut z = [0u8; 5];
        unpk(&mut z, &[0x12, 0x3C]).unwrap();
        assert_eq!(z, [0xF0, 0xF0, 0xF1, 0xF2, 0xC3]);
        let mut short = [0u8; 2];
        unpk(&mut short, &[0x12, 0x3C]).unwrap();
        assert_eq!(short, [0xF2, 0xC3]);
    }

    #[test]
    fn ap_writes_preferred_signs() {
        let mut a = packed("005F");
        assert_eq!(ap(&mut a, &packed("1C")), Ok(Cc(2)));
        assert_eq!(a, packed("006C"));
        let mut b = packed("005C");
        assert_eq!(ap(&mut b, &packed("7D")), Ok(Cc(1)));
        assert_eq!(b, packed("002D"));
    }

    #[test]
    fn a_zero_sum_is_positive() {
        let mut a = packed("5C");
        assert_eq!(ap(&mut a, &packed("5D")), Ok(Cc(0)));
        assert_eq!(a, packed("0C"));
    }

    #[test]
    fn overflow_keeps_the_true_sign_of_a_zero_result() {
        let mut a = packed("9D");
        assert_eq!(ap(&mut a, &packed("1D")), Ok(Cc(3)));
        assert_eq!(a, packed("0D"));
        let mut b = packed("9C");
        assert_eq!(ap(&mut b, &packed("1C")), Ok(Cc(3)));
        assert_eq!(b, packed("0C"));
    }

    #[test]
    fn zap_validates_only_the_second_operand() {
        let mut a = packed("FFFF");
        assert_eq!(zap(&mut a, &packed("0D")), Ok(Cc(0)));
        assert_eq!(a, packed("000C"));
        assert_eq!(zap(&mut a, &packed("0A0C")), Err(ProgramCheck::Data));
    }

    #[test]
    fn cp_is_algebraic() {
        assert_eq!(cp(&packed("0C"), &packed("0D")), Ok(Cc(0)));
        assert_eq!(cp(&packed("1F"), &packed("001C")), Ok(Cc(0)));
        assert_eq!(cp(&packed("1D"), &packed("0C")), Ok(Cc(1)));
    }

    #[test]
    fn mp_needs_leading_zero_bytes_and_signs_a_zero_product() {
        let mut a = packed("0000123C");
        mp(&mut a, &packed("4D")).unwrap();
        assert_eq!(a, packed("0000492D"));
        let mut z = packed("0000000C");
        mp(&mut z, &packed("4D")).unwrap();
        assert_eq!(z, packed("0000000D"));
        assert_eq!(mp(&mut packed("0100123C"), &packed("4D")), Err(ProgramCheck::Data));
        assert_eq!(mp(&mut packed("123C"), &packed("0000004D")), Err(ProgramCheck::Specification));
    }

    #[test]
    fn dp_splits_quotient_and_remainder() {
        let mut a = packed("0000123C");
        dp(&mut a, &packed("4C")).unwrap();
        assert_eq!(a, packed("00030C3C"));
        let mut n = packed("0000123D");
        dp(&mut n, &packed("4C")).unwrap();
        assert_eq!(n, packed("00030D3D"));
    }

    #[test]
    fn dp_refuses_a_zero_divisor_and_an_oversized_quotient() {
        assert_eq!(dp(&mut packed("00123C"), &packed("0C")), Err(ProgramCheck::DecimalDivide));
        assert_eq!(dp(&mut packed("99999C"), &packed("1C")), Err(ProgramCheck::DecimalDivide));
    }

    #[test]
    fn srp_shifts_left_and_rounds_right() {
        let mut a = packed("00123C");
        assert_eq!(srp(&mut a, 2, 0), Ok(Cc(2)));
        assert_eq!(a, packed("12300C"));
        let mut b = packed("125C");
        assert_eq!(srp(&mut b, 0x3F, 5), Ok(Cc(2)));
        assert_eq!(b, packed("013C"));
        let mut c = packed("124C");
        srp(&mut c, 0x3F, 5).unwrap();
        assert_eq!(c, packed("012C"));
    }

    #[test]
    fn srp_left_shift_overflow() {
        let mut a = packed("123D");
        assert_eq!(srp(&mut a, 2, 0), Ok(Cc(3)));
        assert_eq!(a, packed("300D"));
    }

    #[test]
    fn cvb_and_cvd_round_trip() {
        let p = cvd(-123);
        assert_eq!(p, [0, 0, 0, 0, 0, 0, 0x12, 0x3D]);
        assert_eq!(cvb(&p), Ok(-123));
        assert_eq!(cvd(0), [0, 0, 0, 0, 0, 0, 0, 0x0C]);
        assert_eq!(cvb(&[0x09, 0x99, 0x99, 0x99, 0x99, 0x99, 0x99, 0x9C]), Err(ProgramCheck::FixedPointDivide));
        assert_eq!(cvbg(&cvdg(i64::MIN)), Ok(i64::MIN));
    }

    #[test]
    fn tp_reports_sign_and_digit_faults() {
        assert_eq!(tp(&packed("123C")), Ok(Cc(0)));
        assert_eq!(tp(&packed("1234")), Ok(Cc(1)));
        assert_eq!(tp(&packed("1A3C")), Ok(Cc(2)));
        assert_eq!(tp(&packed("1A34")), Ok(Cc(3)));
        assert_eq!(tp(&[0; 17]), Err(ProgramCheck::Specification));
    }

    #[test]
    fn no_instruction_panics_on_arbitrary_bytes() {
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..20_000 {
            let mut a: Vec<u8> = (0..1 + next() % 17).map(|_| next() as u8).collect();
            let b: Vec<u8> = (0..1 + next() % 17).map(|_| next() as u8).collect();
            let _ = tp(&a);
            let _ = cp(&a, &b);
            let _ = pack(&mut a.clone(), &b);
            let _ = unpk(&mut a.clone(), &b);
            let _ = zap(&mut a.clone(), &b);
            let _ = ap(&mut a.clone(), &b);
            let _ = sp(&mut a.clone(), &b);
            let _ = mp(&mut a.clone(), &b);
            let _ = dp(&mut a.clone(), &b);
            let _ = srp(&mut a, next() as u8, next() as u8 % 12);
            if b.len() >= 8 {
                let _ = cvb(b[..8].try_into().unwrap());
            }
        }
    }
}
