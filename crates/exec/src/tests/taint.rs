use super::*;
use std::cell::RefCell;
use std::io::Cursor;
use std::rc::Rc;

/// Each sink a run of `source` reached, by line, with whether an input byte may be in its operand;
/// the same source also runs under the Harness, whose differential compares the VM's taint.
fn sinks(source: &str, sysin: &str) -> Vec<(u32, Option<bool>)> {
    let _ = Harness::source(source).sysin(sysin).run(Executor::Interpreter);
    let mut programs = syntax::parse_all_with(source, &syntax::copy::Libraries::default()).unwrap_or_else(|e| panic!("{e}"));
    let compiled = compile(programs.remove(0), &[]).unwrap_or_else(|e| panic!("{e:?}"));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let recorder = seen.clone();
    let observer: unit::Observer<'_> = Box::new(move |e| {
        if let unit::Event::Sink { line, input, .. } = e {
            recorder.borrow_mut().push((line, input));
        }
    });
    let library = unit::Library { programs, trace_input: true, ..Default::default() };
    let sysin = Some(Box::new(Cursor::new(sysin.as_bytes().to_vec())) as Box<dyn std::io::BufRead>);
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let ended = compiled.execute_observed(library, files::Dds::default(), sysin, unit::Clock::Fixed(0, 0), None, &mut out, &mut err, Some(observer));
    assert!(ended.is_ok(), "{ended:?} {}", String::from_utf8_lossy(&err));
    seen.borrow().clone()
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
