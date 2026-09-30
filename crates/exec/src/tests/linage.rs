use super::*;

/// A program whose print file PRT (DD PRTDD, FILE STATUS FS) has the FD `fd` and the 3-byte record
/// PRT-REC, with FS PIC XX and WORKING-STORAGE `data`.
fn linage_program(fd: &str, data: &str, procedure: &[&str]) -> String {
    let head = [
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. L.",
        "ENVIRONMENT DIVISION.",
        "INPUT-OUTPUT SECTION.",
        "FILE-CONTROL.",
        "    SELECT PRT ASSIGN TO PRTDD FILE STATUS FS.",
        "DATA DIVISION.",
        "FILE SECTION.",
        fd,
        "01  PRT-REC PIC X(3).",
        "WORKING-STORAGE SECTION.",
        "01  FS PIC XX.",
        data,
        "PROCEDURE DIVISION.",
    ];
    head.iter().chain(procedure).filter(|l| !l.is_empty()).map(|l| format!("       {l}\n")).collect()
}

/// Runs the program with PRTDD as a file of its own, `:text` when `text`; returns what it
/// displayed and the file.
fn run_linage(source: &str, name: &str, text: bool) -> (String, Vec<u8>) {
    let path = temp(name);
    let _ = std::fs::remove_file(&path);
    let dd = format!("PRTDD={}{}", path.display(), if text { ":text" } else { "" });
    let (out, err, ending) = run_files(source, &[dd]);
    assert!(ending.is_ok(), "{ending:?}\n{err}");
    (out, std::fs::read(&path).unwrap_or_default())
}

/// Each record: its control character, then the EBCDIC of `text`.
fn records(expected: &[(u8, &str)]) -> Vec<u8> {
    let page = zarch::ebcdic::CodePage::by_ccsid(1140).unwrap();
    expected.iter().flat_map(|(control, text)| std::iter::once(*control).chain(page.encode(text).unwrap())).collect()
}

fn refusal(source: &str) -> String {
    match compile(syntax::parse(source).unwrap_or_else(|e| panic!("{e}")), &[]) {
        Err(errors) => errors.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n"),
        Ok(_) => String::from("(compiled)"),
    }
}

/// Writes L1 to L6 after one line each, the sixth after two, showing whether each raised the
/// end-of-page condition and the LINAGE-COUNTER it left.
const SIX_LINES: &[&str] = &[
    "    DISPLAY LINAGE-COUNTER",
    "    OPEN OUTPUT PRT",
    "    DISPLAY LINAGE-COUNTER",
    "    MOVE 'L1' TO PRT-REC PERFORM W",
    "    MOVE 'L2' TO PRT-REC PERFORM W",
    "    MOVE 'L3' TO PRT-REC PERFORM W",
    "    MOVE 'L4' TO PRT-REC PERFORM W",
    "    MOVE 'L5' TO PRT-REC PERFORM W",
    "    MOVE 'L6' TO PRT-REC",
    "    WRITE PRT-REC AFTER ADVANCING 2 LINES",
    "        AT END-OF-PAGE DISPLAY 'E' LINAGE-COUNTER",
    "        NOT AT END-OF-PAGE DISPLAY 'N' LINAGE-COUNTER",
    "    END-WRITE",
    "    CLOSE PRT",
    "    DISPLAY LINAGE-COUNTER",
    "    GOBACK.",
    "W.",
    "    WRITE PRT-REC AT EOP DISPLAY 'E' LINAGE-COUNTER",
    "        NOT EOP DISPLAY 'N' LINAGE-COUNTER.",
];

const MARGINS: &str = "FD  PRT LINAGE 4 WITH FOOTING AT 3\n           LINES AT TOP 1 LINES AT BOTTOM 2.";

#[test]
fn a_page_body_with_footing_and_margins_counts_its_lines_and_spaces_past_the_margins() {
    let (out, file) = run_linage(&linage_program(MARGINS, "", SIX_LINES), "margins.dat", false);
    assert_eq!(out, "0\n1\nN2\nE3\nE4\nE1\nN2\nE4\n4\n");
    let expected = [(0xF0, "L1 "), (0x40, "L2 "), (0x40, "L3 "), (0x60, "   "), (0x40, "L4 "), (0x40, "L5 "), (0xF0, "L6 ")];
    assert_eq!(file, records(&expected));
}

#[test]
fn a_text_dd_shows_the_margins_as_blank_lines() {
    let (_, text) = run_linage(&linage_program(MARGINS, "", SIX_LINES), "margins.txt", true);
    assert_eq!(String::from_utf8(text).unwrap(), "\nL1\nL2\nL3\n\n\n\nL4\nL5\n\nL6\n");
}

