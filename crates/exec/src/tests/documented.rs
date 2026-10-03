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
