//! The byte-level work of STRING, UNSTRING and INSPECT, apart from storage and operands.

use crate::vocab::InspectMode;

/// One INSPECT phrase, resolved to bytes: `pattern` is empty for CHARACTERS.
pub struct Phrase {
    pub mode: InspectMode,
    pub pattern: Vec<u8>,
    pub by: Option<Vec<u8>>,
    /// The region the phrase applies to, from its BEFORE and AFTER INITIAL bounds.
    pub start: usize,
    pub end: usize,
}

/// The region `[start, end)` of `data` that BEFORE and AFTER INITIAL leave: BEFORE ends it at the
/// first occurrence of its value, AFTER starts it after the first occurrence of its value (or
/// leaves nothing when the value does not occur).
pub fn region(data: &[u8], before: Option<&[u8]>, after: Option<&[u8]>) -> (usize, usize) {
    let find = |needle: &[u8]| (!needle.is_empty()).then(|| data.windows(needle.len()).position(|w| w == needle)).flatten();
    let start = match after {
        Some(a) => find(a).map_or(data.len(), |p| p + a.len()),
        None => 0,
    };
    let end = match before {
        Some(b) => find(b).unwrap_or(data.len()),
        None => data.len(),
    };
    (start, end.max(start))
}

/// Scans `data` left to right; at each position the first phrase that applies and matches takes
/// the characters it matches. Returns how many times each phrase matched, replacing as it goes
/// when phrases carry a BY value.
pub fn inspect(data: &mut [u8], phrases: &[Phrase]) -> Vec<i64> {
    let mut counts = vec![0i64; phrases.len()];
    let mut active = vec![true; phrases.len()];
    let mut next_leading: Vec<usize> = phrases.iter().map(|p| p.start).collect();
    let mut at = 0;
    while at < data.len() {
        let mut taken = 0;
        for (k, phrase) in phrases.iter().enumerate() {
            if !active[k] || at < phrase.start || at >= phrase.end {
                continue;
            }
            if phrase.mode == InspectMode::Leading && at != next_leading[k] {
                active[k] = false;
                continue;
            }
            let len = if phrase.mode == InspectMode::Characters { 1 } else { phrase.pattern.len() };
            let fits = len > 0 && at + len <= phrase.end;
            let hit = fits && (phrase.mode == InspectMode::Characters || data[at..at + len] == phrase.pattern[..]);
            if !hit {
                if phrase.mode == InspectMode::Leading {
                    active[k] = false;
                }
                continue;
            }
            counts[k] += 1;
            if let Some(by) = &phrase.by {
                for (i, b) in by.iter().cycle().take(len).enumerate() {
                    data[at + i] = *b;
                }
            }
            match phrase.mode {
                InspectMode::First => active[k] = false,
                InspectMode::Leading => next_leading[k] = at + len,
                _ => {}
            }
            taken = len;
            break;
        }
        at += taken.max(1);
    }
    counts
}

/// Where the next UNSTRING field ends and which delimiter ended it: the earliest position at or
/// after `from` where any delimiter matches, the first listed winning at a tie.
pub fn next_delimiter(source: &[u8], from: usize, delimiters: &[(bool, Vec<u8>)]) -> Option<(usize, usize)> {
    (from..source.len()).find_map(|p| delimiters.iter().position(|(_, d)| !d.is_empty() && source[p..].starts_with(d)).map(|k| (p, k)))
}

/// The end of a delimiter at `at`, taking every repetition when it is DELIMITED BY ALL.
pub fn past_delimiter(source: &[u8], at: usize, delimiter: &[u8], all: bool) -> usize {
    let mut end = at + delimiter.len();
    while all && !delimiter.is_empty() && source[end..].starts_with(delimiter) {
        end += delimiter.len();
    }
    end
}

/// STRING's view of a sending item: all of it for DELIMITED BY SIZE, or up to the delimiter.
pub fn delimited(bytes: &[u8], delimiter: Option<&[u8]>) -> Vec<u8> {
    match delimiter {
        Some(d) if !d.is_empty() => {
            let end = bytes.windows(d.len()).position(|w| w == d).unwrap_or(bytes.len());
            bytes[..end].to_vec()
        }
        _ => bytes.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phrase(mode: InspectMode, pattern: &str, by: Option<&str>, region: (usize, usize)) -> Phrase {
        Phrase { mode, pattern: pattern.as_bytes().to_vec(), by: by.map(|b| b.as_bytes().to_vec()), start: region.0, end: region.1 }
    }

    #[test]
    fn tallying_all_leading_and_characters() {
        let mut data = b"  AABA  ".to_vec();
        let n = data.len();
        let counts = inspect(&mut data, &[phrase(InspectMode::Leading, " ", None, (0, n)), phrase(InspectMode::All, "A", None, (0, n))]);
        assert_eq!(counts, [2, 3]);
        let counts = inspect(&mut data, &[phrase(InspectMode::Characters, "", None, (0, n))]);
        assert_eq!(counts, [8]);
    }

    #[test]
    fn replacing_first_all_and_within_bounds() {
        let mut data = b"A,B,C.D,E".to_vec();
        let (start, end) = region(&data, Some(b"."), None);
        inspect(&mut data, &[phrase(InspectMode::All, ",", Some(";"), (start, end))]);
        assert_eq!(data, b"A;B;C.D,E");
        let mut data = b"XAXAX".to_vec();
        inspect(&mut data, &[phrase(InspectMode::First, "X", Some("Y"), (0, 5))]);
        assert_eq!(data, b"YAXAX");
        let mut data = b"00012".to_vec();
        inspect(&mut data, &[phrase(InspectMode::Leading, "0", Some(" "), (0, 5))]);
        assert_eq!(data, b"   12");
    }

    #[test]
    fn after_initial_with_no_occurrence_leaves_nothing() {
        assert_eq!(region(b"ABC", None, Some(b"Z")), (3, 3));
        assert_eq!(region(b"ABCD", Some(b"D"), Some(b"A")), (1, 3));
    }

    #[test]
    fn a_leading_run_ends_at_the_first_other_character() {
        let mut data = b"**A**".to_vec();
        assert_eq!(inspect(&mut data, &[phrase(InspectMode::Leading, "*", None, (0, 5))]), [2]);
    }

    #[test]
    fn delimiters() {
        let d = vec![(true, b" ".to_vec()), (false, b",".to_vec())];
        assert_eq!(next_delimiter(b"AB  C,D", 0, &d), Some((2, 0)));
        assert_eq!(past_delimiter(b"AB  C,D", 2, b" ", true), 4);
        assert_eq!(next_delimiter(b"AB  C,D", 4, &d), Some((5, 1)));
        assert_eq!(delimited(b"JOHN  SMITH", Some(b" ")), b"JOHN");
    }
}
