//! The argument rules of NUMVAL, NUMVAL-C and NUMVAL-F (Language Reference SC27-8713-03,
//! pp. 605-609), which TEST-NUMVAL, TEST-NUMVAL-C and TEST-NUMVAL-F check (pp. 651-655).

use super::real::Real;
use numeric::precision::{Fixed, Places};
use std::ops::{Div, Mul};
use zarch::wide::U256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form<'a> {
    Numval,
    /// NUMVAL-C with its currency string.
    Currency(&'a str),
    /// NUMVAL-F.
    Exponent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Number {
    pub negative: bool,
    /// The digits as an integer, with `decimals` of them after the decimal point.
    pub digits: u128,
    pub decimals: u32,
    pub exponent: i32,
}

impl Number {
    /// digits × 10^(exponent - decimals), rounded once where 10^k is exact (k up to 38).
    pub fn to_real(&self) -> Real {
        let value = Real::new(self.negative, U256::from_u128(self.digits), 0);
        let power = self.exponent - self.decimals as i32;
        if value.is_zero() {
            return value;
        }
        match power {
            ..-200 => Real::ZERO,
            201.. => Real::huge(),
            _ => {
                let mut scale = Real::ONE;
                let mut left = power.unsigned_abs();
                while left > 0 {
                    let step = left.min(38);
                    scale = scale.mul(Real::from_u128(10u128.pow(step)));
                    left -= step;
                }
                if power >= 0 { value.mul(scale) } else { value.div(scale) }
            }
        }
    }
}

struct Scan<'a> {
    text: &'a [char],
    at: usize,
}

impl Scan<'_> {
    fn peek(&self) -> Option<char> {
        self.text.get(self.at).copied()
    }

    fn spaces(&mut self) -> bool {
        let start = self.at;
        while self.peek() == Some(' ') {
            self.at += 1;
        }
        self.at > start
    }

    fn take(&mut self, s: &str) -> bool {
        let chars: Vec<char> = s.chars().collect();
        if !chars.is_empty() && self.text[self.at..].starts_with(&chars) {
            self.at += chars.len();
            return true;
        }
        false
    }

    fn take_any_case(&mut self, s: &str) -> bool {
        let n = s.chars().count();
        let found: String = self.text[self.at..].iter().take(n).collect();
        if found.len() == s.len() && found.eq_ignore_ascii_case(s) {
            self.at += n;
            return true;
        }
        false
    }

    /// The 1-based position at the cursor: the character in error, or one past the end when the
    /// string stops short.
    fn error(&self) -> usize {
        self.at + 1
    }
}

/// The number `text` holds under `form`, or the 1-based position of its first character in error.
/// `max_digits` is 18 under ARITH(COMPAT) and 31 under ARITH(EXTEND); `decimal_comma` swaps the
/// roles of the period and the comma.
pub fn parse(text: &str, form: Form, max_digits: usize, decimal_comma: bool) -> Result<Number, usize> {
    let chars: Vec<char> = text.chars().collect();
    let mut s = Scan { text: &chars, at: 0 };
    let (point, grouping) = if decimal_comma { (',', '.') } else { ('.', ',') };
    s.spaces();
    let mut negative = None;
    if let Some(c @ ('+' | '-')) = s.peek() {
        negative = Some(c == '-');
        s.at += 1;
        s.spaces();
    }
    if let Form::Currency(cs) = form
        && s.take(cs)
    {
        s.spaces();
    }
    let (mut digits, mut count, mut decimals, mut seen_point) = (0u128, 0usize, 0u32, false);
    loop {
        match s.peek() {
            Some(d @ '0'..='9') => {
                count += 1;
                if count > max_digits {
                    return Err(s.error());
                }
                digits = digits * 10 + d as u128 - '0' as u128;
                decimals += seen_point as u32;
            }
            Some(c) if c == point && !seen_point => seen_point = true,
            Some(c) if c == grouping && matches!(form, Form::Currency(_)) && !seen_point && count > 0 && s.text.get(s.at + 1).is_some_and(char::is_ascii_digit) => {}
            _ => break,
        }
        s.at += 1;
    }
    if count == 0 {
        return Err(if s.at == chars.len() || s.peek() == Some(' ') { chars.len() + 1 } else { s.error() });
    }
    let mut exponent = 0i32;
    if form == Form::Exponent {
        let before = s.at;
        s.spaces();
        if s.take_any_case("E") {
            s.spaces();
            let mut exponent_negative = false;
            if let Some(c @ ('+' | '-')) = s.peek() {
                exponent_negative = c == '-';
                s.at += 1;
                s.spaces();
            }
            let mut n = 0usize;
            while let Some(d @ '0'..='9') = s.peek() {
                n += 1;
                if n > 4 {
                    return Err(s.error());
                }
                exponent = exponent * 10 + (d as i32 - '0' as i32);
                s.at += 1;
            }
            if n == 0 {
                return Err(if s.at == chars.len() { chars.len() + 1 } else { s.error() });
            }
            if exponent_negative {
                exponent = -exponent;
            }
        } else {
            s.at = before;
        }
    }
    s.spaces();
    if negative.is_none() && form != Form::Exponent {
        if s.take("+") {
            negative = Some(false);
        } else if s.take("-") || s.take_any_case("CR") || s.take_any_case("DB") {
            negative = Some(true);
        }
        s.spaces();
    }
    if s.at != chars.len() {
        return Err(s.error());
    }
    Ok(Number { negative: negative.unwrap_or(false), digits, decimals, exponent })
}

