//! Db2 statement text as PostgreSQL reads it (assumption SQ5), values as the text PostgreSQL's wire
//! carries, and PostgreSQL's SQLSTATEs as Db2's SQLCODEs (assumption SQ4).

use crate::sql::Value;

pub const BOOL: u32 = 16;
pub const BYTEA: u32 = 17;
pub const INT8: u32 = 20;
pub const INT2: u32 = 21;
pub const INT4: u32 = 23;
pub const FLOAT4: u32 = 700;
pub const FLOAT8: u32 = 701;
pub const TIME: u32 = 1083;
pub const TIMESTAMP: u32 = 1114;
pub const TIMESTAMPTZ: u32 = 1184;
pub const NUMERIC: u32 = 1700;

#[derive(Clone, Debug)]
struct Token<'a> {
    text: &'a str,
    word: bool,
    spaced: bool,
}

/// Canonical text's tokens, each knowing whether a space came before it.
fn tokens(sql: &str) -> Vec<Token<'_>> {
    let bytes = sql.as_bytes();
    let (mut out, mut i) = (Vec::new(), 0);
    while i < bytes.len() {
        let spaced = i > 0 && bytes[i - 1] == b' ';
        let start = i;
        let c = bytes[i];
        if c == b' ' {
            i += 1;
            continue;
        }
        let word = c.is_ascii_alphabetic() || matches!(c, b'_' | b'#' | b'@' | b'$');
        if c == b'\'' || c == b'"' {
            i += 1;
            while i < bytes.len() {
                if bytes[i] == c && bytes.get(i + 1) == Some(&c) {
                    i += 2;
                } else if bytes[i] == c {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
        } else if word {
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'_' | b'#' | b'@' | b'$' | b'-')) {
                i += 1;
            }
        } else if c.is_ascii_digit() {
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
        } else {
            i += sql[i..].chars().next().map_or(1, char::len_utf8);
        }
        out.push(Token { text: &sql[start..i], word, spaced });
    }
    out
}

/// A statement's canonical text as PostgreSQL runs it: `?` markers numbered, the rewrites of
/// sql-runtime.md §9 applied, and a cursor name with hyphens quoted.
pub fn rewrite(sql: &str, cursor: Option<&str>) -> String {
    let t = tokens(sql);
    let word = |i: usize| t.get(i).filter(|t| t.word).map_or("", |t| t.text);
    let text = |i: usize| t.get(i).map_or("", |t| t.text);
    let mut out: Vec<(bool, String)> = Vec::new();
    let (mut i, mut marker) = (0, 0);
    while i < t.len() {
        let spaced = t[i].spaced;
        let (replacement, width): (Option<String>, usize) = match (word(i), word(i + 1)) {
            ("CURRENT", "DATE") => (Some("CURRENT_DATE".into()), 2),
            ("CURRENT", "TIME") | ("CURRENT_TIME", _) => (Some("LOCALTIME".into()), if word(i) == "CURRENT" { 2 } else { 1 }),
            ("CURRENT", "TIMESTAMP") | ("CURRENT_TIMESTAMP", _) => (Some("LOCALTIMESTAMP".into()), if word(i) == "CURRENT" { 2 } else { 1 }),
            ("CONCAT", _) => (Some("||".into()), 1),
            ("VALUE", _) if text(i + 1) == "(" => (Some("COALESCE".into()), 1),
            ("VALUES", _) if i == 0 && text(1) != "(" => (Some("SELECT".into()), 1),
            ("WITH", "UR" | "CS" | "RS" | "RR") => {
                let locks = matches!((word(i + 2), word(i + 3), word(i + 4)), ("USE", "AND", "KEEP")) && word(i + 6) == "LOCKS";
                (None, if locks { 7 } else { 2 })
            }
            ("OPTIMIZE", "FOR") if matches!(word(i + 3), "ROW" | "ROWS") => (None, 4),
            ("FOR", "FETCH" | "READ") if word(i + 2) == "ONLY" => (None, 3),
            ("FOR", "UPDATE") if word(i + 2) == "OF" => {
                let mut end = i + 3;
                while t.get(end).is_some_and(|t| t.word || t.text == ",") {
                    end += 1;
                }
                (Some("FOR UPDATE".into()), end - i)
            }
            (w, _) if !w.is_empty() && Some(w) == cursor && w.contains('-') => (Some(format!("\"{w}\"")), 1),
            _ if text(i) == "?" => {
                marker += 1;
                (Some(format!("${marker}")), 1)
            }
            _ => (Some(t[i].text.to_owned()), 1),
        };
        if let Some(r) = replacement {
            out.push((spaced, r));
        }
        i += width;
    }
    let mut sql = String::new();
    for (n, (spaced, piece)) in out.into_iter().enumerate() {
        if n > 0 && spaced {
            sql.push(' ');
        }
        sql.push_str(&piece);
    }
    sql
}

