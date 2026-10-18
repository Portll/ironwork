use super::*;
use crate::testing::Outcome;

const EXTENDED: &[&str] = &["--compliance=extended"];

fn both(source: &str) -> Outcome {
    let walked = Harness::source(source).flags(EXTENDED).run(Executor::Interpreter);
    let vm = Harness::source(source).flags(EXTENDED).run(Executor::Vm);
    assert_eq!((&vm.out, vm.ending.as_ref().ok()), (&walked.out, walked.ending.as_ref().ok()), "{}", walked.err);
    walked
}

fn diagnostics_under(source: &str, compliance: numeric::Compliance) -> Vec<(u32, Option<&'static str>, Severity)> {
    let flags: Vec<String> = if compliance == numeric::Compliance::Extended { EXTENDED.iter().map(|f| f.to_string()).collect() } else { Vec::new() };
    let parsed = syntax::parse_with(source, &syntax::copy::Libraries::default().with_compliance(compliance)).unwrap_or_else(|e| panic!("{e}"));
    let mut shown: Vec<_> = match compile(parsed, &flags) {
        Ok(compiled) => compiled.diagnostics,
        Err(errors) => errors,
    }
    .iter()
    .map(|m| (m.pos.line, m.id, m.severity))
    .collect();
    shown.sort();
    shown
}

/// Zoned and packed items of 32 to 38 digits, moved to and from items of other kinds and sizes.
const STORAGE: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. WSTORE.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 Z1 PIC S9(38) VALUE -12345678901234567890123456789012345678.\n",
    "       01 ZX REDEFINES Z1 PIC X(38).\n",
    "       01 Z2 PIC 9(35)V9(3)\n",
    "           VALUE 12345678901234567890123456789012345.678.\n",
    "       01 Z3 PIC S9(36)V99 SIGN LEADING VALUE -1234.5.\n",
    "       01 Z4 PIC S9(36)V99 SIGN TRAILING SEPARATE VALUE 1234.5.\n",
    "       01 Z5 PIC S9(36)V99 SIGN LEADING SEPARATE VALUE -1234.5.\n",
    "       01 P1 PIC S9(38) COMP-3\n",
    "           VALUE -12345678901234567890123456789012345678.\n",
    "       01 P2 PIC 9(33)V9(5) COMP-3\n",
    "           VALUE 12345678901234567890123456789012.345.\n",
    "       01 P3 PIC S9(32) COMP-3 VALUE 5.\n",
    "       01 PX REDEFINES P3 PIC X(17).\n",
    "       01 N1 PIC S9(32) VALUE 0.\n",
    "       01 N2 PIC S9(38) VALUE 0.\n",
    "       01 NP PIC S9(33) COMP-3 VALUE 0.\n",
    "       01 A1 PIC X(40) VALUE SPACES.\n",
    "       01 SMALL PIC 9(5) VALUE 0.\n",
    "       01 SMALLP PIC S9(5)V99 COMP-3 VALUE 0.\n",
    "       01 B18 PIC S9(18) COMP VALUE 0.\n",
    "       PROCEDURE DIVISION.\n",
    "           DISPLAY Z1 \"|\" Z2 \"|\" Z3 \"|\" Z4 \"|\" Z5\n",
    "           DISPLAY P1 \"|\" P2 \"|\" P3\n",
    "           DISPLAY FUNCTION HEX-OF(PX)\n",
    "           MOVE Z1 TO N1 DISPLAY N1\n",
    "           MOVE Z1 TO N2 DISPLAY N2\n",
    "           MOVE Z1 TO NP DISPLAY NP\n",
    "           MOVE P1 TO N2 DISPLAY N2\n",
    "           MOVE P1 TO NP DISPLAY NP\n",
    "           MOVE P2 TO Z2 DISPLAY Z2\n",
    "           MOVE Z2 TO P2 DISPLAY P2\n",
    "           MOVE Z1 TO SMALL DISPLAY SMALL\n",
    "           MOVE Z1 TO SMALLP DISPLAY SMALLP\n",
    "           MOVE Z1 TO B18 DISPLAY B18\n",
    "           MOVE Z1 TO A1 DISPLAY A1\n",
    "           MOVE P1 TO A1 DISPLAY A1\n",
    "           MOVE ZX TO N2 DISPLAY N2\n",
    "           MOVE 12345 TO SMALL\n",
    "           MOVE SMALL TO N2 DISPLAY N2\n",
    "           MOVE SMALL TO NP DISPLAY NP\n",
    "           MOVE -987654321098765 TO B18\n",
    "           MOVE B18 TO N2 DISPLAY N2\n",
    "           MOVE B18 TO NP DISPLAY NP\n",
    "           MOVE 3.14159 TO Z2 DISPLAY Z2\n",
    "           MOVE ZERO TO N2 DISPLAY N2\n",
    "           STOP RUN.\n",
);

