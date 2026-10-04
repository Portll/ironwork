//! Running an EXEC SQL statement: what it asks of the executor (`SqlHost`), its call to the
//! database, the cursor and unit-of-work state the session keeps, the SQLCA, and the SQLCODE and
//! warning WHENEVER tests.

use super::dynamic::{self, Kind};
use super::host::{self, COLUMN_COUNT, TRUNCATED, Warnings};
use super::{Answer, Call, Database, Outcome, Prepared, Session, SqlError, Value};
use crate::abend::Abend;
use crate::host::Host;
use crate::lir::{AbendId, HostPlace, SqlEntry, SqlStatement, Sqlca};
use crate::storage::Loc;
use crate::vocab::Pos;

type R<T> = Result<T, Abend>;

/// Deadlock or timeout: the backend has rolled the unit of work back.
const DEADLOCK: i32 = -911;

/// Db2 13 for z/OS SQL, PREPARE and EXECUTE IMMEDIATE: "In ... COBOL ... a host variable must be a
/// varying-length string variable."
const STRING_RULE: &str = "its statement string is not one varying-length character or graphic string, as Db2 for z/OS requires of COBOL";

/// Where a statement's calls come from.
#[derive(Clone, Copy)]
struct At<'a> {
    program: &'a str,
    ordinal: u32,
    pos: Pos,
}

impl<'a> At<'a> {
    fn call(self, verb: &'a str, cursor: Option<&'a str>, text: &'a str, inputs: &'a [Value]) -> Call<'a> {
        Call { program: self.program, ordinal: self.ordinal, verb, cursor, text, inputs }
    }
}

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
    /// Tells the observer, for the input trace, the operand of an operation an input could steer.
    fn sink(&mut self, kind: &'static str, pos: Pos, operand: &str);
    /// An indicator variable's storage; an executor whose place for an indicator array named
    /// without subscripts is not its first element locates that element here.
    fn locate_indicator(&mut self, place: P) -> R<Loc> {
        self.locate(place, false)
    }
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
    if let SqlStatement::Connect { what, location } = &entry.statement {
        // A location the trace cannot read is left to the refusal, so tracing never changes how the run ends.
        if !location.is_empty()
            && let Ok(Ok(values)) = host::traced(x, location)
        {
            x.sink("connection-target", pos, &values.iter().map(Value::text).collect::<String>());
        }
        return Err(refused(format!("ironwork for COBOL does not run {}", x.text(what))));
    }
    // A statement string goes to the input trace before the run is refused for want of a database,
    // as CONNECT's location does.
    let string = match &entry.statement {
        SqlStatement::Prepare { source, .. } | SqlStatement::ExecuteImmediate { source } => match statement_string(x, source, pos)? {
            None => return Err(refused(STRING_RULE.into())),
            string => string,
        },
        _ => None,
    };
    if x.session().is_none() {
        return Err(refused("no database is attached to the run".into()));
    }
    let (program, text, ordinal) = (x.program_id(), x.text(&entry.text), entry.ordinal);
    let at = At { program: &program, ordinal, pos };
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
            let values = host::inputs(x, inputs)?;
            change(x, at, (&verb, &text), values, *delete, current_of.as_deref())?
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
                            session(x).opened(&program, &cursor, entry.with_hold, None);
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
        SqlStatement::Commit => end_unit(x, at, &text, true)?,
        SqlStatement::Rollback => end_unit(x, at, &text, false)?,
        SqlStatement::Prepare { name, .. } => {
            let name = x.text(name);
            match string {
                None => unreachable!("a PREPARE has its statement string"),
                Some(Err(e)) => Outcome::error(e.code, e.state),
                Some(Ok(_)) if session(x).running(&program, &name) => Outcome::error(-519, "24506"),
                Some(Ok(text)) => {
                    session(x).prepare(&program, &name, None);
                    let kind = dynamic::kind(&text);
                    if kind == Kind::Unacceptable {
                        Outcome::error(-84, "42612")
                    } else {
                        let answer = database(x, &at.call("PREPARE", Some(&name), &text, &[]), |db, c| db.prepare(c), pos)?;
                        if answer.sqlcode >= 0 {
                            let markers = dynamic::markers(&text);
                            session(x).prepare(&program, &name, Some(Prepared { text, query: kind == Kind::Query, markers }));
                        }
                        answer
                    }
                }
            }
        }
        SqlStatement::ExecuteImmediate { .. } => match string {
            None => unreachable!("an EXECUTE IMMEDIATE has its statement string"),
            Some(Err(e)) => Outcome::error(e.code, e.state),
            Some(Ok(text)) => dynamic_statement(x, at, &text, Ok(Vec::new()), &refused)?,
        },
        SqlStatement::Execute { name, inputs } => {
            let name = x.text(name);
            match session(x).prepared(&program, &name).cloned() {
                None | Some(Prepared { query: true, .. }) => Outcome::error(-518, "07003"),
                Some(p) if p.markers > 0 && p.markers != inputs.len() => Outcome::error(-313, "07001"),
                Some(p) => {
                    let values = if p.markers == 0 { Ok(Vec::new()) } else { host::inputs(x, inputs)? };
                    dynamic_statement(x, at, &p.text, values, &refused)?
                }
            }
        }
        SqlStatement::OpenPrepared { cursor, statement, inputs } => {
            let (cursor, statement) = (x.text(cursor), x.text(statement));
            if session(x).cursor(&program, &cursor).is_some() {
                Outcome::error(-502, "24502")
            } else {
                match session(x).prepared(&program, &statement).cloned() {
                    None => Outcome::error(-514, "26501"),
                    Some(Prepared { query: false, .. }) => Outcome::error(-517, "07005"),
                    Some(p) if p.markers > 0 && p.markers != inputs.len() => Outcome::error(-313, "07001"),
                    Some(p) => match if p.markers == 0 { Ok(Vec::new()) } else { host::inputs(x, inputs)? } {
                        Err(e) => Outcome::error(e.code, e.state),
                        Ok(values) => {
                            let hold = if entry.with_hold { " WITH HOLD" } else { "" };
                            let text = format!("DECLARE {cursor} CURSOR{hold} FOR {}", p.text);
                            let answer = database(x, &at.call("OPEN", Some(&cursor), &text, &values), |db, c| db.open(c), pos)?;
                            if answer.sqlcode >= 0 {
                                session(x).opened(&program, &cursor, entry.with_hold, Some(&statement));
                            }
                            answer
                        }
                    },
                }
            }
        }
        SqlStatement::Declaration => return Ok(None),
        SqlStatement::Unsupported(what) => return Err(refused(format!("ironwork for COBOL does not run {}", x.text(what)))),
        SqlStatement::Connect { .. } => unreachable!("CONNECT is refused before the session is asked"),
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

/// COMMIT or ROLLBACK, static or dynamic; in a CICS task the unit of work is CICS's.
fn end_unit<'w, P: Copy, S>(x: &mut impl SqlHost<'w, P, S>, at: At, text: &str, commit: bool) -> R<Outcome> {
    if x.in_task() {
        return Ok(if commit { Outcome::error(-925, "2D521") } else { Outcome::error(-926, "2D521") });
    }
    let verb = if commit { "COMMIT" } else { "ROLLBACK" };
    let answer = database(x, &at.call(verb, None, text, &[]), |db, c| if commit { db.commit(c) } else { db.rollback(c) }, at.pos)?;
    if answer.sqlcode >= 0 {
        if commit { session(x).committed() } else { session(x).rolled_back() }
    }
    Ok(answer)
}

