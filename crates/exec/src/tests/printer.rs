use super::*;

/// A program with the mnemonic-names TOP-OF-PAGE (C01), CHANNEL-2 (C02), NO-SPACE (CSP) and
/// PAGE-MODE (AFP-5A), the print file PRT (DD PRTDD) whose FD is `fd` with the 3-byte record
/// PRT-REC, and N PIC 9 VALUE 5.
fn print_program(card: &str, fd: &str, procedure: &[&str]) -> String {
    let head = [
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. P.",
        "ENVIRONMENT DIVISION.",
        "CONFIGURATION SECTION.",
        "SPECIAL-NAMES.",
        "    C01 IS TOP-OF-PAGE",
        "    C02 IS CHANNEL-2",
        "    CSP IS NO-SPACE",
        "    AFP-5A IS PAGE-MODE.",
        "INPUT-OUTPUT SECTION.",
        "FILE-CONTROL.",
        "    SELECT PRT ASSIGN TO PRTDD.",
        "DATA DIVISION.",
        "FILE SECTION.",
        fd,
        "01  PRT-REC PIC X(3).",
        "WORKING-STORAGE SECTION.",
        "01  N PIC 9 VALUE 5.",
        "PROCEDURE DIVISION.",
    ];
    let card = if card.is_empty() { String::new() } else { format!("       CBL {card}\n") };
    card + &head.iter().chain(procedure).map(|l| format!("       {l}\n")).collect::<String>()
}

/// Runs the program with PRTDD as a file of its own, `:text` when `text`; returns what it
/// displayed and the file.
fn run_print(source: &str, name: &str, text: bool) -> (String, Vec<u8>) {
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
        Err(errors) => errors[0].message.clone(),
        Ok(_) => String::from("(compiled)"),
    }
}

const AFTER_ONLY: &[&str] = &[
    "    OPEN OUTPUT PRT",
    "    MOVE 'ONE' TO PRT-REC WRITE PRT-REC AFTER ADVANCING 1 LINE",
    "    MOVE 'TWO' TO PRT-REC WRITE PRT-REC AFTER 2 LINES",
    "    MOVE 'THR' TO PRT-REC WRITE PRT-REC AFTER ADVANCING 3",
    "    MOVE 'OVR' TO PRT-REC WRITE PRT-REC AFTER ADVANCING 0 LINES",
    "    MOVE 'FIV' TO PRT-REC WRITE PRT-REC AFTER ADVANCING N LINES",
    "    MOVE 'PAG' TO PRT-REC WRITE PRT-REC AFTER ADVANCING PAGE",
    "    MOVE 'TOP' TO PRT-REC WRITE PRT-REC AFTER TOP-OF-PAGE",
    "    MOVE 'CH2' TO PRT-REC WRITE PRT-REC AFTER ADVANCING CHANNEL-2",
    "    MOVE 'NSP' TO PRT-REC WRITE PRT-REC AFTER NO-SPACE",
    "    MOVE 'DEF' TO PRT-REC WRITE PRT-REC",
    "    MOVE 'AFP' TO PRT-REC WRITE PRT-REC AFTER PAGE-MODE",
    "    CLOSE PRT",
];

#[test]
fn a_file_written_only_after_advancing_carries_asa_characters_before_each_record() {
    let procedure = [AFTER_ONLY, &["    OPEN INPUT PRT", "    READ PRT", "    DISPLAY PRT-REC", "    READ PRT", "    DISPLAY PRT-REC", "    CLOSE PRT", "    GOBACK."]].concat();
    let (out, file) = run_print(&print_program("", "FD  PRT.", &procedure), "asa.dat", false);
    let expected = [
        (0x40, "ONE"),
        (0xF0, "TWO"),
        (0x60, "THR"),
        (0x4E, "OVR"),
        (0x60, "   "),
        (0xF0, "FIV"),
        (0xF1, "PAG"),
        (0xF1, "TOP"),
        (0xF2, "CH2"),
        (0x4E, "NSP"),
        (0x40, "DEF"),
        (0x5A, "AFP"),
    ];
    assert_eq!(file, records(&expected));
    assert_eq!(out, "ONE\nTWO\n");
}

#[test]
fn a_text_dd_shows_asa_characters_as_line_feeds_form_feeds_and_carriage_returns() {
    let procedure = [AFTER_ONLY, &["    GOBACK."]].concat();
    let (_, file) = run_print(&print_program("", "FD  PRT.", &procedure), "asa.txt", true);
    assert_eq!(String::from_utf8(file).unwrap(), "ONE\n\nTWO\n\n\nTHR\rOVR\n\n\n\n\nFIV\n\u{c}PAG\n\u{c}TOP\nCH2\rNSP\nDEF\nAFP\n");
}

