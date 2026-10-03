//! `ironwork fuzz`: a program that adds a quantity from its input file and subscripts with an index
//! from it ends in a data exception and a range check on generated records; each abend is kept once,
//! with a journal that records it, and the same seed finds the same abends on the same inputs.

mod schema;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use schema::read_manifest;

const PROGRAM: &[&str] = &[
    "       CBL SSRANGE",
    "       IDENTIFICATION DIVISION.",
    "       PROGRAM-ID. QTYSUM.",
    "       ENVIRONMENT DIVISION.",
    "       INPUT-OUTPUT SECTION.",
    "       FILE-CONTROL.",
    "           SELECT IN-FILE ASSIGN TO INFILE.",
    "       DATA DIVISION.",
    "       FILE SECTION.",
    "       FD IN-FILE.",
    "       01 IN-REC.",
    "          05 IN-NAME PIC X(10).",
    "          05 IN-QTY  PIC 9(5).",
    "          05 IN-IDX  PIC 9(2).",
    "       WORKING-STORAGE SECTION.",
    "       01 WS-TOTAL PIC 9(9) VALUE 0.",
    "       01 WS-EOF PIC X VALUE 'N'.",
    "       01 WS-TABLE.",
    "          05 WS-SLOT PIC X(3) OCCURS 10 TIMES.",
    "       PROCEDURE DIVISION.",
    "           OPEN INPUT IN-FILE",
    "           PERFORM UNTIL WS-EOF = 'Y'",
    "              READ IN-FILE AT END MOVE 'Y' TO WS-EOF",
    "              NOT AT END",
    "                 ADD IN-QTY TO WS-TOTAL",
    "                 MOVE 'ABC' TO WS-SLOT (IN-IDX)",
    "              END-READ",
    "           END-PERFORM",
    "           CLOSE IN-FILE",
    "           GOBACK.",
];

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iw-fuzz-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("repo/src")).unwrap();
    fs::write(dir.join("repo/src/QTYSUM.cbl"), PROGRAM.join("\n") + "\n").unwrap();
    dir
}

fn fuzz(dir: &Path, out: &str, extra: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ironwork"))
        .current_dir(dir.join("repo"))
        .args(["fuzz", "src/QTYSUM.cbl", "-o"])
        .arg(dir.join(out))
        .args(extra)
        .output()
        .unwrap()
}

/// QTYSUM with each `(from, to)` replaced, written over the test's copy.
fn rewrite(dir: &Path, edits: &[(&str, &str)]) {
    let text = edits.iter().fold(PROGRAM.join("\n"), |t, (from, to)| t.replace(from, to));
    fs::write(dir.join("repo/src/QTYSUM.cbl"), text + "\n").unwrap();
}

