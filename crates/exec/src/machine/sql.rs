//! EXEC SQL in the walker: a block's host variables and SQLCA fields resolved into the
//! `SqlEntry` and `Sqlca` that `rt::sql::run` runs, and the WHENEVER branch in force taken.

use compile::facts::Facts;
use super::*;
use crate::sql::{self, HostType, Session, SqlHost};
use rt::host::Host;
use compile::sql::{sql_names, sqlca_fields};
use rt::lir::{AbendId, HostArray, HostPlace, RowCount, SqlEntry, SqlStatement, Sqlca, SqlcaField};
use syntax::sql::{Action, ChangeKind, HostVar, Rows, Statement, Whenever};

type Entry<'b> = SqlEntry<&'b Ref, String>;

/// The walker as one statement's run sees it, with the abends its host variables of no SQL type
/// give, by `AbendId`.
struct Bound<'m, 'p, 'u, 'w> {
    machine: &'m mut Machine<'p, 'u, 'w>,
    untyped: Vec<Abend>,
}

impl<'p, 'w> Machine<'p, '_, 'w> {
    pub(super) fn sql(&mut self, block: &'p ExecBlock) -> R<Flow> {
        let sql = block.sql.as_ref().expect("the parser types every EXEC SQL block");
        let mut untyped = Vec::new();
        let entry = self.sql_entry(block, &sql.statement, sql.ordinal, &mut untyped);
        let fields = sqlca_fields(block.pos);
        let sqlca = self.sqlca(&fields);
        let ran = sql::run(&mut Bound { machine: self, untyped }, &entry, &sqlca, block.pos)?;
        match ran {
            Some(ran) => self.whenever(&sql.whenever, ran.sqlcode, ran.warned, block.pos),
            None => Ok(Flow::Next),
        }
    }

    /// The statement with its host variables bound, and the text its database call sends.
    fn sql_entry<'b>(&mut self, block: &ExecBlock, statement: &'b Statement, ordinal: u32, untyped: &mut Vec<Abend>) -> Entry<'b> {
        let command = block.command.as_str();
        let mut places = |vars: &'b [HostVar]| self.host_places(vars, command, untyped);
        let (statement, text, with_hold) = match statement {
            Statement::Query { text, inputs, into } => (SqlStatement::Query { inputs: places(inputs), into: places(into) }, text.clone(), false),
            Statement::Change { kind, text, inputs, current_of } => {
                (SqlStatement::Change { delete: matches!(kind, ChangeKind::Delete), inputs: places(inputs), current_of: current_of.clone() }, text.clone(), false)
            }
            Statement::Open { cursor, declared, using, descriptor } => {
                let declared = declared.as_ref().expect("the parser gives OPEN its DECLARE");
                let hold = if declared.with_hold { " WITH HOLD" } else { "" };
                match (&declared.statement, descriptor) {
                    (Some(name), Some(d)) => {
                        let text = format!("DECLARE {cursor} CURSOR{hold} FOR {name}");
                        (SqlStatement::OpenDescriptor { cursor: cursor.clone(), statement: name.clone(), descriptor: &d.var }, text, declared.with_hold)
                    }
                    (Some(name), None) => {
                        let text = format!("DECLARE {cursor} CURSOR{hold} FOR {name}");
                        (SqlStatement::OpenPrepared { cursor: cursor.clone(), statement: name.clone(), inputs: places(using) }, text, declared.with_hold)
                    }
                    (None, _) => {
                        let text = format!("DECLARE {cursor} CURSOR{hold} FOR {}", declared.text);
                        (SqlStatement::Open { cursor: cursor.clone(), inputs: places(&declared.inputs) }, text, declared.with_hold)
                    }
                }
            }
            Statement::Fetch { cursor, into } => (SqlStatement::Fetch { cursor: cursor.clone(), into: places(into) }, format!("FETCH {cursor}"), false),
            Statement::FetchDescriptor { cursor, descriptor } => (SqlStatement::FetchDescriptor { cursor: cursor.clone(), descriptor: &descriptor.var }, format!("FETCH {cursor}"), false),
            Statement::FetchRowset { cursor, rows, into, enabled } => {
                let (rows, into) = (self.row_count(rows, command, untyped), self.host_arrays(into, command, true, untyped));
                (SqlStatement::FetchRowset { cursor: cursor.clone(), rows, into, enabled: *enabled }, format!("FETCH NEXT ROWSET FROM {cursor} FOR ? ROWS"), false)
            }
            Statement::InsertRows { text, inputs, rows, atomic } => {
                let (rows, inputs) = (self.row_count(rows, command, untyped), self.host_arrays(inputs, command, false, untyped));
                (SqlStatement::InsertRows { inputs, rows, atomic: *atomic }, text.clone(), false)
            }
            Statement::Call { procedure, text, args } => (SqlStatement::Call { procedure: procedure.clone(), args: self.host_places(args, command, untyped) }, text.clone(), false),
            Statement::Close { cursor } => (SqlStatement::Close { cursor: cursor.clone() }, format!("CLOSE {cursor}"), false),
            Statement::Commit => (SqlStatement::Commit, "COMMIT".into(), false),
            Statement::Rollback => (SqlStatement::Rollback, "ROLLBACK".into(), false),
            Statement::Prepare { name, source, into } => {
                let source = places(std::slice::from_ref(source));
                let statement = match into {
                    Some((d, names)) => SqlStatement::PrepareInto { name: name.clone(), source, descriptor: &d.var, names: sql_names(*names) },
                    None => SqlStatement::Prepare { name: name.clone(), source },
                };
                (statement, format!("PREPARE {name}"), false)
            }
            Statement::ExecuteImmediate { source } => (SqlStatement::ExecuteImmediate { source: places(std::slice::from_ref(source)) }, "EXECUTE IMMEDIATE".into(), false),
            Statement::Execute { name, inputs, descriptor } => {
                let statement = match descriptor {
                    Some(d) => SqlStatement::ExecuteDescriptor { name: name.clone(), descriptor: &d.var },
                    None => SqlStatement::Execute { name: name.clone(), inputs: places(inputs) },
                };
                (statement, format!("EXECUTE {name}"), false)
            }
            Statement::Describe { name, descriptor, names } => (SqlStatement::Describe { name: name.clone(), descriptor: &descriptor.var, names: sql_names(*names) }, format!("DESCRIBE {name}"), false),
            Statement::Whenever { .. } | Statement::Declaration | Statement::DeclareCursor(_) | Statement::DeclareUnsupported { .. } => (SqlStatement::Declaration, String::new(), false),
            Statement::Unsupported(what) => (SqlStatement::Unsupported(what.clone()), String::new(), false),
            Statement::Connect { what, target } => (SqlStatement::Connect { what: what.clone(), location: places(target.as_slice()) }, String::new(), false),
            Statement::Malformed(why) => unreachable!("the compiler refuses a malformed statement: {why}"),
        };
        SqlEntry { ordinal, verb: command.to_owned(), statement, fingerprint: sql::fingerprint(&text), text, with_hold }
    }

    /// Each host variable's type, a host structure's members one by one with the indicator
    /// array's elements beside them. An item with no SQL type abends EXEC when the statement
    /// reaches it.
    fn host_places<'b>(&mut self, vars: &'b [HostVar], command: &str, untyped: &mut Vec<Abend>) -> Vec<HostPlace<&'b Ref>> {
        let mut out = Vec::new();
        for hv in vars {
            let indicator = |element: usize| hv.indicator.as_ref().map(|r| (r, 2 * element as u32));
            let ty = match self.resolve(&hv.var) {
                Ok(Resolved::Item(item)) => sql::host_type(self.layout, item)
                    .map(|ty| (item, ty))
                    .map_err(|why| Abend { code: "EXEC".into(), message: format!("EXEC SQL {command}: {why}"), pos: hv.var.pos, file: None }),
                Ok(_) => Err(Abend::ironwork(format!("{} is a condition-name, not a data item", hv.var.name), hv.var.pos)),
                Err(abend) => Err(abend),
            };
            match ty {
                Ok((item, HostType::Structure(members))) => {
                    let start = self.layout.items[item].offset;
                    for (i, (m, ty)) in members.into_iter().enumerate() {
                        let member = &self.layout.items[m];
                        out.push(HostPlace { var: &hv.var, member: Some((member.offset - start, member.size)), ty: Ok(ty), indicator: indicator(i) });
                    }
                }
                Ok((_, ty)) => out.push(HostPlace { var: &hv.var, member: None, ty: Ok(ty), indicator: indicator(0) }),
                Err(abend) => {
                    untyped.push(abend);
                    out.push(HostPlace { var: &hv.var, member: None, ty: Err((untyped.len() - 1) as AbendId), indicator: indicator(0) });
                }
            }
        }
        out
    }

    /// A multiple-row statement's host variables, each host-variable array with its dimension. A
    /// rowset FETCH's INTO takes only arrays; anything else abends EXEC when the statement reaches it.
    fn host_arrays<'b>(&mut self, vars: &'b [HostVar], command: &str, arrays_only: bool, untyped: &mut Vec<Abend>) -> Vec<HostArray<&'b Ref>> {
        let mut out = Vec::new();
        for hv in vars {
            let item = |m: &mut Self, r: &Ref| match m.resolve(r) {
                Ok(Resolved::Item(i)) => Some(i),
                _ => None,
            };
            let Some(var) = item(self, &hv.var) else {
                out.extend(self.host_places(std::slice::from_ref(hv), command, untyped).into_iter().map(|place| HostArray { place, array: None }));
                continue;
            };
            let indicator = hv.indicator.as_ref().and_then(|r| item(self, r).map(|i| (i, !r.subscripts.is_empty())));
            let refused = |why: String| Abend { code: "EXEC".into(), message: format!("EXEC SQL {command}: {why}"), pos: hv.var.pos, file: None };
            let array = match sql::host_array(self.layout, var, !hv.var.subscripts.is_empty(), indicator) {
                Ok(None) if arrays_only => Err(refused(format!("{} is not a host-variable array, which a rowset FETCH's INTO takes", hv.var.name))),
                Ok(None) => {
                    out.extend(self.host_places(std::slice::from_ref(hv), command, untyped).into_iter().map(|place| HostArray { place, array: None }));
                    continue;
                }
                Ok(Some(d)) => sql::host_type(self.layout, var).map(|ty| (ty, d)).map_err(refused),
                Err(why) => Err(refused(why)),
            };
            let indicator = hv.indicator.as_ref().map(|r| (r, 0));
            out.push(match array {
                Ok((ty, d)) => HostArray { place: HostPlace { var: &hv.var, member: None, ty: Ok(ty), indicator }, array: Some(d) },
                Err(abend) => {
                    untyped.push(abend);
                    HostArray { place: HostPlace { var: &hv.var, member: None, ty: Err((untyped.len() - 1) as AbendId), indicator }, array: None }
                }
            });
        }
        out
    }

    fn row_count<'b>(&mut self, rows: &'b Rows, command: &str, untyped: &mut Vec<Abend>) -> RowCount<&'b Ref> {
        match rows {
            Rows::Implicit => RowCount::Implicit,
            Rows::Constant(n) => RowCount::Constant(*n),
            Rows::Host(h) => RowCount::Host(self.host_places(std::slice::from_ref(h.as_ref()), command, untyped).remove(0)),
        }
    }

    /// The SQLCA fields the program declares with an SQL type, or its standalone SQLCODE and
    /// SQLSTATE.
    fn sqlca<'b>(&mut self, fields: &'b [(SqlcaField, Ref)]) -> Sqlca<&'b Ref> {
        let typed = |(field, r): &'b (SqlcaField, Ref)| match self.resolve(r) {
            Ok(Resolved::Item(item)) => sql::host_type(self.layout, item).ok().map(|ty| (*field, r, ty)),
            _ => None,
        };
        Sqlca { fields: fields.iter().filter_map(typed).collect() }
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
            Action::GoTo(label) => {
                let label = ProcName { name: label.clone(), section: None };
                Ok(Flow::GoTo(crate::procedure_from(self.program, &label, self.returns.running).map_err(|m| Abend::ironwork(m, pos))?.0))
            }
        }
    }
}

