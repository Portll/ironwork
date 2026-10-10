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

/// Characters MOVEd to numeric items, and lines ACCEPTed into them.
const CHARACTERS_TO_NUMBERS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. CHARNUM.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 A PIC X(8).\n",
    "       01 N PIC 9(3).\n",
    "       01 S PIC S9(3)V99.\n",
    "       01 P PIC S9(5) COMP-3.\n",
    "       01 LONG PIC X(12).\n",
    "       PROCEDURE DIVISION.\n",
    "           MOVE '1,234.5' TO A MOVE A TO N S DISPLAY N ' ' S\n",
    "           MOVE '3.5-' TO A MOVE A TO N S DISPLAY N ' ' S\n",
    "           MOVE '- 4' TO A MOVE A TO N S DISPLAY N ' ' S\n",
    "           MOVE '1a2' TO A MOVE A TO N S DISPLAY N ' ' S\n",
    "           MOVE '-12.5' TO A MOVE A TO P DISPLAY P\n",
    "           ACCEPT LONG DISPLAY '[' LONG ']'\n",
    "           ACCEPT S DISPLAY S\n",
    "           STOP RUN.\n",
);

#[test]
fn a_gnucobol_target_moves_and_accepts_characters_into_numbers_as_cobc_does() {
    // cobc 3.2's output: the digits aligned on the point, zero where another character comes
    // before the receiver is full, and ACCEPT one line, moved as MOVE moves it.
    let expected = "234 +234.50\n003 +000.00\n004 -004.00\n000 +000.00\n-00012\n[abc         ]\n-003.50\n";
    for (name, executor) in [("interpreter", Executor::Interpreter), ("VM", Executor::Vm)] {
        let ran = Harness::source(CHARACTERS_TO_NUMBERS).flags(EXTENDED).sysin("abc\n-3.5\n").run(executor);
        assert_eq!((ran.out.as_str(), ran.ending.as_ref().ok()), (expected, Some(&Ending::StopRun)), "{name}: {}", ran.err);
    }
}

/// Signed zoned items whose sign byte holds a digit, a space or another character, compared with
/// alphanumeric operands.
const SIGN_BYTE_COMPARED: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. SIGNCMP.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 W PIC S9(3) VALUE ZERO.\n",
    "       01 WG REDEFINES W PIC X(3).\n",
    "       01 L PIC S9(3) SIGN LEADING VALUE -123.\n",
    "       01 LG REDEFINES L PIC X(3).\n",
    "       PROCEDURE DIVISION.\n",
    "           IF L = '123' DISPLAY 'SIGNED' END-IF\n",
    "           MOVE SPACES TO WG\n",
    "           IF W = SPACES DISPLAY 'SPACES' END-IF\n",
    "           MOVE '12 ' TO WG\n",
    "           IF W = '12 ' DISPLAY 'TRAILING SPACE' END-IF\n",
    "           MOVE ' 23' TO LG\n",
    "           IF L = ' 23' DISPLAY 'LEADING SPACE' END-IF\n",
    "           MOVE '12$' TO WG\n",
    "           IF W = '120' DISPLAY 'OTHER ZERO' END-IF\n",
    "           STOP RUN.\n",
);

#[test]
fn a_gnucobol_target_keeps_a_space_in_the_sign_byte_and_ibm_makes_it_a_digit() {
    // cobc 3.2's output, by default and under -std=ibm-strict.
    for (name, vm) in [("interpreter", false), ("VM", true)] {
        let executor = || if vm { Executor::Vm } else { Executor::Interpreter };
        let cobc = Harness::source(SIGN_BYTE_COMPARED).flags(EXTENDED).run(executor());
        assert_eq!((cobc.out.as_str(), cobc.ending.as_ref().ok()), ("SIGNED\nSPACES\nTRAILING SPACE\nLEADING SPACE\nOTHER ZERO\n", Some(&Ending::StopRun)), "{name}: {}", cobc.err);
        assert_eq!(Harness::source(SIGN_BYTE_COMPARED).run(executor()).out, "SIGNED\n", "{name}");
    }
}

/// `statement` on an indexed or relative file with no FILE STATUS, no declarative and no data set.
fn vsam_failure(organization: &str, statement: &str) -> String {
    format!(
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. VSAMFAIL.\n       ENVIRONMENT DIVISION.\n       INPUT-OUTPUT SECTION.\n       FILE-CONTROL.\n           SELECT F ASSIGN TO NOFILE\n               {organization}.\n       DATA DIVISION.\n       FILE SECTION.\n       FD F.\n       01 R.\n          05 K PIC X(4).\n       PROCEDURE DIVISION.\n           DISPLAY 'BEFORE'\n           {statement}\n           DISPLAY 'AFTER'\n           STOP RUN.\n"
    )
}

