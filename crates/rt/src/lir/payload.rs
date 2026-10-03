//! Statement payloads from lir.md §9: MOVE, INITIALIZE, DISPLAY, SEARCH ALL, FUNCTION and INVOKE.
//! DISPLAY's, SEARCH ALL's, INVOKE's and those of `text`, `call` and `sql` are generic over the
//! handles they name (semantics-library.md §9, C6): the LIR's ids by default, the walker's own
//! references in the interpreter.

use super::{AbendId, Comparand, Compare, ConstId, Count, DebugId, IntExpr, Operand, PlaceId, RefMod, StorePlan, SymId};
use crate::store::LaxRedefinition;
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
    /// DBCS data or SPACE into a DBCS item, `edit` its PICTURE when it has B.
    Dbcs { justified: bool, edit: Option<u32> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Image {
    Bytes,
    All,
    Figurative,
    Digits { digits: u32 },
    /// A numeric, floating-point or pointer sender's own bytes as stored, once it has been read.
    Stored,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NationalFrom {
    Units,
    Decoded,
    Figurative,
    /// DBCS characters through the code page's DBCS component.
    Dbcs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumericFrom {
    /// The sender as a number; a zoned or packed sender through `store::move_sender`, which gives
    /// digits that are not decimal as bytes to carry unchecked (C260).
    Value,
    /// NUMPROC(PFD), packed to packed of the same kind and scale: the bytes.
    PackedCopy,
    Float,
    Zero,
    /// A figurative other than ZERO, or an ALL literal: bytes filled, not converted.
    Fill,
    /// Alphanumeric bytes read as an unsigned zoned integer of their length; to a zoned or packed
    /// integer without P scaling, the low halves of their last bytes, stored unchecked (C240).
    Zoned,
    DeEdit { edit: u32, digits: u32, scale: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloatFrom {
    Float,
    Fixed,
    Zero,
}

/// NUMCHECK's test of a MOVE's sending item, once the item is located and before it is read
/// (`rt::store::numcheck_sender`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SenderCheck {
    /// No test: no NUMCHECK, a sender that is not a data item, or a zoned sender ZON(LAX) exempts.
    None,
    /// The test `Operand::Load` makes of its item.
    Item,
    /// An alphanumeric or group sender moved to a numeric receiver: an unsigned integer's digits.
    Integer,
}

/// NUMCHECK's facts of a reference, fixed when compiled (`compile::numcheck`): under ZON(LAX), what
/// its item may hold because of the item its record redefines; and whether the compiler removed
/// the test where this reference reads the item, having found it always fails.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlaceNumcheck {
    pub lax: Option<LaxRedefinition>,
    pub removed: bool,
}

/// INITIALIZE of one item: each elementary item the walk reaches, every occurrence listed, with
/// FILLER and its phrases' choice of receivers and senders made. A reference-modified item is one
/// field, which is the item as located.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitPlan {
    pub fields: Vec<InitField>,
}

/// `value` moved into `len` bytes at `offset` from the target's start, by `store`, which was made
/// for the kind the walker stores as; `scaling` is the item's PICTURE P positions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InitField {
    pub offset: u32,
    pub len: u32,
    pub value: InitValue,
    pub store: MovePlan,
    pub scaling: u32,
}

/// What an INITIALIZE field is sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitValue {
    /// SPACE, ZERO or NULL, as the walker gives the item's kind.
    Default(Figurative),
    /// The literal of the item's own VALUE clause.
    Value(ConstId),
    /// REPLACING's operand, read for each field it is sent to.
    Replacing(Operand),
}

/// DISPLAY's items, each shown as the walker shows its kind, then a newline unless NO ADVANCING.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayPlan<P = PlaceId, O = Operand> {
    pub items: Vec<DisplayItem<P, O>>,
    pub no_advancing: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayItem<P = PlaceId, O = Operand> {
    /// Groups, alphanumeric, zoned and edited items, and national items written elsewhere than the
    /// console: the storage in the program's code page.
    Bytes(P),
    /// A national item UPON CONSOLE: converted to the program's code page.
    National(P),
    /// Packed and binary items: the value's last `digits` digits, which for COMP-5 or TRUNC(BIN)
    /// are as many as the item's halfword, fullword or doubleword holds.
    Digits { place: P, digits: u32, signed: bool },
    /// Floating-point, pointer, index and object-reference items: the place, then the abend.
    Refused { place: P, abend: AbendId },
    /// A literal or figurative constant as DISPLAY shows it; a numeric literal as written, its decimal
    /// point the program's.
    Text(SymId),
    /// FUNCTION, LENGTH OF or ADDRESS OF, by the value's kind, a national value unconverted; UPON
    /// CONSOLE a national function's value is DISPLAY-OF's.
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

/// FUNCTION, as the walker evaluates it: each argument in turn, as a comparison evaluates an
/// operand or expression; then, when the values the arguments give number outside `func.arity()`,
/// `arity`'s abend (lowering leaves `arity` None where they cannot); then the function, which for
/// CHAR, INTEGER-OF-DATE, DATE-OF-INTEGER and RANDOM evaluates its first argument again as
/// `integer`, and for NATIONAL-OF with two arguments its second; last, on an alphanumeric result,
/// `refmod`'s start and length. `refmod.check` is always false: the walker checks a function's
/// reference modification against its result whatever SSRANGE says.
///
/// HEX-OF, BIT-OF and BYTE-LENGTH read a `Load` argument's bytes as stored, and any other
/// argument's value as DISPLAY would hold it. WHEN-COMPILED reads `ProgramOptions.when_compiled`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionPlan {
    pub func: Func,
    pub args: Vec<Argument>,
    pub integer: Option<IntExpr>,
    /// TRIM's LEADING or TRAILING.
    pub side: Option<TrimSide>,
    pub refmod: Option<RefMod>,
    pub arity: Option<AbendId>,
    pub at: DebugId,
}

