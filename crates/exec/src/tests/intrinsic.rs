use super::*;

fn run_at_noon(source: &str) -> (String, Result<Ending, Abend>) {
    let o = Harness::source(source).clock(unit::Clock::Fixed(1_790_510_400, 42)).run(Executor::Interpreter);
    (o.out, o.ending)
}

fn displays(data: &str, statements: &[&str]) -> String {
    let body: String = statements.iter().flat_map(|s| s.split('\n')).map(line).chain([line("GOBACK.")]).collect();
    let (out, ending) = run_at_noon(&program("", data, &body));
    assert!(ending.is_ok(), "{ending:?}");
    out
}

fn each_rounded(functions: &[&str]) -> String {
    let statements: Vec<String> = functions.iter().flat_map(|f| [format!("COMPUTE R ROUNDED =\n    FUNCTION {f}"), "DISPLAY R".to_owned()]).collect();
    displays("       01  R PIC 999.9(9).\n       01  X COMP-2 VALUE 2.5.\n", &statements.iter().map(String::as_str).collect::<Vec<_>>())
}

#[test]
fn the_mathematical_and_financial_functions_give_their_values() {
    let out = each_rounded(&[
        "SQRT(2)", "EXP(1)", "LOG(10)", "LOG10(2)", "SIN(1)", "COS(1)", "TAN(1)", "ASIN(0.5)", "ACOS(0.5)", "ATAN(1)", "PI", "E", "EXP10(2)",
        "ANNUITY(0.25 4)", "PRESENT-VALUE(0.5 1 2 3)",
    ]);
    assert_eq!(
        out,
        [
            "001.414213562", "002.718281828", "002.302585093", "000.301029996", "000.841470985", "000.540302306", "001.557407725", "000.523598776",
            "001.047197551", "000.785398163", "003.141592654", "002.718281828", "100.000000000", "000.423441734", "002.444444444",
        ]
        .map(|s| format!("{s}\n"))
        .concat()
    );
}

#[test]
fn the_statistics_and_mixed_functions_follow_their_arguments() {
    let out = each_rounded(&[
        "MEAN(1 2 3 4)", "MEDIAN(3 1 2)", "MEDIAN(4 1 3 2)", "MIDRANGE(1 9 4)", "VARIANCE(2 4 4 4 5 5 7 9)", "STANDARD-DEVIATION(2 4 4 4 5 5 7 9)",
        "SUM(1.5 2.25 3)", "RANGE(3 9.5 1)", "ORD-MAX(3 9 1)", "ORD-MIN(3 9 1)", "MAX(X 1)", "SUM(X 1)", "FACTORIAL(5)", "SIGN(0)",
    ]);
    assert_eq!(
        out,
        [
            "002.500000000", "002.000000000", "002.500000000", "005.000000000", "004.000000000", "002.000000000", "006.750000000", "008.500000000",
            "002.000000000", "003.000000000", "002.500000000", "003.500000000", "120.000000000", "000.000000000",
        ]
        .map(|s| format!("{s}\n"))
        .concat()
    );
}

#[test]
fn all_subscripts_take_every_element_and_stop_at_the_depending_on_count() {
    let data = "       01  T VALUE '010020030040050'.\n           05 N PIC 9(3) OCCURS 5.\n       01  C PIC 9 VALUE 5.\n       01  D.\n           05 V PIC 9(3) OCCURS 1 TO 5 DEPENDING ON C.\n       01  G VALUE '123456'.\n           05 ROW OCCURS 2.\n              10 CELL PIC 9 OCCURS 3.\n       01  S PIC 9(4).\n";
    let out = displays(
        data,
        &[
            "COMPUTE S = FUNCTION SUM(N(ALL))",
            "DISPLAY S",
            "COMPUTE S = FUNCTION MAX(N(ALL)) + FUNCTION ORD-MIN(N(ALL))",
            "DISPLAY S",
            "MOVE '001002003004005' TO D",
            "MOVE 3 TO C",
            "COMPUTE S = FUNCTION SUM(V(ALL))",
            "DISPLAY S",
            "COMPUTE S = FUNCTION SUM(CELL(ALL, 2))",
            "DISPLAY S",
            "COMPUTE S = FUNCTION SUM(CELL(1 ALL)) * 100\n    + FUNCTION SUM(CELL(ALL ALL))",
            "DISPLAY S",
            "COMPUTE S = FUNCTION ORD-MAX(CELL(ALL, ALL))\n    + FUNCTION SUM(N(ALL) 1000)",
            "DISPLAY S",
        ],
    );
    assert_eq!(out, "0150\n0051\n0006\n0007\n0621\n1156\n");
}

