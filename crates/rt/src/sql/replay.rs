//! Recordings: each call a run made and the answer it got, as text a person can read and edit.
//! [`Replay`] answers from a recording; [`Recorder`] writes one while another backend answers.
//!
//! ```text
//! # ironwork sql recording 1
//! @ 1 PAYROLL:3:9f2a41c0 SELECT
//! > char:"00123"
//! < 0 00000 rows=1
//! = dec:1234.50 | char:"SMITH" | null
//! @ 2 PAYROLL:4:1b77e0d2 PREPARE S1
//! < 0 00000 rows=0
//! : char:"NAME" char(10) notnull
//! ```
//!
//! A multiple-row INSERT has a `>` line for each row. A CALL's `=` line gives each argument as the
//! procedure returns it, `-` for one it does not return.

use super::{Abandoned, Answer, Call, Column, ColumnType, Database, Outcome, Value};
use std::io::Write;
use super::fingerprint;

const HEADER: &str = "# ironwork sql recording 1";
const VERSIONED: &str = "# ironwork sql recording ";

#[derive(Debug)]
struct Entry {
    program: String,
    ordinal: u32,
    hash: u32,
    verb: String,
    cursor: Option<String>,
    inputs: Vec<Value>,
    outcome: Outcome,
}

impl Entry {
    fn answers(&self, call: &Call) -> bool {
        self.program == call.program
            && self.ordinal == call.ordinal
            && self.hash == fingerprint(call.text)
            && self.verb == call.verb
            && self.cursor.as_deref() == call.cursor
            && self.inputs == call.inputs
    }
}

fn describe(program: &str, ordinal: u32, hash: u32, verb: &str, cursor: Option<&str>, inputs: &[Value]) -> String {
    let cursor = cursor.map(|c| format!(" {c}")).unwrap_or_default();
    let inputs = if inputs.is_empty() { String::new() } else { format!(" with {}", values_text(inputs)) };
    format!("{program}:{ordinal}:{hash:08x} {verb}{cursor}{inputs}")
}

/// Answers calls from a recording. Strict replay answers call n from entry n; keyed replay answers
/// each call from the first unused entry with the same statement and inputs. A call the recording
/// does not hold ends the run.
pub struct Replay {
    entries: Vec<Entry>,
    used: Vec<bool>,
    next: usize,
    keyed: bool,
}

