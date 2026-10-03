use super::*;

/// A user-defined function: FUNCTION-ID and its phrases, LOCAL-STORAGE and LINKAGE entries, the
/// PROCEDURE DIVISION header's phrases, and the body.
fn function(head: &str, local: &[&str], linkage: &[&str], header: &str, body: &[&str]) -> String {
    let name = head.split_whitespace().next().unwrap();
    let section = |title: &str, entries: &[&str]| {
        if entries.is_empty() { String::new() } else { format!("       {title} SECTION.\n{}", entries.iter().map(|e| format!("       {e}\n")).collect::<String>()) }
    };
    let body: String = body.iter().map(|l| line(l)).collect();
    format!(
        "       IDENTIFICATION DIVISION.\n       FUNCTION-ID. {head}.\n       DATA DIVISION.\n{}{}       PROCEDURE DIVISION\n           {header}.\n{body}       END FUNCTION {name}.\n",
        section("LOCAL-STORAGE", local),
        section("LINKAGE", linkage)
    )
}

/// A program MAIN: REPOSITORY entries, WORKING-STORAGE entries and the body.
fn program(repository: &[&str], data: &[&str], body: &[&str]) -> String {
    let repository = if repository.is_empty() {
        String::new()
    } else {
        format!("       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n       REPOSITORY.\n{}.\n", repository.iter().map(|e| format!("           {e}")).collect::<Vec<_>>().join("\n"))
    };
    let data: String = data.iter().map(|e| format!("       {e}\n")).collect();
    let body: String = body.iter().map(|l| line(l)).collect();
    format!("       IDENTIFICATION DIVISION.\n       PROGRAM-ID. MAIN.\n{repository}       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n{data}       PROCEDURE DIVISION.\n{body}       END PROGRAM MAIN.\n")
}

fn run(source: &str) -> (String, Result<Ending, Abend>) {
    let o = Harness::source(source).run(Executor::Interpreter);
    (o.out, o.ending)
}

/// Every message compiling each program of `source` gives, in source order.
fn messages(source: &str) -> String {
    let programs = syntax::parse_all_with(source, &Default::default()).unwrap_or_else(|e| panic!("{e}"));
    let mut all = Vec::new();
    for p in programs {
        all.extend(compile(p, &[]).map_or_else(|e| e, |c| c.diagnostics));
    }
    all.iter().map(Error::labelled).collect::<Vec<_>>().join("\n")
}

fn parse_error(source: &str) -> String {
    syntax::parse_all_with(source, &Default::default()).unwrap_err().to_string()
}

const GREET: &[&str] = &["01 WHO PIC X(5).", "01 G.", "   05 G1 PIC X(6).", "   05 G2 PIC X(5)."];

fn wrap() -> String {
    function("WRAP", &[], &["01 W PIC X(3).", "01 R PIC X(5)."], "USING W RETURNING R", &["MOVE '<' TO R(1:1)", "MOVE W TO R(2:3)", "MOVE '>' TO R(5:1)", "GOBACK."])
}

#[test]
fn the_programming_guides_example_gives_its_result() {
    let source = [
        function(
            "docalc",
            &[],
            &["1 kind pic x(3).", "1 argA pic 999.", "1 argB pic v999.", "1 res pic 999v999."],
            "using by reference kind argA argB returning res",
            &["if kind equal \"add\" then", "  compute res = argA + argB", "end-if", "goback."],
        ),
        "       Identification division.\n       Program-id. 'mainprog'.\n       Environment division.\n       Configuration section.\n       Repository.\n           function docalc.\n       Data division.\n       Working-storage section.\n       1 result pic 999v999 usage display.\n       Procedure division.\n           compute result = docalc(\"add\" 10 0.23)\n           display \"hello from mainprog, result=\" result\n           goback.\n       End program 'mainprog'.\n".into(),
    ]
    .concat();
    assert_eq!(run(&source), ("hello from mainprog, result=010230\n".into(), Ok(Ending::Goback)));
}

