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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcceptFrom {
    Sysin,
    Date { four_digit_year: bool },
    Day { four_digit_year: bool },
    DayOfWeek,
    Time,
}
