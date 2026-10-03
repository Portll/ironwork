//! DFSORT's numbers in BUILD and OVERLAY items, as OUTFIL OUTREC's p,m,f,edit and p,m,f,to
//! describe them: the formats a number is read from (Table 7), the digits each needs (Table 10),
//! the M0-M26 edit masks (Tables 8, 9 and 11), edit patterns, and the formats TO writes (Table 12).

use jcl::sort::{Mask, NumberFormat, ToFormat};
use zarch::ebcdic::CodePage;

const BLANK: u8 = 0x40;
const MINUS: u8 = 0x60;
const RIGHT_PAREN: u8 = 0x5D;
const LARGEST: u128 = 9_999_999_999_999_999_999_999_999_999_999;

/// A number as DFSORT reads one: its sign, kept for -0, and its magnitude.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Number {
    pub negative: bool,
    pub magnitude: u128,
}

/// A ZD or PD field holding a digit nibble above 9, which DFSORT ends with a data exception.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidDigit;

fn negative_sign(nibble: u8) -> bool {
    nibble % 2 == 1 && nibble != 0xF
}

fn digits_value(digits: impl Iterator<Item = u8>) -> u128 {
    digits.fold(0u128, |n, d| (n * 10 + u128::from(d)) % (LARGEST + 1))
}

/// The value of `bytes` read as `format`.
pub fn read(bytes: &[u8], format: NumberFormat) -> Result<Number, InvalidDigit> {
    let positive = |magnitude| Number { negative: false, magnitude };
    Ok(match format {
        NumberFormat::Zd => {
            if bytes.iter().any(|b| b & 0x0F > 9) {
                return Err(InvalidDigit);
            }
            Number { negative: bytes.last().is_some_and(|b| negative_sign(b >> 4)), magnitude: digits_value(bytes.iter().map(|b| b & 0x0F)) }
        }
        NumberFormat::Pd | NumberFormat::Pd0 => {
            let mut nibbles: Vec<u8> = bytes.iter().flat_map(|b| [b >> 4, b & 0x0F]).collect();
            let sign = nibbles.pop().unwrap_or(0xC);
            if format == NumberFormat::Pd0 && !nibbles.is_empty() {
                nibbles.remove(0);
            }
            if nibbles.iter().any(|&d| d > 9) {
                return Err(InvalidDigit);
            }
            Number { negative: format == NumberFormat::Pd && negative_sign(sign), magnitude: digits_value(nibbles.into_iter()) }
        }
        NumberFormat::Bi => positive(bytes.iter().fold(0u128, |n, &b| (n << 8) | u128::from(b))),
        NumberFormat::Fi => {
            let negative = bytes.first().is_some_and(|b| b & 0x80 != 0);
            let mut value: i128 = if negative { -1 } else { 0 };
            for &b in bytes {
                value = (value << 8) | i128::from(b);
            }
            Number { negative, magnitude: value.unsigned_abs() }
        }
        NumberFormat::Fs => {
            let mut digits = Vec::new();
            let mut negative = false;
            for &b in bytes.iter().rev() {
                if (0xF0..=0xF9).contains(&b) {
                    digits.push(b & 0x0F);
                } else {
                    negative = b == MINUS;
                    break;
                }
            }
            digits.truncate(31);
            Number { negative, magnitude: digits_value(digits.into_iter().rev()) }
        }
        NumberFormat::Uff | NumberFormat::Sff => {
            let mut digits: Vec<u8> = bytes.iter().filter(|b| (0xF0..=0xF9).contains(*b)).map(|b| b & 0x0F).collect();
            if digits.len() > 31 {
                digits.drain(..digits.len() - 31);
            }
            Number { negative: format == NumberFormat::Sff && bytes.iter().any(|&b| b == MINUS || b == RIGHT_PAREN), magnitude: digits_value(digits.into_iter()) }
        }
    })
}

/// The digits a field of `length` bytes needs (Table 10).
pub fn digits_needed(format: NumberFormat, length: usize) -> usize {
    match format {
        NumberFormat::Zd => length,
        NumberFormat::Pd => 2 * length - 1,
        NumberFormat::Pd0 => 2 * length - 2,
        NumberFormat::Bi | NumberFormat::Fi => [3, 5, 8, 10, 13, 15, 17, 20][length.clamp(1, 8) - 1],
        NumberFormat::Fs | NumberFormat::Uff | NumberFormat::Sff => length.min(31),
    }
}

