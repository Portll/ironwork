use super::*;

/// A program whose MAIN paragraph runs `main` and whose paragraph P is the processing procedure.
fn parse(data: &str, main: &[&str], handler: &[&str]) -> String {
    let body = |stmts: &[&str]| stmts.iter().flat_map(|s| s.split('\n')).map(line).collect::<String>();
    let procedure = format!("       MAIN.\n{}{}       P.\n{}", body(main), line("GOBACK."), body(handler));
    let o = Harness::source(&program("", data, &procedure)).run(Executor::Interpreter);
    assert!(o.ending.is_ok(), "{:?}\n{}", o.ending, o.err);
    o.out
}

fn trimmed(out: &str) -> Vec<String> {
    out.lines().map(|l| l.trim_end().to_owned()).collect()
}

#[test]
fn each_event_performs_the_processing_procedure_with_its_text() {
    let data = "       01  DOC PIC X(60) VALUE\n           '<?xml version=\"1.0\"?><msg type=\"short\">Hello, World!</msg>'.\n";
    let out = parse(data, &["XML PARSE DOC PROCESSING PROCEDURE P\n    NOT ON EXCEPTION DISPLAY 'PARSED ' XML-CODE\nEND-XML"], &["DISPLAY XML-EVENT '{' XML-TEXT '}'."]);
    assert_eq!(
        trimmed(&out),
        [
            "START-OF-DOCUMENT             {}",
            "VERSION-INFORMATION           {1.0}",
            "START-OF-ELEMENT              {msg}",
            "ATTRIBUTE-NAME                {type}",
            "ATTRIBUTE-CHARACTERS          {short}",
            "CONTENT-CHARACTERS            {Hello, World!}",
            "END-OF-ELEMENT                {msg}",
            "END-OF-DOCUMENT               {}",
            "PARSED 000000000",
        ]
    );
}

#[test]
fn end_of_input_takes_the_next_segment_when_xml_code_is_one() {
    let data = "       01  SEGMENTS VALUE '<a><b>Hello, world</b></a>  '.\n           05 SEG PIC X(14) OCCURS 2.\n       01  I PIC 9 VALUE 1.\n";
    let handler = [
        "DISPLAY XML-EVENT(1:20) XML-INFORMATION ' '\n    LENGTH OF XML-TEXT ' {' XML-TEXT '}'",
        "IF XML-EVENT = 'END-OF-INPUT' AND I = 1\n    ADD 1 TO I\n    MOVE 1 TO XML-CODE\nEND-IF.",
    ];
    let out = parse(data, &["XML PARSE SEG(I) PROCESSING PROCEDURE P"], &handler);
    assert_eq!(
        trimmed(&out),
        [
            "START-OF-DOCUMENT   000000000 000000000 {}",
            "START-OF-ELEMENT    000000000 000000001 {a}",
            "START-OF-ELEMENT    000000000 000000001 {b}",
            "CONTENT-CHARACTERS  000000002 000000008 {Hello, w}",
            "END-OF-INPUT        000000000 000000000 {}",
            "CONTENT-CHARACTERS  000000001 000000004 {orld}",
            "END-OF-ELEMENT      000000000 000000001 {b}",
            "END-OF-ELEMENT      000000000 000000001 {a}",
            "END-OF-DOCUMENT     000000000 000000000 {}",
        ]
    );
}

#[test]
fn an_exception_passes_the_xmlss_code_and_ends_the_parse() {
    let data = "       01  DOC PIC X(20) VALUE '<msg>Hello</mmsg>'.\n";
    let out = parse(
        data,
        &["XML PARSE DOC PROCESSING PROCEDURE P\n    ON EXCEPTION DISPLAY 'FAILED ' XML-CODE\n    NOT ON EXCEPTION DISPLAY 'PARSED'\nEND-XML"],
        &["IF XML-EVENT = 'EXCEPTION'\n    DISPLAY XML-CODE ' ' XML-TEXT\nEND-IF."],
    );
    assert_eq!(trimmed(&out), ["000798773 <msg>Hello", "FAILED 000798773"]);
}

