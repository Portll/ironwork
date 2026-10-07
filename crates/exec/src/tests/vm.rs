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
    assert!(abend.message.starts_with("IGZ0006S the reference to E addressed an area outside"), "{}", abend.message);
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
fn the_vm_initializes_with_the_interpreter_s_receivers_senders_scaling_and_reports() {
    let data = concat!(
        "       01  G.\n           05 A PIC 9PP VALUE 300.\n           05 B PIC S9(4) COMP VALUE 12.\n",
        "           05 FILLER PIC X(2) VALUE 'FF'.\n           05 C PIC X(3) VALUE 'CCC'.\n           05 E PIC X(2)/X VALUE 'DE'.\n",
        "       01  N PIC 9(5) VALUE 12345.\n",
    );
    let body = [
        "MOVE ALL '*' TO G",
        "INITIALIZE G REPLACING NUMERIC BY N",
        "DISPLAY A ' ' B ' ' G(4:9)",
        "INITIALIZE G WITH FILLER ALL TO VALUE",
        "DISPLAY A ' ' B ' ' G(4:9)",
        "MOVE ALL '*' TO G",
        "INITIALIZE G ALPHANUMERIC TO VALUE THEN TO DEFAULT",
        "DISPLAY A ' ' B ' ' G(4:9)",
        "INITIALIZE RETURN-CODE REPLACING NUMERIC BY 7",
        "DISPLAY RETURN-CODE",
        "INITIALIZE RETURN-CODE",
        "GOBACK.",
    ]
    .map(line)
    .concat();
    let source = program("TRUNC(OPT)", data, &body);
    let (out, ending) = on_both(&source);
    assert_eq!(ending, Ok(Ending::Goback));
    assert_eq!(out, "3 2345 *********\n3 0012 FFCCCDE  \n0 0000 **CCC  / \n0007\n");
    let err = Harness::source(&source).run(Executor::Vm).err;
    assert!(err.contains("TRUNC(OPT) store of 12345 into B PIC S9(4) BINARY"), "{err}");
}

/// N, last in WORKING-STORAGE, has nothing after it for a receiver that overruns it to reach.
#[test]
fn initialize_of_a_reference_modified_item_moves_to_its_characters_alone_as_one_alphanumeric_item() {
    let data = concat!(
        "       01  X PIC X(6) VALUE 'ABCDEF'.\n",
        "       01  G.\n           05 G1 PIC 9(3) VALUE 123.\n           05 G2 PIC X(3) VALUE 'XYZ'.\n",
        "       01  A PIC A(4) VALUE 'ABCD'.\n       01  W PIC N(3) VALUE N'ABC'.\n       01  I PIC 9 VALUE 3.\n",
        "       01  N PIC 9(4) VALUE ZERO.\n",
    );
    let body = [
        "INITIALIZE X (1:2)",
        "DISPLAY '[' X ']'",
        "INITIALIZE N (2:2)",
        "DISPLAY '[' N ']'",
        "INITIALIZE G (2:I)",
        "DISPLAY '[' G ']'",
        "MOVE 'ABCDEF' TO X",
        "INITIALIZE X (I:) REPLACING ALPHANUMERIC BY 'Q'",
        "DISPLAY '[' X ']'",
        "INITIALIZE N (1:1) REPLACING NUMERIC BY 9",
        "INITIALIZE X (1:1) ALPHANUMERIC TO VALUE",
        "INITIALIZE A (2:2) REPLACING ALPHABETIC BY 'Z'",
        "INITIALIZE W (2:1)",
        "DISPLAY '[' N '][' X '][' A '][' W ']' UPON CONSOLE",
        "GOBACK.",
    ]
    .map(line)
    .concat();
    let (out, ending) = on_both(&program("", data, &body));
    assert_eq!(ending, Ok(Ending::Goback));
    assert_eq!(out, "[  CDEF]\n[0  0]\n[1   YZ]\n[ABQ   ]\n[0  0][ABQ   ][AZ D][A C]\n");
}

