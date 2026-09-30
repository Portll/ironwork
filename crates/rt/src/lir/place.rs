//! Places (lir.md §5): data references resolved at lowering, which an executor evaluates to a `Loc`.

use super::{DebugId, IntExpr, SymId};
use crate::storage::Kind;
use crate::{codec_enum, codec_struct};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    pub base: Base,
    pub offset: u32,
    /// One occurrence, before OCCURS DEPENDING ON and reference modification.
    pub len: u32,
    pub kind: Kind,
    /// One per OCCURS level, outermost first.
    pub subscripts: Vec<Subscript>,
    pub odo: Option<Odo>,
    pub refmod: Option<RefMod>,
    pub name: SymId,
    pub at: DebugId,
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
}

/// `check` is the occurrence count, present only under SSRANGE.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Subscript {
    pub stride: u32,
    pub value: IntExpr,
    pub check: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Odo {
    pub object: IntExpr,
    pub max: u32,
    pub element: u32,
    pub check: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RefMod {
    pub start: IntExpr,
    pub length: Option<IntExpr>,
    pub check: bool,
}

codec_struct!(Place { base, offset, len, kind, subscripts, odo, refmod, name, at });
codec_enum!(Base {
    Program = 0,
    Local = 1,
    Linkage(record) = 2,
    ReturnCode = 3,
    Eib = 4,
    SelfRef = 5,
    JniEnv = 6,
});
codec_struct!(Subscript { stride, value, check });
codec_struct!(Odo { object, max, element, check });
codec_struct!(RefMod { start, length, check });
