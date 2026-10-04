use super::*;
use crate::testing::page;
use std::cell::RefCell;
use std::io::Cursor;
use std::rc::Rc;

/// Each sink a run of `source` reached, by line, with whether an input byte may be in its operand;
/// the same source also runs under the Harness, whose differential compares the VM's taint.
fn sinks(source: &str, sysin: &str) -> Vec<(u32, Option<bool>)> {
    file_sinks(source, sysin, &[])
}

/// [`sinks`], with the files of `dds`.
fn file_sinks(source: &str, sysin: &str, dds: &[String]) -> Vec<(u32, Option<bool>)> {
    let (events, abend) = sink_events(source, sysin, dds);
    assert!(abend.is_none(), "{abend:?}");
    events.into_iter().map(|(_, line, _, input)| (line, input)).collect()
}

type SinkEvent = (&'static str, u32, String, Option<bool>);

/// Each sink a run of `source` reached: its kind, line and operand, and whether an input byte may
/// be in the operand; and the abend the run ended with, if any.
fn sink_events(source: &str, sysin: &str, dds: &[String]) -> (Vec<SinkEvent>, Option<String>) {
    let _ = Harness::source(source).sysin(sysin).dds(dds).run(Executor::Interpreter);
    let mut programs = syntax::parse_all_with(source, &syntax::copy::Libraries::default()).unwrap_or_else(|e| panic!("{e}"));
    let compiled = compile(programs.remove(0), &[]).unwrap_or_else(|e| panic!("{e:?}"));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let recorder = seen.clone();
    let observer: unit::Observer<'_> = Box::new(move |e| {
        if let unit::Event::Sink { kind, line, operand, input, .. } = e {
            recorder.borrow_mut().push((kind, line, operand.to_owned(), input));
        }
    });
    let library = unit::Library { programs, trace_input: true, ..Default::default() };
    let sysin = Some(Box::new(Cursor::new(sysin.as_bytes().to_vec())) as Box<dyn std::io::BufRead>);
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let ended = compiled.execute_observed(library, files::Dds::new(dds, false).unwrap(), sysin, unit::Clock::Fixed(0, 0), None, &mut out, &mut err, Some(observer));
    let abend = ended.err().map(|a| format!("{a:?} {}", String::from_utf8_lossy(&err)));
    (seen.borrow().clone(), abend)
}

fn line_of(source: &str, text: &str) -> u32 {
    source.lines().position(|l| l.contains(text)).unwrap_or_else(|| panic!("no line holds {text}")) as u32 + 1
}

const DATA: &str = "       01  A PIC X(8).\n       01  B PIC X(8).\n       01  C PIC X(8).\n       01  G.\n           05 G1 PIC X(4).\n           05 G2 PIC X(4) VALUE 'KEPT'.\n       01  N PIC 9(4).\n";

#[test]
fn input_reaches_a_sink_through_moves_and_a_constant_or_an_overwrite_clears_it() {
    let body = [
        line("ACCEPT A"),
        line("MOVE A TO B"),
        line("MOVE 'K' TO C"),
        line("DISPLAY 'B ' B"),
        line("DISPLAY 'C ' C"),
        line("MOVE SPACES TO B"),
        line("DISPLAY 'B AGAIN ' B"),
        line("GOBACK."),
    ]
    .concat();
    let source = program("", DATA, &body);
    let at = |text: &str| line_of(&source, text);
    assert_eq!(sinks(&source, "ATTACK\n"), [(at("'B '"), Some(true)), (at("'C '"), Some(false)), (at("'B AGAIN '"), Some(false))]);
}

#[test]
fn a_computed_result_holds_input_when_an_operand_does_and_a_group_part_only_its_own() {
    let body = [
        line("ACCEPT G1"),
        line("COMPUTE N = FUNCTION NUMVAL(G1) * 2"),
        line("MOVE G2 TO C"),
        line("DISPLAY N"),
        line("DISPLAY C"),
        line("DISPLAY G"),
        line("GOBACK."),
    ]
    .concat();
    let source = program("", DATA, &body);
    let at = |text: &str| line_of(&source, text);
    assert_eq!(sinks(&source, "12\n"), [(at("DISPLAY N"), Some(true)), (at("DISPLAY C"), Some(false)), (at("DISPLAY G"), Some(true))]);
}

