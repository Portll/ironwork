use super::*;

fn displays(data: &str, statements: &[&str]) -> String {
    let body: String = statements.iter().flat_map(|s| s.split('\n')).map(line).chain([line("GOBACK.")]).collect();
    let o = Harness::source(&program("", data, &body)).run(Executor::Interpreter);
    assert!(o.ending.is_ok(), "{:?}\n{}", o.ending, o.err);
    o.out
}

const DOC: &str = "       01  D PIC X(200) VALUE SPACES.\n       01  N PIC 9(4).\n";

#[test]
fn a_group_becomes_an_object_its_tables_arrays_and_its_numbers_json_numbers() {
    let data = format!(
        "{DOC}       01  Grp.\n           05 Ac-No PIC AA9999 VALUE 'SX1234'.\n           05 More.\n              10 Stuff PIC S99V9 OCCURS 2.\n           05 SSN PIC 999/99/9999 VALUE SPACE.\n           05 FILLER PIC X(3) VALUE 'ZZZ'.\n           05 R REDEFINES SSN PIC X(11).\n"
    );
    let out = displays(&data, &["MOVE 7.8 TO Stuff(1)", "MOVE -9 TO Stuff(2)", "JSON GENERATE D FROM Grp COUNT N\n    ENCODING 1140", "DISPLAY D(1:N)"]);
    assert_eq!(out, "{\"Grp\":{\"Ac-No\":\"SX1234\",\"More\":{\"Stuff\":[7.8,-9.0]},\"SSN\":\" \"}}\n");
}

#[test]
fn the_document_is_utf_8_unless_an_encoding_is_named() {
    let data = format!("{DOC}       01  G.\n           05 A PIC 9 VALUE 1.\n");
    let out = displays(&data, &["JSON GENERATE D FROM G COUNT N", "DISPLAY N ' ' FUNCTION HEX-OF(D(1:N))"]);
    assert_eq!(out, "0013 7B2247223A7B2241223A317D7D\n");
}

#[test]
fn indicating_writes_null_for_each_occurrence_its_indicator_marks() {
    let single = format!("{DOC}       01  MY-RECORD.\n           02 DATA-1-IS-NULL PIC X VALUE 'Y'.\n           02 DATA-1 PIC X(10).\n");
    let generate = "JSON GENERATE D FROM MY-RECORD COUNT N ENCODING 1140\n    INDICATING DATA-1 IS JSON NULL\n    USING 'Y' IN DATA-1-IS-NULL\nEND-JSON";
    assert_eq!(displays(&single, &[generate, "DISPLAY D(1:N)"]), "{\"MY-RECORD\":{\"DATA-1\":null}}\n");
    let grouped = format!("{DOC}       01  MY-RECORD.\n           02 GRP OCCURS 2.\n              03 DATA-1-IS-NULL PIC X.\n              03 DATA-1 PIC X(10).\n");
    let fill = ["MOVE 'VAL1' TO DATA-1(1)", "MOVE 'VAL2' TO DATA-1(2)", "MOVE 'Y' TO DATA-1-IS-NULL(1)", "MOVE 'N' TO DATA-1-IS-NULL(2)"];
    let out = displays(&grouped, &[&fill[..], &[generate, "DISPLAY D(1:N)"]].concat());
    assert_eq!(out, "{\"MY-RECORD\":{\"GRP\":[{\"DATA-1\":null},{\"DATA-1\":\"VAL2\"}]}}\n");
    let parallel = format!("{DOC}       01  MY-RECORD.\n           02 GRP.\n              03 DATA-1-IS-NULL PIC X OCCURS 2.\n              03 DATA-1 PIC X(10) OCCURS 2.\n");
    let out = displays(&parallel, &[&fill[..], &[generate, "DISPLAY D(1:N)"]].concat());
    assert_eq!(out, "{\"MY-RECORD\":{\"GRP\":{\"DATA-1\":[null,\"VAL2\"]}}}\n");
}