#[test]
fn a_returning_item_with_no_address_abends_s0c4_at_the_call_on_both_executors() {
    let sub = |data: &str, call: &str, header: &str, body: &[&str]| {
        two_programs(
            "       01  A PIC X(3) VALUE 'ABC'.\n       01  P POINTER.\n",
            &[line(call), line("DISPLAY '[' A ']'"), line("GOBACK.")].concat(),
            "SUB",
            &format!("       LINKAGE SECTION.\n       01  LA PIC X(3).\n{data}"),
            &[format!("       PROCEDURE DIVISION {header}.\n"), body.iter().map(|s| line(s)).collect()].concat(),
        )
    };
    let nulled = sub("       01  LP POINTER.\n", "CALL 'SUB' USING BY VALUE P RETURNING A", "USING BY VALUE LP RETURNING LA", &["SET ADDRESS OF LA TO LP", "GOBACK."]);
    let abend = on_both(&nulled).1.unwrap_err();
    assert_eq!((abend.code.as_str(), abend.pos.line, abend.pos.col), ("S0C4", 8, 12));
    assert!(abend.message.starts_with("LA is a LINKAGE item with no address"), "{}", abend.message);
    let unset = on_both(&sub("", "CALL 'SUB' RETURNING A", "RETURNING LA", &["GOBACK."]));
    assert_eq!(unset.1, Ok(Ending::Goback));
    let unasked = on_both(&sub("", "CALL 'SUB'", "RETURNING LA", &["MOVE 'XYZ' TO LA", "GOBACK."]));
    assert_eq!(unasked, ("[ABC]\n".to_owned(), Ok(Ending::Goback)));
}

const MOVING: &str = concat!(
    "       01  REC.\n           05 CNT PIC 9 VALUE 2.\n           05 CNT2 PIC 9 VALUE 1.\n",
    "           05 ITEM PIC X OCCURS 1 TO 5 DEPENDING ON CNT.\n           05 MID.\n              10 M1 PIC X.\n",
    "              10 MORE PIC X OCCURS 1 TO 3 DEPENDING ON CNT2.\n              10 M2 PIC X.\n           05 LAST PIC XX.\n",
    "       01  W PIC X(20).\n       01  D PIC X(300).\n       01  N PIC 9(4).\n",
);

#[test]
fn the_vm_places_items_after_occurs_depending_on_tables_where_the_interpreter_does() {
    let body = [
        "MOVE 'A' TO ITEM (1) MOVE 'B' TO ITEM (2)",
        "MOVE 'M' TO M1 MOVE 'X' TO MORE (1) MOVE 'Z' TO M2",
        "MOVE 'LL' TO LAST",
        "DISPLAY REC '|' LENGTH OF REC '|' LENGTH OF MID",
        "MOVE REC TO W DISPLAY W '|'",
        "JSON GENERATE D FROM REC COUNT N ENCODING 1140",
        "DISPLAY D(1:N)",
        "XML GENERATE D FROM REC COUNT IN N",
        "DISPLAY D(1:N)",
        "MOVE 3 TO CNT MOVE 2 TO CNT2",
        "DISPLAY M1 MORE (2) M2 LAST",
        "MOVE '{\"REC\":{\"CNT\":1,\"CNT2\":2,\"ITEM\":[\"Q\"],' TO D",
        "MOVE '\"MID\":{\"M1\":\"R\",\"MORE\":[\"S\",\"T\"],\"M2\":\"U\"},' TO D(39:)",
        "MOVE '\"LAST\":\"VW\"}}' TO D(82:)",
        "JSON PARSE D(1:94) INTO REC ENCODING 1140",
        "DISPLAY REC '|' JSON-CODE",
        "GOBACK.",
    ]
    .iter()
    .flat_map(|s| s.split('\n'))
    .map(line)
    .collect::<String>();
    let (out, ending) = on_both(&program("", MOVING, &body));
    assert_eq!(ending, Ok(Ending::Goback));
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "21ABMXZLL|000000009|000000003");
    assert_eq!(lines[1], "21ABMXZLL           |");
    assert_eq!(lines[2], "{\"REC\":{\"CNT\":2,\"CNT2\":1,\"ITEM\":[\"A\",\"B\"],\"MID\":{\"M1\":\"M\",\"MORE\":[\"X\"],\"M2\":\"Z\"},\"LAST\":\"LL\"}}");
    assert!(lines[3].starts_with("<REC><CNT>2</CNT><CNT2>1</CNT2><ITEM>A</ITEM><ITEM>B</ITEM><MID><M1>M</M1><MORE>X</MORE><M2>Z</M2></MID>"), "{}", lines[3]);
    assert_eq!(lines.len(), 6, "{out}");
    assert_eq!(lines[5], "12QRSTUVW|000000000");
}

