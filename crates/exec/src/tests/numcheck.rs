use super::*;
use rt::store::LaxRedefinition;
use std::collections::BTreeSet;

/// R holds Z, a zoned item whose first byte is a space: not NUMERIC, while PACK, which keeps only
/// digits and the sign's zone, still reads it as 12.
const DATA: &str = concat!(
    "       01  R.\n           05 Z PIC 999.\n       01  W PIC 999.\n       01  X PIC X(3).\n",
    "       01  RP.\n           05 P PIC 999 COMP-3.\n       01  RE.\n           05 E PIC 99 COMP-3.\n",
    "       01  B PIC 99 COMP VALUE 0.\n       01  BX REDEFINES B PIC XX.\n       01  B5 PIC 99 COMP-5 VALUE 0.\n",
    "       01  B5X REDEFINES B5 PIC XX.\n       01  A PIC X(3) VALUE ' 12'.\n       01  S PIC S999.\n       01  SX REDEFINES S PIC X(3).\n",
);

fn ran(card: &str, statements: &[&str]) -> (String, String, Result<Ending, Abend>) {
    let mut body: Vec<String> = vec![line("MOVE ' 12' TO R")];
    body.extend(statements.iter().map(|s| line(s)));
    body.push(line("GOBACK."));
    run_with(&program(card, DATA, &body.concat()), &[])
}

fn messages(err: &str) -> Vec<&str> {
    err.lines().filter_map(|l| l.split_once("NUMCHECK: ").map(|(_, m)| m)).collect()
}

#[test]
fn a_zoned_sender_that_is_not_numeric_is_reported_and_the_statement_runs_under_msg() {
    let (out, err, ending) = ran("NUMCHECK", &["COMPUTE W = Z + 1", "DISPLAY W"]);
    assert!(ending.is_ok(), "{ending:?}");
    assert_eq!(out, "013\n");
    assert_eq!(messages(&err), ["Z X'40F1F2' in program T is not NUMERIC; the statement runs"]);
    let (out, err, _) = ran("", &["COMPUTE W = Z + 1", "DISPLAY W"]);
    assert_eq!((out.as_str(), messages(&err).len()), ("013\n", 0), "NONUMCHECK tests nothing");
}

#[test]
fn abd_ends_the_run_with_u4038_before_the_statement() {
    let (out, _, ending) = ran("NUMCHECK(ABD)", &["COMPUTE W = Z + 1", "DISPLAY W"]);
    let abend = ending.unwrap_err();
    assert_eq!((abend.code, abend.message.as_str()), (AbendCode::user(4038), "NUMCHECK: Z X'40F1F2' in program T is not NUMERIC"));
    assert_eq!(out, "");
}

#[test]
fn a_receiver_that_is_also_a_sender_is_tested_and_a_class_test_or_display_is_not() {
    let (_, err, _) = ran("NC", &["ADD 1 TO Z", "IF Z NUMERIC DISPLAY 'N' END-IF", "DISPLAY Z"]);
    assert_eq!(messages(&err).len(), 1, "{err}");
    let (_, err, _) = ran("NC", &["MOVE 5 TO Z", "COMPUTE W = Z"]);
    assert_eq!(messages(&err).len(), 0, "a receiver alone is not tested");
}

#[test]
fn packed_senders_are_tested_for_their_sign_and_an_even_digit_count_s_spare_half_byte() {
    let statements = ["MOVE X'123C' TO RP", "MOVE X'112F' TO RE", "COMPUTE W = P + 1", "COMPUTE W = E + 1", "DISPLAY W"];
    let (out, err, ending) = ran("NUMCHECK(PAC)", &statements);
    assert!(ending.is_ok(), "{ending:?}");
    assert_eq!(out, "113\n", "E reads as 112 whatever its spare half-byte holds");
    assert_eq!(messages(&err), ["P X'123C' in program T is not NUMERIC; the statement runs", "E X'112F' in program T is not NUMERIC; the statement runs"]);
    let (_, err, _) = ran("NUMCHECK(NOPAC)", &statements);
    assert_eq!(messages(&err).len(), 0, "{err}");
}

#[test]
fn binary_senders_are_tested_for_digits_beyond_their_picture_unless_comp_5_or_notruncbin_under_trunc_bin() {
    let statements = ["MOVE X'0096' TO BX", "MOVE X'0096' TO B5X", "COMPUTE W = B + B5"];
    let (_, err, _) = ran("NUMCHECK(BIN)", &statements);
    assert_eq!(messages(&err), ["B X'0096' in program T has more digits than its PICTURE allows; the statement runs"]);
    let (_, err, _) = ran("NUMCHECK(BIN(NOTRUNCBIN)),TRUNC(BIN)", &statements);
    assert_eq!(messages(&err).len(), 0, "{err}");
    let (_, err, _) = ran("NUMCHECK(BIN),TRUNC(BIN)", &statements);
    assert_eq!(messages(&err).len(), 1, "BIN(TRUNCBIN) tests under TRUNC(BIN) too: {err}");
}

