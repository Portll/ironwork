//! The storage vocabulary a program's data shares: how an item is held, where it sits, and the
//! values that move between items.

use crate::vocab::{Figurative, SignClause};
use numeric::precision::Fixed;
use zarch::hfp::{Hfp, Precision};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Group,
    Alnum { justified: bool },
    National,
    Zoned { digits: u32, scale: u32, signed: bool, sign: Option<SignClause> },
    Packed { digits: u32, scale: u32, signed: bool },
    Binary { digits: u32, scale: u32, signed: bool, native: bool },
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
    Num(Fixed),
    Float(Hfp),
    Fig(Figurative),
    All(Vec<u8>),
    /// A pointer value: `ADDRESS_BASE` (in the interpreter's run unit) plus an offset into run-unit memory, or 0 for NULL.
    Address(u32),
}
