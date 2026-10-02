pub use rt::picture::Sym;
use syntax::ast::{CurrencySign, Environment};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Alphanumeric,
    Numeric,
    National,
    NumericEdited,
    AlphanumericEdited,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub category: Category,
    /// Character positions: bytes for alphanumeric, digits for numeric, characters for national.
    pub size: u32,
    pub digits: u32,
    pub scale: u32,
    pub signed: bool,
    pub edit: Option<Vec<Sym>>,
    /// Scaling positions P to the right of the digits: the value is the digits times ten to this
    /// power. Ps to the left of the digits raise `scale` past `digits` instead.
    pub scaling: u32,
    /// The currency sign value its currency symbol stands for.
    pub currency: Option<String>,
}

/// What SPECIAL-NAMES changes in a PICTURE: whether the comma is the decimal point, and which
/// characters are currency symbols, for which values.
#[derive(Clone, Copy, Debug, Default)]
pub struct Notation<'a> {
    pub decimal_comma: bool,
    pub currency: &'a [CurrencySign],
}

impl<'a> Notation<'a> {
    pub fn of(environment: &'a Environment) -> Self {
        Self { decimal_comma: environment.decimal_point_comma, currency: &environment.currency }
    }

    /// The value `symbol` stands for: with no CURRENCY SIGN clause or CURRENCY option, $ for $
    /// (Language Reference SC27-8713-03, p. 212).
    pub fn currency_value(&self, symbol: char) -> Option<&'a str> {
        match self.currency {
            [] => (symbol == '$').then_some("$"),
            signs => signs.iter().find(|c| c.symbol == symbol).map(|c| c.value.as_str()),
        }
    }
}

/// An edited PICTURE is written out position by position, so its length is bounded tighter.
const MAX_EDITED: u64 = 4096;

/// Enterprise COBOL's limit on an elementary item's character positions.
pub const MAX_POSITIONS: u64 = 134_217_727;

pub fn analyse(text: &str) -> Result<Picture, String> {
    analyse_with(text, Notation::default())
}

/// A PICTURE under `notation`. Under DECIMAL-POINT IS COMMA the comma is the decimal point and the
/// period an insertion character (Language Reference SC27-8713-03, p. 208); a currency symbol is
/// read as $, and the value it stands for kept.
pub fn analyse_with(text: &str, notation: Notation) -> Result<Picture, String> {
    let (runs, currency) = runs(text, notation)?;
    if runs.iter().any(|&(c, _)| matches!(c, 'Z' | '*' | '+' | '-' | '.' | ',' | 'B' | '0' | '/' | '$' | 'C' | 'R' | 'D')) {
        return edited(text, &runs, notation.decimal_comma, currency);
    }
    let (mut digits, mut scale, mut signed, mut after_point) = (0u64, 0u64, false, false);
    let (mut alnum, mut national) = (0u64, 0u64);
    let (mut left, mut right) = (0u64, 0u64);
    let misplaced = || Err(format!("PICTURE {text}: P must be one string of scaling positions at the left or right end of the digits"));
    for (i, &(c, n)) in runs.iter().enumerate() {
        match c {
            '9' if right > 0 => return misplaced(),
            '9' => {
                digits += n;
                if after_point {
                    scale += n;
                }
            }
            'S' if i == 0 && n == 1 => signed = true,
            'V' if left > 0 => return misplaced(),
            'V' if !after_point && n == 1 => after_point = true,
            'X' | 'A' => alnum += n,
            'N' => national += n,
            'P' if digits == 0 && runs.get(i + 1).is_some_and(|&(c, _)| matches!(c, 'P' | '9')) => left += n,
            'P' if digits > 0 && left == 0 && !after_point => right += n,
            'P' => return misplaced(),
            _ => return Err(format!("PICTURE {text}: {c:?} is not a PICTURE symbol")),
        }
    }
    if alnum + digits > MAX_POSITIONS || national > MAX_POSITIONS {
        return Err(format!("PICTURE {text}: more than {MAX_POSITIONS} character positions"));
    }
    let positions = digits + left + right;
    if left > 0 {
        scale = left + digits;
    }
    let (digits, scale, alnum, national) = (digits as u32, scale as u32, alnum as u32, national as u32);
    let scaled = left + right > 0;
    match (digits > 0, alnum > 0, national > 0) {
        (true, false, false) if positions <= 31 => Ok(Picture { category: Category::Numeric, size: digits, digits, scale, signed, edit: None, scaling: right as u32, currency: None }),
        (true, false, false) => Err(format!("PICTURE {text}: more than 31 digits")),
        (_, true, false) if !signed && !after_point && !scaled => Ok(Picture { category: Category::Alphanumeric, size: alnum + digits, digits: 0, scale: 0, signed, edit: None, scaling: 0, currency: None }),
        (false, false, true) if !signed && !after_point && !scaled => Ok(Picture { category: Category::National, size: national, digits: 0, scale: 0, signed, edit: None, scaling: 0, currency: None }),
        _ => Err(format!("PICTURE {text}: mixes symbols of different categories")),
    }
}

