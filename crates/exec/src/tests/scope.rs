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

fn compile_errors(source: &str) -> Vec<String> {
    let program = syntax::parse(source).unwrap_or_else(|e| panic!("{e}"));
    match compile(program, &[]) {
        Ok(c) => panic!("compiled: {:?}", c.diagnostics),
        Err(errors) => errors.iter().map(|e| e.message.clone()).collect(),
    }
}

const SUB_EXTERNAL: &[&str] = &[
    "IDENTIFICATION DIVISION.",
    "PROGRAM-ID. SUB.",
    "DATA DIVISION.",
    "WORKING-STORAGE SECTION.",
    "01  SHARED IS EXTERNAL.",
    "    05  S-NAME PIC X(5).",
    "    05  S-COUNT PIC 9(3).",
    "01  WHOLE REDEFINES SHARED PIC X(8).",
    "01  OWN PIC X(5) VALUE 'LOCAL'.",
    "PROCEDURE DIVISION.",
    "    DISPLAY 'SUB SEES ' WHOLE",
    "    MOVE 'OMEGA' TO S-NAME",
    "    ADD 1 TO S-COUNT",
    "    GOBACK.",
    "END PROGRAM SUB.",
];

#[test]
fn an_external_record_is_one_record_for_the_run_unit_whatever_cancel_does() {
    let main = cobol(&[
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. MAIN.",
        "DATA DIVISION.",
        "WORKING-STORAGE SECTION.",
        "01  SHARED EXTERNAL.",
        "    05  S-NAME PIC X(5).",
        "    05  S-COUNT PIC 9(3).",
        "PROCEDURE DIVISION.",
        "    MOVE 'ALPHA' TO S-NAME",
        "    MOVE 1 TO S-COUNT",
        "    CALL 'SUB'",
        "    DISPLAY S-NAME ' ' S-COUNT",
        "    CANCEL 'SUB'",
        "    CALL 'SUB'",
        "    DISPLAY S-NAME ' ' S-COUNT",
        "    GOBACK.",
        "END PROGRAM MAIN.",
    ]);
    let (out, err, ending) = run_unit(&(main + &cobol(SUB_EXTERNAL)), Vec::new(), "");
    assert!(ending.is_ok(), "{ending:?}\n{err}");
    assert_eq!(out, "SUB SEES ALPHA001\nOMEGA 002\nSUB SEES OMEGA002\nOMEGA 003\n");
}

#[test]
fn an_external_record_of_another_size_ends_the_run() {
    let main = cobol(&[
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. MAIN.",
        "DATA DIVISION.",
        "WORKING-STORAGE SECTION.",
        "01  SHARED PIC X(9) EXTERNAL.",
        "PROCEDURE DIVISION.",
        "    CALL 'SUB'",
        "    GOBACK.",
        "END PROGRAM MAIN.",
    ]);
    let (_, _, ending) = run_unit(&(main + &cobol(SUB_EXTERNAL)), Vec::new(), "");
    let abend = ending.unwrap_err();
    assert!(abend.message.contains("EXTERNAL record SHARED has 9 bytes in the run unit, and this program describes 8"), "{abend:?}");
}

#[test]
fn where_external_and_global_may_be_written() {
    let errors = compile_errors(&cobol(&[
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. T.",
        "ENVIRONMENT DIVISION.",
        "INPUT-OUTPUT SECTION.",
        "FILE-CONTROL.",
        "    SELECT F ASSIGN TO FDD.",
        "DATA DIVISION.",
        "FILE SECTION.",
        "FD  F IS EXTERNAL.",
        "01  FILLER PIC X(4).",
        "WORKING-STORAGE SECTION.",
        "01  A EXTERNAL PIC X(4) VALUE 'AAAA'.",
        "01  B EXTERNAL.",
        "    05  B1 PIC X EXTERNAL.",
        "    05  B2 PIC X VALUE 'B'.",
        "    05  B3 PIC X.",
        "        88  B3-ON VALUE 'Y'.",
        "01  C PIC X(4).",
        "01  D REDEFINES C EXTERNAL PIC X(4).",
        "01  B IS EXTERNAL PIC X.",
        "01  E REDEFINES A PIC X(5).",
        "01  G GLOBAL PIC X.",
        "    88  G-ON VALUE 'Y'.",
        "01  G GLOBAL PIC X.",
        "77  H GLOBAL PIC X.",
        "LOCAL-STORAGE SECTION.",
        "01  L EXTERNAL PIC X.",
        "PROCEDURE DIVISION.",
        "    GOBACK.",
    ]));
    let expected = [
        "A: an item of EXTERNAL record A takes no VALUE clause",
        "B1: EXTERNAL goes on a level-01 entry",
        "B2: an item of EXTERNAL record B takes no VALUE clause",
        "D: EXTERNAL and REDEFINES cannot be in the same entry",
        "B: another EXTERNAL record of the program has the same name",
        "G: another GLOBAL record of the DATA DIVISION has the same name",
        "H: GLOBAL goes on a level-01 entry",
        "L: EXTERNAL is not allowed in the LOCAL-STORAGE SECTION",
        "FD F: a record of an EXTERNAL or GLOBAL file needs a data-name, not FILLER",
        "E: 5 bytes, larger than the EXTERNAL record A it redefines",
    ];
    for e in expected {
        assert!(errors.iter().any(|m| m == e), "{e:?} not in {errors:#?}");
    }
    assert!(!errors.iter().any(|m| m.contains("B3")), "{errors:#?}");
    let record = syntax::parse(&cobol(&[
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. T.",
        "ENVIRONMENT DIVISION.",
        "INPUT-OUTPUT SECTION.",
        "FILE-CONTROL.",
        "    SELECT S ASSIGN TO SDD.",
        "DATA DIVISION.",
        "FILE SECTION.",
        "SD  S GLOBAL.",
        "01  R PIC X.",
    ]))
    .unwrap_err();
    assert!(record.message.contains("SD S: a sort or merge file takes no EXTERNAL or GLOBAL clause"), "{record}");
}

