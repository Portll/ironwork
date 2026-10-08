//! The `hardened` feature's defaults: DD names from the environment only with
//! `--allow-environment`, no network without `--allow-network`, and the version naming the build.
//! Without the feature, both flags are accepted and change nothing.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

fn ironwork(args: &[&str], dd: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ironwork"));
    if let Some(path) = dd {
        command.env("DD_SECRET", path);
    }
    command.args(args).output().unwrap()
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iw-hardened-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A program that opens the file DD SECRET names and shows the OPEN's file status.
fn program(dir: &std::path::Path) -> (String, String) {
    let text = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. OPENS.\n       ENVIRONMENT DIVISION.\n       INPUT-OUTPUT SECTION.\n       FILE-CONTROL.\n           SELECT F ASSIGN TO SECRET\n               ORGANIZATION IS LINE SEQUENTIAL\n               FILE STATUS IS FS.\n       DATA DIVISION.\n       FILE SECTION.\n       FD F.\n       01 R PIC X(20).\n       WORKING-STORAGE SECTION.\n       01 FS PIC XX.\n       PROCEDURE DIVISION.\n           OPEN INPUT F\n           DISPLAY FS\n           STOP RUN.\n";
    let path = dir.join("OPENS.cbl");
    fs::write(&path, text).unwrap();
    let secret = dir.join("secret.txt");
    fs::write(&secret, "secret\n").unwrap();
    (path.to_str().unwrap().to_owned(), secret.to_str().unwrap().to_owned())
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
#[cfg(feature = "hardened")]
fn the_hardened_build_reads_no_dd_from_the_environment_and_opens_no_network_unless_allowed() {
    let dir = temp("on");
    let (path, secret) = program(&dir);
    assert_eq!(stdout(&ironwork(&["run", &path], Some(&secret))), "35\n");
    assert_eq!(stdout(&ironwork(&["run", &path, "--allow-environment"], Some(&secret))), "00\n");
    let refused = ironwork(&["run", &path, "--sql-db", "postgres://u@127.0.0.1:1/db"], None);
    assert!(String::from_utf8_lossy(&refused.stderr).contains("give --allow-network"), "{refused:?}");
    assert_ne!(refused.status.code(), Some(0));
    assert!(stdout(&ironwork(&["--version"], None)).trim_end().ends_with("(hardened)"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
#[cfg(not(feature = "hardened"))]
fn the_default_build_takes_the_allow_flags_and_reads_dd_names_from_the_environment() {
    let dir = temp("off");
    let (path, secret) = program(&dir);
    assert_eq!(stdout(&ironwork(&["run", &path], Some(&secret))), "00\n");
    assert_eq!(stdout(&ironwork(&["run", &path, "--allow-network", "--allow-environment"], Some(&secret))), "00\n");
    assert!(!stdout(&ironwork(&["--version"], None)).contains("hardened"));
    fs::remove_dir_all(dir).unwrap();
}
