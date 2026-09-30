use super::*;

/// Fixed-format lines, each after the seven columns before area A and ending by column 72.
fn cobol(lines: &[&str]) -> String {
    lines
        .iter()
        .map(|l| {
            assert!(l.len() <= 65, "{l:?} runs past column 72");
            format!("       {l}\n")
        })
        .collect()
}

/// A program whose file RPT (DD RPTDD) is described by `fd`, with WORKING-STORAGE `data`, the
/// REPORT SECTION `rd` and the PROCEDURE DIVISION `procedure`.
fn report_program(fd: &str, data: &[&str], rd: &[&str], procedure: &[&str]) -> String {
    [
        cobol(&["IDENTIFICATION DIVISION.", "PROGRAM-ID. R.", "ENVIRONMENT DIVISION.", "INPUT-OUTPUT SECTION.", "FILE-CONTROL.", "    SELECT RPT ASSIGN TO RPTDD.", "DATA DIVISION.", "FILE SECTION.", fd, "WORKING-STORAGE SECTION."]),
        cobol(data),
        cobol(&["REPORT SECTION."]),
        cobol(rd),
        cobol(&["PROCEDURE DIVISION."]),
        cobol(procedure),
    ]
    .concat()
}

/// Runs the program with RPTDD as a file of its own; returns what it displayed and the file.
fn run_report(source: &str, name: &str, text: bool) -> (String, Vec<u8>) {
    let path = temp(name);
    let _ = std::fs::remove_file(&path);
    let dd = format!("RPTDD={}{}", path.display(), if text { ":text" } else { "" });
    let (out, err, ending) = run_files(source, &[dd]);
    assert!(ending.is_ok(), "{ending:?}\n{err}");
    (out, std::fs::read(&path).unwrap_or_default())
}

fn text(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap()
}

/// The first error the front end or the compiler gives.
fn refusal(source: &str) -> String {
    match syntax::parse(source) {
        Err(e) => e.message,
        Ok(p) => match compile(p, &[]) {
            Err(errors) => errors[0].message.clone(),
            Ok(_) => String::from("(compiled)"),
        },
    }
}

#[test]
fn a_report_heading_page_heading_and_details() {
    let source = report_program(
        "FD  RPT REPORT IS LISTING.",
        &["01  N PIC 99 VALUE 0.", "01  NAME-X PIC X(6)."],
        &[
            "RD  LISTING PAGE LIMIT 10 FIRST DETAIL 4.",
            "01  TYPE REPORT HEADING.",
            "    05 LINE 1 COLUMN 10 PIC X(16) VALUE 'CUSTOMER LISTING'.",
            "01  TYPE PAGE HEADING.",
            "    05 LINE PLUS 1.",
            "       10 COLUMN 1 VALUE 'NO'.",
            "       10 COLUMN 5 VALUE 'NAME'.",
            "       10 COLUMN 16 VALUE 'PAGE'.",
            "       10 COLUMN PLUS 2 PIC Z9 SOURCE PAGE-COUNTER.",
            "01  CUSTOMER TYPE DETAIL LINE PLUS 1.",
            "    05 COLUMN 1 PIC Z9 SOURCE N.",
            "    05 COLUMN 5 PIC X(6) SOURCE NAME-X.",
        ],
        &[
            "    OPEN OUTPUT RPT",
            "    INITIATE LISTING",
            "    MOVE 'ALPHA' TO NAME-X ADD 1 TO N GENERATE CUSTOMER",
            "    MOVE 'BETA' TO NAME-X ADD 1 TO N GENERATE CUSTOMER",
            "    MOVE 'GAMMA' TO NAME-X ADD 1 TO N GENERATE CUSTOMER",
            "    TERMINATE LISTING",
            "    CLOSE RPT",
            "    MOVE LINE-COUNTER TO N",
            "    DISPLAY N",
            "    GOBACK.",
        ],
    );
    let (out, file) = run_report(&source, "rw-listing.txt", true);
    assert_eq!(text(file), "\u{c}         CUSTOMER LISTING\nNO  NAME       PAGE  1\n\n 1  ALPHA\n 2  BETA\n 3  GAMMA\n");
    assert_eq!(out, "06\n");
}

