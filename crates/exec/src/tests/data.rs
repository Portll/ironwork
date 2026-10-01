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
    assert_eq!(out, "[3ABC] E\n");
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