/// What cobc 3.2 shows for the program above.
const STORAGE_OUT: &str = concat!(
    "-12345678901234567890123456789012345678|12345678901234567890123456789012345.678|-000000000000000000000000000000001234.50|000000000000000000000000000000001234.50+|-000000000000000000000000000000001234.50\n",
    "-12345678901234567890123456789012345678|012345678901234567890123456789012.34500|+00000000000000000000000000000005\n",
    "000000000000000000000000000000005C\n",
    "-78901234567890123456789012345678\n",
    "-12345678901234567890123456789012345678\n",
    "-678901234567890123456789012345678\n",
    "-12345678901234567890123456789012345678\n",
    "-678901234567890123456789012345678\n",
    "00012345678901234567890123456789012.345\n",
    "012345678901234567890123456789012.34500\n",
    "45678\n",
    "-45678.00\n",
    "-123456789012345678\n",
    "12345678901234567890123456789012345678  \n",
    "12345678901234567890123456789012345678  \n",
    "+01234567890123456789012345678901234567\n",
    "+00000000000000000000000000000000012345\n",
    "+000000000000000000000000000012345\n",
    "-00000000000000000000000987654321098765\n",
    "-000000000000000000987654321098765\n",
    "00000000000000000000000000000000003.141\n",
    "+00000000000000000000000000000000000000\n",
);

