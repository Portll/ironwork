//! DISPLAY (lir.md §9.1): each item shown as its kind or value shows, then the line written to
//! standard output.

use crate::abend::{Abend, AbendCode, Signal};
use crate::fixed::{pow10, zoned_digits};
use crate::storage::{Kind, Loc, Val};
use crate::store::{self, ProgramFacts};
use crate::vocab::{Pos, SignClause, SignPosition};
use numeric::{Dialect, DispSign, Native, Switched};
use std::io::Write;
use zarch::decimal;
use zarch::ebcdic::{self, CodePage};
use zarch::hfp::{Hfp, Precision};

type R<T> = Result<T, Abend>;

/// A data item, by its kind: packed and binary items as their last digits, COMP-5 and TRUNC(BIN)
/// binary items as every digit their halfword, fullword or doubleword holds, a negative value's
/// sign overpunched on the last. Under DISPSIGN(SEP) a signed binary, packed or overpunched zoned
/// item shows its sign, + or -, before its digits (Programming Guide SC27-8714-03, pp. 362-363,
/// Table 48; assumption C213). Under --dialect gnucobol packed and binary items show as cobc's do
/// (assumption C14). GnuCOBOL's forms (`DispSign::CobcIbmStrict` and `DispSign::Cobc`) show items as
/// cobc -std=ibm-strict and cobc's default dialect do. A national item is converted only
/// `upon_console`, and is otherwise written as its bytes (Language Reference SC27-8713-03, p. 333;
/// Programming Guide SC27-8714-03, p. 36).
pub fn place(facts: &dyn ProgramFacts, mem: &[u8], loc: Loc, pos: Pos, upon_console: bool) -> R<String> {
    let dispsign = facts.options().dispsign;
    let separate = dispsign == DispSign::Sep;
    let whole_native = matches!(loc.kind, Kind::Binary { native, .. } if native.shows_whole(numeric::Trunc::Std) || native == Native::CompX);
    Ok(match loc.kind {
        Kind::National => national(facts.page(), store::bytes(mem, loc), upon_console),
        Kind::Dbcs { .. } => facts.page().decode_dbcs(store::bytes(mem, loc)),
        Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. } if dispsign == DispSign::Cobc && !whole_native => cobc(facts, mem, loc, pos)?,
        Kind::Packed { digits, signed, .. } if dispsign != DispSign::Cobc && facts.options().dialect_of(Switched::DisplayOfNondisplayNumeric) == Dialect::Gnucobol && let Some((negative, shown)) = cobc_bad_packed(facts, mem, loc, digits, signed) => {
            if signed { format!("{}{shown}", if negative { '-' } else { '+' }) } else { shown }
        }
        Kind::Packed { digits, signed, .. } | Kind::Binary { digits, signed, .. }
            if dispsign == DispSign::Cobc || facts.options().dialect_of(Switched::DisplayOfNondisplayNumeric) == Dialect::Gnucobol =>
        {
            let Val::Num(f) = store::read_stored(facts, mem, loc, pos)? else { unreachable!() };
            let shown = match loc.kind {
                // cobc's RETURN-CODE, the only COMP-X item at its offset, shows every digit past its nine.
                Kind::Binary { native: Native::CompX, .. } if loc.offset == crate::unit::RETURN_CODE => {
                    let m = f.magnitude.to_u128().unwrap_or(0);
                    zoned_digits(m, (digits as usize).max(m.to_string().len()), decimal::UNSIGNED)
                }
                Kind::Binary { native: Native::CompX, .. } => zoned_digits(f.magnitude.div_rem(pow10(digits)).1.to_u128().unwrap_or(0), digits as usize, decimal::UNSIGNED),
                Kind::Binary { .. } => zoned_digits(f.magnitude.to_u128().unwrap_or(0), whole_binary_digits(loc.len), decimal::UNSIGNED),
                _ => zoned_digits(f.magnitude.div_rem(pow10(digits)).1.to_u128().unwrap_or(0), digits as usize, decimal::UNSIGNED),
            };
            facts.page().decode(&if signed { sign_first(f.negative, shown) } else { shown })
        }
        Kind::Packed { digits, signed, .. } | Kind::Binary { digits, signed, .. } => {
            let Val::Num(f) = store::read_stored(facts, mem, loc, pos)? else { unreachable!() };
            let zone = if signed && !separate && f.negative { decimal::MINUS } else { decimal::UNSIGNED };
            let whole = match loc.kind {
                Kind::Binary { native, .. } => native.shows_whole(facts.options().trunc),
                _ => false,
            };
            let shown = if whole {
                let width = match loc.len {
                    8 if signed => 19,
                    len => whole_binary_digits(len),
                };
                zoned_digits(f.magnitude.to_u128().unwrap_or(0), width, zone)
            } else {
                zoned_digits(f.magnitude.div_rem(pow10(digits)).1.to_u128().unwrap_or(0), digits as usize, zone)
            };
            facts.page().decode(&if signed && separate { sign_first(f.negative, shown) } else { shown })
        }
        Kind::Zoned { signed, sign, .. } if dispsign == DispSign::CobcIbmStrict => facts.page().decode(&cobc_ibm_strict_zoned(store::bytes(mem, loc), signed, sign)),
        Kind::Zoned { signed: true, sign, .. } if separate && !sign.is_some_and(|s| s.separate) => {
            let mut shown = store::bytes(mem, loc).to_vec();
            let at = if sign == Some(SignClause { position: SignPosition::Leading, separate: false }) { 0 } else { shown.len() - 1 };
            let negative = matches!(shown[at] >> 4, 0xB | 0xD);
            shown[at] |= 0xF0;
            facts.page().decode(&sign_first(negative, shown))
        }
        Kind::Float(precision) => float(Hfp::from_bytes(precision, store::bytes(mem, loc))),
        Kind::Pointer | Kind::Index | Kind::ObjectReference | Kind::ProgramPointer => {
            return Err(crate::refusal::IWR0067.abend("DISPLAY of a pointer, index or object reference is not supported", pos));
        }
        _ => facts.page().decode(store::bytes(mem, loc)),
    })
}