#[test]
fn an_external_file_is_one_connector_and_record_area_for_the_run_unit() {
    let path = temp("external-file.dat");
    let _ = std::fs::remove_file(&path);
    let file = |id: &str, procedure: &[&str]| {
        let mut lines = vec![
            "IDENTIFICATION DIVISION.".to_owned(),
            format!("PROGRAM-ID. {id}."),
            "ENVIRONMENT DIVISION.".into(),
            "INPUT-OUTPUT SECTION.".into(),
            "FILE-CONTROL.".into(),
            "    SELECT XF ASSIGN TO XDD FILE STATUS IS XS.".into(),
            "DATA DIVISION.".into(),
            "FILE SECTION.".into(),
            "FD  XF IS EXTERNAL.".into(),
            format!("01  {id}-REC PIC X(6)."),
            "WORKING-STORAGE SECTION.".into(),
            "01  XS PIC XX EXTERNAL.".into(),
            "PROCEDURE DIVISION.".into(),
        ];
        lines.extend(procedure.iter().map(|l| l.to_string()));
        lines.push(format!("END PROGRAM {id}."));
        cobol(&lines.iter().map(String::as_str).collect::<Vec<_>>())
    };
    let main = file(
        "MAIN",
        &[
            "    OPEN OUTPUT XF",
            "    MOVE 'FIRST' TO MAIN-REC",
            "    CALL 'PUT'",
            "    CLOSE XF",
            "    OPEN INPUT XF",
            "    CALL 'GET'",
            "    DISPLAY 'MAIN ' MAIN-REC ' ' XS",
            "    CALL 'GET'",
            "    CALL 'GET'",
            "    DISPLAY 'MAIN ' MAIN-REC ' ' XS",
            "    CLOSE XF",
            "    GOBACK.",
        ],
    );
    let put = file("PUT", &["    DISPLAY 'PUT ' PUT-REC", "    WRITE PUT-REC", "    MOVE 'SECOND' TO PUT-REC", "    WRITE PUT-REC", "    GOBACK."]);
    let get = file("GET", &["    READ XF AT END DISPLAY 'GET AT END ' XS END-READ", "    GOBACK."]);
    let source = [main, put, get].concat();
    let o = Harness::source(&source).dds(&[format!("XDD={}", path.display())]).run(Executor::Interpreter);
    assert!(o.ending.is_ok(), "{:?}\n{}", o.ending, o.err);
    assert_eq!(o.out, "PUT FIRST \nMAIN FIRST  00\nGET AT END 10\nMAIN SECOND 10\n");
    assert_eq!(std::fs::read(&path).unwrap().len(), 12);
}

/// OUTER contains INNER, which contains DEEPEST.
fn nested(outer_data: &[&str], outer_body: &[&str], inner_data: &[&str], inner_body: &[&str], deepest_data: &[&str], deepest_body: &[&str]) -> String {
    let program = |id: &str, data: &[&str], body: &[&str]| {
        [cobol(&["IDENTIFICATION DIVISION.", &format!("PROGRAM-ID. {id}."), "DATA DIVISION.", "WORKING-STORAGE SECTION."]), cobol(data), cobol(&["PROCEDURE DIVISION."]), cobol(body)].concat()
    };
    [
        program("OUTER", outer_data, outer_body),
        program("INNER", inner_data, inner_body),
        program("DEEPEST", deepest_data, deepest_body),
        cobol(&["END PROGRAM DEEPEST.", "END PROGRAM INNER.", "END PROGRAM OUTER."]),
    ]
    .concat()
}

