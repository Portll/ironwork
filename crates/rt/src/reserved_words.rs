//! IBM's reserved words, vendored from cobolwork's provenance by tools/sync-reserved-words.sh: each
//! word of Enterprise COBOL 6.4's Reserved words appendix with the column IBM marks it in.

use std::collections::HashMap;
use std::sync::OnceLock;

const TABLE: &str = include_str!("../data/reserved-words.tsv");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    /// Reserved by Enterprise COBOL, so a user-defined word cannot be it.
    Reserved,
    /// Reserved by the COBOL standard, not by Enterprise COBOL.
    StandardOnly,
    /// Not reserved yet; IBM may reserve it in a later release.
    Potential,
}

fn table() -> &'static HashMap<&'static str, Category> {
    static WORDS: OnceLock<HashMap<&'static str, Category>> = OnceLock::new();
    WORDS.get_or_init(|| {
        TABLE
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| match l.split_once('\t') {
                Some((word, "reserved")) => (word, Category::Reserved),
                Some((word, "standard-only")) => (word, Category::StandardOnly),
                Some((word, "potential")) => (word, Category::Potential),
                _ => panic!("reserved-words.tsv: {l} is not a word and its category"),
            })
            .collect()
    })
}

/// The column IBM's appendix marks `word` in, case-insensitively; None for a word it does not list.
pub fn category(word: &str) -> Option<Category> {
    table().get(word.to_ascii_uppercase().as_str()).copied()
}

/// Whether Enterprise COBOL reserves `word`: its Reserved column, not Standard only or Potential.
pub fn is_reserved(word: &str) -> bool {
    category(word) == Some(Category::Reserved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_word_has_its_column() {
        let count = |c| table().values().filter(|&&v| v == c).count();
        assert_eq!((count(Category::Reserved), count(Category::StandardOnly), count(Category::Potential)), (401, 52, 62));
        assert!(is_reserved("COUNT") && is_reserved("start") && is_reserved("INDEX"));
        assert_eq!((category("CONTROL"), category("ACTIVE-CLASS"), category("WS-COUNT")), (Some(Category::StandardOnly), Some(Category::Potential), None));
        assert!(!is_reserved("CONTROL") && !is_reserved("ACTIVE-CLASS") && !is_reserved("WS-COUNT"));
    }

    /// Run with IRONWORK_COBOLWORK_DIR naming a cobolwork checkout to check the vendored copy.
    #[test]
    fn the_vendored_table_is_cobolworks() {
        let Ok(dir) = std::env::var("IRONWORK_COBOLWORK_DIR") else { return };
        let theirs = std::fs::read_to_string(std::path::Path::new(&dir).join("provenance/reserved-words.tsv")).expect("cobolwork's table");
        assert!(theirs.replace('\r', "") == TABLE.replace('\r', ""), "crates/rt/data/reserved-words.tsv differs from cobolwork's: run tools/sync-reserved-words.sh");
    }
}
