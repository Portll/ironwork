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

/// A program with files `select` and `fd`, WORKING-STORAGE `data`, and DECLARATIVES `declaratives`
/// before the MAIN section's `main`.
fn with_declaratives(select: &[&str], fd: &[&str], data: &[&str], declaratives: &[&str], main: &[&str]) -> String {
    [
        cobol(&["IDENTIFICATION DIVISION.", "PROGRAM-ID. U.", "ENVIRONMENT DIVISION.", "INPUT-OUTPUT SECTION.", "FILE-CONTROL."]),
        cobol(select),
        cobol(&["DATA DIVISION.", "FILE SECTION."]),
        cobol(fd),
        cobol(&["WORKING-STORAGE SECTION."]),
        cobol(data),
        cobol(&["PROCEDURE DIVISION.", "DECLARATIVES."]),
        cobol(declaratives),
        cobol(&["END DECLARATIVES.", "MAIN SECTION.", "M."]),
        cobol(main),
    ]
    .concat()
}

#[test]
fn a_file_procedure_comes_before_an_open_mode_one_and_the_run_goes_on() {
    let input = temp("decl-in.txt");
    std::fs::write(&input, "ONE\n").unwrap();
    let source = with_declaratives(
        &["    SELECT IN-F ASSIGN TO INDD ORGANIZATION LINE SEQUENTIAL", "        FILE STATUS IS FS.", "    SELECT OUT-F ASSIGN TO OUTDD."],
        &["FD  IN-F.", "01  IN-REC PIC X(3).", "FD  OUT-F.", "01  OUT-REC PIC X(3)."],
        &["01  FS PIC XX."],
        &[
            "IN-ERR SECTION.",
            "    USE AFTER STANDARD ERROR PROCEDURE ON INPUT.",
            "IN-ERR-1.",
            "    DISPLAY 'INPUT ' FS.",
            "OUT-ERR SECTION.",
            "    USE AFTER EXCEPTION PROCEDURE OUT-F.",
            "OUT-ERR-1.",
            "    DISPLAY 'OUT-F'.",
        ],
        &[
            "    OPEN INPUT IN-F",
            "    READ IN-F",
            "    READ IN-F NOT AT END DISPLAY 'NOT AT END' END-READ",
            "    DISPLAY 'AFTER READ ' FS",
            "    READ IN-F AT END DISPLAY 'AT END ' FS END-READ",
            "    OPEN INPUT OUT-F",
            "    DISPLAY 'AFTER OPEN'",
            "    WRITE OUT-REC",
            "    DISPLAY 'AFTER WRITE'",
            "    GOBACK.",
        ],
    );
    let (out, err, ending) = run_files(&source, &[format!("INDD={}:text", input.display())]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "INPUT 10\nAFTER READ 10\nAT END 10\nOUT-F\nAFTER OPEN\nOUT-F\nAFTER WRITE\n");
}

#[test]
fn an_invalid_key_phrase_takes_the_condition_and_without_one_the_procedure_does() {
    let path = temp("decl-ksds.dat");
    let _ = std::fs::remove_file(&path);
    let source = with_declaratives(
        &["    SELECT K ASSIGN TO KDD ORGANIZATION INDEXED ACCESS RANDOM", "        RECORD KEY K-ID FILE STATUS FS."],
        &["FD  K.", "01  K-REC.", "    05 K-ID PIC XX."],
        &["01  FS PIC XX."],
        &["K-ERR SECTION.", "    USE AFTER ERROR PROCEDURE ON K.", "K-ERR-1.", "    DISPLAY 'K ' FS."],
        &[
            "    OPEN OUTPUT K",
            "    WRITE K-REC FROM 'A1'",
            "    WRITE K-REC FROM 'A1' INVALID KEY DISPLAY 'INVALID ' FS",
            "    END-WRITE",
            "    WRITE K-REC FROM 'A1' NOT INVALID KEY DISPLAY 'NOT INVALID'",
            "    END-WRITE",
            "    MOVE 'B2' TO K-ID",
            "    WRITE K-REC NOT INVALID KEY DISPLAY 'WRITTEN ' FS END-WRITE",
            "    CLOSE K",
            "    GOBACK.",
        ],
    );
    let (out, err, ending) = run_files(&source, &[format!("KDD={}", path.display())]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "INVALID 22\nK 22\nWRITTEN 00\n");
}

