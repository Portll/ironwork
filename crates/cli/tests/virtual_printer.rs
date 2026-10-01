//! The virtual printer from the command line: `ironwork run` prints an lp command's file on DD
//! PRINTER and journals both DDs, and `ironwork compare` finds that a change which drops the print
//! diverges while one that checks the printer's name first does not.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iw-vprinter-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Writes a payroll report to report.txt, reads the printer's name from DD PRINTERS, and prints the
/// report there; `print` is the statements that send it, and the program then says it did.
fn program(dir: &Path, name: &str, print: &[&str]) -> PathBuf {
    let head = [
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. PAYRPT.",
        "ENVIRONMENT DIVISION.",
        "INPUT-OUTPUT SECTION.",
        "FILE-CONTROL.",
        "    SELECT RPT ASSIGN TO \"report.txt\".",
        "    SELECT PRINTER-LIST ASSIGN TO PRINTERS.",
        "DATA DIVISION.",
        "FILE SECTION.",
        "FD  RPT.",
        "01  RPT-REC PIC X(12).",
        "FD  PRINTER-LIST.",
        "01  PRINTER-REC PIC X(40).",
        "WORKING-STORAGE SECTION.",
        "01  SELECTED-PRINTER PIC X(40).",
        "01  PRINT-COMMAND PIC X(150).",
        "01  SYSTEM-STATUS PIC S9(9) COMP-5.",
        "PROCEDURE DIVISION.",
        "    OPEN OUTPUT RPT.",
        "    WRITE RPT-REC FROM 'PAYROLL 2026'.",
        "    CLOSE RPT.",
        "    OPEN INPUT PRINTER-LIST.",
        "    READ PRINTER-LIST INTO SELECTED-PRINTER.",
        "    CLOSE PRINTER-LIST.",
        "    STRING 'lp -d ' DELIMITED BY SIZE",
        "        SELECTED-PRINTER DELIMITED BY SPACE",
        "        ' report.txt' DELIMITED BY SIZE",
        "        INTO PRINT-COMMAND.",
    ];
    let tail = ["    IF SYSTEM-STATUS = 0", "        DISPLAY 'REPORT SENT TO PRINTER.'", "    END-IF.", "    GOBACK."];
    let source: String = head.iter().chain(print).chain(&tail).map(|l| format!("       {l}\n")).collect();
    let path = dir.join(name);
    fs::write(&path, source).unwrap();
    path
}

const CALL: &[&str] = &["    CALL 'SYSTEM' USING PRINT-COMMAND", "        RETURNING SYSTEM-STATUS."];

fn dds(dir: &Path) -> Vec<String> {
    fs::write(dir.join("printers.txt"), "office\n").unwrap();
    [
        format!("PRINTERS={}:text", dir.join("printers.txt").display()),
        format!("REPORT.TXT={}:text", dir.join("report.txt").display()),
        format!("PRINTER={}", dir.join("printer.prn").display()),
    ]
    .into_iter()
    .flat_map(|d| ["--dd".to_string(), d])
    .collect()
}

#[test]
fn run_prints_the_report_on_dd_printer_and_journals_it() {
    let dir = temp("run");
    fs::create_dir(dir.join("src")).unwrap();
    let source = program(&dir.join("src"), "PAYRPT.cbl", CALL);
    let out = Command::new(env!("CARGO_BIN_EXE_ironwork")).arg("run").arg(&source).args(dds(&dir)).arg("--evidence").arg(dir.join("ev")).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "REPORT SENT TO PRINTER.\n");
    assert_eq!(fs::read_to_string(dir.join("printer.prn")).unwrap(), "PAYROLL 2026\n");
    let run = fs::read_dir(dir.join("ev/runs")).unwrap().next().unwrap().unwrap().path();
    let journal = fs::read_to_string(run).unwrap();
    for dd in ["REPORT.TXT", "PRINTER"] {
        assert!(journal.contains(&format!("\"dd\":\"{dd}\"")), "DD {dd} is journalled: {journal}");
    }
    fs::remove_dir_all(dir).unwrap();
}

fn compare(dir: &Path, base: &Path, head: &Path) -> (Output, String) {
    let statement = dir.join("statement.json");
    let out = Command::new(env!("CARGO_BIN_EXE_ironwork")).arg("compare").arg("--base").arg(base).arg("--head").arg(head).args(dds(dir)).arg("--statement").arg(&statement).output().unwrap();
    (out, fs::read_to_string(statement).unwrap_or_default())
}

#[test]
fn a_change_that_drops_the_print_diverges_on_dd_printer() {
    let dir = temp("dropped");
    let base = program(&dir, "BASE.cbl", CALL);
    let head = program(&dir, "HEAD.cbl", &[]);
    let (out, st) = compare(&dir, &base, &head);
    assert_eq!(out.status.code(), Some(1), "{}\n{st}", String::from_utf8_lossy(&out.stderr));
    assert!(st.contains("\"verdict\":\"diverged\"") && st.contains("DD PRINTER"), "{st}");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_check_on_the_printer_name_that_keeps_the_print_is_equivalent() {
    let dir = temp("checked");
    let base = program(&dir, "BASE.cbl", CALL);
    let checked = [
        "    IF SELECTED-PRINTER IS ALPHABETIC",
        "        CALL 'SYSTEM' USING PRINT-COMMAND",
        "            RETURNING SYSTEM-STATUS",
        "    ELSE",
        "        MOVE 1 TO SYSTEM-STATUS",
        "    END-IF.",
    ];
    let head = program(&dir, "HEAD.cbl", &checked);
    let (out, st) = compare(&dir, &base, &head);
    assert_eq!(out.status.code(), Some(0), "{}\n{st}", String::from_utf8_lossy(&out.stderr));
    assert!(st.contains("\"verdict\":\"equivalent\""), "{st}");
    fs::remove_dir_all(dir).unwrap();
}
