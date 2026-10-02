//! JSON text as JSON PARSE reads it (Language Reference SC27-8713-03, pp. 384-396): RFC 8259's
//! grammar, with IBM's escape \x for NEXT LINE (U+0085). JSON-CODE and JSON-STATUS values from the
//! Programming Guide SC27-8714-03, pp. 819-821.

use super::{CCSIDS, UTF8, UTF16};
use crate::display::utf16_text;
use crate::storage::literal_fixed;
use numeric::precision::Fixed;
use zarch::ebcdic::CodePage;

pub const DIFFERENT_DUPLICATE: i64 = 103;
pub const INCOMPATIBLE: i64 = 104;
pub const NO_MATCH: i64 = 106;
pub const UNCONVERTED_BOOLEAN: i64 = 107;
pub const ANONYMOUS_ARRAY: i64 = 108;
pub const PARSE_ENCODING: i64 = 109;

pub const UNMATCHED_ITEM: i64 = 1;
pub const UNMATCHED_NAME: i64 = 2;
pub const SAME_DUPLICATE: i64 = 4;
pub const SHORT_ARRAY: i64 = 8;
pub const LONG_ARRAY: i64 = 16;
pub const NULL_ITEM: i64 = 32;
pub const NULL_ELEMENT: i64 = 64;
pub const SIZE_ERROR: i64 = 128;
pub const LOST: i64 = 256;
pub const SUBSTITUTED: i64 = 512;

/// EBCDIC's SUB, for a character the code page lacks.
pub const SUB: u8 = 0x3F;

/// The source's text: UTF-8 unless the CCSID ENCODING names is an EBCDIC code page, UTF-16 when
/// the source is national; the JSON-CODE of an encoding that cannot be read.
pub fn text(bytes: Vec<u8>, national: bool, ccsid: Option<u16>) -> Result<String, i64> {
    match (national, ccsid) {
        (true, None | Some(UTF16)) => Ok(utf16_text(&bytes)),
        (false, None | Some(UTF8)) => String::from_utf8(bytes).map_err(|_| Invalid::Malformed.code()),
        (false, Some(c)) if CCSIDS.contains(&c) => CodePage::by_ccsid(c).map(|page| page.decode(&bytes)).ok_or(PARSE_ENCODING),
        _ => Err(PARSE_ENCODING),
    }
}

/// A decimal value of at most 31 digits, and whether high-order integer digits had to go.
pub fn fixed_value(negative: bool, int: &str, frac: &str) -> (Fixed, bool) {
    let frac = &frac[..frac.len().min(31)];
    let room = 31 - frac.len();
    let cut = int.len() > room;
    let int = if cut { &int[int.len() - room..] } else { int };
    let text = format!("{}{}.{frac}", if negative { "-" } else { "" }, if int.is_empty() { "0" } else { int });
    (literal_fixed(text.trim_end_matches('.')).expect("at most 31 digits"), cut)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// Name/value pairs in the order of the text.
    Object(Vec<(String, Value)>),
    Array(Vec<Value>),
    String(String),
    /// A number as written.
    Number(String),
    Bool(bool),
    Null,
}

/// Why JSON text is refused: JSON-CODE 100, 101 and 102.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Invalid {
    Malformed,
    Empty,
    Trailing,
}

impl Invalid {
    pub fn code(self) -> i64 {
        match self {
            Invalid::Malformed => 100,
            Invalid::Empty => 101,
            Invalid::Trailing => 102,
        }
    }
}

struct Reader<'t> {
    text: &'t [char],
    at: usize,
}

type Read<T> = Result<T, Invalid>;

fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