/// The digits a decimal constant is edited and converted with: 15, or 31 past 15 significant digits.
pub fn constant_digits(value: i128) -> usize {
    if value.unsigned_abs() > 999_999_999_999_999 { 31 } else { 15 }
}

/// One position of an edit pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Digit { significant: bool },
    LeadSign,
    TrailSign,
    /// CR, printed for a negative value and blank for a positive one.
    Credit,
    Char(char),
}

/// A mask's general shape: whether it carries a leading sign and what follows its digits, the
/// thousands separator, the decimal point and how many digits follow it, and whether every digit
/// is significant or only the last integer digit and the decimals.
struct Shape {
    lead: bool,
    trail: Option<Slot>,
    group: Option<char>,
    decimal: Option<(char, usize)>,
    all_significant: bool,
}

const fn shape(lead: bool, trail: Option<Slot>, group: Option<char>, decimal: Option<(char, usize)>, all_significant: bool) -> Shape {
    Shape { lead, trail, group, decimal, all_significant }
}

const S: Option<Slot> = Some(Slot::TrailSign);

/// M0-M26 by shape; M6-M9 are fixed patterns and have no shape. Table 8 of OUTFIL OUTREC.
fn mask_shape(n: u8) -> Option<Shape> {
    Some(match n {
        0 => shape(false, S, None, None, false),
        1 => shape(false, S, None, None, true),
        2 => shape(false, S, Some(','), Some(('.', 2)), false),
        3 => shape(false, Some(Slot::Credit), Some(','), Some(('.', 2)), false),
        4 => shape(true, None, Some(','), Some(('.', 2)), false),
        5 => shape(true, S, Some(','), Some(('.', 2)), false),
        10 => shape(false, None, None, None, false),
        11 => shape(false, None, None, None, true),
        12 => shape(true, None, Some(','), None, false),
        13 => shape(true, None, Some('.'), None, false),
        14 => shape(true, S, Some(' '), None, false),
        15 => shape(false, S, Some(' '), None, false),
        16 => shape(true, None, Some(' '), None, false),
        17 => shape(true, None, Some('\''), None, false),
        18 => shape(true, None, Some(','), Some(('.', 2)), false),
        19 => shape(true, None, Some('.'), Some((',', 2)), false),
        20 => shape(true, S, Some(' '), Some((',', 2)), false),
        21 => shape(false, S, Some(' '), Some((',', 2)), false),
        22 => shape(true, None, Some(' '), Some((',', 2)), false),
        23 => shape(true, None, Some('\''), Some(('.', 2)), false),
        24 => shape(true, None, Some('\''), Some((',', 2)), false),
        25 => shape(true, None, None, None, false),
        26 => shape(true, None, None, None, true),
        _ => return None,
    })
}

/// The mask's sign characters, lp, ln, tp, tn (Table 9); None where the mask has no such sign.
fn mask_signs(n: u8) -> [Option<char>; 4] {
    match n {
        0 | 1 | 2 | 15 | 21 => [None, None, Some(' '), Some('-')],
        4 | 26 => [Some('+'), Some('-'), None, None],
        5 | 14 | 20 => [Some(' '), Some('('), Some(' '), Some(')')],
        12 | 13 | 16 | 17 | 18 | 19 | 22 | 23 | 24 | 25 => [Some(' '), Some('-'), None, None],
        _ => [None; 4],
    }
}

fn pattern_slots(text: &str, insignificant: char, significant: char, sign: Option<char>) -> Vec<Slot> {
    let chars: Vec<char> = text.chars().collect();
    let last = chars.len().saturating_sub(1);
    chars
        .iter()
        .enumerate()
        .map(|(i, &c)| match c {
            c if c == insignificant => Slot::Digit { significant: false },
            c if c == significant => Slot::Digit { significant: true },
            c if Some(c) == sign && i == 0 => Slot::LeadSign,
            c if Some(c) == sign && i == last => Slot::TrailSign,
            c => Slot::Char(c),
        })
        .collect()
}

