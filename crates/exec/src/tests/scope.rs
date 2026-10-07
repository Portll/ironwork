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
    assert_eq!((abend.code.to_string(), abend.message.as_str()), ("U4038".to_owned(), "IGZ0066S The length of external data record SHARED in program SUB did not match the existing length of the record. (9 bytes in the run unit, 8 here)"));
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
fn a_contained_program_is_out_of_reach_from_outside_its_container_unless_the_scope_is_flexible() {
    let source = [
        nested(&["01  G GLOBAL PIC X VALUE 'G'."], &["    GOBACK."], &[], &["    DISPLAY G", "    GOBACK."], &[], &["    GOBACK."]),
        cobol(&["IDENTIFICATION DIVISION.", "PROGRAM-ID. ELSEWHERE.", "PROCEDURE DIVISION.", "    CALL 'INNER'", "    GOBACK.", "END PROGRAM ELSEWHERE."]),
    ]
    .concat();
    let first = source.find("       IDENTIFICATION DIVISION.\n       PROGRAM-ID. ELSEWHERE.").unwrap();
    let reordered = format!("{}{}", &source[first..], &source[..first]);
    let strict = Harness::source(&reordered).run(Executor::Interpreter);
    let abend = strict.ending.unwrap_err();
    assert!(abend.message.starts_with("CALL INNER: IEW2456E SYMBOL INNER UNRESOLVED"), "{abend:?}");
    let flexible = Harness::source(&reordered).flags(&["--program-scope=flexible"]).run(Executor::Interpreter);
    let abend = flexible.ending.unwrap_err();
    assert!(abend.message.contains("INNER uses the GLOBAL names of OUTER, which contains it and is not running"), "{abend:?}");
}

/// OUTER contains LEFT, a COMMON SHARED and RIGHT; LEFT contains DEEP. Each displays its name and
/// calls the program `calls` gives it, which ON EXCEPTION reports missing.
fn siblings(calls: &[(&str, &str)]) -> String {
    let call = |id: &str| calls.iter().find(|(from, _)| *from == id).map(|(_, to)| *to);
    let program = |id: &str, common: bool| {
        let header = if common { format!("PROGRAM-ID. {id} IS COMMON.") } else { format!("PROGRAM-ID. {id}.") };
        let mut body = vec![format!("    DISPLAY '{id}'")];
        if let Some(to) = call(id) {
            body.push(format!("    CALL {to} ON EXCEPTION DISPLAY 'NO {}'", to.trim_matches('\'')));
            body.push("    END-CALL".into());
        }
        body.push("    GOBACK.".into());
        let body: Vec<&str> = body.iter().map(String::as_str).collect();
        [cobol(&["IDENTIFICATION DIVISION.", &header, "DATA DIVISION.", "WORKING-STORAGE SECTION.", "01  WS-NAME PIC X(8) VALUE 'SHARED'.", "PROCEDURE DIVISION."]), cobol(&body)].concat()
    };
    [
        program("OUTER", false),
        program("LEFT", false),
        program("DEEP", false),
        cobol(&["END PROGRAM DEEP.", "END PROGRAM LEFT."]),
        program("SHARED", true),
        cobol(&["END PROGRAM SHARED."]),
        program("RIGHT", false),
        cobol(&["END PROGRAM RIGHT.", "END PROGRAM OUTER."]),
    ]
    .concat()
}