#[test]
fn a_procedure_that_leaves_by_go_to_or_stop_run_takes_control_with_it() {
    let program = |exit: &str| {
        with_declaratives(
            &["    SELECT F ASSIGN TO NODD FILE STATUS FS."],
            &["FD  F.", "01  F-REC PIC X."],
            &["01  FS PIC XX."],
            &["E SECTION.", "    USE AFTER ERROR PROCEDURE ON F.", "E-1.", "    DISPLAY 'E ' FS", &format!("    {exit}.")],
            &["    OPEN INPUT F", "    DISPLAY 'NOT HERE'.", "RECOVER.", "    DISPLAY 'RECOVERED'", "    GOBACK."],
        )
    };
    let (out, err, ending) = run_files(&program("GO TO RECOVER"), &[]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "E 35\nRECOVERED\n");
    let (out, _, ending) = run_files(&program("STOP RUN"), &[]);
    assert_eq!((out.as_str(), ending), ("E 35\n", Ok(Ending::StopRun)));
    let (out, _, ending) = run_files(&program("CONTINUE"), &[]);
    assert_eq!((out.as_str(), ending), ("E 35\nNOT HERE\nRECOVERED\n", Ok(Ending::Goback)));
}

#[test]
fn open_extend_of_a_missing_data_set_runs_the_extend_procedure_unless_the_file_is_optional() {
    let (missing, optional) = (temp("decl-extend-missing.dat"), temp("decl-extend-optional.dat"));
    let _ = std::fs::remove_file(&missing);
    let _ = std::fs::remove_file(&optional);
    let source = with_declaratives(
        &["    SELECT F ASSIGN TO FDD FILE STATUS FS.", "    SELECT OPTIONAL G ASSIGN TO GDD FILE STATUS GS."],
        &["FD  F.", "01  F-REC PIC X.", "FD  G.", "01  G-REC PIC X."],
        &["01  FS PIC XX.", "01  GS PIC XX."],
        &["X SECTION.", "    USE AFTER ERROR PROCEDURE EXTEND.", "X-1.", "    DISPLAY 'EXTEND ' FS."],
        &["    OPEN EXTEND F G", "    DISPLAY GS", "    WRITE G-REC FROM 'A'", "    CLOSE G", "    GOBACK."],
    );
    let (out, err, ending) = run_files(&source, &[format!("FDD={}", missing.display()), format!("GDD={}", optional.display())]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "EXTEND 35\n05\n");
    assert!(!missing.exists());
    assert_eq!(std::fs::read(&optional).unwrap(), [0xC1]);
}

#[test]
fn a_file_not_open_has_no_open_mode_and_other_programs_procedures_do_not_apply() {
    let source = with_declaratives(
        &["    SELECT F ASSIGN TO NODD."],
        &["FD  F.", "01  F-REC PIC X."],
        &[],
        &["E SECTION.", "    USE AFTER ERROR PROCEDURE ON INPUT.", "E-1.", "    DISPLAY 'INPUT'."],
        &["    READ F", "    GOBACK."],
    );
    let (out, _, ending) = run_files(&source, &[]);
    assert_eq!(out, "");
    assert_eq!(ending.unwrap_err().code, "IO-47");
    let nested = [
        cobol(&["IDENTIFICATION DIVISION.", "PROGRAM-ID. OUTER.", "PROCEDURE DIVISION.", "DECLARATIVES.", "E SECTION.", "    USE AFTER ERROR PROCEDURE ON INPUT.", "E-1.", "    DISPLAY 'OUTER'.", "END DECLARATIVES.", "M SECTION.", "    CALL 'INNER'", "    GOBACK."]),
        cobol(&["IDENTIFICATION DIVISION.", "PROGRAM-ID. INNER.", "ENVIRONMENT DIVISION.", "INPUT-OUTPUT SECTION.", "FILE-CONTROL.", "    SELECT G ASSIGN TO NODD."]),
        cobol(&["DATA DIVISION.", "FILE SECTION.", "FD  G.", "01  G-REC PIC X.", "PROCEDURE DIVISION.", "    OPEN INPUT G", "    GOBACK.", "END PROGRAM INNER.", "END PROGRAM OUTER."]),
    ]
    .concat();
    let (out, _, ending) = run_files(&nested, &[]);
    assert_eq!(out, "");
    assert_eq!(ending.unwrap_err().code, "IO-35");
}

