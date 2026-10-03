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
