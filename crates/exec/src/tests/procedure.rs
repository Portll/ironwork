use super::*;

/// A caller and SUBPROG, which has the alternate entry point PAYMASTR, as in the Programming Guide's
/// example (SC27-8714-03, p. 553).
fn caller_and_subprog(caller: &[String]) -> String {
    [
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. CALLER.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n",
        "       01  REC PIC X(5) VALUE 'HELLO'.\n       01  CODE-X PIC 9 VALUE 7.\n       01  PGM PIC X(8) VALUE 'PAYMASTR'.\n",
        "       PROCEDURE DIVISION.\n",
        &caller.concat(),
        "           GOBACK.\n       END PROGRAM CALLER.\n",
        &subprog(),
    ]
    .concat()
}

fn subprog() -> String {
    [
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. SUBPROG.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n",
        "       01  COUNTER PIC 9 VALUE 0.\n       LINKAGE SECTION.\n       01  PAYREC PIC X(5).\n       01  PAY-CODE PIC 9.\n",
        "       PROCEDURE DIVISION USING PAYREC.\n",
        &line("ADD 1 TO COUNTER"),
        &line("DISPLAY 'SUBPROG ' PAYREC ' ' COUNTER"),
        &line("ENTRY 'PASSED'."),
        &line("EXIT PROGRAM."),
        &line("ENTRY 'PAYMASTR' USING PAYREC PAY-CODE."),
        &line("ADD 1 TO COUNTER"),
        &line("DISPLAY 'PAYMASTR ' PAYREC ' ' PAY-CODE ' ' COUNTER"),
        &line("GOBACK."),
        &line("ENTRY 'NOCODE' USING PAYREC."),
        &line("DISPLAY PAY-CODE"),
        &line("GOBACK."),
        "       END PROGRAM SUBPROG.\n",
    ]
    .concat()
}

pub(super) fn fuzz_seeds() -> Vec<String> {
    vec![
        caller_and_subprog(&[line("CALL 'PAYMASTR' USING REC CODE-X"), line("CALL PGM USING REC CODE-X"), line("CANCEL PGM")]),
        program(
            "",
            "       01  I PIC 9.\n       01  J PIC 9.\n       01  X COMP-2.\n",
            &[
                "       MAIN SECTION.\n",
                &line("PERFORM SW VARYING I FROM 1 BY 1 UNTIL I > 2"),
                &line("    AFTER J FROM I BY 1 UNTIL J > 3"),
                &line("ALTER SW TO PROCEED TO P2"),
                &line("COMPUTE X = FUNCTION RANDOM(I) / J"),
                &line("GO TO P1 P2 DEPENDING ON I."),
                "       SEG SECTION 50.\n       SW.\n",
                &line("GO TO P1."),
                "       P1.\n",
                &line("DISPLAY I NO ADVANCING."),
                "       P2.\n",
                &line("GO TO."),
            ]
            .concat(),
        ),
    ]
}

#[test]
fn a_call_of_an_entry_name_begins_after_the_entry_and_binds_its_using_list() {
    let source = caller_and_subprog(&[line("CALL 'SUBPROG' USING REC"), line("CALL 'PAYMASTR' USING REC CODE-X"), line("CALL 'SUBPROG' USING REC")]);
    let (out, err, ending) = run_unit(&source, vec![], "");
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "SUBPROG HELLO 1\nPAYMASTR HELLO 7 2\nSUBPROG HELLO 3\n");
}

#[test]
fn a_linkage_item_the_entry_does_not_name_has_no_address() {
    let (_, _, ending) = run_unit(&caller_and_subprog(&[line("CALL 'NOCODE' USING REC")]), vec![], "");
    assert_eq!(ending.unwrap_err().code, "S0C4");
}

#[test]
fn a_dynamic_call_of_an_entry_name_has_storage_of_its_own_until_cancelled() {
    let source = caller_and_subprog(&[
        line("CALL 'PAYMASTR' USING REC CODE-X"),
        line("CALL PGM USING REC CODE-X"),
        line("CALL PGM USING REC CODE-X"),
        line("CANCEL PGM"),
        line("CALL PGM USING REC CODE-X"),
        line("CALL 'PAYMASTR' USING REC CODE-X"),
    ]);
    let (out, err, ending) = run_unit(&source, vec![], "");
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "PAYMASTR HELLO 7 1\nPAYMASTR HELLO 7 1\nPAYMASTR HELLO 7 2\nPAYMASTR HELLO 7 1\nPAYMASTR HELLO 7 2\n");
}

#[test]
fn under_dynam_a_call_of_a_literal_entry_name_is_dynamic() {
    let source = format!("       CBL DYNAM\n{}", caller_and_subprog(&[line("CALL 'SUBPROG' USING REC"), line("CALL 'PAYMASTR' USING REC CODE-X")]));
    let (out, err, ending) = run_unit(&source, vec![], "");
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "SUBPROG HELLO 1\nPAYMASTR HELLO 7 1\n");
}

#[test]
fn an_entry_name_in_a_program_library_is_an_alias_of_the_member() {
    let dir = temp("entrylib");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("PAYMASTR.cbl"), subprog()).unwrap();
    let source = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. MAIN.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  REC PIC X(5) VALUE 'LIBRY'.\n       01  C PIC 9 VALUE 3.\n       PROCEDURE DIVISION.\n           CALL 'PAYMASTR' USING REC C\n           CALL 'PAYMASTR' USING REC C\n           GOBACK.\n";
    let (out, err, ending) = run_unit(source, vec![dir], "");
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "PAYMASTR LIBRY 3 1\nPAYMASTR LIBRY 3 2\n");
}

#[test]
fn the_entry_statements_rules_are_compile_errors() {
    let with = |linkage: &str, head: &str, body: &str| {
        compile_errors(&format!(
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  W PIC X.\n       LINKAGE SECTION.\n{linkage}       PROCEDURE DIVISION{head}.\n{body}"
        ))
    };
    let l = "       01  L PIC X.\n";
    assert!(with(l, " RETURNING L", &line("ENTRY 'E'.")).contains("RETURNING cannot have ENTRY"));
    assert!(with(l, "", &[line("ENTRY 'T'."), line("ENTRY 'E'."), line("ENTRY 'E'.")].concat()).matches("already the program's or another ENTRY's").count() == 2);
    assert!(with(l, "", &line("ENTRY 'E' USING W.")).contains("ENTRY 'E' USING W: not an 01 or 77 item of the LINKAGE SECTION"));
    assert!(with(l, "", &line("IF W = 'A' ENTRY 'E' END-IF.")).contains("ENTRY 'E' must be a sentence of its own"));
    let nested = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. OUTER.\n       PROCEDURE DIVISION.\n           GOBACK.\n       IDENTIFICATION DIVISION.\n       PROGRAM-ID. INNER.\n       PROCEDURE DIVISION.\n           ENTRY 'E'.\n           GOBACK.\n       END PROGRAM INNER.\n       END PROGRAM OUTER.\n";
    assert!(syntax::parse(nested).unwrap_err().message.contains("ENTRY cannot be used in a nested program"));
    assert!(syntax::parse(&program("", "", &line("ENTRY E."))).unwrap_err().message.contains("an alphanumeric literal naming the entry point"));
}

