use super::*;

fn compiled(options: &str, data: &str) -> Compiled {
    let parsed = syntax::parse(&program(options, data, &line("GOBACK."))).unwrap_or_else(|e| panic!("{e}"));
    compile(parsed, &[]).unwrap_or_else(|e| panic!("{e:?}"))
}

/// Each named item's offset from the start of its level-01 record, and its length.
fn placed(c: &Compiled, names: &[&str]) -> Vec<(u32, u32)> {
    let items = &c.layout.items;
    let root = |mut i: usize| {
        while let Some(p) = items[i].parent {
            i = p;
        }
        i
    };
    names
        .iter()
        .map(|n| {
            let i = items.iter().position(|it| it.name.as_deref() == Some(*n)).unwrap_or_else(|| panic!("{n}"));
            (items[i].offset - items[root(i)].offset, items[i].size)
        })
        .collect()
}

#[test]
fn synchronized_items_follow_the_language_references_worked_examples() {
    let c = compiled(
        "",
        concat!(
            "       01  FIELD-A.\n           05 FIELD-B PIC X(5).\n           05 FIELD-C.\n",
            "              10 FIELD-D PIC XX.\n              10 FIELD-E PIC S9(6) COMP SYNC.\n",
            "       01  FIELD-L.\n           05 FIELD-M PIC X(5).\n           05 FIELD-N PIC XX.\n",
            "           05 FIELD-O.\n              10 FIELD-P PIC S9(6) COMP SYNC.\n",
            "       01  WORK-RECORD.\n           05 WORK-CODE PIC X.\n           05 COMP-TABLE OCCURS 10 TIMES.\n",
            "              10 COMP-TYPE PIC X.\n              10 COMP-PAY PIC S9(4)V99 COMP SYNC.\n",
            "              10 COMP-HOURS PIC S9(3) COMP SYNC.\n              10 COMP-NAME PIC X(5).\n",
            "       01  COMP-RECORD.\n           05 A-1 PIC X(5).\n           05 A-2 PIC X(3).\n           05 A-3 PIC X(3).\n",
            "           05 B-1 PIC S9999 USAGE COMP SYNCHRONIZED.\n           05 B-2 PIC S99999 USAGE COMP SYNCHRONIZED.\n",
            "           05 B-3 PIC S9999 USAGE COMP SYNCHRONIZED.\n",
        ),
    );
    assert_eq!(placed(&c, &["FIELD-A", "FIELD-C", "FIELD-E"]), [(0, 12), (5, 7), (8, 4)]);
    assert_eq!(placed(&c, &["FIELD-L", "FIELD-N", "FIELD-O", "FIELD-P"]), [(0, 12), (5, 2), (8, 4), (8, 4)]);
    assert_eq!(placed(&c, &["WORK-RECORD", "COMP-TABLE", "COMP-PAY", "COMP-HOURS", "COMP-NAME"]), [(0, 161), (1, 16), (4, 4), (8, 2), (10, 5)]);
    let find = |n: &str| c.layout.items.iter().find(|i| i.name.as_deref() == Some(n)).unwrap();
    assert_eq!(find("COMP-PAY").dims, [(16, 10)]);
    assert_eq!(placed(&c, &["COMP-RECORD", "B-1", "B-2", "B-3"]), [(0, 22), (12, 2), (16, 4), (20, 2)]);
}

#[test]
fn a_synchronized_record_aligns_each_usage_on_its_boundary_and_slack_joins_the_group_before() {
    let c = compiled(
        "",
        concat!(
            "       01  ALIGNED SYNC.\n           05 C1 PIC X.\n           05 H1 PIC S9(4) COMP.\n           05 C2 PIC X.\n",
            "           05 F1 COMP-1.\n           05 C3 PIC X.\n           05 F2 COMP-2.\n           05 C4 PIC X.\n",
            "           05 P1 POINTER.\n           05 C5 PIC X.\n           05 I1 INDEX.\n           05 C6 PIC X.\n",
            "           05 D1 PIC S9(18) COMP.\n           05 K1 PIC S9(5) COMP-3.\n           05 Z1 PIC 9(3).\n",
            "       01  CLOSED.\n           05 G1.\n              10 G1-A PIC X.\n           05 G2.\n",
            "              10 G2-B PIC S9(9) COMP SYNC.\n",
            "       01  PLAIN.\n           05 PL-A PIC X.\n           05 PL-B PIC S9(9) COMP.\n",
        ),
    );
    let offsets: Vec<u32> = placed(&c, &["H1", "F1", "F2", "P1", "I1", "D1", "K1", "Z1"]).into_iter().map(|(o, _)| o).collect();
    assert_eq!(offsets, [2, 8, 16, 28, 36, 44, 52, 55]);
    assert_eq!(placed(&c, &["ALIGNED"]), [(0, 58)]);
    assert_eq!(placed(&c, &["CLOSED", "G1", "G2", "G2-B"]), [(0, 8), (0, 4), (4, 4), (4, 4)]);
    assert_eq!(placed(&c, &["PLAIN", "PL-B"]), [(0, 5), (1, 4)]);
}

