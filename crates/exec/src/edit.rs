//! Editing: a numeric value laid into a numeric-edited PICTURE, alphanumeric data into an
//! alphanumeric-edited one, and de-editing back to a number.

use crate::picture::Sym;

fn is_digit(s: &Sym) -> bool {
    matches!(s, Sym::Nine | Sym::Z | Sym::Star | Sym::Float(_))
}

/// The characters a numeric-edited item holds for a value. `magnitude` is already aligned to the
/// PICTURE's decimal places and within its digit positions.
pub fn numeric(syms: &[Sym], digits: u32, negative: bool, magnitude: u128, blank_when_zero: bool) -> String {
    let size: usize = syms.iter().map(|s| match s {
        Sym::Implied => 0,
        Sym::Cr | Sym::Db => 2,
        _ => 1,
    }).sum();
    let digit_syms: Vec<&Sym> = syms.iter().filter(|s| is_digit(s)).collect();
    if magnitude == 0 && (blank_when_zero || digit_syms.iter().all(|s| matches!(s, Sym::Z | Sym::Float(_)))) {
        return " ".repeat(size);
    }
    if magnitude == 0 && digit_syms.iter().all(|s| matches!(s, Sym::Star)) {
        return syms.iter().filter(|s| !matches!(s, Sym::Implied)).map(|s| if *s == Sym::Point { '.' } else { '*' }).collect();
    }
    let text = format!("{magnitude:0width$}", width = digits as usize);
    let mut digit_chars = text.chars().skip(text.len() - digits as usize);
    let mut out: Vec<char> = Vec::with_capacity(size);
    let (mut significant, mut fill) = (false, ' ');
    let (mut float_char, mut float_slot) = (None, None);
    for s in syms {
        match *s {
            Sym::Nine => {
                significant = true;
                out.push(digit_chars.next().unwrap_or('0'));
            }
            Sym::Z | Sym::Star => {
                fill = if *s == Sym::Star { '*' } else { ' ' };
                let d = digit_chars.next().unwrap_or('0');
                significant |= d != '0';
                out.push(if significant { d } else { fill });
            }
            Sym::FloatLead(c) => {
                float_char = Some(c);
                float_slot = Some(out.len());
                out.push(' ');
            }
            Sym::Float(_) => {
                let d = digit_chars.next().unwrap_or('0');
                significant |= d != '0';
                if significant {
                    out.push(d);
                } else {
                    float_slot = Some(out.len());
                    out.push(' ');
                }
            }
            Sym::Point | Sym::Implied => {
                significant = true;
                if *s == Sym::Point {
                    out.push('.');
                }
            }
            Sym::Insert(c) => {
                if significant {
                    out.push(c);
                } else if float_char.is_some() {
                    float_slot = Some(out.len());
                    out.push(' ');
                } else {
                    out.push(fill);
                }
            }
            Sym::Sign(c) => out.push(match (c, negative) {
                ('+', false) => '+',
                (_, true) => '-',
                _ => ' ',
            }),
            Sym::Currency => out.push('$'),
            Sym::Cr => out.extend(if negative { ['C', 'R'] } else { [' ', ' '] }),
            Sym::Db => out.extend(if negative { ['D', 'B'] } else { [' ', ' '] }),
            Sym::Char => out.push(' '),
        }
    }
    if let (Some(c), Some(slot)) = (float_char, float_slot) {
        out[slot] = match (c, negative) {
            ('$', _) => '$',
            ('+', false) => '+',
            (_, true) => '-',
            _ => ' ',
        };
    }
    out.into_iter().collect()
}

/// Alphanumeric data laid into an alphanumeric-edited PICTURE: characters fill the X, A and 9
/// positions in order; insertion characters stand where they are written.
pub fn alphanumeric(syms: &[Sym], data: &[u8], space: u8, encode: impl Fn(char) -> u8) -> Vec<u8> {
    let mut source = data.iter();
    syms.iter()
        .map(|s| match s {
            Sym::Insert(c) => encode(*c),
            _ => source.next().copied().unwrap_or(space),
        })
        .collect()
}