#[test]
fn a_processing_procedure_ends_the_parse_by_setting_xml_code_to_minus_one() {
    let data = "       01  DOC PIC X(20) VALUE '<a><b/><c/></a>'.\n";
    let out = parse(
        data,
        &["XML PARSE DOC PROCESSING PROCEDURE P\n    ON EXCEPTION DISPLAY 'STOPPED ' XML-CODE\nEND-XML"],
        &["DISPLAY XML-TEXT", "IF XML-TEXT = 'b' MOVE -1 TO XML-CODE END-IF."],
    );
    assert_eq!(trimmed(&out), ["", "a", "b", "STOPPED 00000000J"]);
}

#[test]
fn namespaces_and_national_text_fill_their_registers() {
    let data = "       01  DOC PIC X(40) VALUE '<p:a xmlns:p=\"urn:x\">&#x4E2D;</p:a>'.\n       01  N PIC N(10).\n";
    let handler = ["DISPLAY XML-EVENT(1:12) '|' XML-NAMESPACE-PREFIX '|'\n    XML-NAMESPACE '|' LENGTH OF XML-NTEXT."];
    let out = parse(data, &["XML PARSE DOC PROCESSING PROCEDURE P"], &handler);
    assert_eq!(trimmed(&out)[1..5], ["START-OF-ELE|p|urn:x|000000000", "NAMESPACE-DE|p|urn:x|000000000", "CONTENT-NATI|||000000002", "END-OF-ELEME|p|urn:x|000000000"]);
    let national = parse(data, &["XML PARSE DOC RETURNING NATIONAL PROCESSING PROCEDURE P"], &["MOVE SPACES TO N", "MOVE XML-NTEXT TO N", "DISPLAY FUNCTION DISPLAY-OF(N) '|' LENGTH OF XML-TEXT."]);
    assert_eq!(trimmed(&national)[1], "a         |000000000");
}

#[test]
fn the_parse_goes_on_past_an_undeclared_prefix_only_when_xml_code_is_reset() {
    let data = "       01  DOC PIC X(28) VALUE '<q:a><b q:c=\"1\">x</b></q:a>'.\n       01  GO-ON PIC 9.\n       01  C PIC 9(9).\n";
    let handler = [
        "MOVE XML-CODE TO C",
        "DISPLAY XML-EVENT(1:20) C ' ' XML-TEXT",
        "IF XML-EVENT = 'EXCEPTION' AND GO-ON = 1\n    MOVE 0 TO XML-CODE\nEND-IF.",
    ];
    let main = [
        "MOVE 1 TO GO-ON",
        "XML PARSE DOC PROCESSING PROCEDURE P\n    NOT ON EXCEPTION DISPLAY 'DONE'\nEND-XML",
        "MOVE 0 TO GO-ON",
        "XML PARSE DOC PROCESSING PROCEDURE P\n    ON EXCEPTION MOVE XML-CODE TO C\n    DISPLAY 'STOPPED ' C\nEND-XML",
    ];
    assert_eq!(
        trimmed(&parse(data, &main, &handler)),
        [
            "START-OF-DOCUMENT   000000000",
            "EXCEPTION           000264193 q:a",
            "START-OF-ELEMENT    000000000 a",
            "START-OF-ELEMENT    000000000 b",
            "EXCEPTION           000264192 q:c",
            "ATTRIBUTE-NAME      000000000 c",
            "ATTRIBUTE-CHARACTERS000000000 1",
            "CONTENT-CHARACTERS  000000000 x",
            "END-OF-ELEMENT      000000000 b",
            "END-OF-ELEMENT      000000000 a",
            "END-OF-DOCUMENT     000000000",
            "DONE",
            "START-OF-DOCUMENT   000000000",
            "EXCEPTION           000264193 q:a",
            "STOPPED 000264193",
        ]
    );
}

#[test]
fn content_a_segment_ends_is_closed_by_a_zero_length_piece_when_markup_follows() {
    let data = "       01  SEGMENTS VALUE '<a>Hello</a>    '.\n           05 SEG PIC X(8) OCCURS 2.\n       01  I PIC 9 VALUE 1.\n";
    let handler = [
        "DISPLAY XML-EVENT(1:20) XML-INFORMATION ' '\n    LENGTH OF XML-TEXT ' {' XML-TEXT '}'",
        "IF XML-EVENT = 'END-OF-INPUT' AND I = 1\n    ADD 1 TO I\n    MOVE 1 TO XML-CODE\nEND-IF.",
    ];
    assert_eq!(
        trimmed(&parse(data, &["XML PARSE SEG(I) PROCESSING PROCEDURE P"], &handler))[2..6],
        [
            "CONTENT-CHARACTERS  000000002 000000005 {Hello}",
            "END-OF-INPUT        000000000 000000000 {}",
            "CONTENT-CHARACTERS  000000001 000000000 {}",
            "END-OF-ELEMENT      000000000 000000001 {a}",
        ]
    );
}

