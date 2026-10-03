//! --dialect gnucobol: each place ironwork gives cobc -std=ibm's result in place of the one its
//! register of assumptions chose (docs/dialect.md). The expected gnucobol output is GnuCOBOL 3.2's.

use super::*;
use numeric::Dialect;

/// The program's output under `dialect`, the same on the interpreter, in the differential run, and
/// on the VM alone.
fn under(source: &str, dialect: Dialect) -> String {
    let flags = [dialect.flag()];
    let walker = Harness::source(source).flags(&flags).run(Executor::Interpreter);
    assert!(walker.ending.is_ok(), "{:?}\n{}", walker.ending, walker.err);
    let vm = Harness::source(source).flags(&flags).run(Executor::Vm);
    assert_eq!(vm.out, walker.out, "the VM under {}", dialect.name());
    walker.out
}

#[test]
fn a_rounded_receiver_s_extra_place_reaches_intermediate_results_under_ibm_alone() {
    let source = program(
        "",
        "       01  A PIC 9(5)V99 VALUE 1.\n       01  B PIC 9(5)V99 VALUE 3.\n       01  C PIC 9(3) VALUE 100.\n       01  D PIC S9(5)V99 VALUE 1.\n       01  E PIC S9(5)V99 VALUE 12.35.\n       01  DIV2 PIC 99V9 VALUE 44.1.\n       01  DIV3 PIC 9(4)V9 VALUE 1661.7.\n       01  X PIC 9(5)V99.\n       01  Y PIC 9(5)V99.\n       01  S PIC 99V9.\n       01  Z PIC S99V9.\n",
        &[
            line("COMPUTE D ROUNDED = D + E / 3"),
            line("COMPUTE Y ROUNDED = A / B * C"),
            line("COMPUTE X ROUNDED = (A / B) + (A / B)"),
            line("DISPLAY 'INNER ' D ' ' Y ' ' X"),
            line("COMPUTE S ROUNDED = 1 + 1661.7 / DIV2"),
            line("COMPUTE Z ROUNDED = - (DIV3 / DIV2)"),
            line("DISPLAY 'INNER ' S ' ' Z"),
            line("COMPUTE S ROUNDED = 1661.7 / DIV2"),
            line("DIVIDE DIV2 INTO DIV3 ROUNDED"),
            line("DIVIDE 3 INTO 2 GIVING X ROUNDED REMAINDER Y"),
            line("DISPLAY 'LAST ' S ' ' DIV3 ' ' X ' ' Y"),
            line("GOBACK."),
        ]
        .concat(),
    );
    let last = "LAST 377 00377 0000067 0000002\n";
    assert_eq!(run(&source), format!("INNER 000051B 0003330 0000067\nINNER 387 37P\n{last}"));
    assert_eq!(under(&source, Dialect::Ibm), run(&source));
    assert_eq!(under(&source, Dialect::Gnucobol), format!("INNER 000051A 0003300 0000066\nINNER 386 37O\n{last}"));
}

/// bench/packed.cbl for 300 turns of its loop: a quotient inside COMPUTE D ROUNDED = D + C / 3
/// keeps a third decimal place under ibm alone.
#[test]
fn the_packed_benchmark_gives_cobc_s_totals_under_gnucobol() {
    let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../bench/packed.cbl")).unwrap().replace("VALUE 500000", "VALUE 300");
    assert_eq!(under(&source, Dialect::Ibm), "PACKED ACC= 0000059890595.94\nPACKED D= 0000000391030.02\n");
    assert_eq!(under(&source, Dialect::Gnucobol), "PACKED ACC= 0000059890449.97\nPACKED D= 0000000391029.04\n");
}

