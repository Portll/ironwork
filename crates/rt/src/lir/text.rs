//! INSPECT, STRING and UNSTRING (lir.md §9.1): the operands and receivers `crate::strings` works over,
//! in the order the walker evaluates them.

use super::{MovePlan, Operand, PlaceId, StepPlan, StorePlan};
use crate::vocab::InspectMode;
use crate::{codec_enum, codec_struct};

/// An operand as STRING, UNSTRING, INSPECT and CALL BY CONTENT take it: bytes, not a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Chars<P = PlaceId, O = Operand> {
    /// A literal or figurative constant, its bytes made at lowering.
    Literal(Vec<u8>),
    /// A data item's storage.
    Place(P),
    /// FUNCTION, LENGTH OF or ADDRESS OF, converted as the statement converts a value.
    Value(O),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InspectPlan<P = PlaceId, O = Operand> {
    pub target: P,
    pub tallying: Vec<InspectPhrase<P, O>>,
    pub replacing: Vec<InspectPhrase<P, O>>,
    pub converting: Option<Converting<P, O>>,
}

/// `pattern` is None for CHARACTERS; `by` is REPLACING's, and `counter` TALLYING's with its add.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InspectPhrase<P = PlaceId, O = Operand> {
    pub mode: InspectMode,
    pub pattern: Option<Chars<P, O>>,
    pub by: Option<Replacement<P, O>>,
    pub counter: Option<(P, StepPlan)>,
    /// BEFORE and AFTER INITIAL as written; the last of each applies.
    pub bounds: Vec<Bound<P, O>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Replacement<P = PlaceId, O = Operand> {
    Chars(Chars<P, O>),
    /// A figurative constant's byte, as many times as the pattern is long.
    Fill(u8),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bound<P = PlaceId, O = Operand> {
    pub after: bool,
    pub value: Chars<P, O>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Converting<P = PlaceId, O = Operand> {
    pub table: ConvertTable<P, O>,
    pub bounds: Vec<Bound<P, O>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConvertTable<P = PlaceId, O = Operand> {
    /// Both operands literals: each distinct byte of the first with its byte of the second.
    Built(Vec<(u8, u8)>),
    /// Operands of different lengths abend when the statement runs.
    Operands { from: Chars<P, O>, to: Chars<P, O> },
}

/// The op returns Arm(1) on overflow and Arm(0) otherwise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StringPlan<P = PlaceId, O = Operand> {
    pub into: P,
    /// WITH POINTER: read as an integer before the first source, stored after the last.
    pub pointer: Option<(P, StorePlan)>,
    pub sources: Vec<StringSource<P, O>>,
}

/// `delimiter` is None for DELIMITED BY SIZE.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StringSource<P = PlaceId, O = Operand> {
    pub chars: Chars<P, O>,
    pub delimiter: Option<Chars<P, O>>,
}

/// The op returns Arm(1) on overflow and Arm(0) otherwise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnstringPlan<P = PlaceId, O = Operand> {
    pub source: P,
    pub pointer: Option<(P, StorePlan)>,
    /// Each DELIMITED BY operand, true for ALL.
    pub delimiters: Vec<(bool, Chars<P, O>)>,
    pub into: Vec<UnstringInto<P>>,
    /// TALLYING IN, which the count of fields filled is added to.
    pub tallying: Option<(P, StepPlan)>,
}

/// `plan` moves the field's bytes as an alphanumeric sender; COUNT IN stores the field's length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnstringInto<P = PlaceId> {
    pub target: P,
    pub plan: MovePlan,
    pub delimiter: Option<DelimiterIn<P>>,
    pub count: Option<(P, StorePlan)>,
}

/// `found` moves the delimiter that ended the field, and `none` moves SPACE when none did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DelimiterIn<P = PlaceId> {
    pub target: P,
    pub found: MovePlan,
    pub none: MovePlan,
}

codec_enum!(Chars { Literal(bytes) = 0, Place(place) = 1, Value(value) = 2 });
codec_struct!(InspectPlan { target, tallying, replacing, converting });
codec_struct!(InspectPhrase { mode, pattern, by, counter, bounds });
codec_enum!(Replacement { Chars(chars) = 0, Fill(byte) = 1 });
codec_struct!(Bound { after, value });
codec_struct!(Converting { table, bounds });
codec_enum!(ConvertTable { Built(pairs) = 0, Operands { from, to } = 1 });
codec_struct!(StringPlan { into, pointer, sources });
codec_struct!(StringSource { chars, delimiter });
codec_struct!(UnstringPlan { source, pointer, delimiters, into, tallying });
codec_struct!(UnstringInto { target, plan, delimiter, count });
codec_struct!(DelimiterIn { target, found, none });
