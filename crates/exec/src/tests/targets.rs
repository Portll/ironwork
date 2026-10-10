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
