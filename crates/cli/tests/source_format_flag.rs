//! `--source-format auto|fixed|free` under `--compliance extended`: a file whose author compiles it
//! with `cobc -free`, read as such by detection or by the flag, and the flag refused under strict.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

fn ironwork(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ironwork")).args(args).output().unwrap()
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iw-source-format-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Fixed form cuts A-LONG-NAME to A-LONG at column 72; free form reads the whole name.
fn program(dir: &std::path::Path) -> String {
    let text = format!(
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. CUT.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01 A-LONG-NAME PIC X(3).\n       PROCEDURE DIVISION.\n{:<66}A-LONG-NAME\n           DISPLAY A-LONG-NAME\n           GOBACK.\n",
        "           MOVE 'ABC' TO"
    );
    let path = dir.join("CUT.cbl");
    fs::write(&path, text).unwrap();
    path.to_str().unwrap().to_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn auto_and_free_read_a_cut_file_in_free_form_and_fixed_keeps_it_fixed() {
    let dir = temp("auto");
    let path = program(&dir);
    for format in [&[][..], &["--source-format", "auto"], &["--source-format=free"]] {
        let argv: Vec<&str> = ["run", path.as_str(), "--compliance", "extended"].iter().copied().chain(format.iter().copied()).collect();
        let o = ironwork(&argv);
        assert_eq!(String::from_utf8_lossy(&o.stdout), "ABC\n", "{argv:?}: {}", stderr(&o));
        assert!(stderr(&o).contains("IWX0001-W"), "{argv:?}: {}", stderr(&o));
    }
    let fixed = ironwork(&["check", &path, "--compliance", "extended", "--source-format", "fixed"]);
    assert_eq!(fixed.status.code(), Some(12), "{}", stderr(&fixed));
    assert!(stderr(&fixed).contains("A-LONG is not defined"), "{}", stderr(&fixed));
    let out = dir.join("out");
    let compiled = ironwork(&["compile", &path, "--compliance=extended", "-o", out.to_str().unwrap()]);
    assert_eq!(compiled.status.code(), Some(4), "{}", stderr(&compiled));
    assert!(out.join("CUT.iwm").exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_flag_needs_extended_and_one_of_its_three_values() {
    let dir = temp("refused");
    let path = program(&dir);
    for strict in [&["--source-format", "free"][..], &["--source-format=fixed"]] {
        let argv: Vec<&str> = ["check", path.as_str()].iter().copied().chain(strict.iter().copied()).collect();
        let o = ironwork(&argv);
        assert_eq!(o.status.code(), Some(2), "{argv:?}");
        assert!(stderr(&o).contains("--source-format is for --compliance extended"), "{argv:?}: {}", stderr(&o));
    }
    assert!(ironwork(&["check", &path, "--source-format", "auto"]).status.code() == Some(12));
    for bad in [&["--source-format"][..], &["--source-format", "variable"], &["--source-format=FREE"]] {
        let argv: Vec<&str> = ["check", path.as_str(), "--compliance", "extended"].iter().copied().chain(bad.iter().copied()).collect();
        let o = ironwork(&argv);
        assert_eq!(o.status.code(), Some(2), "{argv:?}");
        assert!(stderr(&o).contains("--source-format needs fixed, free or auto"), "{argv:?}: {}", stderr(&o));
    }
    fs::remove_dir_all(dir).unwrap();
}
