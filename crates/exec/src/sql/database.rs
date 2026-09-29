//! What answers SQL statements. A backend sees each statement's identity, canonical text and input
//! values, and answers with rows or a code; the runtime decides what those rows mean for the
//! program, so every backend answers alike.

use super::Value;
use std::collections::HashMap;

/// One statement as a backend receives it.
#[derive(Clone, Copy, Debug)]
pub struct Call<'a> {
    pub program: &'a str,
    /// The statement's place among its program's EXEC SQL blocks, from 1.
    pub ordinal: u32,
    /// SELECT, INSERT, UPDATE, DELETE, OPEN, FETCH, CLOSE, COMMIT or ROLLBACK.
    pub verb: &'a str,
    pub cursor: Option<&'a str>,
    /// The canonical text, with `?` for each input.
    pub text: &'a str,
    pub inputs: &'a [Value],
}

/// A backend's answer.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub sqlcode: i32,
    pub sqlstate: String,
    /// Rows an INSERT, UPDATE or DELETE affected: SQLERRD(3).
    pub affected: i64,
    pub rows: Vec<Vec<Value>>,
    /// SQLERRMC's message tokens.
    pub tokens: String,
}

impl Outcome {
    pub fn ok() -> Self {
        Self { sqlcode: 0, sqlstate: "00000".into(), affected: 0, rows: Vec::new(), tokens: String::new() }
    }

    pub fn rows(rows: Vec<Vec<Value>>) -> Self {
        Self { rows, ..Self::ok() }
    }

    pub fn error(sqlcode: i32, sqlstate: &str) -> Self {
        Self { sqlcode, sqlstate: sqlstate.into(), ..Self::ok() }
    }
}

/// Why a backend could not answer at all, which ends the run: a replay that does not hold the
/// call (`SQLR`), or a connection that failed (`SQL`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Abandoned {
    pub code: &'static str,
    pub message: String,
}

pub type Answer = Result<Outcome, Abandoned>;

pub trait Database {
    /// SELECT INTO, SET, VALUES INTO, INSERT, UPDATE and DELETE.
    fn execute(&mut self, call: &Call) -> Answer;
    fn open(&mut self, call: &Call) -> Answer;
    /// One row, or no row at the end.
    fn fetch(&mut self, call: &Call) -> Answer;
    fn close(&mut self, call: &Call) -> Answer;
    fn commit(&mut self, call: &Call) -> Answer;
    fn rollback(&mut self, call: &Call) -> Answer;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenCursor {
    pub with_hold: bool,
    /// On a row, so a positioned UPDATE or DELETE has one to change.
    pub positioned: bool,
}

/// A run unit's connection to its database, and the state the runtime keeps rather than asking
/// the backend, so every backend answers alike.
pub struct Session<'w> {
    pub database: Box<dyn Database + 'w>,
    /// Open cursors by program and cursor name, as two programs may declare the same name.
    cursors: HashMap<(String, String), OpenCursor>,
    /// Whether any statement has reached the database since the last COMMIT or ROLLBACK.
    pub pending: bool,
}

impl<'w> Session<'w> {
    pub fn new(database: Box<dyn Database + 'w>) -> Self {
        Self { database, cursors: HashMap::new(), pending: false }
    }

    pub fn cursor(&mut self, program: &str, name: &str) -> Option<&mut OpenCursor> {
        self.cursors.get_mut(&(program.to_owned(), name.to_owned()))
    }

    pub fn opened(&mut self, program: &str, name: &str, with_hold: bool) {
        self.cursors.insert((program.to_owned(), name.to_owned()), OpenCursor { with_hold, positioned: false });
    }

    pub fn closed(&mut self, program: &str, name: &str) {
        self.cursors.remove(&(program.to_owned(), name.to_owned()));
    }

    /// COMMIT closes every cursor not declared WITH HOLD, and leaves a held one before its next row.
    pub fn committed(&mut self) {
        self.cursors.retain(|_, c| c.with_hold);
        self.cursors.values_mut().for_each(|c| c.positioned = false);
        self.pending = false;
    }

    pub fn rolled_back(&mut self) {
        self.cursors.clear();
        self.pending = false;
    }

    /// Ends a unit of work that no EXEC SQL statement ends: at SYNCPOINT, or at the end of a run
    /// unit or task. The database is asked only while it holds work or an open cursor, and the
    /// call names `program` with ordinal 0.
    pub fn settle(&mut self, program: &str, commit: bool) -> Answer {
        let answer = if !self.pending && self.cursors.is_empty() {
            Outcome::ok()
        } else {
            let verb = if commit { "COMMIT" } else { "ROLLBACK" };
            let call = Call { program, ordinal: 0, verb, cursor: None, text: verb, inputs: &[] };
            if commit { self.database.commit(&call)? } else { self.database.rollback(&call)? }
        };
        if commit && answer.sqlcode >= 0 {
            self.committed();
        } else {
            self.rolled_back();
        }
        Ok(answer)
    }
}
