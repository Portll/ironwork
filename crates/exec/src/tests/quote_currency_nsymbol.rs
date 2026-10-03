use super::*;

fn quotes(card: &str) -> String {
    run(&program(
        card,
        "       01  X PIC X(3).\n       01  Y PIC X VALUE QUOTE.\n       01  N PIC N(2).\n",
        &[line("MOVE ALL QUOTES TO X"), line("MOVE QUOTE TO N"), line("DISPLAY '[' X '][' Y '][' QUOTE '][' N ']'"), line("GOBACK.")].concat(),
    ))
}

#[test]
fn quote_is_a_quotation_mark_unless_apost_makes_it_an_apostrophe() {
    assert_eq!(quotes(""), "[\"\"\"][\"][\"][\"\"]\n");
    assert_eq!(quotes("QUOTE"), "[\"\"\"][\"][\"][\"\"]\n");
    assert_eq!(quotes("APOST"), "[''']['][']['']\n");
}

#[test]
fn an_alphabet_clause_takes_quote_as_apost_or_quote_says() {
    let source = |card: &str| {
        format!(
            concat!(
                "{}       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n",
                "       OBJECT-COMPUTER. IBM-370 PROGRAM COLLATING SEQUENCE IS PCS.\n",
                "       SPECIAL-NAMES.\n           ALPHABET PCS IS QUOTE, 'A'.\n",
                "       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       PROCEDURE DIVISION.\n{}"
            ),
            card,
            [line("DISPLAY FUNCTION CHAR(1) FUNCTION CHAR(2)"), line("GOBACK.")].concat()
        )
    };
    assert_eq!(run(&source("")), "\"A\n");
    assert_eq!(run(&source("       CBL APOST\n")), "'A\n");
}

fn edited(card: &str, data: &str) -> (String, String, Result<Ending, Abend>) {
    run_with(&program(card, data, &[line("MOVE 1234.5 TO E"), line("DISPLAY E"), line("GOBACK.")].concat()), &[])
}

#[test]
fn currency_makes_its_character_the_currency_symbol_in_place_of_the_dollar() {
    let pounds = "       01  E PIC ££,££9.99.\n";
    assert_eq!(edited("CURRENCY('£')", pounds).0, "£1,234.50\n");
    assert_eq!(edited("CURR(X'B1')", pounds).0, "£1,234.50\n", "X'B1' is £ in CCSID 1140");
    assert_eq!(edited("NOCURRENCY", "       01  E PIC $$,$$9.99.\n").0, "$1,234.50\n");
    let errors = compile_errors(&program("CURRENCY('£')", "       01  E PIC $$,$$9.99.\n", &line("GOBACK.")));
    assert!(errors.contains("'$' is not a currency symbol of this program"), "{errors}");
}

#[test]
fn a_currency_sign_clause_makes_the_currency_option_ignored() {
    let source = |card: &str, picture: &str| {
        format!(
            concat!(
                "       CBL {}\n       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n",
                "       SPECIAL-NAMES.\n           CURRENCY SIGN IS 'CHF ' WITH PICTURE SYMBOL 'F'.\n",
                "       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  E PIC {}.\n       PROCEDURE DIVISION.\n{}"
            ),
            card,
            picture,
            [line("MOVE 12.5 TO E"), line("DISPLAY E"), line("GOBACK.")].concat()
        )
    };
    assert_eq!(run(&source("CURRENCY('£')", "F99.99")), "CHF 12.50\n");
    assert!(compile_errors(&source("CURRENCY('£')", "£99.99")).contains("PICTURE £99.99"));
}

#[test]
fn a_hexadecimal_currency_whose_character_cannot_be_a_currency_symbol_is_discarded_with_an_error() {
    let parsed = syntax::parse(&program("CURRENCY(X'F1')", "       01  E PIC $$,$$9.99.\n", &[line("MOVE 1234.5 TO E"), line("DISPLAY E"), line("GOBACK.")].concat())).unwrap();
    let compiled = compile(parsed, &[]).unwrap();
    let messages: Vec<_> = compiled.diagnostics.iter().map(|e| (e.message.as_str(), e.severity)).collect();
    assert_eq!(messages, [("CBL CURRENCY: code page 1140 reads its byte as '1', which cannot be a currency symbol", Severity::Error)]);
    assert_eq!(compiled.options.currency, None);
}

#[test]
fn nsymbol_dbcs_leaves_a_usage_national_item_national() {
    assert_eq!(run(&program("NSYMBOL(DBCS)", "       01  N PIC N(2) USAGE NATIONAL VALUE ALL SPACE.\n", &[line("DISPLAY '[' N ']'"), line("GOBACK.")].concat())), "[  ]\n");
}

#[test]
fn nsymbol_national_with_nodbcs_warns_and_keeps_dbcs() {
    let parsed = syntax::parse(&program("NODBCS,NSYMBOL(NATIONAL)", "", &line("GOBACK."))).unwrap();
    let compiled = compile(parsed, &[]).unwrap();
    let messages: Vec<_> = compiled.diagnostics.iter().map(|e| (e.message.as_str(), e.severity)).collect();
    assert_eq!(messages, [("CBL NODBCS: NSYMBOL(NATIONAL) requires DBCS, which is in effect", Severity::Warning)]);
    assert!(compiled.options.dbcs);
    let alone = compile(syntax::parse(&program("NODBCS", "", &line("GOBACK."))).unwrap(), &[]).unwrap();
    assert!(!alone.options.dbcs && alone.diagnostics.is_empty());
}