fn stderr(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// Each kept run's abend as `code line` and its inputs' bytes, in the manifest's order.
fn kept(manifest: &str) -> Vec<String> {
    let field = |text: &str, key: &str| text.split(&format!("\"{key}\":")).skip(1).map(|r| r.split([',', '}']).next().unwrap().trim_matches('"').to_string()).collect::<Vec<_>>();
    let runs = manifest.split("\"runs\":[").nth(1).unwrap();
    let codes = field(runs, "code");
    let lines = field(runs, "line");
    let mut out: Vec<String> = codes.iter().zip(&lines).map(|(c, l)| format!("{c} {l}")).collect();
    out.extend(field(manifest.split("\"runs\":[").next().unwrap(), "bytes"));
    out
}

#[test]
fn generated_records_find_the_data_exception_and_the_range_check_each_kept_once_with_its_journal() {
    let dir = temp("find");
    let o = fuzz(&dir, "run", &["--runs", "40"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let manifest = read_manifest(&dir.join("run"));
    assert!(manifest.contains("\"tool\":\"ironwork-fuzz\""));
    assert!(manifest.contains("\"program\":{\"file\":\"src/QTYSUM.cbl\",\"id\":\"QTYSUM\"}"));
    assert!(manifest.contains("\"runs\":40"), "{manifest}");
    let found = kept(&manifest);
    assert!(found.iter().any(|k| k == "S0C7 25"), "{found:?}");
    assert!(found.iter().any(|k| k == "U4038 26"), "{found:?}");
    let places: Vec<&String> = found.iter().filter(|k| k.contains(' ')).collect();
    let mut unique = places.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(places.len(), unique.len());
    for journal in manifest.split("\"journal\":\"").skip(1).map(|r| r.split('"').next().unwrap()) {
        let text = fs::read_to_string(dir.join("run/evidence/runs").join(format!("{journal}.jsonl"))).unwrap();
        assert!(text.contains("\"kind\":\"abend\""), "{journal}");
    }
    assert!(dir.join("run/coverage/0.json").exists());
    assert!(!dir.join("run/.work").exists());
}

#[test]
fn the_same_seed_finds_the_same_abends_on_the_same_inputs() {
    let dir = temp("seed");
    for out in ["a", "b"] {
        let o = fuzz(&dir, out, &["--runs", "25", "--seed", "7"]);
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    }
    let read = |out: &str| kept(&read_manifest(&dir.join(out)));
    assert_eq!(read("a"), read("b"));
}

#[test]
fn a_directory_in_use_or_a_called_program_is_refused() {
    let dir = temp("refuse");
    fs::create_dir_all(dir.join("used")).unwrap();
    fs::write(dir.join("used/keep"), "").unwrap();
    let o = fuzz(&dir, "used", &[]);
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("is not empty"));

    let called = PROGRAM.iter().map(|l| match *l {
        "       PROCEDURE DIVISION." => "       LINKAGE SECTION.\n       01 LK-X PIC X.\n       PROCEDURE DIVISION USING LK-X.".to_string(),
        l => l.to_string(),
    });
    fs::write(dir.join("repo/src/QTYSUM.cbl"), called.collect::<Vec<_>>().join("\n") + "\n").unwrap();
    let o = fuzz(&dir, "called", &[]);
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("takes PROCEDURE DIVISION USING"));

    rewrite(&dir, &[]);
    let o = fuzz(&dir, "repo/src/out", &[]);
    assert_eq!(o.status.code(), Some(2));
    assert!(stderr(&o).contains("is inside src"), "{}", stderr(&o));
    assert!(!dir.join("repo/src/out").exists());
}

#[test]
fn an_assign_literal_that_names_a_path_reaches_nothing_outside_the_fuzz_directory() {
    let dir = temp("assign");
    fs::write(dir.join("VICTIM"), "precious").unwrap();
    rewrite(
        &dir,
        &[
            ("ASSIGN TO INFILE.", "ASSIGN TO '../../../VICTIM'.\n           SELECT OUT-FILE ASSIGN TO '../../../WRITTEN'."),
            ("       WORKING-STORAGE SECTION.", "       FD OUT-FILE.\n       01 OUT-REC PIC X(17).\n       WORKING-STORAGE SECTION."),
            ("OPEN INPUT IN-FILE", "OPEN INPUT IN-FILE OUTPUT OUT-FILE"),
            ("              NOT AT END", "              NOT AT END\n                 WRITE OUT-REC FROM IN-REC"),
            ("CLOSE IN-FILE", "CLOSE IN-FILE OUT-FILE"),
        ],
    );
    let o = fuzz(&dir, "run", &["--runs", "20"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(fs::read_to_string(dir.join("VICTIM")).unwrap(), "precious");
    assert!(!dir.join("WRITTEN").exists());
    assert!(read_manifest(&dir.join("run")).contains("\"name\":\"../../../VICTIM\""));
}

#[test]
fn files_that_share_a_dd_get_one_empty_data_set_and_the_rest_is_still_varied() {
    let dir = temp("shared");
    rewrite(
        &dir,
        &[
            ("ASSIGN TO INFILE.", "ASSIGN TO INFILE.\n           SELECT A-FILE ASSIGN TO DISK.\n           SELECT B-FILE ASSIGN TO DISK."),
            ("       WORKING-STORAGE SECTION.", "       FD A-FILE.\n       01 A-REC PIC X(50).\n       FD B-FILE.\n       01 B-REC PIC X(5).\n       WORKING-STORAGE SECTION."),
            ("           OPEN INPUT IN-FILE", "           OPEN INPUT A-FILE\n           CLOSE A-FILE\n           OPEN INPUT B-FILE\n           CLOSE B-FILE\n           OPEN INPUT IN-FILE"),
        ],
    );
    let o = fuzz(&dir, "run", &["--runs", "40"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains("not varied, given empty: DISK"), "{}", stderr(&o));
    assert!(kept(&read_manifest(&dir.join("run"))).iter().any(|k| k.starts_with("S0C7 ")));
}

#[test]
fn a_program_in_the_current_directory_is_named_relative_to_it_in_the_manifest_and_journals() {
    let dir = temp("cwd");
    let o = Command::new(env!("CARGO_BIN_EXE_ironwork"))
        .current_dir(dir.join("repo/src"))
        .args(["fuzz", "QTYSUM.cbl", "--root", "..", "--runs", "40", "-o"])
        .arg(dir.join("run"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let manifest = read_manifest(&dir.join("run"));
    assert!(manifest.contains("\"program\":{\"file\":\"src/QTYSUM.cbl\""), "{manifest}");
    assert!(kept(&manifest).iter().any(|k| k == "S0C7 25"));
    let journals = fs::read_dir(dir.join("run/evidence/runs")).unwrap().map(|e| fs::read_to_string(e.unwrap().path()).unwrap());
    for text in std::iter::once(manifest.clone()).chain(journals) {
        assert!(!text.contains("iw-fuzz-cli-cwd"), "{text}");
    }
}

#[test]
fn a_generated_indexed_file_repeats_no_alternate_key_that_allows_no_duplicates() {
    let dir = temp("altkey");
    rewrite(
        &dir,
        &[
            ("ASSIGN TO INFILE.", "ASSIGN TO INFILE\n               ORGANIZATION INDEXED ACCESS SEQUENTIAL\n               RECORD KEY IN-NAME ALTERNATE RECORD KEY IN-IDX."),
            ("                 ADD IN-QTY TO WS-TOTAL\n", ""),
            ("                 MOVE 'ABC' TO WS-SLOT (IN-IDX)\n", "                 CONTINUE\n"),
        ],
    );
    let o = fuzz(&dir, "run", &["--runs", "60"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let manifest = read_manifest(&dir.join("run"));
    assert!(manifest.contains("\"abend\":0"), "{manifest}");
}

#[test]
fn the_manifest_names_each_root_from_the_repository_root_as_the_journal_numbers_them() {
    let dir = temp("roots");
    fs::create_dir_all(dir.join("repo/copy")).unwrap();
    fs::write(dir.join("repo/copy/ADDQTY.cpy"), "                 ADD IN-QTY TO WS-TOTAL\n").unwrap();
    rewrite(&dir, &[("                 ADD IN-QTY TO WS-TOTAL", "                 COPY ADDQTY.")]);
    let o = fuzz(&dir, "run", &["--runs", "40", "-I", "copy"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let manifest = read_manifest(&dir.join("run"));
    assert!(manifest.contains("\"roots\":[\"src\",\"copy\"]"), "{manifest}");
    assert!(manifest.contains("\"code\":\"S0C7\",\"file\":\"ADDQTY.cpy\",\"line\":1"), "{manifest}");
    let journals: Vec<String> = fs::read_dir(dir.join("run/evidence/runs")).unwrap().map(|e| fs::read_to_string(e.unwrap().path()).unwrap()).collect();
    assert!(journals.iter().flat_map(|j| j.lines()).any(|l| l.contains("\"kind\":\"input\"") && l.contains("\"path\":\"ADDQTY.cpy\"") && l.contains("\"root\":1")));
}

#[test]
fn every_occurrence_of_a_table_is_varied() {
    let dir = temp("occurs");
    rewrite(
        &dir,
        &[
            ("          05 IN-IDX  PIC 9(2).", "          05 IN-IDX  PIC 9(2) OCCURS 70 TIMES."),
            ("       01 WS-TOTAL PIC 9(9) VALUE 0.", "       01 WS-TOTAL PIC 9(9) VALUE 0.\n       01 WS-I PIC 9(2) VALUE 1."),
            ("                 ADD IN-QTY TO WS-TOTAL\n", ""),
            (
                "                 MOVE 'ABC' TO WS-SLOT (IN-IDX)",
                "                 IF IN-IDX (70) IS NUMERIC\n                    MOVE IN-IDX (70) TO WS-I\n                    MOVE 'ABC' TO WS-SLOT (WS-I)\n                 END-IF",
            ),
        ],
    );
    let o = fuzz(&dir, "run", &["--runs", "30"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(kept(&read_manifest(&dir.join("run"))).iter().any(|k| k.starts_with("U4038 ")));
}

fn count(manifest: &str, key: &str) -> i64 {
    manifest.split(&format!("\"{key}\":")).nth(1).and_then(|r| r.split([',', '}']).next()).and_then(|n| n.parse().ok()).unwrap()
}

#[test]
fn variable_length_records_are_fed_behind_rdws() {
    let dir = temp("varying");
    rewrite(
        &dir,
        &[
            ("       FD IN-FILE.", "       FD IN-FILE RECORD VARYING FROM 10 TO 17 DEPENDING ON WS-LEN."),
            ("       01 WS-TOTAL PIC 9(9) VALUE 0.", "       01 WS-TOTAL PIC 9(9) VALUE 0.\n       01 WS-LEN PIC 9(4) COMP."),
        ],
    );
    let o = fuzz(&dir, "run", &["--runs", "60"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let manifest = read_manifest(&dir.join("run"));
    assert_eq!(count(&manifest, "refused"), 0, "{manifest}");
    assert!(kept(&manifest).iter().any(|k| k.starts_with("S0C7 ")), "{manifest}");
}

#[test]
fn a_relative_file_is_fed_a_record_a_slot() {
    let dir = temp("relative");
    rewrite(&dir, &[("ASSIGN TO INFILE.", "ASSIGN TO INFILE\n               ORGANIZATION RELATIVE ACCESS SEQUENTIAL.")]);
    let o = fuzz(&dir, "run", &["--runs", "40"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let manifest = read_manifest(&dir.join("run"));
    assert!(kept(&manifest).iter().any(|k| k.starts_with("S0C7 ")), "{manifest}");
}

#[test]
fn a_contained_program_s_file_gets_its_dd() {
    let dir = temp("contained");
    rewrite(
        &dir,
        &[
            ("           OPEN INPUT IN-FILE", "           CALL 'AUXREAD'\n           OPEN INPUT IN-FILE"),
            (
                "           GOBACK.",
                "           GOBACK.\n       IDENTIFICATION DIVISION.\n       PROGRAM-ID. AUXREAD.\n       ENVIRONMENT DIVISION.\n       INPUT-OUTPUT SECTION.\n       FILE-CONTROL.\n           SELECT AUX-FILE ASSIGN TO AUXDD.\n       DATA DIVISION.\n       FILE SECTION.\n       FD AUX-FILE.\n       01 AUX-REC PIC X(5).\n       PROCEDURE DIVISION.\n           OPEN INPUT AUX-FILE\n           CLOSE AUX-FILE\n           GOBACK.\n       END PROGRAM AUXREAD.\n       END PROGRAM QTYSUM.",
            ),
        ],
    );
    let o = fuzz(&dir, "run", &["--runs", "40"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains("not varied, given empty: AUXDD"), "{}", stderr(&o));
    assert!(kept(&read_manifest(&dir.join("run"))).iter().any(|k| k.starts_with("S0C7 ")));
}

#[test]
fn a_program_s_return_code_of_2_is_not_a_refusal_and_a_refusal_says_why() {
    let dir = temp("rc2");
    rewrite(&dir, &[("           GOBACK.", "           MOVE 2 TO RETURN-CODE\n           GOBACK.")]);
    let o = fuzz(&dir, "run", &["--runs", "20"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(count(&read_manifest(&dir.join("run")), "refused"), 0);

    rewrite(&dir, &[("           OPEN INPUT IN-FILE", "           CALL 'NOSUCH'\n           OPEN INPUT IN-FILE")]);
    let o = fuzz(&dir, "called", &["--runs", "5"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains("5 runs refused; the first: QTYSUM.cbl:21 S806 CALL NOSUCH"), "{}", stderr(&o));
}

const PARM_PROGRAM: &[&str] = &[
    "       CBL SSRANGE",
    "       IDENTIFICATION DIVISION.",
    "       PROGRAM-ID. PARMSUM.",
    "       DATA DIVISION.",
    "       WORKING-STORAGE SECTION.",
    "       01 WS-NUM PIC 9(5).",
    "       01 WS-TOTAL PIC 9(9) VALUE 0.",
    "       01 WS-TABLE.",
    "          05 WS-SLOT PIC X(3) OCCURS 10 TIMES.",
    "       LINKAGE SECTION.",
    "       01 PARM-AREA.",
    "          05 PARM-LEN  PIC S9(4) COMP.",
    "          05 PARM-TEXT PIC X(100).",
    "       PROCEDURE DIVISION USING PARM-AREA.",
    "           DISPLAY 'PARM ' PARM-LEN ' ' PARM-TEXT (1:PARM-LEN + 1)",
    "           IF PARM-LEN >= 5",
    "              MOVE PARM-TEXT (1:5) TO WS-NUM",
    "              ADD WS-NUM TO WS-TOTAL",
    "              MOVE 'ABC' TO WS-SLOT (PARM-LEN)",
    "           END-IF",
    "           GOBACK.",
];

#[test]
fn run_passes_a_parm_as_language_environment_does_and_fuzz_varies_it() {
    let dir = temp("parm");
    fs::write(dir.join("repo/src/PARMSUM.cbl"), PARM_PROGRAM.join("\n") + "\n").unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_ironwork")).current_dir(dir.join("repo")).args(["run", "src/PARMSUM.cbl", "--parm", "00042/RPTOPTS(ON)"]).output().unwrap();
    assert!(String::from_utf8_lossy(&run.stdout).starts_with("PARM 0005 00042"), "{}{}", String::from_utf8_lossy(&run.stdout), stderr(&run));

    let o = Command::new(env!("CARGO_BIN_EXE_ironwork")).current_dir(dir.join("repo")).args(["fuzz", "src/PARMSUM.cbl", "--runs", "60", "-o"]).arg(dir.join("run")).output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let manifest = read_manifest(&dir.join("run"));
    assert!(manifest.contains("\"kind\":\"parm\",\"minimized\""), "{manifest}");
    let found = kept(&manifest);
    assert!(found.iter().any(|k| k == "S0C7 18"), "{found:?}");
    assert!(found.iter().any(|k| k == "U4038 19"), "{found:?}");
}

#[test]
fn fuzz_refuses_a_flag_it_would_not_use() {
    let dir = temp("flags");
    for extra in [&["--declare", "x"][..], &["--sql-record", "x"], &["--proclib", "x"], &["--transid", "T"], &["--parm", "X"], &["--job", "--cics"], &["--datasets", "x"], &["--job", "--datasets", "x:text"], &["--step-parm", "S=X"]] {
        let o = fuzz(&dir, "run", extra);
        assert_eq!(o.status.code(), Some(2), "{extra:?}");
        assert!(!dir.join("run").exists(), "{extra:?}");
    }
    let o = Command::new(env!("CARGO_BIN_EXE_ironwork")).current_dir(dir.join("repo")).args(["check", "src/QTYSUM.cbl", "--parm", "X"]).output().unwrap();
    assert_eq!(o.status.code(), Some(2));
    assert!(stderr(&o).contains("--parm is for run"), "{}", stderr(&o));
}

const SYSIN_PROGRAM: &[&str] = &[
    "       IDENTIFICATION DIVISION.",
    "       PROGRAM-ID. CARDSUM.",
    "       DATA DIVISION.",
    "       WORKING-STORAGE SECTION.",
    "       01 WS-CARD PIC X(5).",
    "       01 WS-NUM REDEFINES WS-CARD PIC 9(5).",
    "       01 WS-TOTAL PIC 9(9) VALUE 0.",
    "       PROCEDURE DIVISION.",
    "           ACCEPT WS-CARD",
    "           ADD WS-NUM TO WS-TOTAL",
    "           GOBACK.",
];

#[test]
fn a_job_s_data_sets_in_stream_data_and_step_parms_are_fuzzed_and_each_abend_placed_in_its_program() {
    let dir = temp("job");
    fs::write(dir.join("repo/src/PARMSUM.cbl"), PARM_PROGRAM.join("\n") + "\n").unwrap();
    fs::write(dir.join("repo/src/CARDSUM.cbl"), SYSIN_PROGRAM.join("\n") + "\n").unwrap();
    fs::create_dir_all(dir.join("repo/jcl")).unwrap();
    let jcl = [
        "//FUZZJOB  JOB",
        "//STEP1    EXEC PGM=QTYSUM",
        "//INFILE   DD DSN=MY.INPUT,DISP=SHR",
        "//STEP2    EXEC PGM=PARMSUM,PARM='00001',COND=EVEN",
        "//STEP3    EXEC PGM=CARDSUM,COND=EVEN",
        "//SYSIN    DD *",
        "00042",
        "/*",
    ];
    fs::write(dir.join("repo/jcl/FUZZ.jcl"), jcl.join("\n") + "\n").unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_ironwork")).current_dir(dir.join("repo")).args(["fuzz", "--job", "jcl/FUZZ.jcl", "-L", "src", "--runs", "60", "-o"]).arg(dir.join("run")).output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let manifest = read_manifest(&dir.join("run"));
    assert!(manifest.contains("\"entry\":\"job\""), "{manifest}");
    assert!(manifest.contains("\"program\":{\"file\":\"jcl/FUZZ.jcl\",\"id\":\"FUZZJOB\"}"), "{manifest}");
    for (kind, name) in [("dd", "MY.INPUT"), ("parm", "STEP2"), ("sysin", "STEP3.SYSIN")] {
        assert!(manifest.contains(&format!("\"kind\":\"{kind}\",\"minimized\":")) && manifest.contains(&format!("\"name\":\"{name}\"")), "{kind} {name}: {manifest}");
    }
    let found = kept(&manifest);
    for place in ["S0C7 25", "U4038 19", "S0C7 10"] {
        assert!(found.iter().any(|k| k == place), "{place}: {found:?}");
    }
    assert!(manifest.contains("\"file\":\"QTYSUM.cbl\""), "{manifest}");
    for journal in manifest.split("\"journal\":\"").skip(1).map(|r| r.split('"').next().unwrap()) {
        let text = fs::read_to_string(dir.join("run/evidence/runs").join(format!("{journal}.jsonl"))).unwrap();
        assert!(text.lines().any(|l| l.contains("\"kind\":\"abend\"") && l.contains("\"line\":")), "{journal}: {text}");
    }
    assert!(dir.join("run/coverage/0.json").exists());
}

#[test]
fn a_loop_only_some_input_causes_is_kept_as_s322_and_one_waiting_at_the_end_of_sysin_is_not() {
    let dir = temp("hang");
    rewrite(
        &dir,
        &[
            ("          05 IN-IDX  PIC 9(2).", "          05 IN-IDX  PIC 9(2).\n          05 IN-DIG  PIC 9."),
            ("       01 WS-EOF PIC X VALUE 'N'.", "       01 WS-EOF PIC X VALUE 'N'.\n       01 WS-N PIC 9(4) COMP VALUE 0."),
            ("                 ADD IN-QTY TO WS-TOTAL\n", "                 MOVE 0 TO WS-N\n                 IF IN-DIG IS NUMERIC\n                    PERFORM UNTIL WS-N = IN-DIG\n                       ADD 2 TO WS-N\n                    END-PERFORM\n                 END-IF\n"),
            ("                 MOVE 'ABC' TO WS-SLOT (IN-IDX)\n", ""),
        ],
    );
    let o = fuzz(&dir, "run", &["--runs", "30", "--timeout", "1", "--hang-limit", "20000"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let manifest = read_manifest(&dir.join("run"));
    assert!(kept(&manifest).iter().any(|k| k == "S322 29" || k == "S322 30"), "{manifest}");

    let waits = [
        "       IDENTIFICATION DIVISION.",
        "       PROGRAM-ID. WAITER.",
        "       DATA DIVISION.",
        "       WORKING-STORAGE SECTION.",
        "       01 WS-CARD PIC X(3).",
        "       PROCEDURE DIVISION.",
        "           PERFORM UNTIL WS-CARD = 'END'",
        "              ACCEPT WS-CARD",
        "           END-PERFORM",
        "           GOBACK.",
    ];
    fs::write(dir.join("repo/src/WAITER.cbl"), waits.join("\n") + "\n").unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_ironwork")).current_dir(dir.join("repo")).args(["fuzz", "src/WAITER.cbl", "--runs", "5", "--timeout", "1", "--hang-limit", "20000", "-o"]).arg(dir.join("waits")).output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let manifest = read_manifest(&dir.join("waits"));
    assert!(!manifest.contains("S322") && count(&manifest, "timeout") > 0, "{manifest}");
}

#[test]
fn an_s806_is_kept_only_where_a_marker_in_the_input_reaches_the_call() {
    let dir = temp("chosen");
    let program = |static_call: bool| {
        let call = if static_call { "           CALL 'NOSUCH'" } else { "           CALL WS-PGM" };
        [
            "       IDENTIFICATION DIVISION.",
            "       PROGRAM-ID. PICKER.",
            "       DATA DIVISION.",
            "       WORKING-STORAGE SECTION.",
            "       01 WS-PGM PIC X(8).",
            "       PROCEDURE DIVISION.",
            "           ACCEPT WS-PGM",
            call,
            "           GOBACK.",
        ]
        .join("\n")
            + "\n"
    };
    fs::write(dir.join("repo/src/PICKER.cbl"), program(false)).unwrap();
    let picker = |out: &str| Command::new(env!("CARGO_BIN_EXE_ironwork")).current_dir(dir.join("repo")).args(["fuzz", "src/PICKER.cbl", "--runs", "20", "-o"]).arg(dir.join(out)).output().unwrap();
    let o = picker("chosen");
    assert!(o.status.success(), "{}", stderr(&o));
    let manifest = read_manifest(&dir.join("chosen"));
    assert!(kept(&manifest).iter().any(|k| k == "S806 8"), "{manifest}");
    assert!(manifest.contains("CALL @#$"), "{manifest}");
    let journal = manifest.split("\"journal\":\"").nth(1).unwrap().split('"').next().unwrap();
    let text = fs::read_to_string(dir.join("chosen/evidence/runs").join(format!("{journal}.jsonl"))).unwrap();
    assert!(text.lines().any(|l| l.contains("\"sink\":\"dynamic-program-load\"") && l.contains("\"reached\":true")), "{text}");

    fs::write(dir.join("repo/src/PICKER.cbl"), program(true)).unwrap();
    let o = picker("static");
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(!read_manifest(&dir.join("static")).contains("S806"));
}

/// Each kept run's abend as `code line optimized`.
fn optimized(manifest: &str) -> Vec<String> {
    let runs = manifest.split("\"runs\":[").nth(1).unwrap();
    runs.split("\"abend\":{")
        .skip(1)
        .map(|a| {
            let abend = a.split('}').next().unwrap();
            let field = |key: &str| abend.split(&format!("\"{key}\":")).nth(1).unwrap().split(',').next().unwrap().trim_matches('"').to_string();
            format!("{} {} {}", field("code"), field("line"), field("optimized"))
        })
        .collect()
}

#[test]
fn each_kept_abend_says_whether_its_input_gives_it_again_compiled_with_optimize_2() {
    let dir = temp("optimized");
    rewrite(&dir, &[("                 ADD IN-QTY TO WS-TOTAL", "                 IF IN-QTY NOT = ZERO\n                    ADD IN-QTY TO WS-TOTAL\n                 END-IF")]);
    let o = fuzz(&dir, "run", &["--runs", "40"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let found = optimized(&read_manifest(&dir.join("run")));
    assert!(found.contains(&"S0C7 25 false".to_string()), "{found:?}");
    assert!(found.contains(&"U4038 28 true".to_string()), "{found:?}");

    let o = fuzz(&dir, "optimized", &["--runs", "40", "--optimize=2"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let found = optimized(&read_manifest(&dir.join("optimized")));
    assert!(found.contains(&"S0C7 26 true".to_string()), "{found:?}");
    assert!(!found.iter().any(|k| k.starts_with("S0C7 25")), "{found:?}");
}
