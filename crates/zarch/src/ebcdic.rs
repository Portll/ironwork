use std::cmp::Ordering;
use std::fmt;
use std::sync::OnceLock;

pub const SPACE: u8 = 0x40;
pub const ZERO: u8 = 0xF0;
pub const QUOTE: u8 = 0x7F;
pub const APOSTROPHE: u8 = 0x7D;
pub const LOW_VALUE: u8 = 0x00;
pub const HIGH_VALUE: u8 = 0xFF;
pub const SHIFT_OUT: u8 = 0x0E;
pub const SHIFT_IN: u8 = 0x0F;

/// An EBCDIC code page: single-byte, or mixed, a single-byte page with two-byte characters between
/// shift-out and shift-in.
pub struct CodePage {
    pub ccsid: u16,
    pub name: &'static str,
    decode: [char; 256],
    encode: &'static [(char, u8)],
    /// A mixed page's DBCS component CCSID and its two-byte characters.
    dbcs: Option<(u16, &'static Dbcs)>,
}

/// The two-byte characters of a mixed page, as ICU's table of IBM's mapping gives them.
pub struct Dbcs {
    /// Six bytes for each code, sorted: the code, then its character, the top bit set when the
    /// character encodes to another code.
    decode: &'static [u8],
    /// Codes whose character is two code points.
    sequences: &'static [(u16, [char; 2])],
    /// Characters encoded to a code that decodes to another character.
    encode_only: &'static [(char, u16)],
    encode: OnceLock<Vec<(char, u16)>>,
}

const ONE_WAY: u32 = 1 << 31;

/// A DBCS code with no character decodes as U+FFFD.
const UNASSIGNED: char = '\u{FFFD}';

impl Dbcs {
    /// The DBCS space, the same in every IBM DBCS code page.
    pub const SPACE: u16 = 0x4040;

    fn records(&self) -> impl Iterator<Item = (u16, u32)> + '_ {
        self.decode.as_chunks::<6>().0.iter().map(|r| (u16::from_be_bytes([r[0], r[1]]), u32::from_be_bytes([r[2], r[3], r[4], r[5]])))
    }

    fn record(&self, k: usize) -> (u16, u32) {
        let r = &self.decode[k * 6..k * 6 + 6];
        (u16::from_be_bytes([r[0], r[1]]), u32::from_be_bytes([r[2], r[3], r[4], r[5]]))
    }

    /// The character or characters of a code, or None for a code with none.
    pub fn decode_code(&self, code: u16) -> Option<Vec<char>> {
        if let Ok(k) = self.sequences.binary_search_by_key(&code, |&(c, _)| c) {
            return Some(self.sequences[k].1.to_vec());
        }
        let (mut low, mut high) = (0, self.decode.len() / 6);
        while low < high {
            let mid = (low + high) / 2;
            match self.record(mid) {
                (c, ch) if c == code => return char::from_u32(ch & !ONE_WAY).map(|ch| vec![ch]),
                (c, _) if c < code => low = mid + 1,
                _ => high = mid,
            }
        }
        None
    }

    /// Two bytes a character, the last odd byte, if any, ignored; a code with no character is U+FFFD.
    pub fn decode(&self, bytes: &[u8]) -> String {
        bytes.as_chunks::<2>().0.iter().flat_map(|&p| self.decode_code(u16::from_be_bytes(p)).unwrap_or_else(|| vec![UNASSIGNED])).collect()
    }

    pub fn encode_char(&self, ch: char) -> Option<u16> {
        let encode = self.encode.get_or_init(|| {
            let mut all: Vec<(char, u16)> = self.records().filter(|&(_, ch)| ch & ONE_WAY == 0).filter_map(|(code, ch)| Some((char::from_u32(ch)?, code))).collect();
            all.extend_from_slice(self.encode_only);
            all.sort_unstable();
            all
        });
        encode.binary_search_by_key(&ch, |&(c, _)| c).ok().map(|k| encode[k].1)
    }

    /// The code for the character at `chars[k]`, and how many characters it takes: two when a code
    /// stands for that character and the next together.
    fn code_at(&self, chars: &[char], k: usize) -> Option<(u16, usize)> {
        let pair = chars.get(k + 1).and_then(|&next| self.sequences.iter().find(|(_, s)| *s == [chars[k], next]));
        match pair {
            Some(&(code, _)) => Some((code, 2)),
            None => self.encode_char(chars[k]).map(|code| (code, 1)),
        }
    }

    /// Two bytes for each character of `text`.
    pub fn encode(&self, text: &str, ccsid: u16) -> Result<Vec<u8>, Unmappable> {
        let chars: Vec<char> = text.chars().collect();
        let mut out = Vec::with_capacity(chars.len() * 2);
        let mut k = 0;
        while k < chars.len() {
            let (code, used) = self.code_at(&chars, k).ok_or(Unmappable { ch: chars[k], ccsid })?;
            out.extend_from_slice(&code.to_be_bytes());
            k += used;
        }
        Ok(out)
    }
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

