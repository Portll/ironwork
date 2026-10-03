//! Results IBM's manuals state, on the interpreter, in the differential run and on the VM alone.

use super::*;

fn on_both(source: &str) -> String {
    let walker = Harness::source(source).run(Executor::Interpreter);
    assert!(walker.ending.is_ok(), "{:?}\n{}", walker.ending, walker.err);
    let vm = Harness::source(source).run(Executor::Vm);
    assert_eq!((&vm.out, &vm.ending), (&walker.out, &walker.ending), "the VM alone");
    walker.out
}

/// Programming Guide SC27-8714-03, p. 795: A * B has four decimal places, and dividing it by C,
/// with one, carries three, more than dmax's two.
#[test]
fn a_quotient_keeps_the_dividend_s_decimal_places_less_the_divisor_s() {
    let source = program(
        "",
        "       01  A PIC 9V99 VALUE 1.11.\n       01  C PIC 9V9 VALUE 0.7.\n       01  K PIC 9(4) VALUE 1000.\n       01  X PIC 9(5)V99.\n       01  Y PIC 9(5)V99.\n",
        &[line("COMPUTE X = (A * A / C) * K"), line("COMPUTE Y = A * A / C"), line("DISPLAY X ' ' Y"), line("GOBACK.")].concat(),
    );
    assert_eq!(on_both(&source), "0176000 0000176\n");
}

/// Language Reference SC27-8713-03, p. 601: argument-1 - (argument-2 * FUNCTION INTEGER
/// (argument-1 / argument-2)), and the table of 11 and 5 with each sign.
#[test]
fn mod_takes_the_sign_of_its_divisor() {
    let source = program(
        "",
        "       01  R PIC S99 SIGN LEADING SEPARATE.\n       01  D PIC S9 VALUE -5.\n",
        &[
            line("COMPUTE R = FUNCTION MOD(11, 5)"),
            line("DISPLAY R"),
            line("COMPUTE R = FUNCTION MOD(-11, 5)"),
            line("DISPLAY R"),
            line("COMPUTE R = FUNCTION MOD(11, D)"),
            line("DISPLAY R"),
            line("COMPUTE R = FUNCTION MOD(-11, D)"),
            line("DISPLAY R"),
            line("COMPUTE R = FUNCTION MOD(10, D)"),
            line("DISPLAY R"),
            line("GOBACK."),
        ]
        .concat(),
    );
    assert_eq!(on_both(&source), "+01\n+04\n-04\n-01\n+00\n");
}

/// Language Reference SC27-8713-03, p. 225: REDEFINES describes the same storage again, so a
/// LINKAGE record that redefines another is at the argument's address.
#[test]
fn a_linkage_record_that_redefines_another_shares_its_argument() {
    let source = two_programs(
        "       01  A PIC X(4) VALUE 'ABCD'.\n",
        &[line("CALL 'SUB' USING A"), line("DISPLAY A"), line("GOBACK.")].concat(),
        "SUB",
        "       LINKAGE SECTION.\n       01  L-A PIC X(4).\n       01  L-B REDEFINES L-A.\n           05  L-B1 PIC XX.\n           05  L-B2 PIC XX.\n",
        &["       PROCEDURE DIVISION USING L-A.\n", &line("DISPLAY L-B2"), &line("MOVE 'ZZ' TO L-B1"), &line("GOBACK.")].concat(),
    );
    assert_eq!(on_both(&source), "CD\nZZCD\n");
}

/// Language Reference SC27-8713-03, p. 231: a group's SIGN clause applies to its signed zoned items,
/// and a subordinate entry's own SIGN clause takes precedence for that entry.
#[test]
fn a_subordinate_sign_clause_takes_precedence_over_its_group_s() {
    let source = program(
        "",
        "       01  G SIGN TRAILING SEPARATE.\n           03  A PIC S9(3) VALUE -12.\n           03  H SIGN LEADING SEPARATE.\n               05  B PIC S9(3) VALUE -34.\n               05  C PIC S9(3) SIGN TRAILING VALUE -56.\n           03  U PIC 9(3) VALUE 78.\n",
        &[line("DISPLAY G"), line("GOBACK.")].concat(),
    );
    assert_eq!(on_both(&source), "012--03405O078\n");
}

/// Language Reference SC27-8713-03, p. 359, Table 40: a signed zoned item is inspected as if moved
/// to an unsigned item of its length, a separate sign not examined. Its sign stays (assumption C330).
#[test]
fn inspect_examines_a_signed_zoned_item_as_its_unsigned_digits() {
    let source = program(
        "",
        "       01  N PIC S9(5) VALUE -12345.\n       01  L PIC S9(3) SIGN LEADING SEPARATE VALUE -505.\n       01  C1 PIC 99 VALUE 0.\n       01  C2 PIC 99 VALUE 0.\n",
        &[
            line("INSPECT N TALLYING C1 FOR ALL '5' C2 FOR ALL '-'"),
            line("DISPLAY C1 ' ' C2"),
            line("INSPECT N REPLACING ALL '5' BY '7'"),
            line("INSPECT L REPLACING ALL '5' BY '6'"),
            line("DISPLAY N ' ' L"),
            line("GOBACK."),
        ]
        .concat(),
    );
    assert_eq!(on_both(&source), "01 00\n1234P -606\n");
}

/// Language Reference SC27-8713-03, p. 437: VARYING one of the table's own indexes searches with it,
/// and the table's first index is left as it was.
#[test]
fn search_varying_one_of_the_table_s_indexes_searches_with_it() {
    let source = program(
        "",
        "       01  T.\n           05  E PIC X OCCURS 5 INDEXED BY I1 I2.\n       01  N1 PIC 9.\n       01  N2 PIC 9.\n",
        &[
            line("MOVE 'ABCDE' TO T"),
            line("SET I1 TO 4"),
            line("SET I2 TO 2"),
            line("SEARCH E VARYING I2 AT END DISPLAY 'END'"),
            line("    WHEN E(I2) = 'C' DISPLAY 'FOUND'"),
            line("END-SEARCH"),
            line("SET N1 TO I1"),
            line("SET N2 TO I2"),
            line("DISPLAY N1 ' ' N2"),
            line("GOBACK."),
        ]
        .concat(),
    );
    assert_eq!(on_both(&source), "FOUND\n4 3\n");
}

/// Language Reference SC27-8713-03, p. 17: a figurative constant compared with an item is as long as
/// the item, so ALL '01' compared with a one-character item is '0'.
#[test]
fn an_all_literal_compared_with_an_item_is_cut_to_the_item_s_length() {
    let source = program(
        "",
        "       01  D PIC 9 VALUE 0.\n       01  X PIC X VALUE '0'.\n       01  Y PIC XXX VALUE '010'.\n",
        &[
            line("IF ALL '00' NOT > D DISPLAY 'D' END-IF"),
            line("IF X = ALL '01' DISPLAY 'X' END-IF"),
            line("IF Y = ALL '01' DISPLAY 'Y' END-IF"),
            line("GOBACK."),
        ]
        .concat(),
    );
    assert_eq!(on_both(&source), "D\nX\nY\n");
}