#[test]
fn a_failing_vsam_open_or_close_ends_the_run_for_a_gnucobol_target_and_returns_for_ibm() {
    // cobc 3.2: `file does not exist (status = 35)` at the OPEN and `file not open (status = 42)`
    // at the CLOSE, the run ended there.
    for (organization, statement, status) in [("ORGANIZATION IS INDEXED RECORD KEY IS K", "OPEN INPUT F", "35"), ("ORGANIZATION IS RELATIVE", "CLOSE F", "42")] {
        let source = vsam_failure(organization, statement);
        for (name, vm) in [("interpreter", false), ("VM", true)] {
            let executor = || if vm { Executor::Vm } else { Executor::Interpreter };
            let cobc = Harness::source(&source).flags(EXTENDED).run(executor());
            assert_eq!(cobc.out, "BEFORE\n", "{statement} {name}: {}", cobc.err);
            assert!(cobc.ending.as_ref().is_err_and(|a| a.message.starts_with("IGZ0035S") && a.message.contains(&format!("status code was {status}"))), "{statement} {name}: {:?}", cobc.ending);
            let ibm = Harness::source(&source).run(executor());
            assert_eq!((ibm.out.as_str(), ibm.ending.as_ref().ok()), ("BEFORE\nAFTER\n", Some(&Ending::StopRun)), "{statement} {name}: {}", ibm.err);
        }
    }
}

/// Zoned items holding characters other than digits, shown and moved; the sign position holds no
/// letter, which ironwork's EBCDIC reads as an overpunched digit.
const ZONED_SHOWN: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. ZONEDCH.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 U PIC 9(4).\n",
    "       01 UG REDEFINES U PIC X(4).\n",
    "       01 S PIC S9(4).\n",
    "       01 SG REDEFINES S PIC X(4).\n",
    "       01 L PIC S9(4) SIGN LEADING.\n",
    "       01 LG REDEFINES L PIC X(4).\n",
    "       01 D PIC S9(2)V99.\n",
    "       01 DG REDEFINES D PIC X(4).\n",
    "       01 E PIC S9(4) SIGN LEADING SEPARATE.\n",
    "       01 EG REDEFINES E PIC X(5).\n",
    "       01 T PIC S9(5).\n",
    "       01 TG REDEFINES T PIC X(5).\n",
    "       PROCEDURE DIVISION.\n",
    "           MOVE '1 #4' TO UG DISPLAY U\n",
    "           MOVE '    ' TO UG DISPLAY U\n",
    "           MOVE '#1*2' TO SG DISPLAY S\n",
    "           MOVE '12#$' TO SG DISPLAY S\n",
    "           MOVE '12# ' TO SG DISPLAY S\n",
    "           MOVE ' 2#4' TO LG DISPLAY L\n",
    "           MOVE '#2 0' TO DG DISPLAY D\n",
    "           MOVE '-#1 2' TO EG DISPLAY E\n",
    "           MOVE '1 #4' TO UG MOVE U TO T DISPLAY '[' TG ']'\n",
    "           MOVE '12 $' TO SG MOVE S TO T DISPLAY '[' TG ']'\n",
    "           MOVE -5 TO S MOVE '1 ' TO SG(1:2) MOVE S TO T\n",
    "           DISPLAY '[' TG(1:4) ']' IF T < 0 DISPLAY 'NEGATIVE' END-IF\n",
    "           STOP RUN.\n",
);

#[test]
fn a_gnucobol_target_shows_and_moves_a_zoned_items_characters_as_cobc_does() {
    // cobc 3.2's output by default, then under -std=ibm-strict.
    for (target, expected) in [(numeric::Target::Gnucobol, "1 #4\n    \n+#1*2\n+12#0\n+12# \n+ 2#4\n+#2.00\n-#1 2\n[010#4]\n[01200]\n[0100]\nNEGATIVE\n"), (numeric::Target::GnucobolIbmStrict, "10#4\n0000\n#1*2+\n12#0+\n12#0+\n+02#4\n#200+\n-#102\n[010#4]\n[01200]\n[0100]\nNEGATIVE\n")] {
        let flags = target.flags();
        let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
        for (name, executor) in [("interpreter", Executor::Interpreter), ("VM", Executor::Vm)] {
            let ran = Harness::source(ZONED_SHOWN).flags(&flags).run(executor);
            assert_eq!((ran.out.as_str(), ran.ending.as_ref().ok()), (expected, Some(&Ending::StopRun)), "{} {name}: {}", target.name(), ran.err);
        }
    }
}

