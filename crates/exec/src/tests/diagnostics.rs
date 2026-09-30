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
    let errors = compile(syntax::parse(&program("CODEPAGE(930)", "", &hello())).unwrap(), &[]).err().expect("a code page ironwork does not carry is refused");
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
