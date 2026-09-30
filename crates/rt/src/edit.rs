//! Editing: a numeric value laid into a numeric-edited PICTURE, alphanumeric data into an
//! alphanumeric-edited one, and de-editing back to a number.

use crate::picture::Sym;

/// The characters a numeric-edited item holds for a value. `magnitude` is already aligned to the
/// PICTURE's decimal places and within its digit positions.
pub fn numeric(syms: &[Sym], digits: u32, negative: bool, magnitude: u128, blank_when_zero: bool) -> String {
    let size: usize = syms.iter().map(Sym::width).sum();
    let digit_syms: Vec<&Sym> = syms.iter().filter(|s| s.is_digit()).collect();
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
            _ if s.is_digit() => magnitude = magnitude * 10 + c.to_digit(10).unwrap_or(0) as u128,
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
