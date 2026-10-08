//! `--autofix DIR`: repairs with one sensible fix, in the program and its COPY members, written to
//! DIR with a diff and a report, and the repaired source checked or run; `--remediate DIR`, the
//! same under `--compliance loose`.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

fn ironwork(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ironwork")).args(args).output().unwrap()
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iw-autofix-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// A missing period, a stray END-IF, a header in Area B, an UNKNOWN-ITEM no fix can mend, and a
/// zero-length literal in the program and in a member.
fn program(dir: &std::path::Path, unknown: bool) -> String {
    fs::write(dir.join("MEMB.cpy"), "       01 M PIC X(3) VALUE ''.\n").unwrap();
    let last = if unknown { "           MOVE UNKNOWN-ITEM TO X\n" } else { "" };
    let text = format!(
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. FIXME.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       COPY MEMB.\n       01 X PIC X(3) VALUE ''.\n       01 N PIC 9 VALUE 1\n       PROCEDURE DIVISION.\n           MAIN-PARA.\n           IF N = 1\n               DISPLAY 'ONE' END-IF.\n           END-IF.\n           DISPLAY '[' X '][' M ']'\n{last}           GOBACK.\n"
    );
    let path = dir.join("FIXME.cbl");
    fs::write(&path, text).unwrap();
    path.to_str().unwrap().to_owned()
}

#[test]
fn run_repairs_the_program_and_its_member_then_runs_the_repaired_source() {
    let dir = temp("run");
    let path = program(&dir, false);
    let out = dir.join("out");
    let o = ironwork(&["run", &path, "--compliance", "extended", "--autofix", out.to_str().unwrap()]);
    assert_eq!(String::from_utf8_lossy(&o.stdout), "ONE\n[   ][   ]\n", "{}", stderr(&o));
    let said = stderr(&o);
    let fixed: Vec<&str> = said.lines().filter(|l| l.contains(": fixed ")).map(|l| l.split(": fixed ").nth(1).unwrap().split(':').next().unwrap()).collect();
    assert_eq!(fixed, ["IWX0063", "IWX0093", "IWX0061", "IWS0104", "IWX0063"], "{}", stderr(&o));
    let diff = fs::read_to_string(out.join("autofix.diff")).unwrap();
    assert!(diff.contains("-           MAIN-PARA.\n+       MAIN-PARA.\n") && diff.contains("+           .\n        PROCEDURE DIVISION.\n"), "{diff}");
    assert!(diff.contains("MEMB.cpy\n@@ -1,1 +1,1 @@\n-       01 M PIC X(3) VALUE ''.\n+       01 M PIC X(3) VALUE ' '.\n"), "{diff}");
    let report = fs::read_to_string(out.join("autofix.json")).unwrap();
    assert!(report.contains("\"remaining\":[]") && report.matches("\"fix\":").count() == 5, "{report}");
    let repaired = out.join("FIXME.cbl");
    let strict = ironwork(&["check", repaired.to_str().unwrap(), "-I", out.to_str().unwrap()]);
    assert_eq!(strict.status.code(), Some(0), "{}", stderr(&strict));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn what_has_no_one_fix_is_left_in_the_report_and_check_gives_its_code() {
    let dir = temp("left");
    let path = program(&dir, true);
    let out = dir.join("out");
    let o = ironwork(&["check", &path, "--compliance=extended", "--autofix", out.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(12), "{}", stderr(&o));
    assert!(stderr(&o).contains("UNKNOWN-ITEM is not defined"), "{}", stderr(&o));
    let report = fs::read_to_string(out.join("autofix.json")).unwrap();
    assert!(report.contains("\"id\":\"IWC0001\"") && report.contains("UNKNOWN-ITEM is not defined"), "{report}");
    assert_eq!(ironwork(&["compile", &path, "--autofix", out.to_str().unwrap()]).status.code(), Some(2));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn remediate_repairs_then_compiles_what_is_left_as_holes_under_loose() {
    let dir = temp("remediate");
    let path = program(&dir, true);
    let out = dir.join("out");
    let o = ironwork(&["check", &path, "--remediate", out.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(4), "{}", stderr(&o));
    let report = fs::read_to_string(out.join("autofix.json")).unwrap();
    assert!(report.contains("\"remaining\":[]") && report.matches("\"fix\":").count() == 5, "{report}");
    let holes = report.split("\"holes\":").nth(1).unwrap().split("\"left_out\":").next().unwrap();
    assert!(holes.contains("\"id\":\"IWX0059\"") && holes.contains("UNKNOWN-ITEM is not defined"), "{report}");
    for executor in ["--vm", "--interpret"] {
        let o = ironwork(&["run", &path, "--remediate", out.to_str().unwrap(), executor]);
        assert_eq!(String::from_utf8_lossy(&o.stdout), "ONE\n[   ][   ]\n", "{executor}: {}", stderr(&o));
        assert_eq!(o.status.code(), Some(244), "{executor}: {}", stderr(&o));
    }
    let o = out.to_str().unwrap();
    for refused in [vec!["check", &path, "--remediate", o, "--autofix", o], vec!["check", &path, "--remediate", o, "--compliance", "extended"], vec!["compile", &path, "--remediate", o]] {
        assert_eq!(ironwork(&refused).status.code(), Some(2), "{refused:?}");
    }
    assert_eq!(ironwork(&["check", &path, "--remediate", o, "--compliance=loose"]).status.code(), Some(4));
    fs::remove_dir_all(dir).unwrap();
}
