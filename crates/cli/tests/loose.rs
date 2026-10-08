//! `--compliance loose`: a data record extended refuses is left out and a statement naming one of
//! its items compiles as a hole; a directive ironwork does not read and a character outside
//! COBOL's set are left out; messages of severity E are warnings.

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
    let ids: Vec<&str> = said.lines().filter_map(|l| l.split("warning: ").nth(1)).map(|w| &w[..7]).filter(|id| matches!(*id, "IWX0064" | "IWX0065" | "IWX0059" | "IWS0105")).collect();
    assert_eq!(ids.iter().filter(|i| **i == "IWX0064").count(), 2, "{said}");
    assert_eq!(ids.iter().filter(|i| **i == "IWX0065").count(), 2, "{said}");
    assert!(ids.contains(&"IWS0105") && ids.contains(&"IWX0059"), "{said}");
    assert!(said.contains("the record T-REC (--compliance loose): IWR0003-S TYPEDEF"), "{said}");
    assert_eq!(ironwork(&["check", &path, "--compliance", "relaxed"]).status.code(), Some(12));
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
