//! Values, expressions and conditions (lir.md §6).

use super::{AbendId, CondId, ConstId, ExprId, FunctionId, Mode, Odo, PlaceId, TempId};
use crate::codec_enum;
use crate::vocab::{BinOp, Figurative, RelOp};
use numeric::precision::Fixed;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operand {
    Load(PlaceId),
    Const(ConstId),
    LengthOf(PlaceId),
    AddressOf(PlaceId),
    Function(FunctionId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Const {
    Bytes(Vec<u8>),
    National(Vec<u8>),
    Number(Fixed),
    Figurative(Figurative),
    All(Vec<u8>),
}

/// `Fixed` locates each place of `prepass` before it evaluates `expr`, as the walker's dmax pass
/// does; static places are left out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IntExpr {
    Const(i64),
    Item(PlaceId),
    Fixed { expr: ExprId, dmax: u32, prepass: Vec<PlaceId> },
    /// Subscript k of the JSON walk in progress (lir.md §9.13): FROM's subscripts, then the
    /// occurrence of each table the walk has entered. Only a markup payload's places hold it.
    Walk(u8),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    Operand(Operand),
    Neg(ExprId),
    Bin(ExprId, BinOp, ExprId),
    /// An exponent from 0 to 31, else abend IRONWORK.
    Pow(ExprId, IntExpr),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cond {
    Rel { a: Comparand, op: RelOp, b: Comparand, how: Compare },
    Class { place: PlaceId, test: ByteClass },
    Sign { value: Comparand, test: SignTest },
    /// A level-88 name: equal to any value, or within any THRU pair.
    Name { subject: PlaceId, values: Vec<(ConstId, Option<ConstId>)>, how: Compare },
    Not(CondId),
    And(CondId, CondId),
    Or(CondId, CondId),
    Counter(TempId),
    InTable { index: PlaceId, count: Count },
    Sql(SqlTest),
}

/// `Expr` locates each place of `prepass` (the float test's, then for `Mode::Fixed` the dmax pass's,
/// static ones left out) before it evaluates `expr` in `mode`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Comparand {
    Operand(Operand),
    Expr { expr: ExprId, dmax: u32, mode: Mode, prepass: Vec<PlaceId> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compare {
    PackedPfd,
    Address,
    Float,
    Fixed,
    National,
    Alphanumeric,
    Refused(AbendId),
    /// Two addresses, one side an object reference: equal when both identify the same object, else
    /// less. Each side is looked up, the first first, and one that was freed or never given abends.
    References,
    /// A zoned integer, the first operand when `zoned_first`, against a nonnumeric one: its bytes
    /// as `rt::store::compared_zoned_bytes` gives them, never its value, compared as alphanumeric.
    ZonedBytes { zoned_first: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ByteClass {
    Packed { signed: bool },
    Zoned { signed: bool },
    Digits,
    Alphabetic,
    AlphabeticLower,
    AlphabeticUpper,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignTest {
    Positive,
    Negative,
    Zero,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Count {
    Fixed(u32),
    Odo(Odo),
}

/// WHENEVER's classes: SQLCODE < 0, SQLCODE = 100, or a warning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlTest {
    Error,
    NotFound,
    Warning,
}

codec_enum!(Operand { Load(place) = 0, Const(id) = 1, LengthOf(place) = 2, AddressOf(place) = 3, Function(id) = 4 });
codec_enum!(Const { Bytes(b) = 0, National(n) = 1, Number(f) = 2, Figurative(f) = 3, All(b) = 4 });
codec_enum!(IntExpr { Const(n) = 0, Item(place) = 1, Fixed { expr, dmax, prepass } = 2, Walk(k) = 3 });
codec_enum!(Expr { Operand(o) = 0, Neg(e) = 1, Bin(a, op, b) = 2, Pow(base, exponent) = 3 });
codec_enum!(Cond {
    Rel { a, op, b, how } = 0,
    Class { place, test } = 1,
    Sign { value, test } = 2,
    Name { subject, values, how } = 3,
    Not(c) = 4,
    And(a, b) = 5,
    Or(a, b) = 6,
    Counter(t) = 7,
    InTable { index, count } = 8,
    Sql(test) = 9,
});
codec_enum!(Comparand { Operand(o) = 0, Expr { expr, dmax, mode, prepass } = 1 });
codec_enum!(Compare {
    PackedPfd = 0,
    Address = 1,
    Float = 2,
    Fixed = 3,
    National = 4,
    Alphanumeric = 5,
    Refused(abend) = 6,
    References = 7,
    ZonedBytes { zoned_first } = 8,
});
codec_enum!(ByteClass { Packed { signed } = 0, Zoned { signed } = 1, Digits = 2, Alphabetic = 3, AlphabeticLower = 4, AlphabeticUpper = 5 });
codec_enum!(SignTest { Positive = 0, Negative = 1, Zero = 2 });
codec_enum!(Count { Fixed(n) = 0, Odo(odo) = 1 });
codec_enum!(SqlTest { Error = 0, NotFound = 1, Warning = 2 });