#[test]
fn a_parenthesis_after_a_separator_comma_opens_the_next_argument() {
    let data = "       01  A PIC 9 VALUE 3.\n       01  B PIC 9 VALUE 4.\n       01  T VALUE '051207'.\n           05 IND PIC 99 OCCURS 3.\n       01  R PIC 99.\n";
    let out = displays(
        data,
        &[
            "COMPUTE R = FUNCTION MIN(A * B, (3 + 1) / 2)",
            "DISPLAY R",
            "COMPUTE R = FUNCTION MIN(B; (3))",
            "DISPLAY R",
            "COMPUTE R = FUNCTION MAX(IND (2), (1 + 1))",
            "DISPLAY R",
        ],
    );
    assert_eq!(out, "02\n03\n12\n");
}

#[test]
fn the_date_functions_window_years_from_the_clock() {
    let out = displays(
        "       01  S PIC 9(8).\n       01  F PIC 9(5)V99.\n",
        &[
            "COMPUTE S = FUNCTION YEAR-TO-YYYY(4)",
            "DISPLAY S",
            "COMPUTE S = FUNCTION DATE-TO-YYYYMMDD(851003)",
            "DISPLAY S",
            "COMPUTE S = FUNCTION DAY-TO-YYYYDDD(95005, -10)",
            "DISPLAY S",
            "COMPUTE S = FUNCTION DAY-OF-INTEGER(143951)",
            "DISPLAY S",
            "COMPUTE S = FUNCTION INTEGER-OF-DAY(1995046)",
            "DISPLAY S",
            "COMPUTE S = FUNCTION TEST-DATE-YYYYMMDD(19950240) * 10\n    + FUNCTION TEST-DAY-YYYYDDD(1995446)",
            "DISPLAY S",
            "COMPUTE F ROUNDED = FUNCTION SECONDS-PAST-MIDNIGHT",
            "DISPLAY F",
        ],
    );
    assert_eq!(out, "00002004\n19851003\n01995005\n01995046\n00143951\n00000032\n4320042\n");
}

#[test]
fn the_character_functions_read_storage_as_it_is() {
    let data = "       01  BIN PIC 9(9) BINARY VALUE 12.\n       01  PAC PIC 9(5) COMP-3 VALUE 12345.\n       01  BAD REDEFINES PAC PIC X(3).\n       01  ZON PIC 9(5) VALUE 12345.\n       01  NAT PIC N(3) VALUE N'ABC'.\n       01  U PIC X(36).\n       01  L PIC 9(3).\n";
    let out = displays(
        data,
        &[
            "DISPLAY FUNCTION HEX-OF('Hello, world!')",
            "DISPLAY FUNCTION HEX-OF(BIN) ' '\n    FUNCTION HEX-OF(PAC) ' ' FUNCTION HEX-OF(ZON)",
            "DISPLAY FUNCTION BIT-OF(PAC)",
            "MOVE 'ABC' TO BAD",
            "DISPLAY FUNCTION HEX-OF(PAC)",
            "DISPLAY FUNCTION HEX-TO-CHAR('C1c2')\n    FUNCTION BIT-TO-CHAR('1100001111000100')",
            "COMPUTE L = FUNCTION BYTE-LENGTH(BIN) * 10\n    + FUNCTION BYTE-LENGTH(NAT)",
            "DISPLAY L",
            "DISPLAY FUNCTION DISPLAY-OF(NAT)\n    FUNCTION DISPLAY-OF(FUNCTION NATIONAL-OF('XYZ'), 37)",
            "MOVE FUNCTION UUID4 TO U",
            "DISPLAY U(9:1) U(14:1) U(15:1) U(19:1) U(24:1)",
        ],
    );
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(&lines[..7], ["C8859393966B40A6969993845A", "0000000C 12345F F1F2F3F4F5", "000100100011010001011111", "C1C2C3", "ABCD", "046", "ABCXYZ"]);
    assert_eq!(lines[7], "--4--");
}

