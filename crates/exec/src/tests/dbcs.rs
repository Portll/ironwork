//! DBCS data: USAGE DISPLAY-1 items, DBCS literals and mixed data, under the mixed code pages of
//! the Programming Guide's Table 47 and under a single-byte one.

use super::*;
use rt::storage::Kind;

const JAPANESE: &str = "CODEPAGE(939)";

fn shown(card: &str, data: &str, statements: &[&str]) -> String {
    let mut body: String = statements.iter().map(|s| line(s)).collect();
    body.push_str(&line("GOBACK."));
    run(&program(card, data, &body))
}

fn compiled(card: &str, data: &str) -> Compiled {
    compile(syntax::parse(&program(card, data, &line("GOBACK."))).unwrap_or_else(|e| panic!("{e}")), &[]).unwrap_or_else(|e| panic!("{e:?}"))
}

#[test]
fn display_1_items_take_two_bytes_a_character_and_picture_g_needs_display_1() {
    let data = "       01  D PIC G(3) USAGE DISPLAY-1.\n       01  N PIC N(2) USAGE DISPLAY-1.\n       01  E PIC GGBG DISPLAY-1.\n";
    let c = compiled(JAPANESE, data);
    let item = |name: &str| c.layout.items.iter().find(|i| i.name.as_deref() == Some(name)).unwrap();
    assert_eq!((item("D").size, item("N").size, item("E").size), (6, 4, 8));
    assert_eq!(item("D").kind, Kind::Dbcs { justified: false, edit: None });
    assert!(matches!(item("E").kind, Kind::Dbcs { edit: Some(_), .. }));
    assert!(compile_errors(&program("", "       01  X PIC G(2).\n", &line("GOBACK."))).contains("a PICTURE with G needs USAGE DISPLAY-1"));
    assert!(compile_errors(&program("", "       01  X PIC GX DISPLAY-1.\n", &line("GOBACK."))).contains("'X' cannot be in a PICTURE of G"));
}

#[test]
fn a_dbcs_move_pads_with_dbcs_spaces_truncates_by_characters_and_puts_a_dbcs_space_at_b() {
    let data = concat!(
        "       01  D PIC G(3) DISPLAY-1.\n       01  J PIC G(3) DISPLAY-1 JUSTIFIED RIGHT.\n",
        "       01  E PIC GBG DISPLAY-1.\n       01  S PIC G DISPLAY-1.\n",
    );
    let statements = [
        "MOVE G'ＡＢ' TO D J E",
        "MOVE G'ＡＢＣＤ' TO S",
        "DISPLAY FUNCTION HEX-OF(D) ' ' FUNCTION HEX-OF(J)",
        "DISPLAY FUNCTION HEX-OF(E) ' ' FUNCTION HEX-OF(S)",
        "MOVE SPACE TO D",
        "DISPLAY FUNCTION HEX-OF(D)",
        "MOVE ALL G'Ｚ' TO D",
        "DISPLAY FUNCTION HEX-OF(D)",
    ];
    assert_eq!(shown(JAPANESE, data, &statements), "42C142C24040 404042C142C2\n42C1404042C2 42C1\n404040404040\n42E942E942E9\n");
}

#[test]
fn a_group_move_carries_dbcs_bytes_and_display_shows_dbcs_and_mixed_data_as_characters() {
    let data = concat!(
        "       01  G.\n           05 D PIC G(3) DISPLAY-1.\n       01  K PIC G(3) DISPLAY-1 VALUE G'日本'.\n",
        "       01  A PIC X(8) VALUE 'Aあ日B'.\n",
    );
    let statements = [
        "MOVE X'42C142C2' TO G",
        "DISPLAY '[' D ']' '[' K ']'",
        "DISPLAY A ' ' FUNCTION HEX-OF(A)",
        "DISPLAY G'あい'",
    ];
    assert_eq!(shown(JAPANESE, data, &statements), "[ＡＢ\u{3000}][日本\u{3000}]\nAあ日B C10E448145620FC2\nあい\n");
}

