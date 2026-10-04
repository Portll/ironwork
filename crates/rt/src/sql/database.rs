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
    /// SELECT, INSERT, UPDATE, DELETE, OPEN, FETCH, CLOSE, COMMIT or ROLLBACK; PREPARE; or a
    /// dynamic statement string's own command word.
    pub verb: &'a str,
    /// The cursor, or the statement name PREPARE gives.
    pub cursor: Option<&'a str>,
    /// The canonical text, with `?` for each input; a dynamic statement's string as it runs.
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
    /// SELECT INTO, SET, VALUES INTO, INSERT, UPDATE and DELETE, and a dynamic statement that is
    /// not a query.
    fn execute(&mut self, call: &Call) -> Answer;
    /// Checks a statement string PREPARE names, which then runs through `execute` or `open`.
    fn prepare(&mut self, call: &Call) -> Answer;
    fn open(&mut self, call: &Call) -> Answer;
    /// One row, or no row at the end.
    fn fetch(&mut self, call: &Call) -> Answer;
    fn close(&mut self, call: &Call) -> Answer;
    fn commit(&mut self, call: &Call) -> Answer;
    fn rollback(&mut self, call: &Call) -> Answer;
    /// Closes every cursor, held ones too, as the end of a CICS task does, so the next task on the
    /// same connection finds none open. A backend without cursors of its own has nothing to close.
    fn close_all(&mut self) -> Result<(), Abandoned> {
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenCursor {
    pub with_hold: bool,
    /// On a row, so a positioned UPDATE or DELETE has one to change.
    pub positioned: bool,
    /// The prepared statement a cursor for one runs.
    pub statement: Option<String>,
}

/// A statement PREPARE made, as EXECUTE and OPEN run it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prepared {
    pub text: String,
    pub query: bool,
    pub markers: usize,
}

/// A run unit's connection to its database, and the state the runtime keeps rather than asking
/// the backend, so every backend answers alike.
pub struct Session<'w> {
    pub database: &'w mut dyn Database,
    /// Open cursors by program and cursor name, as two programs may declare the same name.
    cursors: HashMap<(String, String), OpenCursor>,
    /// Prepared statements by program and statement name, a statement name's scope being a
    /// cursor name's.
    prepared: HashMap<(String, String), Prepared>,
    /// Whether any statement has reached the database since the last COMMIT or ROLLBACK.
    pub pending: bool,
}

impl<'w> Session<'w> {
    pub fn new(database: &'w mut (dyn Database + '_)) -> Self {
        Self { database, cursors: HashMap::new(), prepared: HashMap::new(), pending: false }
    }

    pub fn cursor(&mut self, program: &str, name: &str) -> Option<&mut OpenCursor> {
        self.cursors.get_mut(&(program.to_owned(), name.to_owned()))
    }

    pub fn opened(&mut self, program: &str, name: &str, with_hold: bool, statement: Option<&str>) {
        self.cursors.insert((program.to_owned(), name.to_owned()), OpenCursor { with_hold, positioned: false, statement: statement.map(str::to_owned) });
    }

    pub fn prepared(&self, program: &str, name: &str) -> Option<&Prepared> {
        self.prepared.get(&(program.to_owned(), name.to_owned()))
    }

    pub fn prepare(&mut self, program: &str, name: &str, statement: Option<Prepared>) {
        let key = (program.to_owned(), name.to_owned());
        match statement {
            Some(p) => self.prepared.insert(key, p),
            None => self.prepared.remove(&key),
        };
    }

    /// Whether an open cursor of `program` runs the statement `name`, which PREPARE may not replace.
    pub fn running(&self, program: &str, name: &str) -> bool {
        self.cursors.iter().any(|((p, _), c)| p == program && c.statement.as_deref() == Some(name))
    }

    pub fn closed(&mut self, program: &str, name: &str) {
        self.cursors.remove(&(program.to_owned(), name.to_owned()));
    }

    /// COMMIT closes every cursor not declared WITH HOLD, and leaves a held one before its next row.
    /// It destroys the unit of work's prepared statements but those held cursors run, as
    /// KEEPDYNAMIC(NO) does.
    pub fn committed(&mut self) {
        self.cursors.retain(|_, c| c.with_hold);
        self.cursors.values_mut().for_each(|c| c.positioned = false);
        let cursors = &self.cursors;
        self.prepared.retain(|(program, name), _| cursors.iter().any(|((p, _), c)| p == program && c.statement.as_deref() == Some(name)));
        self.pending = false;
    }

    pub fn rolled_back(&mut self) {
        self.cursors.clear();
        self.prepared.clear();
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

    /// Ends a CICS task: settles its unit of work, then closes the held cursors a commit leaves
    /// open, as the end of a task closes every cursor.
    pub fn end_task(&mut self, program: &str, commit: bool) -> Answer {
        let answer = self.settle(program, commit)?;
        if !self.cursors.is_empty() {
            self.database.close_all()?;
            self.cursors.clear();
        }
        self.prepared.clear();
        Ok(answer)
    }
}