/// TEST-NUMVAL, TEST-NUMVAL-C and TEST-NUMVAL-F.
pub fn test(text: &str, form: Form, max_digits: usize, decimal_comma: bool) -> usize {
    parse(text, form, max_digits, decimal_comma).err().unwrap_or(0)
}

/// NUMVAL and NUMVAL-C as fixed point: spaces, one sign (leading + or -, trailing + - CR or DB),
/// digits with at most one decimal point; NUMVAL-C also allows the currency sign and commas. None
/// when the text is anything else.
pub fn fixed(text: &str, currency: Option<&str>) -> Option<Fixed> {
    let mut t = text.trim().to_ascii_uppercase();
    let mut negative = false;
    for (suffix, minus) in [("CR", true), ("DB", true), ("-", true), ("+", false)] {
        if let Some(rest) = t.strip_suffix(suffix) {
            negative = minus;
            t = rest.trim_end().to_owned();
            break;
        }
    }
    if let Some(rest) = t.strip_prefix('-') {
        negative = true;
        t = rest.trim_start().to_owned();
    } else if let Some(rest) = t.strip_prefix('+') {
        t = rest.trim_start().to_owned();
    }
    if let Some(c) = currency {
        t = t.trim_start_matches(c.trim()).trim_start().replace(',', "");
    }
    let (int, frac) = t.split_once('.').unwrap_or((&t, ""));
    if int.is_empty() && frac.is_empty() || !int.chars().chain(frac.chars()).all(|c| c.is_ascii_digit()) || int.len() + frac.len() > 31 {
        return None;
    }
    let digits: u128 = format!("{int}{frac}").parse().unwrap_or(0);
    let f = Fixed::new(digits as i128, Places::new(int.len().max(1) as u32, frac.len() as u32));
    Some(if negative { Fixed { negative: digits != 0, ..f } } else { f })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(text: &str) -> usize {
        test(text, Form::Numval, 18, false)
    }

    #[test]
    fn test_numval_gives_the_first_character_in_error() {
        assert_eq!(t("- 1234.5678"), 0);
        assert_eq!(t("  12.50  "), 0);
        assert_eq!(t("12.5-"), 0);
        assert_eq!(t("12.5 CR"), 0);
        assert_eq!(t(".5"), 0);
        assert_eq!(t("5."), 0);
        assert_eq!(t("0 1"), 3);
        assert_eq!(t("1A"), 2);
        assert_eq!(t("-12-"), 4);
        assert_eq!(t(""), 1);
        assert_eq!(t("   "), 4);
        assert_eq!(t(" +."), 4);
        assert_eq!(t("1234567890123456789"), 19);
        assert_eq!(test("1234567890123456789", Form::Numval, 31, false), 0);
        assert_eq!(t("1,000"), 2);
        assert_eq!(test("1.000,5", Form::Numval, 18, true), 2);
    }

    #[test]
    fn numval_c_takes_a_currency_string_and_grouping_commas() {
        let c = |text: &str| test(text, Form::Currency("$"), 18, false);
        assert_eq!(c("  $12,345.67CR"), 0);
        assert_eq!(c("-$1,000"), 0);
        assert_eq!(c("$ 12"), 0);
        assert_eq!(c("12$"), 3);
        assert_eq!(c("1,,0"), 2);
        assert_eq!(test("CHF 12.00", Form::Currency("CHF"), 18, false), 0);
        assert_eq!(test("chf 12.00", Form::Currency("CHF"), 18, false), 1);
        let n = parse("  $12,345.67CR", Form::Currency("$"), 18, false).unwrap();
        assert_eq!((n.negative, n.digits, n.decimals), (true, 1_234_567, 2));
    }

    #[test]
    fn numval_f_takes_an_exponent_of_up_to_four_digits() {
        let f = |text: &str| test(text, Form::Exponent, 18, false);
        assert_eq!(f("+ 12.345678E+2"), 0);
        assert_eq!(f("1.5E-3"), 0);
        assert_eq!(f("1.5e3"), 0);
        assert_eq!(f("1.5E+12345"), 10);
        assert_eq!(f("1.5E"), 5);
        assert_eq!(f("1.5-"), 4);
        let n = parse("+ 12.345678E+2", Form::Exponent, 18, false).unwrap();
        assert_eq!((n.negative, n.digits, n.decimals, n.exponent), (false, 12_345_678, 6, 2));
    }
}
