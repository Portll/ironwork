//! Behaviours that follow the target compiler (docs/targets.md): each against cobc 3.2's result
//! under a GnuCOBOL target and IBM's under strict with --dialect ibm, on both executors.

use super::*;

const EXTENDED: &[&str] = &["--compliance=extended"];

/// Zero to a negative power inside ON SIZE ERROR, in a larger expression, and in floating point.
const ZERO_POWER: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. ZEROPOW.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 Z PIC S9(3)V99 VALUE 0.\n",
    "       01 E PIC S9(3) VALUE -2.\n",
    "       01 W PIC S9(3)V99 VALUE 7.\n",
    "       01 F COMP-2 VALUE 0.\n",
    "       PROCEDURE DIVISION.\n",
    "           COMPUTE W = Z ** E\n",
    "               ON SIZE ERROR DISPLAY 'SIZE ERROR ' W\n",
    "               NOT ON SIZE ERROR DISPLAY 'NO SIZE ERROR ' W\n",
    "           END-COMPUTE\n",
    "           MOVE 7 TO W\n",
    "           COMPUTE W = 1 + Z ** E\n",
    "           DISPLAY 'PLAIN ' W\n",
    "           COMPUTE F = F ** -1.5\n",
    "           IF F = 0 DISPLAY 'FLOAT ZERO' END-IF\n",
    "           STOP RUN.\n",
);

#[test]
fn zero_to_a_negative_power_is_zero_for_a_gnucobol_target_and_igz0050s_for_ibm() {
    // cobc 3.2's output: no size error, and the expression goes on with zero.
    let walked = Harness::source(ZERO_POWER).flags(EXTENDED).run(Executor::Interpreter);
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), ("NO SIZE ERROR +000.00\nPLAIN +001.00\nFLOAT ZERO\n", Some(&Ending::StopRun)), "{}", walked.err);
    let vm = Harness::source(ZERO_POWER).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    for (name, executor) in [("interpreter", Executor::Interpreter), ("VM", Executor::Vm)] {
        let ibm = Harness::source(ZERO_POWER).run(executor);
        assert!(ibm.out.starts_with("SIZE ERROR "), "{name}: {}", ibm.out);
        assert!(ibm.ending.as_ref().is_err_and(|a| a.message.starts_with("IGZ0050S")), "{name}: {:?}", ibm.ending);
    }
}

/// JSON and XML GENERATE into receivers longer than the documents.
const GENERATE_REST: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. GENREST.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 J PIC X(30) VALUE ALL '*'.\n",
    "       01 X PIC X(40) VALUE ALL '*'.\n",
    "       01 N PIC 9(4).\n",
    "       01 REC.\n",
    "          05 A PIC X(2) VALUE 'AB'.\n",
    "       PROCEDURE DIVISION.\n",
    "           JSON GENERATE J FROM REC COUNT IN N\n",
    "           IF J(N + 1:) = ALL X'20' DISPLAY 'JSON SPACES' END-IF\n",
    "           IF J(N + 1:) = ALL '*' DISPLAY 'JSON KEPT' END-IF\n",
    "           XML GENERATE X FROM REC COUNT IN N\n",
    "           DISPLAY '[' X ']'\n",
    "           STOP RUN.\n",
);

#[test]
fn a_gnucobol_target_fills_the_rest_of_a_generate_receiver_with_spaces_and_ibm_keeps_it() {
    // cobc 3.2 pads both receivers with spaces in the document's encoding.
    let walked = Harness::source(GENERATE_REST).flags(EXTENDED).run(Executor::Interpreter);
    assert_eq!(walked.out, format!("JSON SPACES\n[<REC><A>AB</A></REC>{}]\n", " ".repeat(20)), "{}", walked.err);
    let vm = Harness::source(GENERATE_REST).flags(EXTENDED).run(Executor::Vm);
    assert_eq!(vm.out, walked.out);
    for (name, executor) in [("interpreter", Executor::Interpreter), ("VM", Executor::Vm)] {
        let ibm = Harness::source(GENERATE_REST).run(executor);
        assert_eq!(ibm.out, format!("JSON KEPT\n[<REC><A>AB</A></REC>{}]\n", "*".repeat(20)), "{name}: {}", ibm.err);
    }
}

