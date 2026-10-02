use super::*;

fn displays(data: &str, statements: &[&str]) -> String {
    let long = statements.iter().flat_map(|s| s.split('\n')).find(|l| l.len() > 61);
    assert!(long.is_none(), "past column 72: {long:?}");
    let body: String = statements.iter().flat_map(|s| s.split('\n')).map(line).chain([line("GOBACK.")]).collect();
    let o = Harness::source(&program("", data, &body)).run(Executor::Interpreter);
    assert!(o.ending.is_ok(), "{:?}\n{}", o.ending, o.err);
    o.out
}

const CODES: &str = "       01  Code-Out PIC 9(3).\n       01  Status-Out PIC 9(3).\n";
const SHOW: &str = "MOVE JSON-CODE TO Code-Out\nMOVE JSON-STATUS TO Status-Out\nDISPLAY 'CODE ' Code-Out ' STATUS ' Status-Out";

#[test]
fn a_document_fills_the_items_its_names_match() {
    let data = format!(
        "{CODES}       01  T PIC X(60) VALUE\n           '{{\"msg\":{{\"ver\":5,\"uid\":1000,\"txt\":\"Hello World!\"}}}}'.\n       01  U PIC X(60).\n       01  msg.\n           04 ver usage comp-1.\n           04 uid pic 9999 usage display.\n           04 txt pic x(32).\n"
    );
    let out = displays(
        &data,
        &[
            "JSON PARSE T INTO msg ENCODING 1140 END-JSON",
            SHOW,
            "IF ver EQUAL TO 5 DISPLAY 'Message ID is ' uid",
            "DISPLAY \"Message text is '\" txt \"'\" END-IF",
            "MOVE FUNCTION DISPLAY-OF(FUNCTION NATIONAL-OF(T) 1208) TO U",
            "MOVE SPACES TO msg",
            "JSON PARSE U INTO msg",
            SHOW,
            "DISPLAY uid ' ' txt",
            "JSON PARSE T INTO msg",
            SHOW,
        ],
    );
    assert_eq!(
        out,
        concat!(
            "CODE 000 STATUS 000\nMessage ID is 1000\nMessage text is 'Hello World!                    '\n",
            "CODE 000 STATUS 000\n1000 Hello World!                    \n",
            "CODE 100 STATUS 000\n",
        )
    );
}