/// PostgreSQL's SQLSTATE as Db2's SQLCODE and SQLSTATE, or None where the table has no row.
pub fn db2_error(state: &str) -> Option<(i32, &'static str)> {
    Some(match state {
        "23505" => (-803, "23505"),
        "23502" => (-407, "23502"),
        "23503" => (-530, "23503"),
        "22001" => (-404, "22001"),
        "22012" => (-802, "22012"),
        "40001" | "40P01" => (-911, "40001"),
        "42601" => (-104, "42601"),
        "42P01" => (-204, "42704"),
        "42703" => (-206, "42703"),
        _ => return None,
    })
}

/// A column's text as a value. Dates and times take Db2's ISO forms, as a character host variable
/// receives them: `HH.MM.SS` and `YYYY-MM-DD-HH.MM.SS.NNNNNN`.
pub fn value(oid: u32, text: &str) -> Result<Value, String> {
    let bad = || format!("PostgreSQL sent \"{text}\" for a column of type {oid}");
    Ok(match oid {
        INT2 | INT4 | INT8 => Value::Int(text.parse().map_err(|_| bad())?),
        NUMERIC => Value::parse_decimal(text).ok_or_else(bad)?,
        FLOAT4 | FLOAT8 => Value::Double(text.parse().map_err(|_| bad())?),
        BOOL => Value::Int(i64::from(text == "t")),
        BYTEA => {
            let hex = text.strip_prefix("\\x").ok_or_else(bad)?;
            let bytes: Option<Vec<u8>> = (0..hex.len()).step_by(2).map(|i| hex.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok())).collect();
            Value::Binary(bytes.ok_or_else(bad)?)
        }
        TIME => Value::Char(text.get(..8).ok_or_else(bad)?.replace(':', ".")),
        TIMESTAMP | TIMESTAMPTZ => {
            let zone = text.rfind(['+', '-']).filter(|&at| oid == TIMESTAMPTZ && at > 10);
            let stamp = zone.map_or(text, |at| &text[..at]);
            let (date, time) = stamp.split_once(' ').ok_or_else(bad)?;
            let (seconds, fraction) = time.split_once('.').unwrap_or((time, ""));
            Value::Char(format!("{date}-{}.{fraction:0<6}", seconds.replace(':', ".")))
        }
        _ => Value::Char(text.to_owned()),
    })
}

/// A value as the text PostgreSQL reads for a parameter of type `oid`, or None for NULL. A
/// character value in Db2's ISO time or timestamp form is given PostgreSQL's.
pub fn text(value: &Value, oid: u32) -> Option<String> {
    Some(match value {
        Value::Null => return None,
        Value::Int(i) => i.to_string(),
        Value::Decimal { value, scale } => Value::decimal_text(*value, *scale),
        Value::Double(f) if f.is_nan() => "NaN".into(),
        Value::Double(f) if f.is_infinite() => if *f > 0.0 { "Infinity" } else { "-Infinity" }.into(),
        Value::Double(f) => format!("{f:?}"),
        Value::Binary(b) => format!("\\x{}", b.iter().map(|x| format!("{x:02x}")).collect::<String>()),
        Value::Char(s) => match oid {
            TIME => iso_time(s.trim_end()).unwrap_or_else(|| s.clone()),
            TIMESTAMP | TIMESTAMPTZ => iso_timestamp(s.trim_end()).unwrap_or_else(|| s.clone()),
            _ => s.clone(),
        },
    })
}

fn digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// `HH.MM.SS` as `HH:MM:SS`.
fn iso_time(s: &str) -> Option<String> {
    let parts: Vec<&str> = s.split('.').collect();
    (parts.len() == 3 && parts.iter().all(|p| p.len() == 2 && digits(p))).then(|| parts.join(":"))
}