#[test]
fn control_breaks_on_two_levels_with_sum_rolling_forward_and_reset() {
    let source = report_program(
        "FD  RPT REPORT IS SALES.",
        &["01  REGION PIC X.", "01  CITY PIC X.", "01  AMT PIC 999."],
        &[
            "RD  SALES CONTROLS ARE FINAL REGION CITY PAGE LIMIT 60.",
            "01  SALE TYPE DETAIL LINE PLUS 1.",
            "    05 COLUMN 1 PIC X SOURCE REGION.",
            "    05 COLUMN 3 PIC X SOURCE CITY.",
            "    05 R-AMT COLUMN 5 PIC ZZ9 SOURCE AMT.",
            "01  TYPE CONTROL FOOTING CITY LINE PLUS 1.",
            "    05 COLUMN 3 VALUE 'T'.",
            "    05 CT COLUMN 5 PIC ZZZ9 SUM R-AMT.",
            "    05 COLUMN 10 PIC ZZZ9 SUM R-AMT RESET ON REGION.",
            "01  TYPE CONTROL FOOTING REGION LINE PLUS 1.",
            "    05 COLUMN 1 PIC X SOURCE REGION.",
            "    05 COLUMN 3 VALUE 'TOTAL'.",
            "    05 RT COLUMN 10 PIC ZZZ9 SUM CT.",
            "01  TYPE CONTROL FOOTING FINAL LINE PLUS 2.",
            "    05 COLUMN 1 VALUE 'ALL'.",
            "    05 COLUMN 9 PIC ZZZZ9 SUM RT.",
        ],
        &[
            "    OPEN OUTPUT RPT",
            "    INITIATE SALES",
            "    MOVE 'A' TO REGION MOVE 'X' TO CITY",
            "    MOVE 10 TO AMT GENERATE SALE",
            "    MOVE 20 TO AMT GENERATE SALE",
            "    MOVE 'Y' TO CITY MOVE 5 TO AMT GENERATE SALE",
            "    MOVE 'B' TO REGION MOVE 'Z' TO CITY",
            "    MOVE 7 TO AMT GENERATE SALE",
            "    MOVE 'C' TO REGION",
            "    TERMINATE SALES",
            "    CLOSE RPT",
            "    GOBACK.",
        ],
    );
    let (_, file) = run_report(&source, "rw-sales.txt", true);
    let expected = [
        "\u{c}A X  10", "A X  20", "  T   30   30", "A Y   5", "  T    5   35", "A TOTAL    35", "B Z   7", "  T    7    7", "B TOTAL     7", "", "ALL        42",
    ];
    assert_eq!(text(file), expected.iter().map(|l| format!("{l}\n")).collect::<String>());
}

#[test]
fn page_overflow_brings_page_footing_page_heading_and_page_counter() {
    let source = report_program(
        "FD  RPT REPORT IS LOG.",
        &["01  N PIC 99 VALUE 0."],
        &[
            "RD  LOG PAGE LIMIT 8 HEADING 1 FIRST DETAIL 3 LAST DETAIL 6.",
            "01  TYPE PH LINE 1.",
            "    05 COLUMN 1 VALUE 'HEAD'.",
            "    05 COLUMN 6 PIC 9 SOURCE PAGE-COUNTER.",
            "01  ENTRY-LINE TYPE DE LINE PLUS 1.",
            "    05 COLUMN 1 PIC 99 SOURCE N.",
            "    05 COLUMN 4 PIC 99 SOURCE LINE-COUNTER.",
            "01  TYPE PF LINE 8.",
            "    05 COLUMN 1 VALUE 'FOOT'.",
            "    05 COLUMN 6 PIC 9 SOURCE PAGE-COUNTER.",
        ],
        &[
            "    OPEN OUTPUT RPT",
            "    INITIATE LOG",
            "    PERFORM 6 TIMES ADD 1 TO N GENERATE ENTRY-LINE END-PERFORM",
            "    TERMINATE LOG",
            "    CLOSE RPT",
            "    MOVE PAGE-COUNTER TO N DISPLAY N",
            "    MOVE LINE-COUNTER TO N DISPLAY N",
            "    GOBACK.",
        ],
    );
    let (out, file) = run_report(&source, "rw-log.txt", true);
    assert_eq!(text(file), "\u{c}HEAD 1\n\n01 03\n02 04\n03 05\n04 06\n\nFOOT 1\n\u{c}HEAD 2\n\n05 03\n06 04\n\n\n\nFOOT 2\n");
    assert_eq!(out, "02\n08\n");
}

