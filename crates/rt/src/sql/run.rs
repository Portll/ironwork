//! Running an EXEC SQL statement: what it asks of the executor (`SqlHost`), its call to the
//! database, the cursor and unit-of-work state the session keeps, the SQLCA, and the SQLCODE and
//! warning WHENEVER tests.

use super::host::{self, COLUMN_COUNT, TRUNCATED, Warnings};
use super::{Answer, Call, Database, Outcome, Session};
use crate::abend::Abend;
use crate::host::Host;
use crate::lir::{AbendId, HostPlace, SqlEntry, SqlStatement, Sqlca};
use crate::vocab::Pos;

type R<T> = Result<T, Abend>;

/// Deadlock or timeout: the backend has rolled the unit of work back.
const DEADLOCK: i32 = -911;

/// What a statement asks of the executor running it beyond `Host`.
pub trait SqlHost<'w, P: Copy, S>: Host<P> {
    /// The run unit's database session, when one is attached.
    fn session(&mut self) -> Option<&mut Session<'w>>;
    /// A CICS task, whose unit of work SYNCPOINT ends rather than EXEC SQL.
    fn in_task(&self) -> bool;
    fn program_id(&self) -> String;
    fn text(&self, text: &S) -> String;
    /// Where a host variable is written, which a program check reading it names.
    fn place_pos(&self, place: P) -> Pos;
    /// The abend a host variable with no SQL type gives when its statement reaches it.
    fn untyped(&mut self, abend: AbendId) -> Abend;
}

/// What WHENEVER tests after a statement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ran {
    pub sqlcode: i32,
    /// SQLWARN0.
    pub warned: bool,
}

fn session<'a, 'w: 'a, P: Copy + 'a, S: 'a>(x: &'a mut impl SqlHost<'w, P, S>) -> &'a mut Session<'w> {
    x.session().expect("a database is attached")
}