/// A numeric PICTURE under BLANK WHEN ZERO, which makes the item numeric-edited (Language
/// Reference SC27-8713-03, p. 195).
pub fn blank_when_zero(p: &Picture) -> Result<Picture, String> {
    if p.signed {
        return Err("BLANK WHEN ZERO cannot be given for a PICTURE with S".into());
    }
    let int = p.digits.saturating_sub(p.scale) as usize;
    let mut syms = vec![Sym::Nine; int];
    if p.scale > 0 {
        syms.push(Sym::Implied);
    }
    syms.extend(std::iter::repeat_n(Sym::Nine, p.digits as usize - int));
    Ok(Picture { category: Category::NumericEdited, edit: Some(syms), ..p.clone() })
}

fn edited(text: &str, runs: &[(char, u64)], decimal_comma: bool, currency: Option<&str>) -> Result<Picture, String> {
    let total: u64 = runs.iter().map(|&(_, n)| n).sum();
    if total > MAX_EDITED {
        return Err(format!("PICTURE {text}: an edited PICTURE longer than {MAX_EDITED} positions"));
    }
    let chars: Vec<char> = runs.iter().flat_map(|&(c, n)| std::iter::repeat_n(c, n as usize)).collect();
    let bad = |why: &str| Err(format!("PICTURE {text}: {why}"));
    if chars.iter().any(|c| matches!(c, 'S' | 'N')) {
        return bad("S and N are not allowed in an edited PICTURE");
    }
    if chars.iter().any(|c| matches!(c, 'X' | 'A')) {
        let mut syms = Vec::new();
        for &c in &chars {
            syms.push(match c {
                'X' | 'A' | '9' => Sym::Char,
                'B' => Sym::Insert(' '),
                '0' | '/' => Sym::Insert(c),
                _ => return bad("an alphanumeric-edited PICTURE takes only X, A, 9, B, 0 and /"),
            });
        }
        let size = syms.len() as u32;
        return Ok(Picture { category: Category::AlphanumericEdited, size, digits: 0, scale: 0, signed: false, edit: Some(syms), scaling: 0, currency: None });
    }
    let (point, comma) = if decimal_comma { (',', '.') } else { ('.', ',') };
    let floating: Vec<char> = ['+', '-', '$'].into_iter().filter(|f| chars.iter().filter(|c| *c == f).count() >= 2).collect();
    if floating.len() > 1 {
        return bad("two floating insertion strings");
    }
    let float = floating.first().copied();
    let (mut syms, mut led, mut i) = (Vec::new(), false, 0);
    let misplaced = "P must be one string of scaling positions at the left or right end of the digits";
    let mut scaling: Option<(usize, u32, usize)> = None;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == 'P' {
            match &mut scaling {
                Some((at, n, _)) if *at + *n as usize == i => *n += 1,
                Some(_) => return bad(misplaced),
                None => scaling = Some((i, 1, syms.len())),
            }
            i += 1;
            continue;
        }
        let sym = match c {
            'C' if next == Some('R') => {
                i += 1;
                Sym::Cr
            }
            'D' if next == Some('B') => {
                i += 1;
                Sym::Db
            }
            c if Some(c) == float && !led => {
                led = true;
                Sym::FloatLead(c)
            }
            c if Some(c) == float => Sym::Float(c),
            '+' | '-' => Sym::Sign(c),
            '$' => Sym::Currency,
            '9' => Sym::Nine,
            'Z' => Sym::Z,
            '*' => Sym::Star,
            c if c == point => Sym::Point,
            'V' => Sym::Implied,
            'B' => Sym::Insert(' '),
            '0' | '/' => Sym::Insert(c),
            c if c == comma => Sym::Insert(c),
            _ => return bad(&format!("{c:?} is not a numeric-edited symbol")),
        };
        syms.push(sym);
        i += 1;
    }
    let points = syms.iter().filter(|s| matches!(s, Sym::Point | Sym::Implied)).count();
    if points > 1 {
        return bad("more than one decimal point");
    }
    let digits = syms.iter().filter(|s| s.is_digit()).count() as u32;
    let mut scale = syms.iter().skip_while(|s| !matches!(s, Sym::Point | Sym::Implied)).filter(|s| s.is_digit()).count() as u32;
    let mut right = 0;
    if let Some((_, n, at)) = scaling {
        let before = syms[..at].iter().filter(|s| s.is_digit()).count() as u32;
        match (points, before) {
            (0, 0) => scale = n + digits,
            (0, b) if b == digits => right = n,
            _ => return bad(misplaced),
        }
    }
    if digits == 0 || digits + scaling.map_or(0, |(_, n, _)| n) > 31 {
        return bad("a numeric-edited PICTURE needs 1 to 31 digit positions");
    }
    let widths: u32 = syms.iter().map(|s| s.width() as u32).sum();
    let size = widths + currency.map_or(0, |v| v.chars().count() as u32 - 1);
    Ok(Picture { category: Category::NumericEdited, size, digits, scale, signed: false, edit: Some(syms), scaling: right, currency: currency.map(str::to_owned) })
}

