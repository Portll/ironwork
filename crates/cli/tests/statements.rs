//! `ironwork run --evidence --trace-statements`: each start of a listed statement is recorded in
//! order, matched by file name and line, up to a cap per statement.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iw-statements-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    let program = [
        "       IDENTIFICATION DIVISION.",
        "       PROGRAM-ID. LOOPER.",
        "       DATA DIVISION.",
        "       WORKING-STORAGE SECTION.",
        "       01 N PIC 9(3) VALUE 0.",
        "       PROCEDURE DIVISION.",
        "           PERFORM 150 TIMES",
        "               ADD 1 TO N",
        "           END-PERFORM.",
        "           IF N > 0",
        "               DISPLAY 'DONE'",
        "           END-IF.",
        "           GOBACK.",
    ];
    fs::write(dir.join("src/LOOPER.cbl"), program.join("\n") + "\n").unwrap();
    dir
}

fn run(dir: &Path, listed: Option<&str>, evidence: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ironwork"));
    command.arg("run").arg(dir.join("src/LOOPER.cbl"));
    if evidence {
        command.arg("--evidence").arg(dir.join("ev"));
    }
    if let Some(text) = listed {
        fs::write(dir.join("listed.txt"), text).unwrap();
        command.arg("--trace-statements").arg(dir.join("listed.txt"));
    }
    command.output().unwrap()
}

/// Each statement record's line, file and whether it is marked capped.
fn statements(dir: &Path) -> Vec<(u32, String, bool)> {
    let run = fs::read_dir(dir.join("ev/runs")).unwrap().next().unwrap().unwrap().path();
    let field = |line: &str, key: &str| -> Option<String> {
        let needle = format!("\"{key}\":");
        let rest = &line[line.find(&needle)? + needle.len()..];
        Some(match rest.strip_prefix('"') {
            Some(s) => s[..s.find('"').unwrap()].to_string(),
            None => rest[..rest.find([',', '}']).unwrap()].to_string(),
        })
    };
    fs::read_to_string(run)
        .unwrap()
        .lines()
        .filter(|l| l.contains("\"kind\":\"statement\""))
        .map(|l| (field(l, "line").unwrap().parse().unwrap(), field(l, "file").unwrap(), field(l, "capped").is_some_and(|c| c == "true")))
        .collect()
}

#[test]
fn listed_statements_are_recorded_in_order_by_file_name_and_line_up_to_the_cap() {
    let dir = temp("listed");
    let out = run(&dir, Some("src/LOOPER.cbl:8\n\nelsewhere/LOOPER.cbl:11\nOTHER.cbl:13\n"), true);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let recorded = statements(&dir);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(recorded.len(), 101, "{recorded:?}");
    assert!(recorded[..99].iter().all(|r| *r == (8, "LOOPER.cbl".into(), false)), "{recorded:?}");
    assert_eq!(recorded[99], (8, "LOOPER.cbl".into(), true));
    assert_eq!(recorded[100], (11, "LOOPER.cbl".into(), false));
}

#[test]
fn a_run_with_no_list_records_no_statements() {
    let dir = temp("unlisted");
    let out = run(&dir, None, true);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let recorded = statements(&dir);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(recorded, []);
}

#[test]
fn a_list_without_a_journal_or_not_of_file_and_line_is_refused() {
    let dir = temp("refused");
    let alone = run(&dir, Some("LOOPER.cbl:8\n"), false);
    let malformed = run(&dir, Some("LOOPER.cbl\n"), true);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(alone.status.code(), Some(246));
    assert!(String::from_utf8_lossy(&alone.stderr).contains("--trace-statements goes with --evidence, for run"));
    assert_eq!(malformed.status.code(), Some(246));
    assert!(String::from_utf8_lossy(&malformed.stderr).contains("line 1: \"LOOPER.cbl\" is not FILE:LINE"), "{}", String::from_utf8_lossy(&malformed.stderr));
}

#[test]
fn a_statement_limit_ends_the_run_with_s322_at_the_same_statement_on_both_executors_and_in_a_job() {
    let dir = temp("limit");
    let limited = |limit: &str, extra: &[&str]| Command::new(env!("CARGO_BIN_EXE_ironwork")).arg("run").arg(dir.join("src/LOOPER.cbl")).args(["--statement-limit", limit]).args(extra).output().unwrap();
    // Wherever in the loop the count runs out, the S322 is placed at the loop's first statement.
    for limit in ["100", "101", "102"] {
        for extra in [&[][..], &["--vm"]] {
            let o = limited(limit, extra);
            let err = String::from_utf8_lossy(&o.stderr);
            assert_eq!(o.status.code(), Some(240), "{limit} {extra:?} {err}");
            assert!(err.contains("LOOPER.cbl:8:16: ABEND S322:") && err.contains("in the loop over lines 8"), "{limit} {extra:?} {err}");
        }
    }
    let done = Command::new(env!("CARGO_BIN_EXE_ironwork")).arg("run").arg(dir.join("src/LOOPER.cbl")).args(["--statement-limit", "200"]).output().unwrap();
    assert!(done.status.success(), "{}", String::from_utf8_lossy(&done.stderr));

    fs::create_dir_all(dir.join("ds")).unwrap();
    fs::write(dir.join("JOB.jcl"), "//LIMIT    JOB\n//STEP1    EXEC PGM=LOOPER\n").unwrap();
    let job = Command::new(env!("CARGO_BIN_EXE_ironwork")).arg("job").arg(dir.join("JOB.jcl")).arg("--datasets").arg(dir.join("ds")).arg("-L").arg(dir.join("src")).args(["--statement-limit", "100"]).output().unwrap();
    let err = String::from_utf8_lossy(&job.stderr);
    assert!(err.contains("LOOPER.cbl:8:16: ABEND S322:") && err.contains("STEP1 PGM=LOOPER ABEND S322"), "{err}");
}