#[test]
fn dbcs_comparisons_pad_with_dbcs_spaces_and_meet_national_through_the_page() {
    let data = concat!(
        "       01  D PIC G(3) DISPLAY-1 VALUE G'ＡＢ'.\n",
        "       01  N PIC N(3) USAGE NATIONAL VALUE N'ＡＢ\u{3000}'.\n",
        "       01  M PIC N(3) USAGE NATIONAL VALUE N'ＡＢ'.\n",
    );
    let statements = [
        "IF D = G'ＡＢ' DISPLAY 'PADDED' END-IF",
        "IF D > G'ＡＡＺ' DISPLAY 'BINARY' END-IF",
        "IF D NOT = SPACE DISPLAY 'NOT SPACE' END-IF",
        "IF D = N DISPLAY 'NATIONAL' END-IF",
        "IF D > M DISPLAY 'U+3000 > U+0020' END-IF",
    ];
    assert_eq!(shown(JAPANESE, data, &statements), "PADDED\nBINARY\nNOT SPACE\nNATIONAL\nU+3000 > U+0020\n");
}

#[test]
fn the_dbcs_and_kanji_classes_check_each_character_s_bytes() {
    let data = "       01  G.\n           05 D PIC G(2) DISPLAY-1.\n";
    let test = |hex: &str| {
        let statements = [
            &format!("MOVE X'{hex}' TO G")[..],
            "IF D DBCS DISPLAY 'DBCS' ELSE DISPLAY 'NOT DBCS' END-IF",
            "IF D KANJI DISPLAY 'KANJI' ELSE DISPLAY 'NOT KANJI' END-IF",
        ];
        shown(JAPANESE, data, &statements)
    };
    assert_eq!(test("45624040"), "DBCS\nKANJI\n");
    assert_eq!(test("81414040"), "DBCS\nNOT KANJI\n");
    assert_eq!(test("40414040"), "NOT DBCS\nNOT KANJI\n");
}

#[test]
fn initialize_reference_modification_length_and_national_of_count_dbcs_characters() {
    let data = concat!(
        "       01  G.\n           05 D PIC G(3) DISPLAY-1 VALUE G'ＡＢＣ'.\n",
        "       01  N PIC N(3) USAGE NATIONAL.\n       01  L PIC 99.\n",
    );
    let statements = [
        "DISPLAY D(2:1) ' ' FUNCTION HEX-OF(D(2:2))",
        "COMPUTE L = FUNCTION LENGTH(D)",
        "DISPLAY L ' ' LENGTH OF D",
        "MOVE FUNCTION NATIONAL-OF(D) TO N",
        "DISPLAY N UPON CONSOLE",
        "MOVE G'Ｘ' TO D",
        "MOVE D TO N",
        "DISPLAY N UPON CONSOLE",
        "INITIALIZE G",
        "DISPLAY FUNCTION HEX-OF(D)",
        "INITIALIZE G REPLACING DBCS DATA BY G'Ｙ'",
        "DISPLAY D",
    ];
    assert_eq!(shown(JAPANESE, data, &statements), "Ｂ 42C242C3\n03 000000006\nＡＢＣ\nＸ\u{3000}\u{3000}\n404040404040\nＹ\u{3000}\u{3000}\n");
}

#[test]
fn string_unstring_and_inspect_work_in_dbcs_characters() {
    let data = concat!(
        "       01  D PIC G(3) DISPLAY-1.\n       01  X PIC G(2) DISPLAY-1.\n       01  Y PIC G(2) DISPLAY-1.\n",
        "       01  P PIC 99 VALUE 1.\n       01  C PIC 99.\n       01  T PIC 99 VALUE 0.\n",
    );
    let statements = [
        "STRING G'ＡＢ' G'Ｃ' DELIMITED BY SIZE INTO D WITH POINTER P",
        "DISPLAY D ' ' P",
        "UNSTRING D DELIMITED BY G'Ｂ' INTO X COUNT IN C Y",
        "DISPLAY X '|' Y '|' C",
        "INSPECT D TALLYING T FOR ALL G'Ｂ'",
        "INSPECT D TALLYING T FOR CHARACTERS",
        "INSPECT D REPLACING ALL G'Ｂ' BY G'Ｘ'",
        "DISPLAY D ' ' T",
    ];
    assert_eq!(shown(JAPANESE, data, &statements), "ＡＢＣ 04\nＡ\u{3000}|Ｃ\u{3000}|01\nＡＸＣ 04\n");
}