/// A FUNCTION argument: one value, or the elements of a table written with ALL subscripts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Argument {
    Value(Comparand),
    /// `element` is the table as written, each ALL subscript 1. Each `(position, count)` is an ALL
    /// subscript's position among the place's subscripts and its occurrences. The counts are
    /// evaluated first, left to right; then, unless one is zero, each element with its ALL
    /// subscripts set, the rightmost varying fastest, is evaluated and read as `Load` reads it.
    All { element: PlaceId, all: Vec<(u32, Count)> },
}

/// One row per intrinsic function: its variant, its tag in a load module, its name, and the
/// fewest and most arguments it takes. Adding a function is adding its row.
macro_rules! functions {
    ($($variant:ident = $tag:literal, $name:literal, $min:literal ..= $max:expr;)*) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Func {
            $($variant,)*
        }

        impl Func {
            pub const ALL: &'static [Func] = &[$(Func::$variant,)*];

            pub fn name(self) -> &'static str {
                match self {
                    $(Func::$variant => $name,)*
                }
            }

            pub fn named(name: &str) -> Option<Func> {
                match name {
                    $($name => Some(Func::$variant),)*
                    _ => None,
                }
            }

            /// The argument counts the function takes.
            pub fn arity(self) -> ::core::ops::RangeInclusive<usize> {
                match self {
                    $(Func::$variant => $min..=$max,)*
                }
            }
        }

        codec_enum!(Func { $($variant = $tag,)* });
    };
}

