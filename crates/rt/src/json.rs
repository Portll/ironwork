//! The text JSON GENERATE writes for elementary data (Language Reference SC27-8713-03, pp. 381-382;
//! JSON-CODE values from the Programming Guide SC27-8714-03, p. 819).

use crate::abend::Abend;
use crate::intrinsic::real::Real;
use crate::storage::{Kind, Loc, Val};
use crate::store::{self, ProgramFacts};
use crate::vocab::{Figurative, Pos};
use std::ops::{Div, Mul};
use zarch::ebcdic::CodePage;
use zarch::hfp::Hfp;
use zarch::wide::U256;

pub mod parse;

pub const RECEIVER_TOO_SMALL: i64 = 1;
pub const BAD_ENCODING: i64 = 2;
pub const UTF8: u16 = 1208;
pub const UTF16: u16 = 1200;
/// The single-byte EBCDIC code pages a JSON document may be written in (p. 375, Table 45).
pub const CCSIDS: &[u16] = &[1047, 1140, 37, 1141, 273, 1142, 277, 1143, 278, 1144, 280, 1145, 284, 1146, 285, 1147, 297, 1148, 500, 1149, 871];

/// The document's bytes for a national receiver or another, in the CCSID ENCODING names, and the
/// size of one character position; None for an encoding the statement cannot write.
pub fn encoded(document: &str, national: bool, ccsid: Option<u16>) -> Option<(Vec<u8>, usize)> {
    match (national, ccsid) {
        (true, None | Some(UTF16)) => Some((document.encode_utf16().flat_map(u16::to_be_bytes).collect(), 2)),
        (true, _) => None,
        (false, None | Some(UTF8)) => Some((document.as_bytes().to_vec(), 1)),
        (false, Some(c)) if CCSIDS.contains(&c) => CodePage::by_ccsid(c).map(|page| (document.chars().map(|ch| page.encode_char(ch).unwrap_or(0x3F)).collect(), 1)),
        (false, Some(_)) => None,
    }
}

/// Writes as much of a JSON or XML GENERATE document as fits in the receiver in whole character
/// positions of `unit` bytes, and returns how many bytes that is.
pub fn write_document(mem: &mut [u8], receiver: Loc, bytes: &[u8], unit: usize) -> usize {
    let written = if bytes.len() <= receiver.len { bytes.len() } else { receiver.len - receiver.len % unit };
    store::write(mem, Loc { len: written, ..receiver }, &bytes[..written]);
    written
}

/// Whether an item equals a figurative constant, as SUPPRESS ... WHEN and CONVERTING ... TO JSON
/// NULL test it: numerically for ZERO and a numeric item, otherwise character by character.
pub fn equals_figurative(facts: &dyn ProgramFacts, mem: &[u8], loc: Loc, f: Figurative, pos: Pos) -> Result<bool, Abend> {
    let bytes = store::bytes(mem, loc);
    if f == Figurative::Zero && loc.kind.is_numeric() {
        return Ok(match store::read(facts, mem, loc, pos)? {
            Val::Num(x) => x.magnitude.is_zero(),
            Val::Float(h) => h.fraction == 0,
            _ => false,
        });
    }
    if loc.kind == Kind::National {
        let unit = store::figurative_unit(f, facts.options().quote);
        return Ok(bytes.chunks(2).all(|c| c == unit.to_be_bytes()));
    }
    let byte = facts.figurative(f);
    Ok(bytes.iter().all(|&b| b == byte))
}

/// A fixed-point value as if moved to a numeric-edited item of `integers` integer positions (at
/// least one), `scale` decimal places after an actual period, and a leading minus sign; then
/// trimmed of leading zeros up to the digit before the point, and of the sign's space.
pub fn fixed_number(negative: bool, magnitude: U256, scale: u32, integers: u32) -> String {
    let digits = magnitude.to_u128().map_or_else(|| decimal(magnitude), |m| m.to_string());
    let digits = format!("{digits:0>width$}", width = scale as usize + 1);
    let (whole, fraction) = digits.split_at(digits.len() - scale as usize);
    let kept = &whole[whole.len().saturating_sub(integers.max(1) as usize)..];
    let trimmed = kept.trim_start_matches('0');
    let whole = if trimmed.is_empty() { "0" } else { trimmed };
    let zero = magnitude.is_zero() || (whole == "0" && fraction.bytes().all(|b| b == b'0'));
    let sign = if negative && !zero { "-" } else { "" };
    if scale == 0 { format!("{sign}{whole}") } else { format!("{sign}{whole}.{fraction}") }
}