#[test]
fn printing_over_a_blank_line_on_a_text_dd_needs_no_carriage_return() {
    let procedure = [
        "    OPEN OUTPUT PRT",
        "    MOVE 'ONE' TO PRT-REC WRITE PRT-REC AFTER ADVANCING 1",
        "    MOVE SPACES TO PRT-REC WRITE PRT-REC AFTER ADVANCING 1",
        "    MOVE 'TWO' TO PRT-REC WRITE PRT-REC BEFORE ADVANCING 1",
        "    MOVE SPACES TO PRT-REC WRITE PRT-REC BEFORE ADVANCING 0",
        "    MOVE 'SIX' TO PRT-REC WRITE PRT-REC BEFORE NO-SPACE",
        "    CLOSE PRT",
        "    GOBACK.",
    ];
    let (_, file) = run_print(&print_program("", "FD  PRT.", &procedure), "blank.txt", true);
    assert_eq!(String::from_utf8(file).unwrap(), "ONE\nTWO\nSIX\n");
}

const WITH_BEFORE: &[&str] = &[
    "    OPEN OUTPUT PRT",
    "    MOVE 'ONE' TO PRT-REC WRITE PRT-REC BEFORE ADVANCING 1 LINE",
    "    MOVE 'TWO' TO PRT-REC WRITE PRT-REC BEFORE 2 LINES",
    "    MOVE 'FIV' TO PRT-REC WRITE PRT-REC BEFORE ADVANCING N",
    "    MOVE 'PAG' TO PRT-REC WRITE PRT-REC BEFORE ADVANCING PAGE",
    "    MOVE 'TOP' TO PRT-REC WRITE PRT-REC BEFORE TOP-OF-PAGE",
    "    MOVE 'NSP' TO PRT-REC WRITE PRT-REC BEFORE NO-SPACE",
    "    MOVE 'AFT' TO PRT-REC WRITE PRT-REC AFTER ADVANCING 2 LINES",
    "    MOVE 'APG' TO PRT-REC WRITE PRT-REC AFTER ADVANCING PAGE",
    "    MOVE 'DEF' TO PRT-REC WRITE PRT-REC",
    "    MOVE 'ZER' TO PRT-REC WRITE PRT-REC BEFORE ADVANCING 0 LINES",
    "    CLOSE PRT",
    "    GOBACK.",
];

#[test]
fn one_write_before_advancing_makes_every_record_of_the_file_a_machine_code() {
    let (_, file) = run_print(&print_program("", "FD  PRT.", WITH_BEFORE), "machine.dat", false);
    let expected = [
        (0x09, "ONE"),
        (0x11, "TWO"),
        (0x19, "FIV"),
        (0x13, "   "),
        (0x89, "PAG"),
        (0x89, "TOP"),
        (0x01, "NSP"),
        (0x13, "   "),
        (0x01, "AFT"),
        (0x8B, "   "),
        (0x01, "APG"),
        (0x0B, "   "),
        (0x01, "DEF"),
        (0x01, "ZER"),
    ];
    assert_eq!(file, records(&expected));
    let (_, text) = run_print(&print_program("", "FD  PRT.", WITH_BEFORE), "machine.txt", true);
    assert_eq!(String::from_utf8(text).unwrap(), "ONE\nTWO\n\nFIV\n\n\n\n\nPAG\n\u{c}TOP\n\u{c}NSP\n\nAFT\n\u{c}APG\nDEF\rZER\n");
}

#[test]
fn under_noadv_the_control_character_is_the_records_first_byte() {
    let procedure = [
        "    OPEN OUTPUT PRT",
        "    MOVE 'XAB' TO PRT-REC WRITE PRT-REC AFTER ADVANCING 2 LINES",
        "    DISPLAY PRT-REC",
        "    MOVE 'XCD' TO PRT-REC WRITE PRT-REC AFTER ADVANCING N LINES",
        "    MOVE 'XEF' TO PRT-REC WRITE PRT-REC",
        "    CLOSE PRT",
        "    GOBACK.",
    ];
    let source = print_program("NOADV", "FD  PRT.", &procedure);
    let (out, file) = run_print(&source, "noadv.dat", false);
    assert_eq!(file, records(&[(0xF0, "AB"), (0x60, "  "), (0xF0, "CD"), (0x40, "EF")]));
    assert_eq!(out, "0AB\n");
    let (_, text) = run_print(&source, "noadv.txt", true);
    assert_eq!(String::from_utf8(text).unwrap(), "\nAB\n\n\n\n\nCD\nEF\n");
}

