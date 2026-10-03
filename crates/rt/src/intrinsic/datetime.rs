//! The date and time formats of FORMATTED-CURRENT-DATE, FORMATTED-DATE, FORMATTED-TIME,
//! FORMATTED-DATETIME, INTEGER-OF-FORMATTED-DATE, SECONDS-FROM-FORMATTED-TIME and
//! TEST-FORMATTED-DATETIME (Language Reference SC27-8713-03, pp. 504-507).

use super::dates::{date_of_integer, day_zero};
use crate::calendar::{civil, days_from_civil, days_in_month, is_leap, SECONDS_PER_DAY};
use numeric::IntDate;

pub const NANOS_PER_SECOND: u64 = 1_000_000_000;
pub const NANOS_PER_DAY: u64 = SECONDS_PER_DAY as u64 * NANOS_PER_SECOND;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Year,
    Month,
    Day,
    DayOfYear,
    Week,
    Weekday,
    Hour,
    Minute,
    Second,
    Fraction(u8),
    OffsetSign,
    OffsetHour,
    OffsetMinute,
    Literal(char),
}

/// One of IBM's date, time or combined formats, as its fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Format {
    fields: Vec<Field>,
}

const DATES: [(&str, bool); 6] = [("YYYYMMDD", false), ("YYYY-MM-DD", true), ("YYYYDDD", false), ("YYYY-DDD", true), ("YYYYWwwD", false), ("YYYY-Www-D", true)];

fn date_fields(text: &str) -> Vec<Field> {
    let mut fields = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let (field, len) = if rest.starts_with("YYYY") {
            (Field::Year, 4)
        } else if rest.starts_with("MM") {
            (Field::Month, 2)
        } else if rest.starts_with("DDD") {
            (Field::DayOfYear, 3)
        } else if rest.starts_with("DD") {
            (Field::Day, 2)
        } else if rest.starts_with("ww") {
            (Field::Week, 2)
        } else if rest.starts_with('D') {
            (Field::Weekday, 1)
        } else {
            (Field::Literal(rest.chars().next().unwrap()), 1)
        };
        fields.push(field);
        rest = &rest[len..];
    }
    fields
}

/// A time format's fields, or `None`; `extended` is whether it takes colons.
fn time_fields(text: &str, extended: bool) -> Option<Vec<Field>> {
    let (mut fields, mut rest) = if extended {
        (vec![Field::Hour, Field::Literal(':'), Field::Minute, Field::Literal(':'), Field::Second], text.strip_prefix("hh:mm:ss")?)
    } else {
        (vec![Field::Hour, Field::Minute, Field::Second], text.strip_prefix("hhmmss")?)
    };
    if let Some(separator) = rest.chars().next().filter(|c| matches!(c, '.' | ',')) {
        let digits = rest[1..].chars().take_while(|&c| c == 's').count();
        if !(1..=9).contains(&digits) {
            return None;
        }
        fields.push(Field::Literal(separator));
        fields.push(Field::Fraction(digits as u8));
        rest = &rest[1 + digits..];
    }
    match rest {
        "" => {}
        "Z" => fields.push(Field::Literal('Z')),
        "+hhmm" if !extended => fields.extend([Field::OffsetSign, Field::OffsetHour, Field::OffsetMinute]),
        "+hh:mm" if extended => fields.extend([Field::OffsetSign, Field::OffsetHour, Field::Literal(':'), Field::OffsetMinute]),
        _ => return None,
    }
    Some(fields)
}

/// ISO weekday, Monday 1 to Sunday 7, of days since 1970-01-01.
fn weekday(days: i64) -> i64 {
    (days + 3).rem_euclid(7) + 1
}

fn weeks_in(year: i64) -> i64 {
    let p = |y: i64| (y + y.div_euclid(4) - y.div_euclid(100) + y.div_euclid(400)).rem_euclid(7);
    if p(year) == 4 || p(year - 1) == 3 { 53 } else { 52 }
}

/// The ISO week-numbering year, week and weekday of days since 1970-01-01.
fn iso_week(days: i64) -> (i64, i64, i64) {
    let year = civil(days * SECONDS_PER_DAY).year;
    let ordinal = days - days_from_civil(year, 1, 1) + 1;
    let wd = weekday(days);
    let week = (ordinal - wd + 10).div_euclid(7);
    if week < 1 {
        (year - 1, weeks_in(year - 1), wd)
    } else if week > weeks_in(year) {
        (year + 1, 1, wd)
    } else {
        (year, week, wd)
    }
}

