pub use rt::edit::{alphanumeric, de_edit, numeric};

#[cfg(test)]
mod tests {
    use crate::picture::{Notation, Picture, analyse, analyse_with};
    use rt::edit::*;
    use syntax::ast::CurrencySign;

    fn currency(p: &Picture) -> &str {
        p.currency.as_deref().unwrap_or_default()
    }

    fn edit(pic: &str, value: i128) -> String {
        let p = analyse(pic).unwrap();
        numeric(p.edit.as_ref().unwrap(), p.digits, value < 0, value.unsigned_abs(), false, '.', currency(&p))
    }

    #[test]
    fn zero_suppression_and_insertion() {
        assert_eq!(edit("ZZ,ZZ9", 1234), " 1,234");
        assert_eq!(edit("ZZ,ZZ9", 5), "     5");
        assert_eq!(edit("ZZZ", 0), "   ");
        assert_eq!(edit("ZZ9.99", 505), "  5.05");
        assert_eq!(edit("ZZ.ZZ", 0), "     ");
        assert_eq!(edit("99/99/99", 270926), "27/09/26");
        assert_eq!(edit("999B999", 123456), "123 456");
    }

    #[test]
    fn check_protection() {
        assert_eq!(edit("**,**9.99", 1234), "****12.34");
        assert_eq!(edit("***", 0), "***");
        assert_eq!(edit("**.**", 0), "**.**");
    }

    #[test]
    fn floating_currency_and_signs() {
        assert_eq!(edit("$$,$$9.99", 123450), "$1,234.50");
        assert_eq!(edit("$$,$$9.99", 550), "    $5.50");
        assert_eq!(edit("----9", -42), "  -42");
        assert_eq!(edit("----9", 42), "   42");
        assert_eq!(edit("++++9", 42), "  +42");
    }

    #[test]
    fn fixed_signs_cr_and_db() {
        assert_eq!(edit("-ZZ9", -7), "-  7");
        assert_eq!(edit("+ZZ9", 7), "+  7");
        assert_eq!(edit("ZZ9-", -7), "  7-");
        assert_eq!(edit("ZZ9CR", -7), "  7CR");
        assert_eq!(edit("ZZ9DB", 7), "  7  ");
        assert_eq!(edit("$ZZ9.99", 1999), "$ 19.99");
    }

    #[test]
    fn blank_when_zero() {
        let p = analyse("999.99").unwrap();
        assert_eq!(numeric(p.edit.as_ref().unwrap(), p.digits, false, 0, true, '.', currency(&p)), "      ");
    }

    #[test]
    fn de_editing_recovers_the_value() {
        for (pic, v) in [("$$,$$9.99CR", -123450i128), ("ZZ9-", -7), ("ZZ,ZZ9", 1234)] {
            let p = analyse(pic).unwrap();
            let text = numeric(p.edit.as_ref().unwrap(), p.digits, v < 0, v.unsigned_abs(), false, '.', currency(&p));
            assert_eq!(de_edit(p.edit.as_ref().unwrap(), &text, currency(&p)), (v < 0, v.unsigned_abs()), "{pic} {text:?}");
        }
    }

    #[test]
    fn under_decimal_point_is_comma_a_comma_shows_the_point_and_de_editing_still_recovers_the_value() {
        let edit = |pic: &str, value: i128| {
            let p = analyse_with(pic, Notation { decimal_comma: true, currency: &[] }).unwrap();
            let text = numeric(p.edit.as_ref().unwrap(), p.digits, value < 0, value.unsigned_abs(), false, ',', currency(&p));
            assert_eq!(de_edit(p.edit.as_ref().unwrap(), &text, currency(&p)), (value < 0, value.unsigned_abs()), "{pic} {text:?}");
            text
        };
        assert_eq!(edit("Z.ZZ9,99-", -123450), "1.234,50-");
        assert_eq!(edit("$$.$$9,99", 550), "    $5,50");
        assert_eq!(edit("***.**9,99", 1234), "*****12,34");
        assert_eq!(edit("99.999", 12345), "12.345");
    }

    #[test]
    fn insertion_outside_a_suppression_string_is_always_inserted_and_check_protection_covers_cr() {
        assert_eq!(edit("$0(10)999", 492), "$0000000000492");
        assert_eq!(edit("0ZZ9", 5), "0  5");
        assert_eq!(edit("ZZ0Z9", 5), "    5");
        let p = analyse("$**.**CR").unwrap();
        assert_eq!(numeric(p.edit.as_ref().unwrap(), p.digits, false, 0, false, '.', currency(&p)), "$**.****".replace('$', "*"));
    }

    #[test]
    fn a_currency_sign_value_fills_the_first_currency_position_fixed_or_floating() {
        let signs = [CurrencySign { value: "W".into(), symbol: 'W', hex: None }, CurrencySign { value: "EUR ".into(), symbol: 'U', hex: None }];
        let edit = |pic: &str, value: i128| {
            let p = analyse_with(pic, Notation { decimal_comma: true, currency: &signs }).unwrap();
            let text = numeric(p.edit.as_ref().unwrap(), p.digits, value < 0, value.unsigned_abs(), false, ',', currency(&p));
            assert_eq!(text.chars().count(), p.size as usize, "{pic} {text:?}");
            assert_eq!(de_edit(p.edit.as_ref().unwrap(), &text, currency(&p)), (value < 0, value.unsigned_abs()), "{pic} {text:?}");
            text
        };
        assert_eq!(edit("W9999", 1234), "W1234");
        assert_eq!(edit("WWWWW", 12), "  W12");
        assert_eq!(edit("U9.999,99", 123456), "EUR 1.234,56");
        assert_eq!(edit("UUUU9,99-", -550), "   EUR 5,50-");
        assert_eq!(edit("UUUU9,99", 123456), "EUR 1234,56");
        let p = analyse_with("U**9", Notation { decimal_comma: false, currency: &signs }).unwrap();
        assert_eq!(numeric(p.edit.as_ref().unwrap(), p.digits, false, 0, false, '.', currency(&p)), "EUR **0");
    }

    #[test]
    fn alphanumeric_editing() {
        let p = analyse("XXBXX/X").unwrap();
        let out = alphanumeric(p.edit.as_ref().unwrap(), b"ABCDE", b' ', |c| c as u8);
        assert_eq!(out, b"AB CD/E");
    }
}
