//! `--assume ID=VALUE`: one chosen assumption switched on each command that compiles, refused by
//! name where there is no alternative, and recorded in the load module, provenance and the journal.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn ironwork(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ironwork")).args(args).output().unwrap()
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iw-assume-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("lib")).unwrap();
    dir
}

/// Two ROUNDED COMPUTEs: 0000512 377 under C101=ibm, 0000511 377 under gnucobol, 0000511 376 off.
fn program(dir: &Path) -> String {
    let lines = [
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. ROUNDS.",
        "DATA DIVISION.",
        "WORKING-STORAGE SECTION.",
        "01  D PIC 9(5)V99 VALUE 1.",
        "01  E PIC 9(5)V99 VALUE 12.35.",
        "01  S PIC 99V9.",
        "01  DIV2 PIC 99V9 VALUE 44.1.",
        "PROCEDURE DIVISION.",
        "    COMPUTE D ROUNDED = D + E / 3",
        "    COMPUTE S ROUNDED = 1661.7 / DIV2",
        "    DISPLAY D ' ' S",
        "    GOBACK.",
    ];
    let path = dir.join("lib/ROUNDS.cbl");
    fs::write(&path, lines.iter().map(|l| format!("       {l}\n")).collect::<String>()).unwrap();
    path.to_str().unwrap().to_owned()
}

fn stdout(o: &Output) -> String {
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn run_takes_each_value_in_either_spelling_on_both_executors() {
    let dir = temp("run");
    let path = program(&dir);
    for executor in [&[][..], &["--vm"][..]] {
        for (args, shown) in [
            (&[][..], "0000512 377\n"),
            (&["--assume", "C101=gnucobol"], "0000511 377\n"),
            (&["--assume=C101=off"], "0000511 376\n"),
            (&["--dialect", "gnucobol", "--assume", "C101=ibm"], "0000512 377\n"),
            (&["--assume", "C101=off", "--assume", "C101=ibm"], "0000512 377\n"),
        ] {
            let argv: Vec<&str> = ["run", path.as_str()].iter().copied().chain(executor.iter().copied()).chain(args.iter().copied()).collect();
            assert_eq!(stdout(&ironwork(&argv)), shown, "{argv:?}");
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn what_cannot_be_switched_is_refused_by_name_as_a_usage_error() {
    let dir = temp("refused");
    let path = program(&dir);
    for (bad, said) in [
        (&["--assume"][..], "--assume needs ID=VALUE, such as C101=off"),
        (&["--assume", "C101"], "--assume C101: needs ID=VALUE, such as C101=off"),
        (&["--assume", "C1=off"], "--assume C1=off: assumption C1 (documented) has no alternative; --assume switches C101, C14, C95, C15, C51, C180 and C262"),
        (&["--assume=C9999=off"], "--assume C9999=off: the register has no assumption C9999; ironwork assumptions lists them"),
        (&["--assume", "C14=off"], "--assume C14=off: C14 takes ibm or gnucobol"),
    ] {
        for (command, status) in [("check", 2), ("run", 246)] {
            let argv: Vec<&str> = [command, path.as_str()].iter().copied().chain(bad.iter().copied()).collect();
            let o = ironwork(&argv);
            assert_eq!(o.status.code(), Some(status), "{argv:?}");
            assert_eq!(String::from_utf8_lossy(&o.stderr).lines().next(), Some(format!("ironwork: {said}").as_str()), "{argv:?}");
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_module_keeps_the_choices_dump_shows_them_and_provenance_and_the_journal_record_them() {
    let dir = temp("recorded");
    let path = program(&dir);
    let out = dir.join("out");
    assert!(ironwork(&["compile", &path, "-o", out.to_str().unwrap(), "--assume", "C101=off", "--assume=C14=gnucobol"]).status.success());
    let module = out.join("ROUNDS.iwm");
    assert_eq!(stdout(&ironwork(&["run", module.to_str().unwrap()])), "0000511 376\n");
    let dumped = stdout(&ironwork(&["dump", "--section", "OPTIONS", module.to_str().unwrap()]));
    let assumed: Vec<&str> = dumped.lines().filter(|l| l.contains("assume")).collect();
    assert_eq!(assumed, ["ROUNDS assume C101=off", "ROUNDS assume C14=gnucobol"], "{dumped}");
    let o = ironwork(&["dump", module.to_str().unwrap(), "--assume", "C101=off"]);
    assert_eq!(o.status.code(), Some(2));

    let (prov, ev) = (dir.join("prov.json"), dir.join("ev"));
    let o = ironwork(&["run", &path, "--assume", "C101=off", "--dialect", "gnucobol", "--provenance", prov.to_str().unwrap(), "--evidence", ev.to_str().unwrap()]);
    assert_eq!(stdout(&o), "0000511 376\n");
    let statement = fs::read_to_string(&prov).unwrap();
    assert!(statement.contains("\"flags\":[\"--assume=C101=off\",\"--dialect=gnucobol\"]"), "{statement}");
    let in_force = "\"assumed\":{\"C101\":\"off\",\"C14\":\"gnucobol\",\"C15\":\"gnucobol\",\"C180\":\"gnucobol\",\"C262\":\"gnucobol\",\"C51\":\"gnucobol\",\"C95\":\"gnucobol\"}";
    assert!(statement.contains(in_force), "{statement}");
    let run = fs::read_dir(ev.join("runs")).unwrap().next().unwrap().unwrap().path();
    let open = fs::read_to_string(run).unwrap().lines().next().unwrap().to_owned();
    assert!(open.contains("\"argv\":[\"run\",\"--assume\",\"C101=off\",\"--dialect\",\"gnucobol\""), "{open}");
    let ibm = dir.join("ibm.json");
    assert_eq!(stdout(&ironwork(&["check", &path, "--provenance", ibm.to_str().unwrap()])), "");
    assert!(fs::read_to_string(&ibm).unwrap().contains("\"assumed\":{}"));
    fs::remove_dir_all(dir).unwrap();
}