impl Replay {
    pub fn parse(text: &str, keyed: bool) -> Result<Self, String> {
        let mut entries: Vec<Entry> = Vec::new();
        let mut header = false;
        let mut awaiting_outcome = false;
        for (n, line) in text.lines().enumerate().map(|(i, l)| (i + 1, l.trim_end())) {
            let fail = |why: String| format!("line {n}: {why}");
            if line.is_empty() {
                continue;
            }
            if line.starts_with('#') {
                if let Some(version) = line.strip_prefix(VERSIONED)
                    && line != HEADER
                {
                    return Err(fail(format!("this ironwork reads sql recording 1, and this recording is {version}")));
                }
                header |= line == HEADER;
                continue;
            }
            if !header {
                return Err(fail(format!("a recording starts with \"{HEADER}\"")));
            }
            let (mark, rest) = line.split_at(1);
            let rest = rest.trim_start();
            match mark {
                "@" => {
                    if awaiting_outcome {
                        return Err(fail("the call before this one has no < line".into()));
                    }
                    let words: Vec<&str> = rest.split_whitespace().collect();
                    let [_, id, verb, cursor @ ..] = words.as_slice() else { return Err(fail("@ takes a number, PROGRAM:ORDINAL:HASH and a verb".into())) };
                    let parts: Vec<&str> = id.split(':').collect();
                    let [program, ordinal, hash] = parts.as_slice() else { return Err(fail(format!("{id} is not PROGRAM:ORDINAL:HASH"))) };
                    let ordinal = ordinal.parse().map_err(|_| fail(format!("{ordinal} is not an ordinal")))?;
                    let hash = u32::from_str_radix(hash, 16).map_err(|_| fail(format!("{hash} is not a hexadecimal hash")))?;
                    let cursor = match cursor {
                        [] => None,
                        [c] => Some((*c).to_owned()),
                        _ => return Err(fail("@ takes at most one cursor after the verb".into())),
                    };
                    entries.push(Entry { program: (*program).into(), ordinal, hash, verb: (*verb).into(), cursor, inputs: Vec::new(), outcome: Outcome::ok() });
                    awaiting_outcome = true;
                }
                ">" => match entries.last_mut() {
                    Some(e) if awaiting_outcome => e.inputs.extend(parse_values(rest).map_err(fail)?),
                    _ => return Err(fail("a > line belongs after an @ line, before its < line".into())),
                },
                "<" => match entries.last_mut() {
                    Some(e) if awaiting_outcome => {
                        e.outcome = parse_outcome(rest).map_err(fail)?;
                        awaiting_outcome = false;
                    }
                    _ => return Err(fail("a < line belongs after an @ line".into())),
                },
                "=" => match entries.last_mut() {
                    Some(e) if !awaiting_outcome && e.verb == "CALL" && e.outcome.parameters.is_empty() => e.outcome.parameters = parse_parameters(rest).map_err(fail)?,
                    Some(e) if !awaiting_outcome && e.verb == "CALL" => return Err(fail("a CALL has one = line".into())),
                    Some(e) if !awaiting_outcome => e.outcome.rows.push(parse_values(rest).map_err(fail)?),
                    _ => return Err(fail("an = line belongs after a < line".into())),
                },
                ":" => match entries.last_mut() {
                    Some(e) if !awaiting_outcome => e.outcome.columns.push(parse_column(rest).map_err(fail)?),
                    _ => return Err(fail("a : line belongs after a < line".into())),
                },
                _ => return Err(fail(format!("{mark} starts no kind of line"))),
            }
        }
        if awaiting_outcome {
            return Err("the last call has no < line".into());
        }
        if !header {
            return Err(format!("a recording starts with \"{HEADER}\""));
        }
        let used = vec![false; entries.len()];
        Ok(Self { entries, used, next: 0, keyed })
    }

    fn answer(&mut self, call: &Call) -> Answer {
        let found = if self.keyed {
            (0..self.entries.len()).find(|&i| !self.used[i] && self.entries[i].answers(call))
        } else {
            (self.next < self.entries.len() && self.entries[self.next].answers(call)).then_some(self.next)
        };
        let actual = describe(call.program, call.ordinal, fingerprint(call.text), call.verb, call.cursor, call.inputs);
        let Some(i) = found else {
            let expected = match self.entries.get(self.next).filter(|_| !self.keyed) {
                Some(e) => format!("the recording's call {} is {}", self.next + 1, describe(&e.program, e.ordinal, e.hash, &e.verb, e.cursor.as_deref(), &e.inputs)),
                None => "the recording holds no such call".into(),
            };
            return Err(Abandoned { code: "SQLR", message: format!("{expected}, and the run made {actual}") });
        };
        self.used[i] = true;
        self.next = i + 1;
        Ok(self.entries[i].outcome.clone())
    }
}

impl Database for Replay {
    fn execute(&mut self, call: &Call) -> Answer {
        self.answer(call)
    }
    fn prepare(&mut self, call: &Call) -> Answer {
        self.answer(call)
    }
    fn open(&mut self, call: &Call) -> Answer {
        self.answer(call)
    }
    fn fetch(&mut self, call: &Call) -> Answer {
        self.answer(call)
    }
    fn fetch_rows(&mut self, call: &Call, _: u32) -> Answer {
        self.answer(call)
    }
    fn insert_rows(&mut self, call: &Call, _: &[Vec<Value>], _: bool) -> Answer {
        self.answer(call)
    }
    fn call(&mut self, call: &Call) -> Answer {
        self.answer(call)
    }
    fn close(&mut self, call: &Call) -> Answer {
        self.answer(call)
    }
    fn commit(&mut self, call: &Call) -> Answer {
        self.answer(call)
    }
    fn rollback(&mut self, call: &Call) -> Answer {
        self.answer(call)
    }
}

/// Writes every call and its answer while `inner` answers.
pub struct Recorder<'w> {
    inner: Box<dyn Database + 'w>,
    out: Box<dyn Write + 'w>,
    seq: u64,
}