#[test]
fn write_before_advancing_prints_then_spaces_past_the_page_in_machine_codes() {
    let procedure = [
        "    OPEN OUTPUT PRT",
        "    MOVE 'B1' TO PRT-REC WRITE PRT-REC BEFORE ADVANCING 1 LINE",
        "    MOVE 'B2' TO PRT-REC WRITE PRT-REC BEFORE ADVANCING 1 LINE",
        "    MOVE 'B3' TO PRT-REC WRITE PRT-REC BEFORE ADVANCING 1 LINE",
        "    DISPLAY LINAGE-COUNTER",
        "    MOVE 'A4' TO PRT-REC WRITE PRT-REC AFTER ADVANCING 1 LINE",
        "    CLOSE PRT",
        "    GOBACK.",
    ];
    let source = linage_program("FD  PRT LINAGE IS 3 LINES LINES AT TOP 2.", "", &procedure);
    let (out, file) = run_linage(&source, "before.dat", false);
    assert_eq!(out, "1\n");
    assert_eq!(file, records(&[(0x13, "   "), (0x09, "B1 "), (0x09, "B2 "), (0x19, "B3 "), (0x0B, "   "), (0x01, "A4 ")]));
    let (_, text) = run_linage(&source, "before.txt", true);
    assert_eq!(String::from_utf8(text).unwrap(), "\nB1\nB2\nB3\n\n\n\nA4\n");
}

#[test]
fn advancing_page_moves_in_lines_and_each_page_takes_the_data_items_as_they_stand() {
    let data = "01  BODY-N PIC 99 VALUE 5.\n       01  FOOT-N PIC 9 VALUE 4.\n       01  TOP-N PIC 9 VALUE 0.\n       01  BOT-N PIC 9 VALUE 1.";
    let procedure = [
        "    OPEN OUTPUT PRT",
        "    MOVE 'P1' TO PRT-REC WRITE PRT-REC AFTER 2",
        "    MOVE 3 TO BODY-N MOVE 2 TO TOP-N FOOT-N",
        "    MOVE 'P2' TO PRT-REC WRITE PRT-REC AFTER ADVANCING PAGE",
        "        AT END-OF-PAGE DISPLAY 'EOP AFTER PAGE'",
        "    END-WRITE",
        "    DISPLAY LINAGE-COUNTER",
        "    MOVE 'P3' TO PRT-REC",
        "    WRITE PRT-REC AT EOP DISPLAY 'EOP ' LINAGE-COUNTER END-WRITE",
        "    MOVE 'P5' TO PRT-REC WRITE PRT-REC AFTER 2 LINES",
        "    DISPLAY LINAGE-COUNTER",
        "    CLOSE PRT",
        "    GOBACK.",
    ];
    let fd = "FD  PRT LINAGE IS BODY-N LINES WITH FOOTING AT FOOT-N\n           LINES AT TOP TOP-N LINES AT BOTTOM BOT-N.";
    let (out, file) = run_linage(&linage_program(fd, data, &procedure), "page.dat", false);
    assert_eq!(out, "01\nEOP 02\n01\n");
    assert_eq!(file, records(&[(0xF0, "P1 "), (0x60, "   "), (0x60, "P2 "), (0x40, "P3 "), (0x60, "   "), (0xF0, "P5 ")]));
}

#[test]
fn without_footing_only_a_page_overflow_raises_end_of_page() {
    let procedure = [
        "    OPEN OUTPUT PRT",
        "    PERFORM 3 TIMES",
        "        WRITE PRT-REC AT END-OF-PAGE DISPLAY 'E' LINAGE-COUNTER",
        "            NOT AT END-OF-PAGE DISPLAY 'N' LINAGE-COUNTER",
        "        END-WRITE",
        "    END-PERFORM",
        "    CLOSE PRT",
        "    GOBACK.",
    ];
    let (out, _) = run_linage(&linage_program("FD  PRT LINAGE 2.", "", &procedure), "nofooting.dat", false);
    assert_eq!(out, "N2\nE1\nN2\n");
}

