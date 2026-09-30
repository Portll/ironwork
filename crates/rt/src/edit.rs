//! Editing: a numeric value laid into a numeric-edited PICTURE, alphanumeric data into an
//! alphanumeric-edited one, and de-editing back to a number.

use crate::picture::Sym;

/// The characters a numeric-edited item holds for a value. `magnitude` is already aligned to the
/// PICTURE's decimal places and within its digit positions. `point` is what a decimal point
/// position shows: a period, or a comma under DECIMAL-POINT IS COMMA. `currency` is the currency
/// sign value, which the first currency position holds in full.
pub fn numeric(syms: &[Sym], digits: u32, negative: bool, magnitude: u128, blank_when_zero: bool, point: char, currency: &str) -> String {
    let width = |s: &Sym| width(s, currency);
    let size: usize = syms.iter().map(width).sum();
    let digit_syms: Vec<&Sym> = syms.iter().filter(|s| s.is_digit()).collect();
    if magnitude == 0 && (blank_when_zero || digit_syms.iter().all(|s| matches!(s, Sym::Z | Sym::Float(_)))) {
        return " ".repeat(size);
    }
    if magnitude == 0 && digit_syms.iter().all(|s| matches!(s, Sym::Star)) {
        return syms.iter().flat_map(|s| std::iter::repeat_n(if *s == Sym::Point { point } else { '*' }, width(s))).collect();
    }
    let text = format!("{magnitude:0width$}", width = digits as usize);
    let mut digit_chars = text.chars().skip(text.len() - digits as usize);
    let mut out: Vec<char> = Vec::with_capacity(size);
    let (mut significant, mut fill) = (false, None);
    let (mut float_char, mut float_slot) = (None, None);
    for s in syms {
        match *s {
            Sym::Nine => {
                significant = true;
                out.push(digit_chars.next().unwrap_or('0'));
            }
            Sym::Z | Sym::Star => {
                let blank = if *s == Sym::Star { '*' } else { ' ' };
                fill = Some(blank);
                let d = digit_chars.next().unwrap_or('0');
                significant |= d != '0';
                out.push(if significant { d } else { blank });
            }
            Sym::FloatLead(c) => {
                float_char = Some(c);
                out.extend(std::iter::repeat_n(' ', width(s)));
                float_slot = Some(out.len() - 1);
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
                    out.push(point);
                }
            }
            Sym::Insert(c) => {
                if significant {
                    out.push(c);
                } else if float_char.is_some() {
                    float_slot = Some(out.len());
                    out.push(' ');
                } else {
                    out.push(fill.unwrap_or(c));
                }
            }
            Sym::Sign(c) => out.push(match (c, negative) {
                ('+', false) => '+',
                (_, true) => '-',
                _ => ' ',
            }),
            Sym::Currency => out.extend(currency.chars()),
            Sym::Cr => out.extend(if negative { ['C', 'R'] } else { [' ', ' '] }),
            Sym::Db => out.extend(if negative { ['D', 'B'] } else { [' ', ' '] }),
            Sym::Char => out.push(' '),
        }
    }
    match (float_char, float_slot) {
        (Some('$'), Some(slot)) => {
            let value: Vec<char> = currency.chars().collect();
            out[slot + 1 - value.len()..=slot].copy_from_slice(&value);
        }
        (Some(c), Some(slot)) => {
            out[slot] = match (c, negative) {
                ('+', false) => '+',
                (_, true) => '-',
                _ => ' ',
            }
        }
        _ => {}
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
/// minus sign, CR or DB. `currency` is the currency sign value, as for [`numeric`].
pub fn de_edit(syms: &[Sym], text: &str, currency: &str) -> (bool, u128) {
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
            _ if s.is_digit() => magnitude = magnitude * 10 + c.to_digit(10).unwrap_or(0) as u128,
            Sym::Sign(_) | Sym::FloatLead(_) | Sym::Float(_) => negative |= c == '-',
            _ => {}
        }
        if matches!(s, Sym::Float(_)) {
            negative |= c == '-';
        }
        at += width(s, currency);
    }
    (negative, magnitude)
}

/// Character positions `s` takes: the first currency position holds the whole currency sign value.
fn width(s: &Sym, currency: &str) -> usize {
    match s {
        Sym::Currency | Sym::FloatLead('$') => currency.chars().count().max(1),
        _ => s.width(),
    }
}
