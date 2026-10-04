use super::*;

#[test]
fn zoned_items_of_seventeen_and_eighteen_digits_are_numbers() {
    let out = run(&program(
        "",
        concat!(
            "       01  A PIC 9(18) VALUE 999999999999999998.\n       01  B PIC S9(17) VALUE -12345678901234567.\n",
            "       01  C PIC 9(18).\n       01  E PIC S9(17)V9 SIGN LEADING SEPARATE.\n       01  LONG PIC X(40) VALUE ALL '1'.\n",
        ),
        &[
            line("ADD 1 TO A"),
            line("DISPLAY A"),
            line("ADD 1 TO A ON SIZE ERROR DISPLAY 'SIZE' END-ADD"),
            line("COMPUTE C = A - B"),
            line("DISPLAY C"),
            line("MOVE B TO E"),
            line("DISPLAY E"),
            line("IF B < A DISPLAY 'B<A' END-IF"),
            line("MOVE LONG TO C"),
            line("DISPLAY C"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    assert_eq!(out, "999999999999999999\nSIZE\n012345678901234566\n-123456789012345670\nB<A\n111111111111111111\n");
}

#[test]
fn zoned_items_of_thirty_one_digits_under_arith_extend() {
    let out = run(&program(
        "ARITH(EXTEND)",
        "       01  L PIC S9(31) VALUE -1234567890123456789012345678901.\n       01  M PIC 9(31).\n",
        &[line("ADD 2 TO L"), line("DISPLAY L"), line("MOVE L TO M"), line("DISPLAY M"), line("IF L < M DISPLAY 'L<M' END-IF"), line("GOBACK.")].concat(),
    ));
    assert_eq!(out, "123456789012345678901234567889R\n1234567890123456789012345678899\nL<M\n");
}

#[test]
fn a_long_zoned_item_keeps_the_data_exception_and_the_numproc_sign_rules() {
    let data = "       01  G.\n           05 Z PIC 9(18).\n           05 ZX REDEFINES Z PIC X(18).\n";
    let add = |options: &str, bytes: &str| {
        let source = program(options, data, &[line(&format!("MOVE X'{bytes}' TO ZX")), line("ADD 1 TO Z"), line("DISPLAY Z"), line("GOBACK.")].concat());
        run_with(&source, &[])
    };
    let (_, _, ending) = add("", "F1F2F3F4F5F6F7F8F9FAF1F2F3F4F5F6F7F8");
    assert_eq!(ending.unwrap_err().code, "S0C7");
    let (out, _, ending) = add("NUMPROC(NOPFD)", "F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F141");
    assert!(ending.is_ok(), "{ending:?}");
    assert_eq!(out, "111111111111111112\n");
    let (_, _, ending) = add("NUMPROC(PFD)", "F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F141");
    assert_eq!(ending.unwrap_err().code, "S0C7");
}

const ODO_RECORD: &str = "       01  REC.\n           05 CNT PIC 9.\n           05 ITEM PIC X OCCURS 1 TO 5 DEPENDING ON CNT.\n";

#[test]
fn accept_from_sysin_transfers_card_images_unconverted() {
    let data = "       01  N PIC 9.\n       01  AMT PIC 9(02)V9(02).\n       01  B PIC 9(04).\n       01  LONG PIC X(100).\n";
    let procedure = [
        line("ACCEPT N"),
        line("ACCEPT AMT"),
        line("DISPLAY FUNCTION HEX-OF(AMT)"),
        line("ACCEPT B"),
        line("DISPLAY FUNCTION HEX-OF(B)"),
        line("ACCEPT LONG"),
        line("DISPLAY '[' LONG(78:6) ']'"),
        line("ACCEPT N"),
        line("ADD 1 TO B"),
        line("GOBACK."),
    ]
    .concat();
    let source = program("", data, &procedure);
    let (out, err, ending) = run_unit(&source, vec![], &format!("6\n\nQNB*\n{}\nTAIL\n", "X".repeat(80)));
    assert_eq!(out, "40404040\nD8D5C25C\n[XXXTAI]\n");
    assert!(err.contains("ACCEPT found SYSIN at its end; N is unchanged"), "{err}");
    let abend = ending.unwrap_err();
    let add = source.lines().position(|l| l.contains("ADD 1 TO B")).unwrap() as u32 + 1;
    assert_eq!((abend.code.as_str(), abend.pos.line), ("S0C7", add));
}

#[test]
fn a_group_holding_its_own_occurs_depending_on_object_receives_at_its_maximum_length() {
    let source = program(
        "",
        &[
            "       01  N PIC 9 VALUE 5.\n",
            ODO_RECORD,
            "       01  OUTSIDE.\n           05 O-ITEM PIC X OCCURS 1 TO 5 DEPENDING ON N.\n",
            "       01  SRC PIC X(6) VALUE '4ABCDE'.\n       01  CSV PIC X(9) VALUE 'AB,5VWXYZ'.\n       01  F1 PIC X(2).\n       01  P PIC 99.\n",
        ]
        .concat(),
        &[
            line("MOVE 1 TO CNT"),
            line("MOVE SRC TO REC"),
            line("DISPLAY '[' REC ']'"),
            line("MOVE ALL '-' TO OUTSIDE"),
            line("MOVE 2 TO N"),
            line("MOVE SRC TO OUTSIDE"),
            line("MOVE 5 TO N"),
            line("DISPLAY '[' OUTSIDE ']'"),
            line("MOVE 1 TO CNT"),
            line("MOVE 1 TO P"),
            line("STRING '3' 'XYZ' DELIMITED BY SIZE INTO REC WITH POINTER P"),
            line("    ON OVERFLOW DISPLAY 'OVERFLOW' END-STRING"),
            line("DISPLAY '[' REC '] ' P"),
            line("MOVE 1 TO CNT"),
            line("UNSTRING CSV DELIMITED BY ',' INTO F1 REC"),
            line("DISPLAY '[' REC ']'"),
            line("MOVE 1 TO CNT"),
            line("ACCEPT REC"),
            line("DISPLAY '[' REC '] ' ITEM(5)"),
            line("GOBACK."),
        ]
        .concat(),
    );
    let (out, err, ending) = run_unit(&source, vec![], "2QRSTU\n");
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "[4ABCD]\n[4A---]\n[3XYZ] 05\n[5VWXYZ]\n[2QR] U\n");
}

/// The Programming Guide's complex ODO example (SC27-8714-03, p. 81), with a variably located
/// table of fixed-length elements in place of its table of variable-length ones.
const COMPLEX_ODO: &str = concat!(
    "       01  FIELD-A.\n           02 COUNTER-1 PIC 99.\n           02 COUNTER-2 PIC 99.\n",
    "           02 TABLE-1.\n              03 RECORD-1 OCCURS 1 TO 5 DEPENDING ON COUNTER-1 PIC X(3).\n",
    "           02 EMPLOYEE-NUMBER PIC X(5).\n",
    "           02 TABLE-2 OCCURS 1 TO 3 DEPENDING ON COUNTER-2 PIC X(2).\n",
    "           02 TAIL-X PIC X(4).\n       01  W PIC 99.\n       01  COPY-A PIC X(40).\n",
);

#[test]
fn an_item_after_an_occurs_depending_on_table_moves_with_its_count() {
    let out = run(&program(
        "",
        COMPLEX_ODO,
        &[
            line("MOVE ALL '*' TO FIELD-A"),
            line("MOVE 2 TO COUNTER-1"),
            line("MOVE 1 TO COUNTER-2"),
            line("MOVE 'AAA' TO RECORD-1 (1)"),
            line("MOVE 'BBB' TO RECORD-1 (2)"),
            line("MOVE 'EMPNO' TO EMPLOYEE-NUMBER"),
            line("MOVE 'T1' TO TABLE-2 (1)"),
            line("MOVE 'TAIL' TO TAIL-X"),
            line("DISPLAY FIELD-A"),
            line("MOVE LENGTH OF FIELD-A TO W"),
            line("DISPLAY W ' ' LENGTH OF TABLE-1"),
            line("MOVE FIELD-A TO COPY-A"),
            line("DISPLAY COPY-A"),
            line("MOVE 3 TO COUNTER-1"),
            line("DISPLAY EMPLOYEE-NUMBER '|' TABLE-2 (1) '|' TAIL-X"),
            line("MOVE 1 TO COUNTER-1"),
            line("DISPLAY EMPLOYEE-NUMBER '|' TAIL-X"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "0201AAABBBEMPNOT1TAIL");
    assert_eq!(lines[1], "21 000000006");
    assert_eq!(lines[2].trim_end(), "0201AAABBBEMPNOT1TAIL");
    assert_eq!(lines[3], "NOT1T|AI|L***");
    assert_eq!(lines[4], "BBBEM|OT1T");
}

#[test]
fn a_variably_located_object_initialize_or_sort_key_is_refused_by_name() {
    let errors = compile_errors(&program(
        "",
        "       01  R.\n           05 N PIC 9.\n           05 T PIC X OCCURS 1 TO 5 DEPENDING ON N.\n           05 M PIC 9.\n           05 U PIC X OCCURS 1 TO 5 DEPENDING ON M.\n",
        &[line("INITIALIZE R"), line("INITIALIZE M"), line("GOBACK.")].concat(),
    ));
    assert!(errors.contains("OCCURS DEPENDING ON M: the object cannot follow an OCCURS DEPENDING ON table in its record"), "{errors}");
    assert!(errors.contains("INITIALIZE R: a variably located item, or a group holding one, cannot be initialized"), "{errors}");
    assert!(errors.contains("INITIALIZE M: a variably located item"), "{errors}");
    let nested = compile_errors(&program(
        "",
        "       01  R.\n           05 N PIC 9.\n           05 A OCCURS 2.\n              10 T PIC X OCCURS 1 TO 5 DEPENDING ON N.\n           05 X PIC X.\n",
        &line("GOBACK."),
    ));
    assert_eq!(nested, "IWR0012-S items after an OCCURS DEPENDING ON table in the same record are not supported yet");
    let sort = compile_errors(&file_program(
        "           SELECT S-FILE ASSIGN TO SORTWK1.\n           SELECT F ASSIGN TO FDD.\n",
        "       SD  S-FILE.\n       01  S-REC.\n           05 S-CNT PIC 9.\n           05 S-ITEM PIC X OCCURS 1 TO 5 DEPENDING ON S-CNT.\n           05 S-KEY PIC X.\n       FD  F.\n       01  F-REC PIC X(7).\n",
        "",
        &line("SORT S-FILE ON ASCENDING KEY S-KEY USING F GIVING F GOBACK."),
    ));
    assert_eq!(sort, "S-KEY: a sort key cannot follow an OCCURS DEPENDING ON table in its record");
}

#[test]
fn read_into_and_write_from_use_the_maximum_length_too() {
    let (input, output) = (temp("odo-in.txt"), temp("odo-out.txt"));
    std::fs::write(&input, "3VWXYZ\n").unwrap();
    let source = file_program(
        "           SELECT IN-F ASSIGN TO IDD.\n           SELECT OUT-F ASSIGN TO ODD.\n",
        "       FD  IN-F.\n       01  IN-REC PIC X(6).\n       FD  OUT-F.\n       01  OUT-REC.\n           05 O-CNT PIC 9.\n           05 O-ITEM PIC X OCCURS 1 TO 5 DEPENDING ON O-CNT.\n",
        &[ODO_RECORD, "       01  SRC PIC X(6) VALUE '2ABCDE'.\n"].concat(),
        &[
            line("OPEN INPUT IN-F OUTPUT OUT-F"),
            line("MOVE 1 TO CNT"),
            line("READ IN-F INTO REC"),
            line("DISPLAY '[' REC '] ' ITEM(5)"),
            line("WRITE OUT-REC FROM SRC"),
            line("CLOSE IN-F OUT-F"),
            line("GOBACK."),
        ]
        .concat(),
    );
    let dd = |name: &str, path: &std::path::Path| format!("{name}={}:text", path.display());
    let (out, err, ending) = run_files(&source, &[dd("IDD", &input), dd("ODD", &output)]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "[3VWX] Z\n");
    assert_eq!(std::fs::read_to_string(&output).unwrap(), "2AB\n");
}

#[test]
fn release_from_and_return_into_use_the_maximum_length_too() {
    let source = file_program(
        "           SELECT S-FILE ASSIGN TO SORTWK1.\n",
        "       SD  S-FILE.\n       01  S-REC.\n           05 S-CNT PIC 9.\n           05 S-ITEM PIC X OCCURS 1 TO 5 DEPENDING ON S-CNT.\n",
        "       01  W.\n           05 W-CNT PIC 9.\n           05 W-ITEM PIC X OCCURS 1 TO 5 DEPENDING ON W-CNT.\n       01  SRC PIC X(6) VALUE '3ABCDE'.\n",
        &[
            "       MAIN-LINE.\n",
            &line("SORT S-FILE ON ASCENDING KEY S-CNT INPUT PROCEDURE FEED"),
            &line("    OUTPUT PROCEDURE DRAIN"),
            &line("GOBACK."),
            "       FEED.\n",
            &line("MOVE 1 TO S-CNT"),
            &line("RELEASE S-REC FROM SRC."),
            "       DRAIN.\n",
            &line("MOVE 1 TO W-CNT"),
            &line("RETURN S-FILE INTO W AT END DISPLAY 'EMPTY' END-RETURN"),
            &line("DISPLAY '[' W '] ' W-ITEM(5)."),
        ]
        .concat(),
    );
    let (out, err, ending) = run_files(&source, &[]);
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "[3ABC]  \n");
}

#[test]
fn zoned_items_compare_with_nonnumeric_operands_by_their_bytes() {
    let data = [
        "       01  U-X PIC X(3) VALUE SPACES.\n       01  U REDEFINES U-X PIC 9(3).\n",
        "       01  S-X PIC X(3) VALUE SPACES.\n       01  S REDEFINES S-X PIC S9(3).\n",
        "       01  P PIC S9(3) VALUE -12.\n       01  T PIC S9(3) SIGN LEADING SEPARATE VALUE -12.\n",
    ]
    .concat();
    let procedure = [
        line("IF U = SPACES DISPLAY 'U' END-IF"),
        line("IF S = SPACES DISPLAY 'S' END-IF"),
        line("IF P = '012' DISPLAY 'P' END-IF"),
        line("IF P = '01K' DISPLAY 'P SIGNED' END-IF"),
        line("IF T = '012' DISPLAY 'T' END-IF"),
        line("GOBACK."),
    ]
    .concat();
    assert_eq!(run(&program("", &data, &procedure)), "U\nP\nT\n");
    assert_eq!(run(&program("NOZWB", &data, &procedure)), "U\nS\nP SIGNED\nT\n");
}

#[test]
fn an_alphanumeric_sender_moves_to_an_integer_unchecked_and_the_arithmetic_reading_it_abends() {
    let data = "       01  IN-X PIC X(5) VALUE '12*45'.\n       01  Z PIC 9(5).\n       01  P PIC S9(5) COMP-3.\n       01  T PIC S9(3) SIGN LEADING SEPARATE.\n";
    let procedure = [
        line("MOVE IN-X TO Z P T"),
        line("DISPLAY FUNCTION HEX-OF(Z)"),
        line("DISPLAY FUNCTION HEX-OF(P)"),
        line("DISPLAY FUNCTION HEX-OF(T)"),
        line("ADD 1 TO Z"),
        line("GOBACK."),
    ]
    .concat();
    let source = program("", data, &procedure);
    let (out, _, ending) = run_with(&source, &[]);
    assert_eq!(out, "F1F2FCF4F5\n12C45C\n4EFCF4F5\n");
    let abend = ending.unwrap_err();
    let add = source.lines().position(|l| l.contains("ADD 1 TO Z")).unwrap() as u32 + 1;
    assert_eq!((abend.code.as_str(), abend.pos.line), ("S0C7", add));
}

#[test]
fn a_zoned_or_packed_sender_moves_unchecked_where_pack_unpk_or_a_byte_copy_moves_it() {
    let data = [
        "       01  A-X PIC X(5) VALUE '12*34'.\n       01  A REDEFINES A-X PIC 9(5).\n",
        "       01  P-X PIC X(3) VALUE X'12A34C'.\n       01  P REDEFINES P-X PIC S9(5) COMP-3.\n",
        "       01  S-X PIC X(5) VALUE X'F1F2F3F440'.\n       01  S REDEFINES S-X PIC S9(5).\n",
        "       01  D-X PIC X(5) VALUE X'F1F25CF3D4'.\n       01  D REDEFINES D-X PIC S9(3)V99.\n",
        "       01  Z PIC 9(5).\n       01  Q PIC S9(5) COMP-3.\n       01  X5 PIC X(5).\n       01  G.\n           05  G1 PIC X(5).\n",
        "       01  U PIC 9(5).\n       01  T PIC S9(7).\n       01  E PIC S9(5)V9.\n",
    ]
    .concat();
    let procedure = [
        line("MOVE A TO Z X5 Q G"),
        line("DISPLAY FUNCTION HEX-OF(Z) ' ' X5 ' ' FUNCTION HEX-OF(Q)"),
        line("DISPLAY FUNCTION HEX-OF(G)"),
        line("MOVE P TO U"),
        line("DISPLAY FUNCTION HEX-OF(U)"),
        line("MOVE S TO T U"),
        line("DISPLAY FUNCTION HEX-OF(T) ' ' FUNCTION HEX-OF(U)"),
        line("MOVE D TO E"),
        line("DISPLAY FUNCTION HEX-OF(E)"),
        line("ADD 1 TO Z"),
        line("GOBACK."),
    ]
    .concat();
    let source = program("", &data, &procedure);
    let (out, _, ending) = run_with(&source, &[]);
    assert_eq!(out, "F1F2FCF3F4 12*34 12C34C\nF1F25CF3F4\nF1F2FAF3F4\nF0F0F1F2F3F440 F1F2F3F4F0\nF0F0F1F2FCD3\n");
    let abend = ending.unwrap_err();
    let add = source.lines().position(|l| l.contains("ADD 1 TO Z")).unwrap() as u32 + 1;
    assert_eq!((abend.code.as_str(), abend.pos.line), ("S0C7", add));
}

#[test]
fn a_zoned_or_packed_sender_is_checked_at_the_move_where_zap_cvb_or_ed_moves_it() {
    let data = "       01  A-X PIC X(5) VALUE '12*34'.\n       01  A REDEFINES A-X PIC 9(5).\n       01  P-X PIC X(3) VALUE X'12A34C'.\n       01  P REDEFINES P-X PIC S9(5) COMP-3.\n       01  B PIC 9(5) COMP.\n       01  N PIC ZZZZ9.\n       01  Q PIC S9(5) COMP-3.\n       01  R PIC S9(7) COMP-3.\n";
    for (options, statement) in [("", "MOVE A TO B"), ("", "MOVE A TO N"), ("", "MOVE P TO R"), ("", "MOVE P TO Q"), ("NUMPROC(PFD)", "MOVE P TO R")] {
        let source = program(options, data, &[line(statement), line("GOBACK.")].concat());
        let (_, _, ending) = run_with(&source, &[]);
        let abend = ending.unwrap_err();
        let at = source.lines().position(|l| l.contains(statement)).unwrap() as u32 + 1;
        assert_eq!((abend.code.as_str(), abend.pos.line), ("S0C7", at), "{options} {statement}");
    }
    let copied = program("NUMPROC(PFD)", data, &[line("MOVE P TO Q"), line("DISPLAY FUNCTION HEX-OF(Q)"), line("GOBACK.")].concat());
    assert_eq!(run(&copied), "12A34C\n");
}

#[test]
fn invdata_cleansign_reads_an_invalid_sign_nibble_as_positive() {
    let data = [
        "       01  Z-X PIC X(3) VALUE X'F1F203'.\n       01  Z REDEFINES Z-X PIC S9(3).\n",
        "       01  P-X PIC X(2) VALUE X'1230'.\n       01  P REDEFINES P-X PIC S9(3) COMP-3.\n",
        "       01  N PIC 9(3).\n",
    ]
    .concat();
    let procedure = [line("ADD 1 TO Z"), line("MOVE Z TO N"), line("DISPLAY N"), line("ADD 2 TO P"), line("MOVE P TO N"), line("DISPLAY N"), line("GOBACK.")].concat();
    assert_eq!(run(&program("INVDATA", &data, &procedure)), "124\n125\n");
    assert_eq!(run(&program("INVDATA(FNC)", &data, &procedure)), "124\n125\n");
    for options in ["", "INVDATA(NOCS)"] {
        let (_, _, ending) = run_with(&program(options, &data, &procedure), &[]);
        assert_eq!(ending.unwrap_err().code, "S0C7", "{options}");
    }
}

#[test]
fn invdata_noforcenumcmp_compares_an_unsigned_zoned_item_with_zero_by_its_zones() {
    let data = "       01  VALUE0 PIC X(4) VALUE '00 0'.\n       01  VALUE1 REDEFINES VALUE0 PIC 9(4).\n       01  W PIC 9(4) VALUE 0.\n";
    let procedure = [
        line("IF VALUE1 = ZERO DISPLAY 'ZERO' ELSE DISPLAY 'ZONES' END-IF"),
        line("IF VALUE1 = W DISPLAY 'W' ELSE DISPLAY 'NOT W' END-IF"),
        line("GOBACK."),
    ]
    .concat();
    for options in ["", "INVDATA(FNC)", "ZONEDATA(MIG)"] {
        assert_eq!(run(&program(options, data, &procedure)), "ZERO\nW\n", "{options}");
    }
    for options in ["INVDATA", "INVDATA(NOFNC,NOCS)", "ZONEDATA(NOPFD)"] {
        assert_eq!(run(&program(options, data, &procedure)), "ZONES\nNOT W\n", "{options}");
    }
}

#[test]
fn noinvdata_compares_an_unsigned_zoned_item_with_zero_by_its_zones_only_when_optimized() {
    let data = "       01  VALUE0 PIC X(4) VALUE '00 0'.\n       01  VALUE1 REDEFINES VALUE0 PIC 9(4).\n       01  W PIC 9(4) VALUE 0.\n";
    let procedure = [
        line("IF VALUE1 = ZERO DISPLAY 'ZERO' ELSE DISPLAY 'ZONES' END-IF"),
        line("IF VALUE1 = 0 DISPLAY 'ZERO' ELSE DISPLAY 'ZONES' END-IF"),
        line("IF VALUE1 = W DISPLAY 'W' ELSE DISPLAY 'NOT W' END-IF"),
        line("GOBACK."),
    ]
    .concat();
    for options in ["", "OPT(0)", "NOOPTIMIZE", "OPT(2),INVDATA(FNC)"] {
        assert_eq!(run(&program(options, data, &procedure)), "ZERO\nZERO\nW\n", "{options}");
    }
    for options in ["OPT(1)", "OPT(2)", "OPTIMIZE", "OPT(2),NUMPROC(PFD)"] {
        assert_eq!(run(&program(options, data, &procedure)), "ZONES\nZONES\nNOT W\n", "{options}");
    }
}

#[test]
fn a_non_digit_compared_with_zero_abends_at_opt_0_and_compares_by_bytes_when_optimized() {
    let data = "       01  A-X PIC X(5) VALUE '12*34'.\n       01  A REDEFINES A-X PIC 9(5).\n       01  F-X PIC X VALUE '*'.\n       01  F REDEFINES F-X PIC 9.\n          88 ENTERED VALUE 0.\n          88 STARTED VALUE ZERO 5.\n";
    let at = |source: &str, statement: &str| source.lines().position(|l| l.contains(statement)).unwrap() as u32 + 1;
    for statement in ["IF A = ZERO DISPLAY 'Y' END-IF", "IF A NOT = 0 DISPLAY 'Y' END-IF", "IF ENTERED DISPLAY 'Y' END-IF", "IF A > 5 DISPLAY 'Y' END-IF"] {
        let source = program("", data, &[line(statement), line("GOBACK.")].concat());
        let abend = run_with(&source, &[]).2.unwrap_err();
        assert_eq!((abend.code.as_str(), abend.pos.line), ("S0C7", at(&source, statement)), "{statement}");
    }
    let optimized = program("OPT(2)", data, &[line("IF A = ZERO DISPLAY 'ZERO' END-IF"), line("IF A NOT = 0 DISPLAY 'NOT 0' END-IF"), line("IF ENTERED DISPLAY 'ENTERED' ELSE DISPLAY 'NO' END-IF"), line("GOBACK.")].concat());
    assert_eq!(run(&optimized), "NOT 0\nNO\n");
    for statement in ["IF A > 5 DISPLAY 'Y' END-IF", "IF STARTED DISPLAY 'Y' END-IF"] {
        let source = program("OPT(2)", data, &[line(statement), line("GOBACK.")].concat());
        let abend = run_with(&source, &[]).2.unwrap_err();
        assert_eq!((abend.code.as_str(), abend.pos.line), ("S0C7", at(&source, statement)), "{statement}");
    }
}

#[test]
fn a_condition_name_of_a_nonnumeric_value_compares_a_zoned_variable_by_its_bytes() {
    let data = "       01  S-X PIC X(2) VALUE SPACES.\n       01  S REDEFINES S-X PIC S9(2).\n          88 EMPTY VALUE SPACES.\n          88 NONE VALUE ZERO.\n";
    let procedure = [line("IF EMPTY DISPLAY 'EMPTY' ELSE DISPLAY 'SIGNED' END-IF"), line("GOBACK.")].concat();
    assert_eq!(run(&program("NOZWB", data, &procedure)), "EMPTY\n");
    assert_eq!(run(&program("", data, &procedure)), "SIGNED\n");
    let none = program("", data, &[line("IF NONE DISPLAY 'NONE' END-IF"), line("GOBACK.")].concat());
    assert_eq!(run_with(&none, &[]).2.unwrap_err().code, "S0C7");
}
