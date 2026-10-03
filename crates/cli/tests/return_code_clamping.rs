use std::path::PathBuf;
use std::process::{Command, Output};

fn ironwork(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ironwork")).args(args).output().unwrap()
}

/// A source in the temp directory, removed when dropped.
struct Source(PathBuf);

impl Source {
    fn new(name: &str, text: &str) -> Self {
        let path = std::env::temp_dir().join(format!("ironwork-return-code-{}-{name}.cbl", std::process::id()));
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

fn program_with_return_code(rc: &str) -> String {
    format!(
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. P.\n       PROCEDURE DIVISION.\n           MOVE {rc} TO RETURN-CODE.\n           GOBACK.\n"
    )
}

#[test]
fn return_code_256_exits_255() {
    let source = Source::new("rc256", &program_with_return_code("256"));
    let out = ironwork(&["run", source.path()]);
    assert_eq!(out.status.code(), Some(255), "RETURN-CODE 256 should exit 255");
}

#[test]
fn return_code_negative_1_exits_255() {
    let source = Source::new("rc-1", &program_with_return_code("-1"));
    let out = ironwork(&["run", source.path()]);
    assert_eq!(out.status.code(), Some(255), "RETURN-CODE -1 should exit 255");
}

#[test]
fn return_code_4_exits_4() {
    let source = Source::new("rc4", &program_with_return_code("4"));
    let out = ironwork(&["run", source.path()]);
    assert_eq!(out.status.code(), Some(4), "RETURN-CODE 4 should exit 4");
}

#[test]
fn return_code_0_exits_0() {
    let source = Source::new("rc0", &program_with_return_code("0"));
    let out = ironwork(&["run", source.path()]);
    assert_eq!(out.status.code(), Some(0), "RETURN-CODE 0 should exit 0");
}

#[test]
fn return_code_255_exits_255() {
    let source = Source::new("rc255", &program_with_return_code("255"));
    let out = ironwork(&["run", source.path()]);
    assert_eq!(out.status.code(), Some(255), "RETURN-CODE 255 should exit 255");
}

#[test]
fn return_code_1000_exits_255() {
    let source = Source::new("rc1000", &program_with_return_code("1000"));
    let out = ironwork(&["run", source.path()]);
    assert_eq!(out.status.code(), Some(255), "RETURN-CODE 1000 should exit 255");
}