/// Runs a statement and fills the SQLCA; None for a declaration, which does nothing.
pub fn run<'w, P: Copy, S>(x: &mut impl SqlHost<'w, P, S>, entry: &SqlEntry<P, S>, sqlca: &Sqlca<P>, pos: Pos) -> R<Option<Ran>> {
    let verb = x.text(&entry.verb);
    let refused = |why: String| Abend { code: "EXEC".into(), message: format!("EXEC SQL {verb} was reached: {why}"), pos, file: None };
    if x.session().is_none() {
        return Err(refused("no database is attached to the run".into()));
    }
    let (program, text, ordinal) = (x.program_id(), x.text(&entry.text), entry.ordinal);
    let mut warnings = Warnings::default();
    let mut outcome = match &entry.statement {
        SqlStatement::Query { inputs, into } => match host::inputs(x, inputs)? {
            Err(e) => Outcome::error(e.code, e.state),
            Ok(values) => {
                let answer = database(x, &Call { program: &program, ordinal, verb: &verb, cursor: None, text: &text, inputs: &values }, |db, c| db.execute(c), pos)?;
                single_row(x, answer, into, &mut warnings)?
            }
        },
        SqlStatement::Change { delete, inputs, current_of } => {
            let current_of = current_of.as_ref().map(|c| x.text(c));
            let position = current_of.as_deref().map(|c| session(x).cursor(&program, c).map(|c| c.positioned));
            match (position, host::inputs(x, inputs)?) {
                (Some(None), _) => Outcome::error(-507, "24501"),
                (Some(Some(false)), _) => Outcome::error(-508, "24504"),
                (_, Err(e)) => Outcome::error(e.code, e.state),
                (_, Ok(values)) => {
                    let mut answer = database(x, &Call { program: &program, ordinal, verb: &verb, cursor: current_of.as_deref(), text: &text, inputs: &values }, |db, c| db.execute(c), pos)?;
                    // Db2 answers a searched change that finds no row with +100.
                    if current_of.is_none() && answer.sqlcode == 0 && answer.affected == 0 {
                        answer = Outcome::error(100, "02000");
                    }
                    if let (Some(c), true, true) = (&current_of, *delete, answer.sqlcode >= 0)
                        && let Some(open) = session(x).cursor(&program, c)
                    {
                        open.positioned = false;
                    }
                    answer
                }
            }
        }
        SqlStatement::Open { cursor, inputs } => {
            let cursor = x.text(cursor);
            if session(x).cursor(&program, &cursor).is_some() {
                Outcome::error(-502, "24502")
            } else {
                match host::inputs(x, inputs)? {
                    Err(e) => Outcome::error(e.code, e.state),
                    Ok(values) => {
                        let answer = database(x, &Call { program: &program, ordinal, verb: "OPEN", cursor: Some(&cursor), text: &text, inputs: &values }, |db, c| db.open(c), pos)?;
                        if answer.sqlcode >= 0 {
                            session(x).opened(&program, &cursor, entry.with_hold);
                        }
                        answer
                    }
                }
            }
        }
        SqlStatement::Fetch { cursor, into } => {
            let cursor = x.text(cursor);
            if session(x).cursor(&program, &cursor).is_none() {
                Outcome::error(-501, "24501")
            } else {
                let answer = database(x, &Call { program: &program, ordinal, verb: "FETCH", cursor: Some(&cursor), text: &text, inputs: &[] }, |db, c| db.fetch(c), pos)?;
                if answer.rows.len() > 1 {
                    return Err(Abend { code: "SQL".into(), message: format!("the database answered FETCH {cursor} with {} rows", answer.rows.len()), pos, file: None });
                }
                let on_row = answer.sqlcode >= 0 && answer.rows.len() == 1;
                if let Some(open) = session(x).cursor(&program, &cursor) {
                    open.positioned = on_row;
                }
                let fetched = single_row(x, answer, into, &mut warnings)?;
                Outcome { affected: i64::from(on_row), ..fetched }
            }
        }
        SqlStatement::Close { cursor } => {
            let cursor = x.text(cursor);
            if session(x).cursor(&program, &cursor).is_none() {
                Outcome::error(-501, "24501")
            } else {
                let answer = database(x, &Call { program: &program, ordinal, verb: "CLOSE", cursor: Some(&cursor), text: &text, inputs: &[] }, |db, c| db.close(c), pos)?;
                if answer.sqlcode >= 0 {
                    session(x).closed(&program, &cursor);
                }
                answer
            }
        }
        SqlStatement::Commit if x.in_task() => Outcome::error(-925, "2D521"),
        SqlStatement::Rollback if x.in_task() => Outcome::error(-926, "2D521"),
        SqlStatement::Commit => {
            let answer = database(x, &Call { program: &program, ordinal, verb: "COMMIT", cursor: None, text: &text, inputs: &[] }, |db, c| db.commit(c), pos)?;
            if answer.sqlcode >= 0 {
                session(x).committed();
            }
            answer
        }
        SqlStatement::Rollback => {
            let answer = database(x, &Call { program: &program, ordinal, verb: "ROLLBACK", cursor: None, text: &text, inputs: &[] }, |db, c| db.rollback(c), pos)?;
            if answer.sqlcode >= 0 {
                session(x).rolled_back();
            }
            answer
        }
        SqlStatement::Declaration => return Ok(None),
        SqlStatement::Unsupported(what) => return Err(refused(format!("ironwork for COBOL does not run {}", x.text(what)))),
    };
    if outcome.sqlcode == DEADLOCK {
        session(x).rolled_back();
    }
    warnings[0] = warnings[1..].iter().any(|&w| w);
    if outcome.sqlcode == 0 && outcome.sqlstate == "00000" {
        if warnings[TRUNCATED] {
            outcome.sqlstate = "01004".into();
        } else if warnings[COLUMN_COUNT] {
            outcome.sqlstate = "01503".into();
        }
    }
    host::sqlca(x, sqlca, &outcome, &warnings);
    Ok(Some(Ran { sqlcode: outcome.sqlcode, warned: warnings[0] }))
}

/// One call to the database. Any statement but COMMIT and ROLLBACK leaves work to settle.
fn database<'w, P: Copy, S>(x: &mut impl SqlHost<'w, P, S>, call: &Call, run: impl FnOnce(&mut dyn Database, &Call) -> Answer, pos: Pos) -> R<Outcome> {
    let session = session(x);
    session.pending |= !matches!(call.verb, "COMMIT" | "ROLLBACK");
    run(&mut *session.database, call).map_err(|a| Abend { code: a.code.into(), message: a.message, pos, file: None })
}

/// A SELECT INTO's or a FETCH's answer: one row is assigned, none is +100, more than one is -811.
fn single_row<'w, P: Copy, S>(x: &mut impl SqlHost<'w, P, S>, answer: Outcome, into: &[HostPlace<P>], warnings: &mut Warnings) -> R<Outcome> {
    if answer.sqlcode < 0 {
        return Ok(answer);
    }
    Ok(match answer.rows.len() {
        0 => Outcome::error(100, "02000"),
        1 => match host::assign(x, into, &answer.rows[0], warnings)? {
            Ok(()) => answer,
            Err(e) => Outcome::error(e.code, e.state),
        },
        _ => Outcome::error(-811, "21000"),
    })
}