#[test]
fn records_and_linage_counters_are_qualified_by_their_file_names() {
    let source = [
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. Q.\n       ENVIRONMENT DIVISION.\n       INPUT-OUTPUT SECTION.\n",
        "       FILE-CONTROL.\n           SELECT A ASSIGN TO ADD.\n           SELECT B ASSIGN TO BDD.\n",
        "       DATA DIVISION.\n       FILE SECTION.\n",
        "       FD  A LINAGE 60.\n       01  REC.\n           05 FLD PIC X(2).\n",
        "       FD  B LINAGE IS SIZE-B LINES.\n       01  REC.\n           05 FLD PIC X(2).\n",
        "       WORKING-STORAGE SECTION.\n       01  SIZE-B PIC 9(3) PACKED-DECIMAL VALUE 100.\n",
        "       PROCEDURE DIVISION.\n           OPEN OUTPUT A B\n",
        "           MOVE 'AA' TO FLD OF REC IN A MOVE 'BB' TO FLD IN B\n",
        "           WRITE REC IN A AFTER 3 WRITE REC OF B\n",
        "           DISPLAY LINAGE-COUNTER OF A ' ' LINAGE-COUNTER IN B\n",
        "           CLOSE A B\n           GOBACK.\n",
    ]
    .concat();
    let (a, b) = (temp("qualified-a.dat"), temp("qualified-b.dat"));
    let (out, err, ending) = run_files(&source, &[format!("ADD={}", a.display()), format!("BDD={}", b.display())]);
    assert!(ending.is_ok(), "{ending:?}\n{err}");
    assert_eq!(out, "04 002\n");
    assert_eq!(std::fs::read(&a).unwrap(), records(&[(0x60, "AA")]));
    assert_eq!(std::fs::read(&b).unwrap(), records(&[(0x40, "BB")]));
    let unqualified = source.replace("LINAGE-COUNTER OF A ' '", "LINAGE-COUNTER ' '");
    assert!(refusal(&unqualified).contains("LINAGE-COUNTER is ambiguous"), "{}", refusal(&unqualified));
    let wrong_file = source.replace("WRITE REC IN A", "WRITE FLD IN REC IN PRT");
    assert!(refusal(&wrong_file).contains("FLD is not defined"), "{}", refusal(&wrong_file));
}

#[test]
fn what_the_language_reference_forbids_of_linage_is_refused() {
    let with = |select: &str, fd: &str, data: &str, statement: &str| {
        [
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. R.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n",
            "       SPECIAL-NAMES. C01 IS TOP-OF-FORM.\n       INPUT-OUTPUT SECTION.\n       FILE-CONTROL.\n",
            &format!("           SELECT F ASSIGN TO FDD\n               {select}.\n           SELECT P ASSIGN TO PDD.\n"),
            &format!("       DATA DIVISION.\n       FILE SECTION.\n       FD  F {fd}.\n       01  F-REC.\n           05 F-KEY PIC X.\n"),
            "       FD  P.\n       01  P-REC PIC X.\n",
            &format!("       WORKING-STORAGE SECTION.\n       01  W PIC S99 VALUE 5.\n       01  X PIC X.\n{data}"),
            &format!("       PROCEDURE DIVISION.\n           {statement}\n           GOBACK.\n"),
        ]
        .concat()
    };
    let cases = [
        (with("ORGANIZATION INDEXED RECORD KEY F-KEY", "LINAGE 5", "", "CONTINUE"), "F: LINAGE is for a sequential file, not an indexed or relative one"),
        (with("ORGANIZATION LINE SEQUENTIAL", "LINAGE 5", "", "CONTINUE"), "F: LINAGE is for a sequential file, not a line-sequential one"),
        (with("", "LINAGE 0", "", "CONTINUE"), "the page body needs at least one line"),
        (with("", "LINAGE 5 FOOTING 6", "", "CONTINUE"), "FOOTING 6 is past the page body of 5 lines"),
        (with("", "LINAGE 5 FOOTING 0", "", "CONTINUE"), "FOOTING 0"),
        (with("", "LINAGE 123456789", "", "CONTINUE"), "more than the 99999999 lines LINAGE allows"),
        (with("", "LINAGE W", "", "CONTINUE"), "F: LINAGE W is not an unsigned integer data item"),
        (with("", "LINAGE 5 TOP X", "", "CONTINUE"), "F: TOP X is not an unsigned integer data item"),
        (with("", "LINAGE 5 BOTTOM NOSUCH", "", "CONTINUE"), "NOSUCH is not defined"),
        (with("", "LINAGE 5 REPORT IS RPT", "", "CONTINUE"), "F: LINAGE on a report file is not supported yet"),
        (with("", "", "", "WRITE F-REC AT END-OF-PAGE CONTINUE"), "WRITE ... END-OF-PAGE: the FD of F has no LINAGE clause"),
        (with("", "LINAGE 5", "", "WRITE F-REC AFTER TOP-OF-FORM"), "ADVANCING TOP-OF-FORM on F, whose FD has LINAGE, is not supported yet"),
        (with("", "LINAGE 5", "", "MOVE 1 TO LINAGE-COUNTER"), "LINAGE-COUNTER can be read, but no statement can change it"),
        (with("", "LINAGE 5", "", "ADD 1 TO LINAGE-COUNTER OF F"), "no statement can change it"),
        (with("", "", "", "DISPLAY LINAGE-COUNTER"), "LINAGE-COUNTER is not defined"),
    ];
    for (source, expected) in cases {
        let message = refusal(&source);
        assert!(message.contains(expected), "expected {expected:?}, got {message}");
    }
}

