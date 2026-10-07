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

/// Language Reference SC27-8713-03, p. 499, and Programming Guide SC27-8714-03, p. 56: an integer or
/// numeric function can be used only where an arithmetic expression can, which DISPLAY's operands
/// are not. COMPUTE gives the value to an item DISPLAY shows.
#[test]
fn display_refuses_a_numeric_function_and_shows_the_item_compute_gives_it_to() {
    let refused = program("", "", &[line("DISPLAY FUNCTION INTEGER(-2.5) FUNCTION UPPER-CASE('a')"), line("GOBACK.")].concat());
    let errors = compile_errors(&refused);
    assert!(errors.contains("DISPLAY FUNCTION INTEGER: an integer or numeric function can be used only where an arithmetic expression can"), "{errors}");
    assert!(!errors.contains("UPPER-CASE"), "{errors}");
    let source = program(
        "",
        "       01  R PIC S9(3)V9 SIGN LEADING SEPARATE.\n",
        &[line("COMPUTE R = FUNCTION INTEGER(-2.5)"), line("DISPLAY R ' ' FUNCTION UPPER-CASE('a')"), line("GOBACK.")].concat(),
    );
    assert_eq!(on_both(&source), "-0030 A\n");
}

/// Language Reference SC27-8713-03, pp. 591 and 599: MAX and MIN of numeric arguments are numeric
/// functions, refused in DISPLAY as the others are; of alphanumeric arguments they are shown.
#[test]
fn display_refuses_max_and_min_of_numeric_arguments() {
    let refused = program(
        "",
        "       01  N PIC S9V9 VALUE -1.5.\n",
        &[line("DISPLAY FUNCTION MAX(-3 -5) FUNCTION MIN(N 2)"), line("    FUNCTION MAX('AB' 'B')"), line("GOBACK.")].concat(),
    );
    let errors = compile_errors(&refused);
    assert!(errors.contains("DISPLAY FUNCTION MAX: an integer or numeric function"), "{errors}");
    assert!(errors.contains("DISPLAY FUNCTION MIN: an integer or numeric function"), "{errors}");
    assert_eq!(errors.matches("DISPLAY FUNCTION").count(), 2, "{errors}");
    let source = program("", "", &[line("DISPLAY FUNCTION MAX('AB' 'B') FUNCTION MIN('AB' 'B')"), line("GOBACK.")].concat());
    assert_eq!(on_both(&source), "BAB\n");
}

/// Programming Guide SC27-8714-03, p. 119: "numeric functions are not valid as senders in MOVE
/// statements", whatever the receiver; MAX and MIN are when their arguments are numeric, and
/// alphanumeric functions are valid senders (Language Reference SC27-8713-03, pp. 402, 591).
/// COMPUTE gives the value to an item.
#[test]
fn move_refuses_a_numeric_function_whatever_the_receiver() {
    let data = "       01  N PIC 9(3).\n       01  X PIC X(3).\n";
    let statements = ["MOVE FUNCTION ORD-MAX(3 9 1) TO N", "MOVE FUNCTION NUMVAL('12') TO X", "MOVE FUNCTION MAX(N 2) TO N", "MOVE FUNCTION UPPER-CASE('a') TO X", "MOVE FUNCTION MAX('a' 'b') TO X", "GOBACK."];
    let errors = compile_errors(&program("", data, &statements.map(line).concat()));
    for name in ["ORD-MAX", "NUMVAL", "MAX"] {
        assert!(errors.contains(&format!("MOVE FUNCTION {name}: an integer or numeric function can be used only where an arithmetic expression can, not as a MOVE's sender")), "{errors}");
    }
    assert_eq!(errors.matches("MOVE FUNCTION").count(), 3, "{errors}");
    let source = program("", data, &[line("COMPUTE N = FUNCTION ORD-MAX(3 9 1)"), line("MOVE FUNCTION UPPER-CASE('a') TO X"), line("DISPLAY N ' ' X"), line("GOBACK.")].concat());
    assert_eq!(on_both(&source), "002 A  \n");
}