#[test]
fn a_prototype_lets_the_definition_follow_the_program_or_sit_in_a_program_library() {
    let prototype = "       IDENTIFICATION DIVISION.\n       FUNCTION-ID. GetRecord AS 'GETREC1' IS PROTOTYPE.\n       DATA DIVISION.\n       LINKAGE SECTION.\n       1 retval pic x(100).\n       PROCEDURE DIVISION RETURNING retval.\n       END FUNCTION GetRecord.\n";
    let definition = function("GetRecord AS 'GETREC1'", &[], &["1 retval pic x(100)."], "returning retval", &["move \"data\" to retval", "goback."]);
    let main = program(&[], &["01 A PIC X(100)."], &["MOVE FUNCTION GetRecord TO A", "DISPLAY '[' A(1:6) ']' FUNCTION GetRecord(2:2)", "GOBACK."]);
    assert_eq!(run(&[prototype, &main, &definition].concat()), ("[data  ]at\n".into(), Ok(Ending::Goback)));
    let library = temp("udf-library");
    std::fs::create_dir_all(&library).unwrap();
    std::fs::write(library.join("GETREC1.cbl"), [prototype, &definition].concat()).unwrap();
    let o = Harness::source(&[prototype, &main].concat()).dirs(vec![library.clone()]).run(Executor::Interpreter);
    std::fs::remove_dir_all(&library).unwrap();
    assert_eq!((o.out, o.ending), ("[data  ]at\n".into(), Ok(Ending::Goback)));
    let o = Harness::source(&[prototype, &main].concat()).run(Executor::Interpreter);
    let abend = o.ending.unwrap_err();
    assert_eq!((abend.code, abend.message.as_str()), (AbendCode::ModuleNotFound, "FUNCTION GETRECORD: its definition, GETREC1, is in neither the source nor the program libraries"));
}

#[test]
fn a_function_is_recursive_with_local_storage_for_each_activation() {
    let fact = function(
        "FACT",
        &["01 M PIC 9(4) COMP."],
        &["01 N PIC 9(4) COMP.", "01 R PIC 9(9) COMP."],
        "USING BY VALUE N RETURNING R",
        &["IF N <= 1", "    MOVE 1 TO R", "ELSE", "    COMPUTE M = N - 1", "    COMPUTE R = N * FUNCTION FACT(M)", "    COMPUTE R = R + M - N + 1", "END-IF", "GOBACK."],
    );
    let main = program(&[], &["01 K PIC 9(4) COMP VALUE 6."], &["DISPLAY FUNCTION FACT(K) ' ' FUNCTION FACT(K - 1) ' ' K", "GOBACK."]);
    assert_eq!(run(&(fact + &main)), ("000000720 000000120 0006\n".into(), Ok(Ending::Goback)));
}

#[test]
fn a_data_item_is_passed_by_reference_and_a_literal_or_expression_as_the_parameter_describes_it() {
    let greet = function("GREET", &[], GREET, "USING WHO RETURNING G", &["MOVE 'HELLO ' TO G1", "MOVE WHO TO G2", "MOVE '!' TO WHO", "GOBACK."]);
    let show = function(
        "SHOW",
        &[],
        &["01 T PIC X(5).", "01 N PIC 9V99.", "01 R.", "   05 RT PIC X(5).", "   05 RS PIC X.", "   05 RN PIC 9V99."],
        "USING T N RETURNING R",
        &["MOVE T TO RT", "MOVE '/' TO RS", "MOVE N TO RN", "GOBACK."],
    );
    let main = program(
        &[],
        &["01 NAME PIC X(5) VALUE 'WORLD'.", "01 K PIC 9 VALUE 1."],
        &["DISPLAY FUNCTION GREET(NAME) '|' NAME", "DISPLAY FUNCTION SHOW('ab' 12.345)", "DISPLAY FUNCTION SHOW(NAME K + 0.5)", "GOBACK."],
    );
    assert_eq!(run(&[greet, show, main].concat()), ("HELLO WORLD|!    \nab   /234\n!    /150\n".into(), Ok(Ending::Goback)));
}