/// SORT S USING A B GIVING O, with `declarative`; `cbl` is a CBL card, or nothing.
fn sort_program(declarative: &[&str], cbl: &str) -> String {
    let body = with_declaratives(
        &["    SELECT S ASSIGN TO SORTWK1.", "    SELECT A ASSIGN TO ADD.", "    SELECT B ASSIGN TO BDD.", "    SELECT O ASSIGN TO ODD."],
        &["SD  S.", "01  S-REC PIC X(3).", "FD  A.", "01  A-REC PIC X(3).", "FD  B.", "01  B-REC PIC X(3).", "FD  O.", "01  O-REC PIC X(3)."],
        &["01  R PIC 99."],
        declarative,
        &["    SORT S ON ASCENDING KEY S-REC USING A B GIVING O", "    MOVE SORT-RETURN TO R", "    DISPLAY 'SORT-RETURN ' R", "    GOBACK."],
    );
    format!("{cbl}{body}")
}

#[test]
fn a_using_file_procedure_lets_the_sort_go_on_unless_it_sets_sort_return_to_16() {
    let (a, o) = (temp("decl-sort-a.txt"), temp("decl-sort-o.txt"));
    std::fs::write(&a, "CCC\nAAA\n").unwrap();
    let dds = [format!("ADD={}:text", a.display()), format!("ODD={}:text", o.display())];
    let handled = sort_program(&["B-ERR SECTION.", "    USE AFTER ERROR PROCEDURE ON B.", "B-1.", "    DISPLAY 'B FAILED'."], "");
    let (out, err, ending) = run_files(&handled, &dds);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "B FAILED\nSORT-RETURN 00\n");
    assert_eq!(std::fs::read_to_string(&o).unwrap(), "AAA\nCCC\n");
    let stopped = sort_program(&["B-ERR SECTION.", "    USE AFTER ERROR PROCEDURE ON B.", "B-1.", "    MOVE 16 TO SORT-RETURN."], "");
    let (out, err, ending) = run_files(&stopped, &dds);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "SORT-RETURN 16\n");
    assert!(err.contains("SORT-RETURN was set to 16"), "{err}");
    let (_, _, ending) = run_files(&sort_program(&["N SECTION.", "    USE AFTER ERROR PROCEDURE ON A."], ""), &dds);
    assert_eq!(ending.unwrap_err().code, "IO-35");
}

