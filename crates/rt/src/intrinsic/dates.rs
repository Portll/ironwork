//! Integer dates, Julian dates and the sliding century window (Language Reference SC27-8713-03,
//! pp. 503-504, 545-549, 577, 645-647, 681).

use crate::calendar::{civil, days_from_civil, days_in_month, is_leap, SECONDS_PER_DAY};

/// The day before integer date 1, 1 January 1601.
const DAY_ZERO: i64 = days_from_civil(1600, 12, 31);
pub const LAST_INTEGER_DATE: i64 = 3_067_671;

/// DAY-OF-INTEGER: the YYYYDDD of integer date `n`.
pub fn day_of_integer(n: i64) -> Option<i64> {
    if !(1..=LAST_INTEGER_DATE).contains(&n) {
        return None;
    }
    let days = DAY_ZERO + n;
    let year = civil(days * SECONDS_PER_DAY).year;
    Some(year * 1000 + days - days_from_civil(year, 1, 1) + 1)
}

/// INTEGER-OF-DAY: the integer date of a valid YYYYDDD.
pub fn integer_of_day(yyyyddd: i64) -> Option<i64> {
    (test_day(yyyyddd) == 0).then(|| days_from_civil(yyyyddd / 1000, 1, 1) + yyyyddd % 1000 - 1 - DAY_ZERO)
}

/// TEST-DATE-YYYYMMDD: 0 for a valid date, else 1 for the year, 2 the month, 3 the day.
pub fn test_date(n: i64) -> i64 {
    if !(16_010_000..=99_999_999).contains(&n) {
        return 1;
    }
    if !(100..=1299).contains(&(n % 10000)) {
        return 2;
    }
    let (year, month, day) = (n / 10000, n / 100 % 100, n % 100);
    if day < 1 || day > i64::from(days_in_month(year, month as u32)) {
        return 3;
    }
    0
}

/// TEST-DAY-YYYYDDD: 0 for a valid date, else 1 for the year, 2 the day.
pub fn test_day(n: i64) -> i64 {
    if !(1_601_000..=9_999_999).contains(&n) {
        return 1;
    }
    let days = if is_leap(n / 1000) { 366 } else { 365 };
    if !(1..=days).contains(&(n % 1000)) {
        return 2;
    }
    0
}

/// The year ending in `yy` within the 100 years that end at `current_year + window`, which must
/// be from 1700 to 9999.
fn windowed(yy: i64, window: i64, current_year: i64) -> Option<i64> {
    let end = current_year.checked_add(window)?;
    (1700..=9999).contains(&end).then(|| end - (end - yy).rem_euclid(100))
}

/// YEAR-TO-YYYY.
pub fn year_to_yyyy(yy: i64, window: i64, current_year: i64) -> Option<i64> {
    if !(0..100).contains(&yy) {
        return None;
    }
    windowed(yy, window, current_year)
}

/// DATE-TO-YYYYMMDD: the run time does not check that YYMMDD is a date.
pub fn date_to_yyyymmdd(yymmdd: i64, window: i64, current_year: i64) -> Option<i64> {
    if !(0..991_232).contains(&yymmdd) {
        return None;
    }
    Some(windowed(yymmdd / 10000, window, current_year)? * 10000 + yymmdd % 10000)
}

/// DAY-TO-YYYYDDD: the run time does not check that YYDDD is a date.
pub fn day_to_yyyyddd(yyddd: i64, window: i64, current_year: i64) -> Option<i64> {
    if !(0..99_367).contains(&yyddd) {
        return None;
    }
    Some(windowed(yyddd / 1000, window, current_year)? * 1000 + yyddd % 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_and_julian_dates_convert_both_ways() {
        assert_eq!(day_of_integer(1), Some(1_601_001));
        assert_eq!(day_of_integer(LAST_INTEGER_DATE), Some(9_999_365));
        assert_eq!(day_of_integer(143_951), Some(1_995_046));
        assert_eq!(integer_of_day(1_995_046), Some(143_951));
        assert_eq!(integer_of_day(2_000_366), Some(integer_of_day(2_001_001).unwrap() - 1));
        assert_eq!(day_of_integer(0), None);
        assert_eq!(integer_of_day(1_999_366), None);
    }

    #[test]
    fn the_tests_name_the_field_in_error_as_the_language_reference_shows() {
        assert_eq!(test_date(19_950_215), 0);
        assert_eq!(test_date(12_950_215), 1);
        assert_eq!(test_date(912_950_215), 1);
        assert_eq!(test_date(19_921_415), 2);
        assert_eq!(test_date(19_950_240), 3);
        assert_eq!(test_date(20_000_229), 0);
        assert_eq!(test_date(19_000_229), 3);
        assert_eq!(test_day(1_995_146), 0);
        assert_eq!(test_day(1_295_146), 1);
        assert_eq!(test_day(1_995_446), 2);
        assert_eq!(test_day(1_996_366), 0);
    }

    #[test]
    fn the_century_window_gives_the_language_references_examples() {
        assert_eq!(year_to_yyyy(4, 23, 1995), Some(2004));
        assert_eq!(year_to_yyyy(4, -15, 1995), Some(1904));
        assert_eq!(year_to_yyyy(98, 23, 2008), Some(1998));
        assert_eq!(year_to_yyyy(98, -15, 2008), Some(1898));
        assert_eq!(date_to_yyyymmdd(851_003, 120, 2002), Some(20_851_003));
        assert_eq!(date_to_yyyymmdd(851_003, -20, 2002), Some(18_851_003));
        assert_eq!(date_to_yyyymmdd(851_003, 10, 2002), Some(19_851_003));
        assert_eq!(date_to_yyyymmdd(981_002, -10, 1994), Some(18_981_002));
        assert_eq!(day_to_yyyyddd(10_004, -20, 2002), Some(1_910_004));
        assert_eq!(day_to_yyyyddd(10_004, -120, 2002), Some(1_810_004));
        assert_eq!(day_to_yyyyddd(10_004, 20, 2002), Some(2_010_004));
        assert_eq!(day_to_yyyyddd(95_005, -10, 2013), Some(1_995_005));
        assert_eq!(year_to_yyyy(100, 50, 2026), None);
        assert_eq!(year_to_yyyy(5, -400, 2026), None);
        assert_eq!(date_to_yyyymmdd(991_232, 50, 2026), None);
    }
}
