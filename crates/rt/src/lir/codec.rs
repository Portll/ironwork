//! Codecs for the types the LIR borrows from `rt`, `numeric` and `zarch`, with load-module.md's tags.

use crate::abend::{AbendCode, Ending, FileStatus, Signal};
use crate::module::ModuleError;
use crate::module::codec::{Decode, Encode, Reader, Writer};
use crate::picture::Sym;
use crate::sql::HostType;
use crate::storage::Kind;
use crate::vocab::{AcceptFrom, BinOp, Figurative, InspectMode, RelOp, SignClause, SignPosition};
use crate::{codec_enum, codec_struct};
use numeric::precision::{Fixed, Places};
use numeric::options::{FastsrtAdvPrint, Warnings};
use numeric::{Arith, Numproc, Options, SortKeys, Trunc, TruncCheck};
use zarch::check::ProgramCheck;
use zarch::ebcdic::CodePage;
use zarch::hfp::Precision;
use zarch::wide::U256;

codec_enum!(Kind {
    Group = 0,
    Alnum { justified } = 1,
    National = 2,
    Zoned { digits, scale, signed, sign } = 3,
    Packed { digits, scale, signed } = 4,
    Binary { digits, scale, signed, native } = 5,
    Float(precision) = 6,
    NumericEdited { edit, digits, scale, blank_when_zero } = 7,
    AlnumEdited { edit } = 8,
    Pointer = 9,
    Index = 10,
    ObjectReference = 11,
    ProgramPointer = 12,
});
codec_struct!(SignClause { position, separate });
codec_enum!(SignPosition { Leading = 0, Trailing = 1 });
codec_enum!(Figurative { Zero = 0, Space = 1, HighValue = 2, LowValue = 3, Quote = 4, Null = 5 });
codec_enum!(BinOp { Add = 0, Sub = 1, Mul = 2, Div = 3, Pow = 4 });
codec_enum!(RelOp { Eq = 0, Ne = 1, Lt = 2, Le = 3, Gt = 4, Ge = 5 });
codec_enum!(AcceptFrom {
    Sysin = 0,
    Date { four_digit_year } = 1,
    Day { four_digit_year } = 2,
    DayOfWeek = 3,
    Time = 4,
});
codec_enum!(InspectMode { Characters = 0, All = 1, Leading = 2, First = 3 });
codec_enum!(HostType {
    SmallInt { signed } = 0,
    Integer { signed } = 1,
    BigInt { signed } = 2,
    Decimal { digits, scale, signed } = 3,
    Zoned { digits, scale, signed, sign } = 4,
    Real = 5,
    Double = 6,
    Char(len) = 7,
    VarChar(max) = 8,
    Structure(members) = 9,
});
codec_enum!(Sym {
    Nine = 0,
    Z = 1,
    Star = 2,
    FloatLead(c) = 3,
    Float(c) = 4,
    Sign(c) = 5,
    Currency = 6,
    Cr = 7,
    Db = 8,
    Point = 9,
    Implied = 10,
    Insert(c) = 11,
    Char = 12,
});
codec_enum!(Precision { Short = 0, Long = 1, Extended = 2 });
codec_enum!(ProgramCheck {
    Specification = 0,
    Data = 1,
    FixedPointOverflow = 2,
    FixedPointDivide = 3,
    DecimalOverflow = 4,
    DecimalDivide = 5,
    HfpExponentOverflow = 6,
    HfpExponentUnderflow = 7,
    HfpSignificance = 8,
    HfpDivide = 9,
});

codec_enum!(Ending { Goback = 0, StopRun = 1, EndOfProgram = 2 });
codec_enum!(AbendCode {
    Check(check) = 0,
    Protection = 1,
    ModuleNotFound = 2,
    Io(status) = 3,
    Cics(code) = 4,
    User(code) = 5,
    Ironwork = 6,
    Exec = 7,
    Sql = 8,
    SqlReplay = 9,
    Java = 10,
    Signal(signal) = 11,
});
codec_enum!(Signal { StopRun = 0, GoBack = 1, SortStopped = 2, ClosedOutput = 3, DeclarativeExit = 4 });
codec_enum!(FileStatus {
    Success = 0,
    SuccessDuplicate = 1,
    SuccessWrongLength = 2,
    SuccessOptional = 3,
    AtEnd = 4,
    RelativeKeyOverflow = 5,
    SequenceError = 6,
    DuplicateKey = 7,
    NotFound = 8,
    BoundaryViolation = 9,
    PermanentError = 10,
    FileNotFound = 11,
    OpenModeUnsupported = 12,
    AlreadyOpen = 13,
    NotOpen = 14,
    NoPriorRead = 15,
    RecordLengthChanged = 16,
    NoNextRecord = 17,
    NotOpenInput = 18,
    NotOpenOutput = 19,
    NotOpenInputOutput = 20,
});

codec_struct!(Options {
    arith, trunc, numproc, codepage, trunc_check, fastsrt, fastsrt_adv_print, sort_keys, adv, thread, dll, rent, dbcs,
    warnings, dynam, debug,
} check options_valid);
codec_enum!(Arith { Compat = 0, Extend = 1 });
codec_enum!(Trunc { Std = 0, Opt = 1, Bin = 2 });
codec_enum!(Numproc { Nopfd = 0, Pfd = 1 });
codec_enum!(TruncCheck { Report = 0, Silent = 1 });
codec_enum!(SortKeys { Dfsort = 0, Strict = 1 });
codec_enum!(FastsrtAdvPrint { Exclude = 0, Include = 1 });
codec_enum!(Warnings { Proceed = 0, Block = 1 });

/// `Options::code_page` panics on a CCSID the tables do not carry.
fn options_valid(options: &Options) -> Result<(), String> {
    match CodePage::by_ccsid(options.codepage) {
        Some(_) => Ok(()),
        None => Err(format!("CODEPAGE({}) is not a page the tables carry", options.codepage)),
    }
}

codec_struct!(Fixed { negative, magnitude, places });
codec_struct!(Places { int, dec });

/// Four 64-bit limbs, low first.
impl Encode for U256 {
    fn encode(&self, w: &mut Writer) {
        [self.lo as u64, (self.lo >> 64) as u64, self.hi as u64, (self.hi >> 64) as u64].encode(w);
    }
}

impl Decode for U256 {
    fn decode(r: &mut Reader<'_>) -> Result<Self, ModuleError> {
        let [a, b, c, d] = <[u64; 4]>::decode(r)?;
        let limbs = |low: u64, high: u64| u128::from(low) | u128::from(high) << 64;
        Ok(U256 { hi: limbs(c, d), lo: limbs(a, b) })
    }
}
