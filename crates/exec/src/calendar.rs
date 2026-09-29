//! The proleptic Gregorian calendar: day counts, civil fields, Lilian days and the epoch offsets.

pub const SECONDS_PER_DAY: i64 = 86_400;
pub const MILLIS_PER_DAY: i64 = 86_400_000;

/// Seconds from 1900-01-01T00:00:00 to the Unix epoch, the origin of CICS ABSTIME.
pub const EPOCH_1900_TO_1970_SECONDS: i64 = 2_208_988_800;
pub const EPOCH_1900_TO_1970_MILLIS: i64 = EPOCH_1900_TO_1970_SECONDS * 1000;

/// Day 0 of the Lilian calendar, 14 October 1582, in days since 1970-01-01.
pub const LILIAN_ZERO: i64 = days_from_civil(1582, 10, 14);
/// 31 December 9999.
pub const LAST_LILIAN: i64 = days_from_civil(9999, 12, 31) - LILIAN_ZERO;

/// Days since 1970-01-01 of a proleptic Gregorian date.
pub const fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let (y, m) = if month <= 2 { (year - 1, month + 9) } else { (year, month - 3) };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + (153 * m + 2) / 5 + day - 1;
    era * 146_097 + doe - 719_468
}

pub fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// The calendar fields of a UTC time. `weekday` counts 1 Monday to 7 Sunday.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Civil {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    /// 1-based.
    pub day_of_year: u32,
    pub weekday: u32,
}

impl Civil {
    /// CICS FORMATTIME's DAYOFWEEK: 0 Sunday to 6 Saturday.
    pub fn cics_weekday(&self) -> i64 {
        i64::from(self.weekday % 7)
    }
}

/// The calendar fields of `seconds` since 1970-01-01T00:00:00 UTC.
pub fn civil(seconds: i64) -> Civil {
    let days = seconds.div_euclid(SECONDS_PER_DAY);
    let secs = seconds.rem_euclid(SECONDS_PER_DAY) as u32;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy_march = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy_march + 2) / 153;
    let day = (doy_march - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    let before = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334][month as usize - 1] + u32::from(is_leap(year) && month > 2);
    Civil {
        year,
        month,
        day,
        hour: secs / 3600,
        minute: secs / 60 % 60,
        second: secs % 60,
        day_of_year: before + day,
        weekday: ((days + 3).rem_euclid(7) + 1) as u32,
    }
}

pub fn lilian(year: i64, month: u32, day: u32) -> i64 {
    days_from_civil(year, i64::from(month), i64::from(day)) - LILIAN_ZERO
}

/// LE's day of week: 1 Sunday to 7 Saturday; Lilian day 1 was a Friday.
pub fn weekday(lilian: i64) -> u32 {
    ((lilian + 4).rem_euclid(7) + 1) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        let epoch = civil(0);
        assert_eq!((epoch.year, epoch.month, epoch.day, epoch.day_of_year, epoch.weekday), (1970, 1, 1, 1, 4));
        let c = civil(1_790_510_400);
        assert_eq!((c.year, c.month, c.day, c.day_of_year, c.weekday), (2026, 9, 27, 270, 7));
        assert_eq!((c.hour, c.minute, c.second), (12, 0, 0));
        assert_eq!(civil(951_782_400).day_of_year, 60);
    }

    #[test]
    fn days_and_civil_round_trip() {
        for (y, m, d) in [(1582, 10, 15), (1900, 3, 1), (1970, 1, 1), (2000, 2, 29), (9999, 12, 31)] {
            let days = days_from_civil(y, i64::from(m), i64::from(d));
            let c = civil(days * SECONDS_PER_DAY);
            assert_eq!((c.year, c.month, c.day), (y, m, d));
        }
        assert_eq!(days_from_civil(1970, 1, 1), 0);
    }

    #[test]
    fn leap_years() {
        assert!(!is_leap(1900));
        assert!(is_leap(2000));
        assert!(is_leap(2024));
        assert_eq!((days_in_month(1900, 2), days_in_month(2024, 2), days_in_month(2023, 4)), (28, 29, 30));
    }

    #[test]
    fn lilian_day_one_is_15_october_1582() {
        assert_eq!(lilian(1582, 10, 15), 1);
        assert_eq!(LAST_LILIAN, lilian(9999, 12, 31));
    }

    #[test]
    fn weekday_conventions() {
        assert_eq!(civil(1_790_510_400).cics_weekday(), 0);
        assert_eq!(civil(0).cics_weekday(), 4);
        assert_eq!(weekday(1), 6);
    }

    #[test]
    fn epoch_offset_is_seventy_years() {
        assert_eq!(EPOCH_1900_TO_1970_SECONDS, -days_from_civil(1900, 1, 1) * SECONDS_PER_DAY);
    }
}
