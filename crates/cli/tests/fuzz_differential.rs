//! `ironwork fuzz --differential`: a program that sums its input file runs alike on the interpreter
//! and the VM; inputs that loop both reach the statement limit and pass; a run that reaches what the
//! VM does not run yet is counted, not failed.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const PROGRAM: &[&str] = &[
    "       CBL SSRANGE",
    "       IDENTIFICATION DIVISION.",
    "       PROGRAM-ID. QTYSUM.",
    "       ENVIRONMENT DIVISION.",
    "       INPUT-OUTPUT SECTION.",
    "       FILE-CONTROL.",
    "           SELECT IN-FILE ASSIGN TO INFILE.",
    "           SELECT OUT-FILE ASSIGN TO OUTFILE.",
    "       DATA DIVISION.",
    "       FILE SECTION.",
    "       FD IN-FILE.",
    "       01 IN-REC.",
    "          05 IN-NAME PIC X(10).",
    "          05 IN-QTY  PIC 9(5).",
    "          05 IN-IDX  PIC 9(2).",
    "       FD OUT-FILE.",
    "       01 OUT-REC PIC X(17).",
    "       WORKING-STORAGE SECTION.",
    "       01 WS-TOTAL PIC 9(9) VALUE 0.",
    "       01 WS-EOF PIC X VALUE 'N'.",
    "       01 WS-TABLE.",
    "          05 WS-SLOT PIC X(3) OCCURS 10 TIMES.",
    "       PROCEDURE DIVISION.",
    "           OPEN INPUT IN-FILE OUTPUT OUT-FILE",
    "           PERFORM UNTIL WS-EOF = 'Y'",
    "              READ IN-FILE AT END MOVE 'Y' TO WS-EOF",
    "              NOT AT END",
    "                 WRITE OUT-REC FROM IN-REC",
    "                 ADD IN-QTY TO WS-TOTAL",
    "                 MOVE 'ABC' TO WS-SLOT (IN-IDX)",
    "              END-READ",
    "           END-PERFORM",
    "           DISPLAY 'TOTAL ' WS-TOTAL",
    "           CLOSE IN-FILE OUT-FILE",
    "           GOBACK.",
];

fn temp(name: &str, edits: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iw-fuzz-differential-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let text = edits.iter().fold(PROGRAM.join("\n"), |t, (from, to)| t.replace(from, to));
    fs::write(dir.join("QTYSUM.cbl"), text + "\n").unwrap();
    dir
}

fn fuzz(dir: &Path, extra: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ironwork")).current_dir(dir).args(["fuzz", "--differential", "QTYSUM.cbl", "-o", "run"]).args(extra).output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn generated_records_abend_and_run_alike_on_both_executors() {
    let dir = temp("alike", &[]);
    let o = fuzz(&dir, &["--runs", "30"]);
    assert_eq!(o.status.code(), Some(0), "{}{}", text(&o.stdout), text(&o.stderr));
    assert!(text(&o.stdout).contains("30 runs, 30 agree (0 at the statement limit), 0 timed out, 0 stopped by the VM, 0 differ (0 kept)"), "{}", text(&o.stdout));
    assert_eq!(fs::read_dir(dir.join("run")).unwrap().count(), 0, "no divergence is kept and the work directory is gone");
}

#[test]
fn inputs_that_loop_stop_at_the_same_statement_on_both_and_are_compared() {
    let dir = temp("loop", &[("                 ADD IN-QTY TO WS-TOTAL", "                 PERFORM UNTIL IN-QTY < 50000\n                    ADD 1 TO WS-TOTAL\n                 END-PERFORM")]);
    let o = fuzz(&dir, &["--runs", "30", "--hang-limit", "5000"]);
    assert_eq!(o.status.code(), Some(0), "{}{}", text(&o.stdout), text(&o.stderr));
    let out = text(&o.stdout);
    let limited: u32 = out.split(" at the statement limit)").next().and_then(|s| s.rsplit(" agree (").next()).and_then(|n| n.parse().ok()).unwrap();
    assert!(limited > 0, "{out}");
    assert!(out.contains(" 0 differ "), "{out}");
}

#[test]
fn a_run_that_reaches_what_the_vm_does_not_run_is_counted_and_passes() {
    let dir = temp("unimplemented", &[("           OPEN INPUT", "           DISPLAY FUNCTION UUID4\n           OPEN INPUT")]);
    let o = fuzz(&dir, &["--runs", "10"]);
    assert_eq!(o.status.code(), Some(0), "{}{}", text(&o.stdout), text(&o.stderr));
    assert!(text(&o.stdout).contains("0 agree (0 at the statement limit), 0 timed out, 10 stopped by the VM"), "{}", text(&o.stdout));
    assert!(text(&o.stderr).contains("10 runs reached what the VM does not run yet: FUNCTION UUID4,"), "{}", text(&o.stderr));
}

#[test]
fn differential_takes_no_job_or_cics_and_vm_is_not_a_fuzz_flag() {
    let dir = temp("usage", &[]);
    let o = fuzz(&dir, &["--job"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(text(&o.stderr).contains("fuzz takes one of --job, --cics, --interface and --differential"), "{}", text(&o.stderr));
    let o = Command::new(env!("CARGO_BIN_EXE_ironwork")).current_dir(&dir).args(["fuzz", "--vm", "QTYSUM.cbl", "-o", "run"]).output().unwrap();
    assert_eq!(o.status.code(), Some(2));
}