#[test]
fn nsymbol_dbcs_makes_n_dbcs_and_leaves_usage_national_alone() {
    let data = "       01  N1 PIC N(3).\n       01  N2 PIC N(3) USAGE NATIONAL.\n       01  G USAGE NATIONAL.\n           05  N3 PIC NN.\n";
    let c = compiled("NSYMBOL(DBCS),CODEPAGE(939)", data);
    let kind = |name: &str| c.layout.items.iter().find(|i| i.name.as_deref() == Some(name)).unwrap().kind;
    assert_eq!((kind("N1"), kind("N2"), kind("N3")), (Kind::Dbcs { justified: false, edit: None }, Kind::National, Kind::National));
    let c = compiled("NS(NAT)", data);
    assert_eq!(c.layout.items.iter().find(|i| i.name.as_deref() == Some("N1")).unwrap().kind, Kind::National);
    assert_eq!(shown("NSYMBOL(DBCS),CODEPAGE(939)", "", &["DISPLAY N'ＡＢ' NX'00410042' UPON CONSOLE"]), "ＡＢAB\n");
}

#[test]
fn a_single_byte_page_moves_dbcs_bytes_shows_the_dbcs_space_and_refuses_a_dbcs_literal() {
    let data = "       01  G.\n           05 D PIC G(2) DISPLAY-1 VALUE SPACE.\n";
    assert_eq!(shown("", data, &["DISPLAY '[' D ']'", "MOVE X'42C14040' TO G", "DISPLAY '[' D ']'"]), "[\u{3000}\u{3000}]\n[\u{FFFD}\u{3000}]\n");
    let (_, _, ending) = run_with(&program("", data, &[line("MOVE G'Ａ' TO D"), line("GOBACK.")].concat()), &[]);
    let abend = ending.unwrap_err();
    assert_eq!(abend.code, AbendCode::Ironwork);
    assert!(abend.message.contains("CODEPAGE(1140) is a single-byte page with no DBCS characters"), "{}", abend.message);
}

#[test]
fn a_dbcs_item_s_value_is_a_dbcs_literal_that_fits_space_or_all() {
    let errors = |data: &str| compile_errors(&program(JAPANESE, data, &line("GOBACK.")));
    assert!(errors("       01  D PIC G(2) DISPLAY-1 VALUE 'AB'.\n").contains("D: a DBCS item's VALUE is a DBCS literal of at most 2 characters"));
    assert!(errors("       01  D PIC G(2) DISPLAY-1 VALUE G'ＡＢＣ'.\n").contains("at most 2 characters"));
    assert!(errors("       01  X PIC X(4) VALUE G'ＡＢ'.\n").contains("X: a DBCS literal can be the VALUE of a DBCS item only"));
    assert_eq!(errors("       01  D PIC G(2) DISPLAY-1 VALUE ALL G'Ｚ'.\n       01  E PIC G(2) DISPLAY-1 VALUE SPACE.\n"), "");
    assert!(syntax::parse(&program(JAPANESE, "", &[line("DISPLAY G''"), line("GOBACK.")].concat())).unwrap_err().message.contains("a DBCS literal holds 1 to 28 characters, not 0"));
}

#[test]
fn national_string_and_unstring_count_national_characters_too() {
    let data = concat!(
        "       01  N PIC N(4) USAGE NATIONAL.\n       01  X PIC N(2) USAGE NATIONAL.\n       01  Y PIC N(2) USAGE NATIONAL.\n",
        "       01  P PIC 99 VALUE 2.\n       01  C PIC 99.\n",
    );
    let statements = [
        "MOVE N'----' TO N",
        "STRING N'AB' DELIMITED BY SIZE INTO N WITH POINTER P",
        "DISPLAY N ' ' P UPON CONSOLE",
        "UNSTRING N DELIMITED BY N'B' INTO X COUNT IN C Y",
        "DISPLAY X '|' Y '|' C UPON CONSOLE",
    ];
    assert_eq!(shown("", data, &statements), "-AB- 04\n-A|- |02\n");
}