#[test]
fn the_vm_places_an_unnamed_group_after_a_table_in_xml_generate_and_json_parse() {
    let data = concat!(
        "       01  REC.\n           05 CNT PIC 9 VALUE 2.\n           05 ITEM PIC X OCCURS 1 TO 3 DEPENDING ON CNT.\n",
        "           05 FILLER.\n              10 A PIC X.\n              10 B PIC X.\n       01  D PIC X(100).\n       01  N PIC 9(4).\n",
    );
    let body = [
        "MOVE 'P' TO ITEM (1) MOVE 'Q' TO ITEM (2)",
        "MOVE 'A' TO A MOVE 'B' TO B",
        "XML GENERATE D FROM REC COUNT IN N",
        "DISPLAY D(1:N)",
        "MOVE '{\"REC\":{\"CNT\":1,\"ITEM\":[\"X\"],\"A\":\"Y\",\"B\":\"Z\"}}' TO D",
        "JSON PARSE D(1:46) INTO REC ENCODING 1140",
        "DISPLAY REC '|' JSON-CODE",
        "GOBACK.",
    ]
    .map(line)
    .concat();
    let (out, ending) = on_both(&program("", data, &body));
    assert_eq!(ending, Ok(Ending::Goback));
    assert_eq!(out, "<REC><CNT>2</CNT><ITEM>P</ITEM><ITEM>Q</ITEM><A>A</A><B>B</B></REC>\n1XYZ|000000000\n");
}

#[test]
fn the_vm_checks_the_count_that_moves_an_item_under_ssrange() {
    let body = [line("MOVE 7 TO CNT"), line("DISPLAY 'BEFORE'"), line("DISPLAY LAST"), line("GOBACK.")].concat();
    let (out, ending) = on_both(&program("SSRANGE", MOVING, &body));
    assert_eq!(out, "BEFORE\n");
    let abend = ending.unwrap_err();
    assert!(abend.message.starts_with("IGZ0007S CNT = 7"), "{}", abend.message);
}

#[test]
fn the_vm_places_an_item_after_a_table_in_an_external_record_shared_with_a_called_program() {
    let shared = "       01  XREC EXTERNAL.\n           05 XCNT PIC 9.\n           05 XT PIC X OCCURS 1 TO 4 DEPENDING ON XCNT.\n           05 XLATER PIC XX.\n";
    let source = two_programs(
        shared,
        &[line("MOVE SPACES TO XREC"), line("MOVE 2 TO XCNT"), line("MOVE 'AB' TO XREC (2:2)"), line("MOVE 'ZZ' TO XLATER"), line("CALL 'SUB'"), line("DISPLAY XREC"), line("STOP RUN.")].concat(),
        "SUB",
        &format!("       WORKING-STORAGE SECTION.\n{shared}"),
        &["       PROCEDURE DIVISION.\n", &line("DISPLAY XLATER"), &line("MOVE 3 TO XCNT"), &line("DISPLAY XLATER"), &line("GOBACK.")].concat(),
    );
    let (out, ending) = on_both(&source);
    assert_eq!(ending, Ok(Ending::StopRun));
    assert_eq!(out, "ZZ\nZ \n3ABZZ \n");
}

#[test]
fn the_vm_places_an_item_after_a_table_in_a_global_record_a_contained_program_reads() {
    let lines = [
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. MAIN.",
        "DATA DIVISION.",
        "WORKING-STORAGE SECTION.",
        "01  GREC GLOBAL.",
        "    05 GCNT PIC 9.",
        "    05 GT PIC X OCCURS 1 TO 4 DEPENDING ON GCNT.",
        "    05 GLATER PIC XX.",
        "PROCEDURE DIVISION.",
        "    MOVE SPACES TO GREC MOVE 2 TO GCNT MOVE 'ZZ' TO GLATER",
        "    CALL 'INNER'",
        "    DISPLAY GREC",
        "    STOP RUN.",
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. INNER.",
        "PROCEDURE DIVISION.",
        "    DISPLAY GLATER MOVE 1 TO GCNT DISPLAY GLATER",
        "    GOBACK.",
        "END PROGRAM INNER.",
        "END PROGRAM MAIN.",
    ];
    let source: String = lines.iter().map(|l| format!("       {l}\n")).collect();
    let (out, ending) = on_both(&source);
    assert_eq!(ending, Ok(Ending::StopRun));
    assert_eq!(out, "ZZ\n Z\n1  Z\n");
}

