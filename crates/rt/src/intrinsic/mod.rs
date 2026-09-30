//! The intrinsic functions' semantics, apart from how an executor evaluates their arguments.

pub mod dates;
pub mod datetime;
pub mod math;
pub mod numval;
pub mod real;
pub mod text;

/// The functions beyond the first twenty-one that ironwork for COBOL runs.
pub const FUNCTIONS: &[&str] = &[
    "ACOS", "ANNUITY", "ASIN", "ATAN", "BIT-OF", "BIT-TO-CHAR", "BYTE-LENGTH", "COS", "DATE-TO-YYYYMMDD", "DAY-OF-INTEGER",
    "DAY-TO-YYYYDDD", "DISPLAY-OF", "E", "EXP", "EXP10", "FACTORIAL", "FORMATTED-CURRENT-DATE", "FORMATTED-DATE", "FORMATTED-DATETIME",
    "FORMATTED-TIME", "HEX-OF", "HEX-TO-CHAR", "INTEGER-OF-DAY", "INTEGER-OF-FORMATTED-DATE", "LOG", "LOG10",
    "MEAN", "MEDIAN", "MIDRANGE", "NUMVAL-F", "ORD-MAX", "ORD-MIN", "PI", "PRESENT-VALUE", "RANGE", "SECONDS-FROM-FORMATTED-TIME",
    "SECONDS-PAST-MIDNIGHT", "SIGN", "SIN", "SQRT", "STANDARD-DEVIATION", "SUM", "TAN", "TEST-DATE-YYYYMMDD", "TEST-DAY-YYYYDDD",
    "TEST-FORMATTED-DATETIME", "TEST-NUMVAL", "TEST-NUMVAL-C", "TEST-NUMVAL-F", "UUID4", "VARIANCE", "YEAR-TO-YYYY",
];

/// Functions whose result is long floating point under ARITH(COMPAT) and extended under
/// ARITH(EXTEND), so an expression holding one is evaluated in floating point (Programming Guide
/// SC27-8714-03, pp. 56-58, 62-63, 800-801; assumption C111).
pub const FLOATING_POINT: &[&str] = &[
    "ACOS", "ANNUITY", "ASIN", "ATAN", "COS", "E", "EXP", "EXP10", "LOG", "LOG10", "MEAN", "MEDIAN", "MIDRANGE", "NUMVAL-F", "PI",
    "PRESENT-VALUE", "RANDOM", "SECONDS-FROM-FORMATTED-TIME", "SECONDS-PAST-MIDNIGHT", "SIN", "SQRT", "STANDARD-DEVIATION", "TAN", "VARIANCE",
];

/// Functions of numeric arguments that are floating point when any argument is (p. 799; assumptions
/// C100 and C111).
pub const MIXED: &[&str] = &["ABS", "MAX", "MIN", "RANGE", "REM", "SUM"];
