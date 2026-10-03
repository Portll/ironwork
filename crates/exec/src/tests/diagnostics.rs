//! Messages below S that IBM gives and carries on from: each is reported, and the program runs as
//! it would without it.

use super::*;
use numeric::options::{Numproc, Trunc};

fn compiled(source: &str) -> compile::Compiled {
    compile(syntax::parse(source).unwrap_or_else(|e| panic!("{e}")), &[]).unwrap_or_else(|e| panic!("{e:?}"))
}

fn diagnostics(source: &str) -> Vec<(Severity, String)> {
    compiled(source).diagnostics.iter().map(|d| (d.severity, d.message.clone())).collect()
}

fn hello() -> String {
    [line("DISPLAY 'HELLO'"), line("GOBACK.")].concat()
}

#[test]
fn a_program_with_no_stop_run_goback_or_exit_program_is_warned_and_runs() {
    let source = program("", "", &line("DISPLAY 'HELLO'."));
    assert_eq!(diagnostics(&source), [(Severity::Warning, "no STOP RUN, GOBACK or EXIT PROGRAM in the program: check that it ends".to_owned())]);
    assert_eq!(run(&source), "HELLO\n");
    for end in ["STOP RUN", "GOBACK", "EXIT PROGRAM"] {
        let nested = program("", "       01  A PIC X.\n", &[line("DISPLAY 'HELLO'"), line(&format!("IF A = 'Y' {end} END-IF."))].concat());
        assert_eq!(diagnostics(&nested), [], "{end}");
    }
}

#[test]
fn numproc_mig_is_a_warning_and_the_default_numproc_is_in_effect() {
    let c = compiled(&program("NUMPROC(PFD),NUMPROC(MIG)", "", &hello()));
    assert_eq!(c.options.numproc, Numproc::Nopfd);
    let messages: Vec<(Severity, &str)> = c.diagnostics.iter().map(|d| (d.severity, d.message.as_str())).collect();
    assert_eq!(messages, [(Severity::Warning, "CBL NUMPROC(MIG): NUMPROC(MIG) was removed in Enterprise COBOL V5, so NUMPROC(NOPFD) is in effect")]);
    assert_eq!(run(&program("NUMPROC(MIG)", "", &hello())), "HELLO\n");
}

#[test]
fn an_invalid_suboption_is_an_error_and_the_option_is_discarded() {
    let c = compiled(&program("TRUNC(BIN),TRUNC(FAST)", "", &hello()));
    assert_eq!(c.options.trunc, Trunc::Bin);
    let messages: Vec<(Severity, &str)> = c.diagnostics.iter().map(|d| (d.severity, d.message.as_str())).collect();
    assert_eq!(messages, [(Severity::Error, "CBL TRUNC(FAST): TRUNC does not take (FAST)")]);
    assert_eq!(run(&program("TRUNC(FAST)", "", &hello())), "HELLO\n");
    let errors = compile(syntax::parse(&program("CODEPAGE(290)", "", &hello())).unwrap(), &[]).err().expect("a code page ironwork does not carry is refused");
    assert_eq!(errors.iter().map(|e| e.severity).collect::<Vec<_>>(), [Severity::Severe]);
}

#[test]
fn options_enterprise_cobol_no_longer_has_are_accepted_without_effect() {
    let given = |card: &str| diagnostics(&program(card, "", &hello()));
    let informational = |m: &str| vec![(Severity::Informational, m.to_owned())];
    let warning = |m: &str| vec![(Severity::Warning, m.to_owned())];
    assert_eq!(given("LIB"), informational("CBL LIB: LIB is no longer needed: COPY members are always read from the libraries"));
    assert_eq!(given("SIZE(MAX)"), informational("CBL SIZE(MAX): SIZE was removed in Enterprise COBOL V5 and has no effect"));
    assert_eq!(given("SZ(2097152)"), informational("CBL SZ(2097152): SIZE was removed in Enterprise COBOL V5 and has no effect"));
    assert_eq!(given("FLAGSAA"), warning("CBL FLAGSAA: FLAGSAA is not an Enterprise COBOL option and has no effect"));
    assert_eq!(given("NOFDUMP"), warning("CBL NOFDUMP: NOFDUMP is not an Enterprise COBOL option and has no effect"));
    assert_eq!(given("FDUMP"), []);
    assert_eq!(run(&program("LIB,SIZE(MAX),FLAGSAA,NOFDUMP", "", &hello())), "HELLO\n");
}

#[test]
fn a_non_cobol_character_in_a_name_is_an_error_and_the_program_runs() {
    let source = program("", "       01  WS@A PIC X VALUE 'Y'.\n", &[line("DISPLAY WS@A"), line("GOBACK.")].concat());
    let messages: Vec<(Severity, u32, u32, String)> = compiled(&source).diagnostics.into_iter().map(|d| (d.severity, d.pos.line, d.pos.col, d.message)).collect();
    let accepted = |line: u32, col: u32| (Severity::Error, line, col, "non-COBOL character '@': the character was accepted".to_owned());
    assert_eq!(messages, [accepted(5, 14), accepted(7, 22)]);
    assert!(!refused(&compiled(&source).diagnostics, &Options::default()));
    assert_eq!(run(&source), "Y\n");
}

#[test]
fn a_declarative_section_with_no_paragraph_after_its_use_statement_is_informational() {
    let source = [
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       ENVIRONMENT DIVISION.\n       INPUT-OUTPUT SECTION.\n",
        "       FILE-CONTROL.\n           SELECT F ASSIGN TO FDD.\n       DATA DIVISION.\n       FILE SECTION.\n       FD  F.\n       01  R PIC X.\n",
        "       PROCEDURE DIVISION.\n       DECLARATIVES.\n       E SECTION.\n           USE AFTER ERROR PROCEDURE ON F.\n       END DECLARATIVES.\n",
        "       MAIN SECTION.\n       M.\n           DISPLAY 'HELLO'\n           GOBACK.\n",
    ]
    .concat();
    let c = compiled(&source);
    let messages: Vec<(Severity, u32, &str)> = c.diagnostics.iter().map(|d| (d.severity, d.pos.line, d.message.as_str())).collect();
    assert_eq!(messages, [(Severity::Informational, 15, "E SECTION: no paragraph-name after its USE statement")]);
    assert_eq!(run(&source), "HELLO\n");
}
