use std::cmp::Ordering;
use std::fmt;

pub const SPACE: u8 = 0x40;
pub const ZERO: u8 = 0xF0;
pub const QUOTE: u8 = 0x7F;
pub const APOSTROPHE: u8 = 0x7D;
pub const LOW_VALUE: u8 = 0x00;
pub const HIGH_VALUE: u8 = 0xFF;

/// A single-byte EBCDIC code page.
pub struct CodePage {
    pub ccsid: u16,
    pub name: &'static str,
    decode: [char; 256],
    encode: &'static [(char, u8)],
}

include!(concat!(env!("OUT_DIR"), "/codepages.rs"));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unmappable {
    pub ch: char,
    pub ccsid: u16,
}

impl fmt::Display for Unmappable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "U+{:04X} has no byte in CCSID {}", self.ch as u32, self.ccsid)
    }
}

impl std::error::Error for Unmappable {}

impl CodePage {
    pub fn all() -> &'static [CodePage] {
        &PAGES
    }

    pub fn by_ccsid(ccsid: u16) -> Option<&'static CodePage> {
        PAGES.iter().find(|p| p.ccsid == ccsid)
    }

    pub fn decode_byte(&self, byte: u8) -> char {
        self.decode[byte as usize]
    }

    pub fn decode(&self, bytes: &[u8]) -> String {
        bytes.iter().map(|&b| self.decode_byte(b)).collect()
    }

    pub fn encode_char(&self, ch: char) -> Option<u8> {
        self.encode.binary_search_by_key(&ch, |&(c, _)| c).ok().map(|i| self.encode[i].1)
    }

    pub fn encode(&self, text: &str) -> Result<Vec<u8>, Unmappable> {
        text.chars().map(|ch| self.encode_char(ch).ok_or(Unmappable { ch, ccsid: self.ccsid })).collect()
    }

    /// What `FUNCTION NATIONAL-OF` yields for these bytes: UTF-16 big-endian.
    pub fn to_utf16be(&self, bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(bytes.len() * 2);
        for &b in bytes {
            let mut units = [0u16; 2];
            for unit in self.decode_byte(b).encode_utf16(&mut units) {
                out.extend_from_slice(&unit.to_be_bytes());
            }
        }
        out
    }
}

/// An alphanumeric comparison: the shorter operand is extended on the right with spaces, then the
/// operands are compared byte by byte in the program collating sequence.
pub fn compare_alphanumeric(a: &[u8], b: &[u8], collation: &Collation) -> Ordering {
    let len = a.len().max(b.len());
    let at = |s: &[u8], i: usize| s.get(i).copied().unwrap_or(SPACE);
    (0..len)
        .map(|i| collation.weight(at(a, i)).cmp(&collation.weight(at(b, i))))
        .find(|o| o.is_ne())
        .unwrap_or(Ordering::Equal)
}

/// A program collating sequence: native EBCDIC byte order unless an alphabet says otherwise.
pub enum Collation {
    Native,
    Weights(Box<[u16; 256]>),
}

impl Collation {
    pub fn weight(&self, byte: u8) -> u16 {
        match self {
            Self::Native => byte as u16,
            Self::Weights(w) => w[byte as usize],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_maps_all_256_bytes_both_ways() {
        assert_eq!(CodePage::all().len(), 21);
        for page in CodePage::all() {
            for b in 0..=255u8 {
                let ch = page.decode_byte(b);
                assert_eq!(page.encode_char(ch), Some(b), "CCSID {} byte {b:02X}", page.ccsid);
            }
        }
    }

    #[test]
    fn figurative_constants_and_digits_are_invariant_across_pages() {
        for page in CodePage::all() {
            assert_eq!(page.decode_byte(SPACE), ' ', "CCSID {}", page.ccsid);
            assert_eq!(page.decode_byte(QUOTE), '"', "CCSID {}", page.ccsid);
            assert_eq!(page.decode_byte(APOSTROPHE), '\'', "CCSID {}", page.ccsid);
            assert_eq!(page.decode(&[0xF0, 0xF1, 0xF9]), "019", "CCSID {}", page.ccsid);
            assert_eq!(page.decode(&[0xC1, 0xD1, 0xE2]), "AJS", "CCSID {}", page.ccsid);
        }
    }

    #[test]
    fn brackets_and_not_sign_differ_between_037_and_1047() {
        let (p037, p1047) = (CodePage::by_ccsid(37).unwrap(), CodePage::by_ccsid(1047).unwrap());
        assert_eq!(p037.encode("[]^¬").unwrap(), [0xBA, 0xBB, 0xB0, 0x5F]);
        assert_eq!(p1047.encode("[]^¬").unwrap(), [0xAD, 0xBD, 0x5F, 0xB0]);
    }

    #[test]
    fn euro_pages_replace_the_currency_sign() {
        let (p037, p1140) = (CodePage::by_ccsid(37).unwrap(), CodePage::by_ccsid(1140).unwrap());
        assert_eq!(p037.decode_byte(0x9F), '¤');
        assert_eq!(p1140.decode_byte(0x9F), '€');
    }

    #[test]
    fn line_feed_is_x25_and_next_line_is_x15() {
        let p1047 = CodePage::by_ccsid(1047).unwrap();
        assert_eq!(p1047.decode_byte(0x25), '\n');
        assert_eq!(p1047.decode_byte(0x15), '\u{85}');
    }

    #[test]
    fn unmappable_character_is_named() {
        let err = CodePage::by_ccsid(37).unwrap().encode("A€").unwrap_err();
        assert_eq!(err, Unmappable { ch: '€', ccsid: 37 });
    }

    #[test]
    fn native_collation_puts_letters_before_digits_and_lowercase_before_uppercase() {
        let p = CodePage::by_ccsid(37).unwrap();
        let cmp = |a: &str, b: &str| compare_alphanumeric(&p.encode(a).unwrap(), &p.encode(b).unwrap(), &Collation::Native);
        assert_eq!(cmp("Z", "0"), Ordering::Less);
        assert_eq!(cmp("a", "A"), Ordering::Less);
        assert_eq!(cmp(" ", "a"), Ordering::Less);
    }

    #[test]
    fn shorter_operand_is_padded_with_spaces() {
        let c = Collation::Native;
        assert_eq!(compare_alphanumeric(&[0xC1], &[0xC1, SPACE, SPACE], &c), Ordering::Equal);
        assert_eq!(compare_alphanumeric(&[0xC1], &[0xC1, 0x00], &c), Ordering::Greater);
    }

    #[test]
    fn national_of_is_utf16_big_endian() {
        assert_eq!(CodePage::by_ccsid(1140).unwrap().to_utf16be(&[0xC1, 0x9F]), [0x00, 0x41, 0x20, 0xAC]);
    }
}