impl<'w> Recorder<'w> {
    /// `source` names what answered, and goes in the recording's header.
    pub fn new(inner: Box<dyn Database + 'w>, mut out: Box<dyn Write + 'w>, source: &str) -> std::io::Result<Self> {
        writeln!(out, "{HEADER}\n# source: {source}")?;
        Ok(Self { inner, out, seq: 0 })
    }

    fn record(&mut self, call: &Call, answer: Answer) -> Answer {
        self.record_rows(call, None, answer)
    }

    /// `rows` are a multiple-row INSERT's, which take a > line each.
    fn record_rows(&mut self, call: &Call, rows: Option<&[Vec<Value>]>, answer: Answer) -> Answer {
        let outcome = answer?;
        self.seq += 1;
        let text = entry_text(self.seq, call, rows, &outcome);
        // Flushed per call: a served session ends when the server is interrupted, not by returning.
        self.out.write_all(text.as_bytes()).and_then(|()| self.out.flush()).map_err(|e| Abandoned { code: "SQLR", message: format!("the recording could not be written: {e}") })?;
        Ok(outcome)
    }
}

impl Database for Recorder<'_> {
    fn execute(&mut self, call: &Call) -> Answer {
        let a = self.inner.execute(call);
        self.record(call, a)
    }
    fn prepare(&mut self, call: &Call) -> Answer {
        let a = self.inner.prepare(call);
        self.record(call, a)
    }
    fn open(&mut self, call: &Call) -> Answer {
        let a = self.inner.open(call);
        self.record(call, a)
    }
    fn fetch(&mut self, call: &Call) -> Answer {
        let a = self.inner.fetch(call);
        self.record(call, a)
    }
    fn fetch_rows(&mut self, call: &Call, rows: u32) -> Answer {
        let a = self.inner.fetch_rows(call, rows);
        self.record(call, a)
    }
    fn insert_rows(&mut self, call: &Call, rows: &[Vec<Value>], atomic: bool) -> Answer {
        let a = self.inner.insert_rows(call, rows, atomic);
        self.record_rows(call, Some(rows), a)
    }
    fn call(&mut self, call: &Call) -> Answer {
        let a = self.inner.call(call);
        self.record(call, a)
    }
    fn close(&mut self, call: &Call) -> Answer {
        let a = self.inner.close(call);
        self.record(call, a)
    }
    fn commit(&mut self, call: &Call) -> Answer {
        let a = self.inner.commit(call);
        self.record(call, a)
    }
    fn rollback(&mut self, call: &Call) -> Answer {
        let a = self.inner.rollback(call);
        self.record(call, a)
    }
    fn close_all(&mut self) -> Result<(), Abandoned> {
        self.inner.close_all()
    }
}

fn entry_text(seq: u64, call: &Call, rows: Option<&[Vec<Value>]>, outcome: &Outcome) -> String {
    let cursor = call.cursor.map(|c| format!(" {c}")).unwrap_or_default();
    let mut text = format!("@ {seq} {}:{}:{:08x} {}{cursor}\n", call.program, call.ordinal, fingerprint(call.text), call.verb);
    match rows {
        Some(rows) => rows.iter().for_each(|row| text += &format!("> {}\n", values_text(row))),
        None if !call.inputs.is_empty() => text += &format!("> {}\n", values_text(call.inputs)),
        None => {}
    }
    text += &format!("< {} {} rows={}", outcome.sqlcode, outcome.sqlstate, outcome.affected);
    if !outcome.tokens.is_empty() {
        text += &format!(" tokens={}", value_text(&Value::Char(outcome.tokens.clone())));
    }
    text.push('\n');
    for column in &outcome.columns {
        text += &format!(": {}\n", column_text(column));
    }
    for row in &outcome.rows {
        text += &format!("= {}\n", values_text(row));
    }
    if !outcome.parameters.is_empty() {
        text += &format!("= {}\n", outcome.parameters.iter().map(|p| p.as_ref().map_or_else(|| "-".to_owned(), value_text)).collect::<Vec<_>>().join(" | "));
    }
    text
}

