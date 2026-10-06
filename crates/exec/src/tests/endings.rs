//! Endings Language Environment gives a run that breaks a rule the runtime checks (assumption C453).

use super::*;

/// Fixed-format lines, each after the seven columns before area A and ending by column 72.
fn cobol(lines: &[&str]) -> String {
    lines
        .iter()
        .map(|l| {
            assert!(l.len() <= 65, "{l:?} runs past column 72");
            format!("       {l}\n")
        })
        .collect()
}

/// The ending each executor gives `source`, which must agree: the code, the message and what was
/// displayed first.
fn ended(source: &str) -> (String, String, String) {
    let walker = Harness::source(source).run(Executor::Interpreter);
    let vm = Harness::source(source).run(Executor::Vm);
    assert_eq!((&vm.out, &vm.ending), (&walker.out, &walker.ending), "{}", vm.err);
    let abend = walker.ending.unwrap_err();
    (abend.code.to_string(), abend.message, walker.out)
}

fn sorting(input_procedure: &str) -> String {
    cobol(&[
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. SORTER.",
        "ENVIRONMENT DIVISION.",
        "INPUT-OUTPUT SECTION.",
        "FILE-CONTROL.",
        "    SELECT S-FILE ASSIGN TO SORTWK.",
        "DATA DIVISION.",
        "FILE SECTION.",
        "SD  S-FILE.",
        "01  S-REC PIC X(4).",
        "PROCEDURE DIVISION.",
        "MAIN-P.",
        "    SORT S-FILE ON ASCENDING KEY S-REC",
        "        INPUT PROCEDURE IN-P OUTPUT PROCEDURE OUT-P",
        "    DISPLAY 'AFTER'",
        "    GOBACK.",
        "IN-P.",
        input_procedure,
        "OUT-P.",
        "    CONTINUE.",
    ])
}

#[test]
fn a_stop_run_or_goback_in_a_sort_procedure_ends_the_run_with_igz0012s() {
    for (statement, how) in [("    STOP RUN.", "STOP RUN"), ("    GOBACK.", "GOBACK or EXIT PROGRAM")] {
        let (code, message, out) = ended(&sorting(statement));
        assert_eq!((code.as_str(), out.as_str()), ("U4038", ""), "{statement}");
        assert_eq!(message, format!("IGZ0012S There was an invalid attempt to end a sort or merge. ({how} in a procedure of SORT S-FILE)"));
    }
}

#[test]
fn a_goback_in_an_xml_processing_procedure_ends_the_run_with_igz0227s() {
    let source = |procedure: &str| {
        cobol(&[
            "IDENTIFICATION DIVISION.",
            "PROGRAM-ID. PARSER.",
            "DATA DIVISION.",
            "WORKING-STORAGE SECTION.",
            "01  DOC PIC X(8) VALUE '<a>x</a>'.",
            "PROCEDURE DIVISION.",
            "MAIN-P.",
            "    XML PARSE DOC PROCESSING PROCEDURE P",
            "    DISPLAY 'AFTER'",
            "    GOBACK.",
            "P.",
            procedure,
        ])
    };
    let (code, message, out) = ended(&source("    GOBACK."));
    assert_eq!((code.as_str(), out.as_str()), ("U4038", ""));
    assert!(message.starts_with("IGZ0227S There was an invalid attempt to end an XML PARSE statement."), "{message}");
    let stopped = Harness::source(&source("    STOP RUN.")).run(Executor::Interpreter);
    assert_eq!(stopped.ending, Ok(Ending::StopRun), "STOP RUN may end a processing procedure");
}

#[test]
fn a_cancel_of_an_active_program_ends_the_run_with_igz0032s() {
    let source = cobol(&[
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. MAIN.",
        "DATA DIVISION.",
        "WORKING-STORAGE SECTION.",
        "01  WS-A PIC X(8) VALUE 'A'.",
        "PROCEDURE DIVISION.",
        "    CALL WS-A",
        "    GOBACK.",
        "END PROGRAM MAIN.",
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. A.",
        "PROCEDURE DIVISION.",
        "    CALL 'B'",
        "    GOBACK.",
        "END PROGRAM A.",
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. B.",
        "PROCEDURE DIVISION.",
        "    CANCEL 'A'",
        "    GOBACK.",
        "END PROGRAM B.",
    ]);
    let (code, message, _) = ended(&source);
    assert_eq!((code.as_str(), message.as_str()), ("U4038", "IGZ0032S A CANCEL was attempted on active program A."));
}