/// ADD, SUBTRACT, MULTIPLY, DIVIDE and COMPUTE over items of 32 to 38 digits, with SIZE ERROR, ROUNDED and REMAINDER.
const ARITHMETIC: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. WARITH.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 X PIC S9(38) VALUE 0.\n",
    "       01 Y PIC S9(38) VALUE 0.\n",
    "       01 Q PIC S9(38) VALUE 0.\n",
    "       01 R PIC S9(38) VALUE 0.\n",
    "       01 XP PIC S9(37)V9 COMP-3 VALUE 0.\n",
    "       01 YP PIC S9(35)V9(3) VALUE 0.\n",
    "       01 D1 PIC S9(5)V99 VALUE 0.\n",
    "       01 M1 PIC 9(36) VALUE 99999999999999999999999999999999999.\n",
    "       01 M2 PIC 9(36) VALUE 3.\n",
    "       PROCEDURE DIVISION.\n",
    "           MOVE 99999999999999999999999999999999999999 TO X\n",
    "           MOVE 1 TO Y\n",
    "           ADD Y TO X ON SIZE ERROR DISPLAY \"SE1\" END-ADD\n",
    "           DISPLAY X\n",
    "           SUBTRACT 1 FROM X GIVING Q\n",
    "           DISPLAY Q\n",
    "           MOVE -99999999999999999999999999999999999999 TO X\n",
    "           SUBTRACT Y FROM X ON SIZE ERROR DISPLAY \"SE2\" END-SUBTRACT\n",
    "           DISPLAY X\n",
    "           ADD X Y GIVING Q ON SIZE ERROR DISPLAY \"SE3\" END-ADD\n",
    "           DISPLAY Q\n",
    "           MOVE 12345678901234567890123456789012 TO X\n",
    "           MOVE 987654321098765432109876543210 TO Y\n",
    "           MULTIPLY X BY Y GIVING Q ON SIZE ERROR DISPLAY \"SE4\"\n",
    "           END-MULTIPLY\n",
    "           DISPLAY Q\n",
    "           MULTIPLY 1000000 BY X ON SIZE ERROR DISPLAY \"SE5\"\n",
    "           END-MULTIPLY\n",
    "           DISPLAY X\n",
    "           DIVIDE X BY 7 GIVING Q REMAINDER R\n",
    "           DISPLAY Q \" \" R\n",
    "           DIVIDE 12345 INTO X GIVING Q REMAINDER R\n",
    "           DISPLAY Q \" \" R\n",
    "           MOVE 12345678901234567890123456789012 TO X\n",
    "           DIVIDE Y INTO X GIVING Q REMAINDER R\n",
    "           DISPLAY Q \" \" R\n",
    "           COMPUTE Q = X + Y * 3 - 7\n",
    "           DISPLAY Q\n",
    "           COMPUTE Q ROUNDED = X / 7\n",
    "           DISPLAY Q\n",
    "           COMPUTE Q = FUNCTION MOD(X, 1000000007)\n",
    "           DISPLAY Q\n",
    "           COMPUTE Q = FUNCTION ABS(X) + FUNCTION MAX(X Y)\n",
    "           DISPLAY Q\n",
    "           COMPUTE Q = FUNCTION SUM(X Y X)\n",
    "           DISPLAY Q\n",
    "           COMPUTE XP = X / 3\n",
    "           DISPLAY XP\n",
    "           COMPUTE YP ROUNDED = X / 7\n",
    "           DISPLAY YP\n",
    "           COMPUTE YP = X * 0.001\n",
    "           DISPLAY YP\n",
    "           COMPUTE D1 = X\n",
    "               ON SIZE ERROR DISPLAY \"SE6\"\n",
    "           END-COMPUTE\n",
    "           DISPLAY D1\n",
    "           COMPUTE Q = M1 * M2\n",
    "           DISPLAY Q\n",
    "           COMPUTE Q = M1 * M1 / M1\n",
    "           DISPLAY Q\n",
    "           COMPUTE Q = (M1 + M2) * 10\n",
    "               ON SIZE ERROR DISPLAY \"SE7\"\n",
    "           END-COMPUTE\n",
    "           DISPLAY Q\n",
    "           COMPUTE Q = X ** 2\n",
    "               ON SIZE ERROR DISPLAY \"SE8\"\n",
    "           END-COMPUTE\n",
    "           DISPLAY Q\n",
    "           COMPUTE Q = FUNCTION SQRT(X)\n",
    "           DISPLAY Q\n",
    "           COMPUTE Q = FUNCTION INTEGER(X / 7)\n",
    "           DISPLAY Q\n",
    "           IF X > Y DISPLAY \"X>Y\" ELSE DISPLAY \"X<=Y\" END-IF\n",
    "           IF X = 12345678901234567890123456789012\n",
    "              DISPLAY \"X EQ LIT\" END-IF\n",
    "           IF XP > YP DISPLAY \"XP>YP\" ELSE DISPLAY \"XP<=YP\" END-IF\n",
    "           IF X NUMERIC DISPLAY \"NUM\" END-IF\n",
    "           IF X POSITIVE DISPLAY \"POS\" END-IF\n",
    "           EVALUATE TRUE\n",
    "              WHEN X = Y DISPLAY \"EQ\"\n",
    "              WHEN X < Y DISPLAY \"LT\"\n",
    "              WHEN OTHER DISPLAY \"GT\"\n",
    "           END-EVALUATE\n",
    "           MOVE 12345678901234567890123456789012345 TO X\n",
    "           COMPUTE Q = X * X / X\n",
    "           DISPLAY Q\n",
    "           COMPUTE Q ROUNDED = X * X / 7\n",
    "           DISPLAY Q\n",
    "           DIVIDE X BY 9999999999 GIVING Q REMAINDER R\n",
    "           DISPLAY Q \" \" R\n",
    "           STOP RUN.\n",
);