/// Language Reference SC27-8713-03, p. 322: BY VALUE is specified for both the argument and the
/// parameter, and the manual gives no result when the parameter is received BY REFERENCE. The
/// parameter gets storage of its own holding the value (assumption C333).
#[test]
fn a_by_value_argument_to_a_by_reference_parameter_gives_it_a_copy() {
    let source = two_programs(
        "       01  N PIC S9(9) BINARY VALUE 42.\n",
        &[line("CALL 'SUB' USING BY VALUE N"), line("DISPLAY N"), line("GOBACK.")].concat(),
        "SUB",
        "       LINKAGE SECTION.\n       01  L PIC S9(9) BINARY.\n",
        &["       PROCEDURE DIVISION USING L.\n", &line("DISPLAY L"), &line("ADD 1 TO L"), &line("GOBACK.")].concat(),
    );
    assert_eq!(on_both(&source), "000000042\n000000042\n");
}

/// Programming Guide SC27-8714-03, pp. 796 and 800: an exponent with decimal places, or one holding
/// a division when dmax is above zero, makes the expression floating point, as a floating-point
/// function does. Language Reference SC27-8713-03, pp. 266 and 296-297: 4 ** 0.5 is +2, zero to a
/// negative power is a size error, and a negative base to a fractional power is taken as its
/// absolute value (assumption C334).
#[test]
fn an_exponent_with_decimal_places_is_evaluated_in_floating_point() {
    let source = program(
        "",
        "       01  W PIC S9(5)V9(7).\n       01  X PIC S9V9(6) SIGN LEADING SEPARATE.\n       01  A PIC 9 VALUE 2.\n       01  N PIC S9 VALUE -4.\n       01  Z PIC 9 VALUE 0.\n       01  D COMP-2 VALUE -2.\n",
        &[
            line("COMPUTE W ROUNDED = FUNCTION SQRT(10) ** 2"),
            line("DISPLAY W"),
            line("COMPUTE X = 4 ** 0.5"),
            line("DISPLAY X"),
            line("COMPUTE X = A ** 0.5"),
            line("DISPLAY X"),
            line("COMPUTE X ROUNDED = 8 ** (1 / 3)"),
            line("DISPLAY X"),
            line("COMPUTE X = D ** 3"),
            line("DISPLAY X"),
            line("COMPUTE X = N ** 0.5"),
            line("DISPLAY X"),
            line("COMPUTE X = Z ** -1.5"),
            line("    ON SIZE ERROR DISPLAY 'SIZE'"),
            line("END-COMPUTE"),
            line("DISPLAY X"),
            line("GOBACK."),
        ]
        .concat(),
    );
    assert_eq!(on_both(&source), "00010000000{\n+2000000\n+1414214\n+2000000\n-8000000\n+2000000\nSIZE\n+2000000\n");
}

/// Programming Guide, 'Fixed-point data and intermediate results': an integral exponent multiplies
/// the base by itself |n| - 1 times at dmax places, a negative one then divides 1 by that power, and
/// an exponent of more than nine digits keeps nine. Zero to a negative power is IGZ0050S, which ON
/// SIZE ERROR takes; a power that truncates to zero under a negative exponent is IGZ0222S (C334).
#[test]
fn an_integral_exponent_in_fixed_point_may_be_negative_or_past_31() {
    let data = "       01  R PIC 9(13)V9(4).\n       01  A PIC 9 VALUE 2.\n       01  Z PIC 9 VALUE 0.\n       01  N PIC S9(10) VALUE -2.\n       01  B PIC S9(10) VALUE 40.\n       01  T PIC S9(10) VALUE 1000000002.\n       01  F PIC V9 VALUE .1.\n       01  G PIC 9V9.\n       01  M PIC S99 VALUE -40.\n";
    let powers = [
        line("COMPUTE R = A ** N"),
        line("DISPLAY R"),
        line("COMPUTE R = A ** B"),
        line("DISPLAY R"),
        line("COMPUTE R = A ** T"),
        line("DISPLAY R"),
        line("COMPUTE R = Z ** N"),
        line("    ON SIZE ERROR DISPLAY 'SIZE'"),
        line("END-COMPUTE"),
        line("DISPLAY R"),
    ]
    .concat();
    assert_eq!(on_both(&program("", data, &[powers.as_str(), &line("GOBACK.")].concat())), "00000000000002500\n10995116277760000\n00000000000040000\nSIZE\n00000000000040000\n");
    for (statement, ending) in [
        ("COMPUTE R = Z ** N.", "IGZ0050S A zero base was raised to a negative power in an exponentiation expression."),
        ("COMPUTE G = F ** M.", "IGZ0222S No significant digits remain in a fixed-point exponentiation operation due to excessive decimal positions specified in the operands or receivers."),
    ] {
        let source = program("", data, &line(statement));
        let walker = Harness::source(&source).run(Executor::Interpreter);
        let vm = Harness::source(&source).run(Executor::Vm);
        assert_eq!(vm.ending, walker.ending, "{statement}");
        let abend = walker.ending.unwrap_err();
        assert_eq!((abend.code.to_string(), abend.message.as_str()), ("U4038".to_owned(), ending), "{statement}");
    }
}