impl<'a, 'p> Host<&'a Ref> for Bound<'_, 'p, '_, '_> {
    type Facts = Facts<'p>;

    fn facts(&self) -> Facts<'p> {
        self.machine.facts()
    }

    fn mem(&mut self) -> &mut [u8] {
        &mut self.machine.unit.mem
    }

    fn taint(&mut self) -> Option<&mut rt::taint::Taint> {
        self.machine.unit.taint.as_mut()
    }

    fn locate(&mut self, place: &'a Ref, receiving: bool) -> R<Loc> {
        self.machine.locate_as(place, receiving)
    }

    fn integer(&mut self, place: &'a Ref, pos: Pos) -> R<i64> {
        Host::integer(self.machine, place, pos)
    }

    fn assign(&mut self, dest: Loc, val: Val, src: Option<Loc>, pos: Pos) -> R<()> {
        Host::assign(self.machine, dest, val, src, pos)
    }

    fn store_fixed(&mut self, dest: Loc, value: &Fixed, pos: Pos) -> R<()> {
        Host::store_fixed(self.machine, dest, value, pos)
    }
}

impl<'a, 'w> SqlHost<'w, &'a Ref, String> for Bound<'_, '_, '_, 'w> {
    fn session(&mut self) -> Option<&mut Session<'w>> {
        self.machine.unit.sql.as_mut()
    }

    fn in_task(&self) -> bool {
        self.machine.unit.cics.is_some()
    }

    fn program_id(&self) -> String {
        self.machine.program.id.clone()
    }

    fn text(&self, text: &String) -> String {
        text.clone()
    }

    fn place_pos(&self, place: &'a Ref) -> Pos {
        place.pos
    }

    fn untyped(&mut self, abend: AbendId) -> Abend {
        self.untyped[abend as usize].clone()
    }

    fn sink(&mut self, kind: &'static str, pos: Pos, operand: &str) {
        self.machine.sink(kind, pos, operand);
    }

    /// A table named without subscripts at its first element: an indicator array, as `:CLS:CLS-IND`
    /// names one (Db2 13 for z/OS, SSEPEK_13.0.0 apsg db2z_indicatorvariablecobol), or a
    /// host-variable array.
    fn locate_first(&mut self, place: &'a Ref) -> R<Loc> {
        let m = &mut *self.machine;
        let dims = match m.resolve(place) {
            Ok(Resolved::Item(i)) if place.subscripts.is_empty() => m.layout.items[i].dims.len(),
            _ => 0,
        };
        if dims == 0 {
            return m.locate_as(place, false);
        }
        let one = Expr::Operand(Operand::Literal(Literal::Number("1".into())));
        m.locate_as(&Ref { subscripts: vec![one; dims], ..place.clone() }, false)
    }
}