/// Zoned and packed items holding no valid number, read by arithmetic and comparisons, moved and
/// shown.
const BAD_DECIMAL: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. BADDATA.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 U PIC 9(4).\n",
    "       01 UG REDEFINES U PIC X(4).\n",
    "       01 S PIC S9(4).\n",
    "       01 SG REDEFINES S PIC X(4).\n",
    "       01 D PIC 9(2)V99.\n",
    "       01 DG REDEFINES D PIC X(4).\n",
    "       01 E PIC S9(3) SIGN LEADING SEPARATE.\n",
    "       01 EG REDEFINES E PIC X(4).\n",
    "       01 L PIC S9(3) SIGN LEADING.\n",
    "       01 LG REDEFINES L PIC X(3).\n",
    "       01 P PIC S9(3) COMP-3.\n",
    "       01 PG REDEFINES P PIC X(2).\n",
    "       01 Q PIC S9(4) COMP-3.\n",
    "       01 QG REDEFINES Q PIC X(3).\n",
    "       01 V PIC 9(3) COMP-3.\n",
    "       01 VG REDEFINES V PIC X(2).\n",
    "       01 W PIC S9(5) COMP-3.\n",
    "       01 WG REDEFINES W PIC X(3).\n",
    "       01 Z PIC 9(5).\n",
    "       01 B PIC 9(5) COMP.\n",
    "       01 R PIC S9(6).\n",
    "       PROCEDURE DIVISION.\n",
    "           MOVE '1 #4' TO UG ADD 1 TO U GIVING R DISPLAY R\n",
    "           IF U = 1034 DISPLAY 'EQ 1034' END-IF\n",
    "           MOVE '12$ ' TO SG COMPUTE R = S * 2 DISPLAY R\n",
    "           MOVE '1*3%' TO DG COMPUTE R = D * 100 DISPLAY R\n",
    "           MOVE HIGH-VALUES TO UG COMPUTE R = U + 0 DISPLAY R\n",
    "           MOVE LOW-VALUES TO UG COMPUTE R = U + 0 DISPLAY R\n",
    "           MOVE '*1 2' TO EG COMPUTE R = E + 0 DISPLAY R\n",
    "           MOVE 'r1 ' TO LG COMPUTE R = L + 0 DISPLAY R\n",
    "           MOVE X'1A2B' TO PG COMPUTE R = P + 0 DISPLAY R\n",
    "           MOVE X'F1234D' TO QG COMPUTE R = Q + 0 DISPLAY R\n",
    "           MOVE X'1C3A' TO VG DISPLAY V\n",
    "           MOVE V TO W DISPLAY FUNCTION HEX-OF(WG)\n",
    "           MOVE V TO Z DISPLAY Z\n",
    "           MOVE V TO B DISPLAY B\n",
    "           MOVE '1 #4' TO UG MOVE U TO W DISPLAY FUNCTION HEX-OF(WG)\n",
    "           STOP RUN.\n",
);

#[test]
fn a_gnucobol_target_reads_invalid_decimal_data_as_cobc_does_and_ibm_ends_the_run() {
    // cobc 3.2's output by default, then under -std=ibm-strict.
    for (target, expected) in [(numeric::Target::Gnucobol, "+001035\nEQ 1034\n+002480\n+002035\n+010000\n-010000\n+000102\n-000210\n+000202\n-001234\n1<3\n001C3A\n001<3\n00223\n01034C\n"), (numeric::Target::GnucobolIbmStrict, "001035+\nEQ 1034\n002480+\n002035+\n010000+\n010000-\n000102+\n000210-\n000202+\n001234-\n1<3\n001C3A\n001<3\n0000000223\n01034C\n")] {
        let flags = target.flags();
        let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
        for (name, executor) in [("interpreter", Executor::Interpreter), ("VM", Executor::Vm)] {
            let ran = Harness::source(BAD_DECIMAL).flags(&flags).run(executor);
            assert_eq!((ran.out.as_str(), ran.ending.as_ref().ok()), (expected, Some(&Ending::StopRun)), "{} {name}: {}", target.name(), ran.err);
        }
    }
    for (name, executor) in [("interpreter", Executor::Interpreter), ("VM", Executor::Vm)] {
        let ibm = Harness::source(BAD_DECIMAL).run(executor);
        assert!(ibm.ending.as_ref().is_err_and(|a| a.message.starts_with("CEE3207S")), "{name}: {:?}", ibm.ending);
    }
}