/// `caller_and_subprog` with procedure-pointers PP and PQ, function-pointer FP and pointer PTR in
/// CALLER's WORKING-STORAGE.
fn pointer_caller(caller: &[String]) -> String {
    let pointers = "       01  PP USAGE PROCEDURE-POINTER.\n       01  PQ USAGE PROCEDURE-POINTER.\n       01  FP USAGE FUNCTION-POINTER.\n       01  PTR USAGE POINTER.\n";
    caller_and_subprog(caller).replacen("       PROCEDURE DIVISION.\n", &format!("{pointers}       PROCEDURE DIVISION.\n"), 1)
}

#[test]
fn a_call_through_a_pointer_set_to_entry_enters_the_entry_as_a_call_of_its_name() {
    let source = pointer_caller(&[
        line("SET PP TO ENTRY 'PAYMASTR'"),
        line("CALL PP USING REC CODE-X"),
        line("SET FP TO ENTRY PGM"),
        line("CALL FP USING REC CODE-X"),
        line("CALL FP USING REC CODE-X"),
        line("SET PQ TO ENTRY 'SUBPROG'"),
        line("CALL PQ USING REC"),
        line("IF PP = PQ DISPLAY 'SAME' ELSE DISPLAY 'DIFFERENT' END-IF"),
        line("SET PQ TO ENTRY 'PAYMASTR'"),
        line("IF PP = PQ DISPLAY 'SAME' END-IF"),
        line("SET PQ TO FP"),
        line("CALL PQ USING REC CODE-X"),
        line("SET PP TO NULL"),
        line("IF PP = NULL DISPLAY 'NULL' END-IF"),
    ]);
    let (out, err, ending) = run_unit(&source, vec![], "");
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "PAYMASTR HELLO 7 1\nPAYMASTR HELLO 7 1\nPAYMASTR HELLO 7 2\nSUBPROG HELLO 2\nDIFFERENT\nSAME\nPAYMASTR HELLO 7 3\nNULL\n");
}

#[test]
fn set_to_entry_of_a_name_no_program_has_abends_at_the_set() {
    let (out, _, ending) = run_unit(&pointer_caller(&[line("SET PP TO ENTRY 'NOSUCH'"), line("DISPLAY 'AFTER'")]), vec![], "");
    assert_eq!(out, "");
    assert_eq!(ending.unwrap_err().code, "IRONWORK");
}

#[test]
fn set_to_entry_takes_pointer_receivers_and_an_alphanumeric_entry() {
    let errors = |body: &str| compile_errors(&pointer_caller(&[line(body)]));
    assert!(errors("SET REC TO ENTRY 'SUBPROG'").contains("SET REC TO ENTRY: the receiver must be a procedure-pointer or function-pointer"));
    assert!(errors("SET PP TO ENTRY 12").contains("SET PP TO ENTRY: the entry literal must be alphanumeric"));
    assert!(errors("SET PP TO ENTRY CODE-X").contains("SET PP TO ENTRY: CODE-X must be an alphanumeric or alphabetic item"));
    assert!(errors("SET PP FP TO ENTRY FUNCTION UPPER-CASE(PGM)").contains("SET PP FP TO ENTRY: FUNCTION UPPER-CASE is an intrinsic function"));
    assert!(errors("SET PP TO REC").contains("SET PP TO: a function-pointer or procedure-pointer takes another, a pointer, ENTRY or NULL"));
    assert!(compile_errors(&pointer_caller(&[line("SET PP FP TO ENTRY PGM"), line("SET PP TO PTR")])).is_empty());
}

#[test]
fn alter_changes_where_a_paragraphs_go_to_goes() {
    let out = run(&program(
        "",
        "       01  D PIC 9 VALUE 0.\n",
        &[
            "       MAIN-LINE.\n",
            &line("PERFORM SWITCH THRU SWITCH-EXIT"),
            &line("ALTER SWITCH TO PROCEED TO SECOND-P"),
            &line("PERFORM SWITCH THRU SWITCH-EXIT"),
            &line("ALTER SWITCH TO FIRST-P"),
            &line("PERFORM SWITCH THRU SWITCH-EXIT"),
            &line("PERFORM NOT-YET THRU NOT-YET-EXIT"),
            &line("ALTER NOT-YET TO NOT-YET-EXIT"),
            &line("PERFORM NOT-YET THRU NOT-YET-EXIT"),
            &line("GO TO P1 P2 DEPENDING ON D"),
            &line("MOVE 2 TO D"),
            &line("GO TO P1 P2 DEPENDING ON D."),
            "       P1.\n",
            &line("DISPLAY 'P1'."),
            "       P2.\n",
            &line("DISPLAY 'P2'"),
            &line("STOP RUN."),
            "       SWITCH.\n",
            &line("GO TO FIRST-P."),
            "       FIRST-P.\n",
            &line("DISPLAY 'FIRST'"),
            &line("GO TO SWITCH-EXIT."),
            "       SECOND-P.\n",
            &line("DISPLAY 'SECOND'."),
            "       SWITCH-EXIT.\n",
            &line("EXIT."),
            "       NOT-YET.\n",
            &line("GO TO."),
            "       FELL-THROUGH.\n",
            &line("DISPLAY 'FELL THROUGH'."),
            "       NOT-YET-EXIT.\n",
            &line("EXIT."),
        ]
        .concat(),
    ));
    assert_eq!(out, "FIRST\nSECOND\nFIRST\nFELL THROUGH\nP2\n");
}

#[test]
fn an_altered_go_to_is_put_back_by_cancel_and_by_initial_but_kept_between_calls() {
    let sub = |initial: &str| {
        [
            &format!("       IDENTIFICATION DIVISION.\n       PROGRAM-ID. SUB{initial}.\n       PROCEDURE DIVISION.\n"),
            "       SWITCH.\n",
            &line("GO TO FIRST-P."),
            "       FIRST-P.\n",
            &line("DISPLAY 'FIRST'"),
            &line("ALTER SWITCH TO SECOND-P"),
            &line("GOBACK."),
            "       SECOND-P.\n",
            &line("DISPLAY 'SECOND'"),
            &line("GOBACK."),
            "       END PROGRAM SUB.\n",
        ]
        .concat()
    };
    let main = |initial: &str| {
        [
            "       CBL DYNAM\n       IDENTIFICATION DIVISION.\n       PROGRAM-ID. MAIN.\n       PROCEDURE DIVISION.\n",
            &line("CALL 'SUB'"),
            &line("CALL 'SUB'"),
            &line("CANCEL 'SUB'"),
            &line("CALL 'SUB'"),
            &line("GOBACK."),
            "       END PROGRAM MAIN.\n",
            &sub(initial),
        ]
        .concat()
    };
    assert_eq!(run_unit(&main(""), vec![], "").0, "FIRST\nSECOND\nFIRST\n");
    assert_eq!(run_unit(&main(" IS INITIAL"), vec![], "").0, "FIRST\nFIRST\nFIRST\n");
}

