use super::*;

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
    let (_, err, _) = ran("NUMCHECK(ZON)", &["MOVE A TO W", "MOVE Z TO X", "MOVE Z TO W"]);
    assert_eq!(messages(&err), ["A X'40F1F2' in program T is not NUMERIC; the statement runs", "Z X'40F1F2' in program T is not NUMERIC; the statement runs", "Z X'40F1F2' in program T is not NUMERIC; the statement runs"]);
    let (_, err, _) = ran("NUMCHECK(ZON(LAX))", &["MOVE A TO W", "MOVE Z TO X", "MOVE Z TO W"]);
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
