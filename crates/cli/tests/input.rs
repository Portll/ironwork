//! `ironwork run --evidence --trace-input`: each sink record says whether an input byte may be in
//! its operand, true or false, or null after an operation taint does not follow.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn temp(name: &str, procedure: &[&str]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iw-input-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    let head = [
        "       IDENTIFICATION DIVISION.",
        "       PROGRAM-ID. TAINTED.",
        "       DATA DIVISION.",
        "       WORKING-STORAGE SECTION.",
        "       01 WS-IN PIC X(8).",
        "       01 WS-OUT PIC X(8).",
        "       01 WS-DOC PIC X(40).",
        "       01 WS-REC.",
        "          05 WS-F PIC X(4) VALUE 'ONE'.",
        "       PROCEDURE DIVISION.",
    ];
    fs::write(dir.join("src/TAINTED.cbl"), [&head[..], procedure].concat().join("\n") + "\n").unwrap();
    fs::write(dir.join("sysin.txt"), "ATTACK\n").unwrap();
    dir
}

fn run(dir: &Path, flags: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ironwork"));
    command.arg("run").arg(dir.join("src/TAINTED.cbl"));
    command.arg("--dd").arg(format!("SYSIN={}:text", dir.join("sysin.txt").display()));
    command.arg("--evidence").arg(dir.join("ev")).args(flags);
    command.output().unwrap()
}

/// Each sink record as (line, input as written, whether it carries a marker).
fn sinks(dir: &Path) -> Vec<(u32, String, bool)> {
    let run = fs::read_dir(dir.join("ev/runs")).unwrap().next().unwrap().unwrap().path();
    let field = |line: &str, key: &str| -> Option<String> {
        let needle = format!("\"{key}\":");
        let rest = &line[line.find(&needle)? + needle.len()..];
        Some(rest[..rest.find([',', '}']).unwrap()].trim_matches('"').to_string())
    };
    fs::read_to_string(run)
        .unwrap()
        .lines()
        .filter(|l| l.contains("\"kind\":\"sink\""))
        .map(|l| (field(l, "line").unwrap().parse().unwrap(), field(l, "input").unwrap_or_default(), field(l, "marker").is_some()))
        .collect()
}

#[test]
fn a_sink_records_whether_input_may_be_in_its_operand_without_a_marker() {
    let dir = temp("moves", &["           ACCEPT WS-IN.", "           MOVE WS-IN TO WS-OUT.", "           DISPLAY WS-OUT.", "           MOVE 'SAFE' TO WS-OUT.", "           DISPLAY WS-OUT.", "           GOBACK."]);
    let out = run(&dir, &["--trace-input"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let recorded = sinks(&dir);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(recorded, [(13, "true".into(), false), (15, "false".into(), false)]);
}

#[test]
fn with_a_marker_too_each_record_carries_both_and_an_unfollowed_operation_leaves_input_null() {
    let dir = temp("unfollowed", &["           DISPLAY WS-REC.", "           JSON GENERATE WS-DOC FROM WS-REC.", "           DISPLAY WS-REC.", "           GOBACK."]);
    let out = run(&dir, &["--trace-input", "--trace-marker", "CWVRFY01"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let recorded = sinks(&dir);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(recorded, [(11, "false".into(), true), (13, "null".into(), true)]);
}

#[test]
fn input_tracing_without_a_journal_is_refused() {
    let dir = temp("refused", &["           GOBACK."]);
    let mut command = Command::new(env!("CARGO_BIN_EXE_ironwork"));
    let out = command.arg("run").arg(dir.join("src/TAINTED.cbl")).arg("--trace-input").output().unwrap();
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--trace-input goes with --evidence, for run"));
}

#[test]
fn a_cics_task_s_commarea_is_input_and_its_statements_are_traced() {
    let dir = std::env::temp_dir().join(format!("iw-input-cli-cics-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    let program = [
        "       IDENTIFICATION DIVISION.",
        "       PROGRAM-ID. TAINTC.",
        "       DATA DIVISION.",
        "       WORKING-STORAGE SECTION.",
        "       01 WS-T PIC X(8).",
        "       01 WS-K PIC X(8) VALUE 'CONSTANT'.",
        "       LINKAGE SECTION.",
        "       01 DFHCOMMAREA PIC X(8).",
        "       PROCEDURE DIVISION.",
        "           MOVE DFHCOMMAREA TO WS-T",
        "           EXEC CICS WRITEQ TD QUEUE('CSMT') FROM(WS-T) END-EXEC",
        "           EXEC CICS WRITEQ TD QUEUE('CSMT') FROM(WS-K) END-EXEC",
        "           EXEC CICS RETURN END-EXEC.",
    ];
    fs::write(dir.join("src/TAINTC.cbl"), program.join("\n") + "\n").unwrap();
    fs::write(dir.join("commarea.txt"), "ATTACK  ").unwrap();
    fs::write(dir.join("statements.txt"), "TAINTC.cbl:10\nTAINTC.cbl:12\n").unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_ironwork"));
    command.arg("cics").arg(dir.join("src/TAINTC.cbl")).arg("--commarea").arg(format!("{}:text", dir.join("commarea.txt").display()));
    command.arg("--evidence").arg(dir.join("ev")).arg("--trace-input").arg("--trace-statements").arg(dir.join("statements.txt"));
    let out = command.output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let recorded = sinks(&dir);
    let journal = fs::read_to_string(fs::read_dir(dir.join("ev/runs")).unwrap().next().unwrap().unwrap().path()).unwrap();
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(recorded, [(11, "true".into(), false), (12, "false".into(), false)]);
    let starts: Vec<&str> = journal.lines().filter(|l| l.contains("\"kind\":\"statement\"")).collect();
    assert_eq!(starts.len(), 2, "{starts:?}");
}
