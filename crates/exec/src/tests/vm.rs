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
#[should_panic(expected = "the VM does not run FUNCTION UUID4")]
fn what_the_vm_does_not_run_stops_it_as_not_run_yet() {
    let _ = on_vm(&program("", "", &line("DISPLAY FUNCTION UUID4.")));
}

fn on_vm_with(source: &str, dds: &[String]) -> (String, Result<Ending, Abend>) {
    let o = Harness::source(source).dds(dds).run(Executor::Vm);
    (o.out, o.ending)
}

#[test]
fn the_vm_runs_file_statements_and_an_error_procedure_that_leaves_by_go_to() {
    let path = temp("vm-records.dat");
    let _ = std::fs::remove_file(&path);
    let source = [
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. F.\n       ENVIRONMENT DIVISION.\n       INPUT-OUTPUT SECTION.\n       FILE-CONTROL.\n",
        "           SELECT F ASSIGN TO FDD FILE STATUS FS.\n           SELECT G ASSIGN TO NODD FILE STATUS FS.\n",
        "       DATA DIVISION.\n       FILE SECTION.\n       FD  F.\n       01  F-REC PIC X.\n       FD  G.\n       01  G-REC PIC X.\n",
        "       WORKING-STORAGE SECTION.\n       01  FS PIC XX.\n       PROCEDURE DIVISION.\n       DECLARATIVES.\n",
        "       E SECTION.\n           USE AFTER ERROR PROCEDURE ON F G.\n       E-1.\n",
        &line("DISPLAY 'E ' FS"),
        &line("IF FS = '35' GO TO RECOVER."),
        "       END DECLARATIVES.\n       MAIN SECTION.\n       M.\n",
        &line("OPEN OUTPUT F"),
        &line("WRITE F-REC FROM 'A' WRITE F-REC FROM 'B' CLOSE F"),
        &line("OPEN INPUT F"),
        &line("PERFORM 3 TIMES"),
        &line("    READ F AT END DISPLAY 'END' NOT AT END DISPLAY F-REC"),
        &line("END-PERFORM"),
        &line("READ F CLOSE F"),
        &line("OPEN INPUT G"),
        &line("DISPLAY 'NOT HERE'."),
        "       RECOVER.\n",
        &line("DISPLAY 'RECOVERED'"),
        &line("GOBACK."),
    ]
    .concat();
    let (out, ending) = on_vm_with(&source, &[format!("FDD={}", path.display())]);
    assert_eq!(ending, Ok(Ending::Goback));
    assert_eq!(out, "A\nB\nEND\nE 10\nE 35\nRECOVERED\n");
    assert_eq!(std::fs::read(&path).unwrap(), [0xC1, 0xC2]);
}

#[test]
fn the_vm_sorts_through_input_and_output_procedures_and_a_table_in_place() {
    let source = file_program(
        "           SELECT S-FILE ASSIGN TO SORTWK1.\n",
        "       SD  S-FILE.\n       01  S-REC.\n           05 S-K PIC 9.\n           05 S-T PIC X.\n",
        "       01  EOF PIC X VALUE 'N'.\n       01  R PIC 99.\n       01  T VALUE '3142'.\n           05 E PIC 9 OCCURS 4.\n",
        &[
            "       MAIN-LINE.\n",
            &line("SORT S-FILE ON DESCENDING KEY S-K"),
            &line("    INPUT PROCEDURE FEED OUTPUT PROCEDURE SHOW"),
            &line("MOVE SORT-RETURN TO R DISPLAY 'SORT-RETURN ' R"),
            &line("SORT E ON ASCENDING KEY DISPLAY T"),
            &line("GOBACK."),
            "       FEED.\n",
            &line("MOVE '1A' TO S-REC RELEASE S-REC"),
            &line("RELEASE S-REC FROM '3B' RELEASE S-REC FROM '2C'."),
            "       SHOW.\n",
            &line("PERFORM UNTIL EOF = 'Y'"),
            &line("    RETURN S-FILE AT END MOVE 'Y' TO EOF"),
            &line("        NOT AT END DISPLAY S-REC END-RETURN"),
            &line("END-PERFORM."),
        ]
        .concat(),
    );
    let (out, ending) = on_vm(&source);
    assert_eq!(ending, Ok(Ending::Goback));
    assert_eq!(out, "3B\n2C\n1A\nSORT-RETURN 00\n1234\n");
}

