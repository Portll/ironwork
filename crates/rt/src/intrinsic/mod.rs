//! The intrinsic functions' semantics, apart from how an executor evaluates their arguments.

use numeric::Arith;
use numeric::precision::{Places, carried, product_places, quotient_places, sum_places};

pub mod dates;
pub mod datetime;
pub mod function;
pub mod math;
pub mod numval;
pub mod real;
pub mod text;
pub mod unicode;

/// The first twenty-one functions ironwork for COBOL runs.
pub const FIRST: &[&str] = &[
    "CHAR", "ORD", "NATIONAL-OF", "LENGTH", "UPPER-CASE", "LOWER-CASE", "REVERSE", "CURRENT-DATE", "NUMVAL", "NUMVAL-C", "TRIM", "MOD", "REM",
    "INTEGER", "INTEGER-PART", "ABS", "MIN", "MAX", "INTEGER-OF-DATE", "DATE-OF-INTEGER", "RANDOM",
];

/// The functions beyond the first twenty-one that ironwork for COBOL runs.
pub const FUNCTIONS: &[&str] = &[
    "ACOS", "ANNUITY", "ASIN", "ATAN", "BIT-OF", "BIT-TO-CHAR", "BYTE-LENGTH", "COS", "DATE-TO-YYYYMMDD", "DAY-OF-INTEGER",
    "DAY-TO-YYYYDDD", "DISPLAY-OF", "E", "EXP", "EXP10", "FACTORIAL", "FORMATTED-CURRENT-DATE", "FORMATTED-DATE", "FORMATTED-DATETIME",
    "FORMATTED-TIME", "HEX-OF", "HEX-TO-CHAR", "INTEGER-OF-DAY", "INTEGER-OF-FORMATTED-DATE", "LOG", "LOG10",
    "MEAN", "MEDIAN", "MIDRANGE", "NUMVAL-F", "ORD-MAX", "ORD-MIN", "PI", "PRESENT-VALUE", "RANGE", "SECONDS-FROM-FORMATTED-TIME",
    "SECONDS-PAST-MIDNIGHT", "SIGN", "SIN", "SQRT", "STANDARD-DEVIATION", "SUM", "TAN", "TEST-DATE-YYYYMMDD", "TEST-DAY-YYYYDDD",
    "TEST-FORMATTED-DATETIME", "TEST-NUMVAL", "TEST-NUMVAL-C", "TEST-NUMVAL-F", "UUID4", "VARIANCE", "YEAR-TO-YYYY",
    "ULENGTH", "UPOS", "USUBSTR", "USUPPLEMENTARY", "UVALID", "UWIDTH", "COMBINED-DATETIME", "CONTENT-OF",
    "WHEN-COMPILED", "MODULE-CALLER-ID", "ARGUMENT LENGTH", "STORED-CHAR-LENGTH", "HEAP ALLOCATE",
    "HEAP FREE", "CONCATENATE", "CHAINING ARGUMENT", "CRT STATUS", "SUBSTITUTE", "SUBSTITUTE-CASE",
];

/// Functions of type alphanumeric or national, or whose type follows an argument that may be one
/// (Language Reference SC27-8713-03, p. 77 and Part 7).
pub const CHARACTER_VALUED: &[&str] = &[
    "BIT-OF", "BIT-TO-CHAR", "CHAR", "CONTENT-OF", "CURRENT-DATE", "DISPLAY-OF", "FORMATTED-CURRENT-DATE", "FORMATTED-DATE",
    "FORMATTED-DATETIME", "FORMATTED-TIME", "HEX-OF", "HEX-TO-CHAR", "LOWER-CASE", "MAX", "MIN", "NATIONAL-OF", "REVERSE", "TRIM",
    "UPPER-CASE", "USUBSTR", "UUID4", "WHEN-COMPILED", "MODULE-CALLER-ID", "CONCATENATE", "CHAINING ARGUMENT",
    "SUBSTITUTE", "SUBSTITUTE-CASE",
];

