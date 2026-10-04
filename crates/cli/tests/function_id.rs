//! User-defined functions from the command line: `run` enters the program after the functions
//! before it, `check` reports a function's own errors, a source of functions alone has nothing to
//! run, and `compile` writes no module for a prototype and one for a program that invokes one.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

fn ironwork(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ironwork")).args(args).output().unwrap()
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iw-function-id-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn source(lines: &[&str]) -> String {
    lines.iter().map(|l| format!("       {l}\n")).collect()
}

const DOUBLE: &[&str] = &[
    "IDENTIFICATION DIVISION.",
    "FUNCTION-ID. DOUBLE.",
    "DATA DIVISION.",
    "LINKAGE SECTION.",
    "01 N PIC 9(3).",
    "01 R PIC 9(4).",
    "PROCEDURE DIVISION USING N RETURNING R.",
    "    COMPUTE R = N * 2",
    "    GOBACK.",
    "END FUNCTION DOUBLE.",
];

const MAIN: &[&str] = &[
    "IDENTIFICATION DIVISION.",
    "PROGRAM-ID. MAIN.",
    "DATA DIVISION.",
    "WORKING-STORAGE SECTION.",
    "01 K PIC 9(3) VALUE 21.",
    "PROCEDURE DIVISION.",
    "    DISPLAY 'DOUBLE ' FUNCTION DOUBLE(K)",
    "    GOBACK.",
    "END PROGRAM MAIN.",
];

#[test]
fn run_enters_the_program_after_the_functions_before_it() {
    let dir = temp("run");
    let path = dir.join("double.cbl");
    fs::write(&path, source(DOUBLE) + &source(MAIN)).unwrap();
    let out = ironwork(&["run", path.to_str().unwrap()]);
    fs::remove_dir_all(&dir).unwrap();
    assert_eq!((String::from_utf8_lossy(&out.stdout).as_ref(), out.status.code()), ("DOUBLE 0042\n", Some(0)), "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn check_reports_a_functions_errors_and_a_source_of_functions_has_nothing_to_run() {
    let dir = temp("check");
    let broken = dir.join("broken.cbl");
    let no_returning: Vec<&str> = DOUBLE.iter().map(|l| if l.starts_with("PROCEDURE") { "PROCEDURE DIVISION USING N." } else { l }).collect();
    fs::write(&broken, source(&no_returning) + &source(MAIN)).unwrap();
    let checked = ironwork(&["check", broken.to_str().unwrap()]);
    let alone = dir.join("alone.cbl");
    fs::write(&alone, source(DOUBLE)).unwrap();
    let run = ironwork(&["run", alone.to_str().unwrap()]);
    let checked_alone = ironwork(&["check", alone.to_str().unwrap()]);
    fs::remove_dir_all(&dir).unwrap();
    let stderr = String::from_utf8_lossy(&checked.stderr);
    assert!(stderr.contains("2:8: IWC0017-S FUNCTION-ID DOUBLE: a user-defined function needs PROCEDURE DIVISION RETURNING"), "{stderr}");
    assert_eq!(checked.status.code(), Some(12));
    assert!(String::from_utf8_lossy(&run.stderr).contains("FUNCTION-ID DOUBLE: the source holds user-defined functions and no program to run"));
    assert_eq!((run.status.code(), checked_alone.status.code()), (Some(241), Some(0)));
}

#[test]
fn compile_writes_no_module_for_a_prototype_and_one_for_a_program_that_invokes_a_function() {
    let dir = temp("compile");
    let prototype: Vec<&str> = DOUBLE.iter().map(|&l| if l == "FUNCTION-ID. DOUBLE." { "FUNCTION-ID. DOUBLE IS PROTOTYPE." } else { l }).filter(|l| !l.starts_with("    ")).collect();
    let functions = dir.join("DOUBLE.cbl");
    fs::write(&functions, source(&prototype) + &source(DOUBLE)).unwrap();
    let written = ironwork(&["compile", functions.to_str().unwrap(), "-o", dir.join("out").to_str().unwrap()]);
    let modules: Vec<String> = fs::read_dir(dir.join("out")).map(|d| d.map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    let program = dir.join("main.cbl");
    fs::write(&program, source(DOUBLE) + &source(MAIN)).unwrap();
    let invoking = ironwork(&["compile", program.to_str().unwrap(), "-o", dir.join("out2").to_str().unwrap()]);
    let written_too = dir.join("out2").join("main.iwm").is_file();
    fs::remove_dir_all(&dir).unwrap();
    assert_eq!((written.status.code(), modules), (Some(0), vec!["DOUBLE.iwm".to_owned()]), "{}", String::from_utf8_lossy(&written.stderr));
    assert_eq!((invoking.status.code(), written_too), (Some(0), true), "{}", String::from_utf8_lossy(&invoking.stderr));
}
