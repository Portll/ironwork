//! `--compliance loose`: a data record, file or report extended refuses is left out and a statement
//! naming one of its items compiles as a hole, as does a statement a later check refuses; a
//! directive ironwork does not read and a character outside COBOL's set are passed over; messages
//! of severity E are warnings.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

fn ironwork(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ironwork")).args(args).output().unwrap()
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iw-loose-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A record with a TYPEDEF item, one with a PICTURE of 40 digits, an unread directive, a stray `$`
/// and a period missing before the PROCEDURE DIVISION; `n` decides whether the run reaches a hole.
fn program(dir: &std::path::Path, n: u8) -> String {
    let text = format!(
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. LOOSE.\n       >>CALL-CONVENTION COBOL\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01 GOOD PIC X(3) VALUE 'ok'.\n       01 T-REC.\n          05 T-A PIC X TYPEDEF.\n          05 T-B PIC 9.\n       01 WIDE PIC 9(40).\n       01 N PIC 9 VALUE {n}\n       PROCEDURE DIVISION.\n           DISPLAY GOOD ' ' N $\n           IF N = 0\n               MOVE 1 TO T-B\n           END-IF\n           DISPLAY 'END'\n           GOBACK.\n"
    );
    let path = dir.join(format!("LOOSE{n}.cbl"));
    fs::write(&path, text).unwrap();
    path.to_str().unwrap().to_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn records_lines_and_characters_are_left_out_and_check_warns_of_each() {
    let dir = temp("check");
    let path = program(&dir, 7);
    let o = ironwork(&["check", &path, "--compliance", "loose"]);
    assert_eq!(o.status.code(), Some(4), "{}", stderr(&o));
    let said = stderr(&o);
    let ids: Vec<&str> = said.lines().filter_map(|l| l.split("warning: ").nth(1)).map(|w| &w[..7]).filter(|id| matches!(*id, "IWX0064" | "IWX0065" | "IWX0059" | "IWX0093")).collect();
    assert_eq!(ids.iter().filter(|i| **i == "IWX0064").count(), 2, "{said}");
    assert_eq!(ids.iter().filter(|i| **i == "IWX0065").count(), 2, "{said}");
    assert!(ids.contains(&"IWX0093") && ids.contains(&"IWX0059"), "{said}");
    assert!(said.contains("the record T-REC (--compliance loose): IWR0003-S TYPEDEF"), "{said}");
    assert_eq!(ironwork(&["check", &path, "--compliance", "relaxed"]).status.code(), Some(12));
    fs::remove_dir_all(dir).unwrap();
}

/// Sources relaxed refuses, each with the warning loose gives instead.
const LEFT_OUT: &[(&str, &str, &str)] = &[
    (
        "NATIONAL",
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. NATIONAL.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n       SPECIAL-NAMES.\n           ALPHABET UNI FOR NATIONAL IS UCS-4.\n       PROCEDURE DIVISION.\n           GOBACK.\n",
        "(--compliance loose): ALPHABET UNI FOR NATIONAL is left out",
    ),
    (
        "DEBUGALL",
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. DEBUGALL.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n       SOURCE-COMPUTER. X WITH DEBUGGING MODE.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01 I PIC 9.\n       PROCEDURE DIVISION.\n       DECLARATIVES.\n       D SECTION.\n           USE FOR DEBUGGING ON ALL REFERENCES OF I MAIN.\n           DISPLAY DEBUG-NAME.\n       END DECLARATIVES.\n       MAIN SECTION.\n           MOVE 1 TO I\n           GOBACK.\n",
        "(--compliance loose): ALL REFERENCES OF I is left out of the USE statement",
    ),
    (
        "ANYNUM",
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. ANYNUM.\n       DATA DIVISION.\n       LINKAGE SECTION.\n       01 BUF PIC X(4).\n       01 LEN ANY NUMERIC.\n       PROCEDURE DIVISION USING BUF LEN.\n           DISPLAY BUF\n           GOBACK.\n",
        "(--compliance loose): the parameter keeps its place, and a statement naming LEN compiles as a hole",
    ),
    (
        "CONTROLS",
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. CONTROLS.\n       ENVIRONMENT DIVISION.\n       INPUT-OUTPUT SECTION.\n       FILE-CONTROL.\n           SELECT RPT ASSIGN TO 'controls.rpt'.\n       DATA DIVISION.\n       FILE SECTION.\n       FD RPT REPORT IS R-1.\n       WORKING-STORAGE SECTION.\n       01 CX PIC X(6).\n       REPORT SECTION.\n       RD R-1 CONTROL IS CX(1:3).\n       01 DET TYPE DE LINE PLUS 1.\n          02 COLUMN 1 PIC X(6) SOURCE IS CX.\n       PROCEDURE DIVISION.\n           OPEN OUTPUT RPT\n           INITIATE R-1\n           GENERATE DET\n           CLOSE RPT\n           GOBACK.\n",
        "(--compliance loose): the report R-1 is left out, and a statement naming it or one of its groups compiles as a hole",
    ),
];

#[test]
fn alphabets_debugging_items_parameters_and_reports_are_left_out() {
    let dir = temp("forms");
    for (name, text, warned) in LEFT_OUT {
        let path = dir.join(format!("{name}.cbl"));
        fs::write(&path, text).unwrap();
        let path = path.to_str().unwrap();
        let o = ironwork(&["check", path, "--compliance", "loose"]);
        assert_eq!(o.status.code(), Some(4), "{name}: {}", stderr(&o));
        assert!(stderr(&o).contains(warned), "{name}: {}", stderr(&o));
        assert_eq!(ironwork(&["check", path, "--compliance", "relaxed"]).status.code(), Some(12), "{name}");
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_communication_statement_and_a_perform_into_declaratives_are_holes() {
    let dir = temp("holes");
    let sources = [
        ("CD", "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. CD.\n       DATA DIVISION.\n       COMMUNICATION SECTION.\n       CD COMM FOR INPUT.\n       01 IN-AREA PIC X(80).\n       PROCEDURE DIVISION.\n           DISPLAY 'before'\n           ENABLE INPUT COMM\n           DISPLAY 'after'\n           GOBACK.\n", "the ENABLE statement at line 9"),
        (
            "THRU",
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. THRU.\n       PROCEDURE DIVISION.\n       DECLARATIVES.\n       D SECTION.\n           USE AFTER ERROR PROCEDURE ON INPUT.\n       D-1.\n           DISPLAY 'declarative'.\n       END DECLARATIVES.\n       MAIN SECTION.\n       M-1.\n           DISPLAY 'before'\n           PERFORM D-1 THRU M-2\n           DISPLAY 'after'\n           GOBACK.\n       M-2.\n           EXIT.\n",
            "the statement at line 13",
        ),
    ];
    for (name, text, hole) in sources {
        let path = dir.join(format!("{name}.cbl"));
        fs::write(&path, text).unwrap();
        let path = path.to_str().unwrap();
        for executor in ["--vm", "--interpret"] {
            let o = ironwork(&["run", path, "--compliance=loose", executor]);
            assert_eq!(String::from_utf8_lossy(&o.stdout), "before\n", "{name} {executor}: {}", stderr(&o));
            assert_eq!(o.status.code(), Some(244), "{name} {executor}: {}", stderr(&o));
            assert!(stderr(&o).contains(&format!("IWR0078-S {hole} was reached")), "{name} {executor}: {}", stderr(&o));
        }
        assert_eq!(ironwork(&["check", path, "--compliance", "relaxed"]).status.code(), Some(12), "{name}");
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_run_reaches_a_hole_only_where_a_statement_names_a_left_out_item() {
    let dir = temp("run");
    for (n, shown, code) in [(7, "ok  7\nEND\n", 0), (0, "ok  0\n", 244)] {
        let path = program(&dir, n);
        for executor in ["--vm", "--interpret"] {
            let o = ironwork(&["run", &path, "--compliance=loose", executor]);
            assert_eq!(String::from_utf8_lossy(&o.stdout), shown, "{n} {executor}: {}", stderr(&o));
            assert_eq!(o.status.code(), Some(code), "{n} {executor}: {}", stderr(&o));
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