impl Reader<'_> {
    fn skip_space(&mut self) {
        while self.text.get(self.at).is_some_and(|&c| is_space(c)) {
            self.at += 1;
        }
    }

    fn next(&mut self) -> Read<char> {
        let c = *self.text.get(self.at).ok_or(Invalid::Malformed)?;
        self.at += 1;
        Ok(c)
    }

    fn expect(&mut self, word: &str) -> Read<()> {
        for w in word.chars() {
            if self.next()? != w {
                return Err(Invalid::Malformed);
            }
        }
        Ok(())
    }

    fn value(&mut self) -> Read<Value> {
        self.skip_space();
        let value = match self.text.get(self.at).ok_or(Invalid::Malformed)? {
            '{' => self.object()?,
            '[' => self.array()?,
            '"' => Value::String(self.string()?),
            't' => {
                self.expect("true")?;
                Value::Bool(true)
            }
            'f' => {
                self.expect("false")?;
                Value::Bool(false)
            }
            'n' => {
                self.expect("null")?;
                Value::Null
            }
            _ => Value::Number(self.number()?),
        };
        self.skip_space();
        Ok(value)
    }

    fn object(&mut self) -> Read<Value> {
        self.at += 1;
        let mut pairs = Vec::new();
        self.skip_space();
        if self.text.get(self.at) == Some(&'}') {
            self.at += 1;
            return Ok(Value::Object(pairs));
        }
        loop {
            self.skip_space();
            if self.text.get(self.at) != Some(&'"') {
                return Err(Invalid::Malformed);
            }
            let name = self.string()?;
            self.skip_space();
            self.expect(":")?;
            pairs.push((name, self.value()?));
            match self.next()? {
                ',' => {}
                '}' => return Ok(Value::Object(pairs)),
                _ => return Err(Invalid::Malformed),
            }
        }
    }

    fn array(&mut self) -> Read<Value> {
        self.at += 1;
        let mut elements = Vec::new();
        self.skip_space();
        if self.text.get(self.at) == Some(&']') {
            self.at += 1;
            return Ok(Value::Array(elements));
        }
        loop {
            elements.push(self.value()?);
            match self.next()? {
                ',' => {}
                ']' => return Ok(Value::Array(elements)),
                _ => return Err(Invalid::Malformed),
            }
        }
    }

    fn hex4(&mut self) -> Read<u32> {
        let mut unit = 0;
        for _ in 0..4 {
            unit = unit * 16 + self.next()?.to_digit(16).ok_or(Invalid::Malformed)?;
        }
        Ok(unit)
    }

    fn string(&mut self) -> Read<String> {
        self.at += 1;
        let mut out = String::new();
        loop {
            match self.next()? {
                '"' => return Ok(out),
                '\\' => out.push(match self.next()? {
                    '"' => '"',
                    '\\' => '\\',
                    '/' => '/',
                    'b' => '\u{8}',
                    'f' => '\u{c}',
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    'x' => '\u{85}',
                    'u' => {
                        let unit = self.hex4()?;
                        let scalar = if (0xD800..0xDC00).contains(&unit) {
                            self.expect("\\u")?;
                            let low = self.hex4()?;
                            if !(0xDC00..0xE000).contains(&low) {
                                return Err(Invalid::Malformed);
                            }
                            0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00)
                        } else {
                            unit
                        };
                        char::from_u32(scalar).ok_or(Invalid::Malformed)?
                    }
                    _ => return Err(Invalid::Malformed),
                }),
                c if (c as u32) < 0x20 => return Err(Invalid::Malformed),
                c => out.push(c),
            }
        }
    }

    fn digits(&mut self) -> usize {
        let start = self.at;
        while self.text.get(self.at).is_some_and(char::is_ascii_digit) {
            self.at += 1;
        }
        self.at - start
    }

    fn number(&mut self) -> Read<String> {
        let start = self.at;
        if self.text.get(self.at) == Some(&'-') {
            self.at += 1;
        }
        let first = self.text.get(self.at).copied();
        let whole = self.digits();
        if whole == 0 || (first == Some('0') && whole > 1) {
            return Err(Invalid::Malformed);
        }
        if self.text.get(self.at) == Some(&'.') {
            self.at += 1;
            if self.digits() == 0 {
                return Err(Invalid::Malformed);
            }
        }
        if matches!(self.text.get(self.at), Some('e' | 'E')) {
            self.at += 1;
            if matches!(self.text.get(self.at), Some('+' | '-')) {
                self.at += 1;
            }
            if self.digits() == 0 {
                return Err(Invalid::Malformed);
            }
        }
        Ok(self.text[start..self.at].iter().collect())
    }
}

/// The value JSON text holds; nothing but whitespace may follow it.
pub fn parse(text: &str) -> Result<Value, Invalid> {
    let chars: Vec<char> = text.chars().collect();
    if chars.iter().all(|&c| is_space(c)) {
        return Err(Invalid::Empty);
    }
    let mut reader = Reader { text: &chars, at: 0 };
    let value = reader.value()?;
    if reader.at < chars.len() {
        return Err(Invalid::Trailing);
    }
    Ok(value)
}

