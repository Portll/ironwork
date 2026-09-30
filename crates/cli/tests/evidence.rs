//! `ironwork run --evidence`: the journal records the source, the COPY member, each DD's digest and
//! the CALL and where an abend was, links every record to the one before, reaches the ledger, and is
//! refused inside a directory the run reads.

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
fn check_provenance_names_every_copy_member_and_the_options_in_force() {
    let dir = temp("provenance");
    write_program(&dir);
    let source = fs::read_to_string(dir.join("src/EVDEMO.cbl")).unwrap();
    fs::write(dir.join("src/EVDEMO.cbl"), format!("       CBL TRUNC(BIN)\n{source}")).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_ironwork"))
        .arg("check")
        .arg(dir.join("src/EVDEMO.cbl"))
        .arg("--provenance")
        .arg(dir.join("prov.json"))
        .arg("--evidence")
        .arg(dir.join("ev"))
        .status()
        .unwrap();
    assert!(status.success());
    let text = fs::read_to_string(dir.join("prov.json")).unwrap();
    assert!(text.starts_with("{\"_type\":\"https://in-toto.io/Statement/v1\""));
    assert!(text.contains("\"predicateType\":\"https://slsa.dev/provenance/v1\""));
    assert!(text.contains("\"uri\":\"file:INREC.cpy\""), "the COPY member is a resolved dependency");
    assert!(text.contains("\"optionCards\":[\"TRUNC(BIN)\"]"));
    assert!(text.contains("\"trunc\":\"Bin\""), "the option in force");
    assert!(text.contains("\"name\":\"EVDEMO.cbl\""));
    assert!(!text.contains(&dir.display().to_string()), "no absolute path");
    let run = fs::read_dir(dir.join("ev/runs")).unwrap().next().unwrap().unwrap().path();
    let journal = fs::read_to_string(run).unwrap();
    assert!(journal.lines().any(|l| field(l, "kind") == Some("output") && field(l, "name") == Some("provenance")), "the journal records the statement by digest");
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

#[test]
fn an_abend_in_a_method_names_the_class_source_on_stderr_and_in_the_journal() {
    let dir = temp("method");
    let lines = |l: &[&str]| l.iter().map(|l| format!("       {l}\n")).collect::<String>();
    let class = lines(&[
        "CBL THREAD,DLL",
        "IDENTIFICATION DIVISION.",
        "CLASS-ID. Divider INHERITS Base.",
        "ENVIRONMENT DIVISION.",
        "CONFIGURATION SECTION.",
        "REPOSITORY.",
        "    CLASS Base IS \"java.lang.Object\"",
        "    CLASS Divider IS \"Divider\".",
        "IDENTIFICATION DIVISION.",
        "OBJECT.",
        "PROCEDURE DIVISION.",
        "IDENTIFICATION DIVISION.",
        "METHOD-ID. \"divide\".",
        "DATA DIVISION.",
        "WORKING-STORAGE SECTION.",
        "01  D PIC 9 VALUE 0.",
        "01  Q PIC 9.",
        "PROCEDURE DIVISION.",
        "    DIVIDE 10 BY D GIVING Q.",
        "END METHOD \"divide\".",
        "END OBJECT.",
        "END CLASS Divider.",
    ]);
    let client = lines(&[
        "CBL THREAD,DLL",
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. CLIENT RECURSIVE.",
        "ENVIRONMENT DIVISION.",
        "CONFIGURATION SECTION.",
        "REPOSITORY.",
        "    CLASS Divider IS \"Divider\".",
        "DATA DIVISION.",
        "WORKING-STORAGE SECTION.",
        "01  T USAGE OBJECT REFERENCE Divider.",
        "PROCEDURE DIVISION.",
        "    INVOKE Divider NEW RETURNING T",
        "    INVOKE T \"divide\"",
        "    GOBACK.",
    ]);
    fs::write(dir.join("lib/Divider.cbl"), class).unwrap();
    fs::write(dir.join("src/CLIENT.cbl"), client).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_ironwork")).arg("run").arg(dir.join("src/CLIENT.cbl")).arg("-L").arg(dir.join("lib")).arg("--evidence").arg(dir.join("ev")).output().unwrap();
    assert_eq!(out.status.code(), Some(16));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.starts_with(&format!("{}:19:", dir.join("lib/Divider.cbl").display())) && stderr.contains("ABEND S0CB"), "{stderr}");
    let run = fs::read_dir(dir.join("ev/runs")).unwrap().next().unwrap().unwrap().path();
    let journal = fs::read_to_string(run).unwrap();
    let abend = journal.lines().find(|l| field(l, "kind") == Some("abend")).unwrap();
    assert_eq!((field(abend, "file"), field(abend, "line")), (Some("Divider.cbl"), Some("19")), "{abend}");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_abend_names_the_file_and_line_it_happened_at_in_the_program_or_a_library_program() {
    let dir = temp("abend-file");
    let failing = |id: &str| {
        [
            "       IDENTIFICATION DIVISION.".to_string(),
            format!("       PROGRAM-ID. {id}."),
            "       DATA DIVISION.".into(),
            "       WORKING-STORAGE SECTION.".into(),
            "       01 WS-A PIC X(3) VALUE '***'.".into(),
            "       01 WS-N REDEFINES WS-A PIC 9(3).".into(),
            "       01 WS-T PIC 9(3) VALUE 0.".into(),
            "       PROCEDURE DIVISION.".into(),
            "           ADD WS-N TO WS-T.".into(),
            "           GOBACK.".into(),
        ]
        .join("\n")
            + "\n"
    };
    fs::write(dir.join("src/SELFAB.cbl"), failing("SELFAB")).unwrap();
    fs::write(dir.join("lib/HELPAB.cbl"), failing("HELPAB")).unwrap();
    fs::write(dir.join("src/CALLAB.cbl"), "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. CALLAB.\n       PROCEDURE DIVISION.\n           CALL 'HELPAB'.\n           GOBACK.\n").unwrap();
    for (program, file) in [("SELFAB", "SELFAB.cbl"), ("CALLAB", "HELPAB.cbl")] {
        let ev = dir.join(format!("ev-{program}"));
        let status = Command::new(env!("CARGO_BIN_EXE_ironwork"))
            .arg("run")
            .arg(dir.join(format!("src/{program}.cbl")))
            .arg("-L")
            .arg(dir.join("lib"))
            .arg("--evidence")
            .arg(&ev)
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(16));
        let run = fs::read_dir(ev.join("runs")).unwrap().next().unwrap().unwrap().path();
        let journal = fs::read_to_string(run).unwrap();
        let abend = journal.lines().find(|l| field(l, "kind") == Some("abend")).unwrap();
        assert_eq!((field(abend, "code"), field(abend, "file"), field(abend, "line")), (Some("S0C7"), Some(file), Some("9")), "{program}");
    }
    fs::remove_dir_all(dir).unwrap();
}