#[test]
fn a_by_value_parameter_is_a_copy() {
    let bump = function("BUMP", &[], &["01 N PIC 9(4) COMP.", "01 R PIC 9(4) COMP."], "USING BY VALUE N RETURNING R", &["ADD 1 TO N", "MOVE N TO R", "GOBACK."]);
    let main = program(&[], &["01 K PIC 9(4) COMP VALUE 41."], &["DISPLAY FUNCTION BUMP(K) ' ' K", "GOBACK."]);
    assert_eq!(run(&(bump + &main)), ("0042 0041\n".into(), Ok(Ending::Goback)));
}

#[test]
fn stop_run_in_a_function_ends_the_run_at_the_statement_that_invoked_it() {
    let quit = function("QUIT", &[], &["01 R PIC X."], "RETURNING R", &["DISPLAY 'IN QUIT'", "STOP RUN."]);
    let main = program(&[], &["01 C PIC X."], &["MOVE FUNCTION QUIT TO C", "DISPLAY 'NOT REACHED'", "GOBACK."]);
    assert_eq!(run(&(quit.clone() + &main)), ("IN QUIT\n".into(), Ok(Ending::StopRun)));
    for invoking in [&["IF FUNCTION QUIT = 'Y'", "    DISPLAY 'YES'", "END-IF"][..], &["PERFORM UNTIL FUNCTION QUIT = 'Y'", "    DISPLAY 'LOOP'", "END-PERFORM"]] {
        let main = program(&[], &[], &[invoking, &["DISPLAY 'NOT REACHED'", "GOBACK."]].concat());
        assert_eq!(run(&(quit.clone() + &main)), ("IN QUIT\n".into(), Ok(Ending::StopRun)), "{invoking:?}");
    }
}

#[test]
fn an_abend_in_a_function_names_its_source_and_a_program_is_no_function() {
    let prototype = |head: &str| format!("       IDENTIFICATION DIVISION.\n       FUNCTION-ID. {head} IS PROTOTYPE.\n       DATA DIVISION.\n       LINKAGE SECTION.\n       01 R PIC X.\n       PROCEDURE DIVISION RETURNING R.\n       END FUNCTION {}.\n", head.split(' ').next().unwrap());
    let library = temp("udf-abend");
    std::fs::create_dir_all(&library).unwrap();
    std::fs::write(library.join("BADF.cbl"), function("BADF", &[], &["01 R PIC X."], "RETURNING R", &["CALL 'NOSUCHPG'", "GOBACK."])).unwrap();
    std::fs::write(library.join("PROG1.cbl"), "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. PROG1.\n       PROCEDURE DIVISION.\n           GOBACK.\n       END PROGRAM PROG1.\n").unwrap();
    let run_in = |head: &str, name: &str| {
        let main = program(&[], &["01 C PIC X."], &[&format!("MOVE FUNCTION {name} TO C"), "GOBACK."]);
        Harness::source(&(prototype(head) + &main)).dirs(vec![library.clone()]).run(Executor::Interpreter).ending.unwrap_err()
    };
    let (bad, not_one) = (run_in("BADF", "BADF"), run_in("NOTFN AS 'PROG1'", "NOTFN"));
    std::fs::remove_dir_all(&library).unwrap();
    assert_eq!((&bad.code, bad.file.as_deref().is_some_and(|f| f.ends_with("BADF.cbl"))), (&AbendCode::ModuleNotFound, true), "{bad:?}");
    assert_eq!(not_one.message, "FUNCTION NOTFN: PROG1 is a program, not a user-defined function");
}

