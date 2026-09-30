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
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. MAIN.\n       PROCEDURE DIVISION.\n",
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
        &[line("DISPLAY A NO ADVANCING"), line("DISPLAY '-' UPON CONSOLE NO ADVANCING"), line("DISPLAY A WITH NO ADVANCING END-DISPLAY"), line("DISPLAY '!'"), line("GOBACK.")].concat(),
    ));
    assert_eq!(out, "ABC-ABC!\n");
    let reversed = program("", "", &[line("DISPLAY 'A' WITH NO ADVANCING UPON CONSOLE"), line("GOBACK.")].concat());
    assert!(syntax::parse(&reversed).unwrap_err().message.contains("Enterprise COBOL takes UPON before WITH NO ADVANCING"));
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
        line("MOVE FUNCTION RANDOM TO X"),
        line("COMPUTE N = X * 1000000000"),
        line("DISPLAY N"),
        line("MOVE 'A' TO T(FUNCTION RANDOM * 3 + 1)"),
        line("IF FUNCTION RANDOM(2147483645) > 0"),
        line("    AND FUNCTION RANDOM < 1 DISPLAY 'IN RANGE' END-IF"),
        line("GOBACK."),
    ]
    .concat();
    let data = "       01  X COMP-2.\n       01  N PIC 9(9).\n       01  G.\n           05 T PIC X OCCURS 3.\n";
    assert_eq!(run(&program("", data, &body)), "000007826\n131537788\n000007826\n000336533\n656124890\nIN RANGE\n");
    let (_, _, ending) = run_with(&program("", data, &[line("MOVE FUNCTION RANDOM(-1) TO X"), line("GOBACK.")].concat()), &[]);
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
    ] {
        let message = refused(body);
        assert!(message.contains(named) && !message.contains("not supported"), "{body}: {message}");
    }
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