/// cobc -std=ibm-strict's DISPLAY of a zoned item, which moves it to a copy with a separate sign
/// (display_numeric, libcob/termio.c): its bytes as cob_move_display_to_display moves them, the sign
/// byte read as [`store::cobc_sign_byte`] reads it and a space as 0, then the sign, before them
/// under SIGN LEADING and after them otherwise.
fn cobc_ibm_strict_zoned(b: &[u8], signed: bool, sign: Option<SignClause>) -> Vec<u8> {
    let leading = sign.is_some_and(|s| s.position == SignPosition::Leading);
    let (mut shown, negative) = match sign {
        Some(SignClause { separate: true, .. }) if leading => (b[1..].to_vec(), b[0] == 0x60),
        Some(SignClause { separate: true, .. }) => (b[..b.len() - 1].to_vec(), b[b.len() - 1] == 0x60),
        _ if signed => {
            let mut body = b.to_vec();
            let at = if leading { 0 } else { body.len() - 1 };
            let negative;
            (body[at], negative) = store::cobc_sign_byte(body[at]);
            (body, negative)
        }
        _ => (b.to_vec(), false),
    };
    for x in &mut shown {
        if *x == ebcdic::SPACE || *x == 0 {
            *x = 0xF0;
        }
    }
    if signed {
        let mark = if negative { 0x60 } else { 0x4E };
        if leading {
            shown.insert(0, mark);
        } else {
            shown.push(mark);
        }
    }
    shown
}