#[test]
fn a_condition_on_input_does_not_put_input_in_what_it_chooses() {
    let body = [line("ACCEPT A"), line("IF A = 'ATTACK'"), line("    MOVE 'YES' TO C"), line("END-IF"), line("DISPLAY C"), line("GOBACK.")].concat();
    let source = program("", DATA, &body);
    assert_eq!(sinks(&source, "ATTACK\n"), [(line_of(&source, "DISPLAY C"), Some(false))]);
}

#[test]
fn a_called_program_s_argument_and_returning_item_carry_input() {
    let source = two_programs(
        "       01  A PIC X(8).\n       01  R PIC X(8).\n",
        &[line("ACCEPT A"), line("CALL 'SUB' USING BY CONTENT A RETURNING R"), line("DISPLAY 'R ' R"), line("CALL 'SUB' USING BY CONTENT 'SAFE' RETURNING R"), line("DISPLAY 'R AGAIN ' R"), line("STOP RUN.")].concat(),
        "SUB",
        "       LINKAGE SECTION.\n       01  P PIC X(8).\n       01  Q PIC X(8).\n",
        &["       PROCEDURE DIVISION USING P RETURNING Q.\n", &line("DISPLAY 'P ' P"), &line("MOVE P TO Q"), &line("GOBACK.")].concat(),
    );
    let at = |text: &str| line_of(&source, text);
    let expected = [(at("'P '"), Some(true)), (at("'R '"), Some(true)), (at("'P '"), Some(false)), (at("'R AGAIN '"), Some(false))];
    assert_eq!(sinks(&source, "ATTACK\n"), expected);
}

#[test]
fn after_an_operation_taint_does_not_follow_a_clear_sink_is_unknown() {
    let data = "       01  D PIC X(40).\n       01  K PIC X(4) VALUE 'KEPT'.\n       01  REC.\n           05 F1 PIC X(4) VALUE 'ONE'.\n";
    let body = [line("DISPLAY K"), line("JSON GENERATE D FROM REC"), line("DISPLAY K"), line("GOBACK.")].concat();
    let source = program("", data, &body);
    let at = |text: &str| source.lines().enumerate().filter(|(_, l)| l.contains(text)).map(|(i, _)| i as u32 + 1).collect::<Vec<_>>();
    let shown = at("DISPLAY K");
    assert_eq!(sinks(&source, ""), [(shown[0], Some(false)), (shown[1], None)]);
}

#[test]
fn a_function_s_value_carries_input_and_its_statements_leave_what_the_invoking_one_read() {
    let function = |name: &str, linkage: &str, header: &str, body: &str| {
        format!("       IDENTIFICATION DIVISION.\n       FUNCTION-ID. {name}.\n       DATA DIVISION.\n       LINKAGE SECTION.\n{linkage}       PROCEDURE DIVISION {header}.\n{body}       END FUNCTION {name}.\n")
    };
    let echoed = function("ECHOED", "       01  W PIC X(8).\n       01  R PIC X(8).\n", "USING W RETURNING R", &[line("MOVE W TO R"), line("GOBACK.")].concat());
    let constant = function("CONSTANT", "       01  R PIC X(4).\n", "RETURNING R", &[line("MOVE 'SAFE' TO R"), line("GOBACK.")].concat());
    let body = [
        line("ACCEPT A"),
        line("DISPLAY 'READ ' A FUNCTION CONSTANT"),
        line("DISPLAY 'NONE ' FUNCTION CONSTANT"),
        line("MOVE FUNCTION ECHOED(A) TO B"),
        line("MOVE FUNCTION ECHOED('CLEAN') TO C"),
        line("DISPLAY 'B ' B"),
        line("DISPLAY 'C ' C"),
        line("GOBACK."),
    ]
    .concat();
    let source = [echoed, constant, program("", DATA, &body)].concat();
    let at = |text: &str| line_of(&source, text);
    let expected = [(at("'READ '"), Some(true)), (at("'NONE '"), Some(false)), (at("'B '"), Some(true)), (at("'C '"), Some(false))];
    assert_eq!(sinks(&source, "ATTACK\n"), expected);
}

