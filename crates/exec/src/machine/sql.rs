//! EXEC SQL as the program meets it: host variables read and written through the SQL runtime, the
//! SQLCA filled, and the WHENEVER branch in force taken.

use super::*;
use crate::sql::{self, Call, HostType, NULL_WITHOUT_INDICATOR, Outcome, SqlError, Value};
use syntax::sql::{Action, ChangeKind, HostVar, Statement, Whenever};

/// SQLWARN0 to SQLWARNA, as the runtime sets them.
type Warnings = [bool; 11];
const TRUNCATED: usize = 1;
const COLUMN_COUNT: usize = 3;
/// Deadlock or timeout: the backend has rolled the unit of work back.
const DEADLOCK: i32 = -911;

/// A host variable's storage, its type, and its indicator's storage when it has one.
struct Target {
    offset: usize,
    len: usize,
    ty: HostType,
    indicator: Option<usize>,
}

impl<'p, 'w> Machine<'p, '_, 'w> {
    pub(super) fn sql(&mut self, block: &'p ExecBlock) -> R<Flow> {
        let sql = block.sql.as_ref().expect("the parser types every EXEC SQL block");
        let (pos, program) = (block.pos, self.program.id.as_str());
        let refused = |why: String| Abend { code: "EXEC".into(), message: format!("EXEC SQL {} was reached: {why}", block.command), pos };
        if self.unit.sql.is_none() {
            return Err(refused("no database is attached to the run".into()));
        }
        let mut warnings = Warnings::default();
        let mut outcome = match &sql.statement {
            Statement::Query { text, inputs, into } => match self.sql_inputs(inputs, &block.command)? {
                Err(e) => Outcome::error(e.code, e.state),
                Ok(values) => {
                    let answer = self.sql_call(&block.command, sql.ordinal, None, text, &values, |db, call| db.execute(call), pos)?;
                    self.sql_single_row(answer, into, &mut warnings, &block.command)?
                }
            },
            Statement::Change { kind, text, inputs, current_of } => {
                let position = current_of.as_deref().map(|c| self.session().cursor(program, c).map(|c| c.positioned));
                match (position, self.sql_inputs(inputs, &block.command)?) {
                    (Some(None), _) => Outcome::error(-507, "24501"),
                    (Some(Some(false)), _) => Outcome::error(-508, "24504"),
                    (_, Err(e)) => Outcome::error(e.code, e.state),
                    (_, Ok(values)) => {
                        let mut answer = self.sql_call(&block.command, sql.ordinal, current_of.as_deref(), text, &values, |db, call| db.execute(call), pos)?;
                        // Db2 answers a searched change that finds no row with +100.
                        if current_of.is_none() && answer.sqlcode == 0 && answer.affected == 0 {
                            answer = Outcome::error(100, "02000");
                        }
                        if let (Some(c), ChangeKind::Delete, true) = (current_of, kind, answer.sqlcode >= 0)
                            && let Some(open) = self.session().cursor(program, c)
                        {
                            open.positioned = false;
                        }
                        answer
                    }
                }
            }
            Statement::Open { cursor, declared } => {
                let declared = declared.as_ref().expect("the parser gives OPEN its DECLARE");
                if self.session().cursor(program, cursor).is_some() {
                    Outcome::error(-502, "24502")
                } else {
                    match self.sql_inputs(&declared.inputs, &block.command)? {
                        Err(e) => Outcome::error(e.code, e.state),
                        Ok(values) => {
                            let hold = if declared.with_hold { " WITH HOLD" } else { "" };
                            let text = format!("DECLARE {cursor} CURSOR{hold} FOR {}", declared.text);
                            let answer = self.sql_call("OPEN", sql.ordinal, Some(cursor), &text, &values, |db, call| db.open(call), pos)?;
                            if answer.sqlcode >= 0 {
                                self.session().opened(program, cursor, declared.with_hold);
                            }
                            answer
                        }
                    }
                }
            }
            Statement::Fetch { cursor, into } => {
                if self.session().cursor(program, cursor).is_none() {
                    Outcome::error(-501, "24501")
                } else {
                    let answer = self.sql_call("FETCH", sql.ordinal, Some(cursor), &format!("FETCH {cursor}"), &[], |db, call| db.fetch(call), pos)?;
                    if answer.rows.len() > 1 {
                        return Err(Abend { code: "SQL".into(), message: format!("the database answered FETCH {cursor} with {} rows", answer.rows.len()), pos });
                    }
                    let on_row = answer.sqlcode >= 0 && answer.rows.len() == 1;
                    if let Some(open) = self.session().cursor(program, cursor) {
                        open.positioned = on_row;
                    }
                    let fetched = self.sql_single_row(answer, into, &mut warnings, &block.command)?;
                    Outcome { affected: i64::from(on_row), ..fetched }
                }
            }
            Statement::Close { cursor } => {
                if self.session().cursor(program, cursor).is_none() {
                    Outcome::error(-501, "24501")
                } else {
                    let answer = self.sql_call("CLOSE", sql.ordinal, Some(cursor), &format!("CLOSE {cursor}"), &[], |db, call| db.close(call), pos)?;
                    if answer.sqlcode >= 0 {
                        self.session().closed(program, cursor);
                    }
                    answer
                }
            }
            Statement::Commit if self.unit.cics.is_some() => Outcome::error(-925, "2D521"),
            Statement::Rollback if self.unit.cics.is_some() => Outcome::error(-926, "2D521"),
            Statement::Commit => {
                let answer = self.sql_call("COMMIT", sql.ordinal, None, "COMMIT", &[], |db, call| db.commit(call), pos)?;
                if answer.sqlcode >= 0 {
                    self.session().committed();
                }
                answer
            }
            Statement::Rollback => {
                let answer = self.sql_call("ROLLBACK", sql.ordinal, None, "ROLLBACK", &[], |db, call| db.rollback(call), pos)?;
                if answer.sqlcode >= 0 {
                    self.session().rolled_back();
                }
                answer
            }
            Statement::Whenever { .. } | Statement::Declaration | Statement::DeclareCursor(_) | Statement::DeclareUnsupported { .. } => return Ok(Flow::Next),
            Statement::Unsupported(what) => return Err(refused(format!("ironwork for COBOL does not run {what}"))),
            Statement::Malformed(why) => unreachable!("the compiler refuses a malformed statement: {why}"),
        };
        if outcome.sqlcode == DEADLOCK {
            self.session().rolled_back();
        }
        warnings[0] = warnings[1..].iter().any(|&w| w);
        if outcome.sqlcode == 0 && outcome.sqlstate == "00000" {
            if warnings[TRUNCATED] {
                outcome.sqlstate = "01004".into();
            } else if warnings[COLUMN_COUNT] {
                outcome.sqlstate = "01503".into();
            }
        }
        self.sqlca(&outcome, &warnings, pos)?;
        self.whenever(&sql.whenever, outcome.sqlcode, warnings[0], pos)
    }

    /// EXEC CICS SYNCPOINT commits the task's unit of work, and SYNCPOINT ROLLBACK backs it out. A
    /// commit the database refuses leaves the work backed out and raises ROLLEDBACK.
    pub(super) fn cics_syncpoint(&mut self, block: &ExecBlock) -> R<Flow> {
        let (pos, program) = (block.pos, self.program.id.as_str());
        let commit = !super::cics::has(block, "ROLLBACK");
        if let Some(session) = self.unit.sql.as_mut() {
            let answer = session.settle(program, commit).map_err(|a| Abend { code: a.code.into(), message: a.message, pos })?;
            if answer.sqlcode < 0 && commit {
                return self.raise(block, crate::cics::Condition::ROLLEDBACK, 0);
            }
            if answer.sqlcode < 0 {
                let message = format!("SYNCPOINT ROLLBACK: the database refused to roll back with SQLCODE {}", answer.sqlcode);
                return Err(Abend { code: "SQL".into(), message, pos });
            }
        }
        self.cics_ok(block)
    }

    fn session(&mut self) -> &mut sql::Session<'w> {
        self.unit.sql.as_mut().expect("a database is attached")
    }

    #[allow(clippy::too_many_arguments)]
    fn sql_call(&mut self, verb: &str, ordinal: u32, cursor: Option<&str>, text: &str, inputs: &[Value], run: impl FnOnce(&mut dyn sql::Database, &Call) -> sql::Answer, pos: Pos) -> R<Outcome> {
        let call = Call { program: &self.program.id, ordinal, verb, cursor, text, inputs };
        let session = self.unit.sql.as_mut().expect("a database is attached");
        session.pending |= !matches!(verb, "COMMIT" | "ROLLBACK");
        run(&mut *session.database, &call).map_err(|a| Abend { code: a.code.into(), message: a.message, pos })
    }

    /// A SELECT INTO's or a FETCH's answer: one row is assigned, none is +100, more than one is -811.
    fn sql_single_row(&mut self, answer: Outcome, into: &[HostVar], warnings: &mut Warnings, command: &str) -> R<Outcome> {
        if answer.sqlcode < 0 {
            return Ok(answer);
        }
        Ok(match answer.rows.len() {
            0 => Outcome::error(100, "02000"),
            1 => match self.sql_assign(into, &answer.rows[0], warnings, command)? {
                Ok(()) => answer,
                Err(e) => Outcome::error(e.code, e.state),
            },
            _ => Outcome::error(-811, "21000"),
        })
    }

    /// Each host variable's storage and type, a host structure's members one by one with the
    /// indicator array's elements beside them.
    fn sql_targets(&mut self, vars: &[HostVar], command: &str) -> R<Vec<Target>> {
        let mut out = Vec::new();
        for hv in vars {
            let loc = self.locate(&hv.var)?;
            let indicator = match &hv.indicator {
                Some(r) => Some(self.locate(r)?.offset),
                None => None,
            };
            let ty = sql::host_type(self.layout, loc.item)
                .map_err(|why| Abend { code: "EXEC".into(), message: format!("EXEC SQL {command}: {why}"), pos: hv.var.pos })?;
            match ty {
                HostType::Structure(members) => {
                    let start = self.layout.items[loc.item].offset as usize;
                    for (i, (m, ty)) in members.into_iter().enumerate() {
                        let item = &self.layout.items[m];
                        out.push(Target { offset: loc.offset + item.offset as usize - start, len: item.size as usize, ty, indicator: indicator.map(|at| at + 2 * i) });
                    }
                }
                ty => out.push(Target { offset: loc.offset, len: loc.len, ty, indicator }),
            }
        }
        Ok(out)
    }

    /// The values the input host variables send: NULL where the indicator is negative.
    fn sql_inputs(&mut self, vars: &[HostVar], command: &str) -> R<Result<Vec<Value>, SqlError>> {
        let mut values = Vec::new();
        for t in self.sql_targets(vars, command)? {
            if let Some(at) = t.indicator
                && i16::from_be_bytes([self.unit.mem[at], self.unit.mem[at + 1]]) < 0
            {
                values.push(Value::Null);
                continue;
            }
            match sql::read(&self.unit.mem[t.offset..t.offset + t.len], &t.ty, self.page, self.options.numproc) {
                Ok(v) => values.push(v),
                Err(sql::ReadError::Check(c)) => return Err(Abend::check(c, vars[0].var.pos)),
                Err(sql::ReadError::Sql(e)) => return Ok(Err(e)),
            }
        }
        Ok(Ok(values))
    }

    /// Assigns a row to the INTO host variables, setting each indicator: -1 for NULL, a cut
    /// string's original length, and 0 otherwise.
    fn sql_assign(&mut self, into: &[HostVar], row: &[Value], warnings: &mut Warnings, command: &str) -> R<Result<(), SqlError>> {
        let targets = self.sql_targets(into, command)?;
        if targets.len() != row.len() {
            warnings[COLUMN_COUNT] = true;
        }
        for (t, value) in targets.iter().zip(row) {
            let indicator = match value {
                Value::Null => match t.indicator {
                    Some(_) => -1,
                    None => return Ok(Err(NULL_WITHOUT_INDICATOR)),
                },
                value => match sql::write(value, &mut self.unit.mem[t.offset..t.offset + t.len], &t.ty, self.page) {
                    Err(e) => return Ok(Err(e)),
                    Ok(written) => {
                        warnings[TRUNCATED] |= written.truncated_from.is_some();
                        written.truncated_from.map_or(0, |n| n.min(i16::MAX as usize) as i16)
                    }
                },
            };
            if let Some(at) = t.indicator {
                self.unit.mem[at..at + 2].copy_from_slice(&indicator.to_be_bytes());
            }
        }
        Ok(Ok(()))
    }

    /// Writes the SQLCA fields the program declares, or its standalone SQLCODE and SQLSTATE.
    fn sqlca(&mut self, o: &Outcome, warnings: &Warnings, pos: Pos) -> R<()> {
        let mark = |on: bool| Value::Char(if on { "W" } else { " " }.into());
        let mut fields = vec![
            ("SQLCAID", None, Value::Char("SQLCA".into())),
            ("SQLCABC", None, Value::Int(136)),
            ("SQLCODE", None, Value::Int(o.sqlcode.into())),
            ("SQLERRML", None, Value::Int(o.tokens.len().min(70) as i64)),
            ("SQLERRMC", None, Value::Char(o.tokens.clone())),
            ("SQLERRP", None, Value::Char(String::new())),
            ("SQLSTATE", None, Value::Char(o.sqlstate.clone())),
        ];
        for n in 1..=6 {
            fields.push(("SQLERRD", Some(n), Value::Int(if n == 3 { o.affected } else { 0 })));
        }
        for (i, name) in ["SQLWARN0", "SQLWARN1", "SQLWARN2", "SQLWARN3", "SQLWARN4", "SQLWARN5", "SQLWARN6", "SQLWARN7", "SQLWARN8", "SQLWARN9", "SQLWARNA"].into_iter().enumerate() {
            fields.push((name, None, mark(warnings[i])));
        }
        for (name, subscript, value) in fields {
            let subscripts = subscript.map(|n: u32| vec![Expr::Operand(Operand::Literal(Literal::Number(n.to_string())))]).unwrap_or_default();
            let Ok(loc) = self.locate(&Ref { name: name.into(), qualifiers: Vec::new(), subscripts, refmod: None, pos }) else { continue };
            let Ok(ty) = sql::host_type(self.layout, loc.item) else { continue };
            // The SQLCA is the program's own declaration: a field that cannot hold its value keeps
            // what it held.
            let _ = sql::write(&value, &mut self.unit.mem[loc.offset..loc.offset + loc.len], &ty, self.page);
        }
        Ok(())
    }

    /// The WHENEVER action in force for this outcome, tested SQLERROR, NOT FOUND, then SQLWARNING.
    fn whenever(&self, w: &Whenever, sqlcode: i32, warned: bool, pos: Pos) -> R<Flow> {
        let action = if sqlcode < 0 {
            &w.sqlerror
        } else if sqlcode == 100 {
            &w.not_found
        } else if warned || sqlcode > 0 {
            &w.sqlwarning
        } else {
            return Ok(Flow::Next);
        };
        match action {
            Action::Continue => Ok(Flow::Next),
            Action::GoTo(label) => Ok(Flow::GoTo(self.procedure(&ProcName { name: label.clone(), section: None }, pos)?.0)),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Execute;
    use crate::sql::{Abandoned, Answer, Call, Database, Outcome, Value};
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    /// Verb, ordinal, text and inputs of each call the database received.
    type Logged = (String, u32, String, Vec<Value>);
    type Calls = Rc<RefCell<Vec<Logged>>>;

    struct Script {
        answers: VecDeque<Outcome>,
        calls: Calls,
    }

    impl Script {
        fn answer(&mut self, c: &Call) -> Answer {
            self.calls.borrow_mut().push((c.verb.into(), c.ordinal, c.text.into(), c.inputs.to_vec()));
            Ok(self.answers.pop_front().unwrap_or_else(Outcome::ok))
        }
    }

    impl Database for Script {
        fn execute(&mut self, c: &Call) -> Answer {
            self.answer(c)
        }
        fn open(&mut self, c: &Call) -> Answer {
            self.answer(c)
        }
        fn fetch(&mut self, c: &Call) -> Answer {
            self.answer(c)
        }
        fn close(&mut self, c: &Call) -> Answer {
            self.answer(c)
        }
        fn commit(&mut self, c: &Call) -> Answer {
            self.answer(c)
        }
        fn rollback(&mut self, c: &Call) -> Answer {
            self.answer(c)
        }
        fn close_all(&mut self) -> Result<(), Abandoned> {
            self.calls.borrow_mut().push(("CLOSE ALL".into(), 0, String::new(), Vec::new()));
            Ok(())
        }
    }

    const DATA: &str = concat!(
        "       IDENTIFICATION DIVISION.\n",
        "       PROGRAM-ID. Q.\n",
        "       DATA DIVISION.\n",
        "       WORKING-STORAGE SECTION.\n",
        "           EXEC SQL INCLUDE SQLCA END-EXEC.\n",
        "       01 WS-ID    PIC S9(9) COMP VALUE 7.\n",
        "       01 WS-NAME  PIC X(10).\n",
        "       01 WS-AMT   PIC S9(5)V99 COMP-3 VALUE 0.\n",
        "       01 WS-IND   PIC S9(4) COMP VALUE 0.\n",
        "       01 WS-RAW   PIC X(4) VALUE 'ABCD'.\n",
        "       01 WS-BAD   REDEFINES WS-RAW PIC S9(5)V99 COMP-3.\n",
        "       01 E-AMT    PIC -9(5).99.\n",
        "       01 E-CODE   PIC -9(3).\n",
        "       01 E-IND    PIC -9(3).\n",
        "       PROCEDURE DIVISION.\n",
    );

    fn run(procedure: &str, answers: Vec<Outcome>) -> (Result<String, String>, Vec<Logged>) {
        let program = syntax::parse(&format!("{DATA}{procedure}")).expect("parses");
        let compiled = crate::compile(program, &[]).expect("compiles");
        let calls = Calls::default();
        let mut db = Script { answers: answers.into(), calls: calls.clone() };
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let ran = compiled.execute_with(crate::unit::Library::default(), crate::files::Dds::default(), None, crate::unit::Clock::System, Some(&mut db), &mut out, &mut err);
        let shown = ran.map(|_| String::from_utf8(out).expect("DISPLAY writes text")).map_err(|a| a.code.to_string());
        (shown, calls.take())
    }

    const SHOW: &str = concat!(
        "           MOVE WS-AMT TO E-AMT.\n",
        "           MOVE SQLCODE TO E-CODE.\n",
        "           MOVE WS-IND TO E-IND.\n",
        "           DISPLAY WS-NAME '|' E-AMT '|' E-CODE '|' E-IND '|'\n",
        "                   SQLWARN0 SQLWARN1 SQLWARN3.\n",
        "           GOBACK.\n",
    );

    fn select(into: &str) -> String {
        format!("           EXEC SQL SELECT NAME, AMT INTO {into}\n                    FROM T WHERE ID = :WS-ID END-EXEC.\n{SHOW}")
    }

    #[test]
    fn one_row_is_assigned_and_the_statement_is_sent_with_its_inputs() {
        let row = vec![Value::Char("SMITH".into()), Value::Decimal { value: 123_450, scale: 2 }];
        let (shown, calls) = run(&select(":WS-NAME, :WS-AMT:WS-IND"), vec![Outcome::rows(vec![row])]);
        assert_eq!(shown.as_deref(), Ok("SMITH     | 01234.50| 000| 000|   \n"));
        let select = ("SELECT".into(), 1, "SELECT NAME, AMT FROM T WHERE ID = ?".into(), vec![Value::Int(7)]);
        assert_eq!(calls, [select, ("COMMIT".into(), 0, "COMMIT".into(), Vec::new())]);
    }

    #[test]
    fn no_row_is_plus_100_and_two_rows_are_minus_811() {
        assert!(run(&select(":WS-NAME, :WS-AMT"), vec![Outcome::rows(Vec::new())]).0.unwrap().contains("| 100|"));
        let two = vec![vec![Value::Char("A".into()), Value::Int(1)], vec![Value::Char("B".into()), Value::Int(2)]];
        assert!(run(&select(":WS-NAME, :WS-AMT"), vec![Outcome::rows(two)]).0.unwrap().contains("|-811|"));
    }

    #[test]
    fn nulls_and_indicators() {
        let null_amount = vec![vec![Value::Char("SMITH".into()), Value::Null]];
        assert!(run(&select(":WS-NAME, :WS-AMT:WS-IND"), vec![Outcome::rows(null_amount.clone())]).0.unwrap().contains("| 000|-001|"));
        assert!(run(&select(":WS-NAME, :WS-AMT"), vec![Outcome::rows(null_amount)]).0.unwrap().contains("|-305|"));
    }

    #[test]
    fn a_cut_string_sets_sqlwarn1_and_its_indicator_holds_the_length() {
        let long = vec![vec![Value::Char("ABCDEFGHIJKL".into()), Value::Int(0)]];
        let shown = run(&select(":WS-NAME:WS-IND, :WS-AMT"), vec![Outcome::rows(long)]).0.unwrap();
        assert_eq!(shown, "ABCDEFGHIJ| 00000.00| 000| 012|WW \n");
    }

    /// As Db2 12.1 for Linux answers them (sql-runtime.md step 8).
    #[test]
    fn warnings_carry_their_sqlstate_and_a_fetch_counts_its_row() {
        let long = vec![vec![Value::Char("ABCDEFGHIJKL".into())]];
        let procedure = "           EXEC SQL SELECT NAME INTO :WS-NAME FROM T END-EXEC.\n           DISPLAY SQLSTATE.\n           GOBACK.\n";
        assert_eq!(run(procedure, vec![Outcome::rows(long)]).0.as_deref(), Ok("01004\n"));
        let extra = vec![vec![Value::Char("A".into()), Value::Int(1)]];
        assert_eq!(run(procedure, vec![Outcome::rows(extra)]).0.as_deref(), Ok("01503\n"));
        let procedure = [
            "           EXEC SQL DECLARE C1 CURSOR FOR SELECT NAME FROM T\n                    END-EXEC.\n",
            "           EXEC SQL OPEN C1 END-EXEC.\n",
            "           EXEC SQL FETCH C1 INTO :WS-NAME END-EXEC.\n",
            "           MOVE SQLERRD(3) TO E-CODE.\n           DISPLAY E-CODE.\n",
            "           EXEC SQL FETCH C1 INTO :WS-NAME END-EXEC.\n",
            "           MOVE SQLERRD(3) TO E-CODE.\n           DISPLAY E-CODE.\n",
            "           GOBACK.\n",
        ]
        .concat();
        let answers = vec![Outcome::ok(), Outcome::rows(vec![vec![Value::Char("X".into())]]), Outcome::rows(Vec::new())];
        assert_eq!(run(&procedure, answers).0.as_deref(), Ok(" 001\n 000\n"));
    }

    #[test]
    fn whenever_branches_in_listing_order() {
        let procedure = concat!(
            "           EXEC SQL WHENEVER NOT FOUND GO TO NONE END-EXEC.\n",
            "           EXEC SQL SELECT NAME INTO :WS-NAME FROM T END-EXEC.\n",
            "           DISPLAY 'FOUND'.\n",
            "           GOBACK.\n",
            "       NONE.\n",
            "           MOVE SQLCODE TO E-CODE.\n",
            "           DISPLAY 'NONE' E-CODE.\n",
            "           GOBACK.\n",
        );
        assert_eq!(run(procedure, vec![Outcome::rows(Vec::new())]).0.as_deref(), Ok("NONE 100\n"));
    }

    #[test]
    fn changes_report_rows_affected_and_errors_branch() {
        let procedure = concat!(
            "           EXEC SQL WHENEVER SQLERROR GO TO FAILED END-EXEC.\n",
            "           EXEC SQL INSERT INTO T (ID) VALUES (:WS-ID) END-EXEC.\n",
            "           MOVE SQLERRD(3) TO E-CODE.\n",
            "           DISPLAY 'ROWS' E-CODE.\n",
            "           EXEC SQL INSERT INTO T (ID) VALUES (:WS-ID) END-EXEC.\n",
            "           DISPLAY 'NOT REACHED'.\n",
            "       FAILED.\n",
            "           MOVE SQLCODE TO E-CODE.\n",
            "           DISPLAY 'FAILED' E-CODE SQLSTATE.\n",
            "           GOBACK.\n",
        );
        let answers = vec![Outcome { affected: 1, ..Outcome::ok() }, Outcome::error(-803, "23505")];
        let (shown, calls) = run(procedure, answers);
        assert_eq!(shown.as_deref(), Ok("ROWS 001\nFAILED-80323505\n"));
        assert_eq!(calls[0].2, "INSERT INTO T (ID) VALUES (?)");
    }

    #[test]
    fn invalid_packed_input_abends_s0c7() {
        let procedure = "           EXEC SQL SELECT NAME INTO :WS-NAME FROM T\n                    WHERE AMT = :WS-BAD END-EXEC.\n           GOBACK.\n";
        assert_eq!(run(procedure, Vec::new()).0, Err("S0C7".into()));
    }

    fn verbs(calls: &[Logged]) -> Vec<&str> {
        calls.iter().map(|c| c.0.as_str()).collect()
    }

    #[test]
    fn units_of_work_reach_the_database() {
        let (_, calls) = run("           EXEC SQL COMMIT END-EXEC.\n           EXEC SQL ROLLBACK WORK END-EXEC.\n           GOBACK.\n", Vec::new());
        assert_eq!(verbs(&calls), ["COMMIT", "ROLLBACK"]);
    }

    #[test]
    fn the_run_unit_commits_at_a_normal_end_and_rolls_back_at_an_abend() {
        let (_, calls) = run("           EXEC SQL DELETE FROM T END-EXEC.\n           GOBACK.\n", Vec::new());
        assert_eq!(calls.iter().map(|c| (c.0.as_str(), c.1)).collect::<Vec<_>>(), [("DELETE", 1), ("COMMIT", 0)]);
        let procedure = "           EXEC SQL DELETE FROM T END-EXEC.\n           EXEC SQL SELECT NAME INTO :WS-NAME FROM T\n                    WHERE AMT = :WS-BAD END-EXEC.\n";
        let (ended, calls) = run(procedure, Vec::new());
        assert_eq!((ended, verbs(&calls)), (Err("S0C7".into()), vec!["DELETE", "ROLLBACK"]));
    }

    const SHOW_CODE: &str = "           MOVE SQLCODE TO E-CODE.\n           DISPLAY E-CODE.\n";

    #[test]
    fn a_cursor_fetches_each_row_until_plus_100_with_inputs_read_at_open() {
        let procedure = concat!(
            "           EXEC SQL DECLARE C1 CURSOR FOR SELECT NAME, AMT FROM T\n",
            "                    WHERE ID > :WS-ID END-EXEC.\n",
            "           MOVE 3 TO WS-ID.\n",
            "           EXEC SQL OPEN C1 END-EXEC.\n",
            "           PERFORM UNTIL SQLCODE NOT = 0\n",
            "               EXEC SQL FETCH C1 INTO :WS-NAME, :WS-AMT END-EXEC\n",
            "               IF SQLCODE = 0\n",
            "                   MOVE WS-AMT TO E-AMT\n",
            "                   DISPLAY WS-NAME E-AMT\n",
            "               END-IF\n",
            "           END-PERFORM.\n",
            "           MOVE SQLCODE TO E-CODE.\n",
            "           DISPLAY 'END' E-CODE.\n",
            "           EXEC SQL CLOSE C1 END-EXEC.\n",
            "           GOBACK.\n",
        );
        let row = |name: &str, cents| Outcome::rows(vec![vec![Value::Char(name.into()), Value::Decimal { value: cents, scale: 2 }]]);
        let answers = vec![Outcome::ok(), row("ADAMS", 100), row("BAKER", 250), Outcome::rows(Vec::new())];
        let (shown, calls) = run(procedure, answers);
        assert_eq!(shown.as_deref(), Ok("ADAMS      00001.00\nBAKER      00002.50\nEND 100\n"));
        assert_eq!(verbs(&calls), ["OPEN", "FETCH", "FETCH", "FETCH", "CLOSE", "COMMIT"]);
        assert_eq!((calls[0].1, calls[0].2.as_str(), &calls[0].3), (2, "DECLARE C1 CURSOR FOR SELECT NAME, AMT FROM T WHERE ID > ?", &vec![Value::Int(3)]));
        assert_eq!(calls[1].2, "FETCH C1");
    }

    #[test]
    fn the_runtime_answers_for_cursor_state_and_commit_closes_all_but_held_cursors() {
        let procedure = [
            "           EXEC SQL DECLARE C1 CURSOR FOR SELECT NAME FROM T\n                    END-EXEC.\n",
            "           EXEC SQL DECLARE C2 CURSOR WITH HOLD FOR\n                    SELECT NAME FROM T END-EXEC.\n",
            "           EXEC SQL FETCH C1 INTO :WS-NAME END-EXEC.\n",
            SHOW_CODE,
            "           EXEC SQL CLOSE C1 END-EXEC.\n",
            SHOW_CODE,
            "           EXEC SQL OPEN C1 END-EXEC.\n",
            "           EXEC SQL OPEN C2 END-EXEC.\n",
            "           EXEC SQL OPEN C1 END-EXEC.\n",
            SHOW_CODE,
            "           EXEC SQL COMMIT END-EXEC.\n",
            "           EXEC SQL FETCH C1 INTO :WS-NAME END-EXEC.\n",
            SHOW_CODE,
            "           EXEC SQL FETCH C2 INTO :WS-NAME END-EXEC.\n",
            SHOW_CODE,
            "           GOBACK.\n",
        ]
        .concat();
        let answers = vec![Outcome::ok(), Outcome::ok(), Outcome::ok(), Outcome::rows(vec![vec![Value::Char("HELD".into())]])];
        let (shown, calls) = run(&procedure, answers);
        assert_eq!(shown.as_deref(), Ok("-501\n-501\n-502\n-501\n 000\n"));
        assert_eq!(verbs(&calls), ["OPEN", "OPEN", "COMMIT", "FETCH", "COMMIT"]);
    }

    #[test]
    fn a_positioned_delete_needs_an_open_cursor_on_a_row() {
        let delete = "           EXEC SQL DELETE FROM T WHERE CURRENT OF C1\n                    END-EXEC.\n";
        let procedure = [
            "           EXEC SQL DECLARE C1 CURSOR FOR SELECT NAME FROM T\n                    FOR UPDATE END-EXEC.\n",
            delete,
            SHOW_CODE,
            "           EXEC SQL OPEN C1 END-EXEC.\n",
            delete,
            SHOW_CODE,
            "           EXEC SQL FETCH C1 INTO :WS-NAME END-EXEC.\n",
            delete,
            SHOW_CODE,
            delete,
            SHOW_CODE,
            "           GOBACK.\n",
        ]
        .concat();
        let answers = vec![Outcome::ok(), Outcome::rows(vec![vec![Value::Char("X".into())]]), Outcome { affected: 1, ..Outcome::ok() }];
        let (shown, calls) = run(&procedure, answers);
        assert_eq!(shown.as_deref(), Ok("-507\n-508\n 000\n-508\n"));
        assert_eq!(verbs(&calls), ["OPEN", "FETCH", "DELETE", "COMMIT"]);
    }

    fn run_task(procedure: &str, answers: Vec<Outcome>) -> (String, Result<(), String>, Vec<Logged>) {
        let compiled = crate::compile(syntax::parse(&format!("{DATA}{procedure}")).expect("parses"), &[]).expect("compiles");
        let calls = Calls::default();
        let mut db = Script { answers: answers.into(), calls: calls.clone() };
        let task = crate::cics::Task { transid: "T1".into(), ..Default::default() };
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let ran = compiled.execute_cics_with(crate::unit::Library::default(), crate::files::Dds::default(), task, crate::unit::Clock::System, Some(&mut db), &mut out, &mut err);
        (String::from_utf8(out).expect("DISPLAY writes text"), ran.map(drop).map_err(|a| a.code.to_string()), calls.take())
    }

    #[test]
    fn a_cics_task_commits_at_syncpoint_and_at_its_end() {
        let delete = "           EXEC SQL DELETE FROM T END-EXEC.\n";
        let procedure = [
            delete,
            "           EXEC CICS SYNCPOINT END-EXEC.\n",
            "           EXEC CICS SYNCPOINT END-EXEC.\n",
            delete,
            "           EXEC CICS SYNCPOINT ROLLBACK END-EXEC.\n",
            delete,
            "           EXEC CICS RETURN END-EXEC.\n",
        ]
        .concat();
        let (_, ended, calls) = run_task(&procedure, Vec::new());
        assert_eq!(ended, Ok(()));
        let got: Vec<(&str, u32)> = calls.iter().map(|c| (c.0.as_str(), c.1)).collect();
        assert_eq!(got, [("DELETE", 1), ("COMMIT", 0), ("DELETE", 2), ("ROLLBACK", 0), ("DELETE", 3), ("COMMIT", 0)]);
    }

    #[test]
    fn a_refused_commit_at_syncpoint_raises_rolledback() {
        let refused = || vec![Outcome::ok(), Outcome::error(-911, "40001")];
        let procedure = concat!(
            "           EXEC SQL DELETE FROM T END-EXEC.\n",
            "           EXEC CICS SYNCPOINT RESP(WS-ID) END-EXEC.\n",
            "           MOVE WS-ID TO E-CODE.\n",
            "           DISPLAY E-CODE.\n",
            "           EXEC CICS RETURN END-EXEC.\n",
        );
        let (shown, ended, calls) = run_task(procedure, refused());
        assert_eq!((shown.as_str(), ended), (" 082\n", Ok(())));
        assert_eq!(verbs(&calls), ["DELETE", "COMMIT"]);
        let unhandled = "           EXEC SQL DELETE FROM T END-EXEC.\n           EXEC CICS SYNCPOINT END-EXEC.\n           EXEC CICS RETURN END-EXEC.\n";
        assert_eq!(run_task(unhandled, refused()).1, Err("AEXJ".into()));
    }

    #[test]
    fn tasks_share_a_database_and_each_ends_its_own_unit_of_work_and_cursors() {
        let procedure = [
            "           EXEC SQL DECLARE C1 CURSOR WITH HOLD FOR\n                    SELECT NAME FROM T END-EXEC.\n",
            "           EXEC SQL OPEN C1 END-EXEC.\n",
            SHOW_CODE,
            "           EXEC CICS RETURN END-EXEC.\n",
        ]
        .concat();
        let compiled = crate::compile(syntax::parse(&format!("{DATA}{procedure}")).expect("parses"), &[]).expect("compiles");
        let calls = Calls::default();
        let mut db = Script { answers: VecDeque::new(), calls: calls.clone() };
        for _ in 0..2 {
            let task = crate::cics::Task { transid: "T1".into(), ..Default::default() };
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let ran = compiled.execute_cics_with(crate::unit::Library::default(), crate::files::Dds::default(), task, crate::unit::Clock::System, Some(&mut db), &mut out, &mut err);
            assert_eq!((String::from_utf8(out).expect("DISPLAY writes text").as_str(), ran.is_ok()), (" 000\n", true));
        }
        assert_eq!(verbs(&calls.take()), ["OPEN", "COMMIT", "CLOSE ALL", "OPEN", "COMMIT", "CLOSE ALL"]);
    }

    #[test]
    fn a_cics_task_cannot_commit_through_sql_and_an_abend_backs_it_out() {
        let procedure = [
            "           EXEC SQL DELETE FROM T END-EXEC.\n",
            "           EXEC SQL COMMIT END-EXEC.\n",
            SHOW_CODE,
            "           EXEC SQL ROLLBACK END-EXEC.\n",
            SHOW_CODE,
            "           EXEC CICS ABEND ABCODE('XYZ1') END-EXEC.\n",
        ]
        .concat();
        let (shown, ended, calls) = run_task(&procedure, Vec::new());
        assert_eq!((shown.as_str(), ended), ("-925\n-926\n", Err("XYZ1".into())));
        assert_eq!(verbs(&calls), ["DELETE", "ROLLBACK"]);
    }

    #[test]
    fn a_cursor_declared_after_its_use_is_refused_when_compiled() {
        let procedure = "           EXEC SQL OPEN C1 END-EXEC.\n           EXEC SQL DECLARE C1 CURSOR FOR SELECT A FROM T\n                    END-EXEC.\n           GOBACK.\n";
        let errors = crate::compile(syntax::parse(&format!("{DATA}{procedure}")).expect("parses"), &[]).err().expect("refused");
        assert!(format!("{errors:?}").contains("EXEC SQL OPEN: cursor C1 is not declared before this statement"), "{errors:?}");
    }

    fn replayed(procedure: &str, recording: &str) -> Result<String, (String, String)> {
        let compiled = crate::compile(syntax::parse(&format!("{DATA}{procedure}")).expect("parses"), &[]).expect("compiles");
        let mut replay = crate::sql::Replay::parse(recording, false).expect("the recording parses");
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let ran = compiled.execute_with(crate::unit::Library::default(), crate::files::Dds::default(), None, crate::unit::Clock::System, Some(&mut replay), &mut out, &mut err);
        ran.map(|_| String::from_utf8(out).expect("DISPLAY writes text")).map_err(|a| (a.code.to_string(), a.message))
    }

    #[test]
    fn a_run_replays_from_a_recording_and_a_call_it_does_not_hold_abends_sqlr() {
        let hash = syntax::sql::fingerprint("SELECT NAME, AMT FROM T WHERE ID = ?");
        let recording = format!(
            "# ironwork sql recording 1\n@ 1 Q:1:{hash:08x} SELECT\n> int:7\n< 0 00000 rows=0\n= char:\"JONES\" | dec:42.10\n@ 2 Q:0:{:08x} COMMIT\n< 0 00000 rows=0\n",
            syntax::sql::fingerprint("COMMIT")
        );
        let shown = replayed(&select(":WS-NAME, :WS-AMT:WS-IND"), &recording);
        assert_eq!(shown.as_deref().map_err(|e| e.0.as_str()), Ok("JONES     | 00042.10| 000| 000|   \n"));
        let other = "           MOVE 8 TO WS-ID.\n".to_owned() + &select(":WS-NAME, :WS-AMT");
        let (code, message) = replayed(&other, &recording).unwrap_err();
        assert_eq!(code, "SQLR");
        assert!(message.contains(&format!("Q:1:{hash:08x} SELECT with int:7")) && message.ends_with("SELECT with int:8"), "{message}");
    }
}