/// cobc's default dialect's DISPLAY of a zoned, packed or binary item: a sign first, or where a
/// separate one is declared, the digits with the program's decimal point at the item's scale, and
/// PICTURE P positions as zeros, as cobc moves it to a numeric-edited copy (pretty_display_numeric,
/// libcob/termio.c). A zoned item's characters other than digits are shown as they are before the
/// point and as 0 after it, its sign byte read as [`store::cobc_sign_byte`] reads it; the sign is -
/// only where a digit is not 0.
fn cobc(facts: &dyn ProgramFacts, mem: &[u8], loc: Loc, pos: Pos) -> R<String> {
    let Some((digits, scale)) = loc.kind.digits_scale() else { unreachable!() };
    let (signed, separate) = match loc.kind {
        Kind::Zoned { signed, sign, .. } => (signed, sign.filter(|s| s.separate).map(|s| s.position)),
        Kind::Packed { signed, .. } | Kind::Binary { signed, .. } => (signed, None),
        _ => unreachable!(),
    };
    let (negative, number) = match loc.kind {
        Kind::Zoned { sign, .. } => {
            let bytes = store::bytes(mem, loc);
            let body = match separate {
                Some(SignPosition::Leading) => &bytes[1..],
                Some(SignPosition::Trailing) => &bytes[..bytes.len() - 1],
                None => bytes,
            };
            let overpunched = match (signed, separate, sign.map(|s| s.position)) {
                (false, ..) | (true, Some(_), _) => None,
                (true, None, Some(SignPosition::Leading)) => Some(0),
                (true, None, _) => Some(body.len() - 1),
            };
            let shown: Vec<u8> = body.iter().enumerate().map(|(i, &b)| if Some(i) == overpunched { store::cobc_sign_byte(b).0 } else { b }).collect();
            let negative = match separate {
                Some(SignPosition::Leading) => bytes[0] == 0x60,
                Some(SignPosition::Trailing) => bytes[bytes.len() - 1] == 0x60,
                None => overpunched.is_some_and(|at| store::cobc_sign_byte(body[at]).1),
            };
            (negative, facts.page().decode(&shown))
        }
        Kind::Packed { signed, .. } if let Some(bad) = cobc_bad_packed(facts, mem, loc, digits, signed) => bad,
        _ => {
            let Val::Num(f) = store::read_stored(facts, mem, loc, pos)? else { unreachable!() };
            let magnitude = f.magnitude.div_rem(pow10(digits)).1.to_u128().unwrap_or(0);
            (f.negative, format!("{magnitude:0width$}", width = digits as usize))
        }
    };
    let point = facts.decimal_point();
    let decimal = |part: &str| part.chars().map(|c| if c.is_ascii_digit() || ",.+-/B".contains(c) { c } else { '0' }).collect::<String>();
    let shown = if scale > digits {
        format!("{point}{}{}", "0".repeat((scale - digits) as usize), decimal(&number))
    } else if scale > 0 {
        let at = number.char_indices().nth((digits - scale) as usize).map_or(number.len(), |(i, _)| i);
        let (whole, fraction) = number.split_at(at);
        format!("{whole}{point}{}", decimal(fraction))
    } else {
        format!("{number}{}", "0".repeat(store::scaling(facts, loc) as usize))
    };
    let mark = if negative && number.chars().any(|c| c != '0') { '-' } else { '+' };
    Ok(match (signed, separate) {
        (false, _) => shown,
        (true, Some(SignPosition::Trailing)) => format!("{shown}{mark}"),
        (true, _) => format!("{mark}{shown}"),
    })
}

/// A packed item holding no valid number, for cobc: whether its sign half-byte is D, and each digit
/// half-byte shown as the character '0' plus its value, as cobc's move to a display copy shows it.
fn cobc_bad_packed(facts: &dyn ProgramFacts, mem: &[u8], loc: Loc, digits: u32, signed: bool) -> Option<(bool, String)> {
    let bytes = store::bytes(mem, loc);
    if !facts.options().emulates_cobc() || !store::cobc_reads_otherwise(bytes, loc.kind) {
        return None;
    }
    let (&last, body) = bytes.split_last()?;
    let nibbles: Vec<u8> = body.iter().flat_map(|b| [b >> 4, b & 0x0F]).chain([last >> 4]).collect();
    let shown = nibbles[nibbles.len().saturating_sub(digits as usize)..].iter().map(|&n| char::from(b'0' + n)).collect();
    Some((signed && last & 0x0F == 0x0D, shown))
}

/// The digits that hold any value of a binary item of `len` bytes, 1 to 8: 3 for a BINARY-CHAR's
/// byte, 5, 10 or 20 for a halfword, fullword or doubleword. cobc -std=ibm-strict shows any binary
/// item in them, whatever its PICTURE.
pub const fn whole_binary_digits(len: usize) -> usize {
    [3, 5, 8, 10, 13, 15, 17, 20][len - 1]
}

/// A COMP-1 item as though moved to PICTURE -.9(8)E-99 and a COMP-2 one to -.9(17)E-99: the
/// leftmost digit after the point not zero, rounded, a blank before a positive mantissa or exponent
/// (Language Reference SC27-8713-03, pp. 214-215 and the DISPLAY statement; Programming Guide
/// SC27-8714-03, p. 52).
pub fn float(value: Hfp) -> String {
    let count = if value.precision == Precision::Short { 8 } else { 17 };
    let (negative, digits, first) = crate::json::significant_digits(value, count);
    let exponent = if digits.bytes().all(|d| d == b'0') { 0 } else { first + 1 };
    let sign = |minus: bool| if minus { '-' } else { ' ' };
    format!("{}.{digits}E{}{:02}", sign(negative), sign(exponent < 0), exponent.unsigned_abs())
}

/// Zoned digits after a separate sign, as DISPSIGN(SEP) shows a signed item.
fn sign_first(negative: bool, digits: Vec<u8>) -> Vec<u8> {
    let mut shown = vec![if negative { 0x60 } else { 0x4E }];
    shown.extend(digits);
    shown
}

/// A numeric literal as written, its decimal point the program's.
pub fn number(written: &str, facts: &dyn ProgramFacts) -> String {
    literal(written, facts.decimal_point(), facts.options().dialect_of(Switched::DecimalCommaDisplayLiteral))
}