/// Intrinsic functions given arguments outside what they take.
const ARGUMENTS_OUT_OF_RANGE: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. ARGRANGE.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 N PIC S9(9) VALUE 0.\n",
    "       01 R PIC S9(9)V9(4).\n",
    "       01 C PIC X VALUE '*'.\n",
    "       01 D PIC X(10) VALUE ALL '*'.\n",
    "       PROCEDURE DIVISION.\n",
    "           MOVE FUNCTION CHAR(N) TO C\n",
    "           COMPUTE R = FUNCTION ORD(C)\n",
    "           DISPLAY 'CHAR ' R\n",
    "           COMPUTE R = FUNCTION DATE-OF-INTEGER(0)\n",
    "           DISPLAY 'DATE ' R\n",
    "           MOVE -1 TO N\n",
    "           COMPUTE R = FUNCTION ANNUITY(N, 3)\n",
    "           DISPLAY 'ANNUITY ' R\n",
    "           MOVE FUNCTION FORMATTED-DATE('YYYYMMDD', 0) TO D\n",
    "           DISPLAY 'FORMATTED [' D ']'\n",
    "           COMPUTE R = FUNCTION RANDOM(N)\n",
    "           IF R >= 0 AND R < 1 DISPLAY 'RANDOM' END-IF\n",
    "           STOP RUN.\n",
);

#[test]
fn a_gnucobol_target_gives_cobcs_values_for_arguments_out_of_range_and_ibm_ends_the_run() {
    // cobc 3.2's values; its RANDOM sequence differs from ironwork's (C54).
    let expected = "CHAR +000000001.0000\nDATE +000000000.0000\nANNUITY +000000000.0000\nFORMATTED [          ]\nRANDOM\n";
    let walked = Harness::source(ARGUMENTS_OUT_OF_RANGE).flags(EXTENDED).run(Executor::Interpreter);
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), (expected, Some(&Ending::StopRun)), "{}", walked.err);
    let vm = Harness::source(ARGUMENTS_OUT_OF_RANGE).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    for (name, executor) in [("interpreter", Executor::Interpreter), ("VM", Executor::Vm)] {
        let ibm = Harness::source(ARGUMENTS_OUT_OF_RANGE).run(executor);
        assert!(ibm.ending.as_ref().is_err_and(|a| a.message.starts_with("IGZ0162S")), "{name}: {:?}", ibm.ending);
    }
}

/// Divisions by zero outside ON SIZE ERROR: DIVIDE GIVING, COMPUTE with two receivers, REMAINDER,
/// INTO, and floating point.
const ZERO_DIVISOR: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. ZERODIV.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 A PIC 9(3) VALUE 5.\n",
    "       01 B PIC 9(3) VALUE 0.\n",
    "       01 Q PIC 9(3) VALUE 7.\n",
    "       01 R PIC 9(3) VALUE 8.\n",
    "       01 P PIC 9(3) VALUE 9.\n",
    "       01 F COMP-2 VALUE 3.\n",
    "       01 Z COMP-2 VALUE 0.\n",
    "       PROCEDURE DIVISION.\n",
    "           DIVIDE A BY B GIVING Q\n",
    "           DISPLAY 'DIVIDE ' Q\n",
    "           COMPUTE Q P = A / B\n",
    "           DISPLAY 'COMPUTE ' Q ' ' P\n",
    "           DIVIDE A BY B GIVING Q REMAINDER R\n",
    "           DISPLAY 'REMAINDER ' Q ' ' R\n",
    "           DIVIDE B INTO A\n",
    "           DISPLAY 'INTO ' A\n",
    "           COMPUTE F = F / Z\n",
    "           IF F = 3 DISPLAY 'FLOAT KEPT' END-IF\n",
    "           STOP RUN.\n",
);