/// MAIN calls SUB twice, and SUB calls INNER, which it contains; each counts its calls in
/// WORKING-STORAGE.
fn nested_counters(card: &str, inner: &str) -> String {
    let counter = |id: &str, call: &str| {
        [
            format!("       IDENTIFICATION DIVISION.\n       PROGRAM-ID. {id}.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n"),
            "       01  N PIC 9 VALUE 0.\n       PROCEDURE DIVISION.\n".into(),
            line("ADD 1 TO N"),
            line(&format!("DISPLAY '{}' N", &id[..3])),
            call.into(),
            line("GOBACK."),
        ]
        .concat()
    };
    [
        card,
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. MAIN.\n       PROCEDURE DIVISION.\n",
        &line("CALL 'SUB'"),
        &line("CALL 'SUB'"),
        &line("GOBACK."),
        &counter("SUB", &line("CALL 'INNER'")),
        &counter(inner, ""),
        "       END PROGRAM INNER.\n       END PROGRAM SUB.\n       END PROGRAM MAIN.\n",
    ]
    .concat()
}

#[test]
fn the_initial_option_makes_every_program_of_the_source_initial_and_noinitial_leaves_is_initial_alone() {
    let run = |card: &str, inner: &str| run_unit(&nested_counters(card, inner), vec![], "").0;
    assert_eq!(run("", "INNER"), "SUB1\nINN1\nSUB2\nINN2\n");
    assert_eq!(run("       CBL INITIAL\n", "INNER"), "SUB1\nINN1\nSUB1\nINN1\n");
    assert_eq!(run("       PROCESS INITIAL,NOINITIAL\n", "INNER"), "SUB1\nINN1\nSUB2\nINN2\n");
    assert_eq!(run("       CBL NOINITIAL\n", "INNER IS INITIAL"), "SUB1\nINN1\nSUB2\nINN1\n");
}

#[test]
fn cancel_acts_only_on_a_contained_program_a_dynamic_call_entered() {
    let source = [
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. MAIN.\n       PROCEDURE DIVISION.\n",
        &line("CALL 'SUB'"),
        &line("GOBACK."),
        "       END PROGRAM MAIN.\n",
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. SUB.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n",
        "       01  WS-NAME PIC X(8) VALUE 'COUNTER'.\n       PROCEDURE DIVISION.\n",
        &line("CALL 'COUNTER'"),
        &line("CANCEL 'COUNTER'"),
        &line("CALL 'COUNTER'"),
        &line("CALL WS-NAME"),
        &line("CANCEL 'COUNTER'"),
        &line("CALL WS-NAME"),
        &line("GOBACK."),
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. COUNTER.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n",
        "       01  N PIC 9 VALUE 0.\n       PROCEDURE DIVISION.\n",
        &line("ADD 1 TO N"),
        &line("DISPLAY N"),
        &line("GOBACK."),
        "       END PROGRAM COUNTER.\n       END PROGRAM SUB.\n",
    ]
    .concat();
    assert_eq!(run_unit(&source, vec![], "").0, "1\n2\n3\n1\n");
}

/// The messages compiling `source`'s first program under `flags` gives, one to a line.
fn compiled_with(source: &str, flags: &[&str]) -> String {
    let flags: Vec<String> = flags.iter().map(|f| f.to_string()).collect();
    let messages = compile(syntax::parse(source).unwrap(), &flags).map_or_else(|errors| errors, |c| c.diagnostics);
    messages.iter().map(syntax::Error::labelled).collect::<Vec<_>>().join("\n")
}

#[test]
fn programs_of_one_compilation_share_a_name_only_under_the_flexible_scope() {
    let nested = |id: &str| format!("       IDENTIFICATION DIVISION.\n       PROGRAM-ID. {id}.\n       PROCEDURE DIVISION.\n{}", line("GOBACK."));
    let source = [nested("OUTER"), nested("LEFT"), nested("TWIN"), "       END PROGRAM TWIN.\n       END PROGRAM LEFT.\n".into(), nested("TWIN"), "       END PROGRAM TWIN.\n       END PROGRAM OUTER.\n".into()].concat();
    let message = "TWIN: two programs of OUTER have this name, and the programs of a separately compiled program each need their own (--program-scope=flexible allows it)";
    assert_eq!(compiled_with(&source, &[]), format!("IWC0295-S {message}"));
    assert_eq!(compiled_with(&source, &["--program-scope=flexible"]), "");
}

#[test]
fn a_static_call_naming_no_program_of_the_compilation_is_refused_only_under_unresolved_calls_fail() {
    let data = "       01  WS-NAME PIC X(8) VALUE 'MISSING'.\n       01  HOURS PIC S9(9) BINARY.\n       01  MINUTES PIC S9(9) BINARY.\n       01  SECONDS COMP-2.\n       01  FC PIC X(12).\n";
    let calls = ["CALL 'MISSING'", "CALL 'SUB'", "CALL 'SUBENTRY'", "CALL WS-NAME", "CALL 'CEEGMTO' USING HOURS MINUTES SECONDS FC", "GOBACK."].map(line).concat();
    let sub = format!("       IDENTIFICATION DIVISION.\n       PROGRAM-ID. SUB.\n       PROCEDURE DIVISION.\n{}{}", line("ENTRY 'SUBENTRY'."), line("GOBACK."));
    let source = |card: &str| format!("{}       END PROGRAM T.\n{sub}", program(card, data, &calls));
    assert_eq!(compiled_with(&source(""), &[]), "");
    let refused = "IWC0296-S CALL 'MISSING': no program of the compilation has this name, and --unresolved-calls=fail refuses a static CALL the binder could not resolve";
    assert_eq!(compiled_with(&source(""), &["--unresolved-calls=fail"]), refused);
    assert_eq!(compiled_with(&source("DYNAM"), &["--unresolved-calls=fail"]), "", "under DYNAM a literal is called dynamically");
}

#[test]
fn a_call_of_a_language_environment_service_reaches_a_program_of_its_name_unless_the_services_are_bound() {
    let data = "       01  HOURS PIC S9(9) BINARY VALUE 99.\n       01  MINUTES PIC S9(9) BINARY.\n       01  SECONDS COMP-2.\n       01  FC PIC X(12).\n";
    let calls = ["CALL 'CEEGMTO' USING HOURS MINUTES SECONDS FC", "IF HOURS = 99 DISPLAY 'UNSET' ELSE DISPLAY 'SET' END-IF", "GOBACK."].map(line).concat();
    let own = [
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. CEEGMTO.\n       DATA DIVISION.\n       LINKAGE SECTION.\n",
        "       01  H PIC S9(9) BINARY.\n       01  M PIC S9(9) BINARY.\n       01  S COMP-2.\n       01  F PIC X(12).\n",
        "       PROCEDURE DIVISION USING H M S F.\n",
        &line("DISPLAY 'MINE'"),
        &line("GOBACK."),
    ]
    .concat();
    let source = format!("{}       END PROGRAM T.\n{own}", program("", data, &calls));
    assert_eq!(run_with(&source, &[]).0, "MINE\nUNSET\n");
    assert_eq!(run_with(&source, &["--le-services=bind"]).0, "SET\n");
}

