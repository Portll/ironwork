//! Integer dates, Julian dates and the sliding century window (Language Reference SC27-8713-03,
//! pp. 503-504, 545-549, 577, 645-647, 681).

use crate::calendar::{civil, days_from_civil, days_in_month, is_leap, LILIAN_ZERO, SECONDS_PER_DAY};
use numeric::IntDate;

/// Days since 1970-01-01 of the day before integer date 1: 31 December 1600 under INTDATE(ANSI),
/// 14 October 1582 under INTDATE(LILIAN) (Programming Guide SC27-8714-03, pp. 59, 375).
pub const fn day_zero(intdate: IntDate) -> i64 {
    match intdate {
        IntDate::Ansi => days_from_civil(1600, 12, 31),
        IntDate::Lilian => LILIAN_ZERO,
    }
}

/// The integer date of 31 December 9999: 3,067,671 under ANSI (Language Reference SC27-8713-03,
/// p. 509), 3,074,324 under LILIAN (assumption C214).
pub const fn last_integer_date(intdate: IntDate) -> i64 {
    days_from_civil(9999, 12, 31) - day_zero(intdate)
}

/// The integer date of `days` since 1970-01-01, if it is from 1 to the last.
fn integer_date(days: i64, intdate: IntDate) -> Option<i64> {
    let n = days - day_zero(intdate);
    (1..=last_integer_date(intdate)).contains(&n).then_some(n)
}

/// Days since 1970-01-01 of integer date `n`, if it is from 1 to the last.
fn days_of(n: i64, intdate: IntDate) -> Option<i64> {
    (1..=last_integer_date(intdate)).contains(&n).then(|| day_zero(intdate) + n)
}

/// INTEGER-OF-DATE: the integer date of a valid YYYYMMDD.
pub fn integer_of_date(yyyymmdd: i64, intdate: IntDate) -> Option<i64> {
    let (year, month, day) = (yyyymmdd / 10000, yyyymmdd / 100 % 100, yyyymmdd % 100);
    if year > 9999 || !(1..=12).contains(&month) || !(1..=i64::from(days_in_month(year, month as u32))).contains(&day) {
        return None;
    }
    integer_date(days_from_civil(year, month, day), intdate)
}

/// DATE-OF-INTEGER: the YYYYMMDD of integer date `n`.
pub fn date_of_integer(n: i64, intdate: IntDate) -> Option<i64> {
    let c = civil(days_of(n, intdate)? * SECONDS_PER_DAY);
    Some(c.year * 10000 + i64::from(c.month) * 100 + i64::from(c.day))
}

/// DAY-OF-INTEGER: the YYYYDDD of integer date `n`.
pub fn day_of_integer(n: i64, intdate: IntDate) -> Option<i64> {
    let days = days_of(n, intdate)?;
    let year = civil(days * SECONDS_PER_DAY).year;
    Some(year * 1000 + days - days_from_civil(year, 1, 1) + 1)
}

/// INTEGER-OF-DAY: the integer date of a valid YYYYDDD.
pub fn integer_of_day(yyyyddd: i64, intdate: IntDate) -> Option<i64> {
    let (year, day) = (yyyyddd / 1000, yyyyddd % 1000);
    if year > 9999 || !(1..=if is_leap(year) { 366 } else { 365 }).contains(&day) {
        return None;
    }
    integer_date(days_from_civil(year, 1, 1) + day - 1, intdate)
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
        let ansi = IntDate::Ansi;
        assert_eq!(day_of_integer(1, ansi), Some(1_601_001));
        assert_eq!(last_integer_date(ansi), 3_067_671);
        assert_eq!(day_of_integer(last_integer_date(ansi), ansi), Some(9_999_365));
        assert_eq!(day_of_integer(143_951, ansi), Some(1_995_046));
        assert_eq!(integer_of_day(1_995_046, ansi), Some(143_951));
        assert_eq!(integer_of_day(2_000_366, ansi), Some(integer_of_day(2_001_001, ansi).unwrap() - 1));
        assert_eq!(day_of_integer(0, ansi), None);
        assert_eq!(integer_of_day(1_999_366, ansi), None);
        assert_eq!(integer_of_day(1_600_366, ansi), None);
        assert_eq!(integer_of_date(16_010_101, ansi), Some(1));
        assert_eq!(date_of_integer(143_951, ansi), Some(19_950_215));
        assert_eq!(integer_of_date(16_001_231, ansi), None);
        assert_eq!(integer_of_date(19_950_229, ansi), None);
        assert_eq!(date_of_integer(3_067_672, ansi), None);
    }

    #[test]
    fn lilian_integer_dates_count_from_15_october_1582_as_language_environment_does() {
        let lilian = IntDate::Lilian;
        assert_eq!(date_of_integer(1, lilian), Some(15_821_015));
        assert_eq!(integer_of_date(15_821_015, lilian), Some(1));
        assert_eq!(integer_of_date(15_821_014, lilian), None);
        assert_eq!(integer_of_date(16_010_101, lilian), Some(6_654), "6,653 days before the ANSI day 1");
        assert_eq!(integer_of_date(19_950_215, lilian), Some(143_951 + 6_653));
        assert_eq!(last_integer_date(lilian), 3_074_324);
        assert_eq!(date_of_integer(3_074_324, lilian), Some(99_991_231));
        assert_eq!(date_of_integer(3_074_325, lilian), None);
        assert_eq!(day_of_integer(1, lilian), Some(1_582_288));
        assert_eq!(integer_of_day(1_582_288, lilian), Some(1));
        assert_eq!(integer_of_day(1_582_287, lilian), None);
        assert_eq!(date_of_integer(0, lilian), None);
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