#[test]
fn a_zero_divisor_leaves_its_receivers_for_a_gnucobol_target_and_ends_the_run_for_ibm() {
    // cobc 3.2's output: each receiver keeps its value and the run goes on.
    let expected = "DIVIDE 007\nCOMPUTE 007 009\nREMAINDER 007 008\nINTO 005\nFLOAT KEPT\n";
    let walked = Harness::source(ZERO_DIVISOR).flags(EXTENDED).run(Executor::Interpreter);
    assert_eq!((walked.out.as_str(), walked.ending.as_ref().ok()), (expected, Some(&Ending::StopRun)), "{}", walked.err);
    let vm = Harness::source(ZERO_DIVISOR).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((vm.out, vm.ending), (walked.out, walked.ending));
    for (name, executor) in [("interpreter", Executor::Interpreter), ("VM", Executor::Vm)] {
        let ibm = Harness::source(ZERO_DIVISOR).run(executor);
        assert!(ibm.ending.as_ref().is_err_and(|a| a.message.starts_with("CEE3211S")), "{name}: {:?}", ibm.ending);
    }
}

/// Zoned items holding letters MOVEd to zoned items of other sizes and scales.
const ZONED_CHARACTERS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. ZONEDCH.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 G1.\n",
    "          05 Z1 PIC 9(4).\n",
    "       01 G6.\n",
    "          05 Z6 PIC 9(6).\n",
    "       01 G2.\n",
    "          05 Z2 PIC 9(2).\n",
    "       01 GD.\n",
    "          05 ZD PIC 9(2)V99.\n",
    "       PROCEDURE DIVISION.\n",
    "           MOVE 'A1B2' TO G1\n",
    "           MOVE Z1 TO Z6 Z2 ZD\n",
    "           DISPLAY '[' G6 '][' G2 '][' GD ']'\n",
    "           STOP RUN.\n",
);

#[test]
fn a_gnucobol_target_copies_a_zoned_senders_characters_and_ibm_moves_its_digits() {
    // cobc 3.2 copies each digit's byte, aligned on the decimal point, '0' where there is none.
    for (name, executor, again) in [("interpreter", Executor::Interpreter, Executor::Interpreter), ("VM", Executor::Vm, Executor::Vm)] {
        let cobc = Harness::source(ZONED_CHARACTERS).flags(EXTENDED).run(executor);
        assert_eq!(cobc.out, "[00A1B2][B2][B200]\n", "{name}: {}", cobc.err);
        let ibm = Harness::source(ZONED_CHARACTERS).run(again);
        assert_eq!(ibm.out, "[001122][22][2200]\n", "{name}: {}", ibm.err);
    }
}

/// Receivers that are also the sending item, with one sending item and with two, and a COMPUTE.
const RECEIVER_ORDER: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. RECORDER.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 X PIC 9(3) VALUE 5.\n",
    "       01 Y PIC 9(3) VALUE 1.\n",
    "       PROCEDURE DIVISION.\n",
    "           ADD X TO X Y\n",
    "           DISPLAY 'ADD ' X ' ' Y\n",
    "           MOVE 5 TO X MOVE 1 TO Y\n",
    "           SUBTRACT X FROM X Y\n",
    "           DISPLAY 'SUB ' X ' ' Y\n",
    "           MOVE 5 TO X MOVE 2 TO Y\n",
    "           MULTIPLY X BY X Y\n",
    "           DISPLAY 'MUL ' X ' ' Y\n",
    "           MOVE 5 TO X MOVE 1 TO Y\n",
    "           COMPUTE X Y = X + 1\n",
    "           DISPLAY 'COMPUTE ' X ' ' Y\n",
    "           MOVE 5 TO X MOVE 1 TO Y\n",
    "           ADD X 1 TO X Y\n",
    "           DISPLAY 'ADD2 ' X ' ' Y\n",
    "           STOP RUN.\n",
);

#[test]
fn a_gnucobol_target_reads_one_sending_item_again_for_each_receiver_and_ibm_keeps_it() {
    // cobc 3.2: one sending item is read again after each store; two are summed once.
    let cobc = "ADD 010 011\nSUB 000 001\nMUL 025 050\nCOMPUTE 006 006\nADD2 011 007\n";
    let ibm = "ADD 010 006\nSUB 000 004\nMUL 025 010\nCOMPUTE 006 006\nADD2 011 007\n";
    for (name, executor, again) in [("interpreter", Executor::Interpreter, Executor::Interpreter), ("VM", Executor::Vm, Executor::Vm)] {
        assert_eq!(Harness::source(RECEIVER_ORDER).flags(EXTENDED).run(executor).out, cobc, "{name}");
        assert_eq!(Harness::source(RECEIVER_ORDER).run(again).out, ibm, "{name}");
    }
}