/// Functions whose result is long floating point under ARITH(COMPAT) and extended under
/// ARITH(EXTEND), so an expression holding one is evaluated in floating point (Programming Guide
/// SC27-8714-03, pp. 56-58, 62-63, 800-801; assumption C111).
pub const FLOATING_POINT: &[&str] = &[
    "ACOS", "ANNUITY", "ASIN", "ATAN", "COMBINED-DATETIME", "COS", "E", "EXP", "EXP10", "LOG", "LOG10", "MEAN", "MEDIAN", "MIDRANGE", "NUMVAL", "NUMVAL-C", "NUMVAL-F", "PI",
    "PRESENT-VALUE", "RANDOM", "SECONDS-FROM-FORMATTED-TIME", "SECONDS-PAST-MIDNIGHT", "SIN", "SQRT", "STANDARD-DEVIATION", "TAN", "VARIANCE",
];

/// Functions of numeric arguments that are floating point when any argument is (p. 799; assumptions
/// C100 and C111).
pub const MIXED: &[&str] = &["ABS", "MAX", "MIN", "RANGE", "REM", "SUM"];

/// The decimal places a function's value contributes to the dmax of an expression holding it, its
/// outer-dmax, given the most decimal places among its arguments, its inner-dmax: all of them for
/// a mixed function, none for any other (Programming Guide SC27-8714-03, pp. 794, 798-799;
/// assumptions C390 and C393).
pub fn outer_dmax(name: &str, inner: u32) -> u32 {
    if MIXED.contains(&name) { inner } else { 0 }
}

/// The places of a function's fixed-point value, given its arguments' places; None for a function
/// whose places do not follow from its arguments' alone. Each mixed function's step follows the
/// fixed-point table (Programming Guide SC27-8714-03, pp. 795, 799); INTEGER, INTEGER-PART and MOD
/// have the digits pp. 798-799 give them (assumptions C390 to C393).
pub fn fixed_places(name: &str, args: &[Places], arith: Arith) -> Option<Places> {
    let first = *args.first()?;
    let widest = args.iter().fold(first, |p, a| Places::new(p.int.max(a.int), p.dec.max(a.dec)));
    Some(match name {
        "MAX" | "MIN" => widest,
        "RANGE" => carried(sum_places(widest, widest), widest.dec, arith),
        "ABS" => first,
        "INTEGER" => carried(Places::new(first.total() + 1, 0), 0, arith),
        "INTEGER-PART" => carried(Places::new(first.total(), 0), 0, arith),
        "MOD" => {
            let second = *args.get(1)?;
            Places::new(first.int.min(second.int), widest.dec)
        }
        "REM" => {
            let second = *args.get(1)?;
            let quotient = carried(quotient_places(first, second, widest.dec), widest.dec, arith);
            let product = carried(product_places(Places::new(quotient.int, 0), second), widest.dec, arith);
            carried(sum_places(first, product), widest.dec, arith)
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_fixed_point_function_has_the_places_ibm_gives_it() {
        let (n3, d2, s9, h3) = (Places::new(3, 0), Places::new(1, 2), Places::new(1, 0), Places::new(3, 0));
        let of = |name: &str, args: &[Places]| fixed_places(name, args, Arith::Compat);
        assert_eq!(of("MAX", &[n3, d2]), Some(Places::new(3, 2)));
        assert_eq!(of("MIN", &[d2, Places::new(5, 0)]), Some(Places::new(5, 2)));
        assert_eq!(of("RANGE", &[n3, d2]), Some(Places::new(4, 2)));
        assert_eq!(of("INTEGER", &[d2]), Some(Places::new(4, 0)));
        assert_eq!(of("INTEGER-PART", &[d2]), Some(Places::new(3, 0)));
        assert_eq!(of("MOD", &[s9, h3]), Some(Places::new(1, 0)));
        assert_eq!(of("REM", &[n3, d2]), Some(Places::new(7, 2)));
        assert_eq!(of("ABS", &[d2]), Some(d2));
        assert_eq!(of("INTEGER", &[Places::new(30, 0)]), Some(Places::new(30, 0)));
        assert_eq!(of("SUM", &[n3]), None);
        assert_eq!((outer_dmax("MAX", 2), outer_dmax("REM", 2), outer_dmax("INTEGER", 2), outer_dmax("MEAN", 2)), (2, 2, 0, 0));
    }
}