/// An INSERT, UPDATE or DELETE, `statement` its verb and text: a positioned one needs its cursor on
/// a row, and a searched one that changes no row is +100, as Db2 answers.
fn change<'w, P: Copy, S>(x: &mut impl SqlHost<'w, P, S>, at: At, statement: (&str, &str), values: Result<Vec<Value>, SqlError>, delete: bool, current_of: Option<&str>) -> R<Outcome> {
    let position = current_of.map(|c| session(x).cursor(at.program, c).map(|c| c.positioned));
    let values = match (position, values) {
        (Some(None), _) => return Ok(Outcome::error(-507, "24501")),
        (Some(Some(false)), _) => return Ok(Outcome::error(-508, "24504")),
        (_, Err(e)) => return Ok(Outcome::error(e.code, e.state)),
        (_, Ok(values)) => values,
    };
    let mut answer = database(x, &at.call(statement.0, current_of, statement.1, &values), |db, c| db.execute(c), at.pos)?;
    if current_of.is_none() && answer.sqlcode == 0 && answer.affected == 0 {
        answer = Outcome::error(100, "02000");
    }
    if let (Some(c), true, true) = (current_of, delete, answer.sqlcode >= 0)
        && let Some(open) = session(x).cursor(at.program, c)
    {
        open.positioned = false;
    }
    Ok(answer)
}

/// The statement string of a PREPARE or EXECUTE IMMEDIATE, normalised, after the input trace is
/// told of it; None where it is not one varying-length string.
fn statement_string<'w, P: Copy, S>(x: &mut impl SqlHost<'w, P, S>, source: &[crate::lir::HostPlace<P>], pos: Pos) -> R<Option<Result<String, SqlError>>> {
    match source {
        [place] if matches!(place.ty, Ok(super::HostType::VarChar(_) | super::HostType::VarGraphic(_)) | Err(_)) => {}
        _ => return Ok(None),
    }
    Ok(Some(host::traced(x, source)?.map(|values| {
        let string = values.first().map(Value::text).unwrap_or_default();
        x.sink("dynamic-sql", pos, &string);
        dynamic::normalise(&string)
    })))
}

/// A dynamic statement that is not run by a cursor, as EXECUTE and EXECUTE IMMEDIATE run it.
fn dynamic_statement<'w, P: Copy, S>(x: &mut impl SqlHost<'w, P, S>, at: At, text: &str, values: Result<Vec<Value>, SqlError>, refused: &dyn Fn(String) -> Abend) -> R<Outcome> {
    let verb = dynamic::verb(text);
    Ok(match dynamic::kind(text) {
        Kind::Query => Outcome::error(-518, "07003"),
        Kind::Unacceptable => Outcome::error(-84, "42612"),
        Kind::Refused(what) => return Err(refused(format!("ironwork for COBOL does not run {what}"))),
        Kind::Commit => end_unit(x, at, text, true)?,
        Kind::Rollback => end_unit(x, at, text, false)?,
        Kind::Change { delete, current_of } => change(x, at, (&verb, text), values, delete, current_of.as_deref())?,
        Kind::Other => match values {
            Err(e) => Outcome::error(e.code, e.state),
            Ok(values) => database(x, &at.call(&verb, None, text, &values), |db, c| db.execute(c), at.pos)?,
        },
    })
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