#[test]
fn the_numval_tests_and_numval_f_take_ibms_formats() {
    let out = displays(
        "       01  S PIC 9(4).\n       01  F PIC 9(4)V9(4).\n",
        &[
            "COMPUTE S = FUNCTION TEST-NUMVAL('0 1')",
            "DISPLAY S",
            "COMPUTE S = FUNCTION TEST-NUMVAL-C('  $12,345.67CR')",
            "DISPLAY S",
            "COMPUTE S = FUNCTION TEST-NUMVAL-C('CHF 12' 'CHF')\n    + FUNCTION TEST-NUMVAL-F('1.5E+12345')",
            "DISPLAY S",
            "COMPUTE F ROUNDED = FUNCTION NUMVAL-F('+ 12.345678E+2')",
            "DISPLAY F",
        ],
    );
    assert_eq!(out, "0003\n0000\n0010\n12345678\n");
}

#[test]
fn a_floating_point_function_makes_its_expression_floating_point() {
    let out = displays(
        "       01  X COMP-2 VALUE 2.5.\n",
        &["IF FUNCTION SQRT(16) = 4 DISPLAY 'FOUR' END-IF", "IF FUNCTION MAX(X 3) > 2.9 DISPLAY 'MAX' END-IF", "DISPLAY FUNCTION MAX('AB' 'B')"],
    );
    assert_eq!(out, "FOUR\nMAX\nB\n");
}

#[test]
fn an_argument_outside_a_functions_domain_ends_the_run() {
    let ending = |statement: &str| run_at_noon(&program("", "       01  X COMP-2.\n", &[line(statement), line("GOBACK.")].concat())).1.unwrap_err().message;
    assert!(ending("COMPUTE X = FUNCTION SQRT(-1)").contains("FUNCTION SQRT(-1): the argument must be zero or positive"));
    assert!(ending("COMPUTE X = FUNCTION LOG(0)").contains("FUNCTION LOG(0): the argument must be greater than zero"));
    assert!(ending("COMPUTE X = FUNCTION ACOS(2)").contains("FUNCTION ACOS(2): the argument must be from -1 to +1"));
    assert!(ending("COMPUTE X = FUNCTION FACTORIAL(29)").contains("FUNCTION FACTORIAL(29): the argument must be from 0 to 28"));
    assert!(ending("MOVE FUNCTION HEX-TO-CHAR('ABC') TO X").contains("a multiple of 2"));
    assert!(ending("COMPUTE X = FUNCTION EXP(200)").contains("HfpExponentOverflow"));
}

#[test]
fn the_formatted_functions_write_and_read_ibms_formats() {
    let out = displays(
        "       01  S PIC 9(7).\n       01  F PIC 9(5)V99.\n       01  N PIC N(8).\n",
        &[
            "DISPLAY FUNCTION FORMATTED-CURRENT-DATE(\n    'YYYY-MM-DDThh:mm:ss.ss+hh:mm')",
            "DISPLAY FUNCTION FORMATTED-DATE('YYYYMMDD' 143951)",
            "DISPLAY FUNCTION FORMATTED-TIME('hhmmss.ss+hhmm'\n    18867.812479168304 -300)",
            "DISPLAY FUNCTION FORMATTED-TIME('hh:mm:ssZ' 18867 -300)",
            "DISPLAY FUNCTION FORMATTED-DATETIME('YYYYMMDDThhmmss'\n    143951 86399)",
            "DISPLAY FUNCTION FORMATTED-DATETIME(\n    'YYYY-MM-DDThh:mm:ssZ' 143951 82800 -120)",
            "COMPUTE S = FUNCTION INTEGER-OF-FORMATTED-DATE(\n    'YYYYMMDDThhmmss.ss+hhmm' '19950215T051427.81+0500')",
            "DISPLAY S",
            "COMPUTE F ROUNDED = FUNCTION SECONDS-FROM-FORMATTED-TIME(\n    'hhmmss.ss+hhmm' '051427.81+0500')",
            "DISPLAY F",
            "COMPUTE S = FUNCTION TEST-FORMATTED-DATETIME(\n    'YYYYMMDD' '20051314')",
            "DISPLAY S",
            "MOVE FUNCTION FORMATTED-DATE(N'YYYYMMDD' 143951) TO N",
            "DISPLAY FUNCTION DISPLAY-OF(N)",
        ],
    );
    assert_eq!(
        out,
        "2026-09-27T12:00:00.42+00:00\n19950215\n051427.81-0500\n10:14:27Z\n19950215T235959\n1995-02-16T01:00:00Z\n0143951\n1886781\n0000006\n19950215\n"
    );
}