/// The specific pattern of mask `n` for `digits` digits: the signs, the rightmost digits of the
/// 31-digit general pattern, and the characters between them. M6-M9 are used whole.
fn mask_slots(n: u8, digits: usize) -> Vec<Slot> {
    let fixed = match n {
        6 => Some("III-TTT-TTTT"),
        7 => Some("TTT-TT-TTTT"),
        8 => Some("IT:TT:TT"),
        9 => Some("IT/TT/TT"),
        _ => None,
    };
    if let Some(text) = fixed {
        return pattern_slots(text, 'I', 'T', None);
    }
    let shape = mask_shape(n).expect("a mask number checked by the reader");
    let decimals = shape.decimal.map_or(0, |(_, k)| k);
    let mut general: Vec<Slot> = Vec::new();
    for i in 0..31 - decimals {
        let from_right = 31 - decimals - i;
        if i > 0 && from_right.is_multiple_of(3)
            && let Some(g) = shape.group
        {
            general.push(Slot::Char(g));
        }
        general.push(Slot::Digit { significant: shape.all_significant || from_right == 1 });
    }
    if let Some((point, k)) = shape.decimal {
        general.push(Slot::Char(point));
        general.extend(std::iter::repeat_n(Slot::Digit { significant: true }, k));
    }
    let mut taken = 0;
    let mut start = general.len();
    while start > 0 && taken < digits.max(1) {
        start -= 1;
        if matches!(general[start], Slot::Digit { .. }) {
            taken += 1;
        }
    }
    let mut out = Vec::new();
    if shape.lead {
        out.push(Slot::LeadSign);
    }
    out.extend_from_slice(&general[start..]);
    out.extend(shape.trail);
    out
}

/// A numeric value edited by a mask or pattern into characters, then fitted to `length`: cut on
/// the left when shorter, blanks on the left when longer.
pub fn edit(value: Number, mask: &Mask, signs: Option<[Option<char>; 4]>, digits: usize, length: Option<usize>, page: &CodePage) -> Vec<u8> {
    let (slots, sign_chars) = match mask {
        Mask::Predefined(n) => {
            let own = mask_signs(*n);
            (mask_slots(*n, digits), signs.map_or(own, |s| s.map(|c| Some(c.unwrap_or(' ')))))
        }
        Mask::Pattern { text, insignificant, significant, sign } => {
            (pattern_slots(text, *insignificant, *significant, signs.map(|_| *sign)), signs.map_or([None; 4], |s| s.map(|c| Some(c.unwrap_or(' ')))))
        }
    };
    let mut out: Vec<char> = Vec::with_capacity(slots.len());
    let digit_count = slots.iter().filter(|s| matches!(s, Slot::Digit { .. })).count();
    let text = value.magnitude.to_string();
    let mapped: Vec<char> = format!("{text:0>digit_count$}").chars().rev().take(digit_count).collect::<Vec<_>>().into_iter().rev().collect();
    let mut next = mapped.iter();
    let [lp, ln, tp, tn] = sign_chars;
    let last_digit = slots.iter().rposition(|s| matches!(s, Slot::Digit { .. }));
    for (i, slot) in slots.iter().enumerate() {
        match slot {
            Slot::Digit { .. } => out.push(*next.next().expect("a digit for each digit slot")),
            Slot::LeadSign => out.push(if value.negative { ln } else { lp }.unwrap_or(' ')),
            Slot::TrailSign => out.push(if value.negative { tn } else { tp }.unwrap_or(' ')),
            Slot::Credit => out.extend(if value.negative { ['C', 'R'] } else { [' ', ' '] }),
            // A pattern's characters after its last digit are kept for a negative value only.
            Slot::Char(_) if matches!(mask, Mask::Pattern { .. }) && !value.negative && last_digit.is_some_and(|l| i > l) => out.push(' '),
            Slot::Char(c) => out.push(*c),
        }
    }
    // Positions in `out` line up with `slots`, except after a CR, which only ends a mask.
    let first_digit = slots.iter().position(|s| matches!(s, Slot::Digit { .. }));
    if let Some(first_digit) = first_digit {
        let significant_point = |i: usize| matches!(slots[i], Slot::Char('.')) && matches!(slots.get(i + 1), Some(Slot::Digit { .. })) && matches!(mask, Mask::Pattern { .. });
        let first_significant = (first_digit..slots.len())
            .find(|&i| match slots[i] {
                Slot::Digit { significant } => significant || out[i] != '0',
                _ => significant_point(i),
            })
            .unwrap_or(slots.len());
        for c in out.iter_mut().take(first_significant).skip(first_digit) {
            *c = ' ';
        }
        let prefix: Vec<char> = out[..first_digit].to_vec();
        if !prefix.is_empty() && first_significant > first_digit && first_significant < out.len() {
            for c in out.iter_mut().take(first_digit) {
                *c = ' ';
            }
            let start = first_significant.saturating_sub(prefix.len());
            for (k, c) in prefix.into_iter().enumerate() {
                out[start + k] = c;
            }
        }
    }
    let mut bytes = page.encode_lossy(&out.into_iter().collect::<String>());
    if let Some(n) = length {
        bytes = fit_left(bytes, n, BLANK);
    }
    bytes
}

