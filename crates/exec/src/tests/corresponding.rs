use super::*;

#[test]
fn move_corresponding_moves_the_pairs_ibms_rules_allow() {
    let data = [
        "       01  A.\n           05  X PIC 99 VALUE 12.\n           05  Y PIC X(3) VALUE 'ABC'.\n",
        "           05  G.\n               10  Z PIC 9 VALUE 7.\n               10  W PIC 9V9 VALUE 1.5.\n",
        "           05  T PIC 9 OCCURS 2.\n           05  R REDEFINES T PIC XX.\n           05  FILLER PIC X VALUE 'F'.\n",
        "           05  N PIC 9V9 VALUE 2.5.\n           05  P PIC 99 VALUE 42.\n",
        "           05  H.\n               10  H1 PIC X VALUE 'h'.\n               10  H2 PIC X VALUE 'i'.\n",
        "       01  B.\n           05  Y PIC X(3).\n           05  X PIC 99.\n",
        "           05  G.\n               10  W PIC 9V9.\n               10  Z PIC X.\n",
        "           05  T PIC 9 OCCURS 2.\n           05  R PIC XX.\n           05  N PIC X(3).\n           05  P PIC A(2).\n",
        "           05  H PIC X(3).\n",
    ]
    .concat();
    let procedure = [line("MOVE ALL '*' TO B"), line("MOVE CORRESPONDING A TO B"), line("DISPLAY B."), line("GOBACK.")].concat();
    assert_eq!(run(&program("", &data, &procedure)), "ABC12157*********hi \n");
}

#[test]
fn add_and_subtract_corresponding_share_rounded_and_size_error() {
    let data = [
        "       01  S.\n           05  A PIC 9V99 VALUE 1.25.\n           05  B PIC 9 VALUE 9.\n",
        "           05  C PIC X VALUE '5'.\n           05  D PIC 9 VALUE 1.\n",
        "       01  R.\n           05  A PIC 9V9 VALUE 1.0.\n           05  B PIC 9 VALUE 5.\n",
        "           05  C PIC 9 VALUE 1.\n           05  D PIC 9 VALUE 2.\n",
    ]
    .concat();
    let procedure = [
        line("ADD CORR S TO R ROUNDED"),
        line("    ON SIZE ERROR DISPLAY 'SIZE ' R"),
        line("    NOT ON SIZE ERROR DISPLAY 'OK'"),
        line("END-ADD"),
        line("SUBTRACT CORRESPONDING S FROM R"),
        line("DISPLAY R."),
        line("GOBACK."),
    ]
    .concat();
    assert_eq!(run(&program("", &data, &procedure)), "SIZE 23513\n10412\n");
}

#[test]
fn corresponding_groups_take_their_subscripts_and_qualifiers() {
    let data = [
        "       01  TAB.\n           05  E OCCURS 2.\n               10  K PIC 9.\n               10  V PIC X.\n",
        "       01  ONE.\n           05  K PIC 9 VALUE 4.\n           05  V PIC X VALUE 'Q'.\n",
        "       01  I PIC 9 VALUE 2.\n",
    ]
    .concat();
    let procedure = [line("INITIALIZE TAB"), line("MOVE CORR ONE TO E OF TAB (I)"), line("ADD CORR ONE TO E (1)"), line("DISPLAY TAB."), line("GOBACK.")].concat();
    assert_eq!(run(&program("", &data, &procedure)), "4 4Q\n");
}

#[test]
fn corresponding_names_two_groups() {
    let data = "       01  A.\n           05  X PIC 9.\n           88  X-ON VALUE 1.\n       01  B.\n           05  X PIC 9.\n       01  E PIC 9.\n";
    let errors = compile_errors(&program("", data, &[line("MOVE CORR E TO B"), line("ADD CORR A TO X-ON"), line("MOVE CORR A(1:1) TO B.")].concat()));
    assert!(errors.contains("MOVE CORRESPONDING E: not a group item"), "{errors}");
    assert!(errors.contains("ADD CORRESPONDING X-ON: a condition-name, not a group item"), "{errors}");
    assert!(errors.contains("MOVE CORRESPONDING A: reference-modified"), "{errors}");
}
