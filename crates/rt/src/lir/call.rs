//! CALL and the LE callable services behind it, and the ENTRY points a CALL enters (lir.md §9.3);
//! invoking a user-defined function, and what its definition gives an invocation (§9.15). CANCEL
//! is `Op::Cancel`.

use super::{BlockId, Chars, Comparand, DebugId, Operand, ParaId, PlaceId, RefMod, SymId};
use crate::{codec_enum, codec_struct};

/// The op returns Arm(0) after a normal return, Arm(1) when no program has the name and ON
/// EXCEPTION is written, or End(StopRun); Next rather than Arm(0) when neither phrase is written.
/// `returning` is moved by the callee's RETURNING kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallPlan<P = PlaceId, O = Operand> {
    pub target: CallTarget<P, O>,
    pub args: Vec<CallArg<P, O>>,
    pub returning: Option<P>,
    pub on_exception: bool,
    pub not_on_exception: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallTarget<P = PlaceId, O = Operand> {
    /// A literal: the program is found when the CALL runs, and `le` runs when none has the name.
    Named { name: SymId, le: Option<LeService> },
    /// An identifier, its value decoded, trimmed and upper-cased when the CALL runs.
    Dynamic(O),
    /// A FUNCTION-POINTER or PROCEDURE-POINTER, which holds a JNI service. Its arguments are all
    /// `Value` or `Omitted`: the service takes each operand's value as it reads, OMITTED as NULL.
    Pointer(P),
    /// Any other FUNCTION-POINTER or PROCEDURE-POINTER: the program of the entry SET TO ENTRY gave
    /// it, called by its name and as it was loaded; a value naming no entry is `Pointer`'s service.
    Entry(P),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallArg<P = PlaceId, O = Operand> {
    Reference(P),
    /// Copied to a temporary; BY REFERENCE of anything but a data item passes as this. A literal's
    /// bytes are those its own data item would hold; LENGTH OF is a binary fullword.
    Content(Chars<P, O>),
    /// A fullword, an address or bytes, from the value.
    Value(O),
    Omitted,
}

/// An ENTRY statement: a CALL of `name` starts at `block`, the statement after the ENTRY in
/// paragraph `paragraph`, in the Main frame, with `using` in place of `Storage.using`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryPoint {
    pub name: SymId,
    pub paragraph: ParaId,
    pub block: BlockId,
    pub using: Vec<u16>,
}

/// The LE callable services the runtime provides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeService {
    Cee3abd,
    Cee3dmp,
    Ceedate,
    Ceedatm,
    Ceedays,
    Ceedywk,
    Ceefrst,
    Ceegmt,
    Ceegmto,
    Ceegtst,
    Ceeloct,
    Ceemout,
    Ceesecs,
    Ceeutc,
}

/// An invocation of a user-defined function, `Operand::UserFunction`: its definition is loaded by
/// `external`, `name` as written naming it in messages; each argument is evaluated in turn; the
/// function runs as a callee (`rt::callee::By::Function`); its value is its RETURNING item's,
/// reference-modified last by `refmod`, which is checked whatever SSRANGE says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserFunctionPlan {
    pub name: SymId,
    pub external: SymId,
    pub args: Vec<UserArgument>,
    pub refmod: Option<RefMod>,
    pub at: DebugId,
}

/// A data item passed BY REFERENCE passes its address. Any other argument passes its value, which
/// the function's activation moves into a temporary its formal parameter describes (C272). An
/// alphanumeric literal for an ANY LENGTH parameter passes its bytes, in a temporary of their
/// length, which is the parameter's length (C481).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UserArgument {
    Reference(PlaceId),
    Value(Comparand),
    Literal(Vec<u8>),
}

/// A user-defined function's definition: a place naming each formal parameter's whole LINKAGE
/// record, in order, and one naming the RETURNING record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionDefinition {
    pub params: Vec<PlaceId>,
    pub returning: PlaceId,
}

codec_struct!(CallPlan { target, args, returning, on_exception, not_on_exception });
codec_struct!(UserFunctionPlan { name, external, args, refmod, at });
codec_enum!(UserArgument { Reference(place) = 0, Value(value) = 1, Literal(bytes) = 2 });
codec_struct!(FunctionDefinition { params, returning });
codec_enum!(CallTarget { Named { name, le } = 0, Dynamic(name) = 1, Pointer(place) = 2, Entry(place) = 3 });
codec_enum!(CallArg { Reference(place) = 0, Content(chars) = 1, Value(value) = 2, Omitted = 3 });
codec_struct!(EntryPoint { name, paragraph, block, using });
codec_enum!(LeService {
    Cee3abd = 0,
    Cee3dmp = 1,
    Ceedate = 2,
    Ceedatm = 3,
    Ceedays = 4,
    Ceedywk = 5,
    Ceefrst = 6,
    Ceegmt = 7,
    Ceegmto = 8,
    Ceegtst = 9,
    Ceeloct = 10,
    Ceemout = 11,
    Ceesecs = 12,
    Ceeutc = 13,
});
