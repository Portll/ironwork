//! The code an abend reports, as z/OS, CICS or ironwork names it, and the signals the interpreter
//! passes up as errors to leave a statement early. With them, the I/O statuses FILE STATUS
//! receives, and how a run ends.

use crate::cics::Condition;
use crate::vocab::Pos;
use std::fmt;
use zarch::check::ProgramCheck;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Abend {
    pub code: AbendCode,
    pub message: String,
    pub pos: Pos,
    /// The source the abend is in, named by the innermost program or method it left: a library
    /// file by path, otherwise from its source table, where the first program's source is empty.
    /// None for an abend in the first program.
    pub file: Option<String>,
}

impl Abend {
    /// A program check, with Language Environment's message for it (assumption C455).
    pub fn check(c: ProgramCheck, pos: Pos) -> Self {
        let (id, what) = match c {
            ProgramCheck::Specification => ("CEE3206S", "a specification"),
            ProgramCheck::Data => ("CEE3207S", "a data"),
            ProgramCheck::FixedPointOverflow => ("CEE3208S", "a fixed-point overflow"),
            ProgramCheck::FixedPointDivide => ("CEE3209S", "a fixed-point divide"),
            ProgramCheck::DecimalOverflow => ("CEE3210S", "a decimal-overflow"),
            ProgramCheck::DecimalDivide => ("CEE3211S", "a decimal-divide"),
            ProgramCheck::HfpExponentOverflow => ("CEE3212S", "an exponent-overflow"),
            ProgramCheck::HfpExponentUnderflow => ("CEE3213S", "an exponent-underflow"),
            ProgramCheck::HfpSignificance => ("CEE3214S", "a significance"),
            ProgramCheck::HfpDivide => ("CEE3215S", "a floating-point divide"),
        };
        let code = AbendCode::Check(c);
        let message = format!("{id} The system detected {what} exception (System Completion Code={}).", &code.as_str()[1..]);
        Self { code, message, pos, file: None }
    }

    pub fn ironwork(message: impl Into<String>, pos: Pos) -> Self {
        Self { code: AbendCode::Ironwork, message: message.into(), pos, file: None }
    }

    /// Zero raised to a negative power: IGZ0050S, a severity-3 condition that ends the run U4038,
    /// which ON SIZE ERROR takes as a size error (assumption C334).
    pub fn zero_power(pos: Pos) -> Self {
        Self { code: AbendCode::user(4038), message: "IGZ0050S A zero base was raised to a negative power in an exponentiation expression.".into(), pos, file: None }
    }

    /// Whether an arithmetic statement with ON SIZE ERROR takes this abend as a size error: a zero
    /// divisor's program check, or zero raised to a negative power.
    pub fn size_error(&self) -> bool {
        self.code.zero_divisor() || self.code == AbendCode::user(4038) && self.message.starts_with("IGZ0050S ")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ending {
    Goback,
    StopRun,
    EndOfProgram,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AbendCode {
    /// A program check nothing handled: S0C6 to S0CF.
    Check(ProgramCheck),
    /// S0C4: an address outside the run unit's storage.
    Protection,
    /// S806: a job step's program that is not in the library.
    ModuleNotFound,
    /// S322: the run reached its statement limit, as a step that runs past its TIME= ends.
    TimeLimit,
    /// IO- and the status: a failing status with no FILE STATUS to hold it.
    Io(FileStatus),
    /// A CICS transaction abend code: a condition's default abend, ABCODE, ASRA.
    Cics(String),
    /// U and four decimal digits.
    User(String),
    Ironwork,
    Exec,
    Sql,
    /// A call the SQL recording does not hold.
    SqlReplay,
    Java,
    Signal(Signal),
}

/// Control flow passed up as an error to the statement that takes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    /// STOP RUN in a USE BEFORE REPORTING procedure or a user-defined function: the statement that
    /// ran it ends the run.
    StopRun,
    /// GOBACK in a USE BEFORE REPORTING procedure.
    GoBack,
    /// RELEASE or RETURN stops the SORT or MERGE, with why as the message.
    SortStopped,
    /// The reader of DISPLAY output went away, as `head` does: not the program's failure.
    ClosedOutput,
    /// An EXCEPTION/ERROR procedure ended in GO TO, STOP RUN or GOBACK, which the statement that
    /// ran it carries out.
    DeclarativeExit,
}

impl Signal {
    const ALL: [Self; 5] = [Self::StopRun, Self::GoBack, Self::SortStopped, Self::ClosedOutput, Self::DeclarativeExit];

    fn text(self) -> &'static str {
        match self {
            Self::StopRun => "REPORT-STOP-RUN",
            Self::GoBack => "REPORT-GOBACK",
            Self::SortStopped => "SORT-STOPPED",
            Self::ClosedOutput => "CLOSED-OUTPUT",
            Self::DeclarativeExit => "DECLARATIVE-EXIT",
        }
    }
}

const CHECKS: [ProgramCheck; 10] = [
    ProgramCheck::Specification,
    ProgramCheck::Data,
    ProgramCheck::FixedPointOverflow,
    ProgramCheck::FixedPointDivide,
    ProgramCheck::DecimalOverflow,
    ProgramCheck::DecimalDivide,
    ProgramCheck::HfpExponentOverflow,
    ProgramCheck::HfpExponentUnderflow,
    ProgramCheck::HfpSignificance,
    ProgramCheck::HfpDivide,
];

fn check_code(c: ProgramCheck) -> &'static str {
    match c {
        ProgramCheck::Specification => "S0C6",
        ProgramCheck::Data => "S0C7",
        ProgramCheck::FixedPointOverflow => "S0C8",
        ProgramCheck::FixedPointDivide => "S0C9",
        ProgramCheck::DecimalOverflow => "S0CA",
        ProgramCheck::DecimalDivide => "S0CB",
        ProgramCheck::HfpExponentOverflow => "S0CC",
        ProgramCheck::HfpExponentUnderflow => "S0CD",
        ProgramCheck::HfpSignificance => "S0CE",
        ProgramCheck::HfpDivide => "S0CF",
    }
}

