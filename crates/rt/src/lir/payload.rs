//! Statement payloads from lir.md §9: MOVE, INITIALIZE, DISPLAY, SEARCH ALL, FUNCTION and INVOKE,
//! and a placeholder for each payload whose service is not in `rt` yet. DISPLAY's, SEARCH ALL's and
//! those of `text`, `call` and `sql` are generic over the handles they name (semantics-library.md §9,
//! C6): the LIR's ids by default, the walker's own references in the interpreter.

use super::{AbendId, Comparand, Compare, Count, DebugId, ExprId, Operand, PlaceId, RefMod, StorePlan, SymId};
use crate::vocab::Figurative;
use crate::{codec_enum, codec_struct};
use zarch::hfp::Precision;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MovePlan {
    Alnum { image: Image, justified: bool },
    AlnumEdited { image: Image, edit: u32, positions: u32 },
    National(NationalFrom),
    Numeric { from: NumericFrom, store: StorePlan },
    Float { from: FloatFrom, precision: Precision },
    Address,
    Index,
    Refused(AbendId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Image {
    Bytes,
    All,
    Figurative,
    Digits { digits: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NationalFrom {
    Units,
    Decoded,
    Figurative,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumericFrom {
    Value,
    /// NUMPROC(PFD), packed to packed of the same kind and scale: the bytes.
    PackedCopy,
    Float,
    Zero,
    /// A figurative other than ZERO, or an ALL literal: bytes filled, not converted.
    Fill,
    /// Alphanumeric bytes read as an unsigned zoned integer of their length.
    Zoned,
    DeEdit { edit: u32, digits: u32, scale: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloatFrom {
    Float,
    Fixed,
    Zero,
}

/// INITIALIZE of one item: each elementary item the walk reaches, every occurrence listed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitPlan {
    pub fields: Vec<InitField>,
}

/// SPACE, ZERO or NULL, as the walker gives the item's kind, moved into `len` bytes at `offset` from
/// the target's start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InitField {
    pub offset: u32,
    pub len: u32,
    pub value: Figurative,
    pub store: MovePlan,
}

/// DISPLAY's items, each shown as the walker shows its kind, then a newline unless NO ADVANCING.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayPlan<P = PlaceId, O = Operand> {
    pub items: Vec<DisplayItem<P, O>>,
    pub no_advancing: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayItem<P = PlaceId, O = Operand> {
    /// Groups, alphanumeric, zoned and edited items: the storage in the program's code page.
    Bytes(P),
    National(P),
    /// Packed and binary items: the value's last `digits` digits, which for COMP-5 or TRUNC(BIN)
    /// are as many as the item's halfword, fullword or doubleword holds.
    Digits { place: P, digits: u32, signed: bool },
    /// Floating-point, pointer, index and object-reference items: the place, then the abend.
    Refused { place: P, abend: AbendId },
    /// A literal or figurative constant as DISPLAY shows it; a numeric literal as written.
    Text(SymId),
    /// FUNCTION, LENGTH OF or ADDRESS OF, by the value's kind.
    Value(O),
}

/// SEARCH ALL's binary search. The op returns Arm(0) on an occurrence whose keys equal their WHEN
/// terms, for a Branch on the whole condition, and Arm(1) when the search ends without one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchAllPlan<P = PlaceId, V = Comparand, N = Count> {
    /// The table's first index, stored by `store` at each occurrence tried.
    pub index: P,
    pub store: StorePlan,
    pub count: N,
    /// The keys the WHEN condition tests for equality, in the table's KEY order.
    pub keys: Vec<SearchKey<V>>,
}

/// `key` as the WHEN term names it, and the `value` it must equal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchKey<V = Comparand> {
    pub ascending: bool,
    pub key: V,
    pub value: V,
    pub how: Compare,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionPlan {
    pub func: Func,
    pub args: Vec<ExprId>,
    pub side: Option<TrimSide>,
    pub refmod: Option<RefMod>,
    pub at: DebugId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Func {
    Char,
    Ord,
    NationalOf,
    Length,
    UpperCase,
    LowerCase,
    Reverse,
    CurrentDate,
    Numval,
    NumvalC,
    Trim,
    Mod,
    Rem,
    Integer,
    IntegerPart,
    Abs,
    Min,
    Max,
    IntegerOfDate,
    DateOfInteger,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrimSide {
    Leading,
    Trailing,
}

/// `args` and `returning` carry each Java type signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvokePlan {
    pub receiver: Receiver,
    pub method: MethodName,
    pub args: Vec<(Operand, SymId)>,
    pub returning: Option<(PlaceId, SymId)>,
    pub on_exception: bool,
    pub not_on_exception: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Receiver {
    SelfRef,
    Super,
    Class(SymId),
    Object(PlaceId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MethodName {
    New,
    Named(SymId),
    Dynamic(PlaceId),
}

codec_enum!(MovePlan {
    Alnum { image, justified } = 0,
    AlnumEdited { image, edit, positions } = 1,
    National(from) = 2,
    Numeric { from, store } = 3,
    Float { from, precision } = 4,
    Address = 5,
    Index = 6,
    Refused(abend) = 7,
});
codec_enum!(Image { Bytes = 0, All = 1, Figurative = 2, Digits { digits } = 3 });
codec_enum!(NationalFrom { Units = 0, Decoded = 1, Figurative = 2 });
codec_enum!(NumericFrom {
    Value = 0,
    PackedCopy = 1,
    Float = 2,
    Zero = 3,
    Fill = 4,
    Zoned = 5,
    DeEdit { edit, digits, scale } = 6,
});
codec_enum!(FloatFrom { Float = 0, Fixed = 1, Zero = 2 });
codec_struct!(InitPlan { fields });
codec_struct!(InitField { offset, len, value, store });
codec_struct!(DisplayPlan { items, no_advancing });
codec_enum!(DisplayItem {
    Bytes(place) = 0,
    National(place) = 1,
    Digits { place, digits, signed } = 2,
    Refused { place, abend } = 3,
    Text(text) = 4,
    Value(value) = 5,
});
codec_struct!(SearchAllPlan { index, store, count, keys });
codec_struct!(SearchKey { ascending, key, value, how });
codec_struct!(FunctionPlan { func, args, side, refmod, at });
codec_enum!(Func {
    Char = 0,
    Ord = 1,
    NationalOf = 2,
    Length = 3,
    UpperCase = 4,
    LowerCase = 5,
    Reverse = 6,
    CurrentDate = 7,
    Numval = 8,
    NumvalC = 9,
    Trim = 10,
    Mod = 11,
    Rem = 12,
    Integer = 13,
    IntegerPart = 14,
    Abs = 15,
    Min = 16,
    Max = 17,
    IntegerOfDate = 18,
    DateOfInteger = 19,
});
codec_enum!(TrimSide { Leading = 0, Trailing = 1 });
codec_struct!(InvokePlan { receiver, method, args, returning, on_exception, not_on_exception });
codec_enum!(Receiver { SelfRef = 0, Super = 1, Class(name) = 2, Object(place) = 3 });
codec_enum!(MethodName { New = 0, Named(name) = 1, Dynamic(place) = 2 });

macro_rules! placeholder {
    ($($ty:ident: $doc:literal,)*) => {$(
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
        pub enum $ty {
            #[default]
            Placeholder,
        }

        codec_enum!($ty { Placeholder = 0 });
    )*};
}

placeholder! {
    FileOp: "A file verb with its phrases, §9.4; waits for `OpenMode`, `StartRel` and `FileDesc`.",
    FileDesc: "A file's declaration, §9.4; not defined yet.",
    SortPlan: "SORT or MERGE, §9.6; waits for `SortKey` and `Fastsrt`.",
    ReleasePlan: "RELEASE, §9.6; not defined yet.",
    ReturnPlan: "RETURN, §9.6; not defined yet.",
    ReportOp: "INITIATE, GENERATE, TERMINATE or SUPPRESS, §9.6; not defined yet.",
    CicsCommand: "An EXEC CICS command, §9.5; waits for the CICS code to move into rt.",
}