#[test]
fn intdate_lilian_numbers_the_integer_dates_from_15_october_1582() {
    let statements = [
        "COMPUTE S = FUNCTION INTEGER-OF-DATE(19950215)",
        "DISPLAY S",
        "COMPUTE S = FUNCTION DATE-OF-INTEGER(1)",
        "DISPLAY S",
        "COMPUTE S = FUNCTION DAY-OF-INTEGER(1)",
        "DISPLAY S",
        "COMPUTE S = FUNCTION INTEGER-OF-DAY(1995046)",
        "DISPLAY S",
        "DISPLAY FUNCTION FORMATTED-DATE('YYYYMMDD' 150604)",
        "DISPLAY FUNCTION FORMATTED-DATETIME('YYYYDDDThhmmss' 1 0)",
        "COMPUTE S = FUNCTION INTEGER-OF-FORMATTED-DATE(\n    'YYYYMMDD' '19950215')",
        "DISPLAY S",
        "DISPLAY FUNCTION FORMATTED-CURRENT-DATE('YYYYMMDDThhmmss')",
        "COMPUTE S = FUNCTION TEST-DATE-YYYYMMDD(15821015)",
        "DISPLAY S",
    ];
    let body: String = statements.iter().flat_map(|s| s.split('\n')).map(line).chain([line("GOBACK.")]).collect();
    let data = "       01  S PIC 9(8).\n";
    let shown = |card: &str| {
        let (out, ending) = run_at_noon(&program(card, data, &body));
        assert!(ending.is_ok(), "{ending:?}");
        out
    };
    assert_eq!(shown("INTDATE(LILIAN)"), "00150604\n15821015\n01582288\n00150604\n19950215\n1582288T000000\n00150604\n20260927T120000\n00000001\n");
    assert_eq!(shown(""), "00143951\n16010101\n01601001\n00143951\n20130504\n1601001T000000\n00143951\n20260927T120000\n00000001\n");
    assert_eq!(shown("INTDATE(ANSI)"), shown(""));
    let ending = |card: &str, statement: &str| run_at_noon(&program(card, data, &[line(statement), line("GOBACK.")].concat())).1;
    assert!(ending("INTDATE(LILIAN)", "COMPUTE S = FUNCTION INTEGER-OF-DATE(15821014)").unwrap_err().message.contains("not a date from 15821015 to 99991231"));
    assert!(ending("", "COMPUTE S = FUNCTION INTEGER-OF-DATE(15821015)").unwrap_err().message.contains("not a date from 16010101 to 99991231"));
    assert!(ending("INTDATE(LILIAN)", "COMPUTE S = FUNCTION DATE-OF-INTEGER(3074324)").is_ok());
    assert!(ending("", "COMPUTE S = FUNCTION DATE-OF-INTEGER(3074324)").unwrap_err().message.contains("outside 1 to 3067671"));
    assert!(ending("INTDATE(LILIAN)", "COMPUTE S = FUNCTION COMBINED-DATETIME(3074324 0)").is_ok());
    assert!(ending("", "COMPUTE S = FUNCTION COMBINED-DATETIME(3074324 0)").unwrap_err().message.contains("outside 1 to 3067671"));
}

#[test]
fn a_reference_modified_national_item_counts_characters_and_stays_national() {
    let data = "       01  NX PIC N(4) VALUE N'ABCD'.\n       01  S PIC 9 VALUE 2.\n";
    let out = displays(data, &["DISPLAY FUNCTION DISPLAY-OF(NX(2:S))", "DISPLAY FUNCTION DISPLAY-OF(NX(3:))", "DISPLAY FUNCTION DISPLAY-OF(\n    FUNCTION NATIONAL-OF('WXYZ')(2:2))"]);
    assert_eq!(out, "BC\nCD\nXY\n");
}