impl AbendCode {
    pub fn user(code: u16) -> Self {
        Self::User(format!("U{code:04}"))
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Check(c) => check_code(*c),
            Self::Protection => "S0C4",
            Self::ModuleNotFound => "S806",
            Self::TimeLimit => "S322",
            Self::Io(status) => status.abend_code(),
            Self::Cics(code) | Self::User(code) => code,
            Self::Ironwork => "IRONWORK",
            Self::Exec => "EXEC",
            Self::Sql => "SQL",
            Self::SqlReplay => "SQLR",
            Self::Java => "JAVA",
            Self::Signal(s) => s.text(),
        }
    }

    /// A program check or a system abend, which Language Environment is not told of under TRAP(OFF);
    /// a condition it signals, as for a failing I/O status or a U code, it handles either way.
    pub fn bypasses_trap_off(&self) -> bool {
        matches!(self, Self::Check(_) | Self::Protection | Self::TimeLimit)
    }

    /// The program check a zero divisor raises: decimal, fixed-point or HFP divide. An arithmetic
    /// statement with ON SIZE ERROR takes it as a size error instead.
    pub fn zero_divisor(&self) -> bool {
        matches!(self, Self::Check(ProgramCheck::DecimalDivide | ProgramCheck::FixedPointDivide | ProgramCheck::HfpDivide))
    }
}

impl fmt::Display for AbendCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialEq<&str> for AbendCode {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

/// The code that prints as `text`, for the SQL runtime, which names its codes as text.
impl From<&str> for AbendCode {
    fn from(text: &str) -> Self {
        let user = text.len() == 5 && text.starts_with('U') && text[1..].bytes().all(|b| b.is_ascii_digit());
        CHECKS
            .map(Self::Check)
            .into_iter()
            .chain([Self::Protection, Self::ModuleNotFound, Self::TimeLimit, Self::Ironwork, Self::Exec, Self::Sql, Self::SqlReplay, Self::Java])
            .chain(FileStatus::ALL.map(Self::Io))
            .chain(Signal::ALL.map(Self::Signal))
            .find(|c| c.as_str() == text)
            .unwrap_or_else(|| if user { Self::User(text.into()) } else { Self::Cics(text.into()) })
    }
}

/// An I/O status, as FILE STATUS receives it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileStatus {
    Success,
    SuccessDuplicate,
    SuccessWrongLength,
    SuccessOptional,
    AtEnd,
    RelativeKeyOverflow,
    SequenceError,
    DuplicateKey,
    NotFound,
    BoundaryViolation,
    PermanentError,
    FileNotFound,
    OpenModeUnsupported,
    AlreadyOpen,
    NotOpen,
    NoPriorRead,
    RecordLengthChanged,
    NoNextRecord,
    NotOpenInput,
    NotOpenOutput,
    NotOpenInputOutput,
    /// Successful, for a phrase that applies only to a reel or unit medium.
    SuccessNonReel,
    ClosedWithLock,
    /// A successful VSAM OPEN once the data set's integrity is verified, under VSAMOPENFS(COMPAT).
    SuccessVerified,
}