#[test]
fn group_indicate_shows_again_after_a_control_break_or_a_new_page() {
    let source = report_program(
        "FD  RPT REPORT IS GI.",
        &["01  KEY-X PIC X.", "01  N PIC 9 VALUE 0."],
        &[
            "RD  GI CONTROL IS KEY-X PAGE LIMIT 4.",
            "01  ROW TYPE DETAIL LINE PLUS 1.",
            "    05 COLUMN 1 PIC X SOURCE KEY-X GROUP INDICATE.",
            "    05 COLUMN 3 PIC 9 SOURCE N.",
        ],
        &[
            "    OPEN OUTPUT RPT",
            "    INITIATE GI",
            "    MOVE 'A' TO KEY-X",
            "    PERFORM 3 TIMES ADD 1 TO N GENERATE ROW END-PERFORM",
            "    MOVE 'B' TO KEY-X",
            "    PERFORM 2 TIMES ADD 1 TO N GENERATE ROW END-PERFORM",
            "    TERMINATE GI",
            "    CLOSE RPT",
            "    GOBACK.",
        ],
    );
    let (_, file) = run_report(&source, "rw-gi.txt", true);
    assert_eq!(text(file), "\u{c}A 1\n  2\n  3\nB 4\n\u{c}B 5\n");
}

#[test]
fn summary_reporting_generates_the_report_and_adds_on_every_generate() {
    let source = report_program(
        "FD  RPT REPORT IS PAYROLL.",
        &["01  DEPT PIC X.", "01  SALARY PIC 999."],
        &[
            "RD  PAYROLL CONTROL IS DEPT.",
            "01  TYPE CH DEPT LINE PLUS 1.",
            "    05 COLUMN 1 VALUE 'DEPT'.",
            "    05 COLUMN 6 PIC X SOURCE DEPT.",
            "01  TYPE CF DEPT LINE PLUS 1 NEXT GROUP PLUS 1.",
            "    05 COLUMN 3 VALUE 'PAY'.",
            "    05 COLUMN 7 PIC ZZZ9 SUM SALARY.",
        ],
        &[
            "    OPEN OUTPUT RPT",
            "    INITIATE PAYROLL",
            "    MOVE 'A' TO DEPT MOVE 100 TO SALARY GENERATE PAYROLL",
            "    MOVE 50 TO SALARY GENERATE PAYROLL",
            "    MOVE 'B' TO DEPT MOVE 25 TO SALARY GENERATE PAYROLL",
            "    TERMINATE PAYROLL",
            "    CLOSE RPT",
            "    GOBACK.",
        ],
    );
    let (_, file) = run_report(&source, "rw-summary.txt", true);
    assert_eq!(text(file), "DEPT A\n  PAY  150\n\nDEPT B\n  PAY   25\n");
}