/// Each sink a CICS task running `source` reached, by line, with whether an input byte may be in
/// its operand; `dir` is its copy library. A task without a terminal runs under the Harness too,
/// whose differential compares the VM's taint.
fn task_sinks(source: &str, dir: Option<&std::path::Path>, commarea: Option<&str>, make: impl Fn() -> cics::Task) -> Vec<(u32, Option<bool>)> {
    let task = make();
    if task.terminal.is_none() {
        let mut harness = Harness::source(source).task(make()).clock(unit::Clock::Fixed(0, 0));
        if let Some(c) = commarea {
            harness = harness.commarea(c);
        }
        let _ = harness.run(Executor::Interpreter);
    }
    let libraries = dir.map_or_else(syntax::copy::Libraries::default, |d| syntax::copy::Libraries::new(vec![d.to_path_buf()]));
    let mut programs = syntax::parse_all_with(source, &libraries).unwrap_or_else(|e| panic!("{e}"));
    let compiled = compile(programs.remove(0), &[]).unwrap_or_else(|e| panic!("{e:?}"));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let recorder = seen.clone();
    let observer: unit::Observer<'_> = Box::new(move |e| {
        if let unit::Event::Sink { line, input, .. } = e {
            recorder.borrow_mut().push((line, input));
        }
    });
    let library = unit::Library { programs, copy: libraries, trace_input: true, ..Default::default() };
    let task = cics::Task { commarea: commarea.map(ebcdic), ..task };
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let (ended, _) = crate::execute_task(&compiled, library, files::Dds::default(), task, unit::Clock::Fixed(0, 0), None, &mut out, &mut err, Some(observer), &mut None);
    assert!(ended.is_ok(), "{ended:?} {}", String::from_utf8_lossy(&err));
    seen.borrow().clone()
}

/// A TD queue write of `item`, the operation a sink names for each value under test.
fn logged(item: &str, length: u32) -> String {
    [line(&format!("EXEC CICS WRITEQ TD FROM({item}) QUEUE('CSMT')")), line(&format!("    LENGTH({length}) END-EXEC"))].concat()
}

#[test]
fn the_commarea_and_a_queue_item_are_input_and_a_command_s_own_values_are_not() {
    let data = "       01  WS-K PIC X(8) VALUE 'CONSTANT'.\n       01  WS-T PIC X(8).\n       01  WS-U PIC X(8).\n       01  WS-Q PIC X(8).\n";
    let body = [
        logged("WS-K", 8),
        line("MOVE DFHCOMMAREA TO WS-T"),
        logged("WS-T", 8),
        line("EXEC CICS ASSIGN USERID(WS-U) END-EXEC"),
        logged("WS-U", 8),
        line("EXEC CICS READQ TS QUEUE('INQ') INTO(WS-Q) END-EXEC"),
        logged("WS-Q", 8),
        line("EXEC CICS RETURN END-EXEC."),
    ]
    .concat();
    let source = cics_program("TAINTQ", data, "       01  DFHCOMMAREA PIC X(8).\n", &body);
    let make = || {
        let mut t = task("TR01");
        t.ts.insert("INQ".into(), cics::TsQueue { items: vec![ebcdic("QUEUED  ")], next: 0 });
        t
    };
    let at = |item: &str| line_of(&source, &format!("FROM({item})"));
    let expected = [(at("WS-K"), Some(false)), (at("WS-T"), Some(true)), (at("WS-U"), Some(false)), (at("WS-Q"), Some(true))];
    assert_eq!(task_sinks(&source, None, Some("ATTACK  "), make), expected);
}