/// What cobc 3.2 shows for the program above.
const ARITHMETIC_OUT: &str = concat!(
    "SE1\n",
    "+99999999999999999999999999999999999999\n",
    "+99999999999999999999999999999999999998\n",
    "SE2\n",
    "-99999999999999999999999999999999999999\n",
    "-99999999999999999999999999999999999998\n",
    "SE4\n",
    "-99999999999999999999999999999999999998\n",
    "+12345678901234567890123456789012000000\n",
    "+01763668414462081127160493827001714285 +00000000000000000000000000000000000005\n",
    "+00001000054994024671356024581351884325 +00000000000000000000000000000000007875\n",
    "+00000000000000000000000000000000000012 +00000000493827048049382704804938270492\n",
    "+00000015308641864530864186453086418635\n",
    "+00000001763668414462081127160493827002\n",
    "+00000000000000000000000000000743484079\n",
    "+00000024691357802469135780246913578024\n",
    "+00000025679012123567901212356790121234\n",
    "+0000004115226300411522630041152263004.0\n",
    "+00001763668414462081127160493827001.714\n",
    "+00000012345678901234567890123456789.012\n",
    "SE6\n",
    "+00000.00\n",
    "+00299999999999999999999999999999999997\n",
    "+00099999999999999999999999999999999999\n",
    "+01000000000000000000000000000000000020\n",
    "SE8\n",
    "+01000000000000000000000000000000000020\n",
    "+00000000000000000000003513641828820144\n",
    "+00000001763668414462081127160493827001\n",
    "X>Y\n",
    "X EQ LIT\n",
    "XP>YP\n",
    "NUM\n",
    "POS\n",
    "GT\n",
    "+00012345678901234567890123456789012345\n",
    "+95257526727219068565334095588651771289\n",
    "+00000000000001234567890246913578037037 +00000000000000000000000000000367049382\n",
);

/// Literals of 32 to 38 digits, edited items, and functions over wide arguments.
const MISCELLANY: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. WMISC.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "       01 A PIC 9(38).\n",
    "       01 B PIC 9(3)V9(35).\n",
    "       01 C PIC 9(18) VALUE 0.\n",
    "       01 E1 PIC -(36)9.99.\n",
    "       01 E2 PIC Z(34)9.9(3).\n",
    "       01 X PIC S9(38) VALUE 12345678901234567890123456789012345678.\n",
    "       01 Y PIC S9(35)V9(3)\n",
    "           VALUE -12345678901234567890123456789012345.678.\n",
    "       01 R PIC S9(38) VALUE 0.\n",
    "       PROCEDURE DIVISION.\n",
    "           MOVE 12345678901234567890123456789012345678 TO A\n",
    "           DISPLAY A\n",
    "           MOVE 123456789012345678901234567890123.45678 TO B\n",
    "           DISPLAY B\n",
    "           ADD 12345678901234567890123456789012345 TO C\n",
    "           DISPLAY C\n",
    "           IF A = 12345678901234567890123456789012345678\n",
    "              DISPLAY \"EQ\"\n",
    "           END-IF\n",
    "           DISPLAY 12345678901234567890123456789012345678\n",
    "           MOVE X TO E1\n",
    "           DISPLAY E1\n",
    "           MOVE Y TO E2\n",
    "           DISPLAY E2\n",
    "           COMPUTE R = FUNCTION ABS(Y) + FUNCTION MAX(X Y)\n",
    "           DISPLAY R\n",
    "           COMPUTE R = FUNCTION SUM(X Y X)\n",
    "           DISPLAY R\n",
    "           COMPUTE R = FUNCTION MOD(X, 1000000007)\n",
    "           DISPLAY R\n",
    "           COMPUTE R = FUNCTION INTEGER(Y / 7)\n",
    "           DISPLAY R\n",
    "           STOP RUN.\n",
);

/// What cobc 3.2 shows for the program above.
const MISCELLANY_OUT: &str = concat!(
    "12345678901234567890123456789012345678\n",
    "123.45678000000000000000000000000000000\n",
    "890123456789012345\n",
    "EQ\n",
    "12345678901234567890123456789012345678\n",
    " 345678901234567890123456789012345678.00\n",
    "12345678901234567890123456789012345.678\n",
    "+12358024580135802458013580245801358023\n",
    "+24679012123567901212356790121235679010\n",
    "+00000000000000000000000000000074141290\n",
    "-00001763668414462081127160493827001764\n",
);

#[test]
fn zoned_and_packed_items_of_32_to_38_digits_read_and_move_as_cobc_moves_them() {
    let outcome = both(STORAGE);
    assert_eq!((outcome.out.as_str(), outcome.ending.as_ref().ok()), (STORAGE_OUT, Some(&Ending::StopRun)), "{}", outcome.err);
}

#[test]
fn arithmetic_over_items_of_32_to_38_digits_gives_cobcs_values_and_size_errors() {
    let outcome = both(ARITHMETIC);
    assert_eq!((outcome.out.as_str(), outcome.ending.as_ref().ok()), (ARITHMETIC_OUT, Some(&Ending::StopRun)), "{}", outcome.err);
}