#[test]
fn a_global_name_reaches_contained_programs_until_one_declares_it_again() {
    let source = nested(
        &["01  G-REC GLOBAL.", "    05  G-NAME PIC X(5) VALUE 'OUTER'.", "    05  G-FLAG PIC X VALUE 'N'.", "        88  G-ON VALUE 'Y'.", "01  G-OTHER GLOBAL PIC X(5) VALUE 'OTHER'.", "01  NOT-GLOBAL PIC X(5) VALUE 'LOCAL'."],
        &["    CALL 'INNER'", "    DISPLAY 'OUTER ' G-NAME ' ' G-FLAG ' ' G-OTHER", "    GOBACK."],
        &["01  G-OTHER GLOBAL PIC X(5) VALUE 'MINE'.", "01  NOT-GLOBAL PIC X(5) VALUE 'OWN'."],
        &["    DISPLAY 'INNER ' G-NAME ' ' G-OTHER ' ' NOT-GLOBAL", "    MOVE 'INNER' TO G-NAME", "    CALL 'DEEPEST'", "    GOBACK."],
        &["01  G-NAME PIC X(5) VALUE 'DEEP'."],
        &["    DISPLAY 'DEEPEST ' G-NAME ' ' G-OTHER ' ' G-NAME OF G-REC", "    SET G-ON TO TRUE", "    MOVE 'SET' TO G-OTHER", "    GOBACK."],
    );
    assert_eq!(run(&source), "INNER OUTER MINE  OWN  \nDEEPEST DEEP  MINE  INNER\nOUTER INNER Y OTHER\n");
}

#[test]
fn a_contained_program_called_from_outside_its_container_has_no_global_storage() {
    let source = [
        nested(&["01  G GLOBAL PIC X VALUE 'G'."], &["    GOBACK."], &[], &["    DISPLAY G", "    GOBACK."], &[], &["    GOBACK."]),
        cobol(&["IDENTIFICATION DIVISION.", "PROGRAM-ID. ELSEWHERE.", "PROCEDURE DIVISION.", "    CALL 'INNER'", "    GOBACK.", "END PROGRAM ELSEWHERE."]),
    ]
    .concat();
    let first = source.find("       IDENTIFICATION DIVISION.\n       PROGRAM-ID. ELSEWHERE.").unwrap();
    let reordered = format!("{}{}", &source[first..], &source[..first]);
    let (_, _, ending) = run_unit(&reordered, Vec::new(), "");
    let abend = ending.unwrap_err();
    assert!(abend.message.contains("INNER uses the GLOBAL names of OUTER, which contains it and is not running"), "{abend:?}");
}

/// OUTER, with a GLOBAL file and `declaratives`, contains READER, which reads it with `reader_declaratives`.
fn global_file(declaratives: &[&str], reader_declaratives: &[&str], reader_body: &[&str]) -> String {
    let section = |uses: &[&str]| if uses.is_empty() { String::new() } else { [cobol(&["DECLARATIVES."]), cobol(uses), cobol(&["END DECLARATIVES.", "MAIN SECTION."])].concat() };
    [
        cobol(&[
            "IDENTIFICATION DIVISION.",
            "PROGRAM-ID. OUTER.",
            "ENVIRONMENT DIVISION.",
            "INPUT-OUTPUT SECTION.",
            "FILE-CONTROL.",
            "    SELECT GF ASSIGN TO GDD FILE STATUS IS GS.",
            "DATA DIVISION.",
            "FILE SECTION.",
            "FD  GF GLOBAL.",
            "01  G-REC PIC X(4).",
            "WORKING-STORAGE SECTION.",
            "01  GS GLOBAL PIC XX.",
            "01  SEEN PIC 9 VALUE 0.",
            "PROCEDURE DIVISION.",
        ]),
        section(declaratives),
        cobol(&["M.", "    CALL 'READER'", "    DISPLAY 'OUTER ' G-REC ' ' GS ' ' SEEN", "    CLOSE GF", "    GOBACK."]),
        cobol(&["IDENTIFICATION DIVISION.", "PROGRAM-ID. READER.", "PROCEDURE DIVISION."]),
        section(reader_declaratives),
        cobol(&["R."]),
        cobol(reader_body),
        cobol(&["    GOBACK.", "END PROGRAM READER.", "END PROGRAM OUTER."]),
    ]
    .concat()
}