#[test]
fn the_vm_calls_through_a_pointer_that_holds_no_entry_as_the_interpreter_does() {
    let data = "       01  PP USAGE PROCEDURE-POINTER.\n";
    let (out, ending) = on_both(&program("", data, &[line("SET PP TO NULL"), line("DISPLAY 'CALLING'"), line("CALL PP"), line("GOBACK.")].concat()));
    assert_eq!(out, "CALLING\n");
    let abend = ending.unwrap_err();
    assert!(abend.message.contains("X'00000000' is not a JNI service"), "{}", abend.message);
}

/// A CICS task on the interpreter, whose Harness compares the VM's run and the events its observer
/// is told with its own, then on the VM, which must run it to its end.
fn task_on_both(source: &str) -> (String, Result<Ending, Abend>) {
    let walker = Harness::source(source).task(task("TR12")).run(Executor::Interpreter);
    let vm = Harness::source(source).task(task("TR12")).run(Executor::Vm);
    assert_eq!((&walker.out, &walker.ending), (&vm.out, &vm.ending));
    (vm.out, vm.ending)
}

#[test]
fn the_vm_tells_an_observer_each_cics_option_the_interpreter_does() {
    let data = "       01  WS-REC PIC X(4) VALUE 'REC1'.\n       01  WS-SYS PIC X(4) VALUE 'REM1'.\n       01  WS-Q PIC X(8) VALUE 'Q2'.\n";
    let ending = |last: &str| {
        let body = [
            line("EXEC CICS WRITEQ TS QUEUE('Q1') FROM(WS-REC)"),
            line("    SYSID(WS-SYS) END-EXEC"),
            line("EXEC CICS WRITEQ TS QUEUE('Q1') QNAME(WS-Q)"),
            line("    FROM(WS-REC) END-EXEC"),
            line("EXEC CICS ASKTIME SYSID(WS-SYS) END-EXEC"),
            line("DISPLAY 'WRITTEN'"),
            line(last),
            line("EXEC CICS RETURN END-EXEC."),
        ]
        .concat();
        let (out, ending) = task_on_both(&cics_program("SINKS", data, "", &body));
        assert_eq!(out, "WRITTEN\n");
        ending.map_err(|a| a.message)
    };
    assert_eq!(ending("EXEC CICS WRITE JOURNALNAME('J1') FROM(WS-REC) END-EXEC"), Err("EXEC CICS WRITE needs FILE".into()));
    assert_eq!(ending("EXEC CICS START TRANSID(WS-REC) SYSID(WS-SYS) END-EXEC"), Err("IWR0058-S EXEC CICS START is not supported yet".into()));
    assert_eq!(ending("CONTINUE"), Ok(Ending::Goback));
}