#[test]
fn thread_forces_noinitial_on_the_programs_it_compiles() {
    let source = two_programs(
        "",
        &[line("CALL 'SUB'"), line("CALL 'SUB'"), line("GOBACK.")].concat(),
        "SUB RECURSIVE",
        "       WORKING-STORAGE SECTION.\n       01  N PIC 9 VALUE 0.\n",
        &["       PROCEDURE DIVISION.\n".into(), line("ADD 1 TO N"), line("DISPLAY N"), line("GOBACK.")].concat(),
    )
    .replacen("PROGRAM-ID. MAIN.", "PROGRAM-ID. MAIN RECURSIVE.", 1);
    let card = format!("       CBL INITIAL,THREAD\n{source}");
    assert_eq!(compile_errors(&card), "warning: IWC0113-W INITIAL conflicts with THREAD, which IBM compiles only as NOINITIAL (see C217)");
    assert_eq!(run_unit(&card, vec![], "").0, "1\n2\n");
    assert_eq!(run_unit(&format!("       CBL INITIAL\n{source}"), vec![], "").0, "1\n1\n");
}

#[test]
fn an_independent_segment_is_entered_with_its_go_tos_as_written() {
    let segment = |name: &str, priority: u8| {
        [
            format!("       {name} SECTION {priority}.\n       {name}-START.\n"),
            line(&format!("DISPLAY 'IN {priority}'.")),
            format!("       {name}-SW.\n"),
            line(&format!("GO TO {name}-FIRST.")),
            format!("       {name}-FIRST.\n"),
            line("DISPLAY 'FIRST'"),
            line(&format!("ALTER {name}-SW TO PROCEED TO {name}-SECOND")),
            line(&format!("GO TO {name}-SW.")),
            format!("       {name}-SECOND.\n"),
            line("DISPLAY 'SECOND'."),
        ]
        .concat()
    };
    let out = run(&program(
        "",
        "",
        &[
            "       MAIN SECTION.\n".to_owned(),
            line("PERFORM FIXED"),
            line("PERFORM FIXED"),
            line("PERFORM INDEP"),
            line("PERFORM INDEP"),
            line("GOBACK."),
            segment("FIXED", 10),
            segment("INDEP", 50),
        ]
        .concat(),
    ));
    assert_eq!(out, "IN 10\nFIRST\nSECOND\nIN 10\nSECOND\nIN 50\nFIRST\nSECOND\nIN 50\nFIRST\nSECOND\n");
}

#[test]
fn the_alter_statements_rules_are_compile_errors() {
    let errors = |head: &str, body: &str| {
        compile_errors(&format!("       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T{head}.\n       PROCEDURE DIVISION.\n       P0.\n{body}       P1.\n           GO TO P2.\n       P2.\n           DISPLAY 'X'.\n       S SECTION.\n           GO TO P2.\n       P3.\n           STOP RUN.\n"))
    };
    assert_eq!(errors("", &line("ALTER P1 TO PROCEED TO P0.")), "");
    assert!(errors("", &line("ALTER P2 TO P0.")).contains("ALTER P2: the paragraph must hold one sentence, a GO TO without DEPENDING ON"));
    assert!(errors("", &line("ALTER S TO P0.")).contains("ALTER S: a section"));
    assert!(errors("", &line("ALTER P1 TO NOWHERE.")).contains("no paragraph or section named NOWHERE"));
    assert!(errors(" IS RECURSIVE", &line("ALTER P1 TO P0.")).contains("ALTER cannot be used in a RECURSIVE program"));
    assert!(errors("", &[line("GO TO."), line("DISPLAY 'X'.")].concat()).contains("a GO TO with no procedure-name must be its paragraph's only sentence"));
    assert!(errors("", &line("IF 1 = 1 GO TO. ")).contains("only sentence"));
}

#[test]
fn perform_varying_after_augments_the_outer_variable_before_setting_the_inner() {
    let out = run(&program(
        "",
        "       01  I PIC 9(2).\n       01  J PIC 9(2).\n       01  K PIC 9(2).\n       01  N PIC 9(4) VALUE 0.\n",
        &[
            "       MAIN-LINE.\n",
            &line("PERFORM SHOW VARYING I FROM 1 BY 1 UNTIL I > 3"),
            &line("    AFTER J FROM I BY 1 UNTIL J > 3"),
            &line("DISPLAY 'END ' I ' ' J"),
            &line("PERFORM SHOW WITH TEST AFTER VARYING I FROM 1 BY 1"),
            &line("    UNTIL I > 2 AFTER J FROM I BY 1 UNTIL J > 2"),
            &line("DISPLAY 'END ' I ' ' J"),
            &line("PERFORM COUNT-IT VARYING I FROM 1 BY 1 UNTIL I > 2"),
            &line("    AFTER J FROM 1 BY 1 UNTIL J > 3"),
            &line("    AFTER K FROM 1 BY 1 UNTIL K > 4"),
            &line("DISPLAY 'N ' N ' ' I ' ' J ' ' K"),
            &line("PERFORM SHOW VARYING I FROM 5 BY 1 UNTIL I > 4"),
            &line("    AFTER J FROM 1 BY 1 UNTIL J > 1"),
            &line("DISPLAY 'NONE ' I ' ' J"),
            &line("GOBACK."),
            "       SHOW.\n",
            &line("DISPLAY I J."),
            "       COUNT-IT.\n",
            &line("ADD 1 TO N."),
        ]
        .concat(),
    ));
    let expected = [
        "0101\n0102\n0103\n0202\n0203\n0303\nEND 04 04\n",
        "0101\n0102\n0103\n0202\n0203\n0303\nEND 03 03\n",
        "N 0024 03 01 01\n",
        "NONE 05 01\n",
    ];
    assert_eq!(out, expected.concat());
}

#[test]
fn after_phrases_are_refused_where_enterprise_cobol_refuses_them() {
    let inline = program("", "       01  I PIC 9.\n       01  J PIC 9.\n", &[line("PERFORM VARYING I FROM 1 BY 1 UNTIL I > 2"), line("    AFTER J FROM 1 BY 1 UNTIL J > 2"), line("  CONTINUE"), line("END-PERFORM.")].concat());
    let message = syntax::parse(&inline).unwrap_err().message;
    assert!(message.contains("an inline PERFORM cannot have AFTER phrases") && !message.contains("not supported"), "{message}");
    let after = (0..7).map(|_| line("    AFTER J FROM 1 BY 1 UNTIL J > 2")).collect::<String>();
    let seven = program("", "       01  I PIC 9.\n       01  J PIC 9.\n", &[line("PERFORM P VARYING I FROM 1 BY 1 UNTIL I > 2"), after, line("GOBACK."), "       P.\n".into(), line("CONTINUE.")].concat());
    assert!(syntax::parse(&seven).unwrap_err().message.contains("at most six AFTER phrases"));
}

#[test]
fn display_takes_no_advancing_with_or_without_with() {
    let out = run(&program(
        "",
        "       01  A PIC X(3) VALUE 'ABC'.\n",
        &[line("DISPLAY A NO ADVANCING"), line("DISPLAY '-' UPON CONSOLE NO ADVANCING"), line("DISPLAY A WITH NO ADVANCING"), line("DISPLAY '!'"), line("GOBACK.")].concat(),
    ));
    assert_eq!(out, "ABC-ABC!\n");
    let reversed = program("", "", &[line("DISPLAY 'A' WITH NO ADVANCING UPON CONSOLE"), line("GOBACK.")].concat());
    assert!(syntax::parse(&reversed).unwrap_err().message.contains("Enterprise COBOL takes UPON before WITH NO ADVANCING"));
    // Enterprise COBOL's DISPLAY has no scope terminator, and END-DISPLAY is not a word it reserves (Language Reference SC27-8713-03, pp. 333, 766).
    let ended = program("", "", &[line("DISPLAY 'A' WITH NO ADVANCING END-DISPLAY"), line("GOBACK.")].concat());
    let refused = compile(syntax::parse(&ended).unwrap(), &[]).err().unwrap();
    assert_eq!(refused.iter().map(|e| (e.id, e.severity)).collect::<Vec<_>>(), [(Some(syntax::messages::IWS0097.id), Severity::Severe)]);
}

