//! `ironwork run --evidence`: the journal records the source, the COPY member, each DD's digest and
//! the CALL, links every record to the one before, reaches the ledger, and is refused inside a
//! directory the run reads.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iw-evidence-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join("lib")).unwrap();
    fs::create_dir_all(dir.join("data")).unwrap();
    dir
}

fn write_program(dir: &Path) {
    let program = [
        "       IDENTIFICATION DIVISION.",
        "       PROGRAM-ID. EVDEMO.",
        "       ENVIRONMENT DIVISION.",
        "       INPUT-OUTPUT SECTION.",
        "       FILE-CONTROL.",
        "           SELECT IN-FILE ASSIGN TO INFILE.",
        "           SELECT OUT-FILE ASSIGN TO OUTFILE.",
        "       DATA DIVISION.",
        "       FILE SECTION.",
        "       FD IN-FILE.",
        "       COPY INREC.",
        "       FD OUT-FILE.",
        "       01 OUT-REC PIC X(10).",
        "       PROCEDURE DIVISION.",
        "           OPEN INPUT IN-FILE OUTPUT OUT-FILE.",
        "           READ IN-FILE END-READ.",
        "           MOVE IN-REC TO OUT-REC.",
        "           CALL 'HELPER'.",
        "           WRITE OUT-REC.",
        "           CLOSE IN-FILE OUT-FILE.",
        "           GOBACK.",
    ];
    fs::write(dir.join("src/EVDEMO.cbl"), program.join("\n") + "\n").unwrap();
    fs::write(dir.join("src/INREC.cpy"), "       01 IN-REC PIC X(10).\n").unwrap();
    fs::write(dir.join("lib/HELPER.cbl"), "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. HELPER.\n       PROCEDURE DIVISION.\n           GOBACK.\n").unwrap();
    fs::write(dir.join("data/in.txt"), "HELLOWORLD\n").unwrap();
}

fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\":");
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    match rest.strip_prefix('"') {
        Some(s) => s.find('"').map(|end| &s[..end]),
        None => Some(&rest[..rest.find([',', '}']).unwrap_or(rest.len())]),
    }
}

#[test]
fn a_run_with_evidence_records_what_it_read_opened_and_loaded() {
    let dir = temp("run");
    write_program(&dir);
    let status = Command::new(env!("CARGO_BIN_EXE_ironwork"))
        .arg("run")
        .arg(dir.join("src/EVDEMO.cbl"))
        .args(["-L"])
        .arg(dir.join("lib"))
        .arg("--dd")
        .arg(format!("INFILE={}:text", dir.join("data/in.txt").display()))
        .arg("--dd")
        .arg(format!("OUTFILE={}:text", dir.join("data/out.txt").display()))
        .arg("--evidence")
        .arg(dir.join("ev"))
        .status()
        .unwrap();
    assert!(status.success());
    let runs: Vec<_> = fs::read_dir(dir.join("ev/runs")).unwrap().map(|e| e.unwrap().path()).collect();
    assert_eq!(runs.len(), 1);
    let journal = fs::read_to_string(&runs[0]).unwrap();
    let lines: Vec<&str> = journal.lines().collect();
    let kinds: Vec<&str> = lines.iter().map(|l| field(l, "kind").unwrap()).collect();
    assert_eq!(kinds.first(), Some(&"open"));
    assert_eq!(kinds.last(), Some(&"close"));
    assert!(lines.iter().any(|l| field(l, "kind") == Some("input") && field(l, "path") == Some("INREC.cpy")), "the COPY member is an input");
    assert!(lines.iter().any(|l| field(l, "dd") == Some("OUTFILE") && field(l, "event") == Some("end") && field(l, "bytes") == Some("11")), "OUTFILE as the run left it");
    assert!(lines.iter().any(|l| field(l, "kind") == Some("call") && field(l, "program") == Some("HELPER")));
    assert!(!journal.contains("HELLOWORLD"), "no record holds data");
    assert!(!journal.contains(&dir.display().to_string()), "no record holds an absolute path");
    for pair in lines.windows(2) {
        assert_eq!(field(pair[1], "prev"), field(pair[0], "hash"));
    }
    let ledger = fs::read_to_string(dir.join("ev/ledger.jsonl")).unwrap();
    let run = ledger.lines().last().unwrap();
    assert_eq!(field(run, "runTip"), field(lines[lines.len() - 1], "hash"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_evidence_directory_inside_the_program_directory_is_refused() {
    let dir = temp("inside");
    write_program(&dir);
    let status = Command::new(env!("CARGO_BIN_EXE_ironwork")).arg("check").arg(dir.join("src/EVDEMO.cbl")).arg("--evidence").arg(dir.join("src/ev")).status().unwrap();
    assert_eq!(status.code(), Some(2));
    assert!(!dir.join("src/ev").exists());
    fs::remove_dir_all(dir).unwrap();
}
