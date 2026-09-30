pub use rt::edit::{alphanumeric, de_edit, numeric};

#[cfg(test)]
mod tests {
    use crate::picture::analyse;
    use rt::edit::*;

    fn edit(pic: &str, value: i128) -> String {
        let p = analyse(pic).unwrap();
        numeric(p.edit.as_ref().unwrap(), p.digits, value < 0, value.unsigned_abs(), false)
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
        assert_eq!(numeric(p.edit.as_ref().unwrap(), p.digits, false, 0, true), "      ");
    }

    #[test]
    fn de_editing_recovers_the_value() {
        for (pic, v) in [("$$,$$9.99CR", -123450i128), ("ZZ9-", -7), ("ZZ,ZZ9", 1234)] {
            let p = analyse(pic).unwrap();
            let text = numeric(p.edit.as_ref().unwrap(), p.digits, v < 0, v.unsigned_abs(), false);
            assert_eq!(de_edit(p.edit.as_ref().unwrap(), &text), (v < 0, v.unsigned_abs()), "{pic} {text:?}");
        }
    }

    #[test]
    fn alphanumeric_editing() {
        let p = analyse("XXBXX/X").unwrap();
        let out = alphanumeric(p.edit.as_ref().unwrap(), b"ABCDE", b' ', |c| c as u8);
        assert_eq!(out, b"AB CD/E");
    }
}
