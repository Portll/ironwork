//! The storage vocabulary a program's data shares: how an item is held, where it sits, and the
//! values that move between items.

use crate::vocab::{Figurative, SignClause};
use numeric::Native;
use numeric::precision::{Fixed, Places};
use zarch::hfp::{Hfp, Precision};
use zarch::wide::U256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Group,
    Alnum { justified: bool },
    National,
    /// USAGE DISPLAY-1: DBCS characters, two bytes each, X'4040' the space. `edit` indexes the
    /// layout's edited PICTUREs for one with B, a position that always holds a DBCS space.
    Dbcs { justified: bool, edit: Option<u32> },
    Zoned { digits: u32, scale: u32, signed: bool, sign: Option<SignClause> },
    Packed { digits: u32, scale: u32, signed: bool },
    Binary { digits: u32, scale: u32, signed: bool, native: Native },
    Float(Precision),
    /// `edit` indexes the layout's list of edited PICTUREs.
    NumericEdited { edit: u32, digits: u32, scale: u32, blank_when_zero: bool },
    AlnumEdited { edit: u32 },
    /// USAGE POINTER: an address, four bytes.
    Pointer,
    /// An index name or USAGE INDEX item, holding an occurrence number in four bytes.
    Index,
    /// USAGE OBJECT REFERENCE: four bytes naming an object, as under LP(32).
    ObjectReference,
    /// USAGE FUNCTION-POINTER or PROCEDURE-POINTER, four bytes.
    ProgramPointer,
}

impl Kind {
    pub fn is_numeric(self) -> bool {
        matches!(self, Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. } | Kind::Float(_) | Kind::Index)
    }

    /// Digits and decimal places of a fixed-point numeric item.
    pub fn digits_scale(self) -> Option<(u32, u32)> {
        match self {
            Kind::Zoned { digits, scale, .. }
            | Kind::Packed { digits, scale, .. }
            | Kind::Binary { digits, scale, .. }
            | Kind::NumericEdited { digits, scale, .. } => Some((digits, scale)),
            Kind::Index => Some((9, 0)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Loc {
    pub offset: usize,
    pub len: usize,
    pub kind: Kind,
    pub item: usize,
}

#[derive(Clone, Debug)]
pub enum Val {
    Bytes(Vec<u8>),
    National(Vec<u8>),
    /// DBCS characters, two bytes each, as a DBCS literal or item holds them.
    Dbcs(Vec<u8>),
    Num(Fixed),
    Float(Hfp),
    Fig(Figurative),
    All(Vec<u8>),
    /// ALL with a national literal: its UTF-16 units, repeated to the length of what it meets.
    AllNational(Vec<u8>),
    /// A pointer value: `ADDRESS_BASE` (in the interpreter's run unit) plus an offset into run-unit memory, or 0 for NULL.
    Address(u32),
}

/// A numeric literal's value; None unless it holds 1 to 31 digits.
pub fn literal_fixed(text: &str) -> Option<Fixed> {
    let (negative, body) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let (int, frac) = body.split_once('.').unwrap_or((body, ""));
    let digits = format!("{int}{frac}");
    if digits.is_empty() || digits.len() > 31 {
        return None;
    }
    let magnitude = U256::from_u128(digits.parse().ok()?);
    Some(Fixed { negative: negative && !magnitude.is_zero(), magnitude, places: Places::new(int.len().max(1) as u32, frac.len() as u32) })
}