#[test]
fn display_shows_packed_and_binary_items_as_cobc_does_under_gnucobol() {
    let source = program(
        "",
        "       01  P1 PIC S9(5)V99 COMP-3 VALUE -123.45.\n       01  P2 PIC S9(5)V99 COMP-3 VALUE 123.45.\n       01  P3 PIC 9(5)V99 COMP-3 VALUE 123.45.\n       01  B1 PIC S9(4) COMP VALUE -12.\n       01  B2 PIC 9(4) COMP VALUE 12.\n       01  B3 PIC S9(9) COMP VALUE -123456.\n       01  B4 PIC S9(18) COMP VALUE -5.\n       01  B5 PIC S9(3)V99 COMP VALUE -1.25.\n       01  C5 PIC S9(4) COMP-5 VALUE -3.\n       01  Z1 PIC S9(3) VALUE -12.\n",
        &[line("DISPLAY P1 ' ' P2 ' ' P3"), line("DISPLAY B1 ' ' B2 ' ' B3 ' ' B4 ' ' B5 ' ' C5 ' ' Z1"), line("GOBACK.")].concat(),
    );
    assert_eq!(under(&source, Dialect::Ibm), "001234N 0012345 0012345\n001K 0012 00012345O 00000000000000000N 0012N 0000L 01K\n");
    assert_eq!(under(&source, Dialect::Gnucobol), "-0012345 +0012345 0012345\n-00012 00012 -0000123456 -00000000000000000005 -0000000125 -00003 01K\n");
    let separate = source.replacen("       IDENTIFICATION", "       CBL DISPSIGN(SEP)\n       IDENTIFICATION", 1);
    assert_eq!(under(&separate, Dialect::Gnucobol), under(&source, Dialect::Gnucobol).replace("01K", "-012"));
}

#[test]
fn display_writes_a_numeric_literal_without_its_decimal_point_under_gnucobol() {
    let source = program("", "", &[line("DISPLAY 1.5 ' ' -1.50 ' ' .5 ' ' +0.25 ' ' 0.0 ' ' 007"), line("GOBACK.")].concat());
    assert_eq!(under(&source, Dialect::Ibm), "1.5 -1.50 .5 +0.25 0.0 007\n");
    assert_eq!(under(&source, Dialect::Gnucobol), "15 -150 5 +025 00 007\n");
    let comma = source
        .replace("DATA DIVISION.", "ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n       SPECIAL-NAMES.\n           DECIMAL-POINT IS COMMA.\n       DATA DIVISION.")
        .replace("1.5 ' ' -1.50 ' ' .5 ' ' +0.25 ' ' 0.0", "1,5 ' ' -1,50 ' ' ,5 ' ' +0,25 ' ' 0,0");
    assert_eq!(under(&comma, Dialect::Ibm), "1,5 -1,50 ,5 +0,25 0,0 007\n");
    assert_eq!(under(&comma, Dialect::Gnucobol), "15 -150 5 +025 00 007\n");
}

#[test]
fn accept_at_the_end_of_sysin_moves_a_space_under_gnucobol() {
    let source = program(
        "",
        "       01  N PIC 9(3) VALUE 7.\n       01  P PIC S9(3) COMP-3 VALUE 7.\n       01  B PIC S9(4) COMP VALUE 7.\n       01  X PIC X(4) VALUE 'QQQQ'.\n       01  E PIC ZZ9 VALUE 5.\n",
        &[line("ACCEPT X"), line("ACCEPT N"), line("ACCEPT P"), line("ACCEPT B"), line("ACCEPT X"), line("ACCEPT E"), line("DISPLAY '[' N '][' P '][' B '][' X '][' E ']'"), line("GOBACK.")].concat(),
    );
    let run = |dialect: Dialect| {
        let flags = [dialect.flag()];
        let walker = Harness::source(&source).flags(&flags).sysin("AB\n").run(Executor::Interpreter);
        let vm = Harness::source(&source).flags(&flags).sysin("AB\n").run(Executor::Vm);
        assert_eq!((&vm.out, &vm.err), (&walker.out, &walker.err));
        (walker.out, walker.err.lines().count())
    };
    assert_eq!(run(Dialect::Ibm), ("[007][007][0007][AB  ][  5]\n".to_owned(), 5));
    assert_eq!(run(Dialect::Gnucobol), ("[000][+000][+00000][    ][  0]\n".to_owned(), 5));
}