#[test]
fn synchronized_binary_items_hold_their_values() {
    let out = run(&program(
        "",
        "       01  R.\n           05 C PIC X VALUE 'A'.\n           05 N PIC S9(5) COMP SYNC VALUE 12345.\n           05 T OCCURS 3.\n              10 T-C PIC X.\n              10 T-N PIC S9(4) COMP SYNC.\n",
        &[line("MOVE 7 TO T-N(3)"), line("ADD N TO T-N(3)"), line("DISPLAY C ' ' N ' ' T-N(3) ' ' LENGTH OF R"), line("GOBACK.")].concat(),
    ));
    assert_eq!(out, "A 12345 2352 000000020\n");
}

#[test]
fn a_redefinition_that_would_need_slack_bytes_is_refused() {
    let base = "       01  RD.\n           05 RD-A PIC X(3).\n           05 RD-B PIC X(4).\n";
    for redefinition in ["           05 RD-C REDEFINES RD-B PIC S9(9) COMP SYNC.\n", "           05 RD-D REDEFINES RD-B.\n              10 RD-E PIC S9(4) COMP SYNC.\n"] {
        let errors = compile_errors(&program("", &[base, redefinition].concat(), &line("GOBACK.")));
        assert!(errors.contains("slack bytes"), "{errors}");
    }
}