#[test]
fn literals_edited_items_and_functions_of_32_to_38_digits_match_cobc() {
    let outcome = both(MISCELLANY);
    assert_eq!((outcome.out.as_str(), outcome.ending.as_ref().ok()), (MISCELLANY_OUT, Some(&Ending::StopRun)), "{}", outcome.err);
}

#[test]
fn a_picture_or_literal_past_31_digits_warns_with_iwx0130_under_extended_and_is_refused_under_strict() {
    let warned = diagnostics_under(STORAGE, numeric::Compliance::Extended);
    assert_eq!(warned.iter().filter(|d| d.1 == Some("IWX0130")).collect::<Vec<_>>(), [&(5, Some("IWX0130"), Severity::Warning)]);
    assert!(!warned.iter().any(|d| d.1 == Some("IWX0060") || d.2 == Severity::Severe), "{warned:?}");
    let strict = diagnostics_under(STORAGE, numeric::Compliance::Strict);
    assert!(strict.iter().any(|d| d.0 == 5 && d.1 == Some("IWC0069")), "{strict:?}");
    let literal_only = concat!(
        "       IDENTIFICATION DIVISION.\n",
        "       PROGRAM-ID. LIT.\n",
        "       DATA DIVISION.\n",
        "       WORKING-STORAGE SECTION.\n",
        "       01 A PIC 9(5) VALUE 0.\n",
        "       PROCEDURE DIVISION.\n",
        "           IF A = 12345678901234567890123456789012\n",
        "              DISPLAY 'NO'\n",
        "           END-IF\n",
        "           GOBACK.\n",
    );
    let shown = diagnostics_under(literal_only, numeric::Compliance::Extended);
    assert!(shown.iter().any(|d| d.1 == Some("IWX0130")) && !shown.iter().any(|d| d.2 == Severity::Severe), "{shown:?}");
    let shown = diagnostics_under(literal_only, numeric::Compliance::Strict);
    assert!(shown.iter().any(|d| d.1 == Some("IWC0105")), "{shown:?}");
}

#[test]
fn a_card_that_names_arith_keeps_the_limit_of_31_and_39_digits_stay_refused() {
    let card = format!("       CBL ARITH(EXTEND)\n{STORAGE}");
    assert!(diagnostics_under(&card, numeric::Compliance::Extended).iter().any(|d| d.1 == Some("IWC0069")));
    let past_38 = STORAGE.replace("S9(38) COMP-3", "S9(39) COMP-3");
    for compliance in [numeric::Compliance::Extended, numeric::Compliance::Strict] {
        let shown = diagnostics_under(&past_38, compliance);
        assert!(shown.iter().any(|d| d.1 == Some("IWC0263") && d.2 == Severity::Severe), "{shown:?}");
    }
}

#[test]
fn a_binary_item_of_more_than_18_digits_stays_refused_as_cobc_refuses_it() {
    let source = STORAGE.replace("S9(18) COMP VALUE 0", "S9(38) COMP VALUE 0");
    let shown = diagnostics_under(&source, numeric::Compliance::Extended);
    assert!(shown.iter().any(|d| d.1 == Some("IWC0241") && d.2 == Severity::Severe), "{shown:?}");
}

#[test]
fn numval_of_a_text_of_more_than_31_digits_ends_the_run_with_iwr0101() {
    let source = concat!(
        "       IDENTIFICATION DIVISION.\n",
        "       PROGRAM-ID. NUMV.\n",
        "       DATA DIVISION.\n",
        "       WORKING-STORAGE SECTION.\n",
        "       01 S PIC X(40) VALUE '1234567890123456789012345678901234'.\n",
        "       01 A PIC 9(34) VALUE 0.\n",
        "       PROCEDURE DIVISION.\n",
        "           COMPUTE A = FUNCTION NUMVAL(S)\n",
        "           DISPLAY A\n",
        "           GOBACK.\n",
    );
    for executor in [Executor::Interpreter, Executor::Vm] {
        let outcome = Harness::source(source).flags(EXTENDED).run(executor);
        let abend = outcome.ending.expect_err("NUMVAL of 34 digits is refused");
        assert!(abend.message.starts_with("IWR0101-S "), "{}", abend.message);
    }
}