/// A result column as a `:` line gives it: its name, its type and whether it takes NULL.
fn column_text(c: &Column) -> String {
    let ty = match &c.ty {
        ColumnType::Char(n) => format!("char({n})"),
        ColumnType::VarChar(n) => format!("varchar({n})"),
        ColumnType::Graphic(n) => format!("graphic({n})"),
        ColumnType::VarGraphic(n) => format!("vargraphic({n})"),
        ColumnType::SmallInt => "smallint".into(),
        ColumnType::Integer => "integer".into(),
        ColumnType::BigInt => "bigint".into(),
        ColumnType::Decimal { precision, scale } => format!("decimal({precision},{scale})"),
        ColumnType::Real => "real".into(),
        ColumnType::Double => "double".into(),
        ColumnType::Date => "date".into(),
        ColumnType::Time => "time".into(),
        ColumnType::Timestamp(p) => format!("timestamp({p})"),
        ColumnType::Binary(n) => format!("binary({n})"),
        ColumnType::VarBinary(n) => format!("varbinary({n})"),
        ColumnType::Numeric => "numeric".into(),
        ColumnType::Other(name) => format!("other:{}", value_text(&Value::Char(name.clone()))),
    };
    format!("{} {ty} {}", value_text(&Value::Char(c.name.clone())), if c.nullable { "null" } else { "notnull" })
}

fn parse_column(text: &str) -> Result<Column, String> {
    let shape = "a : line is a char:\"name\", a type and null or notnull";
    let Ok((Value::Char(name), rest)) = parse_value(text.trim_start()) else { return Err(shape.into()) };
    let rest = rest.trim();
    let (ty, nullable) = match rest.rsplit_once(' ') {
        Some((ty, "null")) => (ty.trim(), true),
        Some((ty, "notnull")) => (ty.trim(), false),
        _ => return Err(shape.into()),
    };
    let sized = |inner: &str| inner.parse::<u16>().map_err(|_| format!("{ty} has no length"));
    let ty = match ty.split_once('(').map(|(w, r)| (w, r.strip_suffix(')'))) {
        Some(("char", Some(n))) => ColumnType::Char(sized(n)?),
        Some(("varchar", Some(n))) => ColumnType::VarChar(sized(n)?),
        Some(("graphic", Some(n))) => ColumnType::Graphic(sized(n)?),
        Some(("vargraphic", Some(n))) => ColumnType::VarGraphic(sized(n)?),
        Some(("binary", Some(n))) => ColumnType::Binary(sized(n)?),
        Some(("varbinary", Some(n))) => ColumnType::VarBinary(sized(n)?),
        Some(("timestamp", Some(p))) => ColumnType::Timestamp(p.parse().map_err(|_| format!("{ty} has no precision"))?),
        Some(("decimal", Some(ps))) => match ps.split_once(',').map(|(p, s)| (p.parse(), s.parse())) {
            Some((Ok(precision), Ok(scale))) => ColumnType::Decimal { precision, scale },
            _ => return Err(format!("{ty} is not decimal(p,s)")),
        },
        _ => match ty {
            "smallint" => ColumnType::SmallInt,
            "integer" => ColumnType::Integer,
            "bigint" => ColumnType::BigInt,
            "real" => ColumnType::Real,
            "double" => ColumnType::Double,
            "date" => ColumnType::Date,
            "time" => ColumnType::Time,
            "numeric" => ColumnType::Numeric,
            other => match other.strip_prefix("other:").map(parse_value) {
                Some(Ok((Value::Char(name), ""))) => ColumnType::Other(name),
                _ => return Err(format!("{other} is not a column type")),
            },
        },
    };
    Ok(Column { name, ty, nullable })
}

fn values_text(values: &[Value]) -> String {
    values.iter().map(value_text).collect::<Vec<_>>().join(" | ")
}

fn value_text(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Int(i) => format!("int:{i}"),
        Value::Decimal { value, scale } => format!("dec:{}", Value::decimal_text(*value, *scale)),
        Value::Double(f) => format!("double:{f:?}"),
        Value::Char(s) => {
            let mut q = String::from("char:\"");
            for c in s.chars() {
                match c {
                    '"' => q += "\\\"",
                    '\\' => q += "\\\\",
                    c if c.is_control() => q += &format!("\\x{:02X}", c as u32),
                    c => q.push(c),
                }
            }
            q + "\""
        }
        Value::Binary(b) => format!("hex:{}", b.iter().map(|x| format!("{x:02X}")).collect::<String>()),
        Value::Date(s) => format!("date:{s}"),
        Value::Time(s) => format!("time:{s}"),
        Value::Timestamp(s) => format!("ts:{s}"),
    }
}