#[cfg(test)]
mod tests {
    use crate::Execute;
    use crate::sql::{Abandoned, Answer, Call, Database, Outcome, Value};
    use crate::testing::{Executor, Harness};
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
        fn prepare(&mut self, c: &Call) -> Answer {
            self.answer(c)
        }
        fn open(&mut self, c: &Call) -> Answer {
            self.answer(c)
        }
        fn fetch(&mut self, c: &Call) -> Answer {
            self.answer(c)
        }
        fn fetch_rows(&mut self, c: &Call, _: u32) -> Answer {
            self.answer(c)
        }
        /// Logged under INSERT ATOMIC or INSERT NOT ATOMIC.
        fn insert_rows(&mut self, c: &Call, _: &[Vec<Value>], atomic: bool) -> Answer {
            self.answer(&Call { verb: if atomic { "INSERT ATOMIC" } else { "INSERT NOT ATOMIC" }, ..*c })
        }
        fn call(&mut self, c: &Call) -> Answer {
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

    /// Runs `source` under the test harness, each executor's run with a database answering
    /// `answers`, and in a CICS task with `task`; the two runs must agree.
    fn both(source: &str, answers: &[Outcome], task: bool) {
        let answers = answers.to_vec();
        let mut harness = Harness::source(source).database(move || Box::new(Script { answers: answers.clone().into(), calls: Calls::default() }));
        if task {
            harness = harness.task(crate::cics::Task { transid: "T1".into(), ..Default::default() });
        }
        harness.run(Executor::Interpreter);
    }

    fn run(procedure: &str, answers: Vec<Outcome>) -> (Result<String, String>, Vec<Logged>) {
        run_source(&format!("{DATA}{procedure}"), answers)
    }

    /// The walker's run of a whole program, after both executors' runs agree, and the calls its
    /// database received; the abend's code where it abends.
    fn run_source(source: &str, answers: Vec<Outcome>) -> (Result<String, String>, Vec<Logged>) {
        let (shown, calls) = run_source_message(source, answers);
        (shown.map_err(|e| e.0), calls)
    }

    fn run_source_message(source: &str, answers: Vec<Outcome>) -> (Result<String, (String, String)>, Vec<Logged>) {
        both(source, &answers, false);
        let program = syntax::parse(source).expect("parses");
        let compiled = crate::compile(program, &[]).expect("compiles");
        crate::testing::check_lowering(&compiled, rt::sql::fingerprint(source), None);
        let calls = Calls::default();
        let mut db = Script { answers: answers.into(), calls: calls.clone() };
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let ran = compiled.execute_with(crate::unit::Library::default(), crate::files::Dds::default(), None, crate::unit::Clock::System, Some(&mut db), &mut out, &mut err);
        let shown = ran.map(|_| String::from_utf8(out).expect("DISPLAY writes text")).map_err(|a| (a.code.to_string(), a.message));
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
    fn a_whenever_label_two_sections_have_is_the_one_in_the_statement_s_section() {
        let procedure = concat!(
            "       S1 SECTION.\n",
            "           PERFORM S2.\n",
            "           GOBACK.\n",
            "       NONE.\n",
            "           DISPLAY 'NONE OF S1'.\n",
            "       S2 SECTION.\n",
            "           EXEC SQL WHENEVER NOT FOUND GO TO NONE END-EXEC.\n",
            "           EXEC SQL SELECT NAME INTO :WS-NAME FROM T END-EXEC.\n",
            "       NONE.\n",
            "           DISPLAY 'NONE OF S2'.\n",
        );
        assert_eq!(run(procedure, vec![Outcome::rows(Vec::new())]).0.as_deref(), Ok("NONE OF S2\n"));
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

    #[test]
    fn a_host_structure_takes_a_column_per_member_and_its_indicator_array_an_element_each() {
        let source = concat!(
            "       IDENTIFICATION DIVISION.\n",
            "       PROGRAM-ID. Q.\n",
            "       DATA DIVISION.\n",
            "       WORKING-STORAGE SECTION.\n",
            "       01 WS-ROW.\n",
            "          05 R-NAME PIC X(5).\n",
            "          05 R-ID   PIC S9(9) COMP.\n",
            "          05 R-AMT  PIC S9(5)V99 COMP-3 VALUE 1.\n",
            "       01 WS-INDS.\n",
            "          05 WS-IND PIC S9(4) COMP OCCURS 3.\n",
            "       01 E-NUM    PIC -9(3).\n",
            "       PROCEDURE DIVISION.\n",
            "           EXEC SQL SELECT NAME, ID, AMT INTO :WS-ROW:WS-INDS FROM T\n",
            "                    END-EXEC.\n",
            "           MOVE R-ID TO E-NUM.\n",
            "           DISPLAY R-NAME E-NUM WITH NO ADVANCING.\n",
            "           MOVE WS-IND(3) TO E-NUM.\n",
            "           DISPLAY E-NUM.\n",
            "           EXEC SQL INSERT INTO T VALUES (:WS-ROW) END-EXEC.\n",
            "           GOBACK.\n",
        );
        let compiled = crate::compile(syntax::parse(source).expect("parses"), &[]).expect("compiles");
        crate::testing::check_lowering(&compiled, rt::sql::fingerprint(source), None);
        let calls = Calls::default();
        let row = vec![Value::Char("ADAMS".into()), Value::Int(42), Value::Null];
        both(source, &[Outcome::rows(vec![row.clone()])], false);
        let mut db = Script { answers: vec![Outcome::rows(vec![row])].into(), calls: calls.clone() };
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let ran = compiled.execute_with(crate::unit::Library::default(), crate::files::Dds::default(), None, crate::unit::Clock::System, Some(&mut db), &mut out, &mut err);
        assert!(ran.is_ok(), "{ran:?}");
        assert_eq!(String::from_utf8(out).expect("DISPLAY writes text"), "ADAMS 042-001\n");
        let sent = Vec::from([Value::Char("ADAMS".into()), Value::Int(42), Value::Decimal { value: 100, scale: 2 }]);
        assert_eq!(calls.take()[1].3, sent);
    }

    #[test]
    fn an_indicator_array_named_without_subscripts_gives_each_member_an_element_in_both_executors() {
        for indicator in [":H:IND", ":H:I.IND"] {
            let source = [
                "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. Q.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n",
                "           EXEC SQL INCLUDE SQLCA END-EXEC.\n",
                "       01 H.\n          10 A PIC X(3).\n          10 B PIC X(3) VALUE '---'.\n",
                "       01 I.\n          10 IND PIC S9(4) COMP OCCURS 2 VALUE 7.\n",
                "       01 E-NUM PIC -9(3).\n",
                "       PROCEDURE DIVISION.\n",
                &format!("           EXEC SQL SELECT A, B INTO {indicator} FROM T END-EXEC.\n"),
                "           MOVE IND(1) TO E-NUM.\n           DISPLAY A '|' B '|' E-NUM WITH NO ADVANCING.\n",
                "           MOVE IND(2) TO E-NUM.\n           DISPLAY '|' E-NUM.\n",
                "           GOBACK.\n",
            ]
            .concat();
            let compiled = crate::compile(syntax::parse(&source).expect("parses"), &[]).expect("compiles");
            assert!(crate::testing::check_lowering(&compiled, rt::sql::fingerprint(&source), None).is_some(), "{indicator}");
            let row = vec![Value::Char("XY".into()), Value::Null];
            both(&source, &[Outcome::rows(vec![row.clone()])], false);
            let mut db = Script { answers: vec![Outcome::rows(vec![row])].into(), calls: Calls::default() };
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let ran = compiled.execute_with(crate::unit::Library::default(), crate::files::Dds::default(), None, crate::unit::Clock::System, Some(&mut db), &mut out, &mut err);
            assert!(ran.is_ok(), "{indicator}: {ran:?}");
            assert_eq!(String::from_utf8(out).expect("DISPLAY writes text"), "XY |---| 000|-001\n", "{indicator}");
        }
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
        run_task_source(&format!("{DATA}{procedure}"), answers)
    }

    fn run_task_source(source: &str, answers: Vec<Outcome>) -> (String, Result<(), String>, Vec<Logged>) {
        both(source, &answers, true);
        let compiled = crate::compile(syntax::parse(source).expect("parses"), &[]).expect("compiles");
        crate::testing::check_lowering(&compiled, rt::sql::fingerprint(source), None);
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
        both(&format!("{DATA}{procedure}"), &[], true);
        let compiled = crate::compile(syntax::parse(&format!("{DATA}{procedure}")).expect("parses"), &[]).expect("compiles");
        crate::testing::check_lowering(&compiled, rt::sql::fingerprint(&procedure), None);
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
        let text = recording.to_owned();
        Harness::source(&format!("{DATA}{procedure}")).database(move || Box::new(crate::sql::Replay::parse(&text, false).expect("the recording parses"))).run(Executor::Interpreter);
        let compiled = crate::compile(syntax::parse(&format!("{DATA}{procedure}")).expect("parses"), &[]).expect("compiles");
        crate::testing::check_lowering(&compiled, rt::sql::fingerprint(procedure), None);
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

    /// Dynamic SQL: statement strings in a VARCHAR, prepared statements and the cursors for them.
    mod dynamic {
        use super::*;

        const DYN: &str = concat!(
            "       IDENTIFICATION DIVISION.\n",
            "       PROGRAM-ID. D.\n",
            "       DATA DIVISION.\n",
            "       WORKING-STORAGE SECTION.\n",
            "           EXEC SQL INCLUDE SQLCA END-EXEC.\n",
            "       01 STMT.\n",
            "          49 STMT-LEN  PIC S9(4) COMP.\n",
            "          49 STMT-TEXT PIC X(120).\n",
            "       01 FIXED-STMT PIC X(40) VALUE 'DELETE FROM T'.\n",
            "       01 WS-ID    PIC S9(9) COMP VALUE 7.\n",
            "       01 WS-NAME  PIC X(10).\n",
            "       01 WS-RAW   PIC X(4) VALUE 'ABCD'.\n",
            "       01 WS-BAD   REDEFINES WS-RAW PIC S9(5)V99 COMP-3.\n",
            "       01 E-CODE   PIC -9(3).\n",
            "       PROCEDURE DIVISION.\n",
        );

        /// Lines that put `text` in STMT.
        fn set(text: &str) -> String {
            let mut lines = String::from("           MOVE SPACES TO STMT-TEXT.\n");
            for (k, chunk) in text.as_bytes().chunks(30).enumerate() {
                let chunk = std::str::from_utf8(chunk).expect("ASCII");
                lines += &format!("           MOVE '{}' TO STMT-TEXT({}:{}).\n", chunk.replace('\'', "''"), 30 * k + 1, chunk.len());
            }
            lines + &format!("           MOVE {} TO STMT-LEN.\n", text.len())
        }

        /// The statement, then its SQLCODE displayed.
        fn exec(sql: &str) -> String {
            format!("           EXEC SQL {sql} END-EXEC.\n           MOVE SQLCODE TO E-CODE.\n           DISPLAY E-CODE.\n")
        }

        fn source(procedure: &str) -> String {
            format!("{DYN}{procedure}           GOBACK.\n")
        }

        fn dynamic(procedure: &str, answers: Vec<Outcome>) -> (Result<String, String>, Vec<Logged>) {
            run_source(&source(procedure), answers)
        }

        #[test]
        fn execute_immediate_sends_the_statement_string_under_its_own_verb() {
            let procedure = [set("UPDATE T   SET A = 1   WHERE B = 2"), exec("EXECUTE IMMEDIATE :STMT"), exec("EXECUTE IMMEDIATE :STMT")].concat();
            let (shown, calls) = dynamic(&procedure, vec![Outcome { affected: 1, ..Outcome::ok() }, Outcome::ok()]);
            assert_eq!(shown.as_deref(), Ok(" 000\n 100\n"));
            let update = |ordinal| ("UPDATE".to_owned(), ordinal, "UPDATE T SET A = 1 WHERE B = 2".to_owned(), Vec::new());
            assert_eq!(calls, [update(1), update(2), ("COMMIT".into(), 0, "COMMIT".into(), Vec::new())]);
        }

        #[test]
        fn a_prepared_statement_runs_with_its_using_list_in_place_of_its_markers() {
            let procedure = [
                set("INSERT INTO T (ID, NAME) VALUES (?, ?)"),
                exec("PREPARE S1 FROM :STMT"),
                "           MOVE 'SMITH' TO WS-NAME.\n".into(),
                exec("EXECUTE S1 USING :WS-ID, :WS-NAME"),
                exec("EXECUTE S1 USING :WS-ID"),
                exec("EXECUTE S1"),
            ]
            .concat();
            let (shown, calls) = dynamic(&procedure, vec![Outcome::ok(), Outcome { affected: 1, ..Outcome::ok() }]);
            assert_eq!(shown.as_deref(), Ok(" 000\n 000\n-313\n-313\n"));
            assert_eq!(verbs(&calls), ["PREPARE", "INSERT", "COMMIT"]);
            assert_eq!((calls[0].1, calls[0].2.as_str()), (1, "INSERT INTO T (ID, NAME) VALUES (?, ?)"));
            assert_eq!((calls[1].1, calls[1].2.as_str(), calls[1].3.len(), &calls[1].3[0]), (2, "INSERT INTO T (ID, NAME) VALUES (?, ?)", 2, &Value::Int(7)));
        }

        #[test]
        fn using_is_not_read_for_a_statement_without_markers() {
            let procedure = [set("DELETE FROM T"), exec("PREPARE S2 FROM :STMT"), exec("EXECUTE S2 USING :WS-BAD")].concat();
            let (shown, calls) = dynamic(&procedure, Vec::new());
            assert_eq!(shown.as_deref(), Ok(" 000\n 100\n"));
            assert_eq!(calls[1], ("DELETE".into(), 2, "DELETE FROM T".into(), Vec::new()));
        }

        #[test]
        fn what_execute_and_execute_immediate_refuse_never_reaches_the_database() {
            let procedure = [
                exec("EXECUTE S9"),
                set("SELECT A FROM T"),
                exec("EXECUTE IMMEDIATE :STMT"),
                exec("PREPARE S1 FROM :STMT"),
                exec("EXECUTE S1"),
                set("CONNECT TO LOC1"),
                exec("EXECUTE IMMEDIATE :STMT"),
                exec("PREPARE S3 FROM :STMT"),
                exec("EXECUTE S3"),
            ]
            .concat();
            let (shown, calls) = dynamic(&procedure, Vec::new());
            assert_eq!(shown.as_deref(), Ok("-518\n-518\n 000\n-518\n-084\n-084\n-518\n"));
            assert_eq!(verbs(&calls), ["PREPARE", "COMMIT"]);
        }

        #[test]
        fn a_cursor_for_a_prepared_select_opens_with_its_using_list() {
            let procedure = [
                "           EXEC SQL DECLARE C1 CURSOR FOR S1 END-EXEC.\n".into(),
                exec("OPEN C1 USING :WS-ID"),
                set("SELECT NAME FROM T WHERE ID = ?"),
                exec("PREPARE S1 FROM :STMT"),
                exec("OPEN C1"),
                exec("OPEN C1 USING :WS-ID"),
                exec("PREPARE S1 FROM :STMT"),
                exec("FETCH C1 INTO :WS-NAME"),
                "           DISPLAY WS-NAME.\n".into(),
                exec("CLOSE C1"),
                set("DELETE FROM T"),
                exec("PREPARE S1 FROM :STMT"),
                exec("OPEN C1 USING :WS-ID"),
            ]
            .concat();
            let answers = vec![Outcome::ok(), Outcome::ok(), Outcome::rows(vec![vec![Value::Char("JONES".into())]])];
            let (shown, calls) = dynamic(&procedure, answers);
            assert_eq!(shown.as_deref(), Ok("-514\n 000\n-313\n 000\n-519\n 000\nJONES     \n 000\n 000\n-517\n"));
            assert_eq!(verbs(&calls), ["PREPARE", "OPEN", "FETCH", "CLOSE", "PREPARE", "COMMIT"]);
            assert_eq!((calls[1].2.as_str(), calls[1].3.as_slice()), ("DECLARE C1 CURSOR FOR SELECT NAME FROM T WHERE ID = ?", [Value::Int(7)].as_slice()));
        }

        #[test]
        fn a_unit_of_work_destroys_its_prepared_statements_but_an_open_held_cursor_s() {
            let procedure = [
                "           EXEC SQL DECLARE H1 CURSOR WITH HOLD FOR S1 END-EXEC.\n".into(),
                set("SELECT NAME FROM T WHERE ID = ?"),
                exec("PREPARE S1 FROM :STMT"),
                set("DELETE FROM T"),
                exec("PREPARE S2 FROM :STMT"),
                exec("OPEN H1 USING :WS-ID"),
                exec("COMMIT"),
                exec("EXECUTE S2"),
                exec("CLOSE H1"),
                exec("OPEN H1 USING :WS-ID"),
                exec("CLOSE H1"),
                exec("ROLLBACK"),
                exec("OPEN H1 USING :WS-ID"),
            ]
            .concat();
            let (shown, calls) = dynamic(&procedure, Vec::new());
            assert_eq!(shown.as_deref(), Ok(" 000\n 000\n 000\n 000\n-518\n 000\n 000\n 000\n 000\n-514\n"));
            assert_eq!(verbs(&calls), ["PREPARE", "PREPARE", "OPEN", "COMMIT", "CLOSE", "OPEN", "CLOSE", "ROLLBACK"]);
            assert_eq!(calls[2].2, "DECLARE H1 CURSOR WITH HOLD FOR SELECT NAME FROM T WHERE ID = ?");
        }

        #[test]
        fn a_dynamic_commit_ends_the_unit_of_work_except_in_a_cics_task() {
            let procedure = [set("COMMIT WORK"), exec("EXECUTE IMMEDIATE :STMT")].concat();
            let (shown, calls) = dynamic(&procedure, Vec::new());
            assert_eq!((shown.as_deref(), calls), (Ok(" 000\n"), vec![("COMMIT".into(), 1, "COMMIT WORK".into(), Vec::new())]));
            let (shown, ended, calls) = run_task_source(&source(&procedure), Vec::new());
            assert_eq!((shown.as_str(), ended, verbs(&calls)), ("-925\n", Ok(()), Vec::<&str>::new()));
        }

        #[test]
        fn a_fixed_length_statement_string_and_a_savepoint_are_refused_by_name() {
            let (shown, _) = run_source_message(&source(&exec("EXECUTE IMMEDIATE :FIXED-STMT")), Vec::new());
            let (code, message) = shown.unwrap_err();
            assert!(code == "EXEC" && message.contains("varying-length"), "{code} {message}");
            let procedure = [set("SAVEPOINT A ON ROLLBACK RETAIN CURSORS"), exec("EXECUTE IMMEDIATE :STMT")].concat();
            let (code, message) = run_source_message(&source(&procedure), Vec::new()).0.unwrap_err();
            assert!(code == "EXEC" && message.contains("does not run SAVEPOINT"), "{code} {message}");
        }

        #[test]
        fn a_recording_answers_prepare_with_the_statement_name_after_its_verb() {
            let insert = "INSERT INTO T (ID) VALUES (?)";
            let procedure = [set(insert), exec("PREPARE INS-1 FROM :STMT"), exec("EXECUTE INS-1 USING :WS-ID")].concat();
            let hash = rt::sql::fingerprint(insert);
            let commit = rt::sql::fingerprint("COMMIT");
            let recording = format!(
                "# ironwork sql recording 1\n@ 1 D:1:{hash:08x} PREPARE INS-1\n< 0 00000 rows=0\n@ 2 D:2:{hash:08x} INSERT\n> int:7\n< 0 00000 rows=1\n@ 3 D:0:{commit:08x} COMMIT\n< 0 00000 rows=0\n"
            );
            let text = recording.clone();
            let source = source(&procedure);
            Harness::source(&source).database(move || Box::new(crate::sql::Replay::parse(&text, false).expect("the recording parses"))).run(Executor::Interpreter);
            let compiled = crate::compile(syntax::parse(&source).expect("parses"), &[]).expect("compiles");
            let mut replay = crate::sql::Replay::parse(&recording, false).expect("the recording parses");
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let ran = compiled.execute_with(crate::unit::Library::default(), crate::files::Dds::default(), None, crate::unit::Clock::System, Some(&mut replay), &mut out, &mut err);
            assert_eq!(ran.map(|_| String::from_utf8(out).expect("text")).map_err(|a| a.message).as_deref(), Ok(" 000\n 000\n"));
        }

        const DA: &str = concat!(
            "       IDENTIFICATION DIVISION.\n",
            "       PROGRAM-ID. DA.\n",
            "       DATA DIVISION.\n",
            "       WORKING-STORAGE SECTION.\n",
            "           EXEC SQL INCLUDE SQLCA END-EXEC.\n",
            "           EXEC SQL INCLUDE SQLDA END-EXEC.\n",
            "       01 STMT.\n",
            "          49 STMT-LEN  PIC S9(4) COMP.\n",
            "          49 STMT-TEXT PIC X(120).\n",
            "       01 IN-DA.\n",
            "          05 IN-DAID PIC X(8).\n",
            "          05 IN-DABC PIC S9(9) BINARY VALUE 60.\n",
            "          05 IN-N    PIC S9(4) BINARY VALUE 1.\n",
            "          05 IN-D    PIC S9(4) BINARY VALUE 1.\n",
            "          05 IN-TYPE PIC S9(4) BINARY VALUE 496.\n",
            "          05 IN-LEN  PIC S9(4) BINARY VALUE 4.\n",
            "          05 IN-DATA POINTER.\n",
            "          05 IN-IND  POINTER.\n",
            "          05 IN-NAME PIC X(32).\n",
            "       01 WS-ID    PIC S9(9) COMP VALUE 7.\n",
            "       01 WS-NAME  PIC X(10).\n",
            "       01 WS-AMT   PIC S9(5)V99 COMP-3 VALUE 0.\n",
            "       01 WS-IND   PIC S9(4) COMP VALUE 0.\n",
            "       01 E-CODE   PIC -9(4).\n",
            "       01 E-NUM    PIC -9(4).\n",
            "       PROCEDURE DIVISION.\n",
            "           MOVE 3 TO SQLN.\n",
            "           SET IN-DATA TO ADDRESS OF WS-ID.\n",
        );

        fn columns() -> Outcome {
            let name = crate::sql::Column { name: "NAME".into(), ty: crate::sql::ColumnType::Char(10), nullable: false };
            let amt = crate::sql::Column { name: "AMT".into(), ty: crate::sql::ColumnType::Decimal { precision: 7, scale: 2 }, nullable: true };
            Outcome { columns: vec![name, amt], ..Outcome::ok() }
        }

        /// SQLD and each described SQLVAR's SQLTYPE, SQLLEN and name.
        const SHOW_DA: &str = concat!(
            "           MOVE SQLD TO E-NUM.\n",
            "           DISPLAY 'SQLD ' E-NUM ' SQLDABC ' SQLDABC.\n",
            "           MOVE SQLTYPE(1) TO E-NUM.\n",
            "           DISPLAY 'TYPE ' E-NUM ' LEN ' SQLLEN(1)\n",
            "                   ' NAME ' SQLNAMEC(1)(1:SQLNAMEL(1)).\n",
            "           MOVE SQLTYPE(2) TO E-NUM.\n",
            "           DISPLAY 'TYPE ' E-NUM ' LEN ' SQLLEN(2)\n",
            "                   ' NAME ' SQLNAMEC(2)(1:SQLNAMEL(2)).\n",
        );

        #[test]
        fn prepare_into_and_describe_fill_the_sqlda_from_the_statement_s_columns() {
            let procedure = [
                set("SELECT NAME, AMT FROM T WHERE ID = ?"),
                exec("PREPARE S1 INTO :SQLDA FROM :STMT"),
                SHOW_DA.into(),
                "           MOVE 1 TO SQLN.\n".into(),
                exec("DESCRIBE S1 INTO :SQLDA"),
                "           MOVE SQLD TO E-NUM.\n           DISPLAY 'SQLD ' E-NUM ' SQLDABC ' SQLDABC.\n".into(),
                exec("DESCRIBE S9 INTO :SQLDA"),
                set("DELETE FROM T"),
                exec("PREPARE S2 FROM :STMT"),
                "           MOVE 3 TO SQLN.\n".into(),
                exec("DESCRIBE S2 INTO :SQLDA USING LABELS"),
                "           MOVE SQLD TO E-NUM.\n           DISPLAY 'SQLD ' E-NUM.\n".into(),
                "           GOBACK.\n".into(),
            ]
            .concat();
            let (shown, calls) = run_source(&format!("{DA}{procedure}"), vec![columns(), Outcome::ok()]);
            let shown = shown.unwrap();
            assert_eq!(
                shown,
                concat!(
                    " 0000\n",
                    "SQLD  0002 SQLDABC 000000148\n",
                    "TYPE  0452 LEN 0010 NAME NAME\n",
                    "TYPE  0485 LEN 1794 NAME AMT\n",
                    " 0000\n",
                    "SQLD  0002 SQLDABC 000000060\n",
                    "-0516\n",
                    " 0000\n",
                    " 0000\n",
                    "SQLD  0000\n",
                )
            );
            assert_eq!(verbs(&calls), ["PREPARE", "PREPARE", "COMMIT"]);
        }

        #[test]
        fn open_fetch_and_execute_take_their_values_where_the_sqlda_points() {
            let procedure = [
                "           EXEC SQL DECLARE C1 CURSOR FOR S1 END-EXEC.\n".into(),
                set("SELECT NAME, AMT FROM T WHERE ID = ?"),
                exec("PREPARE S1 INTO :SQLDA FROM :STMT"),
                "           SET SQLDATA(1) TO ADDRESS OF WS-NAME.\n".into(),
                "           SET SQLDATA(2) TO ADDRESS OF WS-AMT.\n".into(),
                "           SET SQLIND(2) TO ADDRESS OF WS-IND.\n".into(),
                exec("OPEN C1 USING DESCRIPTOR :IN-DA"),
                exec("FETCH C1 USING DESCRIPTOR :SQLDA"),
                "           MOVE WS-IND TO E-NUM.\n           DISPLAY WS-NAME '|' E-NUM.\n".into(),
                exec("CLOSE C1"),
                set("DELETE FROM T WHERE ID = ?"),
                exec("PREPARE S2 FROM :STMT"),
                exec("EXECUTE S2 USING DESCRIPTOR :IN-DA"),
                "           SET IN-DATA TO NULL.\n".into(),
                exec("EXECUTE S2 USING DESCRIPTOR :IN-DA"),
                "           DISPLAY SQLERRMC(1:SQLERRML).\n".into(),
                "           MOVE 2 TO IN-D.\n".into(),
                exec("EXECUTE S2 USING DESCRIPTOR :IN-DA"),
                "           GOBACK.\n".into(),
            ]
            .concat();
            let row = vec![Value::Char("SMITH".into()), Value::Null];
            let answers = vec![columns(), Outcome::ok(), Outcome::rows(vec![row]), Outcome::ok(), Outcome::ok(), Outcome { affected: 1, ..Outcome::ok() }];
            let (shown, calls) = run_source(&format!("{DA}{procedure}"), answers);
            assert_eq!(shown.as_deref(), Ok(" 0000\n 0000\n 0000\nSMITH     |-0001\n 0000\n 0000\n 0000\n-0804\n12\n-0804\n"));
            assert_eq!(verbs(&calls), ["PREPARE", "OPEN", "FETCH", "CLOSE", "PREPARE", "DELETE", "COMMIT"]);
            assert_eq!((calls[1].3.as_slice(), calls[5].3.as_slice()), ([Value::Int(7)].as_slice(), [Value::Int(7)].as_slice()));
        }
    }
    mod multirow {
        use super::*;

        const ROWSETS: &str = concat!(
            "       IDENTIFICATION DIVISION.\n",
            "       PROGRAM-ID. M.\n",
            "       DATA DIVISION.\n",
            "       WORKING-STORAGE SECTION.\n",
            "           EXEC SQL INCLUDE SQLCA END-EXEC.\n",
            "       01 ROWSET-VARS.\n",
            "          05 COL-A PIC X(3) OCCURS 3.\n",
            "          05 COL-B PIC S9(4) COMP OCCURS 3.\n",
            "          05 IND-B PIC S9(4) COMP OCCURS 3.\n",
            "       01 NAMES.\n",
            "          05 NAME OCCURS 3.\n",
            "             49 NAME-LEN PIC S9(4) COMP.\n",
            "             49 NAME-TEXT PIC X(8).\n",
            "       01 GRID.\n",
            "          05 GRID-ROW OCCURS 2.\n",
            "             10 GRID-A PIC X(3) OCCURS 2.\n",
            "       01 ENTRIES.\n",
            "          05 ENTRY-ROW OCCURS 2.\n",
            "             10 ENTRY-A PIC X(3).\n",
            "             10 ENTRY-B PIC S9(4) COMP.\n",
            "       01 N        PIC S9(4) COMP VALUE 3.\n",
            "       01 ONE      PIC X(3).\n",
            "       01 WS-ID    PIC S9(9) COMP VALUE 7.\n",
            "       01 P-IN     PIC X(3) VALUE 'IN1'.\n",
            "       01 P-OUT    PIC X(5) VALUE 'XXXXX'.\n",
            "       01 P-NUM    PIC S9(4) COMP VALUE 5.\n",
            "       01 P-IND    PIC S9(4) COMP VALUE 0.\n",
            "       01 E-CODE   PIC -9(3).\n",
            "       01 E-ROWS   PIC 9.\n",
            "       01 E-IND    PIC -9.\n",
            "       PROCEDURE DIVISION.\n",
            "           EXEC SQL DECLARE C1 CURSOR WITH ROWSET POSITIONING\n",
            "                    FOR SELECT A, B FROM T END-EXEC.\n",
            "           EXEC SQL DECLARE C2 CURSOR FOR SELECT A FROM T END-EXEC.\n",
            "           MOVE '---' TO COL-A(1) COL-A(2) COL-A(3).\n",
        );

        /// The statement in EXEC SQL, its words wrapped inside column 72.
        fn block(sql: &str) -> String {
            let mut lines = vec![String::new()];
            for word in sql.split(' ') {
                let line = lines.last_mut().expect("a line");
                if !line.is_empty() && line.len() + word.len() > 50 {
                    lines.push(word.to_owned());
                } else {
                    if !line.is_empty() {
                        line.push(' ');
                    }
                    line.push_str(word);
                }
            }
            let body: String = lines.iter().map(|l| format!("               {l}\n")).collect();
            format!("           EXEC SQL\n{body}           END-EXEC.\n")
        }

        /// The statement, then SQLCODE, SQLERRD(3) and the three elements of COL-A.
        fn exec(sql: &str) -> String {
            format!("{}           MOVE SQLCODE TO E-CODE.\n           MOVE SQLERRD(3) TO E-ROWS.\n           DISPLAY E-CODE ' ' E-ROWS ' ' COL-A(1) COL-A(2) COL-A(3).\n", block(sql))
        }

        fn rows(procedure: &str, answers: Vec<Outcome>) -> (Result<String, (String, String)>, Vec<Logged>) {
            run_source_message(&format!("{ROWSETS}{procedure}           GOBACK.\n"), answers)
        }

        fn row(a: &str, b: Option<i64>) -> Vec<Value> {
            vec![Value::Char(a.into()), b.map_or(Value::Null, Value::Int)]
        }

        /// The FETCH calls' inputs: the rows each asked the database for.
        fn fetched(calls: &[Logged]) -> Vec<Vec<Value>> {
            calls.iter().filter(|c| c.0 == "FETCH").map(|c| c.3.clone()).collect()
        }

        const INTO: &str = "INTO :COL-A, :COL-B :IND-B";

        #[test]
        fn a_rowset_fills_an_element_a_row_and_a_short_one_is_plus_100_leaving_the_rest() {
            let procedure = [
                exec("OPEN C1"),
                exec(&format!("FETCH NEXT ROWSET FROM C1 FOR :N ROWS {INTO}")),
                "           MOVE IND-B(2) TO E-IND.\n           DISPLAY E-IND ' ' COL-B(3).\n           MOVE '---' TO COL-A(1) COL-A(2) COL-A(3).\n".into(),
                exec(&format!("FETCH NEXT ROWSET FROM C1 {INTO}")),
            ]
            .concat();
            let answers = vec![Outcome::ok(), Outcome::rows(vec![row("AAA", Some(1)), row("BBB", None), row("CCC", Some(3))]), Outcome::rows(vec![row("DDD", Some(4))])];
            let (shown, calls) = rows(&procedure, answers);
            assert_eq!(shown.as_deref(), Ok(" 000 0 ---------\n 000 3 AAABBBCCC\n-1 0003\n 100 1 DDD------\n"));
            assert_eq!(fetched(&calls), [[Value::Int(3)], [Value::Int(3)]]);
            assert!(calls.iter().any(|c| c.2 == "FETCH NEXT ROWSET FROM C1 FOR ? ROWS"), "{calls:?}");
        }

        #[test]
        fn a_row_fetch_after_a_rowset_moves_from_its_first_row_and_reads_nothing_again() {
            let procedure = [
                exec("OPEN C1"),
                exec(&format!("FETCH NEXT ROWSET FROM C1 FOR 3 ROWS {INTO}")),
                "           EXEC SQL FETCH C1 INTO :ONE END-EXEC.\n           DISPLAY ONE.\n".into(),
                exec(&format!("FETCH NEXT ROWSET FROM C1 {INTO}")),
                exec(&format!("FETCH NEXT ROWSET FROM C1 FOR 2 ROWS {INTO}")),
            ]
            .concat();
            let answers = vec![Outcome::ok(), Outcome::rows(vec![row("AAA", Some(1)), row("BBB", Some(2)), row("CCC", Some(3))]), Outcome::rows(vec![row("EEE", Some(5)), row("FFF", Some(6))])];
            let (shown, calls) = rows(&procedure, answers);
            assert_eq!(shown.as_deref(), Ok(" 000 0 ---------\n 000 3 AAABBBCCC\nBBB\n 000 1 CCCBBBCCC\n 000 2 EEEFFFCCC\n"));
            assert_eq!(fetched(&calls), [[Value::Int(3)], [Value::Int(2)]]);
        }

        #[test]
        fn a_rowset_needs_a_rowset_cursor_an_open_one_and_rows_its_arrays_hold() {
            let procedure = [
                exec(&format!("FETCH NEXT ROWSET FROM C1 FOR 2 ROWS {INTO}")),
                exec("OPEN C2"),
                exec("FETCH NEXT ROWSET FROM C2 FOR 2 ROWS INTO :COL-A"),
                exec("OPEN C1"),
                exec(&format!("FETCH NEXT ROWSET FROM C1 FOR 4 ROWS {INTO}")),
                exec(&format!("FETCH NEXT ROWSET FROM C1 FOR 0 ROWS {INTO}")),
                "           MOVE 0 TO N.\n".into(),
                exec(&format!("FETCH NEXT ROWSET FROM C1 FOR :N ROWS {INTO}")),
            ]
            .concat();
            let (shown, calls) = rows(&procedure, Vec::new());
            assert_eq!(shown.as_deref(), Ok("-501 0 ---------\n 000 0 ---------\n-249 0 ---------\n 000 0 ---------\n-246 0 ---------\n-246 0 ---------\n-246 0 ---------\n"));
            assert!(fetched(&calls).is_empty(), "{calls:?}");
        }

        #[test]
        fn a_rowset_into_what_is_no_host_variable_array_abends_naming_it() {
            for (into, why) in [
                ("ONE", "ONE is not a host-variable array, which a rowset FETCH's INTO takes"),
                ("ENTRY-ROW", "ENTRY-ROW is a host-structure array, which Db2 for z/OS does not take in COBOL"),
                ("GRID-A", "GRID-A is a table of more than one dimension, which no host-variable array is"),
                ("COL-A :ONE", "COL-A's indicator is not an indicator array, as a host-variable array's must be"),
            ] {
                let (shown, _) = rows(&[exec("OPEN C1"), exec(&format!("FETCH NEXT ROWSET FROM C1 FOR 1 ROWS INTO :{into}"))].concat(), Vec::new());
                assert_eq!(shown.unwrap_err(), ("EXEC".into(), format!("EXEC SQL FETCH: {why}")), "{into}");
            }
        }

        #[test]
        fn a_varchar_array_takes_a_string_an_element() {
            let procedure = [
                "           MOVE SPACES TO NAMES.\n".into(),
                exec("OPEN C1"),
                exec("FETCH NEXT ROWSET FROM C1 FOR 2 ROWS INTO :NAME"),
                "           MOVE NAME-LEN(2) TO E-ROWS.\n           DISPLAY NAME-TEXT(1) '|' NAME-TEXT(2)(1:E-ROWS) '|'.\n".into(),
            ]
            .concat();
            let answers = vec![Outcome::ok(), Outcome::rows(vec![vec![Value::Char("SMITH".into())], vec![Value::Char("LEE".into())]])];
            assert_eq!(rows(&procedure, answers).0.as_deref(), Ok(" 000 0 ---------\n 000 2 ---------\nSMITH   |LEE|\n"));
        }

        #[test]
        fn a_positioned_change_needs_the_rowset_to_be_the_one_row_the_database_is_on() {
            let update = block("UPDATE T SET A = 'X' WHERE CURRENT OF C1");
            let three = [exec("OPEN C1"), exec(&format!("FETCH NEXT ROWSET FROM C1 FOR 3 ROWS {INTO}")), update.clone()].concat();
            let answers = vec![Outcome::ok(), Outcome::rows(vec![row("AAA", Some(1)), row("BBB", Some(2)), row("CCC", Some(3))])];
            let (code, message) = rows(&three, answers).0.unwrap_err();
            assert!(code == "EXEC" && message.contains("of a rowset of more than one row"), "{code} {message}");
            let one = [exec("OPEN C1"), exec(&format!("FETCH NEXT ROWSET FROM C1 FOR 1 ROWS {INTO}")), update].concat();
            let (shown, calls) = rows(&one, vec![Outcome::ok(), Outcome::rows(vec![row("AAA", Some(1))]), Outcome { affected: 1, ..Outcome::ok() }]);
            assert!(shown.is_ok(), "{shown:?}");
            assert!(calls.iter().any(|c| c.0 == "UPDATE"), "{calls:?}");
        }

        #[test]
        fn a_multiple_row_insert_sends_each_row_and_repeats_a_host_variable() {
            let procedure = [
                "           MOVE 'AAA' TO COL-A(1). MOVE 'BBB' TO COL-A(2).\n           MOVE 1 TO COL-B(1). MOVE -1 TO IND-B(2).\n           MOVE 0 TO IND-B(1). MOVE 2 TO N.\n".into(),
                exec("INSERT INTO T (A, B, ID) VALUES (:COL-A, :COL-B :IND-B, :WS-ID) FOR :N ROWS"),
                exec("INSERT INTO T (A) VALUES (:COL-A) FOR 2 ROWS NOT ATOMIC CONTINUE ON SQLEXCEPTION"),
                exec("INSERT INTO T (A) VALUES (:COL-A) FOR 4 ROWS"),
            ]
            .concat();
            let answers = vec![Outcome { affected: 2, ..Outcome::ok() }, Outcome { affected: 1, ..Outcome::error(-253, "22529") }];
            let (shown, calls) = rows(&procedure, answers);
            assert_eq!(shown.as_deref(), Ok(" 000 2 AAABBB---\n-253 1 AAABBB---\n-246 0 AAABBB---\n"));
            let (aaa, bbb) = (Value::Char("AAA".into()), Value::Char("BBB".into()));
            assert_eq!(calls[0], ("INSERT ATOMIC".into(), 3, "INSERT INTO T (A, B, ID) VALUES (?, ?, ?)".into(), vec![aaa.clone(), Value::Int(1), Value::Int(7), bbb.clone(), Value::Null, Value::Int(7)]));
            assert_eq!((calls[1].0.as_str(), calls[1].3.clone()), ("INSERT NOT ATOMIC", vec![aaa, bbb]));
            assert_eq!(calls.len(), 3, "the last is the COMMIT at the end: {calls:?}");
        }

        #[test]
        fn call_assigns_what_the_procedure_returns_and_leaves_the_rest() {
            let show = "           MOVE P-IND TO E-IND.\n           DISPLAY P-IN ' ' P-OUT ' ' E-IND ' ' SQLWARN0 SQLWARN9.\n";
            let procedure = [exec("CALL PROC1 (:P-IN, :P-OUT, :P-NUM :P-IND, 'LIT')"), show.into(), exec("CALL PROC1 (:P-IN, :P-OUT, :P-NUM :P-IND, 'LIT')"), show.into()].concat();
            let returned = vec![None, Some(Value::Char("HELLO".into())), Some(Value::Null)];
            let answers = vec![Outcome { parameters: returned, ..Outcome::error(466, "0100C") }, Outcome { parameters: vec![None, Some(Value::Char("NO".into())), None], ..Outcome::error(-440, "42884") }];
            let (shown, calls) = rows(&procedure, answers);
            assert_eq!(shown.as_deref(), Ok(" 466 0 ---------\nIN1 HELLO -1 WZ\n-440 0 ---------\nIN1 HELLO -1   \n"));
            assert_eq!(calls[0], ("CALL".into(), 3, "CALL PROC1 (?, ?, ?, 'LIT')".into(), vec![Value::Char("IN1".into()), Value::Char("XXXXX".into()), Value::Int(5)]));
            let wrong = vec![Outcome { parameters: vec![None], ..Outcome::ok() }];
            let (code, message) = rows(&exec("CALL PROC1 (:P-IN, :P-OUT)"), wrong).0.unwrap_err();
            assert!(code == "SQL" && message.contains("1 arguments for its 2"), "{code} {message}");
        }

        #[test]
        fn a_recording_answers_a_call_a_multiple_row_insert_and_a_rowset() {
            let procedure = [
                "           MOVE 'AAA' TO COL-A(1). MOVE 'BBB' TO COL-A(2).\n".into(),
                exec("INSERT INTO T (A) VALUES (:COL-A) FOR 2 ROWS"),
                exec("CALL PROC1 (:P-IN, :P-OUT)"),
                "           DISPLAY P-OUT.\n".into(),
                exec("OPEN C1"),
                exec(&format!("FETCH NEXT ROWSET FROM C1 FOR 2 ROWS {INTO}")),
            ]
            .concat();
            let source = format!("{ROWSETS}{procedure}           GOBACK.\n");
            let id = |text: &str| format!("{:08x}", rt::sql::fingerprint(text));
            let recording = [
                "# ironwork sql recording 1\n".to_owned(),
                format!("@ 1 M:3:{} INSERT\n> char:\"AAA\"\n> char:\"BBB\"\n< 0 00000 rows=2\n", id("INSERT INTO T (A) VALUES (?)")),
                format!("@ 2 M:4:{} CALL PROC1\n> char:\"IN1\" | char:\"XXXXX\"\n< 0 00000 rows=0\n= - | char:\"DONE\"\n", id("CALL PROC1 (?, ?)")),
                format!("@ 3 M:5:{} OPEN C1\n< 0 00000 rows=0\n", id("DECLARE C1 CURSOR FOR SELECT A, B FROM T")),
                format!("@ 4 M:6:{} FETCH C1\n> int:2\n< 0 00000 rows=2\n= char:\"R1\" | int:1\n= char:\"R2\" | null\n", id("FETCH NEXT ROWSET FROM C1 FOR ? ROWS")),
                format!("@ 5 M:0:{} COMMIT\n< 0 00000 rows=0\n", id("COMMIT")),
            ]
            .concat();
            let text = recording.clone();
            let ran = Harness::source(&source).database(move || Box::new(crate::sql::Replay::parse(&text, false).expect("the recording parses"))).run(Executor::Interpreter);
            assert_eq!((ran.out.as_str(), ran.ending.is_ok()), (" 000 2 AAABBB---\n 000 0 AAABBB---\nDONE \n 000 0 AAABBB---\n 000 2 R1 R2 ---\n", true), "{}", ran.err);
        }
    }
}
