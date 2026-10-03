//! `ironwork run --evidence --trace-marker`: each operation an input could steer is recorded once
//! with the marker in its operand and once without, where it is, and never with the operand itself.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

const MARKER: &str = "CWVRFY01";

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iw-trace-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    for sub in ["src", "lib", "data"] {
        fs::create_dir_all(dir.join(sub)).unwrap();
    }
    dir
}

fn write_program(dir: &Path) {
    let program = [
        "       IDENTIFICATION DIVISION.",
        "       PROGRAM-ID. TRACER.",
        "       DATA DIVISION.",
        "       WORKING-STORAGE SECTION.",
        "       01 WS-IN PIC X(8).",
        "       01 WS-PGM PIC X(8).",
        "       01 WS-CMD PIC X(20).",
        "       01 WS-N PIC 9.",
        "       PROCEDURE DIVISION.",
        "           ACCEPT WS-IN.",
        "           MOVE WS-IN TO WS-PGM.",
        "           DISPLAY 'READ ' WS-IN.",
        "           CALL WS-PGM ON EXCEPTION CONTINUE END-CALL.",
        "           STRING 'ls ' WS-IN DELIMITED BY SIZE INTO WS-CMD.",
        "           CALL 'SYSTEM' USING WS-CMD",
        "               ON EXCEPTION CONTINUE END-CALL.",
        "           PERFORM 3 TIMES",
        "               DISPLAY 'TICK'",
        "           END-PERFORM.",
        "           CALL 'HELPER'.",
        "           GOBACK.",
    ];
    fs::write(dir.join("src/TRACER.cbl"), program.join("\n") + "\n").unwrap();
    fs::write(dir.join("lib/HELPER.cbl"), "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. HELPER.\n       PROCEDURE DIVISION.\n           DISPLAY 'HELPED'.\n           GOBACK.\n").unwrap();
    fs::write(dir.join("data/sysin.txt"), format!("{MARKER}\n")).unwrap();
}

fn run(dir: &Path, marker: Option<&str>) -> ExitStatus {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ironwork"));
    command.arg("run").arg(dir.join("src/TRACER.cbl")).arg("-L").arg(dir.join("lib"));
    command.arg("--dd").arg(format!("SYSIN={}:text", dir.join("data/sysin.txt").display()));
    command.arg("--evidence").arg(dir.join("ev"));
    if let Some(m) = marker {
        command.args(["--trace-marker", m]);
    }
    command.stdout(std::process::Stdio::null()).status().unwrap()
}

fn journal(dir: &Path) -> String {
    let run = fs::read_dir(dir.join("ev/runs")).unwrap().next().unwrap().unwrap().path();
    fs::read_to_string(run).unwrap()
}

/// Each sink record as (sink, file, line, reached).
fn sinks(journal: &str) -> Vec<(String, String, u32, bool)> {
    let field = |line: &str, key: &str| -> String {
        let needle = format!("\"{key}\":");
        let rest = &line[line.find(&needle).unwrap() + needle.len()..];
        match rest.strip_prefix('"') {
            Some(s) => s[..s.find('"').unwrap()].to_string(),
            None => rest[..rest.find([',', '}']).unwrap()].to_string(),
        }
    };
    journal
        .lines()
        .filter(|l| l.contains("\"kind\":\"sink\""))
        .map(|l| {
            assert_eq!(field(l, "marker"), MARKER);
            (field(l, "sink"), field(l, "file"), field(l, "line").parse().unwrap(), field(l, "reached") == "true")
        })
        .collect()
}

