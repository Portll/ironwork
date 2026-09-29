use std::collections::BTreeMap;

use super::ModuleError;
use super::codec::Reader;
use super::leb;

/// The one table of strings a module's sections refer to by index (load-module.md §4.2).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StringTable {
    strings: Vec<String>,
}

impl StringTable {
    pub fn get(&self, index: usize) -> Option<&str> {
        self.strings.get(index).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.strings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.strings.iter().map(String::as_str)
    }

    /// The `STRINGS` section body: a count, then each string's byte length and UTF-8 bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        leb::write(&mut out, self.strings.len() as u64);
        for s in &self.strings {
            leb::write(&mut out, s.len() as u64);
            out.extend_from_slice(s.as_bytes());
        }
        out
    }

    /// Refuses invalid UTF-8, a text stored twice, and bytes after the last string.
    pub fn decode(bytes: &[u8]) -> Result<Self, ModuleError> {
        let none = Self::default();
        let mut r = Reader::new("STRINGS", bytes, &none);
        let count = r.count()?;
        let mut strings = Vec::with_capacity(count);
        let mut seen = BTreeMap::new();
        for index in 0..count {
            let at = r.position();
            let len = r.count()?;
            let text = std::str::from_utf8(r.bytes(len)?)
                .map_err(|_| r.malformed(at, format!("string {index} is not UTF-8")))?;
            if let Some(first) = seen.insert(text, index) {
                return Err(r.malformed(at, format!("string {index} repeats string {first}")));
            }
            strings.push(text.to_owned());
        }
        r.finish()?;
        Ok(Self { strings })
    }
}

/// Numbers each text by its first use.
#[derive(Debug, Default)]
pub(crate) struct Interner {
    table: StringTable,
    index: BTreeMap<String, usize>,
}

impl Interner {
    pub(crate) fn intern(&mut self, text: &str) -> usize {
        if let Some(&index) = self.index.get(text) {
            return index;
        }
        let index = self.table.strings.len();
        self.table.strings.push(text.to_owned());
        self.index.insert(text.to_owned(), index);
        index
    }

    pub(crate) fn table(&self) -> &StringTable {
        &self.table
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(texts: &[&str]) -> StringTable {
        StringTable { strings: texts.iter().map(|s| (*s).to_owned()).collect() }
    }

    #[test]
    fn strings_are_numbered_by_first_use() {
        let mut interner = Interner::default();
        let indices: Vec<usize> =
            ["PAYROLL", "WS-TOTAL", "PAYROLL", "", "WS-TOTAL", ""].map(|s| interner.intern(s)).into();
        assert_eq!(indices, [0, 1, 0, 2, 1, 2]);
        assert_eq!(interner.table(), &table(&["PAYROLL", "WS-TOTAL", ""]));
    }

    #[test]
    fn the_table_body_round_trips() {
        let t = table(&["", "A", "ÄÖÜ €", "WS-TOTAL"]);
        let body = t.encode();
        assert_eq!(&body[..4], [4, 0, 1, b'A']);
        assert_eq!(StringTable::decode(&body), Ok(t));
        assert_eq!(StringTable::decode(&[0]), Ok(StringTable::default()));
    }

    #[test]
    fn a_bad_table_is_malformed() {
        let malformed = |bytes: &[u8]| matches!(StringTable::decode(bytes), Err(ModuleError::Malformed { .. }));
        assert!(malformed(&[1, 2, 0xC3, 0x28]), "invalid UTF-8");
        assert!(malformed(&[2, 1, b'A', 1, b'A']), "a text twice");
        assert!(malformed(&[1, 1, b'A', 0]), "trailing bytes");
        assert!(malformed(&[2, 1, b'A']), "fewer strings than the count");
        assert!(malformed(&[1, 5, b'A']), "a length beyond the body");
        assert!(malformed(&[]), "no count");
    }
}