functions! {
    Char = 0, "CHAR", 1..=1;
    Ord = 1, "ORD", 1..=1;
    NationalOf = 2, "NATIONAL-OF", 1..=2;
    Length = 3, "LENGTH", 1..=1;
    UpperCase = 4, "UPPER-CASE", 1..=1;
    LowerCase = 5, "LOWER-CASE", 1..=1;
    Reverse = 6, "REVERSE", 1..=1;
    CurrentDate = 7, "CURRENT-DATE", 0..=0;
    Numval = 8, "NUMVAL", 1..=2;
    NumvalC = 9, "NUMVAL-C", 1..=2;
    Trim = 10, "TRIM", 1..=1;
    Mod = 11, "MOD", 2..=2;
    Rem = 12, "REM", 2..=2;
    Integer = 13, "INTEGER", 1..=1;
    IntegerPart = 14, "INTEGER-PART", 1..=1;
    Abs = 15, "ABS", 1..=1;
    Min = 16, "MIN", 1..=usize::MAX;
    Max = 17, "MAX", 1..=usize::MAX;
    IntegerOfDate = 18, "INTEGER-OF-DATE", 1..=1;
    DateOfInteger = 19, "DATE-OF-INTEGER", 1..=1;
    Random = 20, "RANDOM", 0..=1;
    Acos = 21, "ACOS", 1..=1;
    Annuity = 22, "ANNUITY", 2..=2;
    Asin = 23, "ASIN", 1..=1;
    Atan = 24, "ATAN", 1..=1;
    BitOf = 25, "BIT-OF", 1..=1;
    BitToChar = 26, "BIT-TO-CHAR", 1..=1;
    ByteLength = 27, "BYTE-LENGTH", 1..=1;
    Cos = 28, "COS", 1..=1;
    DateToYyyymmdd = 29, "DATE-TO-YYYYMMDD", 1..=2;
    DayOfInteger = 30, "DAY-OF-INTEGER", 1..=1;
    DayToYyyyddd = 31, "DAY-TO-YYYYDDD", 1..=2;
    DisplayOf = 32, "DISPLAY-OF", 1..=2;
    E = 33, "E", 0..=0;
    Exp = 34, "EXP", 1..=1;
    Exp10 = 35, "EXP10", 1..=1;
    Factorial = 36, "FACTORIAL", 1..=1;
    FormattedCurrentDate = 37, "FORMATTED-CURRENT-DATE", 1..=1;
    FormattedDate = 38, "FORMATTED-DATE", 2..=2;
    FormattedDatetime = 39, "FORMATTED-DATETIME", 3..=4;
    FormattedTime = 40, "FORMATTED-TIME", 2..=3;
    HexOf = 41, "HEX-OF", 1..=1;
    HexToChar = 42, "HEX-TO-CHAR", 1..=1;
    IntegerOfDay = 43, "INTEGER-OF-DAY", 1..=1;
    IntegerOfFormattedDate = 44, "INTEGER-OF-FORMATTED-DATE", 2..=2;
    Log = 45, "LOG", 1..=1;
    Log10 = 46, "LOG10", 1..=1;
    Mean = 47, "MEAN", 1..=usize::MAX;
    Median = 48, "MEDIAN", 1..=usize::MAX;
    Midrange = 49, "MIDRANGE", 1..=usize::MAX;
    NumvalF = 50, "NUMVAL-F", 1..=1;
    OrdMax = 51, "ORD-MAX", 1..=usize::MAX;
    OrdMin = 52, "ORD-MIN", 1..=usize::MAX;
    Pi = 53, "PI", 0..=0;
    PresentValue = 54, "PRESENT-VALUE", 2..=usize::MAX;
    Range = 55, "RANGE", 1..=usize::MAX;
    SecondsFromFormattedTime = 56, "SECONDS-FROM-FORMATTED-TIME", 2..=2;
    SecondsPastMidnight = 57, "SECONDS-PAST-MIDNIGHT", 0..=0;
    Sign = 58, "SIGN", 1..=1;
    Sin = 59, "SIN", 1..=1;
    Sqrt = 60, "SQRT", 1..=1;
    StandardDeviation = 61, "STANDARD-DEVIATION", 1..=usize::MAX;
    Sum = 62, "SUM", 1..=usize::MAX;
    Tan = 63, "TAN", 1..=1;
    TestDateYyyymmdd = 64, "TEST-DATE-YYYYMMDD", 1..=1;
    TestDayYyyyddd = 65, "TEST-DAY-YYYYDDD", 1..=1;
    TestFormattedDatetime = 66, "TEST-FORMATTED-DATETIME", 2..=2;
    TestNumval = 67, "TEST-NUMVAL", 1..=1;
    TestNumvalC = 68, "TEST-NUMVAL-C", 1..=2;
    TestNumvalF = 69, "TEST-NUMVAL-F", 1..=1;
    Uuid4 = 70, "UUID4", 0..=0;
    Variance = 71, "VARIANCE", 1..=usize::MAX;
    YearToYyyy = 72, "YEAR-TO-YYYY", 1..=2;
    WhenCompiled = 73, "WHEN-COMPILED", 0..=0;
    Ulength = 74, "ULENGTH", 1..=1;
    Upos = 75, "UPOS", 2..=2;
    Usubstr = 76, "USUBSTR", 3..=3;
    Usupplementary = 77, "USUPPLEMENTARY", 1..=1;
    Uvalid = 78, "UVALID", 1..=1;
    Uwidth = 79, "UWIDTH", 2..=2;
    CombinedDatetime = 80, "COMBINED-DATETIME", 2..=2;
    ContentOf = 81, "CONTENT-OF", 1..=1;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrimSide {
    Leading,
    Trailing,
}

/// `args` and `returning` carry each Java type signature. The op evaluates the method name, then the
/// receiver, then each argument, and returns Arm(1) when no method matches and ON EXCEPTION is
/// written, Arm(0) otherwise, or Next when neither phrase is written. `returning` is moved by the
/// kind of the value the method returns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvokePlan<P = PlaceId, O = Operand, S = SymId> {
    pub receiver: Receiver<P, S>,
    pub method: MethodName<P, S>,
    pub args: Vec<(O, S)>,
    pub returning: Option<(P, S)>,
    pub on_exception: bool,
    pub not_on_exception: bool,
}

/// `Class` is a REPOSITORY class-name: `name` as written, which messages give, and `external`, which
/// finds the class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Receiver<P = PlaceId, S = SymId> {
    SelfRef,
    Super,
    Class { name: S, external: S },
    Object(P),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MethodName<P = PlaceId, S = SymId> {
    New,
    Named(S),
    Dynamic(P),
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
    Dbcs { justified, edit } = 8,
});
codec_enum!(Image { Bytes = 0, All = 1, Figurative = 2, Digits { digits } = 3, Stored = 4 });
codec_enum!(NationalFrom { Units = 0, Decoded = 1, Figurative = 2, Dbcs = 3 });
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
codec_enum!(SenderCheck { None = 0, Item = 1, Integer = 2 });
codec_struct!(PlaceNumcheck { lax, removed });
codec_enum!(LaxRedefinition { Signed = 0, LeadingSpaces(spaces) = 1 });
codec_struct!(InitPlan { fields });
codec_struct!(InitField { offset, len, value, store, scaling });
codec_enum!(InitValue { Default(value) = 0, Value(value) = 1, Replacing(value) = 2 });
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
codec_struct!(FunctionPlan { func, args, integer, side, refmod, arity, at });
codec_enum!(Argument { Value(value) = 0, All { element, all } = 1 });
codec_enum!(TrimSide { Leading = 0, Trailing = 1 });
codec_struct!(InvokePlan { receiver, method, args, returning, on_exception, not_on_exception });
codec_enum!(Receiver { SelfRef = 0, Super = 1, Class { name, external } = 2, Object(place) = 3 });
codec_enum!(MethodName { New = 0, Named(name) = 1, Dynamic(place) = 2 });