/// How many zeros an exponent may add on either side of a number's digits: past them it is too
/// large or too small for any receiver.
const MOST_ZEROS: i64 = 100;

/// A number's sign, integer digits without leading zeros and fraction digits, its exponent applied.
pub fn decimal(number: &str) -> (bool, String, String) {
    let (negative, body) = match number.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, number),
    };
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(e) => {
            let written = &body[e + 1..];
            (&body[..e], written.parse::<i64>().unwrap_or(if written.starts_with('-') { i64::MIN } else { i64::MAX }))
        }
        None => (body, 0),
    };
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = format!("{int}{frac}");
    let point = (int.len() as i64).saturating_add(exponent).clamp(-MOST_ZEROS, digits.len() as i64 + MOST_ZEROS);
    let (int, frac) = if point <= 0 {
        (String::new(), format!("{}{digits}", "0".repeat(point.unsigned_abs() as usize)))
    } else if point as usize >= digits.len() {
        (format!("{digits}{}", "0".repeat(point as usize - digits.len())), String::new())
    } else {
        (digits[..point as usize].to_owned(), digits[point as usize..].to_owned())
    };
    (negative, int.trim_start_matches('0').to_owned(), frac)
}

/// A string JSON PARSE takes as a number (p. 396): spaces, a sign, digits with at most one decimal
/// point among or before them, and spaces; as sign, integer digits and fraction digits.
pub fn numeric_string(text: &str, point: char) -> Option<(bool, String, String)> {
    let body = text.trim_matches(' ');
    let (negative, body) = match body.as_bytes().first() {
        Some(b'-') => (true, &body[1..]),
        Some(b'+') => (false, &body[1..]),
        _ => (false, body),
    };
    let (int, frac) = body.split_once(point).unwrap_or((body, ""));
    let all_digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    (!(int.is_empty() && frac.is_empty()) && all_digits(int) && all_digits(frac)).then(|| (negative, int.trim_start_matches('0').to_owned(), frac.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(t: &str) -> Value {
        Value::String(t.into())
    }

    #[test]
    fn objects_arrays_and_scalars_parse_in_order() {
        let v = parse(" {\"g\": {\"A\": \"Eh?\", \"3_\": 5, \"t\": [true, false, null, -1.5e2]}} ").unwrap();
        let inner = Value::Object(vec![
            ("A".into(), s("Eh?")),
            ("3_".into(), Value::Number("5".into())),
            ("t".into(), Value::Array(vec![Value::Bool(true), Value::Bool(false), Value::Null, Value::Number("-1.5e2".into())])),
        ]);
        assert_eq!(v, Value::Object(vec![("g".into(), inner)]));
    }

    #[test]
    fn escapes_include_ibms_next_line() {
        assert_eq!(parse(r#"["a\"b\\c\/\né\x😀"]"#).unwrap(), Value::Array(vec![s("a\"b\\c/\né\u{85}😀")]));
    }

    #[test]
    fn bad_text_gives_its_code() {
        assert_eq!(parse("   "), Err(Invalid::Empty));
        assert_eq!(parse("{\"a\":1} x"), Err(Invalid::Trailing));
        for bad in ["{\"a\":01}", "{\"a\":1,}", "{a:1}", "[\"\u{1}\"]", "{\"a\":tru}", "{\"a\":.5}", "{\"a\":1."] {
            assert_eq!(parse(bad), Err(Invalid::Malformed), "{bad}");
        }
    }

    #[test]
    fn numbers_become_digits_either_side_of_the_point() {
        assert_eq!(decimal("-125.53"), (true, "125".into(), "53".into()));
        assert_eq!(decimal("1.5E+3"), (false, "1500".into(), String::new()));
        assert_eq!(decimal("12e-3"), (false, String::new(), "012".into()));
        assert_eq!(decimal("0"), (false, String::new(), String::new()));
        assert_eq!(numeric_string(" -1,234 ", ','), Some((true, "1".into(), "234".into())));
        assert_eq!(numeric_string("0042", '.'), Some((false, "42".into(), String::new())));
        assert_eq!(numeric_string("4x", '.'), None);
        assert_eq!(numeric_string(" ", '.'), None);
    }
}
