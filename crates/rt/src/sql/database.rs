//! What answers SQL statements. A backend sees each statement's identity, canonical text and input
//! values, and answers with rows or a code; the runtime decides what those rows mean for the
//! program, so every backend answers alike.

use super::Value;
use std::collections::{HashMap, VecDeque};

/// One statement as a backend receives it.
#[derive(Clone, Copy, Debug)]
pub struct Call<'a> {
    pub program: &'a str,
    /// The statement's place among its program's EXEC SQL blocks, from 1.
    pub ordinal: u32,
    /// SELECT, INSERT, UPDATE, DELETE, OPEN, FETCH, CLOSE, COMMIT or ROLLBACK; PREPARE; CALL; or a
    /// dynamic statement string's own command word.
    pub verb: &'a str,
    /// The cursor, the statement name PREPARE gives, or the procedure CALL names.
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
    /// A PREPARE's description of its statement's result columns, which DESCRIBE gives the program.
    pub columns: Vec<Column>,
    /// A CALL's arguments as the procedure returns them, None for one it does not return.
    pub parameters: Vec<Option<Value>>,
}

/// A result column as DESCRIBE describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub ty: ColumnType,
    pub nullable: bool,
}

/// A result column's Db2 data type, as an SQLDA's SQLTYPE and SQLLEN give it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ColumnType {
    Char(u16),
    VarChar(u16),
    /// GRAPHIC and VARGRAPHIC, their lengths in DBCS characters.
    Graphic(u16),
    VarGraphic(u16),
    SmallInt,
    Integer,
    BigInt,
    Decimal { precision: u8, scale: u8 },
    Real,
    Double,
    Date,
    Time,
    /// TIMESTAMP(p).
    Timestamp(u8),
    Binary(u16),
    VarBinary(u16),
    /// A type the backend names that has no Db2 counterpart; DESCRIBE refuses it by this name.
    Other(String),
}

impl Outcome {
    pub fn ok() -> Self {
        Self { sqlcode: 0, sqlstate: "00000".into(), affected: 0, rows: Vec::new(), tokens: String::new(), columns: Vec::new(), parameters: Vec::new() }
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
    /// Up to `rows` rows for a rowset FETCH, fewer at the end.
    fn fetch_rows(&mut self, call: &Call, rows: u32) -> Answer;
    /// A multiple-row INSERT: the INSERT of one row `call` names, once for each of `rows`, whose
    /// values `call.inputs` holds one after another. SQLERRD(3) is the rows inserted. ATOMIC undoes
    /// them all when one fails; NOT ATOMIC keeps the rest, -253, or -254 when none went in.
    fn insert_rows(&mut self, call: &Call, rows: &[Vec<Value>], atomic: bool) -> Answer;
    /// CALL of a stored procedure, which a backend that runs none refuses; the answer's
    /// `parameters` give the arguments as the procedure returns them
    /// ([`numeric::assumptions::CALL_FROM_A_RECORDING`]).
    fn call(&mut self, call: &Call) -> Answer {
        Err(Abandoned { code: "EXEC", message: format!("EXEC SQL CALL {} was reached: this database runs no stored procedures; a recording of the CALL answers it (--sql-replay)", call.cursor.unwrap_or_default()) })
    }
    fn close(&mut self, call: &Call) -> Answer;
    fn commit(&mut self, call: &Call) -> Answer;
    fn rollback(&mut self, call: &Call) -> Answer;
    /// Closes every cursor, held ones too, as the end of a CICS task does, so the next task on the
    /// same connection finds none open. A backend without cursors of its own has nothing to close.
    fn close_all(&mut self) -> Result<(), Abandoned> {
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct OpenCursor {
    pub with_hold: bool,
    /// On a row, so a positioned UPDATE or DELETE has one to change.
    pub positioned: bool,
    /// The prepared statement a cursor for one runs.
    pub statement: Option<String>,
    /// The rows from the current position's first: the current rowset's, then those a rowset
    /// FETCH read ahead of a row FETCH, which moves from the rowset's first row (Db2 13 SQL, FETCH,
    /// Table 6).
    pub held: VecDeque<Vec<Value>>,
    /// How many of `held` the current position takes, 0 before the first.
    pub current: usize,
    /// The rows the last FETCH asked for, when it was a rowset FETCH, which the next asks for again
    /// without FOR n ROWS.
    pub rowset_size: Option<u32>,
}

impl OpenCursor {
    /// Where the backend's own cursor is: on the one row the position takes.
    pub fn on_backend_row(&self) -> bool {
        self.current == 1 && self.held.len() == 1
    }
}

/// A statement PREPARE made, as EXECUTE and OPEN run it and DESCRIBE describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prepared {
    pub text: String,
    pub query: bool,
    pub markers: usize,
    pub columns: Vec<Column>,
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
        let cursor = OpenCursor { with_hold, positioned: false, statement: statement.map(str::to_owned), held: VecDeque::new(), current: 0, rowset_size: None };
        self.cursors.insert((program.to_owned(), name.to_owned()), cursor);
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
        for c in self.cursors.values_mut() {
            c.positioned = false;
            c.held.drain(..c.current);
            c.current = 0;
        }
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