#[test]
fn a_function_invokes_another_and_its_value_can_be_reference_modified_or_named_in_the_repository() {
    let twice = function("TWICE", &[], &["01 W PIC X(3).", "01 R PIC X(10)."], "USING W RETURNING R", &["MOVE FUNCTION WRAP(W) TO R(1:5)", "MOVE FUNCTION WRAP(W) TO R(6:5)", "GOBACK."]);
    let main = program(&["FUNCTION WRAP"], &[], &["DISPLAY FUNCTION TWICE('abc') ' '", "    FUNCTION TWICE('abc')(2:3) ' ' WRAP('xyz')", "GOBACK."]);
    assert_eq!(run(&[wrap(), twice, main].concat()), ("<abc><abc> abc <xyz>\n".into(), Ok(Ending::Goback)));
}

#[test]
fn a_numeric_result_counts_in_an_expression_as_an_item_of_its_description_would() {
    let third = function("THIRD", &[], &["01 X PIC 9(3).", "01 R PIC 9(3)V9(4)."], "USING X RETURNING R", &["COMPUTE R = X / 3", "GOBACK."]);
    let half = function("HALF", &[], &["01 X COMP-2.", "01 R COMP-2."], "USING X RETURNING R", &["COMPUTE R = X / 2", "GOBACK."]);
    let main = program(
        &[],
        &["01 T PIC 9(3)V9(4).", "01 A PIC 9(3)V9(6).", "01 B PIC 9(3)V9(6).", "01 C PIC 9V9."],
        &["MOVE FUNCTION THIRD(10) TO T", "COMPUTE A = FUNCTION THIRD(10) / 7", "COMPUTE B = T / 7", "COMPUTE C = FUNCTION HALF(5)", "DISPLAY A ' ' B ' ' C", "GOBACK."],
    );
    let (out, ending) = run(&[third, half, main].concat());
    let shown: Vec<&str> = out.trim_end().split(' ').collect();
    assert_eq!((shown[0], shown[2], ending), (shown[1], "25", Ok(Ending::Goback)), "{out}");
}

#[test]
fn an_invocation_is_checked_against_the_definition_or_prototype() {
    let f = function("F", &[], &["01 A PIC 9(3).", "01 B PIC X(4).", "01 G.", "   05 G1 PIC X(6).", "01 R PIC 9(3)."], "USING A B G RETURNING R", &["GOBACK."]);
    let main = program(
        &[],
        &["01 N PIC 9(4).", "01 M PIC 9(3).", "01 S PIC X(2).", "01 L PIC X(9).", "01 H.", "   05 H1 PIC X(5).", "01 BIG PIC X(8)."],
        &["DISPLAY FUNCTION F(N S H)", "DISPLAY FUNCTION F(M L BIG)", "DISPLAY FUNCTION F(1)", "DISPLAY FUNCTION F(1 'X' BIG)(1:2)", "DISPLAY FUNCTION G", "GOBACK."],
    );
    let m = messages(&(f + &main));
    for expected in [
        "FUNCTION F argument 1 (N): A is passed BY REFERENCE, so its PICTURE, USAGE, SIGN, JUSTIFIED and BLANK WHEN ZERO must be the argument's",
        "FUNCTION F argument 2 (S): B is passed BY REFERENCE, so its PICTURE, USAGE, SIGN, JUSTIFIED and BLANK WHEN ZERO must be the argument's",
        "FUNCTION F argument 3 (H): 5 bytes cannot be passed BY REFERENCE to the 6-byte G",
        "FUNCTION F takes 3 arguments, not 1",
        "FUNCTION F: only an alphanumeric or national function's value can be reference-modified",
        "FUNCTION G: neither an intrinsic function nor a user-defined function defined or prototyped before this program",
    ] {
        assert!(m.contains(expected), "{expected}\n---\n{m}");
    }
    assert_eq!(m.matches("argument ").count(), 3, "{m}");
    let n = function("N", &[], &["01 X PIC 9(4).", "01 V PIC S9(9) BINARY.", "01 R PIC 9(4)."], "USING X BY VALUE V RETURNING R", &["GOBACK."]);
    let main = program(&[], &["01 WI USAGE INDEX.", "01 W PIC 9(4)."], &["COMPUTE W = FUNCTION N('ABCD' WI)", "COMPUTE W = FUNCTION N(ZERO 1)", "GOBACK."]);
    let m = messages(&(n + &main));
    assert_eq!(
        m,
        [
            "FUNCTION N argument 1: X is numeric, and takes an argument COMPUTE could send it (assumption C272)",
            "FUNCTION N argument 2 (WI): BY VALUE V is numeric, and takes an argument COMPUTE could send it",
            "FUNCTION N argument 1: a function's argument is not a figurative constant",
        ]
        .join("\n")
    );
}