#[test]
fn function_random_repeats_its_sequence_for_a_seed_between_zero_and_one() {
    let body = [
        line("COMPUTE N = FUNCTION RANDOM * 1000000000"),
        line("DISPLAY N"),
        line("COMPUTE N = FUNCTION RANDOM * 1000000000"),
        line("DISPLAY N"),
        line("COMPUTE N = FUNCTION RANDOM(0) * 1000000000"),
        line("DISPLAY N"),
        line("COMPUTE N = FUNCTION RANDOM(42) * 1000000000"),
        line("DISPLAY N"),
        line("COMPUTE X = FUNCTION RANDOM"),
        line("COMPUTE N = X * 1000000000"),
        line("DISPLAY N"),
        line("MOVE 'A' TO T(FUNCTION RANDOM * 3 + 1)"),
        line("IF FUNCTION RANDOM(2147483645) > 0"),
        line("    AND FUNCTION RANDOM < 1 DISPLAY 'IN RANGE' END-IF"),
        line("GOBACK."),
    ]
    .concat();
    let data = "       01  X COMP-2.\n       01  N PIC 9(9).\n       01  G.\n           05 T PIC X OCCURS 3.\n";
    assert_eq!(run(&program("", data, &body)), "000007826\n131537788\n000007826\n000336534\n656124890\nIN RANGE\n");
    let (_, _, ending) = run_with(&program("", data, &[line("COMPUTE X = FUNCTION RANDOM(-1)"), line("GOBACK.")].concat()), &[]);
    assert!(ending.unwrap_err().message.contains("FUNCTION RANDOM(-1): the seed must be zero or a positive integer"));
}

#[test]
fn a_zero_divisor_outside_on_size_error_is_the_program_check_of_its_divide() {
    let check = |usage: &str, statement: &str| {
        let data = format!("       01  Z {usage} VALUE 0.\n       01  Y PIC 9.\n       01  G.\n           05 T PIC X OCCURS 3.\n");
        let (out, _, ending) = run_with(&program("", &data, &[line("COMPUTE Y = 5 / Z ON SIZE ERROR DISPLAY 'SIZE' END-COMPUTE"), line(statement), line("GOBACK.")].concat()), &[]);
        (out, ending.unwrap_err().code.to_string())
    };
    let size = "SIZE\n".to_owned();
    assert_eq!(check("PIC 9", "IF 1 / Z = 0 CONTINUE END-IF"), (size.clone(), "S0CB".into()));
    assert_eq!(check("PIC S9(3) COMP-3", "COMPUTE Y = 1 / Z"), (size.clone(), "S0CB".into()));
    assert_eq!(check("PIC S9(4) COMP", "IF 1 / Z = 0 CONTINUE END-IF"), (size.clone(), "S0C9".into()));
    assert_eq!(check("PIC S9(4) COMP", "MOVE 'A' TO T(3 / Z)"), (size.clone(), "S0C9".into()));
    assert_eq!(check("PIC S9(4) COMP", "IF 1.5 / Z = 0 CONTINUE END-IF"), (size.clone(), "S0CB".into()));
    assert_eq!(check("PIC S9(2)V9 COMP", "DIVIDE Z INTO Y"), (size.clone(), "S0CB".into()));
    assert_eq!(check("COMP-2", "IF 1 / Z = 0 CONTINUE END-IF"), (size, "S0CF".into()));
}

#[test]
fn what_is_not_enterprise_cobol_is_refused_as_such() {
    let refused = |body: &str| syntax::parse(&program("", "       01  A PIC X(4).\n", &line(body))).unwrap_err().message;
    for (body, named) in [
        ("IF A <> 'B' CONTINUE END-IF.", "<> is not an Enterprise COBOL relational operator"),
        ("MOVE 'A' & 'B' TO A.", "literal concatenation with & is not Enterprise COBOL's"),
        ("SET ENVIRONMENT 'X' TO 'Y'.", "SET ENVIRONMENT is GnuCOBOL's"),
        ("ACCEPT A FROM ENVIRONMENT 'X'.", "ACCEPT ... FROM ENVIRONMENT is GnuCOBOL's"),
        ("ACCEPT A FROM ENVIRONMENT-VALUE.", "ACCEPT ... FROM ENVIRONMENT-VALUE: GnuCOBOL's, not Enterprise COBOL's"),
        ("ACCEPT A FROM ESCAPE KEY.", "ACCEPT ... FROM ESCAPE: GnuCOBOL's, not Enterprise COBOL's"),
        ("ACCEPT A FROM KEYBOARD.", "ACCEPT ... FROM KEYBOARD: neither an environment-name ACCEPT reads, SYSIN, SYSIPT or CONSOLE, nor a mnemonic-name for one"),
    ] {
        let message = refused(body);
        assert!(message.contains(named) && !message.contains("not supported"), "{body}: {message}");
    }
}

/// Language Reference SC27-8713-03, pp. 126, 307: ACCEPT reads SYSIN, SYSIPT and CONSOLE, by
/// environment-name or by a mnemonic-name for one, and nothing else.
#[test]
fn accept_reads_the_input_devices_ibm_names_and_a_mnemonic_name_for_one() {
    let with_names = |body: &str| {
        let special = "       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n       SPECIAL-NAMES.\n           SYSIN IS CARDS CONSOLE IS OPERATOR SYSOUT IS PRINTER.\n";
        program("", "       01  A PIC X(4).\n", &[line(body), line("DISPLAY A"), line("GOBACK.")].concat()).replace("       DATA DIVISION.\n", &format!("{special}       DATA DIVISION.\n"))
    };
    for from in ["", " FROM SYSIN", " FROM SYSIPT", " FROM CONSOLE", " FROM CARDS", " FROM OPERATOR"] {
        let out = Harness::source(&with_names(&format!("ACCEPT A{from}"))).sysin("ABCD\n").run(Executor::Interpreter);
        assert_eq!(out.out, "ABCD\n", "ACCEPT A{from}: {}", out.err);
    }
    let refused = syntax::parse(&with_names("ACCEPT A FROM PRINTER")).unwrap_err().message;
    assert!(refused.contains("ACCEPT ... FROM PRINTER: a mnemonic-name for SYSOUT, which ACCEPT does not read"), "{refused}");
}

fn perform_program(procedure: &[&str]) -> String {
    program("", "       01  K PIC 9 VALUE 0.\n", &procedure.concat())
}

