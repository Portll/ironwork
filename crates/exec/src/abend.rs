//! The code an abend reports, as z/OS, CICS or ironwork names it, and the signals the interpreter
//! passes up as errors to leave a statement early.

use crate::files::FileStatus;
use std::fmt;
use zarch::check::ProgramCheck;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AbendCode {
    /// A program check nothing handled: S0C6 to S0CF.
    Check(ProgramCheck),
    /// S0C4: an address outside the run unit's storage.
    Protection,
    /// S806: a CALLed program that is not in the library.
    ModuleNotFound,
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
    /// A zero divisor: the arithmetic statement takes it as a size error or a program check.
    DivideByZero,
    /// STOP RUN in a USE BEFORE REPORTING procedure: the report statement that ran it ends the run.
    StopRun,
    /// GOBACK in a USE BEFORE REPORTING procedure.
    GoBack,
    /// RELEASE or RETURN stops the SORT or MERGE, with why as the message.
    SortStopped,
    /// The reader of DISPLAY output went away, as `head` does: not the program's failure.
    ClosedOutput,
}

impl Signal {
    const ALL: [Self; 5] = [Self::DivideByZero, Self::StopRun, Self::GoBack, Self::SortStopped, Self::ClosedOutput];

    fn text(self) -> &'static str {
        match self {
            Self::DivideByZero => "DIVIDE-BY-ZERO",
            Self::StopRun => "REPORT-STOP-RUN",
            Self::GoBack => "REPORT-GOBACK",
            Self::SortStopped => "SORT-STOPPED",
            Self::ClosedOutput => "CLOSED-OUTPUT",
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

    /// For SORT's FASTSRT reader, which still tests the text.
    pub fn starts_with(&self, prefix: &str) -> bool {
        self.as_str().starts_with(prefix)
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

/// The code that prints as `text`, for the SQL runtime and SORT, which still name theirs as text.
impl From<&str> for AbendCode {
    fn from(text: &str) -> Self {
        let user = text.len() == 5 && text.starts_with('U') && text[1..].bytes().all(|b| b.is_ascii_digit());
        CHECKS
            .map(Self::Check)
            .into_iter()
            .chain([Self::Protection, Self::ModuleNotFound, Self::Ironwork, Self::Exec, Self::Sql, Self::SqlReplay, Self::Java])
            .chain(FileStatus::ALL.map(Self::Io))
            .chain(Signal::ALL.map(Self::Signal))
            .find(|c| c.as_str() == text)
            .unwrap_or_else(|| if user { Self::User(text.into()) } else { Self::Cics(text.into()) })
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
        for text in ["S0C7", "S0C4", "S806", "IO-35", "IO-46", "AEIP", "ASRA", "0999", "U4038", "IRONWORK", "EXEC", "SQL", "SQLR", "JAVA", "SORT-STOPPED", "CLOSED-OUTPUT"] {
            assert_eq!(AbendCode::from(text).to_string(), text);
        }
        assert_eq!(AbendCode::from("U4038"), AbendCode::user(4038));
        assert_eq!(AbendCode::from("IO-46"), AbendCode::Io(FileStatus::NoNextRecord));
        assert_eq!(AbendCode::from("SORT-STOPPED"), AbendCode::Signal(Signal::SortStopped));
    }
}