#[test]
fn noalphnum_leaves_a_zoned_item_compared_with_an_alphanumeric_operand_untested() {
    let statements = ["IF Z = 'ABC' DISPLAY 'EQ' END-IF", "IF Z = X DISPLAY 'EQ' END-IF", "IF Z = 12 DISPLAY 'TWELVE' END-IF"];
    let (_, err, _) = ran("NUMCHECK(ZON)", &statements);
    assert_eq!(messages(&err).len(), 3, "{err}");
    let (out, err, _) = ran("NUMCHECK(ZON(NOALPHNUM))", &statements);
    assert_eq!((out.as_str(), messages(&err).len()), ("TWELVE\n", 1), "{err}");
}

#[test]
fn moves_test_an_alphanumeric_sender_to_a_numeric_receiver_and_lax_spares_a_zoned_sender_to_a_zoned_or_alphanumeric_one() {
    let (_, err, _) = ran("NUMCHECK(ZON)", &["MOVE ' 12' TO A", "MOVE A TO W", "MOVE Z TO X", "MOVE Z TO W"]);
    assert_eq!(messages(&err), ["A X'40F1F2' in program T is not NUMERIC; the statement runs", "Z X'40F1F2' in program T is not NUMERIC; the statement runs", "Z X'40F1F2' in program T is not NUMERIC; the statement runs"]);
    let (_, err, _) = ran("NUMCHECK(ZON(LAX))", &["MOVE ' 12' TO A", "MOVE A TO W", "MOVE Z TO X", "MOVE Z TO W"]);
    assert_eq!(messages(&err), ["A X'40F1F2' in program T is not NUMERIC; the statement runs"]);
}

#[test]
fn zonecheck_tests_zoned_senders_only() {
    let statements = ["MOVE X'123C' TO RP", "COMPUTE W = P + Z"];
    let (_, err, _) = ran("ZONECHECK(MSG)", &statements);
    assert_eq!(messages(&err), ["Z X'40F1F2' in program T is not NUMERIC; the statement runs"]);
    assert!(ran("ZC(ABD)", &statements).2.is_err());
}

#[test]
fn cleansign_cleans_a_sign_before_numcheck_tests_it() {
    let statements = ["MOVE X'F1F203' TO SX", "COMPUTE W = S + 1"];
    let (_, err, _) = ran("NUMCHECK", &statements);
    assert_eq!(messages(&err), ["S X'F1F203' in program T is not NUMERIC; the statement runs"]);
    let (_, err, _) = ran("NUMCHECK,INVDATA(CLEANSIGN)", &statements);
    assert_eq!(messages(&err).len(), 0, "{err}");
}

/// The Programming Guide's two ZON(LAX) examples (pp. 390-391), with W a receiver.
const REDEFINED: &str = concat!(
    "       01  NUM1 PIC S9(8).\n       01  NUM2 REDEFINES NUM1.\n           03 NUM2-PART1 PIC 9(4).\n",
    "           03 NUM2-PART2 PIC 9(2).\n           03 NUM2-PART3 PIC 9(2).\n",
    "       01  NUMED PIC ZZ99.99.\n       01  NUM REDEFINES NUMED.\n           03 INTVAL PIC 9(4).\n",
    "           03 FILLER PIC X.\n           03 DECVAL PIC 9(2).\n       01  W PIC 9(4).\n",
);

fn ran_redefined(card: &str) -> Vec<String> {
    let statements = [
        "MOVE -12345678 TO NUM1",
        "COMPUTE W = NUM2-PART3 + NUM2-PART2",
        "MOVE 5.25 TO NUMED",
        "COMPUTE W = INTVAL + DECVAL",
        "MOVE X'F140F1F2' TO NUM(1:4)",
        "COMPUTE W = INTVAL",
        "MOVE X'F1F240F2' TO NUM(1:4)",
        "COMPUTE W = INTVAL",
        "GOBACK.",
    ];
    let body: String = statements.iter().map(|s| line(s)).collect();
    let (_, err, ending) = run_with(&program(card, REDEFINED, &body), &[]);
    assert!(ending.is_ok(), "{ending:?}");
    messages(&err).into_iter().map(str::to_owned).collect()
}