#[test]
fn a_perform_range_returns_when_control_reaches_its_end_by_go_to() {
    let out = run(&perform_program(&[
        "       M.\n",
        &line("PERFORM B THRU C"),
        &line("DISPLAY 'BACK'"),
        &line("PERFORM D THRU A"),
        &line("DISPLAY 'BACK AGAIN'"),
        &line("STOP RUN."),
        "       A.\n",
        &line("DISPLAY 'A'."),
        "       B.\n",
        &line("DISPLAY 'B'"),
        &line("GO TO X."),
        "       C.\n",
        &line("DISPLAY 'C'."),
        "       D.\n",
        &line("DISPLAY 'D'"),
        &line("GO TO A."),
        "       X.\n",
        &line("DISPLAY 'X'"),
        &line("GO TO C."),
    ]));
    assert_eq!(out, "B\nX\nC\nBACK\nD\nA\nBACK AGAIN\n");
}

#[test]
fn a_perform_left_by_go_to_returns_when_control_later_passes_its_end() {
    let out = run(&perform_program(&[
        "       M.\n",
        &line("PERFORM P2"),
        &line("DISPLAY 'BACK ' K"),
        &line("GO TO P2."),
        "       P1.\n",
        &line("DISPLAY 'P1'."),
        "       P2.\n",
        &line("ADD 1 TO K"),
        &line("IF K = 1"),
        &line("    GO TO P1"),
        &line("END-IF"),
        &line("DISPLAY 'P2 ' K."),
        "       P3.\n",
        &line("DISPLAY 'P3'"),
        &line("STOP RUN."),
    ]));
    assert_eq!(out, "P1\nP2 2\nBACK 2\nP2 3\nP3\n");
}

#[test]
fn passing_the_end_of_an_active_perform_returns_to_it_from_a_range_inside_it() {
    let out = run(&perform_program(&[
        "       M.\n",
        &line("PERFORM A THRU B"),
        &line("DISPLAY 'M'"),
        &line("PERFORM A THRU C"),
        &line("DISPLAY 'M2'"),
        &line("STOP RUN."),
        "       A.\n",
        &line("DISPLAY 'A'"),
        &line("PERFORM B THRU C"),
        &line("DISPLAY 'A2'."),
        "       B.\n",
        &line("DISPLAY 'B'."),
        "       C.\n",
        &line("DISPLAY 'C'."),
    ]));
    assert_eq!(out, "A\nB\nM\nA\nB\nC\nA2\nB\nC\nM2\n");
}

#[test]
fn exit_section_in_a_performed_paragraph_goes_past_its_return_to_the_end_of_the_section() {
    let out = run(&perform_program(&[
        "       MAIN SECTION.\n",
        "       M.\n",
        &line("PERFORM S1-A"),
        &line("DISPLAY 'BACK'"),
        &line("STOP RUN."),
        "       S1 SECTION.\n",
        "       S1-A.\n",
        &line("DISPLAY 'A'"),
        &line("EXIT SECTION."),
        "       S1-B.\n",
        &line("DISPLAY 'B'."),
        "       S2 SECTION.\n",
        "       S2-A.\n",
        &line("DISPLAY 'S2'"),
        &line("STOP RUN."),
    ]));
    assert_eq!(out, "A\nS2\n");
}

#[test]
fn passing_the_end_of_a_repeated_perform_left_by_go_to_is_refused() {
    let (out, _, ending) = run_with(
        &perform_program(&[
            "       M.\n",
            &line("PERFORM P2 2 TIMES"),
            &line("STOP RUN."),
            "       P1.\n",
            &line("DISPLAY 'P1'."),
            "       P2.\n",
            &line("ADD 1 TO K"),
            &line("IF K = 1"),
            &line("    GO TO P1"),
            &line("END-IF."),
        ]),
        &[],
    );
    assert_eq!(out, "P1\n");
    assert!(ending.unwrap_err().message.starts_with("control passed the end of P2, which is armed to return to a PERFORM that control left by GO TO"));
}

#[test]
fn numeric_functions_take_floating_point_arguments() {
    let source = |statement: &str| {
        program(
            "",
            "       01  F COMP-2.\n       01  R PIC -9(4).99.\n",
            &[line("MOVE -2.5 TO F"), line(statement), line("DISPLAY R"), line("GOBACK.")].concat(),
        )
    };
    for (statement, shown) in [
        ("COMPUTE R = FUNCTION INTEGER(F)", "-0003.00"),
        ("COMPUTE R = FUNCTION INTEGER-PART(F)", "-0002.00"),
        ("COMPUTE R = FUNCTION INTEGER(100 * F)", "-0250.00"),
        ("COMPUTE R = FUNCTION ABS(F) + 1", " 0003.50"),
        ("COMPUTE R = FUNCTION REM(F * 3, 2)", "-0001.50"),
        ("COMPUTE R = FUNCTION MAX(1, F, 0.25)", " 0001.00"),
        ("COMPUTE R = FUNCTION MIN(1, F, 0.25)", "-0002.50"),
    ] {
        assert_eq!(run(&source(statement)), format!("{shown}\n"), "{statement}");
    }
    let (_, _, ending) = run_with(&source("COMPUTE R = FUNCTION MOD(F, 2)"), &[]);
    assert_eq!(ending.unwrap_err().message, "FUNCTION MOD needs integer arguments, and a floating-point argument is not one");
}

#[test]
fn a_paragraph_name_in_two_sections_is_the_referencing_sections_own() {
    let sections = |third: &str| {
        program(
            "",
            "       01  N PIC 9 VALUE 0.\n",
            &[
                "       FIRST-S SECTION.\n       P0.\n",
                &line("PERFORM P1"),
                &line("GO TO P1."),
                "       P1.\n",
                &line("DISPLAY 'FIRST'."),
                "       SECOND-S SECTION.\n       Q0.\n",
                &line("PERFORM P1"),
                &line("GO TO THIRD-S."),
                "       P1.\n",
                &line("DISPLAY 'SECOND'."),
                "       THIRD-S SECTION.\n       R0.\n",
                &line(third),
                &line("GOBACK."),
            ]
            .concat(),
        )
    };
    let out = run(&sections("CONTINUE."));
    assert_eq!(out, "FIRST\nFIRST\nSECOND\n");
    assert!(compile_errors(&sections("PERFORM P1.")).contains("P1 names more than one paragraph"));
}

#[test]
fn abbreviated_relations_keep_the_subject_the_operator_and_its_not() {
    let data = "       01  S PIC XX VALUE '61'.\n       01  D PIC 999 VALUE 15.\n       01  P PIC 9V999 VALUE .2.\n       01  B PIC 999 VALUE 20.\n";
    let out = run(&program(
        "",
        data,
        &[
            line("IF S = ('02' OR '03' OR '61') DISPLAY 'LIST'."),
            line("IF S = ('02' OR NOT '61') DISPLAY 'NO'."),
            line("IF D < 1 OR = 14 OR = 15 DISPLAY 'OR ='."),
            line("IF (P >= .15 AND <= .202) DISPLAY 'RANGE'."),
            line("IF D > 10 AND NOT > 20 DISPLAY 'NOT >'."),
            line("IF D NOT = 10 AND 15 DISPLAY 'NO'."),
            line("IF D NOT = 10 AND 16 DISPLAY 'NOT = LITERAL'."),
            line("IF D NOT < 10 AND B DISPLAY 'NO'."),
            line("IF D NOT > 10 OR B DISPLAY 'NOT > ITEM'."),
            line("GOBACK."),
        ]
        .concat(),
    ));
    assert_eq!(out, "LIST\nOR =\nRANGE\nNOT >\nNOT = LITERAL\nNOT > ITEM\n");
}