#[test]
fn fastsrt_leaves_a_file_with_an_exception_procedure_to_cobol() {
    let (b, o) = (temp("decl-fs-b.dat"), temp("decl-fs-o.dat"));
    std::fs::write(&b, []).unwrap();
    let source = sort_program(&["O-ERR SECTION.", "    USE AFTER ERROR PROCEDURE ON OUTPUT.", "O-1.", "    DISPLAY 'O'."], "       CBL FASTSRT\n").replace("USING A B", "USING B");
    let (_, err, ending) = run_files(&source, &[format!("BDD={}", b.display()), format!("ODD={}", o.display())]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert!(err.contains("FASTSRT does not apply to GIVING O: an EXCEPTION/ERROR procedure applies to it"), "{err}");
}

#[test]
fn ibm_rules_for_declaratives_are_checked() {
    let errors = |declaratives: &[&str], main: &[&str]| {
        let source = with_declaratives(
            &["    SELECT S ASSIGN TO SORTWK1.", "    SELECT F ASSIGN TO FDD."],
            &["SD  S.", "01  S-REC PIC X.", "FD  F.", "01  F-REC PIC X."],
            &[],
            declaratives,
            main,
        );
        compile_errors(&source)
    };
    fn one(text: &str) -> [&str; 4] {
        ["E1 SECTION.", text, "E1-P.", "    CONTINUE."]
    }
    assert!(errors(&one("    USE AFTER ERROR PROCEDURE ON S."), &["    GOBACK."]).contains("sort or merge file"));
    assert!(errors(&one("    USE AFTER ERROR PROCEDURE ON NOSUCH."), &["    GOBACK."]).contains("no file has that name"));
    let twice = [&one("    USE AFTER ERROR PROCEDURE ON F.")[..], &["E2 SECTION.", "    USE AFTER ERROR PROCEDURE ON F."]].concat();
    assert!(errors(&twice, &["    GOBACK."]).contains("another EXCEPTION/ERROR procedure"));
    let modes = [&one("    USE AFTER ERROR PROCEDURE ON INPUT.")[..], &["E2 SECTION.", "    USE AFTER ERROR PROCEDURE INPUT."]].concat();
    assert!(errors(&modes, &["    GOBACK."]).contains("same open mode"));
    assert!(errors(&one("    USE AFTER ERROR PROCEDURE ON F."), &["    PERFORM E1-P THRU M", "    GOBACK."]).contains("same declarative section"));
    assert_eq!(errors(&one("    USE AFTER ERROR PROCEDURE ON F."), &["    PERFORM E1 THRU E1-P", "    PERFORM M", "    GOBACK."]), "");
}

/// A program WITH DEBUGGING MODE when `mode`, whose debugging section shows DEBUG-ITEM before each
/// procedure, and the line number of each of its lines that DEBUG-LINE can name.
fn debugging_program(mode: bool) -> (String, BTreeMap<&'static str, usize>) {
    let lines: Vec<&'static str> = vec![
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. D.",
        "ENVIRONMENT DIVISION.",
        "CONFIGURATION SECTION.",
        if mode { "SOURCE-COMPUTER. IBM-370 WITH DEBUGGING MODE." } else { "SOURCE-COMPUTER. IBM-370." },
        "INPUT-OUTPUT SECTION.",
        "FILE-CONTROL.",
        "    SELECT F ASSIGN TO NODD.",
        "DATA DIVISION.",
        "FILE SECTION.",
        "FD  F.",
        "01  F-REC PIC X.",
        "WORKING-STORAGE SECTION.",
        "01  N PIC 9 VALUE 0.",
        "PROCEDURE DIVISION.",
        "DECLARATIVES.",
        "DBG SECTION.",
        "    USE FOR DEBUGGING ON ALL PROCEDURES.",
        "DBG-1.",
        "    DISPLAY DEBUG-NAME(1:8) '|' DEBUG-CONTENTS(1:13)",
        "        '|' DEBUG-LINE.",
        "E SECTION.",
        "    USE AFTER ERROR PROCEDURE ON F.",
        "E-1.",
        "    DISPLAY 'E'.",
        "END DECLARATIVES.",
        "MAIN SECTION.",
        "FIRST-P.",
        "D    DISPLAY 'DEBUGGING LINE'",
        "    PERFORM SUB 2 TIMES",
        "    OPEN INPUT F",
        "    GO TO LAST-P.",
        "SUB.",
        "    ADD 1 TO N.",
        "LAST-P.",
        "    DISPLAY N",
        "    GOBACK.",
    ];
    let source = lines
        .iter()
        .map(|l| match l.strip_prefix('D') {
            Some(rest) if rest.starts_with(' ') => format!("      D{rest}\n"),
            _ => format!("       {l}\n"),
        })
        .collect();
    let at = |text: &str| lines.iter().position(|l| l.trim_start().starts_with(text)).unwrap() + 1;
    let numbered = [("MAIN", at("MAIN SECTION")), ("E", at("E SECTION")), ("PERFORM", at("PERFORM SUB")), ("OPEN", at("OPEN INPUT F")), ("GO TO", at("GO TO LAST-P"))];
    (source, numbered.into_iter().collect())
}