#[test]
fn a_definition_keeps_the_rules_ibm_gives_functions() {
    let m = messages(&function("F", &[], &["01 A PIC X(3).", "01 R PIC X."], "USING BY VALUE A RETURNING R", &["GOBACK."]));
    assert_eq!(m, "PROCEDURE DIVISION USING BY VALUE A: a function's BY VALUE parameter is binary, floating-point, a pointer, or one alphanumeric or national character");
    assert_eq!(messages(&function("F", &[], &[], "", &["GOBACK."])), "FUNCTION-ID F: a user-defined function needs PROCEDURE DIVISION RETURNING");
    let prototype = "       IDENTIFICATION DIVISION.\n       FUNCTION-ID. F IS PROTOTYPE.\n       DATA DIVISION.\n       LINKAGE SECTION.\n       01 R PIC X(2).\n       PROCEDURE DIVISION RETURNING R.\n       END FUNCTION F.\n";
    let m = messages(&(prototype.to_owned() + &function("F", &[], &["01 R PIC X(3)."], "RETURNING R", &["GOBACK."])));
    assert_eq!(m, "FUNCTION-ID F: the RETURNING item R differs from the prototype at line 2");
    let main = program(&[], &[], &["EXEC SQL COMMIT END-EXEC", "GOBACK."]);
    let m = messages(&(function("F", &[], &["01 R PIC X."], "RETURNING R", &["GOBACK."]) + &main));
    assert_eq!(m, "EXEC SQL: SQL and CICS cannot be used with user-defined functions, so neither in one nor in a program after one in its source (assumption C273)");
}

#[test]
fn the_function_syntax_ibm_refuses_is_refused() {
    let r = function("F", &[], &["01 R PIC X."], "RETURNING R", &["EXIT FUNCTION."]);
    assert!(parse_error(&r).ends_with("EXIT FUNCTION: Enterprise COBOL does not yet support the format 4 EXIT statement; GOBACK ends a user-defined function"));
    let nested = program(&[], &[], &["GOBACK."]).replace("       END PROGRAM MAIN.\n", &(function("F", &[], &[], "", &[]) + "       END PROGRAM MAIN.\n"));
    assert!(parse_error(&nested).ends_with("a user-defined function or prototype cannot be nested within a program, function, method or class"));
    assert!(parse_error(&function("MAX", &[], &[], "", &[])).ends_with("FUNCTION-ID MAX: MAX is an intrinsic function's name (assumption C271)"));
    assert!(parse_error(&function("-F", &[], &[], "", &[])).contains("a function name"));
    let unended = function("F", &[], &["01 R PIC X."], "RETURNING R", &["GOBACK."]).replace("       END FUNCTION F.\n", "");
    assert!(parse_error(&unended).contains("END FUNCTION F, which ends a user-defined function"));
    let misnamed = function("F", &[], &["01 R PIC X."], "RETURNING R", &["GOBACK."]).replace("END FUNCTION F", "END FUNCTION G");
    assert!(parse_error(&misnamed).ends_with("END FUNCTION G ends function F"));
    assert!(parse_error(&program(&["FUNCTION LENGTH"], &[], &[])).ends_with("FUNCTION LENGTH: a user-defined function in the REPOSITORY paragraph cannot be named LENGTH"));
    let with_repository = "       IDENTIFICATION DIVISION.\n       FUNCTION-ID. F IS PROTOTYPE.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n       REPOSITORY.\n           FUNCTION G.\n       PROCEDURE DIVISION.\n       END FUNCTION F.\n";
    assert!(parse_error(with_repository).contains("no REPOSITORY paragraph: a function prototype cannot have one"));
    let twice = function("F", &[], &["01 R PIC X."], "RETURNING R", &["GOBACK."]);
    assert!(parse_error(&(twice.clone() + &twice)).ends_with("a second definition of user-defined function F"));
}