#[test]
fn suppress_leaves_out_items_and_groups_whose_members_all_are() {
    let data = format!("{DOC}       01  J PIC 9 VALUE 2.\n       01  A.\n           02 E PIC X.\n           02 B.\n              03 C OCCURS 0 TO 2 DEPENDING J.\n                 04 DD PIC X.\n");
    let generate = |suppress: &str| format!("MOVE ALL '_' TO A\nJSON GENERATE D FROM A COUNT N ENCODING 1140{suppress}\nDISPLAY D(1:N)\nMOVE SPACES TO D");
    let mut statements = vec![generate(""), generate("\n    SUPPRESS DD"), generate("\n    SUPPRESS B E")];
    statements.push("MOVE 0 TO J".into());
    statements.push(generate(""));
    let out = displays(&data, &statements.iter().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(out, "{\"A\":{\"E\":\"_\",\"B\":{\"C\":[{\"DD\":\"_\"},{\"DD\":\"_\"}]}}}\n{\"A\":{\"E\":\"_\"}}\n{\"A\":{}}\n{\"A\":{\"E\":\"_\",\"B\":{\"C\":[]}}}\n");
    let when = format!("{DOC}       01  G.\n           02 X PIC 9(3) VALUE 0.\n           02 Y PIC X(3) VALUE SPACES.\n           02 Z PIC X(3) VALUE 'ABC'.\n");
    let out = displays(&when, &["JSON GENERATE D FROM G COUNT N ENCODING 1140\n    SUPPRESS EVERY NUMERIC WHEN ZERO\n    Y WHEN SPACES", "DISPLAY D(1:N)"]);
    assert_eq!(out, "{\"G\":{\"Z\":\"ABC\"}}\n");
}

#[test]
fn converting_writes_booleans_and_nulls_and_name_renames_or_omits() {
    let data = format!("{DOC}       01  myrecord.\n           02 data-a PIC X VALUE 'F'.\n           02 data-b PIC X VALUE 'b'.\n              88 data-b-flag VALUE 'a' THRU 'z'.\n           02 data-c PIC 9999 VALUE 0.\n");
    let out = displays(
        &data,
        &[
            "JSON GENERATE D FROM myrecord COUNT N ENCODING 1140\n    NAME OF data-c IS 'count'\n    CONVERTING data-a TO BOOLEAN USING 'T'\n    ALSO data-b TO BOOLEAN USING data-b-flag\n    ALSO data-c TO JSON NULL USING ZERO",
            "DISPLAY D(1:N)",
            "MOVE SPACES TO D",
            "JSON GENERATE D FROM myrecord COUNT N ENCODING 1140\n    NAME OF myrecord IS OMITTED",
            "DISPLAY D(1:N)",
        ],
    );
    assert_eq!(out, "{\"myrecord\":{\"data-a\":false,\"data-b\":true,\"count\":null}}\n{\"data-a\":\"F\",\"data-b\":\"b\",\"data-c\":0}\n");
}

#[test]
fn a_receiver_too_small_is_an_exception_with_json_code_one() {
    let data = "       01  D PIC X(10).\n       01  N PIC 9(4).\n       01  G.\n           05 LONG-NAME PIC X(20) VALUE 'VALUE'.\n";
    let out = displays(
        data,
        &["JSON GENERATE D FROM G COUNT N ENCODING 37\n    ON EXCEPTION DISPLAY 'TOO SMALL ' JSON-CODE ' ' N\n    NOT ON EXCEPTION DISPLAY 'FITS'\nEND-JSON", "DISPLAY D"],
    );
    assert_eq!(out, "TOO SMALL 000000001 0010\n{\"G\":{\"LON\n");
}

#[test]
fn binary_float_and_comp_5_items_take_their_own_formats() {
    let data = format!("{DOC}       01  G.\n           05 B PIC S9(4) COMP-5 VALUE -12.\n           05 F COMP-2 VALUE 7.8.\n           05 P PIC S9(3)V99 COMP-3 VALUE 1.5.\n");
    let out = displays(&data, &["JSON GENERATE D FROM G COUNT N ENCODING 1140", "DISPLAY D(1:N)"]);
    assert_eq!(out, "{\"G\":{\"B\":-12,\"F\":7.79999999999999982E+00,\"P\":1.50}}\n");
}

#[test]
fn a_group_converted_to_json_null_is_null_and_one_whose_members_are_all_ignored_is_left_out() {
    let data = format!("{DOC}       01  A.\n           02 SUB VALUE SPACES.\n              03 S1 PIC X.\n              03 S2 PIC X.\n           02 FILLS.\n              03 FILLER PIC X.\n           02 B PIC X VALUE 'b'.\n");
    let out = displays(
        &data,
        &[
            "JSON GENERATE D FROM A COUNT N ENCODING 1140\n    CONVERTING SUB TO JSON NULL USING SPACE",
            "DISPLAY D(1:N)",
            "MOVE SPACES TO D",
            "JSON GENERATE D FROM A COUNT N ENCODING 1140\n    CONVERTING A TO JSON NULL USING SPACE",
            "DISPLAY D(1:N)",
        ],
    );
    assert_eq!(out, "{\"A\":{\"SUB\":null,\"B\":\"b\"}}\n{\"A\":{\"SUB\":{\"S1\":\" \",\"S2\":\" \"},\"B\":\"b\"}}\n");
}
