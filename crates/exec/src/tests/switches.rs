//! UPSI switches: SPECIAL-NAMES condition-names, SET ... TO ON and OFF, and the PARM's UPSI runtime
//! option (Language Reference SC27-8713-03, pp. 125-127, 283, 442-443; Programming Guide
//! SC27-8714-03, p. 595).

use super::*;

fn with_switches(id: &str, entries: &str, procedure: &[String]) -> String {
    format!(
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. {id}.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n       SPECIAL-NAMES.\n{entries}       PROCEDURE DIVISION.\n{}",
        procedure.concat()
    )
}

/// What the program displays and writes to standard error, the same on both executors.
fn on_both(source: &str, parm: Option<&str>) -> (String, String) {
    let harness = || match parm {
        Some(p) => Harness::source(source).parm(p),
        None => Harness::source(source),
    };
    let walker = harness().run(Executor::Interpreter);
    assert!(walker.ending.is_ok(), "{:?}\n{}", walker.ending, walker.err);
    let vm = harness().run(Executor::Vm);
    assert_eq!((&vm.out, &vm.err, &vm.ending), (&walker.out, &walker.err, &walker.ending), "the VM alone");
    (walker.out, walker.err)
}

fn shows(condition: &str) -> String {
    line(&format!("IF {condition} DISPLAY '{condition}'")) + &line(&format!("    ELSE DISPLAY 'NOT {condition}'."))
}

#[test]
fn the_parm_s_upsi_option_sets_the_switches_from_the_leftmost_digit_and_they_are_off_without_it() {
    let entries = "           UPSI-0 ON STATUS IS S0\n           UPSI-1 OFF STATUS IS F1\n           UPSI-7 IS SW-7 ON S7.\n";
    let source = with_switches("T", entries, &[shows("S0"), shows("F1"), shows("S7"), line("GOBACK.")]);
    assert_eq!(on_both(&source, Some("/UPSI(10000001)")).0, "S0\nF1\nS7\n");
    assert_eq!(on_both(&source, Some("ARGS/RPTOPTS(ON) UPSI(01111110)")).0, "NOT S0\nNOT F1\nNOT S7\n");
    assert_eq!(on_both(&source, None).0, "NOT S0\nF1\nNOT S7\n");
    assert_eq!(on_both(&source, Some("UPSI(11111111)")).0, "NOT S0\nF1\nNOT S7\n", "with no slash the PARM is all program arguments");
}

#[test]
fn a_malformed_upsi_option_is_named_and_leaves_the_switches_off() {
    let source = with_switches("T", "           UPSI-0 ON STATUS IS S0.\n", &[shows("S0"), line("GOBACK.")]);
    let (out, err) = on_both(&source, Some("/UPSI(1)"));
    assert_eq!(out, "NOT S0\n");
    assert_eq!(err, "ironwork: runtime option UPSI(1): UPSI takes eight digits, each 0 or 1, so the UPSI switches stay off\n");
}

#[test]
fn set_to_on_and_off_sets_each_named_switch_group_by_group() {
    let entries = "           UPSI-0 IS ABBREV-SWITCH ON ON-SWITCH OFF IS OFF-SWITCH\n           UPSI-2 IS SW-2 ON STATUS IS ON-2.\n";
    let body = [line("SET ABBREV-SWITCH SW-2 TO ON SW-2 TO OFF"), shows("ON-SWITCH"), shows("OFF-SWITCH"), shows("ON-2"), line("SET SW-2 TO ON"), shows("ON-2"), line("GOBACK.")];
    assert_eq!(on_both(&with_switches("T", entries, &body), None).0, "ON-SWITCH\nNOT OFF-SWITCH\nNOT ON-2\nON-2\n");
}

/// The mnemonic-name is the conditional variable: it qualifies a condition-name two switches
/// share, and SET TO TRUE sets its switch.
#[test]
fn a_mnemonic_name_qualifies_its_switch_s_condition_names_and_set_to_true_sets_the_switch() {
    let entries = "           UPSI-4 IS SW-4 ON STATUS IS T OFF STATUS IS F\n           UPSI-5 IS SW-5 ON STATUS IS T OFF STATUS IS F.\n";
    let body = [line("SET T OF SW-5 TO TRUE"), shows("T OF SW-4"), shows("T OF SW-5"), line("SET F OF SW-5 TO TRUE"), shows("F OF SW-5"), line("GOBACK.")];
    assert_eq!(on_both(&with_switches("T", entries, &body), Some("/UPSI(00001000)")).0, "T OF SW-4\nT OF SW-5\nF OF SW-5\n");
}

/// One copy of the switches for the run unit, whatever each program calls them; a contained
/// program has its container's SPECIAL-NAMES.
#[test]
fn every_program_of_the_run_unit_shares_the_switches() {
    let main = with_switches(
        "MAIN",
        "           UPSI-3 IS FIRST-NAME ON STATUS IS M-ON.\n",
        &[line("SET FIRST-NAME TO ON"), line("CALL 'SUB'"), shows("M-ON"), line("CALL 'INNER'"), line("GOBACK.")],
    );
    let inner = ["       IDENTIFICATION DIVISION.\n       PROGRAM-ID. INNER.\n       PROCEDURE DIVISION.\n", &shows("M-ON OF FIRST-NAME"), &line("GOBACK."), "       END PROGRAM INNER.\n       END PROGRAM MAIN.\n"].concat();
    let sub = with_switches("SUB", "           UPSI-3 IS OTHER-NAME OFF STATUS IS S-OFF.\n", &[shows("S-OFF"), line("SET OTHER-NAME TO OFF"), line("GOBACK.")]);
    assert_eq!(on_both(&format!("{main}{inner}{sub}"), None).0, "NOT S-OFF\nNOT M-ON\nNOT M-ON OF FIRST-NAME\n");
}

#[test]
fn a_switch_s_names_are_refused_where_the_language_reference_does_not_allow_them() {
    let entries = "           UPSI-0 IS SW-0 ON STATUS IS SW0-ON\n           UPSI-1 ON STATUS IS SW1-ON\n           UPSI-2 IS SW-2 ON STATUS IS SW0-ON.\n";
    let data = "       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  X PIC X.\n";
    let body = [line("MOVE SW-0 TO X"), line("SET X TO ON"), line("SET SW1-ON TO TRUE"), line("SET SW0-ON OF SW-0 TO FALSE"), line("IF SW0-ON GOBACK."), line("GOBACK.")];
    let source = with_switches("T", entries, &body).replace("       PROCEDURE DIVISION.\n", &format!("{data}       PROCEDURE DIVISION.\n"));
    let errors = compile_errors(&source);
    for expected in [
        "SW-0 is the mnemonic-name of UPSI-0: only SET ... TO ON or OFF and a condition-name's qualifier can name it",
        "SET X TO ON: X is not the mnemonic-name of an UPSI switch",
        "SET SW1-ON TO TRUE: the UPSI switch's entry has no mnemonic-name, which would be its conditional variable",
        "SET SW0-ON TO FALSE: the condition-name has no WHEN SET TO FALSE value",
        "SW0-ON is ambiguous; qualify it with OF or IN",
    ] {
        assert!(errors.contains(expected), "{expected}\n{errors}");
    }
}