#[test]
fn the_unicode_functions_count_utf_8_and_utf_16_characters() {
    let data = concat!(
        "       01  A PIC X(6) VALUE X'4BC3A4666572'.\n",
        "       01  BB PIC X(16) VALUE X'005400F6006200750072D858DC6B0073'.\n",
        "       01  B REDEFINES BB PIC N(8).\n",
        "       01  E PIC X(3) VALUE 'ABC'.\n",
        "       01  S PIC X(6).\n",
        "       01  R PIC 99.\n",
    );
    let each = ["ULENGTH(A)", "ULENGTH(B)", "UPOS(A 3)", "UPOS(B 7)", "UWIDTH(B 6)", "UWIDTH(A 9)", "USUPPLEMENTARY(B)", "UVALID(A)", "UVALID(E)"];
    let mut statements: Vec<String> = each.iter().flat_map(|f| [format!("COMPUTE R = FUNCTION {f}"), "DISPLAY R".into()]).collect();
    statements.push("MOVE FUNCTION USUBSTR(A 2 2) TO S".into());
    statements.push("DISPLAY FUNCTION HEX-OF(S(1:3))".into());
    let out = displays(data, &statements.iter().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(out, "05\n07\n04\n15\n04\n00\n06\n00\n01\nC3A466\n");
}

#[test]
fn combined_datetime_is_a_long_approximation_rounded_into_its_receiver() {
    let statements = |data: &str| {
        let body: String = ["COMPUTE MYDATE = 143951", "COMPUTE MYTIME = 18867.812479168304", "COMPUTE MYRESULT =", "    FUNCTION COMBINED-DATETIME (MYDATE, MYTIME)", "DISPLAY 'COMBINED-DATE-TIME: ' MYRESULT", "GOBACK."]
            .into_iter()
            .map(line)
            .collect();
        (data.to_owned(), body)
    };
    let (compat, body) = statements("       01 MYRESULT PIC 9(8).9(10).\n       01 MYDATE PIC 9(18).\n       01 MYTIME PIC 9(06)V9(12).\n");
    assert_eq!(run_at_noon(&program("", &compat, &body)).0, "COMBINED-DATE-TIME: 00143951.1886781248\n");
    let (extend, body) = statements("       01 MYRESULT PIC 9(9).9(17).\n       01 MYDATE PIC 9(31).\n       01 MYTIME PIC 9(19)V9(12).\n");
    assert_eq!(run_at_noon(&program("ARITH(EXTEND)", &extend, &body)).0, "COMBINED-DATE-TIME: 000143951.18867812478856649\n");
}

#[test]
fn content_of_gives_its_arguments_value() {
    let data = "       01  A PIC X(3) VALUE 'abc'.\n       01  N PIC S9(3) VALUE -42.\n       01  R PIC S9(3).\n";
    let out = displays(data, &["DISPLAY FUNCTION CONTENT-OF(A)", "COMPUTE R = FUNCTION CONTENT-OF(N) + 1", "DISPLAY R"]);
    assert_eq!(out, "abc\n04J\n");
}

#[test]
fn numval_is_floating_point_long_under_compat_and_extended_under_extend() {
    let data = "       01  A PIC X(20) VALUE '123456789.123456789'.\n       01  B PIC 999V99.\n       01  C PIC 9(9)V9(9).\n";
    let body: String = ["COMPUTE B = FUNCTION NUMVAL(' 1.23 ')", "COMPUTE C = FUNCTION NUMVAL(A)", "DISPLAY B ' ' C", "COMPUTE C = FUNCTION NUMVAL-C('$1,234.5CR')", "DISPLAY C", "GOBACK."].into_iter().map(line).collect();
    assert_eq!(run_at_noon(&program("", data, &body)).0, "00123 123456789123456787\n000001234500000000\n");
    assert_eq!(run_at_noon(&program("ARITH(EXTEND)", data, &body)).0, "00123 123456789123456789\n000001234500000000\n");
}

#[test]
fn when_compiled_gives_the_compile_time_as_current_date_gives_the_run_s() {
    let at = rt::lir::CompileTime { seconds: 1_790_510_400, hundredths: 42, source: rt::lir::TimeSource::Clock };
    let body = [line("MOVE FUNCTION WHEN-COMPILED TO W"), line("DISPLAY W ' ' FUNCTION WHEN-COMPILED(1:8)"), line("DISPLAY FUNCTION CURRENT-DATE(1:8)"), line("GOBACK.")].concat();
    let o = Harness::source(&program("", "       01  W PIC X(21).\n", &body)).compiled_at(at).clock(unit::Clock::Fixed(0, 0)).run(Executor::Interpreter);
    assert!(o.ending.is_ok(), "{:?}", o.ending);
    assert_eq!(o.out, "2026092712000042+0000 20260927\n19700101\n");
}

#[test]
fn numval_c_takes_the_currency_option_as_its_default_currency_sign() {
    let data = "       01  R PIC 9(4)V99.\n";
    let body: String = ["COMPUTE R = FUNCTION NUMVAL-C('£1,234.50')", "DISPLAY R", "GOBACK."].into_iter().map(line).collect();
    assert_eq!(run_at_noon(&program("CURRENCY('£')", data, &body)).0, "123450\n");
}