fn decimal(mut m: U256) -> String {
    let mut out = Vec::new();
    let ten = U256::from_u128(10);
    while !m.is_zero() {
        let (q, r) = m.div_rem(ten);
        out.push(b'0' + r.lo as u8);
        m = q;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// COMP-1 as if moved to PICTURE -9.9(8)E+99 and COMP-2 to -9.9(17)E+99, rounded, with the
/// sign's space trimmed.
pub fn float_number(value: Hfp, decimals: u32) -> String {
    let (negative, text, exponent) = significant_digits(value, decimals + 1);
    let sign = if negative { "-" } else { "" };
    let exponent_sign = if exponent < 0 { '-' } else { '+' };
    format!("{sign}{}.{}E{exponent_sign}{:02}", &text[..1], &text[1..], exponent.unsigned_abs())
}

/// `value` rounded to `count` significant digits: its sign, the digits, and the power of ten of
/// the first; zero is `count` zeros at power 0.
pub fn significant_digits(value: Hfp, count: u32) -> (bool, String, i32) {
    let v = Real::from_hfp(value);
    if v.is_zero() {
        return (false, "0".repeat(count as usize), 0);
    }
    let magnitude = v.abs();
    let decimals = count - 1;
    let mut exponent = magnitude.to_f64().log10().floor() as i32;
    let significant = |e: i32| -> u128 {
        let shift = decimals as i32 - e;
        let scaled = if shift >= 0 { magnitude.mul(power_of_ten(shift as u32)) } else { magnitude.div(power_of_ten(shift.unsigned_abs())) };
        scaled.round_to_integer().unwrap_or(0).unsigned_abs()
    };
    let low = 10u128.pow(decimals);
    let mut digits = significant(exponent);
    if digits >= low * 10 {
        exponent += 1;
        digits = significant(exponent);
    } else if digits < low {
        exponent -= 1;
        digits = significant(exponent);
    }
    (v.is_negative(), digits.to_string(), exponent)
}

fn power_of_ten(mut n: u32) -> Real {
    let mut scale = Real::ONE;
    while n > 0 {
        let step = n.min(38);
        scale = scale.mul(Real::from_u128(10u128.pow(step)));
        n -= step;
    }
    scale
}

/// Character data trimmed of trailing spaces, or of leading ones for a JUSTIFIED item; all
/// spaces leave one.
pub fn trimmed(text: &str, justified: bool) -> &str {
    let t = if justified { text.trim_start_matches(' ') } else { text.trim_end_matches(' ') };
    if t.is_empty() && !text.is_empty() { &text[..1] } else { t }
}

/// A JSON string, with IBM's escapes: \" \\ \b \t \n \f \r, \x for NEXT LINE (U+0085), and \uhhhh
/// for the other control characters below U+0020.
pub fn string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            '\u{85}' => out.push_str("\\x"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use zarch::hfp::Precision;

    fn u(n: u128) -> U256 {
        U256::from_u128(n)
    }

    #[test]
    fn fixed_point_values_trim_as_the_language_reference_shows() {
        assert_eq!(fixed_number(false, u(78), 1, 2), "7.8");
        assert_eq!(fixed_number(true, u(90), 1, 2), "-9.0");
        assert_eq!(fixed_number(true, u(12340), 3, 3), "-12.340");
        assert_eq!(fixed_number(false, u(45), 2, 4), "0.45");
        assert_eq!(fixed_number(false, u(13), 0, 4), "13");
        assert_eq!(fixed_number(false, u(0), 0, 4), "0");
        assert_eq!(fixed_number(true, u(0), 2, 3), "0.00");
        assert_eq!(fixed_number(false, u(123_456), 0, 5), "23456");
    }

    #[test]
    fn floating_point_values_take_the_external_float_picture() {
        let long = |n: i128| Hfp::from_integer(n, Precision::Long);
        assert_eq!(float_number(long(78).div(long(10), Default::default()).unwrap(), 17), "7.79999999999999982E+00");
        assert_eq!(float_number(long(-1234), 8), "-1.23400000E+03");
        assert_eq!(float_number(long(1).div(long(1000), Default::default()).unwrap(), 8), "1.00000000E-03");
        assert_eq!(float_number(Hfp::zero(Precision::Short), 8), "0.00000000E+00");
    }

    #[test]
    fn strings_are_escaped_as_ibm_escapes_them() {
        assert_eq!(string("a\"b\\c\td\u{85}\u{1}"), "\"a\\\"b\\\\c\\td\\x\\u0001\"");
        assert_eq!(trimmed("SX1234  ", false), "SX1234");
        assert_eq!(trimmed("   ", false), " ");
        assert_eq!(trimmed("  AB", true), "AB");
    }
}