/// `YYYY-MM-DD-HH.MM.SS[.NNNNNN]` as `YYYY-MM-DD HH:MM:SS[.NNNNNN]`.
fn iso_timestamp(s: &str) -> Option<String> {
    let (date, time) = (s.get(..10)?, s.get(11..)?);
    if s.as_bytes()[10] != b'-' || !date.split('-').all(digits) {
        return None;
    }
    let (clock, fraction) = time.get(..8).zip(time.get(8..))?;
    let clock = iso_time(clock)?;
    match fraction.strip_prefix('.') {
        None if fraction.is_empty() => Some(format!("{date} {clock}")),
        Some(f) if digits(f) => Some(format!("{date} {clock}.{f}")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DATE: u32 = 1082;

    #[test]
    fn statements_as_postgresql_reads_them() {
        let cases = [
            ("SELECT NAME FROM EMP WHERE ID = ? AND DEPT = ?", "SELECT NAME FROM EMP WHERE ID = $1 AND DEPT = $2"),
            ("SELECT A FROM T WHERE B <= ? AND C <> '?' WITH UR", "SELECT A FROM T WHERE B <= $1 AND C <> '?'"),
            ("VALUES CURRENT TIMESTAMP", "SELECT LOCALTIMESTAMP"),
            ("VALUES (? + 1)", "VALUES ($1 + 1)"),
            ("SELECT VALUE(A, 0), B CONCAT C FROM T OPTIMIZE FOR 1 ROW", "SELECT COALESCE(A, 0), B || C FROM T"),
            ("SELECT A FROM T WHERE D = CURRENT DATE FOR FETCH ONLY", "SELECT A FROM T WHERE D = CURRENT_DATE"),
            ("DECLARE C1 CURSOR WITH HOLD FOR SELECT A FROM T FOR UPDATE OF A, B", "DECLARE C1 CURSOR WITH HOLD FOR SELECT A FROM T FOR UPDATE"),
            ("SELECT A FROM T WITH RS USE AND KEEP UPDATE LOCKS", "SELECT A FROM T"),
            ("SELECT S.A FROM SYSIBM.SYSDUMMY1 S", "SELECT S.A FROM SYSIBM.SYSDUMMY1 S"),
            ("INSERT INTO T (A) VALUES (1.50)", "INSERT INTO T (A) VALUES (1.50)"),
        ];
        for (db2, postgres) in cases {
            assert_eq!(rewrite(db2, None), postgres, "{db2}");
        }
        assert_eq!(rewrite("FETCH PROGRAMS-CSR", Some("PROGRAMS-CSR")), "FETCH \"PROGRAMS-CSR\"");
        assert_eq!(rewrite("DELETE FROM T WHERE CURRENT OF C1", Some("C1")), "DELETE FROM T WHERE CURRENT OF C1");
    }

    #[test]
    fn columns_as_db2_hands_them_over() {
        assert_eq!(value(NUMERIC, "-12.340"), Ok(Value::Decimal { value: -12340, scale: 3 }));
        assert_eq!(value(INT8, "9000000000"), Ok(Value::Int(9_000_000_000)));
        assert_eq!(value(TIME, "13:45:06"), Ok(Value::Char("13.45.06".into())));
        assert_eq!(value(TIMESTAMP, "2026-09-30 13:45:06.5"), Ok(Value::Char("2026-09-30-13.45.06.500000".into())));
        assert_eq!(value(TIMESTAMP, "2026-09-30 13:45:06"), Ok(Value::Char("2026-09-30-13.45.06.000000".into())));
        assert_eq!(value(TIMESTAMPTZ, "2026-09-30 13:45:06.123456+00"), Ok(Value::Char("2026-09-30-13.45.06.123456".into())));
        assert_eq!(value(DATE, "2026-09-30"), Ok(Value::Char("2026-09-30".into())));
        assert_eq!(value(BYTEA, "\\xc1f0"), Ok(Value::Binary(vec![0xC1, 0xF0])));
        assert!(value(NUMERIC, "NaN").is_err());
    }

    #[test]
    fn parameters_as_postgresql_reads_them() {
        assert_eq!(text(&Value::Null, 0), None);
        assert_eq!(text(&Value::Decimal { value: -5, scale: 2 }, NUMERIC).as_deref(), Some("-0.05"));
        assert_eq!(text(&Value::Char("2026-09-30-13.45.06.000001".into()), TIMESTAMP).as_deref(), Some("2026-09-30 13:45:06.000001"));
        assert_eq!(text(&Value::Char("13.45.06  ".into()), TIME).as_deref(), Some("13:45:06"));
        assert_eq!(text(&Value::Char("13.45.06".into()), 25).as_deref(), Some("13.45.06"));
        assert_eq!(text(&Value::Binary(vec![0, 255]), BYTEA).as_deref(), Some("\\x00ff"));
    }
}
