use super::*;

fn collated(alphabet: &str, data: &str, procedure: &str) -> String {
    format!(
        concat!(
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n",
            "       OBJECT-COMPUTER. IBM-370 PROGRAM COLLATING SEQUENCE IS PCS.\n",
            "       SPECIAL-NAMES.\n           ALPHABET PCS IS\n           {}.\n",
            "       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n{}       PROCEDURE DIVISION.\n{}"
        ),
        alphabet, data, procedure
    )
}

/// X'E9' (Z) down to X'C1' (A), 41 positions with the gaps between the letters, then 0 and 9 in
/// one position, then X'FF'; every other character follows in EBCDIC order, so LOW-VALUE is Z and
/// HIGH-VALUE X'FE'.
const BACKWARDS: &str = "'Z' THROUGH 'A' '0' ALSO '9', HIGH-VALUE";

pub(super) fn fuzz_seeds() -> Vec<String> {
    let data = [
        "       01  C1 PIC X VALUE 'B'.\n           88 EARLY VALUE 'C' THRU 'A'.\n",
        "       01  REC.\n           05 CNT PIC 9.\n           05 ITEM PIC X OCCURS 1 TO 5 DEPENDING ON CNT.\n       01  Z PIC 9(18).\n",
    ]
    .concat();
    vec![collated(BACKWARDS, &data, &[line("IF EARLY MOVE FUNCTION CHAR(3) TO C1 END-IF"), line("MOVE C1 TO REC ADD 1 TO Z"), line("GOBACK.")].concat())]
}

