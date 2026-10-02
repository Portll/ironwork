use super::*;

fn on_vm(source: &str) -> (String, Result<Ending, Abend>) {
    let o = Harness::source(source).run(Executor::Vm);
    (o.out, o.ending)
}

#[test]
fn the_vm_returns_from_a_range_a_go_to_left_and_resumes_after_its_perform() {
    let source = program(
        "",
        "",
        &[
            "       MAIN.\n",
            &line("PERFORM A THRU C."),
            &line("DISPLAY 'BACK'."),
            &line("STOP RUN."),
            "       A.\n",
            &line("DISPLAY 'A'."),
            "       B.\n",
            &line("DISPLAY 'B'."),
            &line("GO TO D."),
            "       C.\n",
            &line("DISPLAY 'C'."),
            "       D.\n",
            &line("DISPLAY 'D'."),
            &line("GO TO C."),
        ]
        .concat(),
    );
    let (out, ending) = on_vm(&source);
    assert_eq!(ending, Ok(Ending::StopRun));
    assert_eq!(out, "A\nB\nD\nC\nBACK\n");
}

#[test]
fn the_vm_runs_arithmetic_moves_and_tables() {
    let source = program(
        "",
        "       01  T.\n           05 E PIC S9(3)V9 COMP-3 OCCURS 5.\n       01  I PIC 9.\n       01  S PIC S9(5)V9 VALUE 0.\n       01  R PIC Z(4)9.9-.\n",
        &[
            line("PERFORM VARYING I FROM 1 BY 1 UNTIL I > 5"),
            line("    COMPUTE E(I) ROUNDED = I * 2.55"),
            line("    ADD E(I) TO S"),
            line("END-PERFORM"),
            line("SUBTRACT 50 FROM S"),
            line("MOVE S TO R"),
            line("DISPLAY R"),
            line("GOBACK."),
        ]
        .concat(),
    );
    let (out, ending) = on_vm(&source);
    assert_eq!(ending, Ok(Ending::Goback));
    assert_eq!(out, "   11.6-\n");
}

#[test]
fn the_vm_calls_a_program_of_the_run_unit_and_keeps_its_storage() {
    let source = two_programs(
        "       01  N PIC 9(3) VALUE 5.\n",
        &[line("CALL 'SUB' USING N"), line("CALL 'SUB' USING N"), line("DISPLAY N"), line("STOP RUN.")].concat(),
        "SUB",
        "       WORKING-STORAGE SECTION.\n       01  CALLS PIC 9 VALUE 0.\n       LINKAGE SECTION.\n       01  M PIC 9(3).\n",
        &["       PROCEDURE DIVISION USING M.\n", &line("ADD 1 TO CALLS"), &line("ADD CALLS TO M"), &line("GOBACK.")].concat(),
    );
    let (out, ending) = on_vm(&source);
    assert_eq!(ending, Ok(Ending::StopRun));
    assert_eq!(out, "008\n");
}

#[test]
fn the_vm_gives_the_interpreter_s_abend_and_position() {
    let source = program("SSRANGE", "       01  T.\n           05 E PIC X OCCURS 3.\n       01  I PIC 9 VALUE 4.\n", &line("DISPLAY E(I)."));
    let (_, ending) = on_vm(&source);
    let abend = ending.unwrap_err();
    assert_eq!((abend.code.as_str(), abend.pos.line), ("U4038", 10));
    assert!(abend.message.starts_with("IGZ0006S subscript 4 of E"), "{}", abend.message);
}

/// Runs on the interpreter, whose Harness compares the VM's run with its own, and then on the VM,
/// which must run it to its end.
fn on_both(source: &str) -> (String, Result<Ending, Abend>) {
    let walker = Harness::source(source).run(Executor::Interpreter);
    let (out, ending) = on_vm(source);
    assert_eq!((&walker.out, &walker.ending), (&out, &ending));
    (out, ending)
}

/// Paragraph MAIN, then each of `rest`, by name and statements.
fn paragraphs(data: &str, main: &[&str], rest: &[(&str, &[&str])]) -> String {
    let body = |lines: &[&str]| lines.iter().flat_map(|s| s.split('\n')).map(line).collect::<String>();
    let rest: String = rest.iter().map(|(name, lines)| format!("       {name}.\n{}", body(lines))).collect();
    program("", data, &format!("       MAIN.\n{}{rest}", body(main)))
}