impl FileStatus {
    pub(crate) const ALL: [Self; 24] = [
        Self::Success,
        Self::SuccessDuplicate,
        Self::SuccessWrongLength,
        Self::SuccessOptional,
        Self::AtEnd,
        Self::RelativeKeyOverflow,
        Self::SequenceError,
        Self::DuplicateKey,
        Self::NotFound,
        Self::BoundaryViolation,
        Self::PermanentError,
        Self::FileNotFound,
        Self::OpenModeUnsupported,
        Self::AlreadyOpen,
        Self::NotOpen,
        Self::NoPriorRead,
        Self::RecordLengthChanged,
        Self::NoNextRecord,
        Self::NotOpenInput,
        Self::NotOpenOutput,
        Self::NotOpenInputOutput,
        Self::SuccessNonReel,
        Self::ClosedWithLock,
        Self::SuccessVerified,
    ];

    /// The code a run ends with when this status fails a statement and no FILE STATUS holds it.
    pub fn abend_code(self) -> &'static str {
        match self {
            Self::Success => "IO-00",
            Self::SuccessDuplicate => "IO-02",
            Self::SuccessWrongLength => "IO-04",
            Self::SuccessOptional => "IO-05",
            Self::SuccessNonReel => "IO-07",
            Self::AtEnd => "IO-10",
            Self::RelativeKeyOverflow => "IO-14",
            Self::SequenceError => "IO-21",
            Self::DuplicateKey => "IO-22",
            Self::NotFound => "IO-23",
            Self::BoundaryViolation => "IO-24",
            Self::PermanentError => "IO-30",
            Self::FileNotFound => "IO-35",
            Self::OpenModeUnsupported => "IO-37",
            Self::ClosedWithLock => "IO-38",
            Self::AlreadyOpen => "IO-41",
            Self::NotOpen => "IO-42",
            Self::NoPriorRead => "IO-43",
            Self::RecordLengthChanged => "IO-44",
            Self::NoNextRecord => "IO-46",
            Self::NotOpenInput => "IO-47",
            Self::NotOpenOutput => "IO-48",
            Self::NotOpenInputOutput => "IO-49",
            Self::SuccessVerified => "IO-97",
        }
    }

    pub fn as_str(self) -> &'static str {
        &self.abend_code()["IO-".len()..]
    }

    /// Whether the status is of class `class`, its first digit: 0 success, 1 AT END, 2 INVALID KEY.
    pub fn covers(self, class: char) -> bool {
        self.as_str().starts_with(class)
    }

    /// Whether a failing status that nothing takes ends the run: all but 97, which reports an OPEN
    /// that succeeded (Language Reference SC27-8713-03, p. 303).
    pub fn ends_the_run(self) -> bool {
        self != Self::SuccessVerified
    }

    /// What a failing status means, for the message when no FILE STATUS or phrase takes it.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::AtEnd => "there is no next record",
            Self::SequenceError => "the key is out of sequence",
            Self::DuplicateKey => "a record with that key is already there",
            Self::NotFound => "there is no record with that key",
            Self::RelativeKeyOverflow => "the record number is too large for the RELATIVE KEY",
            Self::BoundaryViolation => "the record number is outside the file",
            Self::NoPriorRead => "the last statement on the file was not a successful READ",
            Self::RecordLengthChanged => "the record is not the length of the one it replaces, or outside the lengths RECORD IS VARYING allows",
            Self::NoNextRecord => "there is no next record: the last READ reached the end, or START found nothing",
            Self::NotOpenInput => "the file is not open INPUT or I-O",
            Self::NotOpenOutput => "the file is not open for output",
            Self::NotOpenInputOutput => "the file is not open I-O",
            Self::ClosedWithLock => "the file was closed WITH LOCK",
            _ => "the statement failed",
        }
    }

    /// The CICS condition a keyed store's failure raises.
    pub fn cics_condition(self) -> Condition {
        match self {
            Self::DuplicateKey => Condition::DUPREC,
            Self::NotFound => Condition::NOTFND,
            _ => Condition::INVREQ,
        }
    }
}

/// The status a two-digit code names, for SORT, which still passes its statuses as text.
impl From<&str> for FileStatus {
    fn from(code: &str) -> Self {
        Self::ALL.into_iter().find(|s| s.as_str() == code).unwrap_or_else(|| panic!("{code} is not a file status ironwork sets"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_code_prints_as_the_string_it_replaced_and_parses_back_from_it() {
        for c in CHECKS {
            assert_eq!(AbendCode::Check(c).to_string(), c.abend());
        }
        for text in ["S0C7", "S0C4", "S806", "S322", "IO-35", "IO-46", "AEIP", "ASRA", "0999", "U4038", "IRONWORK", "EXEC", "SQL", "SQLR", "JAVA", "SORT-STOPPED", "CLOSED-OUTPUT"] {
            assert_eq!(AbendCode::from(text).to_string(), text);
        }
        assert_eq!(AbendCode::from("U4038"), AbendCode::user(4038));
        assert_eq!(AbendCode::from("IO-46"), AbendCode::Io(FileStatus::NoNextRecord));
        assert_eq!(AbendCode::from("SORT-STOPPED"), AbendCode::Signal(Signal::SortStopped));
    }
}