#[test]
fn the_vm_abends_where_the_interpreter_refuses_to_bind_a_cics_block() {
    let refused = |block: &str| {
        let body = [line("DISPLAY 'BEFORE'"), line(block), line("EXEC CICS RETURN END-EXEC."), "       X.\n".into(), line("GOBACK.")].concat();
        let (out, ending) = task_on_both(&cics_program("REFUSED", "       01  WS-SYS PIC X(4) VALUE 'REM1'.\n", "", &body));
        assert_eq!(out, "BEFORE\n");
        ending.unwrap_err().message
    };
    assert_eq!(refused("EXEC CICS HANDLE ABEND LABEL(X) RESET SYSID(WS-SYS) END-EXEC"), "EXEC CICS HANDLE ABEND takes one of PROGRAM, LABEL, CANCEL and RESET");
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

/// SEARCH ALL of a table whose element is one zoned key of `pic`, laid over `keys`, for the key
/// equal to `value`: what each executor finds, and where the binary search leaves the index.
fn search_all_zoned(options: &str, pic: &str, keys: &[&str], value: &str) -> String {
    let mut data: String = std::iter::once("       01  TBL.\n".to_owned()).chain(keys.iter().map(|k| format!("           05 FILLER PIC X({}) VALUE '{k}'.\n", k.len()))).collect();
    data.push_str(&format!("       01  T REDEFINES TBL.\n           05 E OCCURS {} ASCENDING KEY K INDEXED BY IX.\n              10 K PIC {pic}.\n", keys.len()));
    data.push_str("       01  N PIC 9.\n");
    let body = [
        "SEARCH ALL E AT END DISPLAY 'END' WITH NO ADVANCING",
        &format!("  WHEN K(IX) = {value} DISPLAY 'FOUND' WITH NO ADVANCING"),
        "END-SEARCH",
        "SET N TO IX",
        "DISPLAY ' ' N",
        "GOBACK.",
    ]
    .map(line)
    .concat();
    let (out, ending) = on_both(&program(options, &data, &body));
    assert_eq!(ending, Ok(Ending::Goback));
    out
}

#[test]
fn the_vm_compares_a_search_all_key_by_its_bytes_where_the_interpreter_does() {
    assert_eq!(search_all_zoned("", "9(3)", &["001", "0 2", "003"], "'0 2'"), "FOUND 2\n");
    assert_eq!(search_all_zoned("NOZWB", "S9(3)", &["00A", "00B", "00C"], "'002'"), "END 3\n");
    assert_eq!(search_all_zoned("", "S9(3)", &["00A", "00B", "00C"], "'002'"), "FOUND 2\n");
    assert_eq!(search_all_zoned("OPTIMIZE(1)", "9(2)", &["00", " 0", "01"], "ZERO"), "END 3\n");
    assert_eq!(search_all_zoned("", "9(2)", &["00", " 0", "01"], "ZERO"), "FOUND 2\n");
}

#[test]
fn the_vm_takes_a_floating_point_function_s_whole_part_as_an_exec_cics_number() {
    let data = "       01  WS-REC PIC X(8) VALUE 'ABCDEFGH'.\n       01  WS-OUT PIC X(8).\n       01  WS-LEN PIC S9(4) COMP VALUE 8.\n";
    let body = [
        line("EXEC CICS WRITEQ TS QUEUE('Q1') FROM(WS-REC)"),
        line("    LENGTH(FUNCTION NUMVAL('5.9')) END-EXEC"),
        line("EXEC CICS READQ TS QUEUE('Q1') INTO(WS-OUT) LENGTH(WS-LEN)"),
        line("    END-EXEC"),
        line("DISPLAY WS-LEN ' ' WS-OUT"),
        line("EXEC CICS RETURN END-EXEC."),
    ]
    .concat();
    let (out, ending) = task_on_both(&cics_program("FLOATLEN", data, "", &body));
    assert_eq!((out.as_str(), ending), ("0005 ABCDE   \n", Ok(Ending::Goback)));
}

#[test]
fn the_vm_moves_and_compares_an_all_national_literal_as_the_interpreter_does() {
    let data = "       01  N PIC N(5) USAGE NATIONAL.\n       01  XA PIC N(4) USAGE NATIONAL VALUE ALL N'AB'.\n";
    let body = [
        "MOVE ALL N'AB' TO N",
        "DISPLAY '[' N '][' XA ']' UPON CONSOLE",
        "IF N = ALL N'AB' DISPLAY 'EQ' END-IF",
        "IF ALL N\"AB\" < XA DISPLAY 'LT' ELSE DISPLAY 'GE' END-IF",
        "IF N > ALL N'AA' DISPLAY 'GT' END-IF",
        "EVALUATE N WHEN ALL N'AB' DISPLAY 'WHEN' END-EVALUATE",
        "GOBACK.",
    ];
    let (out, ending) = on_both(&program("", data, &body.map(line).concat()));
    assert!(ending.is_ok(), "{ending:?}");
    assert_eq!(out, "[ABABA][ABAB]\nEQ\nGE\nGT\nWHEN\n");
}

#[test]
fn the_vm_steps_a_floating_point_varying_variable_in_floating_point() {
    let data = "       01  F COMP-2.\n       01  H COMP-1.\n       01  J PIC 9.\n       01  K PIC 99 VALUE 0.\n       01  D PIC -9.99.\n";
    let source = paragraphs(
        data,
        &[
            "PERFORM VARYING F FROM 1.5 BY 1 UNTIL F > 3",
            "    MOVE F TO D",
            "    DISPLAY D",
            "END-PERFORM",
            "PERFORM COUNT-IT VARYING H FROM 0.25 BY 0.5 UNTIL H > 1.5",
            "    AFTER J FROM 1 BY 1 UNTIL J > 2",
            "DISPLAY K",
            "PERFORM WITH TEST AFTER VARYING F FROM 2 BY -0.75 UNTIL F < 0",
            "    MOVE F TO D",
            "    DISPLAY D",
            "END-PERFORM",
            "GOBACK.",
        ],
        &[("COUNT-IT", &["ADD 1 TO K."])],
    );
    let (out, ending) = on_both(&source);
    assert!(ending.is_ok(), "{ending:?}");
    assert_eq!(out, " 1.50\n 2.50\n06\n 2.00\n 1.25\n 0.50\n-0.25\n");
}

#[test]
fn the_vm_evaluates_range_and_max_with_a_floating_point_argument_in_floating_point() {
    let data = "       01  N PIC 9 VALUE 2.\n       01  M PIC 9V9 VALUE 0.5.\n       01  F COMP-2 VALUE 1.5.\n       01  R COMP-2.\n       01  D PIC -9.99.\n       01  C PIC 9 VALUE 2.\n       01  G.\n           05 T PIC 9 OCCURS 1 TO 3 DEPENDING ON C.\n";
    let body = [
        "MOVE 7 TO T(1)",
        "MOVE 4 TO T(2)",
        "COMPUTE R = FUNCTION RANGE(N F)",
        "MOVE R TO D",
        "DISPLAY D",
        "COMPUTE D = FUNCTION RANGE(N M F)",
        "DISPLAY D",
        "IF FUNCTION RANGE(N M F) = 1.5 DISPLAY 'EQ' END-IF",
        "COMPUTE R = FUNCTION MAX(T(ALL) F)",
        "MOVE R TO D",
        "DISPLAY D",
        "MOVE 0 TO C",
        "COMPUTE D = FUNCTION MAX(T(ALL) F)",
        "DISPLAY D",
        "GOBACK.",
    ];
    let (out, ending) = on_both(&program("", data, &body.map(line).concat()));
    assert!(ending.is_ok(), "{ending:?}");
    assert_eq!(out, " 0.50\n 1.50\nEQ\n 7.00\n 1.50\n");
}

#[test]
fn the_vm_writes_and_parses_items_with_picture_scaling_positions() {
    // E's VALUE is 7800 in edited form: a numeric-edited item takes an alphanumeric literal there (Language Reference SC27-8713-03, p. 246).
    let data = concat!(
        "       01  D PIC N(80) USAGE NATIONAL.\n       01  X PIC X(80).\n       01  J PIC N(80) USAGE NATIONAL.\n",
        "       01  G.\n           05 S PIC 9PP VALUE 300.\n           05 T PIC SVPP9 VALUE -.005.\n",
        "           05 B PIC 9(2)PP COMP VALUE 1200.\n           05 C PIC S9(2)PP COMP-5 VALUE -4500.\n",
        "           05 K PIC 9(3)PPP COMP-3 VALUE 45000.\n           05 E PIC Z9PP VALUE '78'.\n",
    );
    let body = [
        "MOVE SPACES TO D",
        "JSON GENERATE D FROM G",
        "DISPLAY D UPON CONSOLE",
        "MOVE SPACES TO X",
        "XML GENERATE X FROM G",
        "DISPLAY X",
        "MOVE D TO J",
        "MOVE ZERO TO S T B C K E",
        "JSON PARSE J INTO G",
        "DISPLAY JSON-CODE ' ' S ' ' T ' ' B ' ' C ' ' K ' ' E",
        "MOVE S TO X",
        "DISPLAY X",
        "GOBACK.",
    ];
    let (out, ending) = on_both(&program("", data, &body.map(line).concat()));
    assert!(ending.is_ok(), "{ending:?}");
    let json = "{\"G\":{\"S\":300,\"T\":-0.005,\"B\":1200,\"C\":-4500,\"K\":45000,\"E\":\"78\"}}";
    let xml = "<G><S>300</S><T>-0.005</T><B>1200</B><C>-4500</C><K>45000</K><E>78</E></G>";
    assert_eq!(out, format!("{json:<80}\n{xml:<80}\n000000000 3 N 12 0004N 045  0\n{:<80}\n", "300"));
}
