//! Collating sequences: the order an ALPHABET clause gives the single-byte characters. PROGRAM
//! COLLATING SEQUENCE makes one the program's, for its alphanumeric comparisons, HIGH-VALUE,
//! LOW-VALUE, CHAR and ORD; the COLLATING SEQUENCE phrase makes one a SORT's or MERGE's.

use syntax::ast::{Alphabet, AlphabetEntry, Environment, Figurative, Literal};
use zarch::ebcdic::{self, CodePage, Collation};

pub struct Sequence {
    collation: Collation,
    /// The first character given each position, lowest position first.
    ordinals: Vec<u8>,
    pub high_value: u8,
    pub low_value: u8,
}

impl Sequence {
    pub fn native() -> Self {
        Self { collation: Collation::Native, ordinals: (0..=255).collect(), high_value: ebcdic::HIGH_VALUE, low_value: ebcdic::LOW_VALUE }
    }

    /// The program's sequence: its PROGRAM COLLATING SEQUENCE, else EBCDIC.
    pub fn program(environment: &Environment, page: &CodePage) -> Result<Self, String> {
        match &environment.collating_sequence {
            Some(name) => Self::named(environment, name, page).map_err(|m| format!("PROGRAM COLLATING SEQUENCE {name}: {m}")),
            None => Ok(Self::native()),
        }
    }

    pub fn named(environment: &Environment, name: &str, page: &CodePage) -> Result<Self, String> {
        let (_, alphabet) = environment.alphabets.iter().find(|(n, _)| n == name).ok_or("not an alphabet-name of SPECIAL-NAMES")?;
        Self::of(alphabet, page)
    }

    /// STANDARD-1 and STANDARD-2 are 7-bit ASCII's order: ASCII_COLLATION in numeric::assumptions.
    pub fn of(alphabet: &Alphabet, page: &CodePage) -> Result<Self, String> {
        Ok(match alphabet {
            Alphabet::Ebcdic | Alphabet::Native => Self::native(),
            Alphabet::Standard1 | Alphabet::Standard2 => {
                Self::from_positions((0..0x80u8).filter_map(|c| page.encode_char(c as char)).map(|b| vec![b]).collect())
            }
            Alphabet::Literal(entries) => Self::from_positions(literal_positions(entries, page)?),
        })
    }

    /// Positions given explicitly, lowest first, each holding characters that collate equal; every
    /// other character follows them in EBCDIC order, in a position of its own. HIGH-VALUE is the
    /// last character of the highest position and LOW-VALUE the first of the lowest (SC27-8713-03,
    /// pp. 128-129).
    fn from_positions(mut positions: Vec<Vec<u8>>) -> Self {
        positions.retain(|p| !p.is_empty());
        let mut given = [false; 256];
        positions.iter().flatten().for_each(|&b| given[b as usize] = true);
        positions.extend((0..=255u8).filter(|&b| !given[b as usize]).map(|b| vec![b]));
        let mut weights = Box::new([0u16; 256]);
        for (w, chars) in positions.iter().enumerate() {
            for &b in chars {
                weights[b as usize] = w as u16;
            }
        }
        let low_value = positions.first().and_then(|p| p.first()).copied().unwrap_or(ebcdic::LOW_VALUE);
        let high_value = positions.last().and_then(|p| p.last()).copied().unwrap_or(ebcdic::HIGH_VALUE);
        Self { collation: Collation::Weights(weights), ordinals: positions.iter().map(|p| p[0]).collect(), high_value, low_value }
    }

    /// A figurative constant's character: HIGH-VALUE and LOW-VALUE are this sequence's highest and
    /// lowest.
    pub fn figurative(&self, f: Figurative) -> u8 {
        match f {
            Figurative::HighValue => self.high_value,
            Figurative::LowValue => self.low_value,
            other => native_figurative(other),
        }
    }

    pub fn collation(&self) -> &Collation {
        &self.collation
    }

    pub fn is_native(&self) -> bool {
        matches!(self.collation, Collation::Native)
    }

    /// Each character's position, from 0: what a sort key collates by.
    pub fn positions(&self) -> [u8; 256] {
        std::array::from_fn(|b| self.collation.weight(b as u8) as u8)
    }

    /// FUNCTION ORD: the character's position, from 1.
    pub fn ordinal(&self, byte: u8) -> u16 {
        self.collation.weight(byte) + 1
    }

    /// FUNCTION CHAR: the character at a position, from 1; the first given it when several share it.
    pub fn character(&self, ordinal: i64) -> Option<u8> {
        usize::try_from(ordinal).ok().and_then(|n| n.checked_sub(1)).and_then(|i| self.ordinals.get(i)).copied()
    }

    /// How many positions there are: the most FUNCTION CHAR takes.
    pub fn count(&self) -> usize {
        self.ordinals.len()
    }
}

fn literal_positions(entries: &[AlphabetEntry], page: &CodePage) -> Result<Vec<Vec<u8>>, String> {
    let mut positions: Vec<Vec<u8>> = Vec::new();
    for entry in entries {
        match entry {
            AlphabetEntry::Literal(l) => positions.extend(characters(l, page)?.into_iter().map(|b| vec![b])),
            AlphabetEntry::Through(first, last) => {
                let (a, b) = (single(first, page)?, single(last, page)?);
                if a <= b {
                    positions.extend((a..=b).map(|c| vec![c]));
                } else {
                    positions.extend((b..=a).rev().map(|c| vec![c]));
                }
            }
            AlphabetEntry::Also(literals) => positions.push(literals.iter().map(|l| single(l, page)).collect::<Result<_, _>>()?),
        }
    }
    let mut seen = [false; 256];
    for &b in positions.iter().flatten() {
        if std::mem::replace(&mut seen[b as usize], true) {
            return Err(format!("the character X'{b:02X}' is given more than one position"));
        }
    }
    Ok(positions)
}

/// The characters an ALPHABET literal gives: its own; for a number, the character at that
/// position of EBCDIC; for a figurative constant, its EBCDIC character (ALPHABET_LITERALS in
/// numeric::assumptions).
fn characters(literal: &Literal, page: &CodePage) -> Result<Vec<u8>, String> {
    Ok(match literal {
        Literal::Alnum(s) => page.encode(s).map_err(|e| e.to_string())?,
        Literal::Hex(b) => b.clone(),
        Literal::Number(n) => match n.parse::<u16>() {
            Ok(k @ 1..=256) if n.bytes().all(|c| c.is_ascii_digit()) => vec![(k - 1) as u8],
            _ => return Err(format!("{n} is not an ordinal position from 1 to 256")),
        },
        Literal::Figurative(Figurative::Null) => return Err("NULL cannot be in an ALPHABET clause".into()),
        Literal::Figurative(f) => vec![native_figurative(*f)],
        Literal::National(_) => return Err("a national literal cannot be in an ALPHABET clause".into()),
        Literal::All(_) => return Err("ALL cannot be in an ALPHABET clause".into()),
    })
}

fn native_figurative(f: Figurative) -> u8 {
    match f {
        Figurative::Space => ebcdic::SPACE,
        Figurative::Zero => ebcdic::ZERO,
        Figurative::Quote => ebcdic::QUOTE,
        Figurative::HighValue => ebcdic::HIGH_VALUE,
        Figurative::LowValue => ebcdic::LOW_VALUE,
        Figurative::Null => 0,
    }
}

fn single(literal: &Literal, page: &CodePage) -> Result<u8, String> {
    match characters(literal, page)?.as_slice() {
        [b] => Ok(*b),
        _ => Err("a literal of THROUGH or ALSO must be one character".into()),
    }
}