#[test]
fn a_linage_file_carries_the_byte_under_noadv_and_a_file_never_advanced_carries_none() {
    let source = [
        "       CBL NOADV",
        "       IDENTIFICATION DIVISION.",
        "       PROGRAM-ID. L.",
        "       ENVIRONMENT DIVISION.",
        "       INPUT-OUTPUT SECTION.",
        "       FILE-CONTROL.",
        "           SELECT PLAIN ASSIGN TO PLAINDD.",
        "           SELECT PAGED ASSIGN TO PAGEDDD.",
        "       DATA DIVISION.",
        "       FILE SECTION.",
        "       FD  PLAIN.",
        "       01  PLAIN-REC PIC X(2).",
        "       FD  PAGED LABEL RECORDS STANDARD",
        "           LINAGE IS 60 LINES WITH FOOTING AT 55.",
        "       01  PAGED-REC PIC X(2).",
        "       PROCEDURE DIVISION.",
        "           OPEN OUTPUT PLAIN PAGED",
        "           MOVE 'AB' TO PLAIN-REC PAGED-REC",
        "           WRITE PLAIN-REC",
        "           WRITE PAGED-REC",
        "           CLOSE PLAIN PAGED",
        "           GOBACK.",
    ]
    .map(|l| format!("{l}\n"))
    .concat();
    let (plain, paged) = (temp("plain.dat"), temp("paged.dat"));
    let (_, err, ending) = run_files(&source, &[format!("PLAINDD={}", plain.display()), format!("PAGEDDD={}", paged.display())]);
    assert!(ending.is_ok(), "{ending:?}\n{err}");
    assert_eq!(std::fs::read(&plain).unwrap(), [0xC1, 0xC2]);
    assert_eq!(std::fs::read(&paged).unwrap(), [0x40, 0xC1, 0xC2]);
}

#[test]
fn a_variable_length_print_record_has_the_control_character_after_its_rdw() {
    let source = [
        "       IDENTIFICATION DIVISION.",
        "       PROGRAM-ID. V.",
        "       ENVIRONMENT DIVISION.",
        "       INPUT-OUTPUT SECTION.",
        "       FILE-CONTROL.",
        "           SELECT PRT ASSIGN TO PRTDD.",
        "       DATA DIVISION.",
        "       FILE SECTION.",
        "       FD  PRT RECORDING MODE IS V.",
        "       01  SHORT-REC PIC X(2).",
        "       01  LONG-REC PIC X(5).",
        "       PROCEDURE DIVISION.",
        "           OPEN OUTPUT PRT",
        "           MOVE 'AB' TO SHORT-REC WRITE SHORT-REC AFTER ADVANCING 1",
        "           MOVE 'CDEFG' TO LONG-REC WRITE LONG-REC AFTER ADVANCING 2",
        "           CLOSE PRT",
        "           OPEN INPUT PRT",
        "           READ PRT READ PRT",
        "           DISPLAY LONG-REC",
        "           CLOSE PRT",
        "           GOBACK.",
    ]
    .map(|l| format!("{l}\n"))
    .concat();
    let (out, file) = run_print(&source, "vba.dat", false);
    assert_eq!(file, [0, 7, 0, 0, 0x40, 0xC1, 0xC2, 0, 10, 0, 0, 0xF0, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7]);
    assert_eq!(out, "CDEFG\n");
}

#[test]
fn advancing_the_language_reference_does_not_allow_is_refused() {
    let with = |select: &str, fd: &str, write: &str| {
        [
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. R.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n       SPECIAL-NAMES. S01 IS POCKET-1.\n",
            "       INPUT-OUTPUT SECTION.\n       FILE-CONTROL.\n",
            &format!("           SELECT F ASSIGN TO FDD {select}.\n"),
            &format!("       DATA DIVISION.\n       FILE SECTION.\n       FD  F.\n       01  F-REC.\n           05 F-KEY PIC X.\n{fd}"),
            &format!("       PROCEDURE DIVISION.\n           {write}\n           GOBACK.\n"),
        ]
        .concat()
    };
    let cases = [
        (with("ORGANIZATION INDEXED RECORD KEY F-KEY", "", "WRITE F-REC AFTER ADVANCING 1 LINE"), "F is not a sequential file"),
        (with("ORGANIZATION LINE SEQUENTIAL", "", "WRITE F-REC BEFORE ADVANCING 1 LINE"), "line-sequential file F"),
        (with("", "", "WRITE F-REC AFTER POCKET-1"), "stacker selection (S01)"),
        (with("", "", "WRITE F-REC AFTER ADVANCING NOSUCH LINES"), "NOSUCH"),
    ];
    for (source, expected) in cases {
        let message = refusal(&source);
        assert!(message.contains(expected), "expected {expected:?}, got {message}");
    }
}