/// Language Reference SC27-8713-03, p. 246: a numeric item's VALUE literal must be numeric; a
/// numeric-edited item's is alphanumeric, and a figurative constant stands for either.
#[test]
fn a_numeric_item_refuses_a_value_literal_that_is_not_numeric() {
    let refused = program("", "       01  A PIC 99 VALUE \"7\".\n       01  B PIC 9(3) COMP-3 VALUE N'1'.\n", &line("GOBACK."));
    let errors = compile_errors(&refused);
    assert!(errors.contains("VALUE of A: an alphanumeric literal, where a numeric item's VALUE literal must be numeric"), "{errors}");
    assert!(errors.contains("VALUE of B: a national literal"), "{errors}");
    let source = program(
        "",
        "       01  A PIC 99 VALUE 7.\n       01  Z PIC 99 VALUE ZERO.\n       01  E PIC ZZ9 VALUE '  7'.\n",
        &[line("DISPLAY A Z E"), line("GOBACK.")].concat(),
    );
    assert_eq!(on_both(&source), "0700  7\n");
}

/// Language Reference SC27-8713-03, pp. 126 and 334: DISPLAY UPON names the environment-name of an
/// output device, SYSOUT, SYSLIST, SYSLST, SYSPUNCH, SYSPCH or CONSOLE, or a SPECIAL-NAMES
/// mnemonic-name for one.
#[test]
fn display_upon_takes_an_output_device_or_a_mnemonic_name_for_one() {
    let source = |upon: &str| {
        let head = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n       SPECIAL-NAMES.\n           CONSOLE IS CRT\n           SYSIN IS INP.\n       PROCEDURE DIVISION.\n";
        format!("{head}{}{}", line(&format!("DISPLAY 'A' UPON {upon}")), line("GOBACK."))
    };
    for upon in ["SYSOUT", "SYSLST", "SYSPCH", "CONSOLE", "CRT"] {
        assert_eq!(on_both(&source(upon)), "A\n", "{upon}");
    }
    let errors = compile_errors(&source("SYSERR"));
    assert!(errors.contains("DISPLAY UPON SYSERR: neither an environment-name DISPLAY writes to"), "{errors}");
    let errors = compile_errors(&source("INP"));
    assert!(errors.contains("DISPLAY UPON INP: a mnemonic-name for SYSIN, which DISPLAY does not write to"), "{errors}");
}

/// Language Reference SC27-8713-03, p. 333, and Programming Guide SC27-8714-03, p. 36: national data
/// DISPLAY writes elsewhere than the console is its UTF-16 bytes, unconverted; UPON CONSOLE converts
/// it to the code page, a character the page lacks becoming the substitution character.
#[test]
fn display_converts_national_data_only_upon_the_console() {
    let source = program(
        "",
        "       01  N PIC N(2) VALUE N'AB'.\n",
        &[
            line("DISPLAY N N'AB' FUNCTION NATIONAL-OF('AB')"),
            line("DISPLAY N N'AB' FUNCTION NATIONAL-OF('AB') UPON CONSOLE"),
            line("DISPLAY FUNCTION DISPLAY-OF(N) N'\u{65e5}' UPON CONSOLE"),
            line("GOBACK."),
        ]
        .concat(),
    );
    let raw = "\0\u{a0}\0\u{e2}";
    assert_eq!(on_both(&source), format!("{raw}{raw}{raw}\nABABAB\nAB\u{1a}\n"));
}