#[test]
fn terminate_ends_the_report_and_a_report_never_generated_writes_nothing() {
    let source = report_program(
        "FD  RPT REPORT IS BOOK.",
        &["01  N PIC 9 VALUE 0."],
        &[
            "RD  BOOK PAGE LIMIT 5.",
            "01  TYPE RH NEXT GROUP NEXT PAGE.",
            "    05 LINE 2 COLUMN 1 VALUE 'TITLE'.",
            "01  TYPE PH LINE 1 COLUMN 1 PIC 9 SOURCE PAGE-COUNTER.",
            "01  ITEM TYPE DE LINE PLUS 1 COLUMN 3 PIC 9 SOURCE N.",
            "01  TYPE RF LINE 3 ON NEXT PAGE COLUMN 1 VALUE 'END'.",
        ],
        &[
            "    OPEN OUTPUT RPT",
            "    INITIATE BOOK",
            "    TERMINATE BOOK",
            "    INITIATE BOOK",
            "    ADD 1 TO N GENERATE ITEM",
            "    ADD 1 TO N GENERATE ITEM",
            "    TERMINATE BOOK",
            "    CLOSE RPT",
            "    MOVE PAGE-COUNTER IN BOOK TO N",
            "    DISPLAY N",
            "    GOBACK.",
        ],
    );
    let (out, file) = run_report(&source, "rw-book.txt", true);
    assert_eq!(text(file), "\u{c}\nTITLE\n\u{c}2\n  1\n  2\n\u{c}\n\nEND\n");
    assert_eq!(out, "3\n");
}

#[test]
fn report_records_are_ebcdic_bytes_with_the_code_and_a_blank_record_at_the_top_of_a_page() {
    let source = report_program(
        "FD  RPT REPORT IS CARDS.",
        &[],
        &["RD  CARDS CODE 'X' PAGE LIMIT 5.", "01  CARD TYPE DE LINE 3.", "    05 COLUMN 1 VALUE 'AB'."],
        &["    OPEN OUTPUT RPT", "    INITIATE CARDS", "    GENERATE CARD", "    GENERATE CARD", "    TERMINATE CARDS", "    CLOSE RPT", "    GOBACK."],
    );
    let (_, file) = run_report(&source, "rw-cards.dat", false);
    let blank = [0xF1, 0x40, 0x40, 0x40, 0x40, 0x40];
    let card = [0xF0, 0xE7, 0xC1, 0xC2, 0x40, 0x40];
    assert_eq!(file, [blank, card, blank, card].concat());
}

#[test]
fn under_noadv_the_report_record_keeps_its_first_byte_for_the_control_character() {
    let cards = |fd: &str| {
        report_program(
            fd,
            &[],
            &["RD  CARDS CODE 'X' PAGE LIMIT 5.", "01  CARD TYPE DE LINE 3.", "    05 COLUMN 1 VALUE 'AB'."],
            &["    OPEN OUTPUT RPT", "    INITIATE CARDS", "    GENERATE CARD", "    TERMINATE CARDS", "    CLOSE RPT", "    GOBACK."],
        )
    };
    let noadv = |source: String| format!("       CBL NOADV\n{source}");
    let (_, derived) = run_report(&noadv(cards("FD  RPT REPORT IS CARDS.")), "rw-noadv.dat", false);
    assert_eq!(derived, [0xF1, 0x40, 0x40, 0x40, 0x40, 0x40, 0xF0, 0xE7, 0xC1, 0xC2, 0x40, 0x40]);
    let fixed = "FD  RPT RECORD CONTAINS 6 CHARACTERS REPORT IS CARDS.";
    let (_, adv) = run_report(&cards(fixed), "rw-adv6.dat", false);
    assert_eq!(adv, [0xF1, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0xF0, 0xE7, 0xC1, 0xC2, 0x40, 0x40, 0x40]);
    let (_, noadv6) = run_report(&noadv(cards(fixed)), "rw-noadv6.dat", false);
    assert_eq!(noadv6, [0xF1, 0x40, 0x40, 0x40, 0x40, 0x40, 0xF0, 0xE7, 0xC1, 0xC2, 0x40, 0x40]);
    let (_, text) = run_report(&noadv(cards(fixed)), "rw-noadv6.txt", true);
    assert_eq!(String::from_utf8(text).unwrap(), "\u{c}\n\nXAB\n");
}

