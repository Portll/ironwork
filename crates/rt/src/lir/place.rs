//! Places (lir.md §5): data references resolved at lowering, which an executor evaluates to a `Loc`.

use super::{DebugId, IntExpr, PlaceNumcheck, SymId, XmlRegister};
use crate::storage::Kind;
use crate::{codec_enum, codec_struct};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    pub base: Base,
    pub offset: u32,
    /// One occurrence, before OCCURS DEPENDING ON and reference modification.
    pub len: u32,
    pub kind: Kind,
    /// PICTURE P positions right of the item's digits: its value is its digits times ten to this.
    pub scaling: u32,
    /// The OCCURS DEPENDING ON tables ahead of the item in its record, not within it: each moves
    /// the item back by its occurrences past the current count, evaluated before the subscripts.
    pub moved: Vec<Odo>,
    /// One per OCCURS level, outermost first.
    pub subscripts: Vec<Subscript>,
    /// The OCCURS DEPENDING ON tables within the item, other than one within another of them: each
    /// leaves its occurrences past the current count out of the length.
    pub odo: Vec<Odo>,
    pub refmod: Option<RefMod>,
    pub name: SymId,
    pub at: DebugId,
    pub numcheck: PlaceNumcheck,
    /// Under SSRANGE, where the item lies in its outermost table and the table's extent, which the
    /// composed subscripts' address is checked against; carried at the LIR section's end.
    pub table: Option<TableRange>,
}

/// An item's displacement within its outermost table at its first occurrence, and the table's
/// bytes with every OCCURS at its maximum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableRange {
    pub displacement: u32,
    pub extent: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base {
    Program,
    Local,
    Linkage(u16),
    /// RETURN-CODE at run-unit offset 0, when the program declares none.
    ReturnCode,
    Eib,
    SelfRef,
    JniEnv,
    /// An XML PARSE fragment register, the current event's text: `offset` and `len` are 0 and the
    /// fragment gives both. Its reference modification is checked whatever `check` says, abending
    /// IRONWORK "reference modification (s:l) of NAME is outside its N bytes".
    Xml(XmlRegister),
}

/// `check` is the occurrence count, present only under SSRANGE.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subscript {
    pub stride: u32,
    pub value: IntExpr,
    pub check: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Odo {
    pub object: IntExpr,
    pub max: u32,
    pub element: u32,
    pub check: bool,
}

/// Start and length count character positions: two bytes each when the place's kind is national.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefMod {
    pub start: IntExpr,
    pub length: Option<IntExpr>,
    pub check: bool,
}

codec_struct!(Place { base, offset, len, kind, scaling, moved, subscripts, odo, refmod, name, at, numcheck } default { table } check crate::module::codec::unchecked);
codec_struct!(TableRange { displacement, extent });
codec_enum!(Base {
    Program = 0,
    Local = 1,
    Linkage(record) = 2,
    ReturnCode = 3,
    Eib = 4,
    SelfRef = 5,
    JniEnv = 6,
    Xml(register) = 7,
});
codec_struct!(Subscript { stride, value, check });
codec_struct!(Odo { object, max, element, check });
codec_struct!(RefMod { start, length, check });