#[test]
fn the_first_program_comes_ahead_of_the_functions_before_it_and_each_knows_those_before_it() {
    let source = [wrap(), function("TWICE", &[], &["01 W PIC X(3).", "01 R PIC X(10)."], "USING W RETURNING R", &["GOBACK."]), program(&[], &[], &["GOBACK."])].concat();
    let programs = syntax::parse_all_with(&source, &Default::default()).unwrap();
    let ids: Vec<(&str, Vec<&str>)> = programs.iter().map(|p| (p.id.as_str(), p.prototypes.iter().map(|f| f.name.as_str()).collect())).collect();
    assert_eq!(ids, [("MAIN", vec!["WRAP", "TWICE"]), ("WRAP", vec!["WRAP"]), ("TWICE", vec!["WRAP", "TWICE"])]);
}

#[test]
fn an_invocation_lowers_to_a_plan_and_a_definition_names_its_records() {
    use rt::lir::{Base, Comparand, DisplayItem, FunctionDefinition, Operand as LirOperand, UserArgument};
    let bump = function("BUMP", &[], &["01 N PIC 9(4) COMP.", "01 R PIC 9(4) COMP."], "USING BY VALUE N RETURNING R", &["MOVE N TO R", "GOBACK."]);
    let main = program(&[], &["01 W PIC X(3).", "01 K PIC 9(4) COMP."], &["DISPLAY FUNCTION WRAP(W) FUNCTION WRAP('xyz')(2:3)", "    FUNCTION BUMP(K)", "GOBACK."]);
    let lowered: Vec<_> = syntax::parse_all_with(&[wrap(), bump, main].concat(), &Default::default()).unwrap().into_iter().map(|p| lower::lower(&compile(p, &[]).unwrap()).unwrap()).collect();
    let [main, wrap, bump] = &lowered[..] else { panic!("{lowered:?}") };
    let shown = &main.plans.display[0].items;
    assert_eq!(shown, &[DisplayItem::Value(LirOperand::UserFunction(0)), DisplayItem::Value(LirOperand::UserFunction(1)), DisplayItem::Value(LirOperand::UserFunction(2))]);
    let plans = &main.services.user_functions;
    assert!(matches!(plans[0].args[..], [UserArgument::Reference(_)]) && plans[0].refmod.is_none());
    assert!(matches!(plans[1].args[..], [UserArgument::Value(Comparand::Operand(LirOperand::Const(_)))]) && plans[1].refmod.as_ref().is_some_and(|r| !r.check));
    assert!(matches!(plans[2].args[..], [UserArgument::Value(Comparand::Operand(LirOperand::Load(_)))]));
    assert_eq!((main.symbols[plans[2].name as usize].as_str(), main.services.function.as_ref()), ("BUMP", None));
    for f in [wrap, bump] {
        let Some(FunctionDefinition { params, returning }) = &f.services.function else { panic!("{f:?}") };
        let bases: Vec<Base> = params.iter().chain([returning]).map(|&q| f.places[q as usize].base).collect();
        assert_eq!(bases, [Base::Linkage(0), Base::Linkage(1)]);
    }
    let odo = function("ODO", &[], &["01 R.", "   05 N PIC 9.", "   05 T PIC X OCCURS 1 TO 5 DEPENDING ON N."], "RETURNING R", &["GOBACK."]);
    match lower::lower(&compile(syntax::parse(&odo).unwrap(), &[]).unwrap()) {
        Err(lower::LowerError::Unsupported(what, _)) => assert_eq!(what, "a user-defined function's parameter or RETURNING record holding an OCCURS DEPENDING ON table"),
        other => panic!("{other:?}"),
    }
}