#[test]
fn name_suppress_and_the_matching_rules_set_the_status() {
    let data = format!(
        "{CODES}       01  T PIC X(80).\n       01  mydata pic 999.\n       01 G.\n         05 h.\n           10 a pic x(10).\n           10 3_ pic 9.\n           10 C-c pic x(10).\n"
    );
    let parse = |text: &str, phrase: &str| format!("MOVE '{text}' TO T\nJSON PARSE T INTO G ENCODING 1140{phrase}\n{SHOW}\nDISPLAY a '|' 3_ '|' C-c\nMOVE ALL '-' TO a C-c");
    let statements = [
        "MOVE '{\"abc+\":100}' TO T\nJSON PARSE T INTO mydata ENCODING 1140\n    NAME OF mydata IS \"abc+\"\nEND-JSON\nDISPLAY 'mydata is ' mydata".to_owned(),
        parse("{\"g\": {\"H\": {\"A\": \"Eh?\", \"3_\": 5, \"c-C\": \"See\"}}}", ""),
        parse("{\"g\": {\"H\": {\"A\": \"Eh?\", \"c-C\": \"See\"}}}", ""),
        parse("{\"G\": {\"h\": {\"A\": \"Eh?\", \"B\": \"Bee\", \"3_\": 5}}}", ""),
        parse("{\"g\": {\"A\": \"Eh?\", \"3_\": 5, \"c-C\": \"See\"}}", ""),
        parse("{\"g\": {\"H\": {\"A\": \"x\", \"3_\": \"x\", \"c-C\": \"abc\"}}}", ""),
        parse("{\"g\": {\"H\": {\"A\": \"one\", \"A\": \"two\"}}}", ""),
        parse("{\"g\": {\"H\": {\"A\": \"one\", \"3_\": 7, \"C-C\": \"x\"}}}", "\n    SUPPRESS 3_\n    NAME OF a IS 'C-C' C-c IS 'A'"),
    ];
    let out = displays(&data, &statements.iter().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(
        out,
        concat!(
            "mydata is 100\n",
            "CODE 000 STATUS 000\nEh?       |5|See       \n",
            "CODE 000 STATUS 001\nEh?       |5|See       \n",
            "CODE 000 STATUS 003\nEh?       |5|----------\n",
            "CODE 106 STATUS 003\n----------|5|----------\n",
            "CODE 104 STATUS 000\nx         |5|----------\n",
            "CODE 103 STATUS 000\none       |5|----------\n",
            "CODE 000 STATUS 000\nx         |5|one       \n",
        )
    );
}

#[test]
fn arrays_fill_tables_up_to_their_occurrences() {
    let data = format!(
        "{CODES}       01  T PIC X(120).\n       01  n pic 9.\n       01  some-data.\n        02 msg occurs 0 to 5 depending on n.\n           04 ver pic 9.\n           04 uid pic 9999 usage display.\n"
    );
    let out = displays(
        &data,
        &[
            "MOVE 4 TO n",
            "MOVE 0 TO ver(1) ver(2) ver(3) ver(4)",
            "MOVE '{\"some-data\":{\"msg\":[{\"ver\":5,\"uid\":10},' TO T",
            "MOVE '{\"ver\":5,\"uid\":11},{\"ver\":5,\"uid\":12}]}}' TO T(41:)",
            "JSON PARSE T INTO some-data ENCODING 1140",
            SHOW,
            "DISPLAY ver(1) uid(1) ' ' ver(2) uid(2) ' ' ver(3) uid(3)\n    ' ' ver(4)",
            "MOVE 2 TO n",
            "JSON PARSE T INTO some-data ENCODING 1140",
            SHOW,
        ],
    );
    assert_eq!(out, "CODE 000 STATUS 008\n50010 50011 50012 0\nCODE 000 STATUS 016\n");
}

#[test]
fn an_anonymous_array_parses_and_generates_with_name_omitted() {
    let data = format!(
        "{CODES}       01  T PIC X(100).\n       01  N PIC 9(4).\n       01 ACT.\n         02 B1 occurs 2.\n           03 C1.\n            04 M1 pic 9.\n            04 D1 occurs 2.\n              05 N1 pic 9.\n"
    );
    let out = displays(
        &data,
        &[
            "move spaces to ACT",
            "move '[{\"C1\":{\"M1\":1,\"D1\":[{\"N1\":3},{\"N1\":4}]}},' to T",
            "move '{\"C1\":{\"M1\":2,\"D1\":[{\"N1\":5},{\"N1\":6}]}}]' to T(43:)",
            "json parse T into b1 encoding 1140\n    name b1 is omitted\nend-json",
            SHOW,
            "display M1(1) M1(2) N1(1 1) N1(1 2) N1(2 1) N1(2 2)",
            "json parse T into b1 encoding 1140",
            SHOW,
            "move spaces to T",
            "json generate T from b1 count N encoding 1140\n    name b1 is omitted\nend-json",
            "display T(1:N)",
        ],
    );
    assert_eq!(
        out,
        "CODE 000 STATUS 000\n123456\nCODE 108 STATUS 000\n[{\"C1\":{\"M1\":1,\"D1\":[{\"N1\":3},{\"N1\":4}]}},{\"C1\":{\"M1\":2,\"D1\":[{\"N1\":5},{\"N1\":6}]}}]\n"
    );
}

#[test]
fn nulls_are_ignored_converted_or_indicated_and_booleans_converted() {
    let data = format!(
        concat!(
            "{}       01  T PIC X(80).\n",
            "       01 my-record.\n         02 data-a pic 9999.\n         02 data-b pic x(10).\n",
            "       01 MY-REC.\n         02 DATA-1-IS-NULL PIC X.\n         02 DATA-1 PIC X(10).\n",
            "       01 myrecord.\n         02 flag-a pic x.\n           88 flag-a-on value 'T' false 'F'.\n",
            "         02 flag-b pic x.\n           88 flag-b-true value '1'.\n           88 flag-b-false value '0'.\n",
            "         02 flag-c pic x.\n",
        ),
        CODES
    );
    let out = displays(
        &data,
        &[
            "MOVE 1234 TO data-a",
            "MOVE '0123456789' TO data-b",
            "MOVE '{\"my-record\" : {\"data-a\" : null,' TO T",
            "MOVE ' \"data-b\" : null}}' TO T(38:)",
            "JSON PARSE T INTO my-record ENCODING 1140",
            SHOW,
            "DISPLAY data-a \"'\" data-b \"'\"",
            "JSON PARSE T INTO my-record ENCODING 1140\n    CONVERTING data-a FROM NULL USING ZERO\n    ALSO data-b FROM JSON NULL USING SPACES\nEND-JSON",
            SHOW,
            "DISPLAY data-a \"'\" data-b \"'\"",
            "MOVE 1234 TO data-a",
            "JSON PARSE T INTO my-record ENCODING 1140\n    IGNORING JSON NULL FOR ALL",
            SHOW,
            "MOVE '{ \"MY-REC\" : { \"data-1\" : null } }' TO T",
            "MOVE ALL 'z' TO MY-REC",
            "JSON PARSE T INTO MY-REC ENCODING 1140\n    INDICATING DATA-1 IS JSON NULL\n    USING 'Y' AND 'N' IN DATA-1-IS-NULL\nEND-JSON",
            SHOW,
            "DISPLAY 'DATA-1-IS-NULL: ' DATA-1-IS-NULL ' ' DATA-1",
            "MOVE '{\"myrecord\":{\"flag-a\":true,' TO T",
            "MOVE '\"flag-b\":false,\"flag-c\":true}}' TO T(28:)",
            "JSON PARSE T INTO myrecord ENCODING 1140\n    CONVERTING flag-a FROM BOOLEAN USING flag-a-on\n    ALSO flag-b FROM BOOLEAN\n    USING flag-b-true AND flag-b-false\n    ALSO flag-c FROM BOOLEAN USING 'a' AND 'z'",
            SHOW,
            "DISPLAY flag-a flag-b flag-c",
            "JSON PARSE T INTO myrecord ENCODING 1140",
            SHOW,
        ],
    );
    assert_eq!(
        out,
        concat!(
            "CODE 000 STATUS 032\n1234'0123456789'\n",
            "CODE 000 STATUS 000\n0000'          '\n",
            "CODE 000 STATUS 000\n",
            "CODE 000 STATUS 000\nDATA-1-IS-NULL: Y zzzzzzzzzz\n",
            "CODE 000 STATUS 000\nT0a\n",
            "CODE 107 STATUS 000\n",
        )
    );
}

#[test]
fn the_programming_guides_client_data_moves_into_edited_and_national_items() {
    let data = concat!(
        "       01 jtxt-client-data.\n",
        "         03 pic x(16)  value '{\"client-data\":{'.\n",
        "         03 pic x(28)  value ' \"account-num\":123456789012,'.\n",
        "         03 pic x(19)  value ' \"balance\":-125.53,'.\n",
        "         03 pic x(17)  value ' \"billing-info\":{'.\n",
        "         03 pic x(22)  value '  \"name-first\":\"John\",'.\n",
        "         03 pic x(22)  value '  \"name-last\":\"Smith\",'.\n",
        "         03 pic x(21)  value '  \"addr-code\":\"10203\"'.\n",
        "         03 pic x(6)   value '  } }}'.\n",
        "       01 jtxt-transactions.\n",
        "         03 pic x(31)  value '{\"transactions\": {\"tx-record\":['.\n",
        "         03 pic x(39)  value '{\"tx-uid\":107,\"tx-item-uid\":\"ab142424\",'.\n",
        "         03 pic x(38)  value '\"tx-priceinUS$\":12.34},{\"tx-uid\":1904,'.\n",
        "         03 pic x(25)  value '\"tx-item-uid\":\"gb051533\",'.\n",
        "         03 pic x(23)  value '\"tx-priceinUS$\":833.22}'.\n",
        "         03 pic x(3)   value ']}}'.\n",
        "       77 txnum pic 999999 usage display value zero.\n",
        "       01 client-data.\n",
        "         03 account-num   pic 999,999,999,999.\n",
        "         03 balance       pic $$$9.99CR.\n",
        "         03 billing-info.\n",
        "          05 name-first  pic n(20).\n",
        "          05 name-last   pic n(20).\n",
        "          05 addr-code   pic n(10).\n",
        "         03 transactions.\n",
        "          05 tx-record occurs 0 to 100 depending txnum.\n",
        "           07 tx-uid       pic 99999 usage display.\n",
        "           07 tx-item-uid  pic AA/9999B99.\n",
        "           07 tx-price     pic $$$9.99.\n",
    );
    let out = displays(
        data,
        &[
            "Json parse jtxt-client-data into client-data\n    encoding 1140\n    with detail\n    suppress transactions\n    not on exception\n    display \"Successful JSON Parse\"\nend-json",
            "Display account-num ' ' balance",
            "Display function display-of(name-last)\n    function display-of(addr-code)",
            "Move 2 to txnum",
            "Json parse jtxt-transactions into transactions encoding 1140\n    name tx-price is 'tx-priceinUS$'\n    not on exception\n    display \"Successful JSON Parse\"\nend-json",
            "Display tx-uid(1) ' ' tx-item-uid(1) ' ' tx-price(1)",
            "Display tx-uid(2) ' ' tx-item-uid(2) ' ' tx-price(2)",
        ],
    );
    assert_eq!(
        out,
        concat!(
            "Successful JSON Parse\n123,456,789,012 $125.53CR\nSmith               10203     \n",
            "Successful JSON Parse\n00107 ab/1424 24  $12.34\n01904 gb/0515 33 $833.22\n",
        )
    );
}

#[test]
fn integers_go_into_national_and_alphanumeric_items_and_any_number_into_floating_point() {
    let data = format!("{CODES}       01  T PIC X(40) VALUE '{{\"G\":{{\"N\":42,\"F\":1.5E1,\"A\":7}}}}'.\n       01  G.\n           05 N PIC N(4).\n           05 F COMP-2.\n           05 A PIC X(3).\n");
    let out = displays(&data, &["JSON PARSE T INTO G ENCODING 1140", SHOW, "DISPLAY FUNCTION DISPLAY-OF(N) '|' A '|'", "COMPUTE Code-Out = F", "DISPLAY Code-Out"]);
    assert_eq!(out, "CODE 000 STATUS 000\n42  |7  |\n015\n");
}

#[test]
fn a_number_moves_only_where_table_46_allows_and_an_exponent_is_never_expanded() {
    let data = format!("{CODES}       01  T PIC X(60).\n       01  G.\n           05 ALPHA PIC A(5).\n           05 ALNUM PIC X(5).\n           05 NUM PIC 9(4).\n");
    let out = displays(
        &data,
        &[
            "MOVE '{\"G\":{\"ALPHA\":42}}' TO T",
            "JSON PARSE T INTO G ENCODING 1140",
            SHOW,
            "MOVE '{\"G\":{\"ALNUM\":1e2}}' TO T",
            "JSON PARSE T INTO G ENCODING 1140",
            SHOW,
            "MOVE '{\"G\":{\"ALNUM\":42}}' TO T",
            "JSON PARSE T INTO G ENCODING 1140",
            SHOW,
            "MOVE '{\"G\":{\"NUM\":1e99999999999999999999}}' TO T",
            "JSON PARSE T INTO G ENCODING 1140",
            SHOW,
            "DISPLAY ALNUM ' ' NUM",
        ],
    );
    assert_eq!(out, "CODE 104 STATUS 000\nCODE 104 STATUS 000\nCODE 000 STATUS 001\nCODE 000 STATUS 129\n42    0000\n");
}

#[test]
fn a_member_after_an_occurs_depending_on_table_is_placed_by_the_count_its_pair_set() {
    let data = concat!(
        "       01  T PIC X(60) VALUE\n           '{\"REC\":{\"CNT\":2,\"ITEM\":[\"A\",\"B\"],\"LATER\":\"XYZ\"}}'.\n",
        "       01  REC.\n           05 CNT PIC 9 VALUE 5.\n           05 ITEM PIC X OCCURS 1 TO 5 DEPENDING ON CNT.\n           05 LATER PIC X(3).\n",
    );
    assert_eq!(displays(&format!("{CODES}{data}"), &["JSON PARSE T INTO REC ENCODING 1140", SHOW, "DISPLAY REC"]), "CODE 000 STATUS 000\n2ABXYZ\n");
}
