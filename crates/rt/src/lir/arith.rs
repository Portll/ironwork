//! Arithmetic plans (lir.md §7): what the walker works out on each execution, fixed at lowering.

use super::{AbendId, ExprId, PlaceId, SymId};
use crate::vocab::SignClause;
use crate::{codec_enum, codec_struct};
use numeric::{Arith, Native};
use zarch::hfp::Precision;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArithPlan {
    pub dmax: u32,
    pub arith: Arith,
    /// The places the walker's dmax pre-pass locates, in its order, static ones left out.
    pub prepass: Vec<PlaceId>,
    pub steps: Vec<ArithStep>,
    pub remainder: Option<RemainderPlan>,
    /// ON or NOT ON SIZE ERROR is written.
    pub handled: bool,
    /// ADD, SUBTRACT, MULTIPLY or DIVIDE rather than COMPUTE: a step whose expression has its own
    /// receiver as an operand of its top operation evaluates the other operand with every step's,
    /// and reads the receiver only when it stores.
    pub per_receiver: bool,
    /// The dmax of every operation below a step's top one, which `dmax` is for: lower than `dmax`
    /// only when C101 is gnucobol, where a ROUNDED receiver's extra place counts in the top
    /// operation alone (dialect.md).
    pub inner_dmax: u32,
}

/// `probe` holds the places the walker's float test locates after the receiver, static ones left out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArithStep {
    pub target: PlaceId,
    pub expr: ExprId,
    pub mode: Mode,
    pub store: StorePlan,
    pub rounded: bool,
    pub probe: Vec<PlaceId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Fixed,
    Float(Precision),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorePlan {
    Zoned { digits: u32, scale: u32, signed: bool, sign: Option<SignClause> },
    Packed { digits: u32, scale: u32, signed: bool },
    /// `name` labels a TRUNC(OPT) report.
    Binary { digits: u32, scale: u32, signed: bool, native: Native, name: SymId },
    NumericEdited { edit: u32, digits: u32, scale: u32, blank_when_zero: bool },
    Float(Precision),
    Index,
    Refused(AbendId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RemainderPlan {
    pub target: PlaceId,
    pub dividend: ExprId,
    pub divisor: ExprId,
    pub quotient_scale: u32,
    pub store: StorePlan,
}

/// PERFORM VARYING's step, SET UP and DOWN BY, and the TALLYING adds: no ROUNDED, no size error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StepPlan {
    pub dmax: u32,
    pub store: StorePlan,
}

/// SET UP BY or DOWN BY on one receiver, by what reading it gives: an address moved by the step,
/// a number the step is added to (no ROUNDED, no size error), or a value the walker refuses once
/// it has read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpDown {
    Pointer,
    Number(StepPlan),
    Refused(AbendId),
}

codec_struct!(ArithPlan { dmax, arith, prepass, steps, remainder, handled, per_receiver, inner_dmax });
codec_struct!(ArithStep { target, expr, mode, store, rounded, probe });
codec_enum!(Mode { Fixed = 0, Float(precision) = 1 });
codec_enum!(StorePlan {
    Zoned { digits, scale, signed, sign } = 0,
    Packed { digits, scale, signed } = 1,
    Binary { digits, scale, signed, native, name } = 2,
    NumericEdited { edit, digits, scale, blank_when_zero } = 3,
    Float(precision) = 4,
    Index = 5,
    Refused(abend) = 6,
});
codec_struct!(RemainderPlan { target, dividend, divisor, quotient_scale, store });
codec_struct!(StepPlan { dmax, store });
codec_enum!(UpDown { Pointer = 0, Number(plan) = 1, Refused(abend) = 2 });