#[test]
fn a_linked_program_gets_the_commarea_s_input_and_eibcalen_keeps_its_own_across_the_link() {
    let main = cics_program(
        "TAINTL",
        "       01  WS-AREA PIC X(8).\n       01  WS-N PIC 9(4).\n",
        "       01  DFHCOMMAREA PIC X(8).\n",
        &[
            line("MOVE DFHCOMMAREA TO WS-AREA"),
            line("EXEC CICS LINK PROGRAM('TAINTS') COMMAREA(WS-AREA)"),
            line("    END-EXEC"),
            line("MOVE EIBCALEN TO WS-N"),
            logged("WS-N", 4),
            line("EXEC CICS RETURN END-EXEC."),
        ]
        .concat(),
    );
    let sub = cics_program("TAINTS", "", "       01  DFHCOMMAREA PIC X(8).\n", &[logged("DFHCOMMAREA", 8), line("EXEC CICS RETURN END-EXEC.")].concat());
    let source = [main, sub].concat();
    let at = |item: &str| line_of(&source, &format!("FROM({item})"));
    assert_eq!(task_sinks(&source, None, Some("ATTACK  "), || task("TR01")), [(at("DFHCOMMAREA"), Some(true)), (at("WS-N"), Some(true))]);
}

#[test]
fn what_the_operator_types_into_a_map_and_the_key_pressed_are_input() {
    let dir = temp("taint-bms");
    ordset(&dir);
    let source = cics_program(
        "ORDERS",
        "           COPY ORDSET.\n       01  WS-C PIC X(8).\n       01  WS-A PIC X.\n       01  WS-K PIC X(8) VALUE 'CONSTANT'.\n",
        "",
        &[
            line("MOVE LOW-VALUES TO ORDMAPO"),
            line("EXEC CICS SEND MAP('ORDMAP') MAPSET('ORDSET') ERASE"),
            line("    END-EXEC"),
            line("EXEC CICS RECEIVE MAP('ORDMAP') MAPSET('ORDSET') END-EXEC"),
            line("MOVE CUSTI TO WS-C"),
            logged("WS-C", 8),
            line("MOVE EIBAID TO WS-A"),
            logged("WS-A", 1),
            logged("WS-K", 8),
            line("EXEC CICS RETURN END-EXEC."),
        ]
        .concat(),
    );
    let make = || {
        let scripted = terminal::Scripted::new(24, 80, terminal::parse_script("type 3 12 ACME\nENTER\n").unwrap(), page());
        cics::Task { terminal: Some(Box::new(scripted)), ..task("ORD1") }
    };
    let at = |item: &str| line_of(&source, &format!("FROM({item})"));
    let logs: Vec<_> = task_sinks(&source, Some(&dir), None, make).into_iter().filter(|(l, _)| [at("WS-C"), at("WS-A"), at("WS-K")].contains(l)).collect();
    assert_eq!(logs, [(at("WS-C"), Some(true)), (at("WS-A"), Some(true)), (at("WS-K"), Some(false))]);
}