/// `bytes` cut on the left to `n`, or widened on the left with `fill`.
fn fit_left(bytes: Vec<u8>, n: usize, fill: u8) -> Vec<u8> {
    if bytes.len() >= n {
        bytes[bytes.len() - n..].to_vec()
    } else {
        std::iter::repeat_n(fill, n - bytes.len()).chain(bytes).collect()
    }
}

/// The length a conversion to `to` implies for a number of `digits` digits (Table 12).
pub fn converted_length(to: ToFormat, digits: usize) -> usize {
    match to {
        ToFormat::Bi | ToFormat::Fi => if digits <= 9 { 4 } else { 8 },
        ToFormat::Pd | ToFormat::Pdc | ToFormat::Pdf => digits / 2 + 1,
        ToFormat::Zd | ToFormat::Zdf | ToFormat::Zdc => digits,
        ToFormat::Fs => digits + 1,
    }
}

/// A numeric value converted to `to` at `length` bytes: cut on the left when shorter than the
/// value needs, and padded on the left with character zeros (ZD), blanks (FS), binary zeros (PD,
/// BI, positive FI) or binary ones (negative FI).
pub fn convert(value: Number, to: ToFormat, length: usize) -> Vec<u8> {
    let magnitude = value.magnitude.min(LARGEST);
    match to {
        ToFormat::Zd | ToFormat::Zdf | ToFormat::Zdc => {
            let text = magnitude.to_string();
            let mut bytes: Vec<u8> = text.bytes().map(|d| 0xF0 | (d - b'0')).collect();
            let positive = if to == ToFormat::Zdc { 0xC0 } else { 0xF0 };
            bytes = fit_left(bytes, length, 0xF0);
            if let Some(last) = bytes.last_mut() {
                *last = (*last & 0x0F) | if value.negative { 0xD0 } else { positive };
            }
            bytes
        }
        ToFormat::Pd | ToFormat::Pdc | ToFormat::Pdf => {
            let digits = 2 * length - 1;
            let text = format!("{magnitude:0>digits$}");
            let mut nibbles: Vec<u8> = text.bytes().map(|d| d - b'0').collect();
            nibbles.drain(..nibbles.len() - digits);
            nibbles.push(if value.negative { 0xD } else if to == ToFormat::Pdf { 0xF } else { 0xC });
            nibbles.chunks(2).map(|p| (p[0] << 4) | p[1]).collect()
        }
        ToFormat::Bi => {
            let v = value.magnitude.min(u128::from(u64::MAX)) as u64;
            fit_left(v.to_be_bytes().to_vec(), length, 0)
        }
        ToFormat::Fi => {
            let v: i64 = if value.negative { i64::try_from(value.magnitude).map_or(i64::MIN, |m| -m) } else { i64::try_from(value.magnitude).unwrap_or(i64::MAX) };
            fit_left(v.to_be_bytes().to_vec(), length, if v < 0 { 0xFF } else { 0 })
        }
        ToFormat::Fs => {
            let mut bytes: Vec<u8> = magnitude.to_string().bytes().map(|d| 0xF0 | (d - b'0')).collect();
            if value.negative {
                bytes.insert(0, MINUS);
            }
            fit_left(bytes, length, BLANK)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> &'static CodePage {
        numeric::options::Options::default().code_page()
    }

    fn zd(text: &str, negative: bool) -> Vec<u8> {
        let mut b: Vec<u8> = text.bytes().map(|d| 0xF0 | (d - b'0')).collect();
        if negative {
            let last = b.len() - 1;
            b[last] = 0xD0 | (b[last] & 0x0F);
        }
        b
    }

    fn mask(n: u8, value: &str, negative: bool) -> String {
        let bytes = zd(value, negative);
        let v = read(&bytes, NumberFormat::Zd).unwrap();
        page().decode(&edit(v, &Mask::Predefined(n), None, digits_needed(NumberFormat::Zd, bytes.len()), None, page()))
    }

    #[test]
    fn the_masks_print_ibms_examples() {
        // Table 8 of OUTFIL OUTREC, with the field length of each example value.
        for (n, value, negative, want) in [
            (0, "01234", false, " 1234 "),
            (0, "00001", true, "    1-"),
            (1, "00123", true, "00123-"),
            (2, "123450", false, "1,234.50 "),
            (2, "000020", true, "    0.20-"),
            (3, "001234", true, "   12.34CR"),
            (3, "123456", false, "1,234.56  "),
            (4, "0123456", false, " +1,234.56"),
            (4, "1234567", true, "-12,345.67"),
            (5, "001234", true, "   (12.34)"),
            (5, "0123456", false, "  1,234.56 "),
            (6, "00123456", false, "    012-3456"),
            (7, "00123456", false, "000-12-3456"),
            (8, "030553", false, " 3:05:53"),
            (9, "083104", false, " 8/31/04"),
            (10, "00000", false, "    0"),
            (11, "00010", false, "00010"),
            (12, "0012345", true, "   -12,345"),
            (13, "1234567", false, " 1.234.567"),
            (14, "0012345", true, "   (12 345)"),
            (15, "0012345", true, "   12 345-"),
            (18, "1234567", true, "-12,345.67"),
            (19, "0123456", false, "  1.234,56"),
            (20, "1234567", true, "(12 345,67)"),
            (25, "00001", true, "    -1"),
            (26, "01234", false, "+01234"),
        ] {
            assert_eq!(mask(n, value, negative), want, "M{n} of {value}{}", if negative { "-" } else { "" });
        }
    }

    #[test]
    fn mask_lengths_follow_table_11() {
        for (n, format, length, want) in [(0, NumberFormat::Zd, 3, 4), (1, NumberFormat::Pd, 10, 20), (2, NumberFormat::Bi, 4, 14), (3, NumberFormat::Uff, 20, 28), (4, NumberFormat::Pd, 8, 21), (5, NumberFormat::Fi, 3, 12), (6, NumberFormat::Zd, 10, 12), (7, NumberFormat::Pd, 5, 11), (10, NumberFormat::Bi, 6, 15), (12, NumberFormat::Pd, 3, 7), (14, NumberFormat::Zd, 5, 8), (15, NumberFormat::Fi, 3, 11), (16, NumberFormat::Sff, 41, 42), (21, NumberFormat::Zd, 3, 5), (22, NumberFormat::Bi, 2, 7), (24, NumberFormat::Zd, 21, 29), (25, NumberFormat::Fs, 16, 17)] {
            let d = digits_needed(format, length);
            assert_eq!(edit(Number { negative: false, magnitude: 0 }, &Mask::Predefined(n), None, d, None, page()).len(), want, "M{n} of {format:?},{length}");
        }
        assert_eq!(page().decode(&edit(Number { negative: true, magnitude: 7 }, &Mask::Predefined(4), None, 1, None, page())), "-7", "5,1,ZD,M4 has the pattern ST");
        assert_eq!(page().decode(&edit(Number { negative: false, magnitude: 7 }, &Mask::Predefined(4), None, 1, Some(5), page())), "   +7", "LENGTH=5 pads, and keeps the pattern ST");
    }

    #[test]
    fn edit_patterns_print_ibms_examples() {
        let pattern = |text: &str| Mask::Pattern { text: text.into(), insignificant: 'I', significant: 'T', sign: 'S' };
        let ed = |v: i128, text: &str, signs, length| page().decode(&edit(Number { negative: v < 0, magnitude: v.unsigned_abs() }, &pattern(text), signs, 0, length, page()));
        assert_eq!(ed(1230, "**I/ITTTCR", None, None), "  **1230  ");
        assert_eq!(ed(-41, "**I/ITTTCR", None, None), "   **041CR");
        assert_eq!(ed(12345, "IIT", None, None), "345", "the leftmost digits are lost");
        assert_eq!(ed(100345, "$IIT.T", None, None), " $34.5");
        assert_eq!(ed(12345, "$IIT.TT", None, Some(5)), "23.45");
        assert_eq!(ed(12345, "$IIT.TT", None, Some(10)), "   $123.45");
        assert_eq!(ed(-5, "SIIT.TT", Some([Some('+'), Some('-'), None, None]), None), "  -0.05");
        assert_eq!(ed(5, "III.II", None, None), "   .05", "a point before digits is significant (assumption C343)");
    }

    #[test]
    fn numbers_are_read_from_each_format() {
        assert_eq!(read(&[0x01, 0x23, 0x4D], NumberFormat::Pd).unwrap(), Number { negative: true, magnitude: 1234 });
        assert_eq!(read(&[0x81, 0x23, 0x4D], NumberFormat::Pd0).unwrap(), Number { negative: false, magnitude: 1234 });
        assert_eq!(read(&[0xFF, 0xFE], NumberFormat::Fi).unwrap(), Number { negative: true, magnitude: 2 });
        assert_eq!(read(&[0xFF, 0xFE], NumberFormat::Bi).unwrap(), Number { negative: false, magnitude: 65534 });
        assert_eq!(read(&[0xF1, 0xFA], NumberFormat::Zd), Err(InvalidDigit));
        let text = |s: &str| page().encode_lossy(s);
        for (value, negative, magnitude) in [("  -003", true, 3), ("--1234", true, 1234), (" +1234", false, 1234), ("  0034", false, 34)] {
            assert_eq!(read(&text(value), NumberFormat::Fs).unwrap(), Number { negative, magnitude }, "FS {value}");
        }
        assert_eq!(read(&text("$58,272,300.10"), NumberFormat::Uff).unwrap().magnitude, 5827230010);
        assert_eq!(read(&text("(82,316.90)"), NumberFormat::Sff).unwrap(), Number { negative: true, magnitude: 8231690 });
        assert_eq!(read(&text("400.52-"), NumberFormat::Sff).unwrap(), Number { negative: true, magnitude: 40052 });
    }

    #[test]
    fn conversions_follow_ibms_examples() {
        let n = |v: i128| Number { negative: v < 0, magnitude: v.unsigned_abs() };
        assert_eq!(convert(n(-12345678), ToFormat::Pd, 5), [0x01, 0x23, 0x45, 0x67, 0x8D]);
        assert_eq!(convert(n(-12345678), ToFormat::Pd, 3), [0x45, 0x67, 0x8D]);
        assert_eq!(convert(n(58), ToFormat::Pd, 3), [0x00, 0x05, 0x8C]);
        assert_eq!(convert(n(-1234), ToFormat::Fi, 4), [0xFF, 0xFF, 0xFB, 0x2E]);
        assert_eq!(convert(n(-1234), ToFormat::Fi, 6), [0xFF, 0xFF, 0xFF, 0xFF, 0xFB, 0x2E]);
        assert_eq!(convert(n(58), ToFormat::Fi, 6), [0, 0, 0, 0, 0, 0x3A]);
        assert_eq!(convert(n(-5000), ToFormat::Bi, 2), [0x13, 0x88], "BI holds the absolute value");
        assert_eq!(convert(n(-123), ToFormat::Zd, 5), [0xF0, 0xF0, 0xF1, 0xF2, 0xD3]);
        assert_eq!(convert(n(123), ToFormat::Zdc, 3), [0xF1, 0xF2, 0xC3]);
        assert_eq!(convert(n(123), ToFormat::Pdf, 2), [0x12, 0x3F]);
        assert_eq!(page().decode(&convert(n(-123), ToFormat::Fs, 6)), "  -123");
        assert_eq!(converted_length(ToFormat::Pd, digits_needed(NumberFormat::Bi, 4)), 6);
        assert_eq!(converted_length(ToFormat::Zd, digits_needed(NumberFormat::Pd, 9)), 17);
        assert_eq!(converted_length(ToFormat::Fs, digits_needed(NumberFormat::Fi, 8)), 21, "Table 12 prints FI,4 for this row; d+1 gives 21 for FI,8");
        assert_eq!(converted_length(ToFormat::Bi, digits_needed(NumberFormat::Fs, 10)), 8);
    }
}