/// Runs of one PICTURE symbol and a count, and the currency sign value the PICTURE uses.
type Runs<'a> = (Vec<(char, u64)>, Option<&'a str>);

/// The PICTURE as runs of one symbol and a count, `9(4)` read as four nines without writing them
/// out, its currency symbol as $, with the value that symbol stands for.
fn runs<'a>(text: &str, notation: Notation<'a>) -> Result<Runs<'a>, String> {
    let mut out: Vec<(char, u64)> = Vec::new();
    let (mut chars, mut currency) = (text.chars(), None);
    while let Some(c) = chars.next() {
        if c == '(' {
            let count: String = chars.by_ref().take_while(|&d| d != ')').collect();
            let n: u64 = count.parse().ok().filter(|&n| (1..=MAX_POSITIONS).contains(&n)).ok_or_else(|| format!("PICTURE {text}: bad repetition ({count})"))?;
            let last = out.last_mut().ok_or_else(|| format!("PICTURE {text}: a repetition with nothing to repeat"))?;
            last.1 += n - 1;
        } else if let Some(value) = notation.currency_value(c) {
            match currency {
                Some((symbol, _)) if symbol != c => return Err(format!("PICTURE {text}: two different currency symbols")),
                _ => currency = Some((c, value)),
            }
            out.push(('$', 1));
        } else if c == '$' {
            return Err(format!("PICTURE {text}: '$' is not a currency symbol of this program, whose CURRENCY SIGN clauses or CURRENCY option name others"));
        } else {
            out.push((c.to_ascii_uppercase(), 1));
        }
    }
    Ok((out, currency.map(|(_, value)| value)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMA: Notation = Notation { decimal_comma: true, currency: &[] };

    #[test]
    fn numeric_pictures() {
        assert_eq!(analyse("S9(3)V99").unwrap(), Picture { category: Category::Numeric, size: 5, digits: 5, scale: 2, signed: true, edit: None, scaling: 0, currency: None });
        assert_eq!(analyse("9(18)").unwrap().digits, 18);
        assert_eq!(analyse("SV9").unwrap().scale, 1);
    }

    #[test]
    fn edited_pictures() {
        let p = analyse("$$,$$9.99CR").unwrap();
        assert_eq!((p.category, p.size, p.digits, p.scale), (Category::NumericEdited, 11, 6, 2));
        assert_eq!(p.edit.as_ref().unwrap()[0], Sym::FloatLead('$'));
        let z = analyse("-ZZ,ZZ9").unwrap();
        assert_eq!(z.edit.unwrap()[0], Sym::Sign('-'));
        assert_eq!(analyse("XXBXX/99").unwrap().category, Category::AlphanumericEdited);
    }

    #[test]
    fn alphanumeric_and_national() {
        assert_eq!(analyse("X(512)").unwrap().size, 512);
        assert_eq!(analyse("N(256)").unwrap().category, Category::National);
    }

    #[test]
    fn unsupported_and_invalid_pictures_say_why() {
        assert!(analyse("ZZ9.99.9").unwrap_err().contains("decimal point"));
        assert!(analyse("9(32)").unwrap_err().contains("31 digits"));
        assert!(analyse("XN").unwrap_err().contains("categories"));
        assert!(analyse("X(999999999)").unwrap_err().contains("repetition"));
        assert!(analyse("X(134217727)X").unwrap_err().contains("character positions"));
        assert_eq!(analyse("XX99").unwrap(), Picture { category: Category::Alphanumeric, size: 4, digits: 0, scale: 0, signed: false, edit: None, scaling: 0, currency: None });
    }

    #[test]
    fn scaling_positions_left_of_the_digits_raise_the_scale_and_right_of_them_scale_the_value() {
        let shape = |pic: &str| analyse(pic).map(|p| (p.category, p.size, p.digits, p.scale, p.scaling));
        assert_eq!(shape("SP(8)9"), Ok((Category::Numeric, 1, 1, 9, 0)));
        assert_eq!(shape("VPP99"), Ok((Category::Numeric, 2, 2, 4, 0)));
        assert_eq!(shape("S999PP"), Ok((Category::Numeric, 3, 3, 0, 2)));
        assert_eq!(shape("99P(6)V"), Ok((Category::Numeric, 2, 2, 0, 6)));
        assert_eq!(shape("ZZZPP"), Ok((Category::NumericEdited, 3, 3, 0, 2)));
        for bad in ["P9P", "9P9", "PPV9", "9V9P", "99VPP", "Z.ZPP", "P(31)9"] {
            assert!(analyse(bad).is_err(), "{bad}");
        }
        assert!(analyse("P(30)9").is_ok());
    }

    #[test]
    fn under_decimal_point_is_comma_the_comma_is_the_point_and_the_period_an_insertion() {
        let p = analyse_with("Z.ZZ9,99", COMMA).unwrap();
        assert_eq!((p.category, p.size, p.digits, p.scale), (Category::NumericEdited, 8, 6, 2));
        assert_eq!(p.edit.as_ref().unwrap()[1], Sym::Insert('.'));
        assert_eq!(p.edit.as_ref().unwrap()[5], Sym::Point);
        assert_eq!(analyse_with("ZZ9.99", Notation::default()).unwrap().scale, 2);
        assert_eq!(analyse_with("ZZ9.99", COMMA).unwrap().scale, 0);
        assert!(analyse_with("9,99,9", COMMA).unwrap_err().contains("decimal point"));
    }

    #[test]
    fn a_picture_can_end_in_an_insertion_comma_or_the_decimal_point() {
        let edit = |pic: &str, notation: Notation, value: u128| {
            let p = analyse_with(pic, notation).unwrap();
            let point = if notation.decimal_comma { ',' } else { '.' };
            ((p.category, p.size, p.digits, p.scale), rt::edit::numeric(p.edit.as_ref().unwrap(), p.digits, false, value, false, point, ""))
        };
        assert_eq!(edit("9,9,9,", Notation::default(), 123), ((Category::NumericEdited, 6, 3, 0), "1,2,3,".into()));
        assert_eq!(edit("999.", Notation::default(), 7), ((Category::NumericEdited, 4, 3, 0), "007.".into()));
        assert_eq!(edit("9.9.9,", COMMA, 123), ((Category::NumericEdited, 6, 3, 0), "1.2.3,".into()));
        assert_eq!(edit("999.", COMMA, 7), ((Category::NumericEdited, 4, 3, 0), "007.".into()));
        assert_eq!(analyse_with("999.", COMMA).unwrap().edit.unwrap()[3], Sym::Insert('.'));
    }

    #[test]
    fn a_currency_sign_clause_names_the_symbol_and_the_value_it_stands_for() {
        let signs = [CurrencySign { value: "W".into(), symbol: 'W', hex: None }, CurrencySign { value: "EUR ".into(), symbol: 'e', hex: None }];
        let notation = Notation { decimal_comma: true, currency: &signs };
        let w = analyse_with("W9.999,99", notation).unwrap();
        assert_eq!((w.category, w.size, w.digits, w.scale, w.currency.as_deref()), (Category::NumericEdited, 9, 6, 2, Some("W")));
        assert_eq!(w.edit.as_ref().unwrap()[0], Sym::Currency);
        let floating = analyse_with("eeee9,99", notation).unwrap();
        assert_eq!((floating.size, floating.digits, floating.currency.as_deref()), (11, 6, Some("EUR ")));
        assert_eq!(floating.edit.as_ref().unwrap()[0], Sym::FloatLead('$'));
        assert!(analyse_with("$$9", notation).unwrap_err().contains("'$'"));
        assert!(analyse_with("We9", notation).unwrap_err().contains("two different currency symbols"));
        assert!(analyse_with("E9", notation).unwrap_err().contains("not a PICTURE symbol"));
        assert_eq!(analyse("$9").unwrap().currency.as_deref(), Some("$"));
    }

    #[test]
    fn blank_when_zero_makes_a_numeric_picture_numeric_edited() {
        let p = blank_when_zero(&analyse("99V9").unwrap()).unwrap();
        assert_eq!((p.category, p.size, p.digits, p.scale), (Category::NumericEdited, 3, 3, 1));
        assert_eq!(p.edit.unwrap(), [Sym::Nine, Sym::Nine, Sym::Implied, Sym::Nine]);
        assert_eq!(blank_when_zero(&analyse("VPP99").unwrap()).unwrap().edit.unwrap(), [Sym::Implied, Sym::Nine, Sym::Nine]);
        assert!(blank_when_zero(&analyse("S99").unwrap()).unwrap_err().contains("with S"));
    }
}
