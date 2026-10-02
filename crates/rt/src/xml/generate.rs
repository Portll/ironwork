//! The text XML GENERATE writes (Language Reference SC27-8713-03, pp. 491-494; XML-CODE values from
//! the Programming Guide SC27-8714-03, pp. 817-818).

use super::{Encoding, UTF8, UTF16, name_char, name_start, xml_char};
use zarch::ebcdic::CodePage;

pub const RECEIVER_TOO_SMALL: i64 = 400;
pub const CODEPAGE_UNSUPPORTED: i64 = 411;
pub const BAD_ENCODING: i64 = 414;
pub const NATIONAL_NOT_UTF16: i64 = 415;
pub const BAD_NAMESPACE: i64 = 416;
pub const ILLEGAL_CHARACTERS: i64 = 417;
pub const SUBSTITUTED: i64 = 418;
pub const BAD_PREFIX: i64 = 419;
pub const NATIONAL_NOT_UTF8: i64 = 420;
/// The single-byte EBCDIC code pages an XML document may be written in (Programming Guide p. 640,
/// Table 77).
pub const CCSIDS: &[u16] = &[1047, 1140, 37, 1141, 273, 1142, 277, 1143, 278, 1144, 280, 1145, 284, 1146, 285, 1147, 297, 1148, 500, 1149, 871];

/// The document's encoding for a national receiver or another, given the CCSID it is in and
/// whether ENCODING named it; the XML-CODE of one the statement cannot write.
pub fn encoding(national: bool, ccsid: Option<u16>, named: bool) -> Result<Encoding, i64> {
    match (national, ccsid) {
        (true, Some(UTF16)) => Ok(Encoding::National),
        (true, _) => Err(NATIONAL_NOT_UTF16),
        (false, Some(UTF8)) => Ok(Encoding::Utf8),
        (false, Some(c)) if CCSIDS.contains(&c) && CodePage::by_ccsid(c).is_some() => Ok(Encoding::Page(CodePage::by_ccsid(c).expect("checked"))),
        (false, _) if !named => Err(CODEPAGE_UNSUPPORTED),
        (false, _) => Err(BAD_ENCODING),
    }
}

/// Content or an attribute value: the five special characters as references, and each character
/// beyond U+FFFF as a character reference.
pub fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '\'' => out.push_str("&apos;"),
            '>' => out.push_str("&gt;"),
            '<' => out.push_str("&lt;"),
            '"' => out.push_str("&quot;"),
            c if c as u32 > 0xFFFF => out.push_str(&format!("&#x{:X};", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// An element or attribute name from a data-name: one that starts with a digit, or with "xml" in
/// any case, takes an underscore before it.
pub fn name(data_name: &str) -> String {
    let xml = data_name.get(..3).is_some_and(|s| s.eq_ignore_ascii_case("xml"));
    if xml || data_name.starts_with(|c: char| c.is_ascii_digit()) { format!("_{data_name}") } else { data_name.to_owned() }
}

/// Whether a namespace prefix is an XML name without a colon.
pub fn valid_prefix(prefix: &str) -> bool {
    let mut chars = prefix.chars();
    chars.next().is_some_and(|c| name_start(c) && c != ':') && chars.all(|c| name_char(c) && c != ':')
}

/// Whether every character may stand in XML content.
pub fn legal(text: &str) -> bool {
    text.chars().all(|c| xml_char(c as u32))
}

/// The encoding declaration's value for a document in this CCSID.
pub fn encoding_name(ccsid: u16) -> String {
    match ccsid {
        1208 => "UTF-8".into(),
        1200 => "UTF-16".into(),
        c => format!("IBM-{c:03}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn special_characters_become_references() {
        assert_eq!(escaped("a&b'c>d<e\"f\u{10813}"), "a&amp;b&apos;c&gt;d&lt;e&quot;f&#x10813;");
    }

    #[test]
    fn names_that_start_with_a_digit_or_xml_take_an_underscore() {
        assert_eq!(name("3D"), "_3D");
        assert_eq!(name("Xml"), "_Xml");
        assert_eq!(name("xMLdata"), "_xMLdata");
        assert_eq!(name("Msg-Text"), "Msg-Text");
    }

    #[test]
    fn prefixes_encodings_and_legal_text() {
        assert!(valid_prefix("po") && !valid_prefix("p:o") && !valid_prefix("1p") && !valid_prefix(""));
        assert_eq!(encoding_name(37), "IBM-037");
        assert_eq!(encoding_name(1140), "IBM-1140");
        assert_eq!(encoding_name(1208), "UTF-8");
        assert!(legal("a\tb") && !legal("a\u{0}b"));
    }
}