#[test]
fn a_perform_after_a_stopped_sort_in_its_paragraph_cannot_resume_as_in_the_interpreter() {
    let source = file_program(
        "           SELECT S ASSIGN TO SORTWK1.\n",
        "       SD  S.\n       01  S-REC PIC X.\n",
        "",
        &[
            "       MAIN-P.\n",
            &line("SORT S ON ASCENDING KEY S-REC"),
            &line("    INPUT PROCEDURE FEED OUTPUT PROCEDURE OUT-P"),
            &line("PERFORM A THRU B"),
            &line("DISPLAY 'BACK'"),
            &line("GOBACK."),
            "       A.\n",
            &line("DISPLAY 'A' GO TO C."),
            "       B.\n",
            &line("DISPLAY 'B'."),
            "       C.\n",
            &line("DISPLAY 'C' GO TO B."),
            "       FEED.\n",
            &line("MOVE 16 TO SORT-RETURN RELEASE S-REC FROM 'X'."),
            "       OUT-P.\n",
            &line("DISPLAY 'OUT'."),
        ]
        .concat(),
    );
    let (out, ending) = on_vm(&source);
    assert_eq!(out, "A\nC\nB\n");
    let abend = ending.unwrap_err();
    assert!(abend.message.starts_with("control passed the end of B"), "{}", abend.message);
}

#[test]
fn the_vm_tells_a_debugging_section_how_a_sort_procedure_was_reached() {
    let source = [
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. D.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n",
        "       SOURCE-COMPUTER. IBM-370 WITH DEBUGGING MODE.\n       INPUT-OUTPUT SECTION.\n       FILE-CONTROL.\n",
        "           SELECT S ASSIGN TO SORTWK1.\n       DATA DIVISION.\n       FILE SECTION.\n       SD  S.\n       01  S-REC PIC X.\n",
        "       WORKING-STORAGE SECTION.\n       01  EOF PIC X VALUE 'N'.\n       PROCEDURE DIVISION.\n       DECLARATIVES.\n",
        "       DBG SECTION.\n           USE FOR DEBUGGING ON FEED SHOW.\n       DBG-1.\n",
        &line("DISPLAY DEBUG-NAME(1:4) '|' DEBUG-CONTENTS(1:11) '|'."),
        "       END DECLARATIVES.\n       MAIN SECTION.\n",
        &line("SORT S ON ASCENDING KEY S-REC"),
        &line("    INPUT PROCEDURE FEED OUTPUT PROCEDURE SHOW"),
        &line("GOBACK."),
        "       FEED SECTION.\n",
        &line("RELEASE S-REC FROM 'B' RELEASE S-REC FROM 'A'."),
        "       SHOW SECTION.\n",
        &line("PERFORM UNTIL EOF = 'Y'"),
        &line("    RETURN S AT END MOVE 'Y' TO EOF"),
        &line("        NOT AT END DISPLAY S-REC END-RETURN"),
        &line("END-PERFORM."),
    ]
    .concat();
    let o = Harness::source(&source).flags(&["-debug"]).run(Executor::Vm);
    assert_eq!(o.ending, Ok(Ending::Goback));
    assert_eq!(o.out, "FEED|SORT INPUT |\nSHOW|SORT OUTPUT|\nA\nB\n");
}

#[test]
fn the_vm_writes_a_report_and_runs_its_use_before_reporting_procedure() {
    let path = temp("vm-report.txt");
    let _ = std::fs::remove_file(&path);
    let source = [
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. R.\n       ENVIRONMENT DIVISION.\n       INPUT-OUTPUT SECTION.\n       FILE-CONTROL.\n",
        "           SELECT RPT ASSIGN TO RPTDD.\n       DATA DIVISION.\n       FILE SECTION.\n       FD  RPT REPORT IS TALLY.\n",
        "       WORKING-STORAGE SECTION.\n       01  N PIC 9 VALUE 0.\n       01  CALLS PIC 9 VALUE 0.\n       REPORT SECTION.\n",
        "       RD  TALLY CONTROL IS FINAL.\n       01  ROW TYPE DE LINE PLUS 1.\n           05 R-N COLUMN 1 PIC 9 SOURCE N.\n",
        "       01  TYPE CF FINAL LINE PLUS 1.\n           05 COLUMN 1 VALUE 'SUM'.\n           05 COLUMN 5 PIC Z9 SUM R-N.\n",
        "       PROCEDURE DIVISION.\n       DECLARATIVES.\n       ROW-USE SECTION.\n           USE BEFORE REPORTING ROW.\n       ROW-PARA.\n",
        &line("ADD 1 TO CALLS"),
        &line("IF N = 2 SUPPRESS PRINTING END-IF."),
        "       END DECLARATIVES.\n       MAIN SECTION.\n       MAIN-PARA.\n",
        &line("OPEN OUTPUT RPT INITIATE TALLY"),
        &line("PERFORM 3 TIMES ADD 1 TO N GENERATE ROW END-PERFORM"),
        &line("TERMINATE TALLY CLOSE RPT"),
        &line("DISPLAY CALLS"),
        &line("GOBACK."),
    ]
    .concat();
    let (out, ending) = on_vm_with(&source, &[format!("RPTDD={}:text", path.display())]);
    assert_eq!(ending, Ok(Ending::Goback));
    assert_eq!(out, "3\n");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "1\n3\nSUM  6\n");
}