/// Db2's ISO forms: `YYYY-MM-DD`, `HH.MM.SS`, and `YYYY-MM-DD-HH.MM.SS` with any fraction.
fn iso_date(s: &str) -> bool {
    s.len() == 10 && s.bytes().enumerate().all(|(i, b)| if i == 4 || i == 7 { b == b'-' } else { b.is_ascii_digit() })
}

fn iso_time(s: &str) -> bool {
    s.len() == 8 && s.bytes().enumerate().all(|(i, b)| if i == 2 || i == 5 { b == b'.' } else { b.is_ascii_digit() })
}

fn iso_timestamp(s: &str) -> bool {
    let fraction = s.get(19..).unwrap_or("x");
    s.is_ascii() && s.len() >= 19 && iso_date(&s[..10]) && &s[10..11] == "-" && iso_time(&s[11..19]) && (fraction.is_empty() || fraction.strip_prefix('.').is_some_and(|f| !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit())))
}

fn parse_outcome(text: &str) -> Result<Outcome, String> {
    let mut words = text.splitn(4, ' ');
    let (Some(code), Some(state), Some(rows)) = (words.next(), words.next(), words.next()) else { return Err("< takes SQLCODE, SQLSTATE and rows=N".into()) };
    let sqlcode = code.parse().map_err(|_| format!("{code} is not an SQLCODE"))?;
    if state.len() != 5 {
        return Err(format!("{state} is not a five-character SQLSTATE"));
    }
    let affected = rows.strip_prefix("rows=").and_then(|n| n.parse().ok()).ok_or_else(|| format!("{rows} is not rows=N"))?;
    let tokens = match words.next().map(str::trim) {
        None | Some("") => String::new(),
        Some(t) => match t.strip_prefix("tokens=").map(parse_value) {
            Some(Ok((Value::Char(s), rest))) if rest.trim().is_empty() => s,
            _ => return Err("the rest of a < line is tokens=char:\"...\"".into()),
        },
    };
    Ok(Outcome { sqlcode, sqlstate: state.into(), affected, rows: Vec::new(), tokens, columns: Vec::new(), parameters: Vec::new() })
}

/// A CALL's = line: a value for each argument, or - for one the procedure does not return.
fn parse_parameters(text: &str) -> Result<Vec<Option<Value>>, String> {
    parse_list(text, |item| match item.strip_prefix('-') {
        Some(rest) if rest.trim_start().is_empty() || rest.trim_start().starts_with('|') => Ok((None, rest)),
        _ => parse_value(item).map(|(v, rest)| (Some(v), rest)),
    })
}

fn parse_values(text: &str) -> Result<Vec<Value>, String> {
    parse_list(text, parse_value)
}

/// Items separated by |, each read from the start of what is left by `item`.
fn parse_list<T>(text: &str, item: impl Fn(&str) -> Result<(T, &str), String>) -> Result<Vec<T>, String> {
    let (mut out, mut rest) = (Vec::new(), text.trim_start());
    while !rest.is_empty() {
        let (v, after) = item(rest)?;
        out.push(v);
        rest = after.trim_start();
        if let Some(next) = rest.strip_prefix('|') {
            rest = next.trim_start();
            if rest.is_empty() {
                return Err("a value is missing after |".into());
            }
        } else if !rest.is_empty() {
            return Err(format!("values are separated by |, not \"{rest}\""));
        }
    }
    Ok(out)
}