#[test]
fn a_call_reaches_the_programs_its_container_holds_as_ibms_scope_rules_say() {
    let run = |calls: &[(&str, &str)]| Harness::source(&siblings(calls)).run(Executor::Interpreter).out;
    assert_eq!(run(&[("OUTER", "'LEFT'"), ("LEFT", "'DEEP'")]), "OUTER\nLEFT\nDEEP\n", "a program reaches those it directly contains");
    assert_eq!(run(&[("OUTER", "'DEEP'")]), "OUTER\nNO DEEP\n", "not one its contained program contains");
    assert_eq!(run(&[("OUTER", "'LEFT'"), ("LEFT", "'RIGHT'")]), "OUTER\nLEFT\nNO RIGHT\n", "nor a sibling that is not COMMON");
    assert_eq!(run(&[("OUTER", "'LEFT'"), ("LEFT", "'DEEP'"), ("DEEP", "'SHARED'")]), "OUTER\nLEFT\nDEEP\nSHARED\n", "a COMMON program is reached from anywhere inside its container");
    assert_eq!(run(&[("OUTER", "'RIGHT'"), ("RIGHT", "WS-NAME")]), "OUTER\nRIGHT\nSHARED\n", "a dynamic CALL keeps the same rules");
    assert_eq!(run(&[("OUTER", "'SHARED'"), ("SHARED", "'OUTER'")]), "OUTER\nSHARED\nNO OUTER\n", "a COMMON program does not reach the program containing it");
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

fn run_global_file(name: &str, source: &str) -> String {
    let path = temp(name);
    std::fs::write(&path, "ABCD\n").unwrap();
    let o = Harness::source(source).dds(&[format!("GDD={}:text", path.display())]).run(Executor::Interpreter);
    assert!(o.ending.is_ok(), "{:?}\n{}", o.ending, o.err);
    o.out
}

#[test]
fn a_global_file_is_the_containing_programs_connector_record_and_status() {
    let source = global_file(&[], &[], &["    OPEN INPUT GF", "    READ GF", "    DISPLAY 'READER ' G-REC ' ' GS"]);
    assert_eq!(run_global_file("global-connector.txt", &source), "READER ABCD 00\nOUTER ABCD 00 0\n");
}

#[test]
fn a_global_declarative_serves_a_contained_program_without_its_own() {
    let global = ["G-ERR SECTION.", "    USE GLOBAL AFTER ERROR PROCEDURE ON INPUT.", "G-ERR-1.", "    ADD 1 TO SEEN", "    DISPLAY 'GLOBAL ' GS ' ' SEEN."];
    let body = ["    OPEN INPUT GF", "    READ GF", "    READ GF", "    DISPLAY 'AFTER ' GS"];
    assert_eq!(run_global_file("global-declaratives.txt", &global_file(&global, &[], &body)), "GLOBAL 10 1\nAFTER 10\nOUTER ABCD 10 1\n");
    let own = ["R-ERR SECTION.", "    USE AFTER ERROR PROCEDURE ON GF.", "R-ERR-1.", "    DISPLAY 'OWN ' GS."];
    assert_eq!(run_global_file("global-declaratives.txt", &global_file(&global, &own, &body)), "OWN 10\nAFTER 10\nOUTER ABCD 10 0\n");
    let local = ["L-ERR SECTION.", "    USE AFTER ERROR PROCEDURE ON INPUT.", "L-ERR-1.", "    DISPLAY 'NOT GLOBAL'."];
    let not_global = global_file(&local, &[], &body);
    let o = Harness::source(&not_global).dds(&[format!("GDD={}:text", temp("global-declaratives.txt").display())]).run(Executor::Interpreter);
    assert!(o.out.starts_with("AFTER 10\n"), "{}", o.out);
    let stop = ["G-ERR SECTION.", "    USE GLOBAL AFTER ERROR PROCEDURE ON GF.", "G-ERR-1.", "    DISPLAY 'STOPPING'", "    STOP RUN."];
    assert_eq!(run_global_file("global-declaratives.txt", &global_file(&stop, &[], &body)), "STOPPING\n");
}

#[test]
fn a_global_files_status_must_be_a_global_name_of_its_program() {
    let source = global_file(&[], &[], &["    OPEN INPUT GF"]).replace("01  GS GLOBAL PIC XX.", "01  GS PIC XX.");
    let programs = syntax::parse_all_with(&source, &syntax::copy::Libraries::default()).unwrap();
    let errors = compile(programs[1].clone(), &[]).err().unwrap();
    assert!(errors.iter().any(|e| e.message == "GF, a GLOBAL file of OUTER: its FILE STATUS GS is not a GLOBAL name of OUTER, which is not supported yet"), "{errors:?}");
}

#[test]
fn external_and_global_storage_lowers_as_what_each_activation_binds() {
    let lowered = |source: &str, k: usize| {
        let programs = syntax::parse_all_with(source, &syntax::copy::Libraries::default()).unwrap();
        lower::lower(&compile(programs[k].clone(), &[]).unwrap()).unwrap_or_else(|e| panic!("{e}"))
    };
    let name = |p: &rt::lir::Program, s: u32| p.symbols[s as usize].clone();
    let external = cobol(&["IDENTIFICATION DIVISION.", "PROGRAM-ID. T.", "DATA DIVISION.", "WORKING-STORAGE SECTION.", "01  X PIC X(3) EXTERNAL.", "PROCEDURE DIVISION.", "    GOBACK."]);
    let t = lowered(&external, 0);
    let [(0, rt::lir::Binding::External { name: x, size: 3 })] = t.services.scope.records[..] else { panic!("{:?}", t.services.scope) };
    assert_eq!(name(&t, x), "X");
    let tree = nested(&["01  G GLOBAL PIC X."], &["    GOBACK."], &[], &["    GOBACK."], &[], &["    GOBACK."]);
    let (outer, inner, deepest) = (lowered(&tree, 0), lowered(&tree, 1), lowered(&tree, 2));
    let [rt::lir::Global { section: rt::lir::Section::WorkingStorage, name: g, at: rt::lir::GlobalAt::Program(0) }] = outer.services.scope.globals[..] else { panic!("{:?}", outer.services.scope) };
    assert_eq!(name(&outer, g), "G");
    let [(0, rt::lir::Binding::Global { program, section: rt::lir::Section::WorkingStorage, name: g })] = inner.services.scope.records[..] else { panic!("{:?}", inner.services.scope) };
    assert_eq!((name(&inner, program), name(&inner, g)), ("OUTER".into(), "G".into()));
    assert_eq!(deepest.services.scope.containers.iter().map(|&s| name(&deepest, s)).collect::<Vec<_>>(), ["INNER", "OUTER"]);
    let reader = lowered(&global_file(&["G-ERR SECTION.", "    USE GLOBAL AFTER ERROR PROCEDURE ON INPUT.", "G-ERR-1.", "    DISPLAY GS."], &[], &["    OPEN INPUT GF"]), 1);
    let [rt::lir::SharedFile { file: 0, external: false, declared_in: Some(outer_id) }] = reader.services.scope.files[..] else { panic!("{:?}", reader.services.scope) };
    assert_eq!(name(&reader, outer_id), "OUTER");
    assert_eq!(reader.services.scope.areas.len(), 1);
}

/// Runs `source` on the interpreter, which runs it on the VM too and compares the two, and then
/// on the VM alone, which must run it to its end; `reset` puts the files back before each run.
fn on_both(source: &str, dds: &[String], reset: impl Fn()) -> (String, Result<Ending, Abend>) {
    reset();
    let walker = Harness::source(source).dds(dds).run(Executor::Interpreter);
    reset();
    let vm = Harness::source(source).dds(dds).run(Executor::Vm);
    assert_eq!((&vm.out, &vm.ending), (&walker.out, &walker.ending), "{}", walker.err);
    (walker.out, walker.ending)
}

#[test]
fn a_global_declarative_left_by_goback_or_exit_program_ends_the_run() {
    let path = temp("global-leaving.txt");
    let reset = || std::fs::write(&path, "ABCD\n").unwrap();
    let dds = [format!("GDD={}:text", path.display())];
    for exit in ["GOBACK", "EXIT PROGRAM"] {
        let leave = ["G-ERR SECTION.", "    USE GLOBAL AFTER ERROR PROCEDURE ON GF.", "G-ERR-1.", "    DISPLAY 'LEAVING'", &format!("    {exit}.")];
        let source = global_file(&leave, &[], &["    OPEN INPUT GF", "    READ GF", "    READ GF", "    DISPLAY 'NOT HERE'"]);
        let (out, ending) = on_both(&source, &dds, reset);
        assert_eq!(out, "LEAVING\n");
        let message = ending.unwrap_err().message;
        assert_eq!(message, "the GLOBAL EXCEPTION/ERROR procedure of OUTER, run for READER, left by GO TO, GOBACK or EXIT PROGRAM, which is not supported yet");
    }
}

#[test]
fn the_nearest_containing_programs_global_declarative_runs_over_its_own_storage() {
    let path = temp("global-nearest.txt");
    let reset = || std::fs::write(&path, "ABCD\n").unwrap();
    let source = |inner_use: &str| {
        [
            cobol(&["IDENTIFICATION DIVISION.", "PROGRAM-ID. OUTER.", "ENVIRONMENT DIVISION.", "INPUT-OUTPUT SECTION.", "FILE-CONTROL."]),
            cobol(&["    SELECT GF ASSIGN TO GDD FILE STATUS IS GS.", "DATA DIVISION.", "FILE SECTION.", "FD  GF GLOBAL.", "01  G-REC PIC X(4)."]),
            cobol(&["WORKING-STORAGE SECTION.", "01  GS GLOBAL PIC XX.", "01  SEEN GLOBAL PIC 9 VALUE 0.", "PROCEDURE DIVISION.", "DECLARATIVES."]),
            cobol(&["O-ERR SECTION.", "    USE GLOBAL AFTER ERROR PROCEDURE ON INPUT.", "O-ERR-1.", "    ADD 1 TO SEEN", "    PERFORM O-SHOW.", "O-SHOW.", "    DISPLAY 'OUTER ' GS ' ' SEEN."]),
            cobol(&["END DECLARATIVES.", "MAIN SECTION.", "M.", "    CALL 'INNER'", "    DISPLAY 'OUTER AFTER ' SEEN", "    GOBACK."]),
            cobol(&["IDENTIFICATION DIVISION.", "PROGRAM-ID. INNER.", "DATA DIVISION.", "WORKING-STORAGE SECTION.", "01  MINE PIC X(5) VALUE 'INNER'.", "PROCEDURE DIVISION.", "DECLARATIVES."]),
            cobol(&["I-ERR SECTION.", inner_use, "I-ERR-1.", "    DISPLAY MINE ' ' GS.", "END DECLARATIVES.", "MAIN SECTION.", "I.", "    CALL 'DEEPEST'", "    GOBACK."]),
            cobol(&["IDENTIFICATION DIVISION.", "PROGRAM-ID. DEEPEST.", "PROCEDURE DIVISION."]),
            cobol(&["    OPEN INPUT GF", "    READ GF", "    READ GF", "    DISPLAY 'DEEPEST ' GS", "    CLOSE GF", "    GOBACK."]),
            cobol(&["END PROGRAM DEEPEST.", "END PROGRAM INNER.", "END PROGRAM OUTER."]),
        ]
        .concat()
    };
    let dds = [format!("GDD={}:text", path.display())];
    let (out, ending) = on_both(&source("    USE GLOBAL AFTER ERROR PROCEDURE ON INPUT."), &dds, reset);
    assert!(ending.is_ok(), "{ending:?}");
    assert_eq!(out, "INNER 10\nDEEPEST 10\nOUTER AFTER 0\n");
    let (out, ending) = on_both(&source("    USE AFTER ERROR PROCEDURE ON INPUT."), &dds, reset);
    assert!(ending.is_ok(), "{ending:?}");
    assert_eq!(out, "OUTER 10 1\nOUTER 10 1\nDEEPEST 10\nOUTER AFTER 1\n");
}

#[test]
fn global_local_storage_and_linkage_records_are_the_containing_activations() {
    let source = |set: &str| {
        nested(
            &["01  W PIC X(4) VALUE 'WORK'.", "LOCAL-STORAGE SECTION.", "01  L GLOBAL PIC X(3) VALUE 'LOC'.", "LINKAGE SECTION.", "01  P GLOBAL PIC X(4)."],
            &[set, "    CALL 'INNER'", "    DISPLAY 'OUTER ' L ' ' W", "    GOBACK."],
            &[],
            &["    DISPLAY 'INNER ' L", "    MOVE 'NEW' TO L", "    DISPLAY 'INNER ' P", "    MOVE 'DONE' TO P", "    GOBACK."],
            &[],
            &["    GOBACK."],
        )
    };
    let (out, ending) = on_both(&source("    SET ADDRESS OF P TO ADDRESS OF W"), &[], || ());
    assert!(ending.is_ok(), "{ending:?}");
    assert_eq!(out, "INNER LOC\nINNER WORK\nOUTER NEW DONE\n");
    let (out, ending) = on_both(&source("    CONTINUE"), &[], || ());
    assert_eq!(out, "INNER LOC\n");
    assert_eq!(ending.unwrap_err().code, AbendCode::Protection);
}

/// MAIN and SUB, each describing indexed file XK as EXTERNAL, with `main` and `sub` as their
/// procedures.
fn external_indexed(main: &[&str], sub: &[&str]) -> String {
    let program = |id: &str, prefix: &str, procedure: &[&str]| {
        let mut lines = vec![
            "IDENTIFICATION DIVISION.".to_owned(),
            format!("PROGRAM-ID. {id}."),
            "ENVIRONMENT DIVISION.".into(),
            "INPUT-OUTPUT SECTION.".into(),
            "FILE-CONTROL.".into(),
            "    SELECT XK ASSIGN TO XKDD ORGANIZATION INDEXED".into(),
            format!("        ACCESS DYNAMIC RECORD KEY {prefix}-ID FILE STATUS XS."),
            "DATA DIVISION.".into(),
            "FILE SECTION.".into(),
            "FD  XK IS EXTERNAL.".into(),
            format!("01  {prefix}-REC."),
            format!("    05  {prefix}-ID PIC X."),
            format!("    05  {prefix}-DATA PIC X(3)."),
            "WORKING-STORAGE SECTION.".into(),
            "01  XS PIC XX EXTERNAL.".into(),
            "PROCEDURE DIVISION.".into(),
        ];
        lines.extend(procedure.iter().map(|l| l.to_string()));
        lines.push(format!("END PROGRAM {id}."));
        cobol(&lines.iter().map(String::as_str).collect::<Vec<_>>())
    };
    [program("MAIN", "M", main), program("SUB", "S", sub)].concat()
}

#[test]
fn an_external_indexed_file_is_read_by_its_key_in_any_program_describing_it() {
    let path = temp("external-indexed.dat");
    let reset = || {
        let _ = std::fs::remove_file(&path);
    };
    let main = [
        "    OPEN OUTPUT XK",
        "    WRITE M-REC FROM 'AONE'",
        "    WRITE M-REC FROM 'BTWO'",
        "    CLOSE XK",
        "    OPEN I-O XK",
        "    CALL 'SUB'",
        "    DISPLAY 'MAIN ' M-REC ' ' XS",
        "    CLOSE XK WITH LOCK",
        "    CALL 'SUB'",
        "    OPEN INPUT XK",
        "    DISPLAY 'MAIN ' XS",
        "    GOBACK.",
    ];
    let sub = [
        "    MOVE 'B' TO S-ID",
        "    READ XK KEY IS S-ID INVALID KEY DISPLAY 'NO B ' XS",
        "        NOT INVALID KEY DISPLAY 'SUB ' S-DATA ' ' XS",
        "    END-READ",
        "    MOVE 'Z' TO S-ID",
        "    READ XK INVALID KEY DISPLAY 'NO Z ' XS END-READ",
        "    GOBACK.",
    ];
    let (out, ending) = on_both(&external_indexed(&main, &sub), &[format!("XKDD={}", path.display())], reset);
    assert!(ending.is_ok(), "{ending:?}");
    assert_eq!(out, "SUB TWO 00\nNO Z 23\nMAIN ZTWO 23\nMAIN 38\n");
}

#[test]
fn a_contained_program_reads_a_global_indexed_file_by_its_key() {
    let path = temp("global-indexed.dat");
    let reset = || {
        let _ = std::fs::remove_file(&path);
    };
    let source = [
        cobol(&["IDENTIFICATION DIVISION.", "PROGRAM-ID. OUTER.", "ENVIRONMENT DIVISION.", "INPUT-OUTPUT SECTION.", "FILE-CONTROL."]),
        cobol(&["    SELECT GK ASSIGN TO GKDD ORGANIZATION INDEXED", "        ACCESS DYNAMIC RECORD KEY GK-ID", "        ALTERNATE RECORD KEY GK-DATA FILE STATUS GKS."]),
        cobol(&["DATA DIVISION.", "FILE SECTION.", "FD  GK GLOBAL.", "01  GK-REC.", "    05  GK-ID PIC X.", "    05  GK-DATA PIC X(3)."]),
        cobol(&["WORKING-STORAGE SECTION.", "01  GKS GLOBAL PIC XX.", "PROCEDURE DIVISION."]),
        cobol(&["    OPEN OUTPUT GK", "    WRITE GK-REC FROM 'AONE'", "    WRITE GK-REC FROM 'BTWO'", "    CLOSE GK"]),
        cobol(&["    OPEN INPUT GK", "    CALL 'INNER'", "    DISPLAY 'OUTER ' GK-REC ' ' GKS", "    CLOSE GK", "    GOBACK."]),
        cobol(&["IDENTIFICATION DIVISION.", "PROGRAM-ID. INNER.", "PROCEDURE DIVISION."]),
        cobol(&["    MOVE 'B' TO GK-ID", "    READ GK INVALID KEY DISPLAY 'NO B'", "        NOT INVALID KEY DISPLAY 'INNER ' GK-DATA", "    END-READ"]),
        cobol(&["    MOVE 'ONE' TO GK-DATA", "    READ GK KEY IS GK-DATA", "    DISPLAY 'INNER ' GK-ID ' ' GKS"]),
        cobol(&["    MOVE 'B' TO GK-ID", "    START GK KEY >= GK-ID", "    READ GK NEXT", "    DISPLAY 'INNER ' GK-REC ' ' GKS", "    GOBACK."]),
        cobol(&["END PROGRAM INNER.", "END PROGRAM OUTER."]),
    ]
    .concat();
    let (out, ending) = on_both(&source, &[format!("GKDD={}", path.display())], reset);
    assert!(ending.is_ok(), "{ending:?}");
    assert_eq!(out, "INNER TWO\nINNER A 00\nINNER BTWO 00\nOUTER BTWO 00\n");
}

#[test]
fn a_global_file_written_as_a_print_file_by_one_program_only_ends_the_run() {
    let path = temp("global-print.txt");
    let reset = || std::fs::write(&path, "").unwrap();
    let source = global_file(&[], &[], &["    OPEN OUTPUT GF", "    WRITE G-REC"]).replace("    CALL 'READER'", "    WRITE G-REC AFTER ADVANCING 1\n           CALL 'READER'");
    let (_, ending) = on_both(&source, &[format!("GDD={}", path.display())], reset);
    assert_eq!(ending.unwrap_err().message, "IWR0074-S GF, a GLOBAL file of OUTER, is written as a print file in one of OUTER and READER and not the other, which is not supported yet");
}

#[test]
fn an_index_of_a_global_table_is_the_declaring_programs() {
    let source = cobol(&[
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. OUTER.",
        "DATA DIVISION.",
        "WORKING-STORAGE SECTION.",
        "01  TBL GLOBAL.",
        "    05  ROW OCCURS 3 TIMES INDEXED BY IDX.",
        "        10  CH PIC X.",
        "PROCEDURE DIVISION.",
        "    MOVE 'ABC' TO TBL",
        "    SET IDX TO 1",
        "    CALL 'INNER'",
        "    DISPLAY 'OUTER ' CH(IDX)",
        "    GOBACK.",
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. INNER.",
        "PROCEDURE DIVISION.",
        "    SET IDX UP BY 2",
        "    DISPLAY 'INNER ' CH(IDX)",
        "    GOBACK.",
        "END PROGRAM INNER.",
        "END PROGRAM OUTER.",
    ]);
    let walked = Harness::source(&source).run(Executor::Interpreter);
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), ("INNER C\nOUTER C\n", Some(&Ending::Goback)), "{}", walked.err);
    let vm = Harness::source(&source).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
}
