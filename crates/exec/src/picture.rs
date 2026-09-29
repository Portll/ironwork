#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Alphanumeric,
    Numeric,
    National,
    NumericEdited,
    AlphanumericEdited,
}

/// One position of an edited PICTURE.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sym {
    Nine,
    Z,
    Star,
    /// The first symbol of a floating insertion string: a sign or currency position, not a digit.
    FloatLead(char),
    /// A later symbol of a floating insertion string: a digit position.
    Float(char),
    /// A fixed + or -.
    Sign(char),
    Currency,
    Cr,
    Db,
    Point,
    /// V: the decimal point, occupying no position.
    Implied,
    /// B (as a space), 0, / or a comma.
    Insert(char),
    /// X, A or 9 in an alphanumeric-edited PICTURE.
    Char,
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
}

/// An edited PICTURE is written out position by position, so its length is bounded tighter.
const MAX_EDITED: u64 = 4096;

/// Enterprise COBOL's limit on an elementary item's character positions.
pub const MAX_POSITIONS: u64 = 134_217_727;

pub fn analyse(text: &str) -> Result<Picture, String> {
    let runs = runs(text)?;
    let (mut digits, mut scale, mut signed, mut after_point) = (0u64, 0u64, false, false);
    let (mut alnum, mut national) = (0u64, 0u64);
    for (i, &(c, n)) in runs.iter().enumerate() {
        match c {
            '9' => {
                digits += n;
                if after_point {
                    scale += n;
                }
            }
            'S' if i == 0 && n == 1 => signed = true,
            'V' if !after_point && n == 1 => after_point = true,
            'X' | 'A' => alnum += n,
            'N' => national += n,
            'P' => return Err(format!("PICTURE {text}: scaling position P is not supported yet")),
            'Z' | '*' | '+' | '-' | '.' | ',' | 'B' | '0' | '/' | '$' | 'C' | 'R' | 'D' => return edited(text, &runs),
            _ => return Err(format!("PICTURE {text}: {c:?} is not a PICTURE symbol")),
        }
    }
    if alnum + digits > MAX_POSITIONS || national > MAX_POSITIONS {
        return Err(format!("PICTURE {text}: more than {MAX_POSITIONS} character positions"));
    }
    let (digits, scale, alnum, national) = (digits as u32, scale as u32, alnum as u32, national as u32);
    match (digits > 0, alnum > 0, national > 0) {
        (true, false, false) if digits <= 31 => Ok(Picture { category: Category::Numeric, size: digits, digits, scale, signed, edit: None }),
        (true, false, false) => Err(format!("PICTURE {text}: more than 31 digits")),
        (_, true, false) if !signed && !after_point => Ok(Picture { category: Category::Alphanumeric, size: alnum + digits, digits: 0, scale: 0, signed, edit: None }),
        (false, false, true) if !signed && !after_point => Ok(Picture { category: Category::National, size: national, digits: 0, scale: 0, signed, edit: None }),
        _ => Err(format!("PICTURE {text}: mixes symbols of different categories")),
    }
}

fn edited(text: &str, runs: &[(char, u64)]) -> Result<Picture, String> {
    let total: u64 = runs.iter().map(|&(_, n)| n).sum();
    if total > MAX_EDITED {
        return Err(format!("PICTURE {text}: an edited PICTURE longer than {MAX_EDITED} positions"));
    }
    let chars: Vec<char> = runs.iter().flat_map(|&(c, n)| std::iter::repeat_n(c, n as usize)).collect();
    let bad = |why: &str| Err(format!("PICTURE {text}: {why}"));
    if chars.iter().any(|c| matches!(c, 'S' | 'N' | 'P')) {
        return bad("S, N and P are not allowed in an edited PICTURE");
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
        return Ok(Picture { category: Category::AlphanumericEdited, size, digits: 0, scale: 0, signed: false, edit: Some(syms) });
    }
    let floating: Vec<char> = ['+', '-', '$'].into_iter().filter(|f| chars.iter().filter(|c| *c == f).count() >= 2).collect();
    if floating.len() > 1 {
        return bad("two floating insertion strings");
    }
    let float = floating.first().copied();
    let (mut syms, mut led, mut i) = (Vec::new(), false, 0);
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
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
            '.' => Sym::Point,
            'V' => Sym::Implied,
            'B' => Sym::Insert(' '),
            '0' | '/' | ',' => Sym::Insert(c),
            _ => return bad(&format!("{c:?} is not a numeric-edited symbol")),
        };
        syms.push(sym);
        i += 1;
    }
    let points = syms.iter().filter(|s| matches!(s, Sym::Point | Sym::Implied)).count();
    if points > 1 {
        return bad("more than one decimal point");
    }
    let is_digit = |s: &Sym| matches!(s, Sym::Nine | Sym::Z | Sym::Star | Sym::Float(_));
    let digits = syms.iter().filter(|s| is_digit(s)).count() as u32;
    let scale = syms.iter().skip_while(|s| !matches!(s, Sym::Point | Sym::Implied)).filter(|s| is_digit(s)).count() as u32;
    if digits == 0 || digits > 31 {
        return bad("a numeric-edited PICTURE needs 1 to 31 digit positions");
    }
    let size = syms.iter().map(|s| match s {
        Sym::Implied => 0,
        Sym::Cr | Sym::Db => 2,
        _ => 1,
    }).sum();
    Ok(Picture { category: Category::NumericEdited, size, digits, scale, signed: false, edit: Some(syms) })
}

/// The PICTURE as runs of one symbol and a count, `9(4)` read as four nines without writing them out.
fn runs(text: &str) -> Result<Vec<(char, u64)>, String> {
    let mut out: Vec<(char, u64)> = Vec::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '(' {
            let count: String = chars.by_ref().take_while(|&d| d != ')').collect();
            let n: u64 = count.parse().ok().filter(|&n| (1..=MAX_POSITIONS).contains(&n)).ok_or_else(|| format!("PICTURE {text}: bad repetition ({count})"))?;
            let last = out.last_mut().ok_or_else(|| format!("PICTURE {text}: a repetition with nothing to repeat"))?;
            last.1 += n - 1;
        } else {
            out.push((c.to_ascii_uppercase(), 1));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_pictures() {
        assert_eq!(analyse("S9(3)V99").unwrap(), Picture { category: Category::Numeric, size: 5, digits: 5, scale: 2, signed: true, edit: None });
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
        assert_eq!(analyse("XX99").unwrap(), Picture { category: Category::Alphanumeric, size: 4, digits: 0, scale: 0, signed: false, edit: None });
    }
}
