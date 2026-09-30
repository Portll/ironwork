use std::path::PathBuf;
use std::process::{Command, Output};

fn ironwork(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ironwork")).args(args).output().unwrap()
}

/// A source in the temp directory, removed when dropped.
struct Source(PathBuf);

impl Source {
    fn new(name: &str, text: &str) -> Self {
        let path = std::env::temp_dir().join(format!("ironwork-diagnostics-{}-{name}.cbl", std::process::id()));
        std::fs::write(&path, text).unwrap();
        Source(path)
    }

    fn path(&self) -> &str {
        self.0.to_str().unwrap()
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// An object-oriented program with no CBL card, so without THREAD and DLL; `data` and `body` add
/// to its WORKING-STORAGE and PROCEDURE DIVISION.
fn object_oriented(data: &str, body: &str) -> String {
    format!(
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. CLIENT.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n       REPOSITORY.\n           CLASS Account IS \"Account\".\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  A1 USAGE OBJECT REFERENCE Account.\n{data}       PROCEDURE DIVISION.\n{body}           IF A1 = NULL DISPLAY 'NO ACCOUNT' END-IF\n           GOBACK.\n"
    )
}

const MISSING: &str = "warning: program CLIENT uses object-oriented syntax, which IBM compiles only with THREAD, DLL, RENT and DBCS: THREAD, DLL missing from its CBL or PROCESS cards (see J13 and J19)";

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn check_exits_0_with_nothing_to_say() {
    let source = Source::new("clean", "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. P.\n       PROCEDURE DIVISION.\n           GOBACK.\n");
    let out = ironwork(&["check", source.path()]);
    assert_eq!((out.status.code(), stderr(&out)), (Some(0), String::new()));
}

#[test]
fn an_object_oriented_program_without_thread_and_dll_checks_with_return_code_4_and_runs() {
    let source = Source::new("oo", &object_oriented("", ""));
    let warning = format!("{}: {MISSING}\n", source.path());
    let checked = ironwork(&["check", source.path()]);
    assert_eq!((checked.status.code(), stderr(&checked)), (Some(4), warning.clone()));
    let ran = ironwork(&["run", source.path()]);
    assert_eq!((ran.status.code(), stderr(&ran), String::from_utf8_lossy(&ran.stdout).into_owned()), (Some(0), warning.clone(), "NO ACCOUNT\n".into()));
    let task = ironwork(&["cics", source.path()]);
    assert_eq!(task.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&task.stdout), "NO ACCOUNT\n");
    assert!(stderr(&task).starts_with(&warning) && stderr(&task).contains("ironwork: the task ended"), "{}", stderr(&task));
}

#[test]
fn warnings_block_refuses_the_run_and_keeps_return_code_4() {
    let source = Source::new("blocked", &object_oriented("", ""));
    let warning = format!("{}: {MISSING}\n", source.path());
    for command in ["check", "run", "cics"] {
        let out = ironwork(&[command, source.path(), "-warnings-block"]);
        assert_eq!((out.status.code(), stderr(&out), out.stdout.is_empty()), (Some(4), warning.clone(), true), "{command}");
    }
}

#[test]
fn an_error_keeps_its_line_and_return_code_12_and_is_listed_before_warnings() {
    let refused = Source::new("undefined", "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. P.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  X PIC X.\n       PROCEDURE DIVISION.\n           MOVE Y TO X.\n           GOBACK.\n");
    let error = format!("{}:7:17: Y is not defined\n", refused.path());
    for command in ["check", "run"] {
        let out = ironwork(&[command, refused.path()]);
        assert_eq!((out.status.code(), stderr(&out), out.stdout.is_empty()), (Some(12), error.clone(), true), "{command}");
    }
    let mixed = Source::new("mixed", &object_oriented("       01  X PIC X.\n", "           MOVE Y TO X\n"));
    let out = ironwork(&["check", mixed.path()]);
    assert_eq!(out.status.code(), Some(12));
    assert_eq!(stderr(&out), format!("{0}:12:17: Y is not defined\n{0}: {MISSING}\n", mixed.path()));
}

#[test]
fn a_syntax_error_is_return_code_12() {
    let source = Source::new("syntax", "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. P.\n       PROCEDURE DIVISION.\n           MOVE TO.\n");
    let out = ironwork(&["check", source.path()]);
    assert_eq!((out.status.code(), stderr(&out)), (Some(12), format!("{}:4:19: expected TO, found Period\n", source.path())));
}

/// `object_oriented`'s program, which compiles with a warning, behind a CBL card.
fn carded(card: &str) -> String {
    format!("       CBL {card}\n{}", object_oriented("", ""))
}

#[test]
fn a_nocompile_card_says_where_run_refuses_and_outranks_warnings_block() {
    let blocked = Source::new("nocompile-w", &carded("NOCOMPILE(W)"));
    let warning = format!("{}: {MISSING}\n", blocked.path());
    for command in ["check", "run", "cics"] {
        let out = ironwork(&[command, blocked.path()]);
        assert_eq!((out.status.code(), stderr(&out), out.stdout.is_empty()), (Some(4), warning.clone(), true), "{command}");
    }
    for (k, card) in ["NOC(E)", "NOCOMPILE(S)", "COMPILE"].into_iter().enumerate() {
        let source = Source::new(&format!("card-{k}"), &carded(card));
        let warning = format!("{}: {MISSING}\n", source.path());
        let checked = ironwork(&["check", source.path(), "-warnings-block"]);
        assert_eq!((checked.status.code(), stderr(&checked)), (Some(4), warning.clone()), "{card}");
        let ran = ironwork(&["run", source.path(), "-warnings-block"]);
        assert_eq!((ran.status.code(), stderr(&ran), String::from_utf8_lossy(&ran.stdout).into_owned()), (Some(0), warning, "NO ACCOUNT\n".into()), "{card}");
    }
}

#[test]
fn nocompile_alone_checks_the_program_and_runs_nothing() {
    let source = Source::new("syntax-check", "       CBL NOCOMPILE\n       IDENTIFICATION DIVISION.\n       PROGRAM-ID. P.\n       PROCEDURE DIVISION.\n           DISPLAY 'RAN'.\n           GOBACK.\n");
    let checked = ironwork(&["check", source.path()]);
    assert_eq!((checked.status.code(), stderr(&checked)), (Some(0), String::new()));
    let ran = ironwork(&["run", source.path()]);
    let note = format!("ironwork: {}: NOCOMPILE is a syntax check, with no program to run\n", source.path());
    assert_eq!((ran.status.code(), stderr(&ran), ran.stdout.is_empty()), (Some(0), note, true));
}