#[test]
fn lax_tests_an_unsigned_item_over_a_signed_one_as_signed_and_spares_spaces_over_an_edited_item_s_leading_z_positions() {
    let not_numeric = |item: &str, hex: &str| format!("{item} X'{hex}' in program T is not NUMERIC; the statement runs");
    assert_eq!(
        ran_redefined("NUMCHECK(ZON)"),
        [not_numeric("NUM2-PART3", "F7D8"), not_numeric("INTVAL", "4040F0F5"), not_numeric("INTVAL", "F140F1F2"), not_numeric("INTVAL", "F1F240F2")]
    );
    assert_eq!(ran_redefined("NUMCHECK(ZON(LAX))"), [not_numeric("INTVAL", "F1F240F2")], "a space past the Z positions is still reported");
}

#[test]
fn lax_tolerances_come_from_level_01_redefinitions_of_signed_trailing_or_edited_items_outside_tables() {
    let data = concat!(
        "       01  S1 PIC S999.\n       01  R1 REDEFINES S1.\n           05 R1A PIC 9.\n           05 R1B PIC 99.\n",
        "       01  S2 PIC S999 SIGN LEADING.\n       01  R2 REDEFINES S2 PIC 999.\n",
        "       01  E1 PIC ZZ,ZZ9.99.\n       01  R3 REDEFINES E1 PIC 9(6).\n",
        "       01  E2 PIC $ZZ9.\n       01  R4 REDEFINES E2 PIC 9(4).\n",
        "       01  E3 PIC ZZ,999.\n       01  R5 REDEFINES E3 PIC S9(6).\n",
        "       01  E4 PIC ZZ9.\n       01  R6 REDEFINES E4.\n           05 R6T PIC 9 OCCURS 3.\n",
        "       01  E5 PIC ZZ9.\n       01  R7 REDEFINES E5 PIC S999 SIGN TRAILING SEPARATE.\n",
    );
    let c = compiled(data);
    let lax = |name: &str| c.layout.numcheck.lax(c.layout.items.iter().position(|i| i.name.as_deref() == Some(name)).unwrap());
    assert_eq!(lax("R1A"), None, "its last byte is not S1's");
    assert_eq!(lax("R1B"), Some(LaxRedefinition::Signed));
    assert_eq!(lax("R2"), None, "S2's sign leads");
    assert_eq!(lax("R3"), Some(LaxRedefinition::LeadingSpaces(5)));
    assert_eq!(lax("R4"), None, "E2 starts with its currency sign");
    assert_eq!(lax("R5"), Some(LaxRedefinition::LeadingSpaces(2)), "the comma after the last Z is not counted");
    assert_eq!(lax("R6T"), None, "an item in a table");
    assert_eq!(lax("R7"), None, "a separate sign");
}

fn compiled(data: &str) -> Compiled {
    compile(syntax::parse(&program("NUMCHECK(ZON(LAX))", data, &line("GOBACK."))).unwrap_or_else(|e| panic!("{e}")), &[]).unwrap_or_else(|e| panic!("{e:?}"))
}

/// Items that hold what a VALUE clause gives them for the whole run: Z and P invalid, A not an
/// integer's digits, D an integer's, HN not NUMERIC, K valid; GR shares Z's storage.
const CONSTANT: &str = concat!(
    "       01  G VALUE ' 12'.\n           05 Z PIC 999.\n       01  GR REDEFINES G PIC X(3).\n       01  GP VALUE X'123C'.\n           05 P PIC 999 COMP-3.\n",
    "       01  A PIC X(3) VALUE 'ABC'.\n       01  D PIC X(3) VALUE '123'.\n       01  H VALUE 'A'.\n           05 HN PIC 9.\n",
    "               88 HN-ONE VALUE 1.\n       01  K PIC 999 VALUE 2.\n       01  W PIC 999.\n       01  X PIC X(3).\n",
);

const READS: [&str; 13] = [
    "COMPUTE W = Z + 1",
    "ADD P TO W",
    "MOVE Z TO W",
    "MOVE A TO W",
    "MOVE D TO W",
    "MOVE A TO X",
    "IF Z > 5 DISPLAY 'GT' END-IF",
    "IF Z = ZERO DISPLAY 'Z0' END-IF",
    "IF Z = K DISPLAY 'EQ' END-IF",
    "IF HN-ONE DISPLAY 'ONE' END-IF",
    "PERFORM UNTIL Z > 5 CONTINUE END-PERFORM",
    "DISPLAY Z",
    "IF Z NUMERIC DISPLAY 'N' END-IF",
];

/// The program reading the items, and with `tail` after its GOBACK in a paragraph nothing runs.
fn reading(card: &str, tail: &[&str]) -> String {
    let mut body: String = READS.iter().map(|s| line(s)).collect();
    body.push_str(&line("GOBACK."));
    if !tail.is_empty() {
        body.push_str("       NEVER.\n");
        body.extend(tail.iter().map(|s| line(s)));
    }
    program(card, CONSTANT, &body)
}