/// Days since 1970-01-01 of an ISO week date.
fn from_iso_week(year: i64, week: i64, wd: i64) -> i64 {
    let jan4 = days_from_civil(year, 1, 4);
    jan4 - (weekday(jan4) - 1) + (week - 1) * 7 + wd - 1
}

/// What TEST-FORMATTED-DATETIME, INTEGER-OF-FORMATTED-DATE and SECONDS-FROM-FORMATTED-TIME read
/// from a value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reading {
    pub integer_date: Option<i64>,
    /// Seconds past midnight as whole seconds, and the fraction's digits and their count.
    pub seconds: Option<(u32, u64, u8)>,
}

impl Format {
    /// IBM's format `text` names, or `None`.
    pub fn parse(text: &str) -> Option<Self> {
        let (date, time) = match text.split_once('T') {
            Some((d, t)) => (Some(d), Some(t)),
            None if text.starts_with('h') => (None, Some(text)),
            None => (Some(text), None),
        };
        let mut fields = Vec::new();
        let mut extended = None;
        if let Some(d) = date {
            let &(_, e) = DATES.iter().find(|(f, _)| *f == d)?;
            extended = Some(e);
            fields.extend(date_fields(d));
            if time.is_some() {
                fields.push(Field::Literal('T'));
            }
        }
        if let Some(t) = time {
            let e = extended.unwrap_or(t.contains(':'));
            fields.extend(time_fields(t, e)?);
        }
        Some(Self { fields })
    }

    pub fn has_date(&self) -> bool {
        self.fields.contains(&Field::Year)
    }

    pub fn has_time(&self) -> bool {
        self.fields.contains(&Field::Hour)
    }

    /// Whether the time is written in UTC, with a Z.
    pub fn is_utc(&self) -> bool {
        self.fields.last() == Some(&Field::Literal('Z'))
    }