/// DISPLAY's text for a numeric literal written with `.` as its decimal point: as written, the
/// point the program's, or under gnucobol without the point, as cobc shows it
/// (assumption C95).
pub fn literal(written: &str, decimal_point: char, dialect: Dialect) -> String {
    match dialect {
        Dialect::Ibm => written.replace('.', &decimal_point.to_string()),
        Dialect::Gnucobol => written.replace('.', ""),
    }
}

/// A literal, figurative constant, FUNCTION, LENGTH OF or ADDRESS OF, by its value; a national
/// value as `place` shows a national item.
pub fn value(facts: &dyn ProgramFacts, val: Val, pos: Pos, upon_console: bool) -> R<String> {
    Ok(match val {
        Val::Bytes(b) | Val::All(b) => facts.page().decode(&b),
        Val::National(b) | Val::AllNational(b) => national(facts.page(), &b, upon_console),
        Val::Dbcs(b) => facts.page().decode_dbcs(&b),
        Val::Fig(f) => facts.page().decode_byte(facts.figurative(f)).to_string(),
        Val::Num(f) => facts.page().decode(&zoned_digits(f.magnitude.to_u128().unwrap_or(0), f.places.total() as usize, decimal::UNSIGNED)),
        Val::Float(_) => return Err(crate::refusal::IWR0066.abend("DISPLAY of a floating-point value is not supported yet", pos)),
        Val::Address(_) => return Err(crate::refusal::IWR0067.abend("DISPLAY of a pointer is not supported", pos)),
    })
}

/// The line, and a newline unless NO ADVANCING.
pub fn write(out: &mut dyn Write, text: &str, no_advancing: bool, pos: Pos) -> R<()> {
    let result = if no_advancing { write!(out, "{text}") } else { writeln!(out, "{text}") };
    result.map_err(|e| match e.kind() {
        std::io::ErrorKind::BrokenPipe => Abend { code: AbendCode::Signal(Signal::ClosedOutput), message: "standard output closed".into(), pos, file: None },
        _ => Abend::ironwork(format!("DISPLAY: {e}"), pos),
    })
}

/// National data DISPLAY writes: UPON CONSOLE converted to the program's code page, otherwise its
/// bytes unconverted, which standard output shows as the code page's characters.
pub fn national(page: &CodePage, units: &[u8], upon_console: bool) -> String {
    if upon_console { page.decode(&to_page(page, units)) } else { page.decode(units) }
}

/// UTF-16 in a code page, a mixed page's DBCS characters between shift-out and shift-in and a
/// character the page does not hold given the substitution character X'3F', as UPON CONSOLE and
/// FUNCTION DISPLAY-OF convert it.
pub fn to_page(page: &CodePage, units: &[u8]) -> Vec<u8> {
    let (mut out, mut run) = (Vec::with_capacity(units.len()), String::new());
    for c in utf16_text(units).chars() {
        if page.encode(c.encode_utf8(&mut [0; 4])).is_ok() {
            run.push(c);
        } else {
            out.extend(page.encode(&std::mem::take(&mut run)).unwrap_or_default());
            out.push(SUBSTITUTE);
        }
    }
    out.extend(page.encode(&run).unwrap_or_default());
    out
}

/// EBCDIC's substitution character, which DISPLAY-OF gives for a character the code page lacks
/// (Language Reference SC27-8713-03, p. 551).
const SUBSTITUTE: u8 = 0x3F;

/// Big-endian UTF-16, an unpaired surrogate shown as U+FFFD.
pub fn utf16_text(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes.chunks(2).map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect();
    String::from_utf16_lossy(&units)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floating_point_items_show_as_the_external_float_picture_ibm_gives_them() {
        let short = |n: i128| Hfp::from_integer(n, Precision::Short);
        let long = |n: i128| Hfp::from_integer(n, Precision::Long);
        let tenth = |h: Hfp, d: i128| h.div(Hfp::from_integer(d, h.precision), Default::default()).unwrap();
        assert_eq!(float(tenth(short(15), 10)), " .15000000E 01");
        assert_eq!(float(long(-1234)), "-.12340000000000000E 04");
        assert_eq!(float(tenth(short(1), 1000)), " .99999993E-03");
        assert_eq!(float(tenth(long(1), 3)), " .33333333333333333E 00");
        assert_eq!(float(Hfp::zero(Precision::Short)), " .00000000E 00");
        assert_eq!(float(long(12_345)), " .12345000000000000E 05");
        assert_eq!(float(short(0x000F_FFFF_F000)), " .68719473E 11");
    }
}