#[test]
fn debugging_lines_and_sections_follow_debugging_mode_and_the_debug_runtime_option() {
    let (off, _) = debugging_program(false);
    assert_eq!(run_with(&off, &["-debug"]).0, "E\n2\n");
    let (on, lines) = debugging_program(true);
    assert_eq!(run_with(&on, &[]).0, "DEBUGGING LINE\nE\n2\n");
    let (out, err, ending) = run_with(&on, &["-debug"]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    let line = |what: &str| format!("{:06}", lines[what]);
    let expected = [
        format!("MAIN    |START PROGRAM|{}", line("MAIN")),
        format!("FIRST-P |FALL THROUGH |{}", line("MAIN")),
        "DEBUGGING LINE".to_owned(),
        format!("SUB     |PERFORM LOOP |{}", line("PERFORM")),
        format!("SUB     |PERFORM LOOP |{}", line("PERFORM")),
        format!("E       |USE PROCEDURE|{}", line("OPEN")),
        format!("E-1     |FALL THROUGH |{}", line("E")),
        "E".to_owned(),
        format!("LAST-P  |             |{}", line("GO TO")),
        "2".to_owned(),
    ];
    assert_eq!(out, expected.map(|l| l + "\n").concat());
}

#[test]
fn ibm_rules_for_debugging_sections_are_checked() {
    let program = |declaratives: &[&str], main: &[&str], cbl: &str| {
        let body = [
            cobol(&["IDENTIFICATION DIVISION.", "PROGRAM-ID. D.", "ENVIRONMENT DIVISION.", "CONFIGURATION SECTION.", "SOURCE-COMPUTER. X WITH DEBUGGING MODE."]),
            cobol(&["DATA DIVISION.", "WORKING-STORAGE SECTION.", "01  W PIC X(6).", "PROCEDURE DIVISION.", "DECLARATIVES."]),
            cobol(declaratives),
            cobol(&["END DECLARATIVES.", "MAIN SECTION.", "M."]),
            cobol(main),
        ]
        .concat();
        compile_errors(&format!("{cbl}{body}"))
    };
    let debug = |on: &str, body: &str| vec!["DBG SECTION.".to_owned(), format!("    USE FOR DEBUGGING ON {on}."), "DBG-1.".to_owned(), format!("    {body}.")];
    let run = |lines: &[String], main: &[&str], cbl: &str| program(&lines.iter().map(String::as_str).collect::<Vec<_>>(), main, cbl);
    let back = ["    GOBACK."];
    assert_eq!(run(&debug("M", "MOVE DEBUG-LINE TO W"), &back, ""), "");
    assert!(run(&debug("M", "PERFORM M"), &back, "").contains("only to declarative procedures"));
    assert!(run(&debug("M", "CONTINUE"), &["    PERFORM DBG-1", "    GOBACK."], "").contains("only a debugging section may refer"));
    assert!(run(&debug("M", "CONTINUE"), &["    MOVE DEBUG-NAME TO W", "    GOBACK."], "").contains("only a debugging section may reference DEBUG-ITEM"));
    assert!(run(&debug("DBG-1", "CONTINUE"), &back, "").contains("in a debugging section"));
    assert!(run(&debug("NOSUCH", "CONTINUE"), &back, "").contains("NOSUCH"));
    assert!(run(&debug("M M", "CONTINUE"), &back, "").contains("twice"));
    let both = [debug("ALL PROCEDURES", "CONTINUE"), debug("M", "CONTINUE").into_iter().map(|l| l.replace("DBG", "DBH")).collect()].concat();
    assert!(run(&both, &back, "").contains("ALL PROCEDURES: it may be written once"));
    assert!(run(&debug("M", "CONTINUE"), &back, "       CBL THREAD\n").contains("USE FOR DEBUGGING is not allowed in a program compiled with THREAD"));
    assert!(run(&debug("M", "CONTINUE"), &["    GO TO M DBG-1 DEPENDING ON RETURN-CODE", "    GOBACK."], "").contains("only a debugging section may refer"));
    assert!(run(&debug("M", "CONTINUE"), &["    ALTER A TO PROCEED TO DBG-1", "    GOBACK.", "A.", "    GO TO M."], "").contains("only a debugging section may refer"));
}

#[test]
fn go_to_depending_and_an_altered_go_to_reach_a_procedure_as_go_to_does() {
    let source = cobol(&[
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. D.",
        "ENVIRONMENT DIVISION.",
        "CONFIGURATION SECTION.",
        "SOURCE-COMPUTER. IBM-370 WITH DEBUGGING MODE.",
        "DATA DIVISION.",
        "WORKING-STORAGE SECTION.",
        "01  N PIC 9 VALUE 2.",
        "PROCEDURE DIVISION.",
        "DECLARATIVES.",
        "DBG SECTION.",
        "    USE FOR DEBUGGING ON P2 P3.",
        "DBG-1.",
        "    DISPLAY DEBUG-NAME(1:4) '|' DEBUG-CONTENTS(1:4) '|'.",
        "END DECLARATIVES.",
        "MAIN SECTION.",
        "M.",
        "    GO TO P1 P2 DEPENDING ON N.",
        "P1.",
        "    DISPLAY 'P1'.",
        "P2.",
        "    ALTER A TO PROCEED TO P3",
        "    GO TO A.",
        "A.",
        "    GO TO P1.",
        "P3.",
        "    GOBACK.",
    ]);
    let (out, err, ending) = run_with(&source, &["-debug"]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "P2  |    |\nP3  |    |\n");
}

pub(super) fn fuzz_seeds() -> Vec<String> {
    let (debugging, _) = debugging_program(true);
    vec![debugging, sort_program(&["B-ERR SECTION.", "    USE GLOBAL AFTER STANDARD EXCEPTION PROCEDURE ON B.", "B-1.", "    MOVE 16 TO SORT-RETURN."], "")]
}