#[test]
fn a_variable_length_report_record_ends_after_its_last_field() {
    let source = report_program(
        "FD  RPT RECORDING MODE IS V REPORT IS SLIP.",
        &["01  N PIC 9 VALUE 7."],
        &["RD  SLIP.", "01  ROW TYPE DE.", "    05 LINE PLUS 1.", "       10 COLUMN 1 VALUE 'N='.", "       10 COLUMN 3 PIC 9 SOURCE N.", "    05 LINE PLUS 1 COLUMN 1 PIC X(9) VALUE 'LONG LINE'."],
        &["    OPEN OUTPUT RPT", "    INITIATE SLIP", "    GENERATE ROW", "    TERMINATE SLIP", "    CLOSE RPT", "    GOBACK."],
    );
    let (_, file) = run_report(&source, "rw-slip.dat", false);
    let long = [0xD3, 0xD6, 0xD5, 0xC7, 0x40, 0xD3, 0xC9, 0xD5, 0xC5];
    assert_eq!(file, [&[0, 8, 0, 0, 0x40, 0xD5, 0x7E, 0xF7][..], &[0, 14, 0, 0, 0x40], &long].concat());
}

#[test]
fn use_before_reporting_runs_before_the_group_and_suppress_printing_drops_its_lines() {
    let source = report_program(
        "FD  RPT REPORT IS TALLY.",
        &["01  N PIC 9 VALUE 0.", "01  CALLS PIC 9 VALUE 0."],
        &[
            "RD  TALLY CONTROL IS FINAL.",
            "01  ROW TYPE DE LINE PLUS 1.",
            "    05 R-N COLUMN 1 PIC 9 SOURCE N.",
            "01  TYPE CF FINAL LINE PLUS 1.",
            "    05 COLUMN 1 VALUE 'SUM'.",
            "    05 COLUMN 5 PIC Z9 SUM R-N.",
        ],
        &[
            "DECLARATIVES.",
            "ROW-USE SECTION.",
            "    USE BEFORE REPORTING ROW.",
            "ROW-PARA.",
            "    ADD 1 TO CALLS",
            "    IF N = 2",
            "        SUPPRESS PRINTING",
            "    END-IF.",
            "END DECLARATIVES.",
            "MAIN SECTION.",
            "MAIN-PARA.",
            "    OPEN OUTPUT RPT",
            "    INITIATE TALLY",
            "    PERFORM 3 TIMES ADD 1 TO N GENERATE ROW END-PERFORM",
            "    TERMINATE TALLY",
            "    CLOSE RPT",
            "    DISPLAY CALLS",
            "    GOBACK.",
        ],
    );
    let (out, file) = run_report(&source, "rw-tally.txt", true);
    assert_eq!(text(file), "1\n3\nSUM  6\n");
    assert_eq!(out, "3\n");
    let global = source.replace("USE BEFORE REPORTING ROW", "USE GLOBAL BEFORE REPORTING ROW");
    let (out, file) = run_report(&global, "rw-tally-global.txt", true);
    assert_eq!((text(file).as_str(), out.as_str()), ("1\n3\nSUM  6\n", "3\n"));
}