/// A numeric-edited item's value: the digits in its digit positions, negative when it shows a
/// minus sign, CR or DB.
pub fn de_edit(syms: &[Sym], text: &str) -> (bool, u128) {
    let chars: Vec<char> = text.chars().collect();
    let (mut at, mut magnitude, mut negative) = (0usize, 0u128, false);
    for s in syms {
        let c = chars.get(at).copied().unwrap_or(' ');
        match s {
            Sym::Implied => continue,
            Sym::Cr | Sym::Db => {
                negative |= c != ' ';
                at += 2;
                continue;
            }
            _ if is_digit(s) => magnitude = magnitude * 10 + c.to_digit(10).unwrap_or(0) as u128,
            Sym::Sign(_) | Sym::FloatLead(_) | Sym::Float(_) => negative |= c == '-',
            _ => {}
        }
        if matches!(s, Sym::Float(_)) {
            negative |= c == '-';
        }
        at += 1;
    }
    (negative, magnitude)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::picture::analyse;

    fn edit(pic: &str, value: i128) -> String {
        let p = analyse(pic).unwrap();
        numeric(p.edit.as_ref().unwrap(), p.digits, value < 0, value.unsigned_abs(), false)
    }

    #[test]
    fn zero_suppression_and_insertion() {
        assert_eq!(edit("ZZ,ZZ9", 1234), " 1,234");
        assert_eq!(edit("ZZ,ZZ9", 5), "     5");
        assert_eq!(edit("ZZZ", 0), "   ");
        assert_eq!(edit("ZZ9.99", 505), "  5.05");
        assert_eq!(edit("ZZ.ZZ", 0), "     ");
        assert_eq!(edit("99/99/99", 270926), "27/09/26");
        assert_eq!(edit("999B999", 123456), "123 456");
    }

    #[test]
    fn check_protection() {
        assert_eq!(edit("**,**9.99", 1234), "****12.34");
        assert_eq!(edit("***", 0), "***");
        assert_eq!(edit("**.**", 0), "**.**");
    }

    #[test]
    fn floating_currency_and_signs() {
        assert_eq!(edit("$$,$$9.99", 123450), "$1,234.50");
        assert_eq!(edit("$$,$$9.99", 550), "    $5.50");
        assert_eq!(edit("----9", -42), "  -42");
        assert_eq!(edit("----9", 42), "   42");
        assert_eq!(edit("++++9", 42), "  +42");
    }

    #[test]
    fn fixed_signs_cr_and_db() {
        assert_eq!(edit("-ZZ9", -7), "-  7");
        assert_eq!(edit("+ZZ9", 7), "+  7");
        assert_eq!(edit("ZZ9-", -7), "  7-");
        assert_eq!(edit("ZZ9CR", -7), "  7CR");
        assert_eq!(edit("ZZ9DB", 7), "  7  ");
        assert_eq!(edit("$ZZ9.99", 1999), "$ 19.99");
    }

    #[test]
    fn blank_when_zero() {
        let p = analyse("999.99").unwrap();
        assert_eq!(numeric(p.edit.as_ref().unwrap(), p.digits, false, 0, true), "      ");
    }

    #[test]
    fn de_editing_recovers_the_value() {
        for (pic, v) in [("$$,$$9.99CR", -123450i128), ("ZZ9-", -7), ("ZZ,ZZ9", 1234)] {
            let p = analyse(pic).unwrap();
            let text = numeric(p.edit.as_ref().unwrap(), p.digits, v < 0, v.unsigned_abs(), false);
            assert_eq!(de_edit(p.edit.as_ref().unwrap(), &text), (v < 0, v.unsigned_abs()), "{pic} {text:?}");
        }
    }

    #[test]
    fn alphanumeric_editing() {
        let p = analyse("XXBXX/X").unwrap();
        let out = alphanumeric(p.edit.as_ref().unwrap(), b"ABCDE", b' ', |c| c as u8);
        assert_eq!(out, b"AB CD/E");
    }
}
