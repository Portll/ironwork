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
            "PARSED 00000000{",
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
            "START-OF-DOCUMENT   00000000{ 000000000 {}",
            "START-OF-ELEMENT    00000000{ 000000001 {a}",
            "START-OF-ELEMENT    00000000{ 000000001 {b}",
            "CONTENT-CHARACTERS  00000000B 000000008 {Hello, w}",
            "END-OF-INPUT        00000000{ 000000000 {}",
            "CONTENT-CHARACTERS  00000000A 000000004 {orld}",
            "END-OF-ELEMENT      00000000{ 000000001 {b}",
            "END-OF-ELEMENT      00000000{ 000000001 {a}",
            "END-OF-DOCUMENT     00000000{ 000000000 {}",
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
    assert_eq!(trimmed(&out), ["00079877C <msg>Hello", "FAILED 00079877C"]);
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