/// One value literal from the start of `text`, and what follows it.
fn parse_value(text: &str) -> Result<(Value, &str), String> {
    if let Some(quoted) = text.strip_prefix("char:\"") {
        let mut s = String::new();
        let mut chars = quoted.char_indices();
        while let Some((i, c)) = chars.next() {
            match c {
                '"' => return Ok((Value::Char(s), &quoted[i + 1..])),
                '\\' => match chars.next() {
                    Some((_, '"')) => s.push('"'),
                    Some((_, '\\')) => s.push('\\'),
                    Some((j, 'x')) => {
                        let hex = quoted.get(j + 1..j + 3).ok_or("\\x takes two hexadecimal digits")?;
                        let code = u32::from_str_radix(hex, 16).map_err(|_| format!("\\x{hex} is not hexadecimal"))?;
                        s.push(char::from_u32(code).ok_or("\\x names no character")?);
                        chars.next();
                        chars.next();
                    }
                    _ => return Err("a backslash in char:\"...\" escapes \", \\ or xNN".into()),
                },
                c => s.push(c),
            }
        }
        return Err("char:\" is not closed".into());
    }
    let end = text.find(|c: char| c.is_whitespace() || c == '|').unwrap_or(text.len());
    let (word, rest) = text.split_at(end);
    let value = match word.split_once(':') {
        None if word == "null" => Value::Null,
        Some(("int", n)) => Value::Int(n.parse().map_err(|_| format!("{word} is not an integer"))?),
        Some(("dec", n)) => Value::parse_decimal(n).ok_or_else(|| format!("{word} is not a decimal"))?,
        Some(("double", n)) => Value::Double(n.parse().map_err(|_| format!("{word} is not a double"))?),
        Some(("hex", h)) if h.len() % 2 == 0 => {
            let bytes: Result<Vec<u8>, _> = (0..h.len()).step_by(2).map(|i| u8::from_str_radix(&h[i..i + 2], 16)).collect();
            Value::Binary(bytes.map_err(|_| format!("{word} is not hexadecimal"))?)
        }
        Some(("date", d)) if iso_date(d) => Value::Date(d.into()),
        Some(("time", t)) if iso_time(t) => Value::Time(t.into()),
        Some(("ts", t)) if iso_timestamp(t) => Value::Timestamp(t.into()),
        _ => return Err(format!("{word} is not a value: null, int:, dec:, double:, char:\"...\", hex:, date:, time: or ts:")),
    };
    Ok((value, rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call<'a>(verb: &'a str, text: &'a str, inputs: &'a [Value]) -> Call<'a> {
        Call { program: "P", ordinal: 2, verb, cursor: None, text, inputs }
    }

    #[test]
    fn values_round_trip() {
        let values = vec![
            Value::Null,
            Value::Int(-42),
            Value::Decimal { value: -123_450, scale: 2 },
            Value::Decimal { value: 5, scale: 2 },
            Value::Decimal { value: 7, scale: 0 },
            Value::Double(0.1),
            Value::Double(6.02e23),
            Value::Char("say \"hi\" | x \\ é\n".into()),
            Value::Binary(vec![0xC1, 0x00]),
            Value::Date("2026-09-30".into()),
            Value::Time("13.45.06".into()),
            Value::Timestamp("2026-09-30-13.45.06.500000".into()),
            Value::Timestamp("2026-09-30-13.45.06".into()),
        ];
        let text = values_text(&values);
        assert!(text.contains("dec:-1234.50") && text.contains("dec:0.05") && text.contains("dec:7"), "{text}");
        assert!(text.contains("date:2026-09-30 | time:13.45.06 | ts:2026-09-30-13.45.06.500000"), "{text}");
        assert_eq!(parse_values(&text), Ok(values));
        for bad in ["date:2026-9-30", "time:13:45:06", "ts:2026-09-30 13.45.06", "ts:2026-09-30-13.45.06.", "date:２０２６-09-30"] {
            assert!(parse_values(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_recording_answers_the_calls_it_holds_in_order() {
        let inputs = [Value::Int(7)];
        let first = entry_text(1, &call("SELECT", "SELECT A FROM T WHERE K = ?", &inputs), None, &Outcome::rows(vec![vec![Value::Char("X".into())]]));
        let second = entry_text(2, &call("COMMIT", "COMMIT", &[]), None, &Outcome::ok());
        let mut replay = Replay::parse(&format!("{HEADER}\n{first}{second}"), false).expect("parses");
        assert_eq!(replay.execute(&call("SELECT", "SELECT A FROM T WHERE K = ?", &inputs)).unwrap().rows, [[Value::Char("X".into())]]);
        assert_eq!(replay.commit(&call("COMMIT", "COMMIT", &[])), Ok(Outcome::ok()));
        let beyond = replay.commit(&call("COMMIT", "COMMIT", &[])).unwrap_err();
        assert_eq!((beyond.code, beyond.message.starts_with("the recording holds no such call")), ("SQLR", true));
    }

    #[test]
    fn strict_replay_refuses_a_different_call_and_names_both() {
        let text = format!("{HEADER}\n{}", entry_text(1, &call("SELECT", "SELECT A FROM T WHERE K = ?", &[Value::Int(7)]), None, &Outcome::ok()));
        let mut replay = Replay::parse(&text, false).unwrap();
        let err = replay.execute(&call("SELECT", "SELECT A FROM T WHERE K = ?", &[Value::Int(8)])).unwrap_err();
        assert_eq!(err.code, "SQLR");
        assert!(err.message.contains("with int:7") && err.message.contains("with int:8"), "{}", err.message);
    }

    #[test]
    fn keyed_replay_takes_calls_in_any_order() {
        let (a, b) = ([Value::Int(1)], [Value::Int(2)]);
        let text = format!(
            "{HEADER}\n{}{}",
            entry_text(1, &call("SELECT", "Q", &a), None, &Outcome::rows(vec![vec![Value::Int(10)]])),
            entry_text(2, &call("SELECT", "Q", &b), None, &Outcome::rows(vec![vec![Value::Int(20)]]))
        );
        let mut replay = Replay::parse(&text, true).unwrap();
        assert_eq!(replay.execute(&call("SELECT", "Q", &b)).unwrap().rows, [[Value::Int(20)]]);
        assert_eq!(replay.execute(&call("SELECT", "Q", &a)).unwrap().rows, [[Value::Int(10)]]);
        assert!(replay.execute(&call("SELECT", "Q", &a)).is_err());
    }

    #[test]
    fn a_recorder_writes_what_replay_reads() {
        struct Fixed;
        impl Database for Fixed {
            fn execute(&mut self, _: &Call) -> Answer {
                Ok(Outcome { tokens: "T1".into(), ..Outcome::rows(vec![vec![Value::Decimal { value: 150, scale: 2 }, Value::Null]]) })
            }
            fn prepare(&mut self, _: &Call) -> Answer {
                let columns = vec![
                    Column { name: "NAME".into(), ty: ColumnType::Char(10), nullable: false },
                    Column { name: "AMT".into(), ty: ColumnType::Decimal { precision: 7, scale: 2 }, nullable: true },
                    Column { name: "ODD \"ONE\"".into(), ty: ColumnType::Other("PostgreSQL type OID 16".into()), nullable: true },
                    Column { name: "TS".into(), ty: ColumnType::Timestamp(6), nullable: true },
                ];
                Ok(Outcome { columns, ..Outcome::ok() })
            }
            fn open(&mut self, _: &Call) -> Answer {
                Ok(Outcome::ok())
            }
            fn fetch(&mut self, _: &Call) -> Answer {
                Ok(Outcome::error(100, "02000"))
            }
            fn fetch_rows(&mut self, _: &Call, _: u32) -> Answer {
                Ok(Outcome { affected: 2, ..Outcome::rows(vec![vec![Value::Int(1)], vec![Value::Int(2)]]) })
            }
            fn insert_rows(&mut self, _: &Call, rows: &[Vec<Value>], _: bool) -> Answer {
                Ok(Outcome { affected: rows.len() as i64, ..Outcome::ok() })
            }
            fn call(&mut self, _: &Call) -> Answer {
                Ok(Outcome { parameters: vec![None, Some(Value::Char("A|B".into())), Some(Value::Null)], ..Outcome::error(466, "0100C") })
            }
            fn close(&mut self, _: &Call) -> Answer {
                Ok(Outcome::ok())
            }
            fn commit(&mut self, _: &Call) -> Answer {
                Ok(Outcome::ok())
            }
            fn rollback(&mut self, _: &Call) -> Answer {
                Ok(Outcome::ok())
            }
        }
        let written = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        struct Sink(std::rc::Rc<std::cell::RefCell<Vec<u8>>>);
        impl Write for Sink {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.borrow_mut().extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let inputs = [Value::Char("A|B".into())];
        let mut recorder = Recorder::new(Box::new(Fixed), Box::new(Sink(written.clone())), "a test double").unwrap();
        let live = recorder.execute(&call("SELECT", "SELECT X, Y FROM T WHERE Z = ?", &inputs)).unwrap();
        let text = String::from_utf8(written.borrow().clone()).unwrap();
        assert!(text.starts_with(&format!("{HEADER}\n# source: a test double\n")), "{text}");
        let prepared = recorder.prepare(&Call { cursor: Some("S1"), ..call("PREPARE", "SELECT NAME, AMT FROM T", &[]) }).unwrap();
        let text = String::from_utf8(written.borrow().clone()).unwrap();
        assert!(text.contains("PREPARE S1\n< 0 00000 rows=0\n: char:\"NAME\" char(10) notnull\n: char:\"AMT\" decimal(7,2) null\n"), "{text}");
        let mut replay = Replay::parse(&text, false).unwrap();
        assert_eq!(replay.execute(&call("SELECT", "SELECT X, Y FROM T WHERE Z = ?", &inputs)), Ok(live));
        assert_eq!(replay.prepare(&Call { cursor: Some("S1"), ..call("PREPARE", "SELECT NAME, AMT FROM T", &[]) }), Ok(prepared));

        written.borrow_mut().clear();
        let mut recorder = Recorder::new(Box::new(Fixed), Box::new(Sink(written.clone())), "a test double").unwrap();
        let rows = [vec![Value::Int(1), Value::Char("X".into())], vec![Value::Int(2), Value::Null]];
        let flat = rows.concat();
        let insert = Call { cursor: None, ..call("INSERT", "INSERT INTO T VALUES (?, ?)", &flat) };
        let fetch = Call { cursor: Some("C1"), ..call("FETCH", "FETCH NEXT ROWSET FROM C1 FOR ? ROWS", &[Value::Int(2)]) };
        let procedure = Call { cursor: Some("P1"), ..call("CALL", "CALL P1 (?, ?, ?)", &inputs) };
        let live = [recorder.insert_rows(&insert, &rows, true), recorder.fetch_rows(&fetch, 2), recorder.call(&procedure)];
        let text = String::from_utf8(written.borrow().clone()).unwrap();
        assert!(text.contains("> int:1 | char:\"X\"\n> int:2 | null\n"), "{text}");
        assert!(text.contains("= - | char:\"A|B\" | null\n"), "{text}");
        let mut replay = Replay::parse(&text, false).unwrap();
        assert_eq!([replay.insert_rows(&insert, &rows, true), replay.fetch_rows(&fetch, 2), replay.call(&procedure)], live);
    }

    #[test]
    fn malformed_recordings_name_the_line() {
        assert_eq!(Replay::parse("@ 1 P:1:0 SELECT\n< 0 00000 rows=0\n", false).err().unwrap(), format!("line 1: a recording starts with \"{HEADER}\""));
        let err = Replay::parse(&format!("{HEADER}\n@ 1 P:1:0 SELECT\n= int:1\n"), false).err().unwrap();
        assert_eq!(err, "line 3: an = line belongs after a < line");
        assert!(Replay::parse(&format!("{HEADER}\n@ 1 P:1:0 SELECT\n< 0 00000 rows=0\n= int:x\n"), false).err().unwrap().starts_with("line 4: "));
        assert_eq!(Replay::parse(&format!("{HEADER}\n@ 1 P:1:0 SELECT\n"), false).err().unwrap(), "the last call has no < line");
        assert_eq!(Replay::parse("# ironwork sql recording 2\n", false).err().unwrap(), "line 1: this ironwork reads sql recording 1, and this recording is 2");
        let call_lines = |equals: &str| Replay::parse(&format!("{HEADER}\n@ 1 P:1:0 CALL P\n< 0 00000 rows=0\n{equals}"), false).err();
        assert_eq!(call_lines("= - | int:1\n= int:2\n").unwrap(), "line 5: a CALL has one = line");
        assert!(call_lines("= -- | int:1\n").unwrap().starts_with("line 4: "));
        assert_eq!(call_lines("= - | int:1\n"), None);
    }
}