#[test]
fn the_vm_runs_a_cics_task_through_link_a_handled_condition_and_return() {
    let main = cics_program(
        "MAINP",
        "       01  WS-AREA PIC X(5) VALUE 'AAAAA'.\n       01  WS-DATA PIC X(8).\n",
        "",
        &[
            "       MAIN-LINE.\n",
            &line("EXEC CICS HANDLE CONDITION QIDERR(NO-QUEUE) END-EXEC"),
            &line("EXEC CICS LINK PROGRAM('SUBP') COMMAREA(WS-AREA) END-EXEC"),
            &line("EXEC CICS READQ TS QUEUE('NOQ') INTO(WS-DATA) END-EXEC"),
            &line("DISPLAY 'NOT REACHED'."),
            "       NO-QUEUE.\n",
            &line("DISPLAY 'BACK ' WS-AREA"),
            &line("EXEC CICS RETURN TRANSID('NEXT') COMMAREA(WS-AREA) END-EXEC."),
        ]
        .concat(),
    );
    let sub = cics_program("SUBP", "", "       01  DFHCOMMAREA PIC X(5).\n", &[line("MOVE 'BBBBB' TO DFHCOMMAREA"), line("EXEC CICS RETURN END-EXEC.")].concat());
    let source = format!("{main}       END PROGRAM MAINP.\n{sub}       END PROGRAM SUBP.\n");
    let o = Harness::source(&source).task(task("TR12")).run(Executor::Vm);
    assert_eq!((o.out.as_str(), o.ending), ("BACK BBBBB\n", Ok(Ending::Goback)));
    let t = o.task.expect("the task");
    assert_eq!((t.next_transid.as_deref(), t.returned_commarea), (Some("NEXT"), Some(ebcdic("BBBBB"))));
}

#[test]
fn the_vm_inspects_a_national_item_in_national_characters() {
    let data = "       01  W PIC N(6) VALUE N'AB AB'.\n       01  P PIC N VALUE N'B'.\n       01  C1 PIC 99 VALUE 0.\n       01  C2 PIC 99 VALUE 0.\n";
    let source = program(
        "",
        data,
        &[
            "INSPECT W TALLYING C1 FOR ALL SPACES C2 FOR CHARACTERS",
            "INSPECT W REPLACING ALL SPACES BY ZERO",
            "    FIRST P BY N'b' AFTER INITIAL N'A'",
            "DISPLAY C1 ' ' C2 ' ' FUNCTION DISPLAY-OF(W)",
            "INSPECT W CONVERTING N'0A' TO N'-a' BEFORE INITIAL P",
            "DISPLAY FUNCTION DISPLAY-OF(W)",
            "GOBACK.",
        ]
        .map(line)
        .concat(),
    );
    let (out, ending) = on_both(&source);
    assert_eq!(ending, Ok(Ending::Goback));
    assert_eq!(out, "02 04 Ab0AB0\nab-aB0\n");
}

#[test]
fn not_on_exception_runs_after_the_call_releases_its_nesting_in_both_executors() {
    std::thread::Builder::new().stack_size(64 << 20).spawn(not_on_exception_at_the_nesting_limit).unwrap().join().unwrap();
}

fn not_on_exception_at_the_nesting_limit() {
    let source = |limit: u32| {
        [
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. MAIN.\n       PROCEDURE DIVISION.\n",
            &line("CALL 'REC'"),
            "           GOBACK.\n       END PROGRAM MAIN.\n",
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. REC IS RECURSIVE.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  N PIC 999 VALUE 0.\n       PROCEDURE DIVISION.\n",
            &line("ADD 1 TO N"),
            &line(&format!("IF N < {limit} CALL 'REC'")),
            &line("ELSE CALL 'LEAF' NOT ON EXCEPTION PERFORM P END-CALL"),
            &line("END-IF"),
            "           GOBACK.\n       P.\n",
            &line("DISPLAY 'P ' N."),
            "       END PROGRAM REC.\n",
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. LEAF.\n       PROCEDURE DIVISION.\n           GOBACK.\n       END PROGRAM LEAF.\n",
        ]
        .concat()
    };
    let endings: Vec<bool> = (94..=101).map(|limit| on_both(&source(limit)).1.is_ok()).collect();
    assert!(endings.contains(&true) && endings.contains(&false), "{endings:?}");
}
