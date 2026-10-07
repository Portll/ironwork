//! `ironwork fuzz --differential`: a program that sums its input file, a CICS task that sums its
//! COMMAREA and a subprogram that adds to its argument run alike on the interpreter and the VM;
//! inputs that loop both reach the statement limit and pass; a run that reaches what the VM does not
//! run yet is counted, not failed.

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
fn differential_takes_no_job_and_vm_is_not_a_fuzz_flag() {
    let dir = temp("usage", &[]);
    let o = fuzz(&dir, &["--job"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(text(&o.stderr).contains("--differential goes with a program, --cics or --interface, not --job"), "{}", text(&o.stderr));
    let o = Command::new(env!("CARGO_BIN_EXE_ironwork")).current_dir(&dir).args(["fuzz", "--vm", "QTYSUM.cbl", "-o", "run"]).output().unwrap();
    assert_eq!(o.status.code(), Some(2));
}

const TASK: &[&str] = &[
    "       IDENTIFICATION DIVISION.",
    "       PROGRAM-ID. CICSQTY.",
    "       DATA DIVISION.",
    "       WORKING-STORAGE SECTION.",
    "       01 WS-TOTAL PIC 9(7) VALUE 0.",
    "       01 WS-COMM.",
    "          05 WS-STATE PIC X VALUE 'S'.",
    "          05 WS-COUNT PIC 9(3) VALUE 0.",
    "       LINKAGE SECTION.",
    "       01 DFHCOMMAREA.",
    "          05 CA-STATE PIC X.",
    "          05 CA-COUNT PIC 9(3).",
    "       PROCEDURE DIVISION.",
    "           IF EIBCALEN = 0",
    "              EXEC CICS RETURN END-EXEC",
    "           END-IF",
    "           ADD CA-COUNT TO WS-TOTAL",
    "           MOVE CA-COUNT TO WS-COUNT",
    "           EXEC CICS WRITEQ TS QUEUE('QTYQ') FROM(WS-COMM) END-EXEC",
    "           DISPLAY 'TOTAL ' WS-TOTAL",
    "           EXEC CICS RETURN TRANSID('QTY1') COMMAREA(WS-COMM) END-EXEC.",
];

const SUBPROGRAM: &[&str] = &[
    "       IDENTIFICATION DIVISION.",
    "       PROGRAM-ID. ADDONE.",
    "       DATA DIVISION.",
    "       LINKAGE SECTION.",
    "       01  QTY-REC.",
    "           05 QTY PIC 9(5).",
    "           05 QTY-NOTE PIC X(5).",
    "       PROCEDURE DIVISION USING QTY-REC.",
    "           ADD 1 TO QTY",
    "           MOVE 'DONE' TO QTY-NOTE",
    "           IF QTY > 50000",
    "              MOVE 8 TO RETURN-CODE",
    "           END-IF",
    "           GOBACK.",
];

/// A directory holding one program, `lines` with each edit made.
fn program(name: &str, file: &str, lines: &[&str], edits: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iw-fuzz-differential-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let text = edits.iter().fold(lines.join("\n"), |t, (from, to)| t.replace(from, to));
    fs::write(dir.join(file), text + "\n").unwrap();
    dir
}

fn ironwork(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ironwork")).current_dir(dir).args(args).output().unwrap()
}

/// The number just before `label` in a summary line.
fn count_before(out: &str, label: &str) -> u32 {
    out.split(label).next().and_then(|s| s.rsplit([' ', '(']).next()).and_then(|n| n.parse().ok()).unwrap_or_else(|| panic!("no count before {label:?} in {out}"))
}

#[test]
fn a_cics_task_runs_alike_on_both_executors_its_queues_and_returned_commarea_compared() {
    let dir = program("cics-alike", "CICSQTY.cbl", TASK, &[]);
    let o = ironwork(&dir, &["fuzz", "--cics", "--differential", "CICSQTY.cbl", "-o", "run", "--runs", "30"]);
    assert_eq!(o.status.code(), Some(0), "{}{}", text(&o.stdout), text(&o.stderr));
    assert!(text(&o.stdout).contains("ironwork fuzz --differential: 30 runs, 30 agree (0 at the statement limit), 0 timed out, 0 stopped by the VM, 0 differ (0 kept)"), "{}", text(&o.stdout));
    assert_eq!(fs::read_dir(dir.join("run")).unwrap().count(), 0, "no divergence is kept and the work directory is gone");
    // What each run's task record holds: RETURN's TRANSID and COMMAREA and the TS queue it wrote.
    fs::write(dir.join("commarea"), [0xE2, 0xF0, 0xF4, 0xF2]).unwrap();
    for executor in ["--interpret", "--vm"] {
        let o = ironwork(&dir, &["cics", "CICSQTY.cbl", "--commarea", "commarea", "--task-out", "tasks.jsonl", "--statement-limit", "1000", executor]);
        assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
        assert_eq!(
            fs::read_to_string(dir.join("tasks.jsonl")).unwrap(),
            "{\"returnCommarea\":\"E2F0F4F2\",\"returnTransid\":\"QTY1\",\"task\":1,\"td\":{},\"transid\":\"TRAN\",\"ts\":{\"QTYQ\":[\"E2F0F4F2\"]}}\n",
            "{executor}"
        );
    }
}

#[test]
fn a_cics_task_that_loops_stops_at_the_same_statement_on_both_and_is_compared() {
    let dir = program("cics-loop", "CICSQTY.cbl", TASK, &[("           ADD CA-COUNT TO WS-TOTAL", "           PERFORM UNTIL CA-COUNT < 500\n              ADD 1 TO WS-TOTAL\n           END-PERFORM")]);
    let o = ironwork(&dir, &["fuzz", "--cics", "--differential", "CICSQTY.cbl", "-o", "run", "--runs", "30", "--hang-limit", "5000"]);
    assert_eq!(o.status.code(), Some(0), "{}{}", text(&o.stdout), text(&o.stderr));
    let out = text(&o.stdout);
    assert!(count_before(&out, " at the statement limit)") > 0, "{out}");
    assert!(out.contains(" 0 differ "), "{out}");
}

#[test]
fn a_cics_task_that_reaches_what_the_vm_does_not_run_is_counted_and_passes() {
    let dir = program("cics-unimplemented", "CICSQTY.cbl", TASK, &[("           IF EIBCALEN = 0", "           DISPLAY FUNCTION UUID4\n           IF EIBCALEN = 0")]);
    let o = ironwork(&dir, &["fuzz", "--cics", "--differential", "CICSQTY.cbl", "-o", "run", "--runs", "10"]);
    assert_eq!(o.status.code(), Some(0), "{}{}", text(&o.stdout), text(&o.stderr));
    assert!(text(&o.stdout).contains("0 agree (0 at the statement limit), 0 timed out, 10 stopped by the VM"), "{}", text(&o.stdout));
    assert!(text(&o.stderr).contains("10 runs reached what the VM does not run yet: FUNCTION UUID4,"), "{}", text(&o.stderr));
}

#[test]
fn a_subprogram_runs_alike_on_both_executors_its_argument_compared_as_the_caller_sees_it() {
    let dir = program("interface-alike", "ADDONE.cbl", SUBPROGRAM, &[]);
    let o = ironwork(&dir, &["fuzz", "--interface", "--differential", "ADDONE.cbl", "-o", "run", "--runs", "30"]);
    assert_eq!(o.status.code(), Some(0), "{}{}", text(&o.stdout), text(&o.stderr));
    assert!(text(&o.stdout).contains("ironwork fuzz --differential: 30 runs, 30 agree (0 at the statement limit), 0 timed out, 0 stopped by the VM, 0 differ (0 kept)"), "{}", text(&o.stdout));
    assert_eq!(fs::read_dir(dir.join("run")).unwrap().count(), 0, "no divergence is kept and the work directory is gone");
    fs::write(dir.join("qty"), [0xF0, 0xF0, 0xF0, 0xF4, 0xF1, 0x40, 0x40, 0x40, 0x40, 0x40]).unwrap();
    for executor in ["--interpret", "--vm"] {
        let o = ironwork(&dir, &["run", "ADDONE.cbl", "--argument", "qty", "--arguments-out", "returned", executor]);
        assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
        assert_eq!(fs::read(dir.join("returned/arg1")).unwrap(), [0xF0, 0xF0, 0xF0, 0xF4, 0xF2, 0xC4, 0xD6, 0xD5, 0xC5, 0x40], "{executor}");
    }
    let o = ironwork(&dir, &["run", "ADDONE.cbl", "--arguments-out", "returned"]);
    assert!(o.status.code() != Some(0) && text(&o.stderr).contains("--arguments-out goes with --argument"), "{}", text(&o.stderr));
}

#[test]
fn a_subprogram_that_loops_stops_at_the_same_statement_on_both_and_is_compared() {
    let dir = program("interface-loop", "ADDONE.cbl", SUBPROGRAM, &[("           ADD 1 TO QTY", "           PERFORM UNTIL QTY < 50000\n              ADD 1 TO RETURN-CODE\n           END-PERFORM")]);
    let o = ironwork(&dir, &["fuzz", "--interface", "--differential", "ADDONE.cbl", "-o", "run", "--runs", "30", "--hang-limit", "5000"]);
    assert_eq!(o.status.code(), Some(0), "{}{}", text(&o.stdout), text(&o.stderr));
    let out = text(&o.stdout);
    assert!(count_before(&out, " at the statement limit)") > 0, "{out}");
    assert!(out.contains(" 0 differ "), "{out}");
}

#[test]
fn a_subprogram_that_reaches_what_the_vm_does_not_run_is_counted_and_passes() {
    let dir = program("interface-unimplemented", "ADDONE.cbl", SUBPROGRAM, &[("           ADD 1 TO QTY", "           DISPLAY FUNCTION UUID4\n           ADD 1 TO QTY")]);
    let o = ironwork(&dir, &["fuzz", "--interface", "--differential", "ADDONE.cbl", "-o", "run", "--runs", "10"]);
    assert_eq!(o.status.code(), Some(0), "{}{}", text(&o.stdout), text(&o.stderr));
    assert!(text(&o.stdout).contains("0 agree (0 at the statement limit), 0 timed out, 10 stopped by the VM"), "{}", text(&o.stdout));
}
