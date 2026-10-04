//! INITCHECK: warnings at compile time for items used before a path sets them, and nothing changed
//! at run time (Programming Guide SC27-8714-03, pp. 373-374; assumptions C224 and C225).

use super::*;

fn lax(item: &str, what: &str) -> String {
    format!("warning: IWC0290-W INITCHECK: {item} may be used uninitialized: no path to this statement sets {what} (see C224)")
}

fn strict(item: &str, what: &str) -> String {
    format!("warning: IWC0289-W INITCHECK(STRICT): {item} may be used uninitialized: a path to this statement does not set {what} (see C224)")
}

const YZ: &str = "       01  Y PIC X.\n       01  Z PIC X.\n";

fn example() -> String {
    [line("IF Y > '5'"), line("  MOVE '2' TO Z"), line("END-IF"), line("DISPLAY Z"), line("GOBACK.")].concat()
}

#[test]
fn ibms_example_warns_of_y_under_lax_and_of_y_and_z_under_strict() {
    assert_eq!(compile_errors(&program("INITCHECK", YZ, &example())), lax("Y", "it"));
    assert_eq!(compile_errors(&program("INITCHECK(LAX)", YZ, &example())), lax("Y", "it"));
    assert_eq!(compile_errors(&program("INITCHECK(STRICT)", YZ, &example())), [strict("Y", "it"), strict("Z", "it")].join("\n"));
    assert_eq!(compile_errors(&program("", YZ, &example())), "");
    assert_eq!(compile_errors(&program("NOIC", YZ, &example())), "");
}

#[test]
fn the_warnings_change_nothing_at_run_time() {
    let data = "       01  Y PIC X VALUE '7'.\n       01  Z PIC X VALUE '0'.\n       01  N PIC 9.\n";
    let body = [line("IF Y > '5'"), line("  MOVE '2' TO Z"), line("END-IF"), line("MOVE 3 TO N"), line("DISPLAY Z N"), line("GOBACK.")].concat();
    for card in ["", "INITCHECK", "INITCHECK(STRICT)"] {
        assert_eq!(run(&program(card, data, &body)), "23\n", "{card}");
    }
}

#[test]
fn performed_paragraphs_go_to_and_loops_carry_what_they_set() {
    let data = "       01  X PIC 9.\n       01  Y PIC 9.\n       01  I PIC 9 VALUE 0.\n       01  F PIC X VALUE 'N'.\n";
    let body = [
        "       MAIN-LINE.\n",
        &line("PERFORM SET-X"),
        &line("DISPLAY X"),
        &line("PERFORM UNTIL I > 2"),
        &line("  IF I > 0 DISPLAY Y END-IF"),
        &line("  MOVE I TO Y"),
        &line("  ADD 1 TO I"),
        &line("END-PERFORM"),
        &line("IF F = 'Y' GO TO FINISH END-IF"),
        &line("MOVE 1 TO X."),
        "       FINISH.\n",
        &line("DISPLAY X Y"),
        &line("GOBACK."),
        "       SET-X.\n",
        &line("MOVE 1 TO X."),
    ]
    .concat();
    assert_eq!(compile_errors(&program("INITCHECK", data, &body)), "");
    let lines: Vec<String> = compile_errors(&program("INITCHECK(STRICT)", data, &body)).lines().map(str::to_owned).collect();
    assert_eq!(lines, [strict("Y", "it"), strict("Y", "it")]);
    assert_eq!(run(&program("INITCHECK", data, &body)), "1\n0\n1\n12\n");
}

#[test]
fn a_by_reference_argument_is_not_a_use_and_by_content_and_by_value_ones_are() {
    let data = "       01  P.\n           05 A PIC X.\n           05 B PIC X.\n       01  C PIC X.\n       01  V PIC 9(4) BINARY.\n";
    let body = [line("CALL 'SUB' USING BY REFERENCE A BY CONTENT C BY VALUE V"), line("DISPLAY B"), line("GOBACK.")].concat();
    assert_eq!(compile_errors(&program("INITCHECK", data, &body)), [lax("C", "it"), lax("V", "it")].join("\n"));
}

#[test]
fn linkage_items_are_not_analysed_and_a_group_names_its_first_item_not_set() {
    let source = "       CBL INITCHECK\n       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  G.\n           05 G1 PIC X.\n           05 G2 PIC X.\n       LINKAGE SECTION.\n       01  L PIC X.\n       PROCEDURE DIVISION USING L.\n           MOVE L TO G1\n           DISPLAY G\n           GOBACK.\n";
    assert_eq!(compile_errors(source), lax("G", "G2, which G holds"));
}