#[test]
fn a_page_its_data_items_cannot_make_ends_the_run() {
    let procedure = ["    MOVE 9 TO FOOT-N", "    OPEN OUTPUT PRT", "    GOBACK."];
    let source = linage_program("FD  PRT LINAGE 5 FOOTING FOOT-N.", "01  FOOT-N PIC 9 VALUE 1.", &procedure);
    let (_, _, ending) = run_files(&source, &[format!("PRTDD={}", temp("bad-footing.dat").display())]);
    let abend = ending.unwrap_err();
    assert!(abend.message.contains("PRT: LINAGE puts the footing at line 9, outside the page body of 5 lines (C73)"), "{}", abend.message);
}

#[test]
fn extend_starts_a_new_page_top_margin_first() {
    let procedure = [
        "    OPEN OUTPUT PRT",
        "    MOVE 'A1' TO PRT-REC WRITE PRT-REC",
        "    CLOSE PRT",
        "    OPEN EXTEND PRT",
        "    DISPLAY LINAGE-COUNTER",
        "    MOVE 'E1' TO PRT-REC WRITE PRT-REC",
        "    CLOSE PRT",
        "    OPEN INPUT PRT",
        "    DISPLAY LINAGE-COUNTER",
        "    CLOSE PRT",
        "    GOBACK.",
    ];
    let (out, file) = run_linage(&linage_program("FD  PRT LINAGE 5 TOP 2.", "", &procedure), "extend.dat", false);
    assert_eq!(out, "1\n1\n");
    assert_eq!(file, records(&[(0x60, "A1 "), (0x60, "E1 ")]));
}

#[test]
fn a_failed_write_runs_neither_end_of_page_phrase_and_leaves_the_counter() {
    let procedure = [
        "    OPEN OUTPUT PRT",
        "    WRITE PRT-REC",
        "    CLOSE PRT",
        "    WRITE PRT-REC AT EOP DISPLAY 'EOP'",
        "        NOT AT EOP DISPLAY 'NOT EOP' END-WRITE",
        "    DISPLAY FS ' ' LINAGE-COUNTER",
        "    GOBACK.",
    ];
    let (out, _) = run_linage(&linage_program("FD  PRT LINAGE 5 FOOTING 1.", "", &procedure), "failed.dat", false);
    assert_eq!(out, "48 2\n");
}

#[test]
fn a_print_file_opened_i_o_reads_past_its_control_bytes_and_keeps_them_on_rewrite() {
    let procedure = [
        "    OPEN OUTPUT PRT",
        "    MOVE 'ONE' TO PRT-REC WRITE PRT-REC AFTER ADVANCING 2 LINES",
        "    MOVE 'TWO' TO PRT-REC WRITE PRT-REC AFTER ADVANCING 1 LINE",
        "    CLOSE PRT",
        "    OPEN I-O PRT",
        "    READ PRT DISPLAY PRT-REC",
        "    READ PRT DISPLAY PRT-REC",
        "    MOVE 'NEW' TO PRT-REC REWRITE PRT-REC DISPLAY FS",
        "    WRITE PRT-REC AFTER ADVANCING 1 LINE DISPLAY FS",
        "    CLOSE PRT",
        "    GOBACK.",
    ];
    let (out, file) = run_linage(&linage_program("FD  PRT.", "", &procedure), "update.dat", false);
    assert_eq!(out, "ONE\nTWO\n00\n48\n");
    assert_eq!(file, records(&[(0xF0, "ONE"), (0x40, "NEW")]));
    let (_, file) = run_linage(&linage_program("FD  PRT LINAGE 9.", "", &procedure), "update-linage.dat", false);
    assert_eq!(file, records(&[(0xF0, "ONE"), (0x40, "NEW")]));
}
