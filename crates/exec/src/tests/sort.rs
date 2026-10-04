use super::*;

fn text_file(name: &str, lines: &[&str]) -> std::path::PathBuf {
    let path = temp(name);
    std::fs::write(&path, lines.iter().map(|l| format!("{l}\n")).collect::<String>()).unwrap();
    path
}

fn dd(name: &str, path: &std::path::Path) -> String {
    format!("{name}={}:text", path.display())
}

fn compile_errors(source: &str) -> String {
    let parsed = syntax::parse(source).unwrap_or_else(|e| panic!("{e}"));
    compile(parsed, &[]).err().map(|e| e.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n")).unwrap_or_default()
}

const SELECT_SD: &str = "           SELECT S-FILE ASSIGN TO SORTWK1.\n";

#[test]
fn sort_using_two_files_giving_one_by_mixed_keys() {
    let (a, b, out) = (text_file("sort-a.txt", &["D1050X", "D2010Y", "D1200Z"]), text_file("sort-b.txt", &["D2300W", "D1050V"]), temp("sort-out.txt"));
    let source = file_program(
        &[SELECT_SD, "           SELECT IN-A ASSIGN TO ADD FILE STATUS FS-A.\n", "           SELECT IN-B ASSIGN TO BDD.\n", "           SELECT OUT-F ASSIGN TO ODD.\n"].concat(),
        concat!(
            "       SD  S-FILE.\n       01  S-REC.\n           05 S-DEPT PIC X(2).\n           05 S-AMT PIC 9(3).\n           05 S-TAG PIC X.\n",
            "       FD  IN-A.\n       01  A-REC PIC X(6).\n       FD  IN-B.\n       01  B-REC PIC X(6).\n       FD  OUT-F.\n       01  O-REC PIC X(6).\n",
        ),
        "       01  FS-A PIC XX.\n       01  RC PIC 99.\n",
        &[
            line("SORT S-FILE ON ASCENDING KEY S-DEPT DESCENDING KEY S-AMT"),
            line("    USING IN-A IN-B GIVING OUT-F"),
            line("MOVE SORT-RETURN TO RC"),
            line("DISPLAY RC ' ' FS-A"),
            line("GOBACK."),
        ]
        .concat(),
    );
    let (stdout, err, ending) = run_files(&source, &[dd("ADD", &a), dd("BDD", &b), dd("ODD", &out)]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(stdout, "00 00\n");
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "D1200Z\nD1050X\nD1050V\nD2300W\nD2010Y\n");
}

#[test]
fn input_and_output_procedures_with_packed_and_binary_keys() {
    let source = file_program(
        SELECT_SD,
        "       SD  S-FILE.\n       01  S-REC.\n           05 S-P PIC S9(3) COMP-3.\n           05 S-B PIC S9(4) COMP.\n           05 S-T PIC X.\n",
        "       01  DONE PIC X VALUE 'N'.\n       01  E-P PIC ---9.\n       01  E-B PIC ----9.\n       01  W PIC X(5).\n",
        &[
            "       MAIN-LINE.\n",
            &line("SORT S-FILE ASCENDING S-P DESCENDING S-B"),
            &line("    WITH DUPLICATES IN ORDER"),
            &line("    INPUT PROCEDURE IS FEED OUTPUT PROCEDURE SHOW-ALL"),
            &line("DISPLAY 'AFTER ' SORT-RETURN"),
            &line("GOBACK."),
            "       FEED.\n",
            &line("MOVE -5 TO S-P MOVE 10 TO S-B MOVE 'A' TO S-T RELEASE S-REC"),
            &line("MOVE 3 TO S-P MOVE -2 TO S-B MOVE 'B' TO S-T RELEASE S-REC"),
            &line("MOVE -5 TO S-P MOVE 300 TO S-B MOVE 'C' TO S-T RELEASE S-REC"),
            &line("MOVE -100 TO S-P MOVE 0 TO S-B MOVE 'D' TO S-T RELEASE S-REC"),
            &line("MOVE 3 TO S-P MOVE -2 TO S-B MOVE 'E' TO S-T RELEASE S-REC"),
            &line("MOVE 0 TO S-P MOVE -7 TO S-B MOVE 'F' TO S-T RELEASE S-REC"),
            &line("MOVE 'G' TO S-T MOVE -5 TO S-P MOVE 10 TO S-B"),
            &line("MOVE S-REC TO W MOVE SPACES TO S-REC"),
            &line("RELEASE S-REC FROM W."),
            "       SHOW-ALL.\n",
            &line("PERFORM UNTIL DONE = 'Y'"),
            &line("    RETURN S-FILE INTO W AT END MOVE 'Y' TO DONE"),
            &line("    NOT AT END MOVE S-P TO E-P MOVE S-B TO E-B"),
            &line("        DISPLAY S-T E-P E-B"),
            &line("    END-RETURN"),
            &line("END-PERFORM"),
            &line("RETURN S-FILE AT END DISPLAY 'STILL AT END' END-RETURN."),
        ]
        .concat(),
    );
    let (out, err, ending) = run_files(&source, &[]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "D-100    0\nC  -5  300\nA  -5   10\nG  -5   10\nF   0   -7\nB   3   -2\nE   3   -2\nSTILL AT END\nAFTER 0000\n");
}

#[test]
fn a_go_to_out_of_an_input_procedure_comes_back_through_its_end() {
    let source = file_program(
        SELECT_SD,
        "       SD  S-FILE.\n       01  S-REC PIC X.\n",
        "       01  DONE PIC X VALUE 'N'.\n",
        &[
            "       MAIN-LINE.\n",
            &line("SORT S-FILE DESCENDING S-REC"),
            &line("    INPUT PROCEDURE P-IN THRU P-EXIT OUTPUT PROCEDURE P-OUT"),
            &line("DISPLAY 'END'"),
            &line("GOBACK."),
            "       P-IN.\n",
            &line("MOVE 'A' TO S-REC RELEASE S-REC"),
            &line("GO TO OUTSIDE."),
            "       P-EXIT.\n",
            &line("EXIT."),
            "       P-OUT.\n",
            &line("PERFORM UNTIL DONE = 'Y'"),
            &line("    RETURN S-FILE AT END MOVE 'Y' TO DONE"),
            &line("    NOT AT END DISPLAY S-REC END-RETURN"),
            &line("END-PERFORM."),
            "       OUTSIDE.\n",
            &line("MOVE 'B' TO S-REC RELEASE S-REC"),
            &line("GO TO P-EXIT."),
        ]
        .concat(),
    );
    let (out, err, ending) = run_files(&source, &[]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "B\nA\nEND\n");
}

#[test]
fn merge_two_ordered_files_and_one_out_of_order_by_its_selection() {
    let (a, b, c, out) = (
        text_file("merge-a.txt", &["A1a", "B1a", "D1a"]),
        text_file("merge-b.txt", &["B1b", "C1b", "D1b"]),
        text_file("merge-c.txt", &["B1c", "A1c"]),
        temp("merge-out.txt"),
    );
    let fd = |f: &str| format!("       FD  {f}.\n       01  {f}-REC PIC X(3).\n");
    let source = file_program(
        &[SELECT_SD, "           SELECT IN-A ASSIGN TO ADD.\n", "           SELECT IN-B ASSIGN TO BDD.\n", "           SELECT IN-C ASSIGN TO CDD.\n", "           SELECT OUT-F ASSIGN TO ODD.\n"].concat(),
        &["       SD  S-FILE.\n       01  S-REC.\n           05 S-K PIC X(2).\n           05 S-T PIC X.\n", &fd("IN-A"), &fd("IN-B"), &fd("IN-C"), &fd("OUT-F")].concat(),
        "       01  RC PIC 99.\n",
        &[
            "       MAIN-LINE.\n",
            &line("MERGE S-FILE ON ASCENDING KEY S-K USING IN-A IN-B"),
            &line("    GIVING OUT-F"),
            &line("MOVE SORT-RETURN TO RC"),
            &line("DISPLAY 'FIRST ' RC"),
            &line("MERGE S-FILE ON ASCENDING KEY S-K USING IN-A IN-C"),
            &line("    OUTPUT PROCEDURE SHOW-ALL"),
            &line("MOVE SORT-RETURN TO RC"),
            &line("DISPLAY 'SECOND ' RC"),
            &line("GOBACK."),
            "       SHOW-ALL.\n",
            &line("PERFORM 5 TIMES"),
            &line("    RETURN S-FILE AT END CONTINUE"),
            &line("        NOT AT END DISPLAY S-REC END-RETURN"),
            &line("END-PERFORM."),
        ]
        .concat(),
    );
    let (stdout, err, ending) = run_files(&source, &[dd("ADD", &a), dd("BDD", &b), dd("CDD", &c), dd("ODD", &out)]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(stdout, "FIRST 00\nA1a\nB1a\nB1c\nA1c\nD1a\nSECOND 00\n");
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "A1a\nB1a\nB1b\nC1b\nD1a\nD1b\n");
}

#[test]
fn sort_return_16_stops_the_sort_and_failures_set_it() {
    let source = file_program(
        &[SELECT_SD, "           SELECT OPTIONAL IN-A ASSIGN TO NODD FILE STATUS FS-A.\n", "           SELECT IN-B ASSIGN TO NODD2 FILE STATUS FS-B.\n"].concat(),
        concat!(
            "       SD  S-FILE RECORD VARYING FROM 2 TO 6.\n       01  S-SHORT PIC X(2).\n       01  S-LONG.\n           05 S-A PIC X(2).\n           05 S-K PIC X(4).\n",
            "       FD  IN-A.\n       01  A-REC PIC X(6).\n       FD  IN-B.\n       01  B-REC PIC X(6).\n",
        ),
        "       01  FS-A PIC XX.\n       01  FS-B PIC XX.\n       01  RC PIC 99.\n",
        &[
            "       MAIN-LINE.\n",
            &line("SORT S-FILE ON ASCENDING KEY S-K"),
            &line("    INPUT PROCEDURE FEED-THEN-STOP"),
            &line("    OUTPUT PROCEDURE NEVER-RUN"),
            &line("MOVE SORT-RETURN TO RC"),
            &line("DISPLAY 'STOPPED ' RC"),
            &line("SORT S-FILE ON ASCENDING KEY S-K INPUT PROCEDURE SHORT-ONE"),
            &line("    OUTPUT PROCEDURE NEVER-RUN"),
            &line("MOVE SORT-RETURN TO RC"),
            &line("DISPLAY 'SHORT ' RC"),
            &line("SORT S-FILE ON ASCENDING KEY S-K USING IN-A GIVING IN-B"),
            &line("MOVE SORT-RETURN TO RC"),
            &line("DISPLAY 'OPTIONAL EMPTY ' RC ' ' FS-A ' ' FS-B"),
            &line("SORT S-FILE ON ASCENDING KEY S-K USING IN-B GIVING IN-A"),
            &line("MOVE SORT-RETURN TO RC"),
            &line("DISPLAY 'NO DD ' RC ' ' FS-B"),
            &line("GOBACK."),
            "       FEED-THEN-STOP.\n",
            &line("MOVE 'AAKKKK' TO S-LONG RELEASE S-LONG"),
            &line("MOVE 16 TO SORT-RETURN"),
            &line("DISPLAY 'STOPPING'"),
            &line("RELEASE S-LONG"),
            &line("DISPLAY 'NOT REACHED'."),
            "       SHORT-ONE.\n",
            &line("RELEASE S-SHORT FROM 'XY'."),
            "       NEVER-RUN.\n",
            &line("DISPLAY 'NEVER'."),
        ]
        .concat(),
    );
    let (out, err, ending) = run_files(&source, &[]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "STOPPING\nSTOPPED 16\nSHORT 16\nOPTIONAL EMPTY 16 00 35\nNO DD 16 35\n");
    assert!(err.contains("ends inside a key"), "{err}");
}

#[test]
fn a_using_file_with_no_dd_and_no_file_status_ends_the_run() {
    let source = file_program(
        &[SELECT_SD, "           SELECT IN-A ASSIGN TO NODD.\n           SELECT OUT-F ASSIGN TO NODD2.\n"].concat(),
        "       SD  S-FILE.\n       01  S-REC PIC X(4).\n       FD  IN-A.\n       01  A-REC PIC X(4).\n       FD  OUT-F.\n       01  O-REC PIC X(4).\n",
        "",
        &[line("SORT S-FILE ON ASCENDING KEY S-REC USING IN-A GIVING OUT-F"), line("GOBACK.")].concat(),
    );
    let (_, _, ending) = run_files(&source, &[]);
    assert_eq!(ending.unwrap_err().code, "IO-35");
}

#[test]
fn giving_indexed_and_relative_files() {
    let (input, ksds, rrds) = (text_file("sort-kin.txt", &["C3", "A1", "B2"]), temp("sort-ksds.txt"), temp("sort-rrds.txt"));
    let _ = (std::fs::remove_file(&ksds), std::fs::remove_file(&rrds));
    let source = file_program(
        &[
            SELECT_SD,
            "           SELECT IN-F ASSIGN TO IDD.\n",
            "           SELECT K-F ASSIGN TO KDD ORGANIZATION INDEXED\n               RECORD KEY K-ID.\n",
            "           SELECT R-F ASSIGN TO RDD ORGANIZATION RELATIVE\n               RELATIVE KEY RK.\n",
        ]
        .concat(),
        concat!(
            "       SD  S-FILE.\n       01  S-REC PIC X(2).\n       FD  IN-F.\n       01  I-REC PIC X(2).\n",
            "       FD  K-F.\n       01  K-REC.\n           05 K-ID PIC X.\n           05 FILLER PIC X.\n       FD  R-F.\n       01  R-REC PIC X(2).\n",
        ),
        "       01  RK PIC 9(4).\n",
        &[line("SORT S-FILE ON ASCENDING KEY S-REC USING IN-F GIVING K-F R-F"), line("DISPLAY RK ' ' SORT-RETURN"), line("GOBACK.")].concat(),
    );
    let (out, err, ending) = run_files(&source, &[dd("IDD", &input), dd("KDD", &ksds), dd("RDD", &rrds)]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "0003 0000\n");
    assert_eq!(std::fs::read_to_string(&ksds).unwrap(), "A1\nB2\nC3\n");
    assert_eq!(std::fs::read_to_string(&rrds).unwrap(), "A1\nB2\nC3\n");
}

#[test]
fn table_sort_by_its_occurs_keys_by_named_keys_and_by_the_element() {
    let out = run(&program(
        "",
        concat!(
            "       01  T.\n           05 E OCCURS 5 ASCENDING KEY IS E-K.\n",
            "              10 E-K PIC S9(3) COMP-3.\n              10 E-N PIC X.\n              10 E-F COMP-2.\n",
            "       01  G VALUE 'DBEAC'.\n           05 L PIC X OCCURS 5.\n",
            "       01  I PIC 9.\n",
        ),
        &[
            line("MOVE 30 TO E-K(1) MOVE 'C' TO E-N(1) MOVE 2.5 TO E-F(1)"),
            line("MOVE -4 TO E-K(2) MOVE 'A' TO E-N(2) MOVE -1 TO E-F(2)"),
            line("MOVE 12 TO E-K(3) MOVE 'B' TO E-N(3) MOVE 100 TO E-F(3)"),
            line("MOVE -4 TO E-K(4) MOVE 'D' TO E-N(4) MOVE 0 TO E-F(4)"),
            line("MOVE 7 TO E-K(5) MOVE 'E' TO E-N(5) MOVE 0.25 TO E-F(5)"),
            line("SORT E"),
            line("PERFORM VARYING I FROM 1 BY 1 UNTIL I > 5"),
            line("    DISPLAY E-N(I) WITH NO ADVANCING END-PERFORM"),
            line("DISPLAY ' '"),
            line("SORT E ON DESCENDING KEY E-F"),
            line("PERFORM VARYING I FROM 1 BY 1 UNTIL I > 5"),
            line("    DISPLAY E-N(I) WITH NO ADVANCING END-PERFORM"),
            line("DISPLAY ' '"),
            line("SORT L ON ASCENDING KEY"),
            line("DISPLAY G"),
            line("SORT L DESCENDING"),
            line("DISPLAY G"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    assert_eq!(out, "ADEBC \nBCEDA \nABCDE\nEDCBA\n");
}

/// A file SORT's alphanumeric keys follow its COLLATING SEQUENCE, else the PROGRAM COLLATING
/// SEQUENCE; a table SORT's follow only the phrase (TABLE_SORT_COLLATION).
#[test]
fn collating_sequences_order_alphanumeric_sort_keys() {
    let source = |sorts: &str| {
        [
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n",
            "       OBJECT-COMPUTER. IBM-370 PROGRAM COLLATING SEQUENCE IS BACK.\n",
            "       SPECIAL-NAMES. ALPHABET BACK IS 'Z' THROUGH 'A'\n           ALPHABET ASCII IS STANDARD-1.\n",
            "       INPUT-OUTPUT SECTION.\n       FILE-CONTROL.\n",
            SELECT_SD,
            "       DATA DIVISION.\n       FILE SECTION.\n       SD  S-FILE.\n       01  S-REC.\n           05 S-X PIC X(2).\n           05 S-N PIC 9.\n",
            "       WORKING-STORAGE SECTION.\n       01  T VALUE 'a1A111B1'.\n           05 E PIC X(2) OCCURS 4.\n       01  DONE PIC X.\n",
            "       PROCEDURE DIVISION.\n       MAIN-LINE.\n",
            sorts,
            &line("GOBACK."),
            "       FEED.\n",
            &line("MOVE 'a1' TO S-X RELEASE S-REC MOVE 'A1' TO S-X RELEASE S-REC"),
            &line("MOVE '11' TO S-X RELEASE S-REC"),
            &line("MOVE 'B1' TO S-X RELEASE S-REC."),
            "       SHOW.\n",
            &line("MOVE 'N' TO DONE"),
            &line("PERFORM UNTIL DONE = 'Y'"),
            &line("    RETURN S-FILE AT END MOVE 'Y' TO DONE"),
            &line("    NOT AT END DISPLAY S-X WITH NO ADVANCING END-RETURN"),
            &line("END-PERFORM"),
            &line("DISPLAY ' '."),
        ]
        .concat()
    };
    let sorts = [
        line("SORT S-FILE ASCENDING S-X COLLATING SEQUENCE ASCII"),
        line("    INPUT PROCEDURE FEED OUTPUT PROCEDURE SHOW"),
        line("SORT S-FILE ASCENDING S-X INPUT PROCEDURE FEED"),
        line("    OUTPUT PROCEDURE SHOW"),
        line("SORT E ASCENDING KEY E"),
        line("DISPLAY T"),
        line("SORT E ASCENDING KEY E COLLATING SEQUENCE ASCII"),
        line("DISPLAY T"),
    ]
    .concat();
    let (out, err, ending) = run_files(&source(&sorts), &[]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "11A1B1a1 \nB1A1a111 \na1A1B111\n11A1B1a1\n");
    let undefined = source(&line("SORT E ASCENDING KEY E COLLATING SEQUENCE NOPE"));
    assert!(compile_errors(&undefined).contains("COLLATING SEQUENCE NOPE: not an alphabet-name of SPECIAL-NAMES"));
}

#[test]
fn what_is_not_modelled_is_refused_by_name() {
    let errors = |fd: &str, data: &str, body: &str| {
        let select = [SELECT_SD, "           SELECT F ASSIGN TO FDD.\n"].concat();
        let fd = ["       SD  S-FILE.\n       01  S-REC.\n           05 S-K PIC X(2).\n       FD  F.\n       01  F-REC PIC X(2).\n", fd].concat();
        compile_errors(&file_program(&select, &fd, data, &[line(body), "       P.\n".into(), line("EXIT.")].concat()))
    };
    assert!(errors("", "", "SORT F ON ASCENDING KEY F-REC USING F GIVING F.").contains("not a sort or merge file (SD)"));
    assert!(errors("", "", "SORT S-FILE ON ASCENDING KEY F-REC USING F GIVING F.").contains("must be in its records"));
    assert!(errors("", "", "SORT S-FILE ON ASCENDING KEY S-K USING S-FILE GIVING F.").contains("a sort or merge file (SD) cannot be one"));
    assert!(errors("", "", "SORT S-FILE ON ASCENDING KEY S-K USING F.").contains("no GIVING or OUTPUT PROCEDURE"));
    assert!(errors("", "", "MERGE S-FILE ON ASCENDING KEY S-K USING F GIVING F.").contains("USING names at least two files"));
    assert!(errors("", "", "RELEASE F-REC.").contains("not a record of a sort file"));
    assert!(errors("", "", "RETURN F AT END CONTINUE.").contains("not a sort or merge file"));
    assert!(errors("", "       01  T.\n           05 E PIC X OCCURS 3.\n", "SORT E ON ASCENDING KEY E USING F.").contains("a table SORT takes no USING"));
    assert!(errors("", "       01  T.\n           05 E PIC X OCCURS 3.\n", "SORT E.").contains("no KEY phrase"));
    assert!(errors("", "       01  W PIC X.\n", "SORT W ON ASCENDING KEY W.").contains("not a table"));
    assert!(errors("", "", "SORT S-FILE ON ASCENDING KEY S-K INPUT PROCEDURE P GIVING F.").is_empty());
}

#[test]
fn release_outside_an_input_procedure_and_a_sort_control_dd_stop_the_run() {
    let control = text_file("sort-igzsrtcd.txt", &[" OPTION EQUALS"]);
    let source = file_program(
        &[SELECT_SD, "           SELECT F ASSIGN TO FDD.\n"].concat(),
        "       SD  S-FILE.\n       01  S-REC PIC X(2).\n       FD  F.\n       01  F-REC PIC X(2).\n",
        "",
        &["       MAIN-LINE.\n", &line("RELEASE S-REC"), &line("GOBACK."), "       P.\n", &line("EXIT.")].concat(),
    );
    let (_, _, ending) = run_files(&source, &[]);
    assert!(ending.unwrap_err().message.contains("no SORT input procedure is running"));
    let sorting = source.replace("RELEASE S-REC", "SORT S-FILE ON ASCENDING KEY S-REC INPUT PROCEDURE P GIVING F");
    let (_, _, ending) = run_files(&sorting, &[dd("IGZSRTCD", &control)]);
    assert!(ending.unwrap_err().message.contains("DD IGZSRTCD holds sort control statements"));
}

#[test]
fn an_output_procedure_generates_a_report_whose_declaratives_come_first() {
    let out = temp("sort-report.txt");
    let _ = std::fs::remove_file(&out);
    let source = file_program(
        &[SELECT_SD, "           SELECT P ASSIGN TO PDD.\n"].concat(),
        "       SD  S-FILE.\n       01  S-REC.\n           05 S-K PIC X.\n       FD  P REPORT IS R.\n",
        concat!(
            "       01  DONE PIC X VALUE 'N'.\n       01  SEEN PIC 9 VALUE 0.\n       REPORT SECTION.\n       RD  R.\n",
            "       01  D TYPE DE LINE PLUS 1.\n           05 COLUMN 1 PIC X SOURCE S-K.\n           05 COLUMN 3 PIC 9 SOURCE SEEN.\n",
        ),
        &[
            "       DECLARATIVES.\n       U SECTION.\n           USE BEFORE REPORTING D.\n       U-1.\n",
            &line("ADD 1 TO SEEN."),
            "       END DECLARATIVES.\n       M SECTION.\n",
            &line("OPEN OUTPUT P INITIATE R"),
            &line("SORT S-FILE ON DESCENDING KEY S-K"),
            &line("    INPUT PROCEDURE FEED OUTPUT PROCEDURE SHOW"),
            &line("TERMINATE R CLOSE P"),
            &line("DISPLAY 'SEEN ' SEEN"),
            &line("GOBACK."),
            "       FEED SECTION.\n",
            &line("MOVE 'A' TO S-K RELEASE S-REC"),
            &line("MOVE 'C' TO S-K RELEASE S-REC"),
            &line("MOVE 'B' TO S-K RELEASE S-REC."),
            "       SHOW SECTION.\n",
            &line("PERFORM UNTIL DONE = 'Y'"),
            &line("    RETURN S-FILE AT END MOVE 'Y' TO DONE"),
            &line("    NOT AT END GENERATE D END-RETURN"),
            &line("END-PERFORM."),
        ]
        .concat(),
    );
    let (stdout, err, ending) = run_files(&source, &[dd("PDD", &out)]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(stdout, "SEEN 3\n");
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "C 1\nB 2\nA 3\n");
}

#[test]
fn object_references_are_no_sort_keys_and_an_sd_writes_no_report() {
    let source = |key: &str, body: &str| {
        [
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n       REPOSITORY.\n           CLASS Account IS \"Account\".\n",
            "       INPUT-OUTPUT SECTION.\n       FILE-CONTROL.\n",
            SELECT_SD,
            "       DATA DIVISION.\n       FILE SECTION.\n       SD  S-FILE.\n       01  S-REC.\n           05 S-X PIC X(4).\n           05 S-O USAGE OBJECT REFERENCE Account.\n",
            "       WORKING-STORAGE SECTION.\n       01  A USAGE OBJECT REFERENCE Account.\n       01  DONE PIC X.\n",
            "       01  T.\n           05 E OCCURS 3.\n               10 E-O USAGE OBJECT REFERENCE Account.\n",
            "       PROCEDURE DIVISION.\n",
            &line(&format!("SORT S-FILE ASCENDING {key} INPUT PROCEDURE P")),
            &line("    OUTPUT PROCEDURE Q"),
            &line(body),
            "       P.\n",
            &line("RELEASE S-REC."),
            "       Q.\n",
            &line("RETURN S-FILE AT END MOVE 'Y' TO DONE END-RETURN."),
        ]
        .concat()
    };
    // Object references want THREAD, which refuses a file SORT; without it the program compiles with a warning.
    assert_eq!(compile_errors(&source("S-X", "GOBACK.")), "");
    assert!(compile_errors(&source("S-O", "GOBACK.")).contains("S-O: a POINTER, INDEX, object reference or function-pointer item cannot be a sort key"));
    assert!(compile_errors(&source("S-X", "SORT E ON ASCENDING KEY E-O.")).contains("E-O: a POINTER, INDEX, object reference or function-pointer item cannot be a sort key"));
    assert!(compile_errors(&source("S-X", "RELEASE S-REC FROM A.")).contains("A is an object reference"));
    assert!(compile_errors(&source("S-X", "RETURN S-FILE INTO A AT END CONTINUE END-RETURN.")).contains("A is an object reference"));
    assert!(compile_errors(&source("S-X", "RETURN S-FILE AT END MOVE A TO DONE END-RETURN.")).contains("A is an object reference"));
    let sd_report = file_program(SELECT_SD, "       SD  S-FILE REPORT IS R.\n       01  S-REC PIC X.\n", "", &line("GOBACK."));
    assert!(syntax::parse(&sd_report).unwrap_err().message.contains("SD S-FILE: a sort or merge file takes no REPORT clause"));
}

fn run_flagged(source: &str, dds: &[String], flags: &[&str]) -> (String, String, Result<Ending, Abend>) {
    let flags: Vec<String> = flags.iter().map(|f| f.to_string()).collect();
    let compiled = compile(syntax::parse(source).unwrap_or_else(|e| panic!("{e}")), &flags).unwrap_or_else(|e| panic!("{e:?}"));
    crate::testing::check_lowering(&compiled, rt::sql::fingerprint(&format!("{source}\n{}", flags.join(" "))), None);
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let ending = compiled.run_with(files::Dds::new(dds, false).unwrap(), &mut out, &mut err);
    (String::from_utf8(out).unwrap(), String::from_utf8(err).unwrap(), ending)
}

#[test]
fn same_record_area_shares_one_area_and_same_area_shares_among_vsam_files() {
    let input = text_file("same-in.txt", &["ABCD"]);
    let (out, k1, k2) = (temp("same-out.txt"), temp("same-k1.txt"), temp("same-k2.txt"));
    let source = file_program(
        &[
            SELECT_SD,
            "           SELECT IN-F ASSIGN TO IDD.\n           SELECT OUT-F ASSIGN TO ODD.\n",
            "           SELECT K1 ASSIGN TO K1DD ORGANIZATION INDEXED\n               RECORD KEY K1-ID.\n",
            "           SELECT K2 ASSIGN TO K2DD ORGANIZATION INDEXED\n               RECORD KEY K2-ID.\n",
            "       I-O-CONTROL.\n           SAME RECORD AREA FOR IN-F OUT-F S-FILE\n           SAME K1 K2 OUT-F.\n",
        ]
        .concat(),
        concat!(
            "       SD  S-FILE.\n       01  S-REC PIC X(4).\n       FD  IN-F.\n       01  IN-REC PIC X(4).\n",
            "       FD  OUT-F.\n       01  OUT-REC PIC X(6).\n       FD  K1.\n       01  K1-ID PIC XX.\n       FD  K2.\n       01  K2-ID PIC XX.\n",
        ),
        "",
        &[
            "       MAIN-LINE.\n",
            &line("MOVE ALL '*' TO OUT-REC"),
            &line("OPEN INPUT IN-F OUTPUT OUT-F"),
            &line("READ IN-F"),
            &line("DISPLAY OUT-REC ' ' S-REC"),
            &line("WRITE OUT-REC"),
            &line("CLOSE IN-F OUT-F"),
            &line("MOVE 'K1' TO K1-ID"),
            &line("DISPLAY K2-ID ' ' OUT-REC"),
            &line("SORT S-FILE ON DESCENDING KEY S-REC"),
            &line("    INPUT PROCEDURE P-IN OUTPUT PROCEDURE P-OUT"),
            &line("GOBACK."),
            "       P-IN.\n",
            &line("OPEN INPUT IN-F READ IN-F RELEASE S-REC CLOSE IN-F"),
            &line("MOVE 'ZZZZ' TO S-REC RELEASE S-REC."),
            "       P-OUT.\n",
            &line("RETURN S-FILE AT END CONTINUE END-RETURN"),
            &line("DISPLAY IN-REC."),
        ]
        .concat(),
    );
    let dds = [dd("IDD", &input), dd("ODD", &out), dd("K1DD", &k1), dd("K2DD", &k2)];
    let (stdout, err, ending) = run_files(&source, &dds);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(stdout, "ABCD** ABCD\nK1 ABCD**\nZZZZ\n");
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "ABCD**\n");
    let refused = compile_errors(&source.replace("SAME K1 K2 OUT-F.", "SAME K1 NOPE."));
    assert!(refused.contains("SAME AREA names NOPE, which is not a file"), "{refused}");
}

#[test]
fn zoned_and_packed_keys_compare_as_dfsort_does_unless_strict() {
    let records = |zoned: bool| {
        let key = |z: &str, p: &str| format!("X'{}'", if zoned { z } else { p });
        [key("F0F1D5", "015D"), key("F0F0DA", "00AD"), key("F0F0D0", "000D"), key("F0F0C0", "000C"), key("404040", "0004"), key("F1F2F3", "123C")]
    };
    for zoned in [true, false] {
        let usage = if zoned { "" } else { " COMP-3" };
        let feed: String = records(zoned)
            .iter()
            .zip(["F", "E", "A", "B", "C", "D"])
            .rev()
            .map(|(key, tag)| [line(&format!("MOVE {key} TO S-KX MOVE '{tag}' TO S-T")), line("RELEASE S-REC")].concat())
            .collect();
        let source = file_program(
            SELECT_SD,
            &format!("       SD  S-FILE.\n       01  S-REC.\n           05 S-K PIC S9(3){usage}.\n           05 S-KX REDEFINES S-K PIC X({}).\n           05 S-T PIC X.\n", if zoned { 3 } else { 2 }),
            "       01  DONE PIC X VALUE 'N'.\n",
            &[
                "       MAIN-LINE.\n",
                &line("SORT S-FILE ASCENDING S-K INPUT PROCEDURE P-IN"),
                &line("    OUTPUT PROCEDURE P-OUT"),
                &line("DISPLAY ' '"),
                &line("GOBACK."),
                "       P-IN.\n",
                &feed,
                "       P-OUT.\n",
                &line("PERFORM UNTIL DONE = 'Y'"),
                &line("    RETURN S-FILE AT END MOVE 'Y' TO DONE"),
                &line("    NOT AT END DISPLAY S-T WITH NO ADVANCING END-RETURN"),
                &line("END-PERFORM."),
            ]
            .concat(),
        );
        let (out, err, ending) = run_flagged(&source, &[], &[]);
        assert!(ending.is_ok(), "{ending:?} {err}");
        assert_eq!(out, "FEACBD \n", "zoned {zoned}");
        let (_, _, strict) = run_flagged(&source, &[], &["-strict-sort-keys"]);
        assert_eq!(strict.unwrap_err().code, "S0C7", "zoned {zoned}");
    }
}

fn fastsrt_program(select: &str, fd: &str, sort: &str) -> String {
    file_program(
        &[SELECT_SD, select].concat(),
        &["       SD  S-FILE.\n       01  S-REC PIC X(2).\n", fd].concat(),
        "       01  FS-A PIC XX VALUE 'XX'.\n       01  FS-O PIC XX VALUE 'YY'.\n       01  RK PIC 9(4) VALUE 7.\n       01  RC PIC 99.\n",
        &[line(sort), line("MOVE SORT-RETURN TO RC"), line("DISPLAY RC ' ' FS-A ' ' FS-O ' ' RK"), line("GOBACK.")].concat(),
    )
}

#[test]
fn fastsrt_leaves_file_status_and_relative_key_alone_and_fails_the_sort_on_an_io_error() {
    let input = text_file("fastsrt-in.txt", &["B2", "A1"]);
    let (out, rel) = (temp("fastsrt-out.txt"), temp("fastsrt-rel.txt"));
    let select = "           SELECT IN-A ASSIGN TO ADD FILE STATUS FS-A.\n           SELECT OUT-F ASSIGN TO ODD FILE STATUS FS-O.\n           SELECT R-F ASSIGN TO RDD ORGANIZATION RELATIVE\n               RELATIVE KEY RK.\n";
    let fd = "       FD  IN-A.\n       01  A-REC PIC X(2).\n       FD  OUT-F.\n       01  O-REC PIC X(2).\n       FD  R-F.\n       01  R-REC PIC X(2).\n";
    let dds = [dd("ADD", &input), dd("ODD", &out), dd("RDD", &rel)];
    let using_giving = fastsrt_program(select, fd, "SORT S-FILE ASCENDING S-REC USING IN-A GIVING OUT-F");
    let (stdout, err, ending) = run_flagged(&format!("       CBL FASTSRT\n{using_giving}"), &dds, &[]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(stdout, "00 XX YY 0007\n");
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "A1\nB2\n");
    assert!(err.contains("FASTSRT: DFSORT does the I/O of USING IN-A, so its FILE STATUS FS-A is not updated by the SORT"), "{err}");
    let (stdout, err, _) = run_flagged(&using_giving, &dds, &[]);
    assert_eq!(stdout, "00 00 00 0007\n");
    assert!(err.contains("under FASTSRT, DFSORT would do the I/O of GIVING OUT-F and its FILE STATUS FS-O would not be updated by the SORT"), "{err}");
    let (_, err, _) = run_flagged(&using_giving, &dds, &["-silent"]);
    assert!(err.is_empty(), "{err}");

    let relative = fastsrt_program(select, fd, "SORT S-FILE ASCENDING S-REC USING IN-A GIVING R-F");
    let (stdout, err, _) = run_flagged(&format!("       CBL FASTSRT\n{relative}"), &dds, &[]);
    assert_eq!(stdout, "00 XX YY 0007\n");
    assert!(err.contains("RELATIVE KEY RK is not updated"), "{err}");
    assert_eq!(run_flagged(&relative, &dds, &[]).0, "00 00 YY 0002\n");

    let missing = fastsrt_program(
        "           SELECT IN-A ASSIGN TO NODD.\n           SELECT OUT-F ASSIGN TO ODD.\n",
        "       FD  IN-A.\n       01  A-REC PIC X(2).\n       FD  OUT-F.\n       01  O-REC PIC X(2).\n",
        "SORT S-FILE ASCENDING S-REC USING IN-A GIVING OUT-F",
    );
    let (stdout, err, ending) = run_flagged(&format!("       CBL FASTSRT\n{missing}"), &dds, &[]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(stdout, "16 XX YY 0007\n");
    assert_eq!(run_flagged(&missing, &dds, &[]).2.unwrap_err().code, "IO-35");
}

#[test]
fn fastsrt_names_why_it_cannot_apply() {
    let (a, b) = (text_file("fastsrt-a.txt", &["A1"]), text_file("fastsrt-b.txt", &["B1"]));
    let out = temp("fastsrt-out2.txt");
    let dds = [dd("ADD", &a), dd("BDD", &b), dd("ODD", &out), dd("LDD", &b), dd("WDD", &b)];
    let select = [
        "           SELECT IN-A ASSIGN TO ADD FILE STATUS FS-A.\n           SELECT IN-B ASSIGN TO BDD.\n",
        "           SELECT OUT-F ASSIGN TO ODD FILE STATUS FS-O.\n           SELECT LS-F ASSIGN TO LDD\n               ORGANIZATION LINE SEQUENTIAL.\n",
        "           SELECT WIDE ASSIGN TO WDD.\n",
    ]
    .concat();
    let fd = "       FD  IN-A.\n       01  A-REC PIC X(2).\n       FD  IN-B.\n       01  B-REC PIC X(2).\n       FD  OUT-F.\n       01  O-REC PIC X(2).\n       FD  LS-F.\n       01  L-REC PIC X(2).\n       FD  WIDE.\n       01  W-REC PIC X(3).\n";
    let report = |sort: &str| {
        let source = format!("       CBL FASTSRT\n{}", fastsrt_program(&select, fd, sort));
        let (stdout, err, ending) = run_flagged(&source, &dds, &[]);
        assert!(ending.is_ok(), "{ending:?} {err}");
        (stdout, err)
    };
    let (stdout, err) = report("SORT S-FILE ASCENDING S-REC USING IN-A IN-B GIVING OUT-F");
    assert_eq!(stdout, "00 00 YY 0007\n");
    assert!(err.contains("FASTSRT does not apply to USING IN-A: USING names more than one file; COBOL does its I/O"), "{err}");
    assert!(report("SORT S-FILE ASCENDING S-REC USING LS-F GIVING OUT-F").1.contains("USING LS-F: it is a line-sequential file"));
    assert!(report("SORT S-FILE ASCENDING S-REC USING WIDE GIVING OUT-F").1.contains("its largest record is 3 bytes and the SD's 2"));
    assert!(report("SORT S-FILE ASCENDING S-REC USING IN-A GIVING IN-A").1.contains("GIVING IN-A: it is also the USING file"));
    assert!(report("MERGE S-FILE ASCENDING S-REC USING IN-A IN-B GIVING OUT-F").1.contains("USING IN-A: it applies only to SORT"));
}

#[test]
fn fastsrt_leaves_linage_and_adv_print_output_to_cobol() {
    let input = text_file("fastsrt-print-in.txt", &["B2", "A1"]);
    let out = temp("fastsrt-print-out.bin");
    let dds = [dd("ADD", &input), format!("ODD={}", out.display())];
    let run = |options: &str, fd: &str| {
        let source = format!(
            "       CBL {options}\n{}",
            file_program(
                &[SELECT_SD, "           SELECT IN-A ASSIGN TO ADD.\n           SELECT OUT-F ASSIGN TO ODD FILE STATUS FS-O.\n"].concat(),
                &["       SD  S-FILE.\n       01  S-REC PIC X(2).\n       FD  IN-A.\n       01  A-REC PIC X(2).\n", fd, "       01  O-REC PIC X(2).\n"].concat(),
                "       01  FS-O PIC XX VALUE 'YY'.\n       01  RC PIC 99.\n",
                &[
                    line("SORT S-FILE ASCENDING S-REC USING IN-A GIVING OUT-F"),
                    line("MOVE SORT-RETURN TO RC"),
                    line("DISPLAY RC ' ' FS-O"),
                    line("GOBACK."),
                    "       NEVER-RUN.\n".into(),
                    line("WRITE O-REC AFTER ADVANCING 2 LINES."),
                ]
                .concat(),
            )
        );
        let _ = std::fs::remove_file(&out);
        let (stdout, err, ending) = run_flagged(&source, &dds, &[]);
        assert!(ending.is_ok(), "{ending:?} {err}");
        (stdout, err, std::fs::read(&out).unwrap_or_default())
    };
    let cobol_writes = [0x40, 0xC1, 0xF1, 0x40, 0xC2, 0xF2];
    let (stdout, err, written) = run("FASTSRT", "       FD  OUT-F.\n");
    assert!(err.contains("FASTSRT does not apply to GIVING OUT-F: it is a print file, whose records ADV makes a byte longer than its FD's 2 (--fastsrt-adv-print=exclude); COBOL does its I/O"), "{err}");
    assert_eq!((stdout.as_str(), written.as_slice()), ("00 00\n", &cobol_writes[..]));
    let (stdout, err, _) = run("FASTSRT,NOADV", "       FD  OUT-F.\n");
    assert!(err.contains("FASTSRT: DFSORT does the I/O of GIVING OUT-F, so its FILE STATUS FS-O is not updated by the SORT"), "{err}");
    assert_eq!(stdout, "00 YY\n");
    for options in ["FASTSRT", "FASTSRT,NOADV"] {
        let (stdout, err, written) = run(options, "       FD  OUT-F LINAGE IS 60.\n");
        assert!(err.contains("FASTSRT does not apply to GIVING OUT-F: its FD has LINAGE; COBOL does its I/O"), "{options}: {err}");
        assert_eq!((stdout.as_str(), written.as_slice()), ("00 00\n", &cobol_writes[..]), "{options}");
    }
}

/// Runs `source` under a CBL card of `options`, `out` removed first: standard output, standard
/// error and the bytes then in `out`.
fn run_cbl(options: &str, source: &str, flags: &[&str], dds: &[String], out: &std::path::Path) -> (String, String, Vec<u8>) {
    let _ = std::fs::remove_file(out);
    let (stdout, err, ending) = run_flagged(&format!("       CBL {options}\n{source}"), dds, flags);
    assert!(ending.is_ok(), "{ending:?} {err}");
    (stdout, err, std::fs::read(out).unwrap_or_default())
}

/// A SORT of two-byte records. P-IN and OUT-F are print files, written with ADVANCING in a
/// paragraph that never runs; IN-A and PLAIN are not.
fn print_sort_program(sort: &str) -> String {
    file_program(
        &[
            SELECT_SD,
            "           SELECT IN-A ASSIGN TO ADD.\n           SELECT P-IN ASSIGN TO PDD.\n",
            "           SELECT OUT-F ASSIGN TO ODD FILE STATUS FS-O.\n           SELECT PLAIN ASSIGN TO QDD.\n",
        ]
        .concat(),
        concat!(
            "       SD  S-FILE.\n       01  S-REC PIC X(2).\n       FD  IN-A.\n       01  A-REC PIC X(2).\n       FD  P-IN.\n       01  P-REC PIC X(2).\n",
            "       FD  OUT-F.\n       01  O-REC PIC X(2).\n       FD  PLAIN.\n       01  Q-REC PIC X(2).\n",
        ),
        "       01  FS-O PIC XX VALUE 'YY'.\n       01  RC PIC 99.\n",
        &[
            line(sort),
            line("MOVE SORT-RETURN TO RC"),
            line("DISPLAY RC ' ' FS-O"),
            line("GOBACK."),
            "       FEED.\n".into(),
            line("DISPLAY 'FEEDING'"),
            line("MOVE 'B2' TO S-REC RELEASE S-REC"),
            line("MOVE 'A1' TO S-REC RELEASE S-REC."),
            "       SHOW.\n".into(),
            line("PERFORM 3 TIMES RETURN S-FILE AT END DISPLAY 'END'"),
            line("    NOT AT END DISPLAY S-REC END-RETURN END-PERFORM."),
            "       NEVER-RUN.\n".into(),
            line("WRITE P-REC AFTER ADVANCING 2 LINES"),
            line("WRITE O-REC AFTER ADVANCING 2 LINES."),
        ]
        .concat(),
    )
}

#[test]
fn fastsrt_writes_a_noadv_print_giving_file_as_the_sd_holds_its_records() {
    let (input, out) = (text_file("fsnoadv-in.txt", &["B2", "A1"]), temp("fsnoadv-out.bin"));
    let dds = [dd("ADD", &input), format!("ODD={}", out.display())];
    let source = print_sort_program("SORT S-FILE ASCENDING S-REC USING IN-A GIVING OUT-F");
    let unchanged = "the I/O of GIVING OUT-F, a print file under NOADV, whose records DFSORT writes as the SD holds them, with no printer control character";
    let (stdout, err, written) = run_cbl("NOADV", &source, &[], &dds, &out);
    assert_eq!((stdout.as_str(), written.as_slice()), ("00 00\n", &[0x40, 0xF1, 0x40, 0xF2][..]));
    assert!(err.contains(&format!("under FASTSRT, DFSORT would do {unchanged}")), "{err}");
    let (stdout, err, written) = run_cbl("FASTSRT,NOADV", &source, &[], &dds, &out);
    assert_eq!((stdout.as_str(), written.as_slice()), ("00 YY\n", &[0xC1, 0xF1, 0xC2, 0xF2][..]));
    assert!(err.contains(&format!("FASTSRT: DFSORT does {unchanged}")), "{err}");
    assert!(run_cbl("FASTSRT,NOADV", &source, &["-silent"], &dds, &out).1.is_empty());
}

#[test]
fn fastsrt_adv_print_include_pads_the_giving_records_or_fails_the_sort() {
    let (input, out) = (text_file("fsinc-in.txt", &["B2", "A1"]), temp("fsinc-out.bin"));
    let dds = [dd("ADD", &input), format!("ODD={}", out.display())];
    let include = ["--fastsrt-adv-print=include"];
    let source = print_sort_program("SORT S-FILE ASCENDING S-REC USING IN-A GIVING OUT-F");
    for (options, flags) in [("FASTSRT", &[][..]), ("NOFASTSRT", &include[..])] {
        let (stdout, _, written) = run_cbl(options, &source, flags, &dds, &out);
        assert_eq!((stdout.as_str(), written.as_slice()), ("00 00\n", &[0x40, 0xC1, 0xF1, 0x40, 0xC2, 0xF2][..]), "{options} {flags:?}");
    }
    let padded = "the I/O of GIVING OUT-F, a print file under ADV taken by --fastsrt-adv-print=include, whose records DFSORT writes with no printer control character; DFSORT pads each record with X'00' to the data set's 3 bytes (ICE171I)";
    assert!(run_cbl("NOFASTSRT", &source, &include, &dds, &out).1.contains(&format!("under FASTSRT, DFSORT would do {padded}")));
    let (stdout, err, written) = run_cbl("FASTSRT", &source, &include, &dds, &out);
    assert_eq!((stdout.as_str(), written.as_slice()), ("00 YY\n", &[0xC1, 0xF1, 0x00, 0xC2, 0xF2, 0x00][..]));
    assert!(err.contains(&format!("FASTSRT: DFSORT does {padded}")), "{err}");

    let fed = print_sort_program("SORT S-FILE ASCENDING S-REC INPUT PROCEDURE FEED GIVING OUT-F");
    let (stdout, err, written) = run_cbl("FASTSRT", &fed, &include, &dds, &out);
    assert_eq!((stdout.as_str(), written.as_slice()), ("16 YY\n", &[][..]));
    assert!(err.contains("SORT S-FILE failed: GIVING OUT-F's data set has 3-byte records, and with no USING file of its own DFSORT does not pad the SD's 2-byte records: ICE043A"), "{err}");
    let (stdout, _, written) = run_cbl("FASTSRT", &fed, &[], &dds, &out);
    assert_eq!((stdout.as_str(), written.as_slice()), ("FEEDING\n00 00\n", &[0x40, 0xC1, 0xF1, 0x40, 0xC2, 0xF2][..]));
}

#[test]
fn fastsrt_adv_print_include_reads_the_control_character_as_the_records_first_byte() {
    let (print, out) = (temp("fsinc-print.bin"), temp("fsinc-plain.bin"));
    std::fs::write(&print, [0xF1, 0xC1, 0xF1, 0x40, 0xC2, 0xF2]).unwrap();
    let dds = [format!("PDD={}", print.display()), format!("QDD={}", out.display())];
    let include = ["--fastsrt-adv-print=include"];
    let source = print_sort_program("SORT S-FILE ASCENDING S-REC USING P-IN GIVING PLAIN");
    for (options, flags) in [("NOFASTSRT", &include[..]), ("FASTSRT", &[][..])] {
        let (stdout, _, written) = run_cbl(options, &source, flags, &dds, &out);
        assert_eq!((stdout.as_str(), written.as_slice()), ("00 YY\n", &[0xC1, 0xF1, 0xC2, 0xF2][..]), "{options}");
    }
    let (stdout, err, written) = run_cbl("FASTSRT", &source, &include, &dds, &out);
    assert_eq!((stdout.as_str(), written.as_slice()), ("00 YY\n", &[0x40, 0xC2, 0xF1, 0xC1][..]));
    assert!(
        err.contains("FASTSRT: DFSORT does the I/O of USING P-IN, a print file under ADV taken by --fastsrt-adv-print=include, whose records DFSORT reads as its data set holds them, 3 bytes with the printer control character first, so each key is read a byte before where the FD has it"),
        "{err}"
    );
    assert!(err.contains("FASTSRT: DFSORT does the I/O of GIVING PLAIN; DFSORT cuts each 3-byte record to the data set's 2 bytes (ICE171I)"), "{err}");
    let shown = print_sort_program("SORT S-FILE ASCENDING S-REC USING P-IN OUTPUT PROCEDURE SHOW");
    assert_eq!(run_cbl("FASTSRT", &shown, &include, &dds, &out).0, " B\n1A\nEND\n00 YY\n");
    assert_eq!(run_cbl("FASTSRT", &shown, &[], &dds, &out).0, "A1\nB2\nEND\n00 YY\n");
}

#[test]
fn fastsrt_adv_print_include_fails_a_variable_record_longer_than_the_giving_data_set() {
    let (print, out) = (temp("fsinc-vprint.bin"), temp("fsinc-vout.bin"));
    std::fs::write(&print, [0, 7, 0, 0, 0x40, 0xC1, 0xF1, 0, 6, 0, 0, 0x40, 0xC2]).unwrap();
    let dds = [format!("VDD={}", print.display()), format!("WDD={}", out.display())];
    let source = file_program(
        &[SELECT_SD, "           SELECT V-IN ASSIGN TO VDD.\n           SELECT V-OUT ASSIGN TO WDD.\n"].concat(),
        concat!(
            "       SD  S-FILE RECORD VARYING FROM 1 TO 2.\n       01  S-REC.\n           05 S-K PIC X.\n           05 S-T PIC X.\n",
            "       FD  V-IN RECORD VARYING FROM 1 TO 2.\n       01  V-REC PIC X(2).\n       FD  V-OUT RECORD VARYING FROM 1 TO 2.\n       01  W-REC PIC X(2).\n",
        ),
        "       01  RC PIC 99.\n",
        &[
            line("SORT S-FILE ASCENDING S-K USING V-IN GIVING V-OUT"),
            line("MOVE SORT-RETURN TO RC"),
            line("DISPLAY RC"),
            line("GOBACK."),
            "       NEVER-RUN.\n".into(),
            line("WRITE V-REC AFTER ADVANCING 1 LINE."),
        ]
        .concat(),
    );
    let cobol = [0, 6, 0, 0, 0xC1, 0xF1, 0, 5, 0, 0, 0xC2];
    for options in ["NOFASTSRT", "FASTSRT"] {
        let (stdout, _, written) = run_cbl(options, &source, &[], &dds, &out);
        assert_eq!((stdout.as_str(), written.as_slice()), ("00\n", &cobol[..]), "{options}");
    }
    let (stdout, err, _) = run_cbl("FASTSRT", &source, &["--fastsrt-adv-print=include"], &dds, &out);
    assert_eq!(stdout, "16\n");
    assert!(err.contains("SORT S-FILE failed: a 3-byte record is longer than the largest of GIVING V-OUT's data set, 2 bytes: ICE217A"), "{err}");
}
