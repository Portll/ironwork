//! `ironwork compare`: a refactor is equivalent, a dropped ROUNDED diverges at the record it
//! changes, a declared change is equivalent as declared, and --expected checks outputs against
//! given files.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iw-compare-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Interest on each amount in IN, one line each to OUT; `compute` is the arithmetic statement.
fn program(dir: &Path, name: &str, compute: &str, paragraph: &str) -> PathBuf {
    let lines = [
        "       IDENTIFICATION DIVISION.".to_string(),
        "       PROGRAM-ID. INTEREST.".into(),
        "       ENVIRONMENT DIVISION.".into(),
        "       INPUT-OUTPUT SECTION.".into(),
        "       FILE-CONTROL.".into(),
        "           SELECT IN-FILE ASSIGN TO IN.".into(),
        "           SELECT OUT-FILE ASSIGN TO OUT.".into(),
        "       DATA DIVISION.".into(),
        "       FILE SECTION.".into(),
        "       FD IN-FILE.".into(),
        "       01 IN-REC PIC 9(5)V99.".into(),
        "       FD OUT-FILE.".into(),
        "       01 OUT-REC PIC 9(5)V99.".into(),
        "       WORKING-STORAGE SECTION.".into(),
        "       01 WS-EOF PIC X VALUE 'N'.".into(),
        "       PROCEDURE DIVISION.".into(),
        "           OPEN INPUT IN-FILE OUTPUT OUT-FILE.".into(),
        format!("           PERFORM {paragraph} UNTIL WS-EOF = 'Y'."),
        "           CLOSE IN-FILE OUT-FILE.".into(),
        "           GOBACK.".into(),
        format!("       {paragraph}."),
        "           READ IN-FILE AT END MOVE 'Y' TO WS-EOF".into(),
        "           NOT AT END".into(),
        format!("               {compute}"),
        "               WRITE OUT-REC".into(),
        "           END-READ.".into(),
    ];
    let path = dir.join(name);
    fs::write(&path, lines.join("\n") + "\n").unwrap();
    path
}

fn compare(dir: &Path, base: Option<&Path>, head: &Path, extra: &[String]) -> (Output, String) {
    fs::write(dir.join("in.txt"), "0001000\n0000333\n0012345\n").unwrap();
    let statement = dir.join("statement.json");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ironwork"));
    cmd.arg("compare").arg("--head").arg(head).arg("--dd").arg(format!("IN={}:text", dir.join("in.txt").display())).arg("--dd").arg(format!("OUT={}:text", dir.join("out.txt").display())).arg("--statement").arg(&statement);
    if let Some(b) = base {
        cmd.arg("--base").arg(b);
    }
    cmd.args(extra);
    let out = cmd.output().unwrap();
    (out, fs::read_to_string(statement).unwrap_or_default())
}

const ROUNDED: &str = "COMPUTE OUT-REC ROUNDED = IN-REC * 1.035";
const TRUNCATED: &str = "COMPUTE OUT-REC = IN-REC * 1.035";

#[test]
fn a_refactor_is_equivalent() {
    let dir = temp("refactor");
    let base = program(&dir, "BASE.cbl", ROUNDED, "EACH-AMOUNT");
    let head = program(&dir, "HEAD.cbl", ROUNDED, "NEXT-AMOUNT");
    let (out, st) = compare(&dir, Some(&base), &head, &[]);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(st.contains("\"verdict\":\"equivalent\""));
    assert!(st.contains("\"name\":\"base:BASE.cbl\"") && st.contains("\"name\":\"head:HEAD.cbl\""));
    assert!(st.contains("\"coverage\":null"), "coverage is not claimed");
    assert!(!dir.join("out.txt").exists(), "neither run wrote the caller's file");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_dropped_rounded_diverges_at_the_record_it_changes() {
    let dir = temp("rounded");
    let base = program(&dir, "BASE.cbl", ROUNDED, "EACH-AMOUNT");
    let head = program(&dir, "HEAD.cbl", TRUNCATED, "EACH-AMOUNT");
    let (out, st) = compare(&dir, Some(&base), &head, &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(st.contains("\"verdict\":\"diverged\""));
    // 3.33 * 1.035 = 3.44655: ROUNDED gives 3.45, truncation 3.44, on the second record.
    assert!(st.contains("\"firstDifference\":{\"line\":2"), "{st}");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_declared_change_is_equivalent_as_declared() {
    let dir = temp("declared");
    let base = program(&dir, "BASE.cbl", ROUNDED, "EACH-AMOUNT");
    let head = program(&dir, "HEAD.cbl", TRUNCATED, "EACH-AMOUNT");
    fs::write(dir.join("declare.txt"), "DD OUT lines 1-3 interest is truncated from this release, as the rate notice says\n").unwrap();
    let (out, st) = compare(&dir, Some(&base), &head, &["--declare".into(), dir.join("declare.txt").display().to_string()]);
    assert_eq!(out.status.code(), Some(0));
    assert!(st.contains("\"verdict\":\"equivalent-as-declared\""));
    assert!(st.contains("interest is truncated from this release"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn expected_outputs_check_a_translation_without_a_base() {
    let dir = temp("expected");
    let head = program(&dir, "HEAD.cbl", ROUNDED, "EACH-AMOUNT");
    fs::write(dir.join("want.txt"), "0001035\n0000345\n0012777\n").unwrap();
    let (out, st) = compare(&dir, None, &head, &["--expected".into(), format!("OUT={}", dir.join("want.txt").display())]);
    assert_eq!(out.status.code(), Some(0), "{st}");
    fs::write(dir.join("want.txt"), "0001035\n0000344\n0012777\n").unwrap();
    let (out, _) = compare(&dir, None, &head, &["--expected".into(), format!("OUT={}", dir.join("want.txt").display())]);
    assert_eq!(out.status.code(), Some(1));
    fs::remove_dir_all(dir).unwrap();
}
