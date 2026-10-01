//! ULENGTH, UPOS, USUBSTR, USUPPLEMENTARY, UVALID and UWIDTH over an argument's bytes: UTF-8 in an
//! alphanumeric item, UTF-16 big-endian in a national one (Language Reference SC27-8713-03,
//! pp. 667-684).

/// Where the first ill-formed data starts, from 1: a byte for UTF-8, an encoding unit for UTF-16;
/// `None` when the whole argument is well formed (UVALID, p. 679).
pub fn invalid(bytes: &[u8], utf16: bool) -> Option<usize> {
    if !utf16 {
        return std::str::from_utf8(bytes).err().map(|e| e.valid_up_to() + 1);
    }
    let units: Vec<u16> = bytes.chunks(2).map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect();
    let mut k = 0;
    while k < units.len() {
        match units[k] {
            0xD800..=0xDBFF if units.get(k + 1).is_some_and(|u| (0xDC00..=0xDFFF).contains(u)) => k += 2,
            0xD800..=0xDFFF => return Some(k + 1),
            _ => k += 1,
        }
    }
    None
}

/// Each character's byte offset and width; a byte or unit that starts no well-formed character
/// counts as one.
pub fn characters(bytes: &[u8], utf16: bool) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let width = if utf16 {
            let high = u16::from_be_bytes([bytes[at], *bytes.get(at + 1).unwrap_or(&0)]);
            let low = bytes.get(at + 2..at + 4).map(|b| u16::from_be_bytes([b[0], b[1]]));
            if (0xD800..0xDC00).contains(&high) && low.is_some_and(|l| (0xDC00..0xE000).contains(&l)) { 4 } else { 2.min(bytes.len() - at) }
        } else {
            (1..=4).rev().find(|&w| bytes.get(at..at + w).is_some_and(|c| std::str::from_utf8(c).is_ok_and(|s| s.chars().count() == 1))).unwrap_or(1)
        };
        out.push((at, width));
        at += width;
    }
    out
}

/// The position from 1 of the first character beyond U+FFFF: a byte for UTF-8, an encoding unit
/// for UTF-16; 0 when there is none.
pub fn supplementary(bytes: &[u8], utf16: bool) -> usize {
    characters(bytes, utf16).into_iter().find(|&(_, w)| w == 4).map_or(0, |(at, _)| if utf16 { at / 2 + 1 } else { at + 1 })
}

/// USUBSTR's bytes: `length` characters from character `start`; `None` when they are not all there.
pub fn substring(bytes: &[u8], utf16: bool, start: i128, length: i128) -> Option<&[u8]> {
    let chars = characters(bytes, utf16);
    if start < 1 || length < 0 || start + length - 1 > chars.len() as i128 {
        return None;
    }
    let first = start as usize - 1;
    if length == 0 {
        return Some(&bytes[..0]);
    }
    let (from, _) = chars[first];
    let (last, width) = chars[first + length as usize - 1];
    Some(&bytes[from..last + width])
}

#[cfg(test)]
mod tests {
    use super::*;

    const KAFER: &[u8] = &[0x4B, 0xC3, 0xA4, 0x66, 0x65, 0x72];
    const TOBURS: &[u8] = &[0x00, 0x54, 0x00, 0xF6, 0x00, 0x62, 0x00, 0x75, 0x00, 0x72, 0xD8, 0x58, 0xDC, 0x6B, 0x00, 0x73];

    fn positions(bytes: &[u8], utf16: bool) -> Vec<(usize, usize)> {
        characters(bytes, utf16).into_iter().map(|(at, w)| (at + 1, w)).collect()
    }

    #[test]
    fn the_language_references_examples_give_their_lengths_positions_and_widths() {
        assert_eq!(positions(KAFER, false), [(1, 1), (2, 2), (4, 1), (5, 1), (6, 1)]);
        assert_eq!(positions(TOBURS, true), [(1, 2), (3, 2), (5, 2), (7, 2), (9, 2), (11, 4), (15, 2)]);
        assert_eq!(characters(&[0x61, 0xCC, 0x88, 0x4B], false).len(), 3);
    }

    #[test]
    fn substrings_take_whole_characters() {
        assert_eq!(substring(KAFER, false, 1, 2), Some(&[0x4B, 0xC3, 0xA4][..]));
        assert_eq!(substring(KAFER, false, 3, 2), Some(&[0x66, 0x65][..]));
        assert_eq!(substring(TOBURS, true, 5, 2), Some(&[0x00, 0x72, 0xD8, 0x58, 0xDC, 0x6B][..]));
        assert_eq!(substring(KAFER, false, 5, 2), None);
    }

    #[test]
    fn validity_and_supplementary_characters() {
        assert_eq!(invalid(KAFER, false), None);
        assert_eq!(invalid(TOBURS, true), None);
        assert_eq!(invalid(&[0x00, 0x54, 0xD9, 0xC3, 0x00, 0x62], true), Some(2));
        assert_eq!(invalid(&[0x00, 0x54, 0x00, 0xF6, 0x00, 0x62, 0xDC, 0x01], true), Some(4));
        assert_eq!(invalid(&[0x41, 0xC0, 0x80], false), Some(2));
        assert_eq!(supplementary(&[0x00, 0x20, 0x00, 0x20, 0xD8, 0x34, 0xDD, 0x1E], true), 3);
        assert_eq!(supplementary(&[0x20, 0x20, 0xF0, 0x9D, 0x84, 0x9E], false), 3);
        assert_eq!(supplementary(&[0x61, 0xCC, 0x88, 0xF0, 0xA1, 0xB7, 0xA4, 0x4B], false), 4);
        assert_eq!(supplementary(KAFER, false), 0);
    }
}
