//! The program's collating sequence (lir.md §4), as exec/src/collating.rs builds it from the ALPHABET
//! that PROGRAM COLLATING SEQUENCE names.

use crate::{codec_enum, codec_struct};

/// What alphanumeric comparisons, HIGH-VALUE and LOW-VALUE, FUNCTION CHAR and ORD, and a file SORT
/// without its own COLLATING SEQUENCE follow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Collating {
    /// EBCDIC: each byte its own position, HIGH-VALUE X'FF' and LOW-VALUE X'00'.
    Native,
    Sequence(Sequence),
}

/// `positions` gives each byte's position from 0, where characters that collate equal share one;
/// `characters` the first character given each position, which FUNCTION CHAR returns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sequence {
    pub positions: Box<[u8; 256]>,
    pub characters: Vec<u8>,
    /// The last character of the highest position and the first of the lowest.
    pub high_value: u8,
    pub low_value: u8,
}

codec_enum!(Collating { Native = 0, Sequence(sequence) = 1 });
codec_struct!(Sequence { positions, characters, high_value, low_value } check sequence_valid);

/// Every position holds a character, and each character named holds the position named for it.
fn sequence_valid(s: &Sequence) -> Result<(), String> {
    let count = s.characters.len();
    if let Some(b) = (0..=255u8).find(|&b| usize::from(s.positions[usize::from(b)]) >= count) {
        return Err(format!("X'{b:02X}' at position {} of a sequence of {count}", s.positions[usize::from(b)]));
    }
    if let Some((k, &c)) = s.characters.iter().enumerate().find(|&(k, &c)| usize::from(s.positions[usize::from(c)]) != k) {
        return Err(format!("character {k} of the sequence, X'{c:02X}', is at position {}", s.positions[usize::from(c)]));
    }
    let at = |b: u8| usize::from(s.positions[usize::from(b)]);
    if at(s.high_value) + 1 != count || at(s.low_value) != 0 {
        return Err(format!("HIGH-VALUE X'{:02X}' or LOW-VALUE X'{:02X}' is not at the sequence's end", s.high_value, s.low_value));
    }
    Ok(())
}
