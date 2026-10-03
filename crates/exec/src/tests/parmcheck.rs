use super::*;

/// MAIN, whose WORKING-STORAGE ends with LAST-ITEM, and two programs it calls: LONGER, whose
/// parameter is 16 bytes, and SAME, whose parameter is 4 bytes, as MAIN's items are.
fn caller_and_callees(card: &str, body: &[String]) -> String {
    let card = if card.is_empty() { String::new() } else { format!("       CBL {card}\n") };
    [
        &card,
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. MAIN.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n",
        "       01  FIRST-ITEM PIC X(4) VALUE 'AAAA'.\n       01  LAST-ITEM PIC X(4) VALUE 'BBBB'.\n",
        "       PROCEDURE DIVISION.\n",
        &body.concat(),
        &line("DISPLAY FIRST-ITEM ' ' LAST-ITEM"),
        "           GOBACK.\n       END PROGRAM MAIN.\n",
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. LONGER.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  SPARE PIC X(32).\n",
        "       LINKAGE SECTION.\n       01  L PIC X(16).\n       PROCEDURE DIVISION USING L.\n",
        &line("MOVE ALL 'Z' TO L"),
        "           GOBACK.\n       END PROGRAM LONGER.\n",
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. SAME.\n       DATA DIVISION.\n       LINKAGE SECTION.\n       01  L PIC X(4).\n       PROCEDURE DIVISION USING L.\n",
        &line("MOVE ALL 'Y' TO L"),
        "           GOBACK.\n       END PROGRAM SAME.\n",
    ]
    .concat()
}

#[test]
fn under_parmcheck_msg_a_call_that_writes_past_working_storage_is_a_warning_and_the_run_goes_on() {
    let source = caller_and_callees("PARMCHECK", &[line("CALL 'LONGER' USING LAST-ITEM"), line("CALL 'LONGER' USING FIRST-ITEM BY CONTENT LAST-ITEM")]);
    let (out, err, ending) = run_unit(&source, vec![], "");
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "ZZZZ ZZZZ\n");
    assert_eq!(
        err,
        "ironwork: 9:12: PARMCHECK: LONGER, called at line 9 of program MAIN, wrote past the end of WORKING-STORAGE, beyond parameter LAST-ITEM\n\
         ironwork: 10:12: PARMCHECK: LONGER, called at line 10 of program MAIN, wrote past the end of WORKING-STORAGE, beyond parameter FIRST-ITEM\n"
    );
}

#[test]
fn under_parmcheck_abd_a_call_that_writes_past_working_storage_ends_the_run_with_u4038() {
    let source = caller_and_callees("PC(ABD)", &[line("CALL 'LONGER' USING LAST-ITEM")]);
    let (out, err, ending) = run_unit(&source, vec![], "");
    let abend = ending.unwrap_err();
    assert_eq!(abend.code, "U4038");
    assert_eq!(abend.message, "PARMCHECK: LONGER, called at line 9 of program MAIN, wrote past the end of WORKING-STORAGE, beyond parameter LAST-ITEM");
    assert_eq!((out.as_str(), err.as_str()), ("", ""));
}

#[test]
fn without_parmcheck_or_with_a_callee_that_keeps_to_its_parameter_nothing_is_reported() {
    for card in ["", "NOPARMCHECK"] {
        let (out, err, ending) = run_unit(&caller_and_callees(card, &[line("CALL 'LONGER' USING LAST-ITEM")]), vec![], "");
        assert!(ending.is_ok(), "{card}: {ending:?} {err}");
        assert_eq!((out.as_str(), err.as_str()), ("AAAA ZZZZ\n", ""), "{card}");
    }
    let (out, err, ending) = run_unit(&caller_and_callees("PARMCHECK(ABD,1)", &[line("CALL 'SAME' USING LAST-ITEM"), line("CALL 'SAME' USING FIRST-ITEM")]), vec![], "");
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!((out.as_str(), err.as_str()), ("YYYY YYYY\n", ""));
}

