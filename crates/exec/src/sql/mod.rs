//! The SQL runtime: values at the database boundary, and the host variables they come from and go
//! to. It reaches storage only as a byte range and a host type, so any executor can call it.

mod convert;
mod database;
mod postgres;
mod replay;

pub use convert::{ReadError, Written, read, write};
pub use database::{Abandoned, Answer, Call, Database, OpenCursor, Outcome, Session};
pub use postgres::{Postgres, Stream, Tls};
pub use replay::{Recorder, Replay};

use crate::layout::{Kind, Layout};
use syntax::ast::SignClause;
use zarch::hfp::Precision;

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
    /// A host structure: its members' layout items and types, in order.
    Structure(Vec<(usize, HostType)>),
}

/// The host type of layout item `item`, or why it cannot be a host variable.
pub fn host_type(layout: &Layout, item: usize) -> Result<HostType, String> {
    let it = &layout.items[item];
    let name = it.name.as_deref().unwrap_or("FILLER");
    Ok(match it.kind {
        Kind::Binary { scale: 0, signed, .. } => match it.size {
            2 => HostType::SmallInt { signed },
            4 => HostType::Integer { signed },
            8 => HostType::BigInt { signed },
            n => return Err(format!("{name}: a binary item of {n} bytes has no SQL type")),
        },
        Kind::Binary { .. } => return Err(format!("{name}: a binary item with decimal places has no SQL type")),
        Kind::Packed { digits, scale, signed } if digits <= 31 => HostType::Decimal { digits, scale, signed },
        Kind::Zoned { digits, scale, signed, sign } if digits <= 31 => HostType::Zoned { digits, scale, signed, sign },
        Kind::Packed { .. } | Kind::Zoned { .. } => return Err(format!("{name}: more than 31 digits has no SQL type")),
        Kind::Float(Precision::Short) => HostType::Real,
        Kind::Float(Precision::Long) => HostType::Double,
        Kind::Alnum { .. } => HostType::Char(it.size),
        Kind::Group => structure(layout, item, name)?,
        _ => return Err(format!("{name}: this USAGE or PICTURE has no SQL type")),
    })
}

/// A group is VARCHAR when it is a 49-level length halfword and a 49-level text, and otherwise a
/// host structure of its members.
fn structure(layout: &Layout, item: usize, name: &str) -> Result<HostType, String> {
    let members: Vec<usize> = layout.items[item].children.iter().copied().filter(|&c| layout.items[c].redefines.is_none()).collect();
    if let [length, text] = members[..] {
        let (l, t) = (&layout.items[length], &layout.items[text]);
        if l.level == 49 && t.level == 49 && l.size == 2 && matches!(l.kind, Kind::Binary { scale: 0, .. }) && matches!(t.kind, Kind::Alnum { .. }) {
            return Ok(HostType::VarChar(t.size));
        }
    }
    let mut out = Vec::new();
    for m in members {
        if layout.items[m].table {
            return Err(format!("{name}: a host structure holding a table"));
        }
        match host_type(layout, m)? {
            HostType::Structure(_) => return Err(format!("{name}: a host structure nested in another")),
            t => out.push((m, t)),
        }
    }
    if out.is_empty() {
        return Err(format!("{name}: a group with no members"));
    }
    Ok(HostType::Structure(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Resolved;

    fn types(data: &str, names: &[&str]) -> Vec<Result<HostType, String>> {
        let source = format!(
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n{data}       PROCEDURE DIVISION.\n           GOBACK.\n"
        );
        let compiled = crate::compile(syntax::parse(&source).expect("parses"), &[]).expect("compiles");
        let layout = &compiled.layout;
        names
            .iter()
            .map(|n| match layout.resolve(n, &[], syntax::Pos::default()) {
                Ok(Resolved::Item(i)) => host_type(layout, i),
                other => panic!("{n}: {other:?}"),
            })
            .collect()
    }

    #[test]
    fn cobol_declarations_as_db2_reads_them() {
        let got = types(
            concat!(
                "       01 H PIC S9(4) COMP.\n",
                "       01 I PIC S9(9) COMP-5.\n",
                "       01 B PIC S9(18) BINARY.\n",
                "       01 D PIC S9(5)V99 COMP-3.\n",
                "       01 Z PIC S9(5)V99 SIGN LEADING SEPARATE.\n",
                "       01 C PIC X(10).\n",
                "       01 F COMP-2.\n",
                "       01 V.\n",
                "          49 V-LEN PIC S9(4) COMP.\n",
                "          49 V-TEXT PIC X(30).\n",
                "       01 S.\n",
                "          05 S-ID PIC S9(9) COMP.\n",
                "          05 S-NAME PIC X(20).\n",
            ),
            &["H", "I", "B", "D", "Z", "C", "F", "V", "S"],
        );
        assert_eq!(got[0], Ok(HostType::SmallInt { signed: true }));
        assert_eq!(got[1], Ok(HostType::Integer { signed: true }));
        assert_eq!(got[2], Ok(HostType::BigInt { signed: true }));
        assert_eq!(got[3], Ok(HostType::Decimal { digits: 7, scale: 2, signed: true }));
        assert!(matches!(got[4], Ok(HostType::Zoned { digits: 7, scale: 2, signed: true, sign: Some(_) })));
        assert_eq!(got[5], Ok(HostType::Char(10)));
        assert_eq!(got[6], Ok(HostType::Double));
        assert_eq!(got[7], Ok(HostType::VarChar(30)));
        let Ok(HostType::Structure(members)) = &got[8] else { panic!("{:?}", got[8]) };
        assert_eq!(members.iter().map(|(_, t)| t.clone()).collect::<Vec<_>>(), [HostType::Integer { signed: true }, HostType::Char(20)]);
    }

    #[test]
    fn declarations_with_no_sql_type() {
        let got = types("       01 E PIC ZZ9.99.\n       01 P USAGE POINTER.\n       01 BS PIC S9(3)V9 COMP.\n", &["E", "P", "BS"]);
        assert!(got.iter().all(Result::is_err), "{got:?}");
    }
}