#[test]
fn the_vm_leaves_xml_parse_when_its_processing_procedure_goes_to_another_paragraph() {
    let source = paragraphs(
        "       01  DOC PIC X(11) VALUE '<a><b/></a>'.\n       01  N PIC 9 VALUE 0.\n",
        &["XML PARSE DOC PROCESSING PROCEDURE P THRU Q\n    ON EXCEPTION DISPLAY 'EXCEPTION'\n    NOT ON EXCEPTION DISPLAY 'PARSED'\nEND-XML", "DISPLAY 'NOT HERE'.", "STOP RUN."],
        &[
            ("P", &["PERFORM R.", "IF XML-TEXT = 'b' GO TO DONE END-IF."]),
            ("Q", &["DISPLAY XML-EVENT(1:17) '|' XML-TEXT '|'."]),
            ("R", &["ADD 1 TO N."]),
            ("DONE", &["DISPLAY 'LEFT AT ' N.", "STOP RUN."]),
        ],
    );
    let (out, ending) = on_both(&source);
    assert_eq!(ending, Ok(Ending::StopRun));
    assert_eq!(out, "START-OF-DOCUMENT||\nSTART-OF-ELEMENT |a|\nLEFT AT 3\n");
}

#[test]
fn the_vm_ends_the_run_from_an_xml_parse_processing_procedure() {
    let source = paragraphs(
        "       01  DOC PIC X(8) VALUE '<a>x</a>'.\n",
        &["XML PARSE DOC PROCESSING PROCEDURE P", "DISPLAY 'NOT HERE'.", "GOBACK."],
        &[("P", &["DISPLAY XML-EVENT(1:16).", "IF XML-EVENT = 'START-OF-ELEMENT' STOP RUN END-IF."])],
    );
    let (out, ending) = on_both(&source);
    assert_eq!(ending, Ok(Ending::StopRun));
    assert_eq!(out, "START-OF-DOCUMEN\nSTART-OF-ELEMENT\n");
}

#[test]
fn the_vm_checks_reference_modification_of_xml_text_against_the_events_text() {
    let source = paragraphs(
        "       01  DOC PIC X(8) VALUE '<a>x</a>'.\n",
        &["XML PARSE DOC PROCESSING PROCEDURE P.", "GOBACK."],
        &[("P", &["IF XML-EVENT = 'CONTENT-CHARACTERS'\n    DISPLAY XML-TEXT(1:1)\n    DISPLAY XML-TEXT(2:1)\nEND-IF."])],
    );
    let (out, ending) = on_both(&source);
    assert_eq!(out, "x\n");
    let abend = ending.unwrap_err();
    assert_eq!(abend.message, "reference modification (2:1) of XML-TEXT is outside its 1 bytes");
}

#[test]
fn the_vm_sets_and_tests_a_tables_null_indicators_with_the_walks_subscripts() {
    let data = concat!(
        "       01  T PIC X(80) VALUE '{\"R\":{\"E\":[{\"V\":\"ab\"},{\"V\":null}]}}'.\n",
        "       01  R.\n           02 E OCCURS 2.\n              03 F PIC X.\n                 88 F-NULL VALUE 'Y' FALSE 'N'.\n              03 V PIC X(3).\n",
        "       01  D PIC N(60).\n       01  N PIC 9(4).\n       01  C PIC 9(3).\n",
    );
    let source = program(
        "",
        data,
        &[
            "MOVE ALL '-' TO R",
            "JSON PARSE T INTO R ENCODING 1140\n    INDICATING V IS JSON NULL USING F-NULL\nEND-JSON",
            "MOVE JSON-CODE TO C",
            "DISPLAY C ' ' R",
            "JSON GENERATE D FROM R COUNT N\n    INDICATING V IS JSON NULL USING F-NULL\nEND-JSON",
            "DISPLAY N ' ' FUNCTION DISPLAY-OF(D(1:N))",
            "GOBACK.",
        ]
        .iter()
        .flat_map(|s| s.split('\n'))
        .map(line)
        .collect::<String>(),
    );
    let (out, ending) = on_both(&source);
    assert_eq!(ending, Ok(Ending::Goback));
    assert_eq!(out, "000 Nab Y---\n0035 {\"R\":{\"E\":[{\"V\":\"ab\"},{\"V\":null}]}}\n");
}

#[test]
fn the_vm_generates_xml_from_an_elementary_item_as_its_reference_locates_it() {
    let data = "       01  T.\n           05 E PIC X(6) OCCURS 2 VALUE 'a<b'.\n       01  I PIC 9 VALUE 2.\n       01  D PIC N(40).\n       01  N PIC 9(4).\n";
    let source = program("", data, &[line("XML GENERATE D FROM E(I)(2:2) COUNT IN N"), line("DISPLAY N ' ' FUNCTION DISPLAY-OF(D(1:N))"), line("GOBACK.")].concat());
    let (out, ending) = on_both(&source);
    assert_eq!(ending, Ok(Ending::Goback));
    assert_eq!(out, "0012 <E>&lt;b</E>\n");
}

#[test]
#[should_panic(expected = "the VM does not run file I/O yet")]
fn file_statements_stop_the_vm_as_not_run_yet() {
    let source = file_program(
        "           SELECT F ASSIGN TO INFILE.\n",
        "       FD  F.\n       01  REC PIC X(10).\n",
        "",
        &[line("OPEN INPUT F."), line("STOP RUN.")].concat(),
    );
    let _ = on_vm(&source);
}