#[test]
fn file_status_holds_input_only_from_a_key_its_verb_reads() {
    let data = temp("taint-keys.ksds");
    let _ = std::fs::remove_file(&data);
    let source = file_program(
        "           SELECT K-FILE ASSIGN TO KDD ORGANIZATION INDEXED\n               ACCESS DYNAMIC RECORD KEY K-KEY FILE STATUS IS FS.\n",
        "       FD  K-FILE.\n       01  K-REC.\n           05 K-KEY PIC X(4).\n           05 K-DATA PIC X(4).\n",
        "       01  FS PIC XX.\n",
        &[
            line("OPEN OUTPUT K-FILE"),
            line("WRITE K-REC FROM 'K001DATA'"),
            line("WRITE K-REC FROM 'K002MORE'"),
            line("CLOSE K-FILE"),
            line("OPEN INPUT K-FILE"),
            line("READ K-FILE NEXT"),
            line("CLOSE K-FILE"),
            line("OPEN INPUT K-FILE"),
            line("DISPLAY 'OPEN ' FS"),
            line("READ K-FILE KEY IS K-KEY"),
            line("DISPLAY 'READ ' FS"),
            line("START K-FILE KEY IS = K-KEY"),
            line("DISPLAY 'START INPUT ' FS"),
            line("MOVE 'K002' TO K-KEY"),
            line("START K-FILE KEY IS = K-KEY"),
            line("DISPLAY 'START CONSTANT ' FS"),
            line("CLOSE K-FILE"),
            line("GOBACK."),
        ]
        .concat(),
    );
    let at = |shown: &str| line_of(&source, &format!("'{shown} '"));
    let shown: Vec<_> = file_sinks(&source, "", &[format!("KDD={}", data.display())]).into_iter().filter(|(l, _)| [at("OPEN"), at("READ"), at("START INPUT"), at("START CONSTANT")].contains(l)).collect();
    let _ = std::fs::remove_file(&data);
    assert_eq!(shown, [(at("OPEN"), Some(false)), (at("READ"), Some(true)), (at("START INPUT"), Some(true)), (at("START CONSTANT"), Some(false))]);
}

#[test]
fn connect_traces_the_location_a_host_variable_names_before_it_is_refused() {
    let connect = |data: &str, body: &[&str], sysin: &str| {
        let source = program("", data, &body.iter().map(|s| line(s)).collect::<String>());
        let (events, abend) = sink_events(&source, sysin, &[]);
        (events, line_of(&source, "EXEC SQL"), abend.unwrap_or_default())
    };
    let (events, at, abend) = connect(DATA, &["ACCEPT A", "EXEC SQL CONNECT TO :A END-EXEC", "GOBACK."], "CWVRFY01\n");
    assert_eq!(events, [("connection-target", at, "CWVRFY01".to_owned(), Some(true))]);
    assert!(abend.contains("does not run CONNECT"), "{abend}");
    let (events, at, abend) = connect(DATA, &["MOVE 'LOCAL' TO B", "EXEC SQL SET CONNECTION :B END-EXEC", "GOBACK."], "");
    assert_eq!(events, [("connection-target", at, "LOCAL".to_owned(), Some(false))]);
    assert!(abend.contains("does not run SET CONNECTION"), "{abend}");
    let (events, _, abend) = connect(DATA, &["EXEC SQL CONNECT TO DB1 END-EXEC", "GOBACK."], "");
    assert!(events.is_empty() && abend.contains("does not run CONNECT"), "{events:?} {abend}");
}

#[test]
fn a_dynamic_statement_string_is_traced_before_the_run_needs_a_database() {
    let data = "       01  S.\n           49 S-LEN PIC S9(4) COMP.\n           49 S-TEXT PIC X(40).\n";
    let traced = |body: &[&str], sysin: &str| {
        let source = program("", data, &body.iter().map(|s| line(s)).collect::<String>());
        let (events, abend) = sink_events(&source, sysin, &[]);
        (events, line_of(&source, "EXEC SQL"), abend.unwrap_or_default())
    };
    let (events, at, abend) = traced(&["ACCEPT S-TEXT", "MOVE 20 TO S-LEN", "EXEC SQL EXECUTE IMMEDIATE :S END-EXEC", "GOBACK."], "DELETE FROM T WHERE X\n");
    assert_eq!(events, [("dynamic-sql", at, "DELETE FROM T WHERE".to_owned(), Some(true))]);
    assert!(abend.contains("no database is attached"), "{abend}");
    let (events, at, _) = traced(&["MOVE 'DELETE FROM T' TO S-TEXT", "MOVE 13 TO S-LEN", "EXEC SQL PREPARE S1 FROM :S END-EXEC", "GOBACK."], "");
    assert_eq!(events, [("dynamic-sql", at, "DELETE FROM T".to_owned(), Some(false))]);
}