#[test]
fn scaling_positions_give_the_algebraic_value_in_arithmetic_moves_and_numeric_comparisons() {
    let out = run(&program(
        "",
        concat!(
            "       01  R99PP PIC 99PP VALUE 1200.\n       01  RPP99 PIC PP99 VALUE .0012.\n       01  RSV PIC SVPP9 VALUE -.005.\n",
            "       01  RPK PIC S999PP COMP-3 VALUE -12300.\n       01  RBN PIC 9PP COMP VALUE 300.\n       01  EDP PIC ZZZPP.\n",
            "       01  W5 PIC 9(5).\n       01  W4 PIC 9V9(4).\n       01  X4 PIC X(4).\n",
        ),
        &[
            line("DISPLAY R99PP ' ' RPP99 ' ' RSV ' ' RPK ' ' RBN"),
            line("ADD 100 TO R99PP"),
            line("MOVE R99PP TO W5"),
            line("MOVE RPP99 TO W4"),
            line("DISPLAY R99PP ' ' W5 ' ' W4"),
            line("COMPUTE W5 = RPK * -1"),
            line("MOVE 12345 TO EDP"),
            line("DISPLAY W5 ' [' EDP ']'"),
            line("MOVE EDP TO W5"),
            line("MOVE R99PP TO X4"),
            line("DISPLAY W5 ' ' X4"),
            line("IF R99PP = 1300 DISPLAY 'EQUAL' END-IF"),
            line("IF R99PP = '13' DISPLAY 'DIGITS' END-IF"),
            line("MOVE 99 TO R99PP"),
            line("DISPLAY R99PP"),
            line("ADD 150 TO R99PP ROUNDED"),
            line("DISPLAY R99PP"),
            line("ADD 9900 TO R99PP ON SIZE ERROR DISPLAY 'SIZE' END-ADD"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    assert_eq!(out, "12 12 N 12L 3\n13 01300 00012\n12300 [123]\n12300 1300\nEQUAL\nDIGITS\n00\n02\nSIZE\n");
}

const RENAMED: &str = concat!(
    "       01  R.\n           05 RA PIC XX VALUE 'AB'.\n           05 RG.\n              10 RB PIC X(3) VALUE 'CDE'.\n",
    "              10 RC PIC 99 VALUE 12.\n           05 RD PIC X VALUE 'Z'.\n",
    "       66  AB RENAMES RA THRU RB.\n       66  CC RENAMES RC.\n       66  WHOLE RENAMES RA THROUGH RD.\n       66  GR RENAMES RG OF R.\n",
    "       01  OTHER-REC.\n           05 RA PIC X.\n       01  N2 PIC 99.\n",
);

#[test]
fn renames_regroups_storage_one_item_or_a_range() {
    let out = run(&program(
        "",
        RENAMED,
        &[
            line("DISPLAY AB '|' CC '|' WHOLE '|' GR"),
            line("ADD 1 TO CC"),
            line("MOVE LENGTH OF AB TO N2"),
            line("DISPLAY RC ' ' N2"),
            line("MOVE 'XY' TO AB"),
            line("DISPLAY R ' ' CC OF R"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    assert_eq!(out, "ABCDE|12|ABCDE12Z|CDE12\n13 05\nXY   13Z 13\n");
}

#[test]
fn renames_keeps_ibms_restrictions() {
    let refused = |data: &str, procedure: &str, why: &str| {
        let errors = compile_errors(&program("", data, &[line(procedure), line("GOBACK.")].concat()));
        assert!(errors.contains(why), "{why}: {errors}");
    };
    refused("       01  R.\n           05 RA PIC X.\n       66  BAD RENAMES R.\n", "CONTINUE", "level-01");
    refused("       01  T.\n           05 TT OCCURS 2.\n              10 T1 PIC X.\n       66  BAD RENAMES T1.\n", "CONTINUE", "OCCURS");
    refused(&RENAMED.replace("RA THRU RB", "RD THRU RA"), "CONTINUE", "no earlier");
    refused(&RENAMED.replace("RA THRU RB", "RG THRU RB"), "CONTINUE", "within the first");
    refused(RENAMED, "INITIALIZE AB", "RENAMES");
    refused("       77  N PIC 9.\n       66  BAD RENAMES N.\n", "CONTINUE", "level-01 record");
    refused("       01  R.\n           05 RA PIC X.\n       66  AB RENAMES RA.\n           05 RB PIC X.\n", "CONTINUE", "after a level-66");
}

#[test]
fn qualify_extend_resolves_the_programming_guides_example_by_its_complete_sets_of_qualifiers() {
    let data = "       01  A.\n           02 B.\n              03 C PIC X.\n              03 A PIC X.\n           02 C PIC X.\n";
    let body: String = ["MOVE 'X' TO C OF B OF A", "MOVE 'Z' TO A OF B", "MOVE 'Y' TO C OF A", "DISPLAY A", "MOVE 'W' TO C OF B", "DISPLAY A", "MOVE SPACE TO A", "DISPLAY '[' A ']'", "GOBACK."].map(line).concat();
    assert_eq!(run(&program("QUALIFY(EXTEND)", data, &body)), "XZY\nWZY\n[   ]\n");
    assert_eq!(run(&program("QUA(E)", data, &body)), "XZY\nWZY\n[   ]\n");
    for card in ["", "QUALIFY(COMPAT)", "QUA(C)"] {
        let errors = compile_errors(&program(card, data, &body));
        assert_eq!((errors.matches("C is ambiguous").count(), errors.matches("A is ambiguous").count(), errors.lines().count()), (1, 4, 5), "{card}: {errors}");
    }
}

#[test]
fn a_complete_set_skips_filler_ends_a_condition_names_at_its_variable_and_resolves_renames() {
    let data = concat!(
        "       01  S.\n           05 F PIC X VALUE 'Y'.\n              88 OK VALUE 'Y'.\n           05 G.\n              10 F PIC X VALUE 'N'.\n                 88 OK VALUE 'Y'.\n",
        "           05 FILLER.\n              10 K PIC X VALUE '1'.\n           05 H.\n              10 K PIC X VALUE '2'.\n       66  R2 RENAMES K OF S.\n",
        "       77  K PIC X VALUE '3'.\n",
    );
    let body: String = ["IF OK OF F OF S DISPLAY 'OK' END-IF", "IF OK OF G DISPLAY 'NOT' END-IF", "DISPLAY K OF S", "DISPLAY K", "DISPLAY K OF H", "DISPLAY R2", "GOBACK."].map(line).concat();
    assert_eq!(run(&program("QUALIFY(EXTEND)", data, &body)), "OK\n1\n3\n2\n1\n");
    let errors = compile_errors(&program("", data, &body));
    assert!(errors.contains("RENAMES K: ambiguous"), "{errors}");
    let errors = compile_errors(&program("", &data.replace("       66  R2 RENAMES K OF S.\n", ""), &body.replace(&line("DISPLAY R2"), "")));
    assert_eq!(errors, ["OK", "K", "K"].map(|n| format!("IWC0002-S {n} is ambiguous; qualify it with OF or IN")).join("\n"));
}

#[test]
fn a_records_file_name_may_end_a_complete_set_and_need_not() {
    let source = |card: &str| {
        [
            card,
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. Q.\n       ENVIRONMENT DIVISION.\n       INPUT-OUTPUT SECTION.\n",
            "       FILE-CONTROL.\n           SELECT F1 ASSIGN TO F1DD.\n",
            "       DATA DIVISION.\n       FILE SECTION.\n",
            "       FD  F1.\n       01  FR.\n           05 K PIC X.\n       01  OTHER-REC.\n           05 FR.\n              10 K PIC X.\n",
            "       PROCEDURE DIVISION.\n           MOVE 'A' TO K OF FR OF F1\n           MOVE 'B' TO K OF FR\n           GOBACK.\n",
        ]
        .concat()
    };
    assert_eq!(compile_errors(&source("       CBL QUALIFY(EXTEND)\n")), "");
    let errors = compile_errors(&source(""));
    assert_eq!(errors.matches("K is ambiguous").count(), 2, "{errors}");
}

#[test]
fn set_to_false_stores_the_when_set_to_false_value() {
    let data = concat!(
        "       01  FLAG PIC X VALUE 'Y'.\n           88 FLAG-ON VALUE 'Y' FALSE 'N'.\n",
        "           88 FLAG-X VALUES 'A' THRU 'C' WHEN SET TO FALSE IS 'Z'.\n           88 FLAG-Q VALUE 'Q'.\n",
    );
    let out = run(&program(
        "",
        data,
        &[
            line("SET FLAG-ON TO FALSE"),
            line("DISPLAY FLAG"),
            line("SET FLAG-X TO TRUE"),
            line("DISPLAY FLAG"),
            line("SET FLAG-X TO FALSE"),
            line("DISPLAY FLAG"),
            line("IF NOT FLAG-ON AND NOT FLAG-X DISPLAY 'OFF' END-IF"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    assert_eq!(out, "N\nA\nZ\nOFF\n");
    let errors = compile_errors(&program("", data, &[line("SET FLAG-Q TO FALSE"), line("GOBACK.")].concat()));
    assert!(errors.contains("WHEN SET TO FALSE"), "{errors}");
}

#[test]
fn occurs_is_refused_at_the_levels_ibm_refuses_it() {
    for (data, level) in [("       01  T PIC X OCCURS 3.\n", "01"), ("       77  T PIC X OCCURS 3.\n", "77")] {
        let errors = compile_errors(&program("", data, &line("GOBACK.")));
        assert!(errors.contains(&format!("OCCURS at level {level}")), "{errors}");
    }
}

#[test]
fn arith_compat_holds_numeric_items_and_literals_to_18_digits_and_arith_extend_to_31() {
    let data = concat!(
        "       01  A PIC 9(19).\n       01  B PIC S9(17)V99 COMP-3.\n       01  C PIC 9(18)PP.\n",
        "       01  E PIC Z(19).\n       01  D PIC 9(18) VALUE 1234567890123456789.\n       01  F PIC S9(18) VALUE -1.\n",
    );
    let procedure = [line("MOVE 1234567890123456789 TO A"), line("GOBACK.")].concat();
    let errors = compile_errors(&program("", data, &procedure));
    assert_eq!(errors.lines().filter(|l| l.contains("ARITH(COMPAT)")).count(), 5, "{errors}");
    assert!(errors.contains("the literal 1234567890123456789 has more than 18 digits"), "{errors}");
    assert_eq!(compile_errors(&program("ARITH(EXTEND)", data, &procedure)), "");
    assert!(compile_errors(&program("ARITH(EXTEND)", "       01  G PIC 9(19) COMP.\n", &line("GOBACK."))).contains("18 digits"));
}

#[test]
fn decimal_point_is_comma_in_literals_editing_de_editing_numval_and_contained_programs() {
    let source = [
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. DPC.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n",
        "       SPECIAL-NAMES.\n           DECIMAL-POINT IS COMMA.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n",
        "       01  A PIC S9(5)V99 VALUE 1234,5.\n       01  E PIC Z.ZZ9,99-.\n       01  S PIC ***.**9,99.\n",
        "       01  Z PIC ZZZ,ZZ.\n       01  B PIC S9(5)V99.\n       01  N PIC 9(3)V99.\n",
        "       01  TG.\n           05 T PIC 9 OCCURS 3 VALUE 7.\n       01  C PIC X(12) VALUE '1.234,56'.\n",
        "       01  K PIC 9V9 VALUE ,5.\n          88 HALF VALUE 0,5.\n       PROCEDURE DIVISION.\n",
        &line("MOVE A TO E"),
        &line("DISPLAY '[' E ']'"),
        &line("MOVE -1,25 TO E"),
        &line("DISPLAY '[' E ']'"),
        &line("MOVE 12,34 TO S"),
        &line("MOVE 0 TO Z"),
        &line("DISPLAY '[' S '][' Z ']'"),
        &line("MOVE E TO B"),
        &line("DISPLAY B"),
        &line("COMPUTE N = FUNCTION NUMVAL('12,5') + FUNCTION NUMVAL-C(C)"),
        &line("DISPLAY N"),
        &line("DISPLAY 3,75 ' ' T(2)"),
        &line("IF HALF DISPLAY 'HALF' END-IF"),
        &line("CALL 'INNER'"),
        &line("GOBACK."),
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. INNER.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n",
        "       01  E2 PIC ZZ9,9.\n       PROCEDURE DIVISION.\n",
        &line("MOVE 2,5 TO E2"),
        &line("DISPLAY 'INNER [' E2 ']'"),
        &line("GOBACK."),
        "       END PROGRAM INNER.\n       END PROGRAM DPC.\n",
    ]
    .concat();
    assert_eq!(run(&source), "[1.234,50 ]\n[    1,25-]\n[*****12,34][      ]\n000012N\n24706\n3,75 7\nHALF\nINNER [  2,5]\n");
}

#[test]
fn blank_when_zero_makes_a_numeric_item_numeric_edited() {
    let out = run(&program(
        "",
        "       01  A PIC 9(3)V9 BLANK WHEN ZERO VALUE ZERO.\n       01  B PIC 9 BLANK WHEN ZERO VALUE '5'.\n       01  N PIC 9(3)V9.\n       01  D PIC 999 VALUE '000' BLANK WHEN ZERO.\n",
        &[
            line("DISPLAY '[' A '][' B '][' D ']'"),
            line("MOVE 0 TO A B"),
            line("DISPLAY '[' A '][' B ']'"),
            line("MOVE 12.5 TO A"),
            line("MOVE A TO N"),
            line("DISPLAY '[' A '] ' N"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    assert_eq!(out, "[0000][5][000]\n[    ][ ]\n[0125] 0125\n");
    let signed = program("", "       01  S PIC S9 BLANK WHEN ZERO.\n", &line("GOBACK."));
    assert!(compile_errors(&signed).contains("BLANK WHEN ZERO cannot be given for a PICTURE with S"));
}

#[test]
fn currency_signs_edit_fixed_and_floating_de_edit_and_reach_contained_programs() {
    let source = [
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. CUR.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n",
        "       SPECIAL-NAMES.\n           CURRENCY SIGN IS \"W\"\n           CURRENCY SIGN 'EUR ' WITH PICTURE SYMBOL 'y'\n",
        "           DECIMAL-POINT IS COMMA.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n",
        "       01  J PIC WWWWW.\n       01  M PIC W9999.\n       01  L PIC yyyy9,99-.\n       01  B PIC S9(5)V99.\n",
        "       01  K PIC 9999999V99.\n       PROCEDURE DIVISION.\n",
        &line("MOVE 1234 TO J"),
        &line("MOVE 42 TO M"),
        &line("DISPLAY '[' J '][' M ']'"),
        &line("MOVE 12 TO J"),
        &line("MOVE -5,5 TO L"),
        &line("DISPLAY '[' J '][' L ']'"),
        &line("MOVE L TO B"),
        &line("MOVE J TO K"),
        &line("DISPLAY B ' ' K"),
        &line("CALL 'INNER'"),
        &line("GOBACK."),
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. INNER.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n",
        "       01  E2 PIC WW9,9.\n       PROCEDURE DIVISION.\n",
        &line("MOVE 2,5 TO E2"),
        &line("DISPLAY 'INNER [' E2 ']'"),
        &line("GOBACK."),
        "       END PROGRAM INNER.\n       END PROGRAM CUR.\n",
    ]
    .concat();
    assert_eq!(run(&source), "[W1234][W0042]\n[  W12][   EUR 5,50-]\n000055} 000001200\nINNER [ W2,5]\n");
    let dollar = source.replace("PIC WWWWW", "PIC $$$$$");
    assert!(compile_errors(&dollar).contains("'$' is not a currency symbol"), "{}", compile_errors(&dollar));
}

/// A program whose SPECIAL-NAMES are `names`, with `data` in its WORKING-STORAGE, that moves 5.5
/// to E and displays it.
fn currency_program(card: &str, names: &str, data: &str) -> String {
    [
        card,
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. CUR.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n",
        "       SPECIAL-NAMES.\n",
        names,
        ".\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n",
        data,
        "       PROCEDURE DIVISION.\n",
        &line("MOVE 5.5 TO E"),
        &line("DISPLAY '[' E ']'"),
        &line("GOBACK."),
    ]
    .concat()
}

#[test]
fn a_hexadecimal_currency_sign_is_its_bytes_in_the_programs_code_page() {
    let euro = currency_program("", "           CURRENCY SIGN IS X'9F' WITH PICTURE SYMBOL 'Y'", "       01  E PIC YY9.99.\n");
    assert_eq!(run(&euro), "[ €5.50]\n");
    let dollar = currency_program("", "           CURRENCY SIGN IS X'5B'", "       01  E PIC $$9.99.\n");
    assert_eq!(run(&dollar), "[ $5.50]\n");
    let pound = currency_program("       CBL CODEPAGE(285)\n", "           CURRENCY SIGN IS X'5B'", "       01  E PIC ££9.99.\n");
    assert_eq!(run(&pound), "[ £5.50]\n");
    assert!(compile_errors(&pound.replace("CODEPAGE(285)", "CODEPAGE(37)")).contains("PICTURE ££9.99: '£' is not a numeric-edited symbol"));
    let letter = currency_program("", "           CURRENCY SIGN IS X'86'", "       01  E PIC ff9.99.\n");
    assert_eq!(run(&letter), "[ f5.50]\n");
    let refused = |names: &str| {
        let source = currency_program("", names, "       01  E PIC 9.9.\n");
        syntax::parse(&source).map_or_else(|e| e.message, |_| compile_errors(&source))
    };
    assert!(refused("           CURRENCY SIGN IS X'5B5B'").contains("CURRENCY SIGN X'5B5B' is not one character that can be a PICTURE currency symbol"));
    assert!(refused("           CURRENCY SIGN IS X'F1'").contains("CURRENCY SIGN X'F1' is \"1\" in the program's code page, which cannot be a PICTURE currency symbol"));
    assert!(refused("           CURRENCY SIGN IS X'F1C1' PICTURE SYMBOL 'Y'").contains("which contains a digit, +, -, . or ,"));
    assert!(refused("           CURRENCY SIGN IS '$'\n           CURRENCY SIGN IS X'5B'").contains("a second CURRENCY SIGN clause for the currency symbol '$'"));
    assert!(refused("           CURRENCY SIGN IS X''").contains("CURRENCY SIGN needs a nonempty alphanumeric literal"));
}

#[test]
fn a_group_sender_moves_its_bytes_and_a_statements_shared_result_is_computed_before_any_receiver_changes() {
    let out = run(&program(
        "",
        "       01  G.\n           05 G1 PIC X(3) VALUE '12A'.\n       01  N PIC 9(3).\n       01  E PIC ZZ9.\n       01  X PIC 99 VALUE 10.\n       01  Y PIC 99.\n       01  C PIC 9 VALUE 1.\n       01  DT.\n           05 D PIC 9 OCCURS 3 VALUE 0.\n",
        &[
            line("MOVE G TO N"),
            line("MOVE G TO E"),
            line("DISPLAY N ' ' E"),
            line("DIVIDE 4 INTO X GIVING X Y"),
            line("DISPLAY X Y"),
            line("ADD X TO X Y"),
            line("DISPLAY X Y"),
            line("ADD 1 TO C D(C)"),
            line("DISPLAY C DT"),
            line("COMPUTE C D(C) = C + 1"),
            line("DISPLAY C DT"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    assert_eq!(out, "12A 12A\n0202\n0404\n2010\n3013\n");
}

#[test]
fn a_zero_divisor_shared_by_several_receivers_is_a_size_error_or_the_program_check_of_its_divide() {
    let check = |usage: &str| {
        let data = format!("       01  Z PIC S9(4) COMP VALUE 0.\n       01  A {usage} VALUE 10.\n       01  B {usage} VALUE 20.\n");
        let body = [line("DIVIDE Z INTO A B ON SIZE ERROR DISPLAY A ' ' B END-DIVIDE"), line("DIVIDE Z INTO A B"), line("GOBACK.")].concat();
        let (out, _, ending) = run_with(&program("", &data, &body), &[]);
        (out, ending.unwrap_err().code.to_string())
    };
    assert_eq!(check("PIC 99 COMP"), ("10 20\n".to_owned(), "S0C9".to_owned()));
    assert_eq!(check("PIC 99"), ("10 20\n".to_owned(), "S0CB".to_owned()));
}

const INITIALIZED: &str = concat!(
    "       01  G.\n           05 A1 PIC AAA VALUE 'ABC'.\n           05 X1 PIC X(3) VALUE 'XYZ'.\n",
    "           05 E1 PIC XBX VALUE 'P Q'.\n           05 N1 PIC 9(3) VALUE 123.\n           05 NE PIC ZZ9 VALUE ' 45'.\n",
    "           05 FILLER PIC XX VALUE '**'.\n           05 T PIC 9 OCCURS 2 VALUE 7.\n           05 R REDEFINES T PIC XX.\n",
    "           05 FILLER.\n              10 IN-FILLER PIC X VALUE 'F'.\n       01  FIVE PIC 9 VALUE 5.\n",
);

#[test]
fn initialize_phrases_choose_receivers_and_senders_by_category() {
    let step = |statement: &str| [line("MOVE ALL '-' TO G"), line(statement), line("DISPLAY '[' G ']'")].concat();
    let out = run(&program(
        "",
        INITIALIZED,
        &[
            line("DISPLAY '[' G ']'"),
            step("INITIALIZE G"),
            step("INITIALIZE G REPLACING ALPHABETIC BY 'Q'\n               NUMERIC DATA BY FIVE"),
            step("INITIALIZE G WITH FILLER ALL TO VALUE"),
            step("INITIALIZE G NUMERIC TO VALUE THEN TO DEFAULT"),
            step("INITIALIZE G ALPHANUMERIC TO VALUE THEN REPLACING\n               NUMERIC-EDITED DATA BY 6 ALPHANUMERIC BY 'NO'"),
            step("INITIALIZE G WITH FILLER\n               REPLACING ALPHANUMERIC-EDITED BY 'ABC'"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    assert_eq!(
        out,
        concat!(
            "[ABCXYZP Q123 45**77F]\n",
            "[         000  0--00 ]\n",
            "[Q  ------005-----55-]\n",
            "[ABCXYZP Q123 45**77F]\n",
            "[         123  0--77 ]\n",
            "[---XYZ------  6----F]\n",
            "[------A B-----------]\n",
        )
    );
}

#[test]
fn initialize_replacing_that_no_item_matches_warns_as_igyps2047() {
    let data = "       01  ALPHA PIC AABAABAA.\n       01  GROUP1.\n           05 ALPHA2 PIC AABAA.\n           05 BETA PIC AAA.\n";
    let source = |statement: &str| program("", data, &[line("MOVE 'ABCDEFGH' TO ALPHA"), line(statement), line("DISPLAY ALPHA"), line("GOBACK.")].concat());
    let message = compile_errors(&source("INITIALIZE ALPHA REPLACING ALPHABETIC DATA BY ALL '3'"));
    assert!(message.contains("INITIALIZE ALPHA: none of its items is of a category REPLACING names (ALPHABETIC), so it is not initialized"), "{message}");
    assert!(message.starts_with("warning"), "{message}");
    assert_eq!(run(&source("INITIALIZE ALPHA REPLACING ALPHABETIC DATA BY ALL '3'")), "AB CD EF\n");
    let edited = compile_errors(&source("INITIALIZE ALPHA (1:2) REPLACING\n               ALPHANUMERIC-EDITED DATA BY ALL '3'"));
    assert!(edited.contains("INITIALIZE ALPHA: none of its items is of a category REPLACING names (ALPHANUMERIC-EDITED)"), "{edited}");
    assert_eq!(run(&source("INITIALIZE ALPHA (1:2) REPLACING ALPHANUMERIC DATA BY ALL '3'")), "33 CD EF\n");
    for quiet in [
        "INITIALIZE GROUP1 REPLACING ALPHABETIC DATA BY ALL '5'",
        "INITIALIZE ALPHA REPLACING\n               ALPHANUMERIC-EDITED DATA BY ALL '3'",
        "INITIALIZE ALPHA REPLACING ALPHABETIC DATA BY ALL '3'\n               THEN TO DEFAULT",
        "INITIALIZE ALPHA (1:2) REPLACING ALPHANUMERIC DATA BY ALL '3'",
    ] {
        assert_eq!(compile_errors(&source(quiet)), "", "{quiet}");
    }
    let twice = syntax::parse(&source("INITIALIZE ALPHA REPLACING NUMERIC BY 1\n               NUMERIC BY 2")).unwrap_err();
    assert!(twice.message.contains("NUMERIC is named twice in the REPLACING phrase"), "{}", twice.message);
}

#[test]
fn a_floating_point_value_literal_initializes_comp_1_and_comp_2() {
    let data = concat!(
        "       01  A COMP-1 VALUE 543.12345E10.\n       01  B COMP-2 VALUE -1.5E-03.\n",
        "       01  C USAGE COMP-2 VALUE +.25e2.\n       01  M PIC S9(9)V9(6) SIGN LEADING SEPARATE.\n",
    );
    let out = run(&program("", data, &[line("MOVE A TO M DISPLAY M"), line("MOVE B TO M DISPLAY M"), line("MOVE C TO M DISPLAY M"), line("GOBACK.")].concat()));
    assert_eq!(out, "+233610000000000\n-000000000001500\n+000000025000000\n");
    let refused = |entry: &str| syntax::parse(&program("", entry, &line("GOBACK."))).unwrap_err().message;
    assert!(refused("       01  F PIC 9(4) VALUE 1.5E+03.\n").contains("a floating-point VALUE literal is for a COMP-1 or COMP-2 item"));
    assert!(refused("       01  F COMP-2 VALUE 1.23456789012345678E1.\n").contains("mantissa has at most 16 digits"));
    assert!(refused("       01  F COMP-2 VALUE 1.0E+40.\n").contains("more than 31 digits in fixed point"));
}

#[test]
fn length_of_a_table_element_needs_no_subscript() {
    let data = "       01  T.\n           05 E OCCURS 3.\n              10 E1 PIC X(4).\n              10 E2 PIC 9(3) OCCURS 2.\n       01  N PIC 9(4).\n";
    let out = run(&program(
        "",
        data,
        &[line("MOVE LENGTH OF E TO N DISPLAY N"), line("COMPUTE N = LENGTH OF E2 * 2 DISPLAY N"), line("DISPLAY LENGTH OF E1"), line("GOBACK.")].concat(),
    ));
    assert_eq!(out, "0010\n0006\n000000004\n");
    let errors = compile_errors(&program("", data, &[line("MOVE E1 TO N"), line("GOBACK.")].concat()));
    assert!(errors.contains("E1 takes 1 subscripts, not 0"), "{errors}");
}

#[test]
fn an_arithmetic_receiver_must_be_numeric_or_numeric_edited() {
    let source = |procedure: &str| {
        format!(
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  G PIC X(5).\n       01  H PIC 9(3) VALUE 2.\n       01  E PIC ZZ9.\n       01  N PIC 9(3).\n       01  Q.\n           05  Q1 PIC 9.\n       PROCEDURE DIVISION.\n{procedure}           GOBACK.\n"
        )
    };
    let refused = compile_errors(&source("           COMPUTE G = H * 2\n           ADD 1 TO G\n           DIVIDE H INTO N GIVING N REMAINDER Q\n           MULTIPLY 2 BY H GIVING E\n           SUBTRACT 1 FROM H GIVING N\n"));
    let expected = [
        "IWC0323-S COMPUTE G: a receiving operand of an arithmetic statement must be numeric or numeric-edited, and G is alphanumeric",
        "IWC0323-S ADD G: a receiving operand of an arithmetic statement must be numeric or numeric-edited, and G is alphanumeric",
        "IWC0323-S DIVIDE Q: a receiving operand of an arithmetic statement must be numeric or numeric-edited, and Q is a group",
    ];
    assert_eq!(refused.lines().filter(|l| l.starts_with("IWC0323")).collect::<Vec<_>>(), expected, "{refused}");
    assert_eq!(compile_errors(&source("           COMPUTE E = H * 2\n           ADD 1 TO N\n           DIVIDE H INTO N GIVING N REMAINDER E\n")), "");
}

#[test]
fn a_label_records_data_name_must_be_defined() {
    let source = |label: &str, data: &str| {
        format!(
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       ENVIRONMENT DIVISION.\n       INPUT-OUTPUT SECTION.\n       FILE-CONTROL.\n           SELECT F ASSIGN TO FDD.\n       DATA DIVISION.\n       FILE SECTION.\n       FD  F {label}.\n       01  R PIC X(80).\n       WORKING-STORAGE SECTION.\n{data}       PROCEDURE DIVISION.\n           GOBACK.\n"
        )
    };
    assert_eq!(compile_errors(&source("LABEL RECORDS ARE STANDARD", "")), "");
    assert_eq!(compile_errors(&source("LABEL RECORD IS LBL", "       01  LBL PIC X(80).\n")), "");
    assert_eq!(compile_errors(&source("LABEL RECORDS XXXXX084 DATA RECORD IS R", "")), "IWC0324-S LABEL RECORDS XXXXX084: not defined as a data-name");
}

#[test]
fn recording_mode_f_takes_records_of_one_length() {
    let source = |clauses: &str, second: &str| {
        format!(
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       ENVIRONMENT DIVISION.\n       INPUT-OUTPUT SECTION.\n       FILE-CONTROL.\n           SELECT F ASSIGN TO FDD.\n       DATA DIVISION.\n       FILE SECTION.\n       FD  F {clauses}.\n       01  R1 PIC 9(1).\n{second}       WORKING-STORAGE SECTION.\n       PROCEDURE DIVISION.\n           GOBACK.\n"
        )
    };
    assert_eq!(compile_errors(&source("RECORDING MODE F", "       01  R2 PIC X(79).\n")), "IWC0325-S FD F: RECORDING MODE F, but its records are 1 to 79 bytes");
    assert_eq!(compile_errors(&source("RECORDING MODE F RECORD CONTAINS 1 TO 80 CHARACTERS", "")), "IWC0325-S FD F: RECORDING MODE F, but its records are 1 to 80 bytes");
    assert_eq!(compile_errors(&source("RECORDING MODE V", "       01  R2 PIC X(79).\n")), "");
    assert_eq!(compile_errors(&source("RECORDING MODE F", "       01  R2 PIC X(1).\n")), "");
}