fn compiled_messages(source: &str) -> Vec<(String, String)> {
    let c = compile(syntax::parse(source).unwrap_or_else(|e| panic!("{e}")), &[]).unwrap_or_else(|e| panic!("{e:?}"));
    c.diagnostics.iter().filter(|d| d.message.starts_with("NUMCHECK: ")).map(|d| (d.pos.to_string(), d.message.clone())).collect()
}

/// Each run-time report's position and item.
fn reported(err: &str) -> BTreeSet<(String, String)> {
    err.lines()
        .filter_map(|l| l.strip_prefix("ironwork: ")?.split_once(": NUMCHECK: "))
        .map(|(pos, m)| (pos.to_owned(), m.split(' ').next().unwrap_or_default().to_owned()))
        .collect()
}

#[test]
fn a_test_that_always_fails_is_an_error_when_compiled_and_is_removed_where_the_interpreter_makes_it() {
    let found = compiled_messages(&reading("NUMCHECK", &[]));
    let at: BTreeSet<(String, String)> = found.iter().map(|(pos, m)| (pos.clone(), m["NUMCHECK: ".len()..].split(' ').next().unwrap_or_default().to_owned())).collect();
    assert_eq!(at.len(), found.len());
    assert_eq!(
        found.iter().map(|(_, m)| m.as_str()).find(|m| m.starts_with("NUMCHECK: A ")),
        Some("NUMCHECK: A is not NUMERIC wherever this statement reads it: its VALUE clauses give it X'C1C2C3' and no statement changes it, so the test is removed (see C281)")
    );
    let (_, err, ending) = run_with(&reading("NUMCHECK", &[]), &[]);
    assert!(ending.is_ok(), "{ending:?}");
    assert_eq!(reported(&err), BTreeSet::new(), "every test made here was removed");
    let (_, err, ending) = run_with(&reading("NUMCHECK", &["MOVE SPACES TO G GP H A"]), &[]);
    assert!(ending.is_ok(), "{ending:?}");
    assert_eq!(reported(&err), at, "where the items can change, the interpreter tests exactly the references the compiler named");
    assert_eq!(compiled_messages(&reading("NUMCHECK", &["MOVE SPACES TO G GP H A"])), []);
    assert_eq!(compiled_messages(&reading("", &[])), [], "NONUMCHECK finds nothing");
}

#[test]
fn an_item_any_statement_may_set_is_not_taken_as_constant() {
    let setters = [
        "MOVE 1 TO Z",
        "MOVE SPACES TO G",
        "INITIALIZE G",
        "ACCEPT Z",
        "CALL 'SUB' USING G",
        "CALL 'SUB' USING BY REFERENCE Z",
        "SET ADDRESS OF L TO ADDRESS OF G",
        "PERFORM NEVER VARYING Z FROM 1 BY 1 UNTIL Z > 3",
        "STRING 'A' DELIMITED BY SIZE INTO G",
        "INSPECT G REPLACING ALL ' ' BY '0'",
        "MOVE '000' TO GR",
    ];
    for setter in setters {
        let source = reading("NUMCHECK", &[setter]).replace("       PROCEDURE DIVISION.\n", "       LINKAGE SECTION.\n       01  L PIC X.\n       PROCEDURE DIVISION.\n");
        let named: Vec<String> = compiled_messages(&source).into_iter().map(|(_, m)| m).filter(|m| m.starts_with("NUMCHECK: Z ")).collect();
        assert_eq!(named, Vec::<String>::new(), "{setter}");
    }
}

#[test]
fn a_by_content_argument_that_always_fails_is_named_where_the_call_tests_it() {
    let source = |tail: &str| {
        format!(
            "       CBL NUMCHECK\n       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n\
             {}       PROCEDURE DIVISION.\n{}{}{tail}       IDENTIFICATION DIVISION.\n       PROGRAM-ID. SUB.\n       DATA DIVISION.\n\
             \x20      LINKAGE SECTION.\n       01  L PIC 999.\n       PROCEDURE DIVISION USING L.\n{}       END PROGRAM SUB.\n       END PROGRAM T.\n",
            "       01  G VALUE ' 12'.\n           05 Z PIC 999.\n",
            line("CALL 'SUB' USING BY CONTENT Z"),
            line("GOBACK."),
            line("GOBACK."),
        )
    };
    let named: BTreeSet<(String, String)> = compiled_messages(&source("")).into_iter().map(|(pos, m)| (pos, m["NUMCHECK: ".len()..].split(' ').next().unwrap_or_default().to_owned())).collect();
    assert_eq!(named.len(), 1);
    let (_, err, ending) = run_with(&source(&format!("       NEVER.\n{}", line("MOVE SPACES TO G."))), &[]);
    assert!(ending.is_ok(), "{ending:?}");
    assert_eq!(reported(&err), named);
}