    pub fn len(&self) -> usize {
        self.fields.iter().map(|f| match f {
            Field::Year => 4,
            Field::DayOfYear => 3,
            Field::Month | Field::Day | Field::Week | Field::Hour | Field::Minute | Field::Second | Field::OffsetHour | Field::OffsetMinute => 2,
            Field::Weekday | Field::OffsetSign | Field::Literal(_) => 1,
            Field::Fraction(n) => *n as usize,
        }).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// The value: `integer_date`, as `intdate` numbers it, for a date part, `nanos` past midnight
    /// for a time part, and the offset from UTC in minutes for an offset part. The caller has
    /// adjusted a UTC format's date and time by the offset.
    pub fn render(&self, integer_date: i64, nanos: u64, offset_minutes: i32, intdate: IntDate) -> String {
        let days = day_zero(intdate) + integer_date;
        let c = civil(days * SECONDS_PER_DAY);
        let (year, month, day) = (c.year, c.month, c.day);
        let (week_year, week, wd) = iso_week(days);
        let seconds = nanos / NANOS_PER_SECOND;
        let mut out = String::new();
        for f in &self.fields {
            match *f {
                Field::Year if self.fields.contains(&Field::Week) => out.push_str(&format!("{week_year:04}")),
                Field::Year => out.push_str(&format!("{year:04}")),
                Field::Month => out.push_str(&format!("{month:02}")),
                Field::Day => out.push_str(&format!("{day:02}")),
                Field::DayOfYear => out.push_str(&format!("{:03}", days - days_from_civil(year, 1, 1) + 1)),
                Field::Week => out.push_str(&format!("{week:02}")),
                Field::Weekday => out.push_str(&wd.to_string()),
                Field::Hour => out.push_str(&format!("{:02}", seconds / 3600)),
                Field::Minute => out.push_str(&format!("{:02}", seconds / 60 % 60)),
                Field::Second => out.push_str(&format!("{:02}", seconds % 60)),
                Field::Fraction(n) => out.push_str(&format!("{:09}", nanos % NANOS_PER_SECOND)[..n as usize]),
                Field::OffsetSign => out.push(if offset_minutes < 0 { '-' } else { '+' }),
                Field::OffsetHour => out.push_str(&format!("{:02}", offset_minutes.unsigned_abs() / 60)),
                Field::OffsetMinute => out.push_str(&format!("{:02}", offset_minutes.unsigned_abs() % 60)),
                Field::Literal(c) => out.push(c),
            }
        }
        out
    }

    /// Reads `value` against the format, or gives the 1-based position of the first character at
    /// which it is known to be in error (p. 649): a field's digits are checked as they come, so a
    /// year that starts 15 is in error at its second digit. A date reads as an integer date as
    /// `intdate` numbers them, whose first year it may not precede; one before integer date 1 is
    /// in error at its last character.
    pub fn read(&self, value: &str, intdate: IntDate) -> Result<Reading, usize> {
        let chars: Vec<char> = value.chars().collect();
        let first_year = date_of_integer(1, intdate).unwrap_or_default() / 10000;
        let mut at = 0usize;
        let (mut year, mut month, mut day, mut ordinal, mut week, mut wd) = (None, None, None, None, None, None);
        let (mut hour, mut minute, mut second, mut fraction) = (0u32, 0u32, 0u32, (0u64, 0u8));
        let mut zero_offset = false;
        let mut date_end = 0usize;
        for f in &self.fields {
            let mut digits = |n: usize, lo: i64, hi: i64| -> Result<i64, usize> {
                let mut v = 0i64;
                for k in 0..n {
                    let c = chars.get(at).copied().ok_or(at + 1)?;
                    let d = c.to_digit(10).ok_or(at + 1)? as i64;
                    v = v * 10 + d;
                    let rest = 10i64.pow((n - k - 1) as u32);
                    if v * rest + rest - 1 < lo || v * rest > hi {
                        return Err(at + 1);
                    }
                    at += 1;
                }
                Ok(v)
            };
            match *f {
                Field::Year => year = Some(digits(4, first_year, 9999)?),
                Field::Month => month = Some(digits(2, 1, 12)?),
                Field::Day => {
                    let most = days_in_month(year.unwrap_or(2000), month.unwrap_or(1) as u32);
                    day = Some(digits(2, 1, most.into())?);
                }
                Field::DayOfYear => ordinal = Some(digits(3, 1, if is_leap(year.unwrap_or(2000)) { 366 } else { 365 })?),
                Field::Week => week = Some(digits(2, 1, weeks_in(year.unwrap_or(2000)))?),
                Field::Weekday => wd = Some(digits(1, 1, 7)?),
                Field::Hour => hour = digits(2, 0, 23)? as u32,
                Field::Minute => minute = digits(2, 0, 59)? as u32,
                Field::Second => second = digits(2, 0, 59)? as u32,
                Field::Fraction(n) => fraction = (digits(n as usize, 0, 999_999_999)? as u64, n),
                Field::OffsetSign => {
                    match chars.get(at) {
                        Some('+' | '-') => {}
                        Some('0') => zero_offset = true,
                        _ => return Err(at + 1),
                    }
                    at += 1;
                }
                Field::OffsetHour => {
                    digits(2, 0, if zero_offset { 0 } else { 23 })?;
                }
                Field::OffsetMinute => {
                    digits(2, 0, if zero_offset { 0 } else { 59 })?;
                }
                Field::Literal(c) => {
                    if chars.get(at) != Some(&c) {
                        return Err(at + 1);
                    }
                    at += 1;
                }
            }
            if matches!(f, Field::Year | Field::Month | Field::Day | Field::DayOfYear | Field::Week | Field::Weekday) {
                date_end = at;
            }
        }
        let days = match (year, month, day, ordinal, week, wd) {
            (Some(y), Some(m), Some(d), ..) => Some(days_from_civil(y, m, d)),
            (Some(y), _, _, Some(o), ..) => Some(days_from_civil(y, 1, 1) + o - 1),
            (Some(y), _, _, _, Some(w), Some(d)) => Some(from_iso_week(y, w, d)),
            _ => None,
        };
        if days.is_some_and(|d| d <= day_zero(intdate)) {
            return Err(date_end);
        }
        if at < chars.len() {
            return Err(at + 1);
        }
        Ok(Reading {
            integer_date: days.map(|d| d - day_zero(intdate)),
            seconds: self.has_time().then_some((hour * 3600 + minute * 60 + second, fraction.0, fraction.1)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEB_15_1995: i64 = 143_951;
    const NANOS: u64 = 18_867_812_479_168;
    const ANSI: IntDate = IntDate::Ansi;
    const LILIAN: IntDate = IntDate::Lilian;

    #[test]
    fn under_lilian_dates_read_and_render_as_lilian_days_from_15_october_1582() {
        let f = |s: &str| Format::parse(s).unwrap();
        assert_eq!(f("YYYYMMDD").render(1, 0, 0, LILIAN), "15821015");
        assert_eq!(f("YYYY-DDD").render(FEB_15_1995 + 6_653, 0, 0, LILIAN), "1995-046");
        assert_eq!(f("YYYYMMDD").read("19950215", LILIAN).unwrap().integer_date, Some(FEB_15_1995 + 6_653));
        assert_eq!(f("YYYYMMDD").read("15821015", LILIAN).unwrap().integer_date, Some(1));
        assert_eq!(f("YYYYMMDD").read("15821014", LILIAN), Err(8), "the day before integer date 1");
        assert_eq!(f("YYYY-DDD").read("1582-001", LILIAN), Err(8));
        assert_eq!(f("YYYYMMDD").read("15811231", LILIAN), Err(4));
        assert_eq!(f("YYYYMMDD").read("15821015", ANSI), Err(2));
    }

    #[test]
    fn only_ibms_formats_parse() {
        for ok in ["YYYYMMDD", "YYYY-MM-DD", "YYYYDDD", "YYYY-DDD", "YYYYWwwD", "YYYY-Www-D", "hhmmss", "hh:mm:ss.sssssssss", "hhmmssZ", "hhmmss.ss+hhmm", "YYYYMMDDThhmmss.ss+hhmm", "YYYY-MM-DDThh:mm:ss.ss+hh:mm", "hh:mm:ss,sssZ"] {
            assert!(Format::parse(ok).is_some(), "{ok}");
        }
        for bad in ["YYMMDD", "YYYYMMDDThh:mm:ss", "YYYY-MM-DDThhmmss", "hhmm", "hhmmss.", "hhmmss.ssssssssss", "hhmmss+hh:mm", "hh:mm:ss+hhmm", "YYYYMMDDT"] {
            assert!(Format::parse(bad).is_none(), "{bad}");
        }
        assert_eq!(Format::parse("YYYYMMDDThhmmss.ss+hhmm").unwrap().len(), 23);
    }

    #[test]
    fn values_render_as_the_language_references_examples_show() {
        let f = |s: &str| Format::parse(s).unwrap();
        assert_eq!(f("YYYYMMDD").render(FEB_15_1995, 0, 0, ANSI), "19950215");
        assert_eq!(f("YYYY-DDD").render(FEB_15_1995, 0, 0, ANSI), "1995-046");
        assert_eq!(f("YYYY-Www-D").render(FEB_15_1995, 0, 0, ANSI), "1995-W07-3");
        assert_eq!(f("hhmmss.ss+hhmm").render(0, NANOS, -300, ANSI), "051427.81-0500");
        assert_eq!(f("YYYY-MM-DDThh:mm:ss.ss+hh:mm").render(FEB_15_1995, NANOS, 300, ANSI), "1995-02-15T05:14:27.81+05:00");
        assert_eq!(f("hh:mm:ssZ").render(0, 36_000 * NANOS_PER_SECOND, 0, ANSI), "10:00:00Z");
        let dec_31_2020 = days_from_civil(2020, 12, 31) - day_zero(ANSI);
        assert_eq!(f("YYYYWwwD").render(dec_31_2020, 0, 0, ANSI), "2020W534");
        let jan_1_2021 = dec_31_2020 + 1;
        assert_eq!(f("YYYYWwwD").render(jan_1_2021, 0, 0, ANSI), "2020W535");
    }

    #[test]
    fn a_value_is_in_error_where_it_first_cannot_conform() {
        let f = |s: &str| Format::parse(s).unwrap();
        assert_eq!(f("YYYYMMDD").read("19950215", ANSI).unwrap().integer_date, Some(FEB_15_1995));
        assert_eq!(f("YYYYMMDD").read("20051314", ANSI), Err(6));
        assert_eq!(f("YYYYMMDD").read("15990316", ANSI), Err(2));
        assert_eq!(f("YYYYMMDD").read("19959215", ANSI), Err(5));
        assert_eq!(f("YYYYMMDD").read("19950229", ANSI), Err(8));
        assert_eq!(f("YYYYMMDDThhmmss").read("19950215T0514:27", ANSI), Err(14));
        assert_eq!(f("YYYYMMDD").read("1995021", ANSI), Err(8));
        assert_eq!(f("YYYYMMDD").read("199502150", ANSI), Err(9));
        assert_eq!(f("YYYY-Www-D").read("1995-W07-3", ANSI).unwrap().integer_date, Some(FEB_15_1995));
        assert_eq!(f("YYYY-DDD").read("1995-046", ANSI).unwrap().integer_date, Some(FEB_15_1995));
        let r = f("YYYYMMDDThhmmss.ss+hhmm").read("19950215T051427.81+0500", ANSI).unwrap();
        assert_eq!((r.integer_date, r.seconds), (Some(FEB_15_1995), Some((18_867, 81, 2))));
        assert_eq!(f("hhmmss+hhmm").read("0514270", ANSI).map(|_| ()), Err(8));
        assert_eq!(f("hhmmss+hhmm").read("05142700001", ANSI), Err(11));
        assert_eq!(f("hhmmss+hhmm").read("05142700000", ANSI).unwrap().seconds, Some((18_867, 0, 0)));
    }
}