#[test]
fn an_evaluate_subject_that_is_a_condition_name_takes_truth_values() {
    let data = "       01  X PIC 9 VALUE 1.\n           88  X-ONE VALUE 1.\n           88  X-TWO VALUE 2.\n";
    let out = run(&program(
        "",
        data,
        &[
            line("EVALUATE X-ONE ALSO X WHEN TRUE ALSO 1 DISPLAY 'A'"),
            line("END-EVALUATE"),
            line("EVALUATE X-TWO WHEN FALSE DISPLAY 'B' END-EVALUATE"),
            line("EVALUATE X-ONE WHEN X-TWO DISPLAY 'NO'"),
            line("    WHEN NOT X-TWO DISPLAY 'C' END-EVALUATE"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    assert_eq!(out, "A\nB\nC\n");
}

#[test]
fn an_abbreviated_relation_may_write_is_before_its_operator() {
    let body = |a: u8| {
        [
            line(&format!("MOVE {a} TO A")),
            line("IF A GREATER THAN B"),
            line("    AND IS NOT LESS THAN C OR D"),
            line("    DISPLAY 'TRUE' ELSE DISPLAY 'FALSE' END-IF"),
        ]
        .concat()
    };
    let data = "       01  A PIC 9.\n       01  B PIC 9 VALUE 1.\n       01  C PIC 9 VALUE 3.\n       01  D PIC 9 VALUE 9.\n";
    let source = program("", data, &[body(5), body(2), line("GOBACK.")].concat());
    assert_eq!(run(&source), "TRUE\nFALSE\n");
}

#[test]
fn an_abbreviated_object_may_be_qualified_subscripted_negated_or_in_parentheses() {
    let data = "       01  A PIC 9 VALUE 5.\n       01  B PIC 9 VALUE 1.\n       01  C PIC 9 VALUE 2.\n       01  D PIC 9 VALUE 5.\n       01  G.\n           05 E PIC 9 VALUE 5.\n           05 TE PIC 9 OCCURS 3 VALUE 5.\n";
    let check = |condition: &str| [line(&format!("IF {condition}")), line("    DISPLAY 'T' ELSE DISPLAY 'F' END-IF")].concat();
    let body = [
        check("NOT (A NOT = B AND C AND NOT D)"),
        check("A = B OR E OF G"),
        check("A = B OR TE (2)"),
        check("A = B OR NOT D"),
        check("A = 5 AND (C OR D)"),
        check("A > B AND NOT (C OR D)"),
        check("A = B OR (> C AND < 9)"),
        line("GOBACK."),
    ]
    .concat();
    assert_eq!(run(&program("", data, &body)), "F\nT\nT\nF\nT\nF\nT\n");
    let distributed = program("", data, &[check("A NOT = (NOT B OR C)"), line("GOBACK.")].concat());
    let refused = syntax::parse(&distributed).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(refused.contains("NOT cannot follow the left parenthesis that distributes a relational operator"), "{refused}");
}

#[test]
fn an_initial_program_resets_the_programs_it_contains_and_closes_its_files_when_it_returns() {
    let out = temp("initial-out.txt");
    let source = [
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. MAIN.\n       PROCEDURE DIVISION.\n",
        &line("CALL 'SUB'"),
        &line("CALL 'SUB'"),
        &line("GOBACK."),
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. SUB IS INITIAL.\n       ENVIRONMENT DIVISION.\n       INPUT-OUTPUT SECTION.\n       FILE-CONTROL.\n",
        "           SELECT F ASSIGN TO OUTDD ORGANIZATION LINE SEQUENTIAL\n               FILE STATUS IS FS.\n",
        "       DATA DIVISION.\n       FILE SECTION.\n       FD  F.\n       01  R PIC X(3).\n       WORKING-STORAGE SECTION.\n       01  FS PIC XX.\n       PROCEDURE DIVISION.\n",
        &line("OPEN OUTPUT F"),
        &line("DISPLAY 'OPEN ' FS"),
        &line("CALL 'INNER'"),
        &line("GOBACK."),
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. INNER.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  N PIC 9 VALUE 0.\n       PROCEDURE DIVISION.\n",
        &line("ADD 1 TO N"),
        &line("DISPLAY 'INN' N"),
        &line("GOBACK."),
        "       END PROGRAM INNER.\n       END PROGRAM SUB.\n       END PROGRAM MAIN.\n",
    ]
    .concat();
    let (stdout, err, ending) = run_files(&source, &[format!("OUTDD={}", out.display())]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(stdout, "OPEN 00\nINN1\nOPEN 00\nINN1\n");
    let _ = std::fs::remove_file(out);
}

#[test]
fn paragraphs_and_sections_named_by_digits_alone_are_procedure_names() {
    let out = run(&perform_program(&[
        "       M SECTION.\n",
        "       M1.\n",
        &line("MOVE 2 TO K"),
        &line("GO TO 3 4 5 DEPENDING ON K."),
        "       3.\n",
        &line("DISPLAY '3'."),
        "       4.\n",
        &line("DISPLAY '4'"),
        &line("PERFORM 3 TIMES DISPLAY 'T' NO ADVANCING END-PERFORM"),
        &line("PERFORM 01 2 TIMES"),
        &line("PERFORM 0002"),
        &line("ALTER 9 TO PROCEED TO 8"),
        &line("GO TO 9."),
        "       5.\n",
        &line("DISPLAY '5'."),
        "       9.\n",
        &line("GO TO 5."),
        "       8.\n",
        &line("DISPLAY '8'"),
        &line("STOP RUN."),
        "       01 SECTION.\n",
        "       1.\n",
        &line("DISPLAY '1 OF 01'."),
        "       0002 SECTION.\n",
        "       1.\n",
        &line("DISPLAY '1 OF 0002'."),
        "       2.\n",
        &line("DISPLAY '2 OF 0002'."),
    ]));
    assert_eq!(out, "4\nTTT1 OF 01\n1 OF 01\n1 OF 0002\n2 OF 0002\n8\n");
}

#[test]
fn perform_times_takes_a_subscripted_count_and_test_needs_no_with() {
    let out = run(&program(
        "",
        "       01  T.\n           05 N PIC 9 OCCURS 3 VALUE 2.\n       01  I PIC 9 VALUE 3.\n       01  K PIC 9 VALUE 0.\n",
        &[
            "       M.\n",
            &line("MOVE 1 TO N (2)"),
            &line("PERFORM P N (I) TIMES"),
            &line("PERFORM N (2) TIMES DISPLAY 'I' END-PERFORM"),
            &line("PERFORM P TEST AFTER UNTIL K > 4"),
            &line("PERFORM TEST BEFORE UNTIL K > 5 ADD 1 TO K END-PERFORM"),
            &line("DISPLAY K"),
            &line("STOP RUN."),
            "       P.\n",
            &line("ADD 1 TO K"),
            &line("DISPLAY 'P' K."),
        ]
        .concat(),
    ));
    assert_eq!(out, "P1\nP2\nI\nP3\nP4\nP5\n6\n");
}

#[test]
fn an_unqualified_paragraph_name_names_the_one_in_its_own_section() {
    let source = perform_program(&[
        "       S1 SECTION.\n       S1-START.\n",
        &line("PERFORM P"),
        &line("PERFORM P THRU Q"),
        &line("PERFORM S2"),
        &line("GO TO Q."),
        "       P.\n",
        &line("DISPLAY 'P OF S1'."),
        "       Q.\n",
        &line("DISPLAY 'Q OF S1'."),
        "       Z.\n",
        &line("STOP RUN."),
        "       S2 SECTION.\n       S2-START.\n",
        &line("MOVE 1 TO K"),
        &line("GO TO P Q DEPENDING ON K."),
        "       P.\n",
        &line("DISPLAY 'P OF S2'"),
        &line("ALTER R TO PROCEED TO Q"),
        &line("PERFORM R THRU Q."),
        "       R.\n",
        &line("GO TO P."),
        "       Q.\n",
        &line("DISPLAY 'Q OF S2'."),
    ]);
    assert_eq!(run(&source), "P OF S1\nP OF S1\nQ OF S1\nP OF S2\nQ OF S2\nQ OF S2\nQ OF S1\n");
    let elsewhere = format!("{source}       S3 SECTION.\n{}", line("GO TO P."));
    assert_eq!(compile_errors(&elsewhere), "IWC0004-S P names more than one paragraph; qualify it with OF and its section");
}

#[test]
fn alphabetic_lower_and_upper_test_each_character_and_allow_spaces() {
    let data = "       01  U PIC X(4) VALUE 'AB C'.\n       01  L PIC X(4) VALUE 'ab c'.\n       01  M PIC X(4) VALUE 'Ab c'.\n";
    let test = |item: &str| {
        format!("IF {item} IS ALPHABETIC-UPPER DISPLAY '{item} U' END-IF\n           IF {item} NOT ALPHABETIC-LOWER DISPLAY '{item} NOT L' END-IF")
    };
    let out = run(&program("", data, &[line(&test("U")), line(&test("L")), line(&test("M")), line("IF U(2:2) ALPHABETIC-UPPER AND L(1:1) ALPHABETIC-LOWER"), line("    DISPLAY 'BOTH' END-IF"), line("GOBACK.")].concat()));
    assert_eq!(out, "U U\nU NOT L\nM NOT L\nBOTH\n");
}

#[test]
fn procedure_division_returning_names_an_01_or_77_item_of_the_linkage_section() {
    let with = |head: &str| {
        severe(&format!(
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. SUB.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  R PIC X(3) VALUE 'XYZ'.\n       LINKAGE SECTION.\n       01  L.\n           05 L1 PIC X(3).\n       PROCEDURE DIVISION{head}.\n{}",
            line("GOBACK.")
        ))
    };
    assert_eq!(with(" RETURNING R"), ["PROCEDURE DIVISION RETURNING R: not an 01 or 77 item of the LINKAGE SECTION"]);
    assert_eq!(with(" RETURNING L1"), ["PROCEDURE DIVISION RETURNING L1: not an 01 or 77 item of the LINKAGE SECTION"]);
    assert_eq!(with(" RETURNING L"), Vec::<String>::new());
}

#[test]
fn perform_varying_steps_a_numeric_item_from_and_by_an_identifier_or_literal() {
    let data = "       01  I PIC 99.\n       01  J PIC 99.\n       01  X PIC X(4).\n       01  F COMP-2.\n       01  G.\n           05 T PIC X OCCURS 3 INDEXED BY K.\n";
    let refused = |body: &str| severe(&program("", data, &[line(body), line("    CONTINUE"), line("END-PERFORM"), line("GOBACK.")].concat()));
    assert_eq!(refused("PERFORM VARYING J FROM I + 1 BY 1 UNTIL J > 3"), ["PERFORM VARYING J FROM: an arithmetic expression, where FROM takes an identifier, index-name or literal"]);
    assert_eq!(refused("PERFORM VARYING J FROM 1 BY I * 2 UNTIL J > 3"), ["PERFORM VARYING J BY: an arithmetic expression, where BY takes an identifier, index-name or literal"]);
    assert_eq!(refused("PERFORM VARYING X FROM 1 BY 1 UNTIL X > 3"), ["PERFORM VARYING X: not a numeric elementary item or an index-name"]);
    for accepted in ["PERFORM VARYING I FROM 10 BY -1 UNTIL I < 8", "PERFORM VARYING I FROM FUNCTION LENGTH(X) BY 1 UNTIL I > 5", "PERFORM VARYING F FROM 1.5 BY 0.5 UNTIL F > 3", "PERFORM VARYING K FROM 1 BY 1 UNTIL K > 3"] {
        assert_eq!(refused(accepted), Vec::<String>::new(), "{accepted}");
    }
}

#[test]
fn search_varying_names_an_index_or_an_elementary_integer_item() {
    let data = "       01  G.\n           05 E PIC X OCCURS 3 INDEXED BY K M.\n       01  V PIC 9(2)V9.\n       01  A PIC X.\n       01  F COMP-2.\n       01  IX USAGE INDEX.\n       01  B PIC S9(4) COMP.\n";
    let refused = |v: &str| severe(&program("", data, &[line(&format!("SEARCH E VARYING {v} WHEN E(K) = 'A' CONTINUE END-SEARCH")), line("GOBACK.")].concat()));
    for (v, refusal) in [("V", "SEARCH E VARYING V"), ("A", "SEARCH E VARYING A"), ("F", "SEARCH E VARYING F")] {
        assert_eq!(refused(v), [format!("{refusal}: not an index-name, an index data item or an elementary integer item")]);
    }
    for accepted in ["M", "IX", "B"] {
        assert_eq!(refused(accepted), Vec::<String>::new(), "{accepted}");
    }
}

#[test]
fn a_handle_label_names_a_paragraph_or_section_of_the_program() {
    let refused = |block: &str| severe(&program("", "", &["       MAIN.\n".to_owned(), line(block), line("GOBACK."), "       ERR-1.\n".to_owned(), line("GOBACK.")].concat()));
    assert_eq!(refused("EXEC CICS HANDLE CONDITION ERROR(NOPARA) END-EXEC"), ["EXEC CICS HANDLE CONDITION ERROR(NOPARA): no paragraph or section named NOPARA"]);
    assert_eq!(refused("EXEC CICS HANDLE AID PF3(NOPARA) END-EXEC"), ["EXEC CICS HANDLE AID PF3(NOPARA): no paragraph or section named NOPARA"]);
    assert_eq!(refused("EXEC CICS HANDLE ABEND LABEL(NOPARA) END-EXEC"), ["EXEC CICS HANDLE ABEND LABEL(NOPARA): no paragraph or section named NOPARA"]);
    for accepted in ["EXEC CICS HANDLE CONDITION ERROR(ERR-1) LENGERR END-EXEC", "EXEC CICS HANDLE AID PF3(err-1) END-EXEC", "EXEC CICS HANDLE ABEND PROGRAM('X') END-EXEC"] {
        assert_eq!(refused(accepted), Vec::<String>::new(), "{accepted}");
    }
}
