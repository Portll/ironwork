//! EXEC SQL in the walker: a block's host variables and SQLCA fields resolved into the
//! `SqlEntry` and `Sqlca` that `rt::sql::run` runs, and the WHENEVER branch in force taken.

use super::facts::Facts;
use super::*;
use crate::sql::{self, HostType, Session, SqlHost};
use rt::host::Host;
use rt::lir::{AbendId, HostPlace, SqlEntry, SqlStatement, Sqlca, SqlcaField};
use syntax::sql::{Action, ChangeKind, HostVar, Statement, Whenever};

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
            Statement::Open { cursor, declared } => {
                let declared = declared.as_ref().expect("the parser gives OPEN its DECLARE");
                let hold = if declared.with_hold { " WITH HOLD" } else { "" };
                let text = format!("DECLARE {cursor} CURSOR{hold} FOR {}", declared.text);
                (SqlStatement::Open { cursor: cursor.clone(), inputs: places(&declared.inputs) }, text, declared.with_hold)
            }
            Statement::Fetch { cursor, into } => (SqlStatement::Fetch { cursor: cursor.clone(), into: places(into) }, format!("FETCH {cursor}"), false),
            Statement::Close { cursor } => (SqlStatement::Close { cursor: cursor.clone() }, format!("CLOSE {cursor}"), false),
            Statement::Commit => (SqlStatement::Commit, "COMMIT".into(), false),
            Statement::Rollback => (SqlStatement::Rollback, "ROLLBACK".into(), false),
            Statement::Whenever { .. } | Statement::Declaration | Statement::DeclareCursor(_) | Statement::DeclareUnsupported { .. } => (SqlStatement::Declaration, String::new(), false),
            Statement::Unsupported(what) => (SqlStatement::Unsupported(what.clone()), String::new(), false),
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

/// The SQLCA's fields by name, in the order they are filled.
pub(crate) fn sqlca_fields(pos: Pos) -> Vec<(SqlcaField, Ref)> {
    let named = |name: &str, subscript: Option<u8>| {
        let subscripts = subscript.map(|n| vec![Expr::Operand(Operand::Literal(Literal::Number(n.to_string())))]).unwrap_or_default();
        Ref { name: name.into(), qualifiers: Vec::new(), subscripts, refmod: None, pos }
    };
    let mut fields = vec![
        (SqlcaField::CaId, named("SQLCAID", None)),
        (SqlcaField::CaBc, named("SQLCABC", None)),
        (SqlcaField::Code, named("SQLCODE", None)),
        (SqlcaField::ErrMl, named("SQLERRML", None)),
        (SqlcaField::ErrMc, named("SQLERRMC", None)),
        (SqlcaField::ErrP, named("SQLERRP", None)),
        (SqlcaField::State, named("SQLSTATE", None)),
    ];
    fields.extend((1..=6).map(|n| (SqlcaField::ErrD(n), named("SQLERRD", Some(n)))));
    let warnings = ["SQLWARN0", "SQLWARN1", "SQLWARN2", "SQLWARN3", "SQLWARN4", "SQLWARN5", "SQLWARN6", "SQLWARN7", "SQLWARN8", "SQLWARN9", "SQLWARNA"];
    fields.extend((0..).zip(warnings).map(|(n, name)| (SqlcaField::Warn(n), named(name, None))));
    fields
}

impl<'a, 'p> Host<&'a Ref> for Bound<'_, 'p, '_, '_> {
    type Facts = Facts<'p>;

    fn facts(&self) -> Facts<'p> {
        self.machine.facts()
    }

    fn mem(&mut self) -> &mut [u8] {
        &mut self.machine.unit.mem
    }

    fn locate(&mut self, place: &'a Ref, receiving: bool) -> R<Loc> {
        self.machine.locate_as(place, receiving)
    }

    fn integer(&mut self, place: &'a Ref, pos: Pos) -> R<i64> {
        Host::integer(self.machine, place, pos)
    }

    fn assign(&mut self, dest: Loc, val: Val, src: Option<Loc>, pos: Pos) -> R<()> {
        self.machine.assign(dest, val, src, pos)
    }

    fn store_fixed(&mut self, dest: Loc, value: &Fixed, pos: Pos) -> R<()> {
        self.machine.store_fixed(dest, value, false, pos)
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
        crate::testing::check_lowering(&compiled, rt::sql::fingerprint(procedure), None);
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
        let mut db = Script { answers: vec![Outcome::rows(vec![row])].into(), calls: calls.clone() };
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let ran = compiled.execute_with(crate::unit::Library::default(), crate::files::Dds::default(), None, crate::unit::Clock::System, Some(&mut db), &mut out, &mut err);
        assert!(ran.is_ok(), "{ran:?}");
        assert_eq!(String::from_utf8(out).expect("DISPLAY writes text"), "ADAMS 042-001\n");
        let sent = Vec::from([Value::Char("ADAMS".into()), Value::Int(42), Value::Decimal { value: 100, scale: 2 }]);
        assert_eq!(calls.take()[1].3, sent);
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
        crate::testing::check_lowering(&compiled, rt::sql::fingerprint(procedure), None);
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
}