    /// A mixed page's two-byte characters; None for a single-byte page.
    pub fn dbcs(&self) -> Option<&'static Dbcs> {
        self.dbcs.map(|(_, d)| d)
    }

    /// The CCSID of a mixed page's DBCS component (Programming Guide SC27-8714-03, Table 47).
    pub fn dbcs_ccsid(&self) -> Option<u16> {
        self.dbcs.map(|(c, _)| c)
    }

    pub fn decode_byte(&self, byte: u8) -> char {
        self.decode[byte as usize]
    }

    /// The characters of `bytes`; on a mixed page, those between shift-out and shift-in are DBCS
    /// characters, two bytes each, and the shifts themselves are none.
    pub fn decode(&self, bytes: &[u8]) -> String {
        let Some(dbcs) = self.dbcs() else { return bytes.iter().map(|&b| self.decode_byte(b)).collect() };
        let mut out = String::with_capacity(bytes.len());
        let mut k = 0;
        while k < bytes.len() {
            if bytes[k] != SHIFT_OUT {
                out.push(self.decode_byte(bytes[k]));
                k += 1;
                continue;
            }
            let close = bytes[k + 1..].iter().position(|&b| b == SHIFT_IN).map_or(bytes.len(), |p| k + 1 + p);
            let shifted = &bytes[k + 1..close];
            out.push_str(&dbcs.decode(shifted));
            if shifted.len() % 2 == 1 {
                out.push(UNASSIGNED);
            }
            k = close + 1;
        }
        out
    }

    pub fn encode_char(&self, ch: char) -> Option<u8> {
        self.encode.binary_search_by_key(&ch, |&(c, _)| c).ok().map(|i| self.encode[i].1)
    }

    /// The bytes of `text`; on a mixed page, a character the single bytes lack is a DBCS character,
    /// a run of them between shift-out and shift-in.
    pub fn encode(&self, text: &str) -> Result<Vec<u8>, Unmappable> {
        self.encode_with(text, None)
    }

    /// Encodes `text`, substituting the page's `?` for characters it cannot map.
    pub fn encode_lossy(&self, text: &str) -> Vec<u8> {
        let unknown = self.encode_char('?').unwrap_or(0x6F);
        self.encode_with(text, Some(unknown)).unwrap_or_default()
    }

    fn encode_with(&self, text: &str, unknown: Option<u8>) -> Result<Vec<u8>, Unmappable> {
        let chars: Vec<char> = text.chars().collect();
        let mut out = Vec::with_capacity(chars.len());
        let mut shifted = false;
        let mut k = 0;
        while k < chars.len() {
            let single = self.encode_char(chars[k]);
            let double = self.dbcs().filter(|_| single.is_none()).and_then(|d| d.code_at(&chars, k));
            match (single, double) {
                (Some(byte), _) => {
                    if std::mem::take(&mut shifted) {
                        out.push(SHIFT_IN);
                    }
                    out.push(byte);
                    k += 1;
                }
                (None, Some((code, used))) => {
                    if !std::mem::replace(&mut shifted, true) {
                        out.push(SHIFT_OUT);
                    }
                    out.extend_from_slice(&code.to_be_bytes());
                    k += used;
                }
                (None, None) => {
                    let byte = unknown.ok_or(Unmappable { ch: chars[k], ccsid: self.ccsid })?;
                    if std::mem::take(&mut shifted) {
                        out.push(SHIFT_IN);
                    }
                    out.push(byte);
                    k += 1;
                }
            }
        }
        if shifted {
            out.push(SHIFT_IN);
        }
        Ok(out)
    }

    /// DBCS data's characters: the mixed page's two-byte characters, or under a single-byte page,
    /// which has none, U+3000 for the DBCS space and U+FFFD for each other character.
    pub fn decode_dbcs(&self, bytes: &[u8]) -> String {
        match self.dbcs() {
            Some(dbcs) => dbcs.decode(bytes),
            None => bytes.as_chunks::<2>().0.iter().map(|p| if *p == [0x40, 0x40] { '\u{3000}' } else { UNASSIGNED }).collect(),
        }
    }

    /// What `FUNCTION NATIONAL-OF` yields for these bytes: UTF-16 big-endian.
    pub fn to_utf16be(&self, bytes: &[u8]) -> Vec<u8> {
        self.decode(bytes).encode_utf16().flat_map(u16::to_be_bytes).collect()
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
        assert_eq!(CodePage::all().len(), 32);
        for page in CodePage::all().iter().filter(|p| p.dbcs().is_none()) {
            for b in 0..=255u8 {
                let ch = page.decode_byte(b);
                assert_eq!(page.encode_char(ch), Some(b), "CCSID {} byte {b:02X}", page.ccsid);
            }
        }
    }

    #[test]
    fn a_mixed_page_s_single_bytes_round_trip_save_the_shifts_and_those_with_no_character() {
        let mixed: Vec<u16> = CodePage::all().iter().filter(|p| p.dbcs().is_some()).map(|p| p.ccsid).collect();
        assert_eq!(mixed, [930, 933, 935, 937, 939, 1364, 1388, 1390, 1399, 5026, 5035]);
        for page in CodePage::all().iter().filter(|p| p.dbcs().is_some()) {
            assert_eq!((page.decode_byte(SHIFT_OUT), page.decode_byte(SHIFT_IN)), ('\u{E}', '\u{F}'), "CCSID {}", page.ccsid);
            for b in (0..=255u8).filter(|&b| b != SHIFT_OUT && b != SHIFT_IN) {
                match page.decode_byte(b) {
                    '\u{1A}' => assert!(b == 0x3F || page.encode_char('\u{1A}') == Some(0x3F), "CCSID {} byte {b:02X}", page.ccsid),
                    ch => assert_eq!(page.encode_char(ch), Some(b), "CCSID {} byte {b:02X}", page.ccsid),
                }
            }
        }
        assert_eq!(CodePage::by_ccsid(930).unwrap().decode_byte(0xCA), '\u{1A}', "unassigned in CCSID 290");
    }

    #[test]
    fn dbcs_pages_decode_and_encode_two_bytes_a_character() {
        let p930 = CodePage::by_ccsid(930).unwrap();
        let dbcs = p930.dbcs().unwrap();
        assert_eq!(p930.dbcs_ccsid(), Some(300));
        assert_eq!(dbcs.decode(&[0x40, 0x40, 0x44, 0x81, 0x45, 0x62, 0x42, 0xC1]), "\u{3000}あ日Ａ");
        assert_eq!(dbcs.encode("\u{3000}あ日Ａ", 930).unwrap(), [0x40, 0x40, 0x44, 0x81, 0x45, 0x62, 0x42, 0xC1]);
        assert_eq!(dbcs.encode("A", 930), Err(Unmappable { ch: 'A', ccsid: 930 }));
        assert_eq!(dbcs.decode(&[0x40, 0x41]), "\u{FFFD}", "a code with no character");
        assert_eq!(dbcs.encode_char('\u{2015}'), Some(0x444A), "encoded only, to a code that decodes to another character");
        for page in CodePage::all().iter().filter(|p| p.dbcs().is_some()) {
            assert_eq!(page.dbcs().unwrap().decode(&[0x40, 0x40]), "\u{3000}", "CCSID {}", page.ccsid);
            assert_eq!(page.dbcs().unwrap().encode_char('\u{3000}'), Some(Dbcs::SPACE), "CCSID {}", page.ccsid);
        }
    }

    #[test]
    fn a_mixed_page_shifts_out_for_dbcs_characters_and_back_in() {
        let p939 = CodePage::by_ccsid(939).unwrap();
        let mixed = [0xC1, SHIFT_OUT, 0x44, 0x81, 0x45, 0x62, SHIFT_IN, 0xC2];
        assert_eq!(p939.encode("Aあ日B").unwrap(), mixed);
        assert_eq!(p939.decode(&mixed), "Aあ日B");
        assert_eq!(p939.encode("あ").unwrap(), [SHIFT_OUT, 0x44, 0x81, SHIFT_IN], "a run at the end is shifted back in");
        assert_eq!(p939.decode(&[SHIFT_OUT, 0x44, 0x81]), "あ", "no shift-in before the end");
        assert_eq!(p939.decode(&[SHIFT_OUT, 0x44, SHIFT_IN]), "\u{FFFD}", "an odd byte between the shifts");
        assert_eq!(p939.to_utf16be(&mixed), [0x00, 0x41, 0x30, 0x42, 0x65, 0xE5, 0x00, 0x42]);
        assert_eq!(p939.encode("A\u{1F600}"), Err(Unmappable { ch: '\u{1F600}', ccsid: 939 }));
        assert_eq!(p939.encode_lossy("あ\u{1F600}"), [SHIFT_OUT, 0x44, 0x81, SHIFT_IN, 0x6F]);
        let p1140 = CodePage::by_ccsid(1140).unwrap();
        assert_eq!(p1140.decode(&[SHIFT_OUT, 0xC1]), "\u{E}A", "a single-byte page has no shifts");
    }

    #[test]
    fn a_code_standing_for_two_code_points_takes_both_and_a_one_way_code_decodes() {
        let dbcs = CodePage::by_ccsid(1390).unwrap().dbcs().unwrap();
        assert_eq!(dbcs.decode(&[0xEC, 0xC3]), "\u{E6}\u{300}");
        assert_eq!(dbcs.encode("\u{E6}\u{300}", 1390).unwrap(), [0xEC, 0xC3]);
        assert_eq!(dbcs.decode(&[0x42, 0xE1]), "€");
        assert_ne!(dbcs.encode_char('€'), Some(0x42E1));
        assert_eq!(CodePage::by_ccsid(1390).unwrap().dbcs_ccsid(), Some(16684));
        assert_eq!(dbcs.decode(&[0xB3, 0x42]), "\u{2000B}", "a supplementary character");
    }

    #[test]
    fn encode_lossy_substitutes_question_mark() {
        let page = CodePage::by_ccsid(37).unwrap();
        assert_eq!(page.encode_lossy("A\u{1F600}"), [0xC1, 0x6F]);
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