/// RETURN-CODE given values its halfword and its fullword hold, and its length.
const RETURN_CODE_SIZE: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. RCSIZE.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 N PIC S9(9) COMP-5.\n",
    "       01 X PIC S9(12).\n",
    "       PROCEDURE DIVISION.\n",
    "           MOVE 7 TO RETURN-CODE DISPLAY RETURN-CODE\n",
    "           MOVE 70000 TO RETURN-CODE DISPLAY RETURN-CODE\n",
    "           MOVE RETURN-CODE TO X DISPLAY X\n",
    "           MOVE 1234567890 TO RETURN-CODE DISPLAY RETURN-CODE\n",
    "           COMPUTE RETURN-CODE = -40000 DISPLAY RETURN-CODE\n",
    "           MOVE LENGTH OF RETURN-CODE TO N DISPLAY N\n",
    "           ADD 1 TO RETURN-CODE DISPLAY RETURN-CODE\n",
    "           MOVE 259 TO RETURN-CODE\n",
    "           STOP RUN.\n",
);

#[test]
fn a_gnucobol_target_keeps_return_code_in_a_fullword_and_ibm_in_a_halfword() {
    // cobc 3.2's output by default, then under -std=ibm-strict; IBM's halfword cuts each value
    // to four digits.
    let ibm_shown = "0007\n0000\n00000000000{\n7890\n0000\n0000000002\n0001\n";
    let flags = |target: numeric::Target| target.flags().to_vec();
    for (flags, expected) in [(flags(numeric::Target::Gnucobol), "+000000007\n+000070000\n+000000070000\n+1234567890\n-000040000\n+0000000004\n-000039999\n"), (flags(numeric::Target::GnucobolIbmStrict), "+000000007\n+000070000\n000000070000+\n+1234567890\n-000040000\n+0000000004\n-000039999\n"), (Vec::new(), ibm_shown)] {
        let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
        for (name, executor) in [("interpreter", Executor::Interpreter), ("VM", Executor::Vm)] {
            let ran = Harness::source(RETURN_CODE_SIZE).flags(&flags).run(executor);
            assert_eq!((ran.out.as_str(), ran.ending.as_ref().ok(), ran.return_code), (expected, Some(&Ending::StopRun), 259), "{flags:?} {name}: {}", ran.err);
        }
    }
}

/// INSPECT of a national item: tallies of ALL, CHARACTERS before and after a value, and LEADING,
/// then REPLACING.
const NATIONAL_INSPECTED: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. INSPN.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 NT PIC N(6) VALUE N'ABCABC'.\n",
    "       01 NX REDEFINES NT PIC X(12).\n",
    "       01 C PIC 9(4) VALUE 0.\n",
    "       01 D PIC 9(4) VALUE 0.\n",
    "       PROCEDURE DIVISION.\n",
    "           INSPECT NT TALLYING C FOR ALL N'B'\n",
    "           DISPLAY C\n",
    "           INSPECT NT TALLYING D FOR CHARACTERS\n",
    "           DISPLAY D\n",
    "           MOVE 0 TO C\n",
    "           INSPECT NT TALLYING C FOR CHARACTERS BEFORE N'C'\n",
    "           DISPLAY C\n",
    "           MOVE 0 TO C\n",
    "           INSPECT NT TALLYING C FOR CHARACTERS AFTER N'B'\n",
    "           DISPLAY C\n",
    "           MOVE 0 TO C\n",
    "           INSPECT NT TALLYING C FOR LEADING N'A'\n",
    "           DISPLAY C\n",
    "           INSPECT NT REPLACING ALL N'C' BY N'Q' AFTER N'B'\n",
    "           DISPLAY FUNCTION HEX-OF(NX)\n",
    "           STOP RUN.\n",
);

#[test]
fn a_gnucobol_target_tallies_a_national_items_characters_in_bytes_and_ibm_in_characters() {
    // cobc 3.2's output; IBM counts national characters.
    for (flags, expected) in [(EXTENDED, "0002\n0012\n0004\n0008\n0001\n004100420051004100420051\n"), (&[][..], "0002\n0006\n0002\n0004\n0001\n004100420051004100420051\n")] {
        for (name, executor) in [("interpreter", Executor::Interpreter), ("VM", Executor::Vm)] {
            let ran = Harness::source(NATIONAL_INSPECTED).flags(flags).run(executor);
            assert_eq!((ran.out.as_str(), ran.ending.as_ref().ok()), (expected, Some(&Ending::StopRun)), "{flags:?} {name}: {}", ran.err);
        }
    }
}