#[test]
fn a_traced_run_records_whether_the_marker_reached_each_sink() {
    let dir = temp("reached");
    write_program(&dir);
    assert!(run(&dir, Some(MARKER)).success());
    let text = journal(&dir);
    let got = sinks(&text);
    let at = |sink: &str, file: &str, line: u32, reached: bool| (sink.to_string(), file.to_string(), line, reached);
    assert_eq!(
        got,
        vec![
            at("log", "TRACER.cbl", 12, true),
            at("dynamic-program-load", "TRACER.cbl", 13, true),
            at("os-command", "TRACER.cbl", 15, true),
            at("log", "TRACER.cbl", 18, false),
            at("log", "HELPER.cbl", 4, false),
        ]
    );
    assert!(!text.contains("READ CWVRFY01") && !text.contains("ls CWVRFY01"), "no record holds an operand");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_traced_cics_task_records_the_commands_its_commarea_reached() {
    let dir = temp("cics");
    let program = [
        "       IDENTIFICATION DIVISION.",
        "       PROGRAM-ID. CTRACE.",
        "       DATA DIVISION.",
        "       WORKING-STORAGE SECTION.",
        "       01 WS-PGM PIC X(8).",
        "       01 WS-Q PIC X(8) VALUE 'FIXEDQ'.",
        "       01 WS-MSG PIC X(20).",
        "       01 WS-RESP PIC S9(8) COMP.",
        "       LINKAGE SECTION.",
        "       01 DFHCOMMAREA PIC X(8).",
        "       PROCEDURE DIVISION.",
        "           MOVE DFHCOMMAREA TO WS-PGM.",
        "           EXEC CICS LINK PROGRAM(WS-PGM) RESP(WS-RESP)",
        "           END-EXEC.",
        "           MOVE DFHCOMMAREA TO WS-MSG.",
        "           EXEC CICS WRITEQ TS QUEUE(WS-Q) FROM(WS-MSG)",
        "               RESP(WS-RESP) END-EXEC.",
        "           EXEC CICS SEND TEXT FROM(WS-MSG) RESP(WS-RESP)",
        "           END-EXEC.",
        "           EXEC CICS RETURN END-EXEC.",
    ];
    fs::write(dir.join("src/CTRACE.cbl"), program.join("\n") + "\n").unwrap();
    fs::write(dir.join("data/commarea.txt"), format!("{MARKER}\n")).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_ironwork"))
        .arg("cics")
        .arg(dir.join("src/CTRACE.cbl"))
        .arg("--commarea")
        .arg(format!("{}:text", dir.join("data/commarea.txt").display()))
        .arg("--evidence")
        .arg(dir.join("ev"))
        .args(["--trace-marker", MARKER])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    let at = |sink: &str, line: u32, reached: bool| (sink.to_string(), "CTRACE.cbl".to_string(), line, reached);
    assert_eq!(sinks(&journal(&dir)), vec![at("cics-dynamic-transfer", 13, true), at("queue-name", 16, false), at("screen", 18, true)]);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_scripted_pseudo_conversation_runs_the_returned_transaction_on_the_same_terminal() {
    let dir = temp("conversation");
    let mapset = [
        "NAMEMS   DFHMSD TYPE=&SYSPARM,MODE=INOUT,LANG=COBOL,TIOAPFX=YES",
        "NAMEM    DFHMDI SIZE=(24,80),LINE=1,COLUMN=1",
        "         DFHMDF POS=(3,1),LENGTH=5,ATTRB=ASKIP,INITIAL='NAME:'",
        "NAME     DFHMDF POS=(3,7),LENGTH=12,ATTRB=(UNPROT,IC)",
        "         DFHMDF POS=(3,20),LENGTH=1,ATTRB=ASKIP",
        "         DFHMSD TYPE=FINAL",
        "         END",
    ];
    fs::write(dir.join("src/NAMEMS.bms"), mapset.join("\n") + "\n").unwrap();
    let program = [
        "       IDENTIFICATION DIVISION.",
        "       PROGRAM-ID. NAMEPGM.",
        "       DATA DIVISION.",
        "       WORKING-STORAGE SECTION.",
        "       COPY NAMEMS.",
        "       01 WS-MSG PIC X(12).",
        "       01 WS-STATE PIC X VALUE 'S'.",
        "       LINKAGE SECTION.",
        "       01 DFHCOMMAREA PIC X.",
        "       PROCEDURE DIVISION.",
        "           IF EIBCALEN = 0",
        "               MOVE LOW-VALUES TO NAMEMO",
        "               EXEC CICS SEND MAP('NAMEM') MAPSET('NAMEMS')",
        "               ERASE END-EXEC",
        "               EXEC CICS RETURN TRANSID('NAME')",
        "               COMMAREA(WS-STATE) END-EXEC",
        "           END-IF",
        "           EXEC CICS RECEIVE MAP('NAMEM') MAPSET('NAMEMS')",
        "               INTO(NAMEMI) END-EXEC",
        "           MOVE NAMEI TO WS-MSG",
        "           EXEC CICS WRITEQ TD QUEUE('LOGQ') FROM(WS-MSG)",
        "           END-EXEC",
        "           EXEC CICS RETURN END-EXEC.",
    ];
    fs::write(dir.join("src/NAMEPGM.cbl"), program.join("\n") + "\n").unwrap();
    fs::write(dir.join("data/screens"), format!("type 3 8 {MARKER}\nENTER\n")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ironwork"))
        .arg("cics")
        .arg(dir.join("src/NAMEPGM.cbl"))
        .args(["--transid", "NAME", "--screens"])
        .arg(dir.join("data/screens"))
        .arg("--td")
        .arg(format!("LOGQ={}", dir.join("data/logq").display()))
        .arg("--evidence")
        .arg(dir.join("ev"))
        .args(["--trace-marker", MARKER])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stderr).contains("task 2: NAME runs NAMEPGM"));
    assert_eq!(fs::read_to_string(dir.join("data/logq")).unwrap().trim_end(), MARKER);
    assert_eq!(fs::read_dir(dir.join("ev/runs")).unwrap().count(), 1, "one journal for the conversation");
    assert_eq!(sinks(&journal(&dir)), vec![("log".to_string(), "NAMEPGM.cbl".to_string(), 21, true)]);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_untraced_run_records_no_sinks() {
    let dir = temp("untraced");
    write_program(&dir);
    assert!(run(&dir, None).success());
    assert!(sinks(&journal(&dir)).is_empty());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_marker_without_a_journal_is_refused() {
    let dir = temp("nojournal");
    write_program(&dir);
    let status = Command::new(env!("CARGO_BIN_EXE_ironwork")).arg("run").arg(dir.join("src/TRACER.cbl")).args(["--trace-marker", MARKER]).status().unwrap();
    assert_eq!(status.code(), Some(246));
    fs::remove_dir_all(dir).unwrap();
}