#[test]
fn a_utf_8_character_split_between_segments_comes_whole() {
    let data = "       01  W PIC X(5) VALUE X'3C613EC3'.\n       01  L PIC 9 VALUE 4.\n";
    let handler = [
        "DISPLAY XML-EVENT(1:20) FUNCTION HEX-OF(XML-TEXT)",
        "IF XML-EVENT = 'END-OF-INPUT'\n    MOVE X'A93C2F613E' TO W\n    MOVE 5 TO L\n    MOVE 1 TO XML-CODE\nEND-IF.",
    ];
    let out = parse(data, &["XML PARSE W(1:L) WITH ENCODING 1208\n    PROCESSING PROCEDURE P"], &handler);
    assert_eq!(trimmed(&out)[3..5], ["CONTENT-CHARACTERS  C3A9", "END-OF-ELEMENT      61"]);
}

#[test]
fn ebcdic_new_line_is_white_space() {
    let data = "       01  DOC.\n           05 PIC X(21) VALUE '<?xml version=\"1.0\"?>'.\n           05 PIC X VALUE X'15'.\n           05 PIC X(8) VALUE '<a>x</a>'.\n";
    let out = parse(data, &["XML PARSE DOC PROCESSING PROCEDURE P\n    ON EXCEPTION DISPLAY 'FAILED ' XML-CODE\n    NOT ON EXCEPTION DISPLAY 'PARSED'\nEND-XML"], &["CONTINUE."]);
    assert_eq!(trimmed(&out), ["PARSED"]);
}

#[test]
fn xml_code_after_an_event_follows_table_75() {
    let data = "       01  DOC PIC X(8) VALUE '<a>x</a>'.\n       01  SET-TO PIC S9.\n";
    let main = ["XML PARSE DOC PROCESSING PROCEDURE P\n    ON EXCEPTION DISPLAY 'STOPPED ' XML-CODE\n    NOT ON EXCEPTION DISPLAY 'PARSED'\nEND-XML"];
    let handler = ["IF XML-EVENT = 'END-OF-DOCUMENT'\n    MOVE SET-TO TO XML-CODE\nEND-IF."];
    let source = |value: i8| {
        let data = data.replace("SET-TO PIC S9.", &format!("SET-TO PIC S9 VALUE {value}."));
        let body = |stmts: &[&str]| stmts.iter().flat_map(|s| s.split('\n')).map(line).collect::<String>();
        program("", &data, &format!("       MAIN.\n{}{}       P.\n{}", body(&main), line("GOBACK."), body(&handler)))
    };
    let run = |value: i8| Harness::source(&source(value)).run(Executor::Interpreter);
    assert_eq!(trimmed(&run(-1).out), ["STOPPED 00000000J"]);
    assert_eq!(trimmed(&run(0).out), ["PARSED"]);
    let fatal = run(1).ending.unwrap_err();
    assert_eq!(fatal.code, AbendCode::user(4038));
    assert!(fatal.message.starts_with("IGZ0230S"), "{}", fatal.message);
}

#[test]
fn an_exceptions_text_is_the_current_segment_and_includes_a_duplicate_attribute() {
    let data = "       01  SEGMENTS VALUE '<a>xxxxxy</c>   '.\n           05 SEG PIC X(8) OCCURS 2.\n       01  I PIC 9 VALUE 1.\n       01  DUP PIC X(16) VALUE '<a x=\"1\" x=\"2\"/>'.\n";
    let handler = ["IF XML-EVENT = 'EXCEPTION'\n    DISPLAY XML-TEXT\nEND-IF", "IF XML-EVENT = 'END-OF-INPUT' AND I = 1\n    ADD 1 TO I\n    MOVE 1 TO XML-CODE\nEND-IF."];
    let out = parse(data, &["XML PARSE SEG(I) PROCESSING PROCEDURE P", "XML PARSE DUP PROCESSING PROCEDURE P"], &handler);
    assert_eq!(trimmed(&out), ["y", "<a x=\"1\" x=\"2\""]);
}
