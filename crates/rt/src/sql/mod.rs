//! The SQL runtime: values at the database boundary, the host variables they come from and go to,
//! and EXEC SQL statements run against the session (`run`, over the executor's `SqlHost`). It
//! reaches storage only as a byte range and a host type, so any executor can call it.

mod convert;
mod database;
mod host;
mod postgres;
mod replay;
mod run;

pub use convert::{ReadError, Written, read, write};
pub use database::{Abandoned, Answer, Call, Database, OpenCursor, Outcome, Session};
pub use postgres::{Postgres, Stream, Tls};
pub use replay::{Recorder, Replay};
pub use run::{Ran, SqlHost, run};

use crate::vocab::SignClause;

/// A value at the database boundary.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Int(i64),
    /// The unscaled value and its decimal places.
    Decimal { value: i128, scale: u32 },
    Double(f64),
    Char(String),
    Binary(Vec<u8>),
}

impl Value {
    /// The value as a recording writes it, a string without its trailing blanks; NULL as nothing.
    pub fn text(&self) -> String {
        match self {
            Value::Null => String::new(),
            Value::Int(n) => n.to_string(),
            Value::Decimal { value, scale } => Self::decimal_text(*value, *scale),
            Value::Double(d) => d.to_string(),
            Value::Char(s) => s.trim_end().to_owned(),
            Value::Binary(b) => b.iter().map(|byte| format!("{byte:02X}")).collect(),
        }
    }

    /// A decimal's text, `-1234.50` for -123450 at scale 2, as recordings and backends write it.
    pub fn decimal_text(value: i128, scale: u32) -> String {
        let s = scale as usize;
        let digits = format!("{:0>width$}", value.unsigned_abs(), width = s + 1);
        let (whole, places) = digits.split_at(digits.len() - s);
        let sign = if value < 0 { "-" } else { "" };
        if s == 0 { format!("{sign}{whole}") } else { format!("{sign}{whole}.{places}") }
    }

    /// A decimal from its text, its scale the count of digits after the point.
    pub fn parse_decimal(text: &str) -> Option<Value> {
        let (negative, digits) = match text.strip_prefix('-') {
            Some(d) => (true, d),
            None => (false, text.strip_prefix('+').unwrap_or(text)),
        };
        let (whole, places) = digits.split_once('.').unwrap_or((digits, ""));
        if whole.is_empty() || !whole.bytes().chain(places.bytes()).all(|b| b.is_ascii_digit()) {
            return None;
        }
        let magnitude: i128 = format!("{whole}{places}").parse().ok()?;
        Some(Value::Decimal { value: if negative { -magnitude } else { magnitude }, scale: places.len() as u32 })
    }
}

/// An SQLCODE and its SQLSTATE.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SqlError {
    pub code: i32,
    pub state: &'static str,
}

pub const NOT_ASSIGNABLE: SqlError = SqlError { code: -303, state: "42806" };
pub const OUT_OF_RANGE: SqlError = SqlError { code: -304, state: "22003" };
pub const NULL_WITHOUT_INDICATOR: SqlError = SqlError { code: -305, state: "22002" };
pub const BAD_LENGTH: SqlError = SqlError { code: -311, state: "22501" };
pub const UNCONVERTIBLE: SqlError = SqlError { code: -330, state: "22021" };

/// What a host variable holds, as Db2 reads its COBOL declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostType {
    SmallInt { signed: bool },
    Integer { signed: bool },
    BigInt { signed: bool },
    Decimal { digits: u32, scale: u32, signed: bool },
    Zoned { digits: u32, scale: u32, signed: bool, sign: Option<SignClause> },
    Real,
    Double,
    Char(u32),
    VarChar(u32),
    /// GRAPHIC: a DBCS item of this many characters.
    Graphic(u32),
    /// VARGRAPHIC: a 49-level halfword of DBCS characters and a 49-level DBCS item of at most this
    /// many.
    VarGraphic(u32),
    /// A host structure: its members' layout items and types, in order.
    Structure(Vec<(usize, HostType)>),
}

/// A statement's identity in a recording: 32-bit FNV-1a over its canonical text.
pub fn fingerprint(text: &str) -> u32 {
    text.bytes().fold(0x811c_9dc5, |h, b| (h ^ u32::from(b)).wrapping_mul(0x0100_0193))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fingerprint_of_nothing_is_the_fnv_offset() {
        assert_eq!(fingerprint(""), 0x811c_9dc5);
        assert_ne!(fingerprint("COMMIT"), fingerprint("ROLLBACK"));
    }
}
