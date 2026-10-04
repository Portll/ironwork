//! The vocabulary the front end, the interpreter and a compiled program share: source positions
//! and the small enums of the data division, INSPECT and OPEN.

use std::fmt;

/// Where a diagnostic points: 1-based source line and column.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pos {
    /// Index into the program's file table: 0 for the program itself, then each COPY member.
    pub file: u16,
    pub line: u32,
    pub col: u32,
}

impl fmt::Display for Pos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignPosition {
    Leading,
    Trailing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignClause {
    pub position: SignPosition,
    pub separate: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InspectMode {
    Characters,
    All,
    Leading,
    First,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Figurative {
    Zero,
    Space,
    HighValue,
    LowValue,
    Quote,
    Null,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenMode {
    Input,
    Output,
    Extend,
    InputOutput,
}

/// The phrases CLOSE can write after a file-name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Closing {
    /// REEL or UNIT, with FOR REMOVAL, WITH NO REWIND or neither: the volume, not the file.
    Volume,
    NoRewind,
    /// WITH LOCK: the file cannot be opened again while the program is in the run unit.
    Lock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcceptFrom {
    Sysin,
    Date { four_digit_year: bool },
    Day { four_digit_year: bool },
    DayOfWeek,
    Time,
    /// Under `--compliance extended`, the job step's PARM program arguments, as Micro Focus and
    /// GnuCOBOL give the command line.
    CommandLine,
    /// Under `--compliance extended`, how many words the PARM's program arguments hold.
    ArgumentNumber,
    /// Under `--compliance extended`, the next word of the PARM's program arguments.
    ArgumentValue,
}