#[test]
fn report_writer_features_not_implemented_are_refused_by_name() {
    let with_group = |group: &[&str]| {
        let rd = [&["RD  R CONTROL IS K PAGE LIMIT 20."][..], group].concat();
        report_program("FD  RPT REPORT IS R.", &["01  K PIC X."], &rd, &["    GOBACK."])
    };
    let cases: &[(&[&str], &str)] = &[
        (&["01  D TYPE DE LINE PLUS 1.", "    05 COLUMN 1 PIC X SOURCE K OCCURS 2."], "OCCURS in a report group"),
        (&["01  D TYPE DE LINE PLUS 1.", "    05 COLUMN 1 PIC X SOURCE K PRESENT WHEN K = 'A'."], "PRESENT and ABSENT"),
        (&["01  TYPE CH K OR PAGE LINE PLUS 1 COLUMN 1 PIC X SOURCE K."], "OR PAGE"),
        (&["01  TYPE CF FOR ALL LINE PLUS 1 COLUMN 1 PIC X SOURCE K."], "FOR ALL"),
        (&["01  D TYPE DE LINE PLUS 1.", "    05 COLUMN 1 PIC X SOURCES ARE K K."], "multiple SOURCES"),
        (&["01  D TYPE DE LINES ARE 1 2 COLUMN 1 PIC X SOURCE K."], "multiple LINES"),
        (&["01  TYPE CF K LINE PLUS 1 COLUMN 1 PIC X SOURCE K GROUP INDICATE."], "GROUP INDICATE outside a DETAIL"),
        (&["01  D TYPE DE LINE PLUS 1 COLUMN 1 PIC 9 SUM K.", "01  TYPE CF K LINE PLUS 1 COLUMN 1 PIC 99 SUM OF (K)."], "SUM of an arithmetic expression"),
        (&["01  D TYPE DE LINE PLUS 1 COLUMN 1 PIC X SOURCE K STYLE BOLD."], "STYLE"),
    ];
    for (group, name) in cases {
        let message = refusal(&with_group(group));
        assert!(message.contains(name), "expected a refusal naming {name}, got {message}");
    }
    let rd_cases: &[(&str, &str)] =
        &[("RD  R IS GLOBAL.", "GLOBAL report"), ("RD  R CODE IS MNEMO.", "CODE with a mnemonic-name"), ("RD  R LAST DETAIL WS-LAST PAGE 20.", "LAST DETAIL with an identifier")];
    for (rd, name) in rd_cases {
        let source = report_program("FD  RPT REPORT IS R.", &[], &[rd, "01  D TYPE DE LINE PLUS 1 COLUMN 1 VALUE 'X'."], &["    GOBACK."]);
        let message = refusal(&source);
        assert!(message.contains(name), "expected a refusal naming {name}, got {message}");
    }
    let statements: &[(&[&str], &str)] = &[(&["    INITIATE R UPON RPT."], "INITIATE ... UPON")];
    for (procedure, name) in statements {
        let source = report_program("FD  RPT REPORT IS R.", &[], &["RD  R.", "01  D TYPE DE LINE PLUS 1 COLUMN 1 VALUE 'X'."], procedure);
        let message = refusal(&source);
        assert!(message.contains(name), "expected a refusal naming {name}, got {message}");
    }
}

#[test]
fn report_writer_names_are_checked_at_compile_time() {
    let source = |procedure: &str, rd: &[&str]| report_program("FD  RPT REPORT IS R.", &["01  K PIC X."], rd, &[procedure]);
    let rd = ["RD  R PAGE LIMIT 20.", "01  D TYPE DE LINE PLUS 1 COLUMN 1 PIC X SOURCE K."];
    assert!(refusal(&source("    GENERATE NOSUCH.", &rd)).contains("NOSUCH"));
    assert!(refusal(&source("    INITIATE NOSUCH.", &rd)).contains("NOSUCH"));
    assert!(refusal(&source("    GENERATE R.", &rd)).contains("summary reporting"));
    assert!(refusal(&source("    GOBACK.", &["RD  R.", "01  D TYPE DE LINE 3 COLUMN 1 PIC X SOURCE K."])).contains("PAGE LIMIT"));
    assert!(refusal(&source("    GOBACK.", &["RD  R PAGE LIMIT 20.", "01  D TYPE DE LINE PLUS 1 COLUMN 1 PIC X SOURCE NOSUCH."])).contains("NOSUCH"));
    let unnamed = report_program("FD  RPT.", &[], &["RD  R.", "01  D TYPE DE LINE PLUS 1 COLUMN 1 VALUE 'X'."], &["    GOBACK."]);
    assert!(refusal(&unnamed).contains("no FD"));
}
