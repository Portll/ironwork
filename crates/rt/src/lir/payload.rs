//! Statement payloads from lir.md §9: MOVE, FUNCTION and INVOKE, and a placeholder for each other.

use super::{AbendId, DebugId, ExprId, Operand, PlaceId, RefMod, StorePlan, SymId};
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
    InitPlan: "INITIALIZE's flat plan of offset, length, value and store; §9.1 does not define it yet.",
    DisplayPlan: "DISPLAY's format per item; §9.1 does not define it yet.",
    InspectPlan: "INSPECT's patterns and CONVERTING table; §9.1 does not define it yet.",
    StringPlan: "STRING's sources and delimiters; §9.1 does not define it yet.",
    UnstringPlan: "UNSTRING's delimiters and each receiver's MOVE plan; §9.1 does not define it yet.",
    SearchAllPlan: "SEARCH ALL's keys matched to WHEN terms; §9.1 does not define it yet.",
    FileOp: "A file verb with its phrases, §9.4; waits for `OpenMode`, `StartRel` and `FileDesc`.",
    FileDesc: "A file's declaration, §9.4; not defined yet.",
    CallPlan: "CALL's target and arguments, §9.3; waits for `LeService`.",
    SortPlan: "SORT or MERGE, §9.6; waits for `SortKey` and `Fastsrt`.",
    ReleasePlan: "RELEASE, §9.6; not defined yet.",
    ReturnPlan: "RETURN, §9.6; not defined yet.",
    ReportOp: "INITIATE, GENERATE, TERMINATE or SUPPRESS, §9.6; not defined yet.",
    CicsCommand: "An EXEC CICS command, §9.5; waits for the CICS code to move into rt.",
    SqlEntry: "An EXEC SQL block, §9.7; waits for the SQL code to move into rt.",
    Sqlca: "The SQLCA fields a program declares, §9.7; waits for the SQL code to move into rt.",
}
