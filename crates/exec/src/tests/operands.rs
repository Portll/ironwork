//! What an operand may be (compile/src/operands.rs, and a condition-name as data in compile's
//! `Check::reference`): each rule broken is a severe compile error, and what the rule allows
//! compiles.

use super::*;

fn refused_in(data: &str) -> impl Fn(&str) -> Vec<String> + '_ {
    move |body: &str| severe(&program("", data, &[line(body), line("GOBACK.")].concat()))
}

#[test]
fn all_takes_an_alphanumeric_or_national_literal_or_a_figurative_constant() {
    let refused = refused_in("       01  X PIC X(4).\n       01  N PIC N(4) USAGE NATIONAL.\n");
    let rule = "the literal after ALL is alphanumeric, national or a figurative constant other than ALL";
    assert_eq!(refused("MOVE ALL 5 TO X"), [format!("ALL 5: {rule}")]);
    assert_eq!(refused("IF X = ALL ALL 'A' CONTINUE END-IF"), [format!("ALL ALL: {rule}")]);
    for accepted in ["MOVE ALL 'AB' TO X", "MOVE ALL N'AB' TO N", "MOVE ALL SPACES TO X", "IF N = ALL N'A' CONTINUE END-IF"] {
        assert_eq!(refused(accepted), Vec::<String>::new(), "{accepted}");
    }
}

#[test]
fn a_condition_name_is_not_a_data_item() {
    let refused = refused_in("       01  F PIC X.\n           88 YES VALUE 'Y'.\n       01  X PIC X.\n");
    assert_eq!(refused("MOVE YES TO X"), ["YES is a condition-name, not a data item"]);
    assert_eq!(refused("DISPLAY YES"), ["YES is a condition-name, not a data item"]);
    assert_eq!(refused("MOVE 'Y' TO YES"), ["YES is a condition-name, not a data item"]);
    for accepted in ["IF YES CONTINUE END-IF", "SET YES TO TRUE", "EVALUATE TRUE WHEN YES CONTINUE END-EVALUATE", "IF F = 'N' OR YES CONTINUE END-IF"] {
        assert_eq!(refused(accepted), Vec::<String>::new(), "{accepted}");
    }
}

#[test]
fn an_arithmetic_expression_or_a_numeric_function_is_compared_only_with_a_number() {
    let refused = refused_in("       01  N PIC 9 VALUE 1.\n       01  X PIC X.\n       01  E PIC Z9.\n");
    let rule = "an arithmetic expression or a numeric function is compared only with a numeric operand";
    assert_eq!(refused("IF N + 1 = X CONTINUE END-IF"), [format!("an arithmetic expression compared with X: {rule}")]);
    assert_eq!(refused("IF N + 1 = 'A' CONTINUE END-IF"), [format!("an arithmetic expression compared with an alphanumeric literal: {rule}")]);
    assert_eq!(refused("IF SPACE = N * 2 CONTINUE END-IF"), [format!("SPACE compared with an arithmetic expression: {rule}")]);
    assert_eq!(refused("IF FUNCTION LENGTH(X) = E CONTINUE END-IF"), [format!("FUNCTION LENGTH compared with E: {rule}")]);
    assert_eq!(refused("EVALUATE N + 1 WHEN 'A' CONTINUE END-EVALUATE"), [format!("an arithmetic expression compared with an alphanumeric literal: {rule}")]);
    for accepted in ["IF N + 1 = 2 CONTINUE END-IF", "IF N + 1 = ZERO CONTINUE END-IF", "IF N = X CONTINUE END-IF", "IF FUNCTION UPPER-CASE(X) = 'A' CONTINUE END-IF", "EVALUATE N + 1 WHEN 2 CONTINUE END-EVALUATE"] {
        assert_eq!(refused(accepted), Vec::<String>::new(), "{accepted}");
    }
}

#[test]
fn function_arguments_are_of_the_kinds_and_numbers_the_function_takes() {
    let data = "       01  A PIC X VALUE 'B'.\n       01  N PIC 9 VALUE 2.\n       01  P POINTER.\n       01  R PIC 9.\n       01  C PIC 9 VALUE 2.\n       01  G.\n           05 T PIC 9 OCCURS 1 TO 3 DEPENDING ON C.\n";
    let refused = refused_in(data);
    assert_eq!(refused("MOVE FUNCTION MAX(A 1) TO A"), ["FUNCTION MAX: alphanumeric and numeric arguments, where all must be of the same class"]);
    assert_eq!(refused("COMPUTE R = FUNCTION SQRT(T(ALL))"), ["FUNCTION SQRT: an ALL subscript stands for a varying number of arguments, and SQRT takes 1"]);
    assert_eq!(refused("COMPUTE R = FUNCTION MAX(N ZERO)"), ["FUNCTION MAX: a figurative constant is an argument only inside an arithmetic expression"]);
    assert_eq!(refused("COMPUTE R = FUNCTION MIN(P N)"), ["FUNCTION MIN: P is a pointer or object reference, where an argument is alphabetic, alphanumeric, national or numeric"]);
    for accepted in ["COMPUTE R = FUNCTION MAX(T(ALL) N)", "MOVE FUNCTION MAX(A 'C') TO A", "COMPUTE R = FUNCTION SUM(N, ZERO + 1)", "COMPUTE R = FUNCTION NUMVAL('1')"] {
        assert_eq!(refused(accepted), Vec::<String>::new(), "{accepted}");
    }
}

#[test]
fn a_character_function_takes_no_numeric_argument() {
    let refused = refused_in("       01  N PIC 9(3) VALUE 5.\n       01  E PIC Z9.\n       01  X PIC X(3).\n");
    let rule = |f: &str, a: &str| format!("FUNCTION {f}: {a} is numeric, where {f} takes an alphabetic, alphanumeric or national argument");
    assert_eq!(refused("DISPLAY FUNCTION TRIM(N)"), [rule("TRIM", "N")]);
    assert_eq!(refused("IF FUNCTION TRIM(N) = 'A' CONTINUE END-IF"), [rule("TRIM", "N")]);
    assert_eq!(refused("MOVE FUNCTION REVERSE(N + 1) TO X"), [rule("REVERSE", "an arithmetic expression")]);
    assert_eq!(refused("MOVE FUNCTION UPPER-CASE(FUNCTION NUMVAL(X)) TO X"), [rule("UPPER-CASE", "FUNCTION NUMVAL")]);
    assert_eq!(refused("MOVE FUNCTION LOWER-CASE(5) TO X"), [rule("LOWER-CASE", "a numeric literal")]);
    for accepted in ["DISPLAY FUNCTION TRIM(E)", "MOVE FUNCTION REVERSE(N(1:2)) TO X", "IF FUNCTION UPPER-CASE(X) = 'A' CONTINUE END-IF", "COMPUTE N = FUNCTION MAX(N 1)"] {
        assert_eq!(refused(accepted), Vec::<String>::new(), "{accepted}");
    }
}