#[test]
fn relation_and_condition_name_conditions_follow_the_program_collating_sequence() {
    let out = run(&collated(
        BACKWARDS,
        concat!(
            "       01  C1 PIC X VALUE 'B'.\n           88 EARLY VALUE 'C' THRU 'A'.\n       01  W PIC X(3) VALUE 'ABC'.\n",
            "       01  N1 PIC 9 VALUE 1.\n       01  NA PIC N VALUE N'A'.\n       01  NB PIC N VALUE N'B'.\n",
        ),
        &[
            line("IF 'A' > 'B' DISPLAY 'A>B' END-IF"),
            line("IF EARLY DISPLAY 'EARLY' END-IF"),
            line("IF '0' = '9' DISPLAY '0=9' END-IF"),
            line("IF W < 'ABD' DISPLAY 'W<ABD' ELSE DISPLAY 'W>=ABD' END-IF"),
            line("EVALUATE C1 WHEN 'C' THRU 'A' DISPLAY 'IN'"),
            line("    WHEN OTHER DISPLAY 'OUT' END-EVALUATE"),
            line("IF N1 < 2 DISPLAY 'NUMERIC' END-IF"),
            line("IF NA < NB DISPLAY 'NATIONAL' END-IF"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    assert_eq!(out, "A>B\nEARLY\n0=9\nW>=ABD\nIN\nNUMERIC\nNATIONAL\n");
}

#[test]
fn high_value_low_value_char_ord_max_and_min_follow_it_too() {
    let out = run(&collated(
        BACKWARDS,
        "       01  C1 PIC X.\n       01  C2 PIC X(2).\n       01  R PIC 999.\n",
        &[
            line("MOVE LOW-VALUE TO C1"),
            line("DISPLAY C1"),
            line("MOVE HIGH-VALUE TO C1"),
            line("IF C1 = X'FE' AND C1 = HIGH-VALUE DISPLAY 'HIGH FE' END-IF"),
            line("COMPUTE R = FUNCTION ORD('Z')"),
            line("DISPLAY R"),
            line("COMPUTE R = FUNCTION ORD('9')"),
            line("DISPLAY R"),
            line("COMPUTE R = FUNCTION ORD(X'FF')"),
            line("DISPLAY R"),
            line("MOVE FUNCTION CHAR(42) TO C1"),
            line("DISPLAY C1"),
            line("MOVE FUNCTION CHAR(41) TO C1"),
            line("DISPLAY C1"),
            line("MOVE FUNCTION MAX('A', 'M', 'B') TO C1"),
            line("DISPLAY C1"),
            line("MOVE FUNCTION MIN('A', 'M', 'B') TO C1"),
            line("DISPLAY C1"),
            line("MOVE FUNCTION MAX('AB', 'A') TO C2"),
            line("DISPLAY '[' C2 ']'"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    assert_eq!(out, "Z\nHIGH FE\n001\n042\n043\n0\nA\nA\nM\n[A ]\n");
}

#[test]
fn standard_1_is_ascii_order() {
    let out = run(&collated(
        "STANDARD-1",
        "       01  C1 PIC X.\n       01  R PIC 999.\n",
        &[
            line("IF 'a' > 'A' AND '1' < 'A' AND SPACE < '0'"),
            line("    DISPLAY 'ASCII' END-IF"),
            line("COMPUTE R = FUNCTION ORD('A')"),
            line("DISPLAY R"),
            line("MOVE FUNCTION CHAR(98) TO C1"),
            line("DISPLAY C1"),
            line("MOVE LOW-VALUE TO C1"),
            line("IF C1 = X'00' DISPLAY 'LOW 00' END-IF"),
            line("GOBACK."),
        ]
        .concat(),
    ));
    assert_eq!(out, "ASCII\n066\na\nLOW 00\n");
}

#[test]
fn search_all_and_a_contained_program_use_it() {
    let source = concat!(
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. OUTER.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n",
        "       OBJECT-COMPUTER. IBM-370 PROGRAM COLLATING SEQUENCE IS PCS.\n",
        "       SPECIAL-NAMES. ALPHABET PCS IS 'Z' THROUGH 'A'.\n",
        "       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n",
        "       01  T VALUE 'ZXTMC'.\n           05 K PIC X OCCURS 5 ASCENDING KEY IS K INDEXED BY I.\n",
        "       PROCEDURE DIVISION.\n",
        "           SEARCH ALL K AT END DISPLAY 'MISSING'\n               WHEN K(I) = 'X' DISPLAY 'FOUND X' END-SEARCH\n",
        "           CALL 'INNER'\n           GOBACK.\n",
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. INNER.\n       PROCEDURE DIVISION.\n",
        "           IF 'A' > 'B' DISPLAY 'INNER TOO' END-IF\n           GOBACK.\n",
        "       END PROGRAM INNER.\n       END PROGRAM OUTER.\n",
    );
    let (out, err, ending) = run_unit(source, vec![], "");
    assert!(ending.is_ok(), "{ending:?} {err}");
    assert_eq!(out, "FOUND X\nINNER TOO\n");
}

#[test]
fn an_alphabet_that_cannot_be_one_is_refused() {
    let errors = |alphabet: &str| {
        let source = collated(alphabet, "", &line("GOBACK."));
        compile(syntax::parse(&source).unwrap_or_else(|e| panic!("{e}")), &[]).err().map(|e| e[0].message.clone()).unwrap_or_default()
    };
    assert!(errors("'A' 'B' ALSO 'A'").contains("X'C1' is given more than one position"), "{}", errors("'A' 'B' ALSO 'A'"));
    assert!(errors("0").contains("0 is not an ordinal position from 1 to 256"));
    assert!(errors("'AB' THRU 'C'").contains("one character"));
    assert!(errors("N'A'").contains("national literal"));
    assert_eq!(errors("256 2 'Q' ALSO LOW-VALUE"), "");
    let undefined = collated("NATIVE", "", &line("GOBACK.")).replace("SEQUENCE IS PCS", "SEQUENCE IS NONE");
    let e = compile(syntax::parse(&undefined).unwrap(), &[]).err().unwrap();
    assert!(e[0].message.contains("PROGRAM COLLATING SEQUENCE NONE: not an alphabet-name"), "{}", e[0].message);
}

#[test]
fn a_sort_s_ascii_order_is_standard_1_s() {
    for ccsid in [37, 1140, 1047] {
        let page = zarch::ebcdic::CodePage::by_ccsid(ccsid).unwrap();
        let standard = crate::collating::Sequence::of(&syntax::ast::Alphabet::Standard1, page).unwrap();
        assert_eq!(rt::sort::Collating::ascii(page), rt::sort::Collating::Positions(std::rc::Rc::new(standard.positions())), "CCSID {ccsid}");
    }
}