fn run_global_file(source: &str) -> String {
    let path = temp("global-file.txt");
    std::fs::write(&path, "ABCD\n").unwrap();
    let o = Harness::source(source).dds(&[format!("GDD={}:text", path.display())]).run(Executor::Interpreter);
    assert!(o.ending.is_ok(), "{:?}\n{}", o.ending, o.err);
    o.out
}

#[test]
fn a_global_file_is_the_containing_programs_connector_record_and_status() {
    let source = global_file(&[], &[], &["    OPEN INPUT GF", "    READ GF", "    DISPLAY 'READER ' G-REC ' ' GS"]);
    assert_eq!(run_global_file(&source), "READER ABCD 00\nOUTER ABCD 00 0\n");
}

#[test]
fn a_global_declarative_serves_a_contained_program_without_its_own() {
    let global = ["G-ERR SECTION.", "    USE GLOBAL AFTER ERROR PROCEDURE ON INPUT.", "G-ERR-1.", "    ADD 1 TO SEEN", "    DISPLAY 'GLOBAL ' GS ' ' SEEN."];
    let body = ["    OPEN INPUT GF", "    READ GF", "    READ GF", "    DISPLAY 'AFTER ' GS"];
    assert_eq!(run_global_file(&global_file(&global, &[], &body)), "GLOBAL 10 1\nAFTER 10\nOUTER ABCD 10 1\n");
    let own = ["R-ERR SECTION.", "    USE AFTER ERROR PROCEDURE ON GF.", "R-ERR-1.", "    DISPLAY 'OWN ' GS."];
    assert_eq!(run_global_file(&global_file(&global, &own, &body)), "OWN 10\nAFTER 10\nOUTER ABCD 10 0\n");
    let local = ["L-ERR SECTION.", "    USE AFTER ERROR PROCEDURE ON INPUT.", "L-ERR-1.", "    DISPLAY 'NOT GLOBAL'."];
    let not_global = global_file(&local, &[], &body);
    let o = Harness::source(&not_global).dds(&[format!("GDD={}:text", temp("global-file.txt").display())]).run(Executor::Interpreter);
    assert!(o.out.starts_with("AFTER 10\n"), "{}", o.out);
    let stop = ["G-ERR SECTION.", "    USE GLOBAL AFTER ERROR PROCEDURE ON GF.", "G-ERR-1.", "    DISPLAY 'STOPPING'", "    STOP RUN."];
    assert_eq!(run_global_file(&global_file(&stop, &[], &body)), "STOPPING\n");
}

#[test]
fn a_global_files_status_must_be_a_global_name_of_its_program() {
    let source = global_file(&[], &[], &["    OPEN INPUT GF"]).replace("01  GS GLOBAL PIC XX.", "01  GS PIC XX.");
    let programs = syntax::parse_all_with(&source, &syntax::copy::Libraries::default()).unwrap();
    let errors = compile(programs[1].clone(), &[]).err().unwrap();
    assert!(errors.iter().any(|e| e.message == "GF, a GLOBAL file of OUTER: its FILE STATUS GS is not a GLOBAL name of OUTER, which is not supported yet"), "{errors:?}");
}

#[test]
fn external_and_global_storage_does_not_lower_yet() {
    let unsupported = |source: &str, k: usize| {
        let programs = syntax::parse_all_with(source, &syntax::copy::Libraries::default()).unwrap();
        match lower::lower(&compile(programs[k].clone(), &[]).unwrap()) {
            Err(lower::LowerError::Unsupported(what, _)) => what,
            other => panic!("{other:?}"),
        }
    };
    let external = cobol(&["IDENTIFICATION DIVISION.", "PROGRAM-ID. T.", "DATA DIVISION.", "WORKING-STORAGE SECTION.", "01  X PIC X EXTERNAL.", "PROCEDURE DIVISION.", "    GOBACK."]);
    assert_eq!(unsupported(&external, 0), "EXTERNAL data and files, and GLOBAL names of a containing program");
    let tree = nested(&["01  G GLOBAL PIC X."], &["    GOBACK."], &[], &["    GOBACK."], &[], &["    GOBACK."]);
    assert_eq!(unsupported(&tree, 0), "GLOBAL names and declaratives of a program that contains others");
    assert_eq!(unsupported(&tree, 1), "EXTERNAL data and files, and GLOBAL names of a containing program");
    let reader = global_file(&[], &[], &["    OPEN INPUT GF"]);
    assert_eq!(unsupported(&reader, 1), "EXTERNAL data and files, and GLOBAL names of a containing program");
}