#[test]
fn parmcheck_checks_a_call_of_a_language_environment_service() {
    let data = "       01  HOURS PIC S9(9) BINARY.\n       01  MINUTES PIC S9(9) BINARY.\n       01  SECONDS COMP-2.\n       01  FC PIC X(4).\n";
    let source = program("PARMCHECK", data, &[line("CALL 'CEEGMTO' USING HOURS MINUTES SECONDS FC"), line("GOBACK.")].concat());
    let (_, err, ending) = run_unit(&source, vec![], "");
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(err, "ironwork: 11:12: PARMCHECK: CEEGMTO, called at line 11 of program T, wrote past the end of WORKING-STORAGE, beyond parameter FC\n");
}

/// The run on the VM alone, which fails where the VM stops.
fn on_vm(source: &str) -> (String, String, Result<Ending, Abend>) {
    let o = Harness::source(source).clock(unit::Clock::Fixed(1_790_510_400, 42)).run(Executor::Vm);
    (o.out, o.err, o.ending)
}

#[test]
fn a_callee_that_ends_the_run_with_stop_run_is_not_tested() {
    let source = caller_and_callees("PARMCHECK(ABD)", &[line("CALL 'LONGER' USING LAST-ITEM")]).replace("GOBACK.\n       END PROGRAM LONGER.", "STOP RUN.\n       END PROGRAM LONGER.");
    let (out, err, ending) = run_unit(&source, vec![], "");
    assert_eq!(ending.map(|(e, _)| e), Ok(Ending::StopRun), "{err}");
    assert_eq!((out.as_str(), err.as_str()), ("", ""));
    assert_eq!(on_vm(&source), (String::new(), String::new(), Ok(Ending::StopRun)));
}

#[test]
fn a_call_through_a_function_pointer_sets_and_tests_the_buffer_with_no_arguments() {
    let source = [
        "       CBL PARMCHECK(ABD)\n       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n",
        "       01  R USAGE OBJECT REFERENCE.\n       LINKAGE SECTION.\n           COPY JNI.\n       PROCEDURE DIVISION.\n",
        &line("SET ADDRESS OF JNIENV TO JNIENVPTR"),
        &line("SET ADDRESS OF JNINATIVEINTERFACE TO JNIENV"),
        &line("CALL NewGlobalRef USING BY VALUE JNIENVPTR NULL"),
        &line("    RETURNING R"),
        &line("IF R = NULL DISPLAY 'NULL' END-IF"),
        &line("GOBACK."),
    ]
    .concat();
    let (out, err, ending) = run_unit(&source, vec![], "");
    assert_eq!(ending.map(|(e, _)| e), Ok(Ending::Goback), "{err}");
    assert_eq!((out.as_str(), err.as_str()), ("NULL\n", ""));
    assert_eq!(on_vm(&source), (out, err, Ok(Ending::Goback)));
}

#[test]
fn the_buffer_is_n_bytes_after_the_programs_own_working_storage_and_before_what_the_compiler_adds() {
    let laid_out = |card: &str| {
        let source = file_program("           SELECT S ASSIGN TO SORTWK.\n", "       SD  S.\n       01  S-REC PIC X(10).\n", "       01  A PIC X(3).\n       01  B PIC X(5).\n", &line("GOBACK."));
        compile(syntax::parse(&format!("{card}{source}")).unwrap(), &[]).unwrap().layout
    };
    let offset = |l: &layout::Layout, name: &str| l.items.iter().find(|i| i.name.as_deref() == Some(name)).unwrap().offset;
    let plain = laid_out("");
    let checked = laid_out("       CBL PARMCHECK(MSG,5000)\n");
    assert_eq!((plain.parmcheck, checked.parmcheck), (None, Some((13, 5000))));
    assert_eq!(checked.size, plain.size + 5000);
    for name in ["A", "B"] {
        assert_eq!(offset(&checked, name), offset(&plain, name), "{name}");
    }
    for name in ["SORT-RETURN", "S-REC"] {
        assert_eq!(offset(&checked, name), offset(&plain, name) + 5000, "{name}");
    }
    assert_eq!(laid_out("       CBL PC(1)\n").parmcheck, Some((13, 1)));
}
