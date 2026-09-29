//! Language Environment callable services (SA38-0683-60): which ones ironwork provides, their
//! condition tokens, Lilian dates and seconds, and the picture strings of the date and time
//! services. The CALL side, which reads and writes the arguments, is machine/le_services.rs. What
//! the manual leaves open is `numeric::assumptions` L1 to L14.

use numeric::precision::{Fixed, Places};
use zarch::check::ProgramMask;
use zarch::ebcdic::{self, CodePage};
use zarch::hfp::{Hfp, Precision, Rounding};
use zarch::wide::U256;

/// The services ironwork provides, each with its parameters in order; `fc` is always last.
pub const PROVIDED: &[(&str, &[&str])] = &[
    ("CEE3ABD", &["abcode", "clean-up"]),
    ("CEE3DMP", &["title", "options", "fc"]),
    ("CEEDATE", &["input_Lilian_date", "picture_string", "output_char_date", "fc"]),
    ("CEEDATM", &["input_seconds", "picture_string", "output_timestamp", "fc"]),
    ("CEEDAYS", &["input_char_date", "picture_string", "output_Lilian_date", "fc"]),
    ("CEEDYWK", &["input_Lilian_date", "output_day_no", "fc"]),
    ("CEEFRST", &["address", "fc"]),
    ("CEEGMT", &["output_GMT_Lilian", "output_GMT_seconds", "fc"]),
    ("CEEGMTO", &["offset_hours", "offset_minutes", "offset_seconds", "fc"]),
    ("CEEGTST", &["heap_id", "size", "address", "fc"]),
    ("CEELOCT", &["output_Lilian", "output_seconds", "output_Gregorian", "fc"]),
    ("CEEMOUT", &["message_string", "destination_code", "fc"]),
    ("CEESECS", &["input_timestamp", "picture_string", "output_seconds", "fc"]),
    ("CEEUTC", &["output_GMT_Lilian", "output_GMT_seconds", "fc"]),
];

/// Every callable service SA38-0683-60 documents, other than the math services.
const DOCUMENTED: &[&str] = &[
    "CEE3AB2", "CEE3ABD", "CEE3CIB", "CEE3CTY", "CEE3DLY", "CEE3DMP", "CEE3GRC", "CEE3GRN", "CEE3GRO", "CEE3INF", "CEE3LNG", "CEE3MC2", "CEE3MCS",
    "CEE3MDS", "CEE3MTS", "CEE3PR2", "CEE3PRM", "CEE3RPH", "CEE3SPM", "CEE3SRC", "CEE3SRP", "CEE3USR", "CEECBLDY", "CEECMI", "CEECRHP", "CEECZST",
    "CEEDATE", "CEEDATM", "CEEDAYS", "CEEDCOD", "CEEDLYM", "CEEDSHP", "CEEDYWK", "CEEENV", "CEEFMDA", "CEEFMDT", "CEEFMON", "CEEFMTM", "CEEFRST",
    "CEEFTDS", "CEEGMT", "CEEGMTO", "CEEGPID", "CEEGQDT", "CEEGTJS", "CEEGTST", "CEEHDLR", "CEEHDLU", "CEEISEC", "CEEITOK", "CEELCNV", "CEELOCT",
    "CEEMGET", "CEEMICT", "CEEMOUT", "CEEMRCE", "CEEMRCR", "CEEMSG", "CEENCOD", "CEEQCEN", "CEEQDTC", "CEEQRYL", "CEERAN0", "CEERCDM", "CEESCEN",
    "CEESCOL", "CEESECI", "CEESECS", "CEESETL", "CEESGL", "CEESICLR", "CEESISET", "CEESISHF", "CEESITST", "CEESTXF", "CEETDLI", "CEETEST", "CEEUSGD",
    "CEEUTC",
];

/// The math services are CEESxnnn, x naming the operand type.
const MATH: &[&str] = &[
    "ABS", "ACS", "ASN", "ATH", "ATN", "AT2", "CJG", "COS", "CSH", "CTN", "DIM", "DVD", "ERC", "ERF", "EXP", "GMA", "IMG", "INT", "LGM", "LG1", "LG2",
    "LOG", "MLT", "MOD", "NIN", "NWN", "SGN", "SIN", "SNH", "SQT", "TAN", "TNH",
];

pub fn parameters(name: &str) -> Option<&'static [&'static str]> {
    PROVIDED.iter().find(|(n, _)| *n == name).map(|(_, p)| *p)
}

pub fn provides(name: &str) -> bool {
    parameters(name).is_some()
}

fn documented(name: &str) -> bool {
    let math = name.len() == 8 && name.starts_with("CEES") && (MATH.contains(&&name[5..]) || name[5..].starts_with("XP"));
    DOCUMENTED.contains(&name) || math
}

/// The S806 message for a CALL that finds nothing.
pub fn missing(name: &str) -> String {
    if documented(name) {
        format!("CALL {name}: {name} is a Language Environment callable service that ironwork for COBOL does not provide yet")
    } else {
        format!("CALL {name}: no such program in the run unit or its program libraries")
    }
}

/// A condition of facility CEE: its severity and message number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Condition {
    pub severity: u8,
    pub number: u16,
}

const fn condition(severity: u8, number: u16) -> Condition {
    Condition { severity, number }
}

pub const DESTINATION: Condition = condition(3, 451);
pub const HEAP_ID: Condition = condition(3, 803);
pub const HEAP_SIZE: Condition = condition(3, 808);
pub const FREE_ADDRESS: Condition = condition(3, 810);
pub const HEAP_SHORT: Condition = condition(3, 813);
pub const SECONDS_RANGE: Condition = condition(3, 2505);
pub const INSUFFICIENT: Condition = condition(3, 2507);
pub const DATE_VALUE: Condition = condition(3, 2508);
pub const HOURS: Condition = condition(3, 2510);
pub const LILIAN_RANGE: Condition = condition(3, 2512);
pub const DATE_RANGE: Condition = condition(3, 2513);
pub const MINUTES: Condition = condition(3, 2516);
pub const MONTH: Condition = condition(3, 2517);
pub const PICTURE: Condition = condition(3, 2518);
pub const SECONDS_VALUE: Condition = condition(3, 2519);
pub const DAYS_NONNUMERIC: Condition = condition(3, 2520);
pub const SECS_NONNUMERIC: Condition = condition(3, 2525);
pub const DATE_TRUNCATED: Condition = condition(2, 2526);
pub const TIMESTAMP_TRUNCATED: Condition = condition(2, 2527);
pub const DUMP_OPTIONS: Condition = condition(2, 3102);

impl Condition {
    /// The symbolic feedback code: CEE and the message number in base 32.
    pub fn symbol(self) -> String {
        let digit = |d: u16| char::from_digit(u32::from(d), 32).unwrap_or('0').to_ascii_uppercase();
        let n = self.number;
        format!("CEE{}{}{}", digit(n / 1024 % 32), digit(n / 32 % 32), digit(n % 32))
    }

    /// The 12-byte condition token: case 1, facility CEE, no instance-specific information.
    pub fn token(self) -> [u8; 12] {
        let mut t = [0u8; 12];
        t[0..2].copy_from_slice(&u16::from(self.severity).to_be_bytes());
        t[2..4].copy_from_slice(&self.number.to_be_bytes());
        t[4] = 0x40 | (self.severity & 7) << 3 | 0x01;
        t[5..8].copy_from_slice(&[0xC3, 0xC5, 0xC5]);
        t
    }

    pub fn text(self) -> &'static str {
        match self.number {
            451 => "CEEMOUT was given a destination code it does not accept.",
            803 => "No heap has the identifier given to a get-storage or discard-heap request.",
            808 => "A get-storage (CEEGTST) or reallocate (CEECZST) request asked for a size that is not positive.",
            810 => "CEEFRST was given an address that is not heap storage, or the heap's control information is damaged.",
            813 => "There was not enough storage to satisfy a get-storage request.",
            2505 => "The number of seconds is outside the range the service supports.",
            2507 => "CEEDAYS or CEESECS ran out of input before the date was complete, so no Lilian value was produced.",
            2508 => "CEEDAYS or CEESECS was given a date that does not exist.",
            2510 => "CEEISEC or CEESECS was given an hour it cannot accept.",
            2512 => "CEEDATE or CEEDYWK was given a Lilian day outside the supported range.",
            2513 => "CEEISEC, CEEDAYS or CEESECS was given a date outside the supported range.",
            2516 => "CEEISEC was given a minute it cannot accept.",
            2517 => "CEEISEC was given a month it cannot accept.",
            2518 => "A date or time service was given a picture string it cannot use.",
            2519 => "CEEISEC was given a second it cannot accept.",
            2520 => "CEEDAYS found a non-digit where its picture wants a number, or the date does not fit the picture.",
            2525 => "CEESECS found a non-digit where its picture wants a number, or the timestamp does not fit the picture.",
            2526 => "The date CEEDATE produced did not fit its output field and was cut short.",
            2527 => "The timestamp CEEDATM produced did not fit its output field and was cut short.",
            3102 => "CEE3DMP ignored options or suboptions it does not recognise.",
            _ => "",
        }
    }
}

const fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let (y, m) = if month <= 2 { (year - 1, month + 9) } else { (year, month - 3) };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + (153 * m + 2) / 5 + day - 1;
    era * 146_097 + doe - 719_468
}

/// Day 0 of the Lilian calendar, 14 October 1582, in days since 1970-01-01.
const LILIAN_ZERO: i64 = days_from_civil(1582, 10, 14);
/// 31 December 9999.
pub const LAST_LILIAN: i64 = days_from_civil(9999, 12, 31) - LILIAN_ZERO;
const DAY_MS: i64 = 86_400_000;

fn leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        2 if leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

pub fn lilian(year: i64, month: u32, day: u32) -> i64 {
    days_from_civil(year, i64::from(month), i64::from(day)) - LILIAN_ZERO
}

/// 1 Sunday to 7 Saturday; day 1 was a Friday.
pub fn weekday(lilian: i64) -> u32 {
    ((lilian + 4).rem_euclid(7) + 1) as u32
}

/// An instant as a Lilian day and the milliseconds into it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    pub lilian: i64,
    pub millis: i64,
}

impl Stamp {
    /// The clock's reading: Unix seconds and hundredths.
    pub fn from_unix(seconds: i64, hundredths: u32) -> Self {
        Self { lilian: seconds.div_euclid(86_400) - LILIAN_ZERO, millis: seconds.rem_euclid(86_400) * 1000 + i64::from(hundredths.min(99)) * 10 }
    }

    pub fn from_millis(ms: i64) -> Self {
        Self { lilian: ms.div_euclid(DAY_MS), millis: ms.rem_euclid(DAY_MS) }
    }

    pub fn total_millis(self) -> i64 {
        self.lilian.saturating_mul(DAY_MS).saturating_add(self.millis)
    }

    pub fn fields(self) -> Fields {
        let (year, month, day, _, _, _, yday, _) = crate::unit::civil(self.lilian.saturating_add(LILIAN_ZERO).saturating_mul(86_400));
        let s = self.millis / 1000;
        Fields {
            year,
            month,
            day,
            yday,
            weekday: weekday(self.lilian),
            hour: (s / 3600) as u32,
            minute: (s / 60 % 60) as u32,
            second: (s % 60) as u32,
            millis: (self.millis % 1000) as u32,
        }
    }

    /// YYYYMMDDHHMISS999, as CEELOCT gives it.
    pub fn gregorian(self) -> String {
        let f = self.fields();
        format!("{:04}{:02}{:02}{:02}{:02}{:02}{:03}", f.year, f.month, f.day, f.hour, f.minute, f.second, f.millis)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fields {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub yday: u32,
    pub weekday: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub millis: u32,
}

/// Lilian seconds as a COMP-2: the millisecond count, converted and divided by 1000 in long HFP.
pub fn seconds_hfp(total_millis: i64) -> [u8; 8] {
    let value = Fixed { negative: total_millis < 0, magnitude: U256::from_u128(u128::from(total_millis.unsigned_abs())), places: Places::new(16, 3) };
    let bytes = numeric::float::from_fixed(value, Precision::Long, ProgramMask::default()).map_or(vec![0; 8], Hfp::to_bytes);
    bytes.try_into().unwrap_or([0; 8])
}

/// A COMP-2 of Lilian seconds, to the nearest millisecond; None when it is not a number of
/// milliseconds a fullword-pair can hold.
pub fn hfp_millis(bytes: [u8; 8]) -> Option<i64> {
    let (negative, ms) = Hfp::from_bytes(Precision::Long, &bytes).to_scaled_integer(3, Rounding::HalfAwayFromZero)?;
    let ms = i64::try_from(ms.to_u128()?).ok()?;
    Some(if negative { -ms } else { ms })
}

const MONTHS: [&str; 12] = ["JANUARY", "FEBRUARY", "MARCH", "APRIL", "MAY", "JUNE", "JULY", "AUGUST", "SEPTEMBER", "OCTOBER", "NOVEMBER", "DECEMBER"];
const DAYS: [&str; 7] = ["SUNDAY", "MONDAY", "TUESDAY", "WEDNESDAY", "THURSDAY", "FRIDAY", "SATURDAY"];
const ROMAN: [&str; 12] = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI", "XII"];

/// One term of a date and time picture string (SA38-0683-60, Table 34).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Term {
    /// Y, YY or YYYY.
    Year(u8),
    /// MM, or ZM with its leading zero suppressed.
    Month(bool),
    /// MMM, Mmm and longer: each letter's case is the output's; Z suppresses trailing blanks.
    MonthName { upper: Vec<bool>, trim: bool },
    /// RRRR, or RRRZ.
    Roman(bool),
    Day(bool),
    DayOfYear,
    Hour(bool),
    Minute,
    Second,
    /// 9, 99 or 999.
    Fraction(u8),
    Meridiem { lower: bool, dots: bool },
    Weekday,
    WeekdayName { upper: Vec<bool>, trim: bool },
    /// <JJJJ>, <CCCC>, <CCCCCCCC>, YYY and ZYY, which ironwork does not provide.
    Era,
    /// A delimiter, kept as its byte.
    Literal(u8),
}

impl Term {
    fn is_time(&self) -> bool {
        matches!(self, Term::Hour(_) | Term::Minute | Term::Second | Term::Fraction(_) | Term::Meridiem { .. })
    }
}

/// The US defaults for a null or blank picture string (SA38-0683-60, Table 33).
const DEFAULT_DATE: &str = "MM/DD/YY";
const DEFAULT_TIMESTAMP: &str = "MM/DD/YY ZH:MI:SS AP";

fn blank(picture: &[u8]) -> bool {
    picture.iter().all(|&b| b == ebcdic::SPACE)
}

fn picture_or_default(picture: &[u8], default: &str, page: &CodePage) -> Vec<u8> {
    if blank(picture) { page.encode(default).unwrap_or_default() } else { picture.to_vec() }
}

pub fn terms(picture: &[u8], page: &CodePage) -> Vec<Term> {
    let chars: Vec<char> = picture.iter().map(|&b| page.decode_byte(b)).collect();
    let at = |i: usize| chars.get(i).copied().unwrap_or('\0');
    let run = |i: usize, f: fn(char) -> bool| chars[i..].iter().take_while(|&&c| f(c)).count();
    let starts = |i: usize, s: &str| chars[i..].iter().copied().take(s.chars().count()).eq(s.chars());
    // A Z or z after a name suppresses its blanks, unless it starts ZM, ZD, ZH or ZYY.
    let trim = |i: usize| matches!(at(i), 'Z' | 'z') && !(at(i) == 'Z' && (matches!(at(i + 1), 'M' | 'D' | 'H') || starts(i + 1, "YY")));
    let mut out: Vec<Term> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let after_hour = matches!(out.iter().rev().find(|t| !matches!(t, Term::Literal(_))), Some(Term::Hour(_)));
        let (term, used) = match chars[i] {
            'Y' => match run(i, |c| c == 'Y').min(4) {
                3 => (Term::Era, 3),
                n => (Term::Year(n as u8), n),
            },
            'Z' => match at(i + 1) {
                'M' => (Term::Month(true), 2),
                'D' => (Term::Day(true), 2),
                'H' => (Term::Hour(true), 2),
                'Y' if at(i + 2) == 'Y' => (Term::Era, 3),
                _ => (Term::Literal(picture[i]), 1),
            },
            'M' if at(i + 1) == 'I' => (Term::Minute, 2),
            'M' => match run(i, |c| c == 'M' || c == 'm') {
                1 => (Term::Literal(picture[i]), 1),
                2 if at(i + 1) == 'M' => (if after_hour { Term::Minute } else { Term::Month(false) }, 2),
                2 => (Term::Literal(picture[i]), 1),
                n => {
                    let width = n.min(20);
                    let upper = chars[i..i + width].iter().map(|c| c.is_ascii_uppercase()).collect();
                    let trim = width == n && trim(i + width);
                    (Term::MonthName { upper, trim }, width + usize::from(trim))
                }
            },
            'W' => match run(i, |c| c == 'W' || c == 'w') {
                1 | 2 => (Term::Weekday, 1),
                n => {
                    let width = n.min(20);
                    let upper = chars[i..i + width].iter().map(|c| c.is_ascii_uppercase()).collect();
                    let trim = width == n && trim(i + width);
                    (Term::WeekdayName { upper, trim }, width + usize::from(trim))
                }
            },
            'R' => match run(i, |c| c == 'R') {
                n if n >= 4 => (Term::Roman(false), 4),
                3 if at(i + 3) == 'Z' => (Term::Roman(true), 4),
                _ => (Term::Literal(picture[i]), 1),
            },
            'D' => match run(i, |c| c == 'D') {
                1 => (Term::Literal(picture[i]), 1),
                2 => (Term::Day(false), 2),
                _ => (Term::DayOfYear, 3),
            },
            'H' if at(i + 1) == 'H' => (Term::Hour(false), 2),
            'S' if at(i + 1) == 'S' => (Term::Second, 2),
            '9' => {
                let n = run(i, |c| c == '9').min(3);
                (Term::Fraction(n as u8), n)
            }
            'A' | 'a' if starts(i + 1, if chars[i] == 'A' { "P" } else { "p" }) => (Term::Meridiem { lower: chars[i] == 'a', dots: false }, 2),
            'A' | 'a' if starts(i, if chars[i] == 'A' { "A.P." } else { "a.p." }) => (Term::Meridiem { lower: chars[i] == 'a', dots: true }, 4),
            '<' if starts(i, "<JJJJ>") || starts(i, "<CCCC>") => (Term::Era, 6),
            '<' if starts(i, "<CCCCCCCC>") => (Term::Era, 10),
            _ => (Term::Literal(picture[i]), 1),
        };
        out.push(term);
        i += used;
    }
    out
}

/// A name in the case its picture term asks for, padded or cut to the term's width.
fn cased(name: &str, upper: &[bool], trim: bool) -> String {
    let mut s: String = name.chars().zip(upper).map(|(c, &u)| if u { c } else { c.to_ascii_lowercase() }).collect();
    if !trim {
        while s.len() < upper.len() {
            s.push(' ');
        }
    }
    s
}

/// Writes a date, or a timestamp when `time` (CEEDATM); without it every time term is zero, as
/// CEEDATE shows them (SA38-0683-60, Table 27).
pub fn format(terms: &[Term], f: &Fields, time: bool, page: &CodePage) -> Vec<u8> {
    let twelve = time && terms.iter().any(|t| matches!(t, Term::Meridiem { .. }));
    let (hour, minute, second, millis) = if time { (f.hour, f.minute, f.second, f.millis) } else { (0, 0, 0, 0) };
    let clock_hour = if twelve && hour % 12 == 0 { 12 } else if twelve { hour % 12 } else { hour };
    let mut out = Vec::new();
    for t in terms {
        let text = match t {
            Term::Literal(b) => {
                out.push(*b);
                continue;
            }
            Term::Year(4) => format!("{:04}", f.year),
            Term::Year(2) => format!("{:02}", f.year % 100),
            Term::Year(_) => format!("{}", f.year % 10),
            Term::Month(true) => f.month.to_string(),
            Term::Month(false) => format!("{:02}", f.month),
            Term::MonthName { upper, trim } => cased(MONTHS[f.month as usize - 1], upper, *trim),
            Term::Roman(trim) => cased(ROMAN[f.month as usize - 1], &[true; 4], *trim),
            Term::Day(true) => f.day.to_string(),
            Term::Day(false) => format!("{:02}", f.day),
            Term::DayOfYear => format!("{:03}", f.yday),
            Term::Hour(true) => clock_hour.to_string(),
            Term::Hour(false) => format!("{clock_hour:02}"),
            Term::Minute => format!("{minute:02}"),
            Term::Second => format!("{second:02}"),
            Term::Fraction(n) => format!("{:03}", millis)[..*n as usize].to_owned(),
            Term::Meridiem { lower, dots } => {
                let pm = time && hour >= 12;
                let s = match (pm, dots) {
                    (false, false) => "AM",
                    (true, false) => "PM",
                    (false, true) => "A.M.",
                    (true, true) => "P.M.",
                };
                if *lower { s.to_ascii_lowercase() } else { s.to_owned() }
            }
            Term::Weekday => DAYS[f.weekday as usize - 1][..1].to_owned(),
            Term::WeekdayName { upper, trim } => cased(DAYS[f.weekday as usize - 1], upper, *trim),
            Term::Era => String::new(),
        };
        out.extend(text.chars().map(|c| page.encode_char(c).unwrap_or(ebcdic::SPACE)));
    }
    out
}

/// A fixed-length 80-character output, and whether the formatted text was longer.
fn output_80(mut text: Vec<u8>) -> (Vec<u8>, bool) {
    let truncated = text.len() > 80;
    text.resize(80, ebcdic::SPACE);
    (text, truncated)
}

/// CEEDATE: a Lilian date in the picture's form, 80 bytes, and the condition if any.
pub fn date(lilian: i64, picture: &[u8], page: &CodePage) -> (Vec<u8>, Option<Condition>) {
    let blanks = vec![ebcdic::SPACE; 80];
    if !(1..=LAST_LILIAN).contains(&lilian) {
        return (blanks, Some(LILIAN_RANGE));
    }
    let terms = terms(&picture_or_default(picture, DEFAULT_DATE, page), page);
    if terms.contains(&Term::Era) {
        return (blanks, Some(PICTURE));
    }
    let fields = Stamp { lilian, millis: 0 }.fields();
    let (text, truncated) = output_80(format(&terms, &fields, false, page));
    (text, truncated.then_some(DATE_TRUNCATED))
}

/// CEEDATM: Lilian seconds in the picture's form, 80 bytes, and the condition if any.
pub fn timestamp(seconds: [u8; 8], picture: &[u8], page: &CodePage) -> (Vec<u8>, Option<Condition>) {
    let blanks = vec![ebcdic::SPACE; 80];
    let Some(ms) = hfp_millis(seconds).filter(|ms| (DAY_MS..(LAST_LILIAN + 1) * DAY_MS).contains(ms)) else {
        return (blanks, Some(SECONDS_RANGE));
    };
    let terms = terms(&picture_or_default(picture, DEFAULT_TIMESTAMP, page), page);
    if terms.contains(&Term::Era) {
        return (blanks, Some(PICTURE));
    }
    let (text, truncated) = output_80(format(&terms, &Stamp::from_millis(ms).fields(), true, page));
    (text, truncated.then_some(TIMESTAMP_TRUNCATED))
}

/// Which service is reading a date: CEEDAYS ignores time terms; CEESECS reads them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reading {
    Days,
    Seconds,
}

/// A numeric field of up to `width` digits, leading blanks allowed. A field before a delimiter may
/// be shorter, as 6/2/88 is for MM/DD/YY; any other must be full. Width 0 is a delimiter, which
/// takes one character, whatever it is.
fn number(input: &[char], at: &mut usize, width: usize, delimited: bool, nonnumeric: Condition) -> Result<u32, Condition> {
    if width == 0 {
        *at += 1;
        return Ok(0);
    }
    let (mut value, mut digits, mut taken) = (0u32, 0, 0);
    while taken < width && *at + taken < input.len() {
        match input[*at + taken] {
            ' ' if digits == 0 => {}
            c if c.is_ascii_digit() => {
                value = value * 10 + c.to_digit(10).unwrap_or(0);
                digits += 1;
            }
            _ => break,
        }
        taken += 1;
    }
    *at += taken;
    let complete = digits > 0 && (taken == width || delimited);
    match (complete, *at < input.len()) {
        (true, _) => Ok(value),
        (false, true) => Err(nonnumeric),
        (false, false) => Err(INSUFFICIENT),
    }
}

/// The run of characters `f` accepts, at most 20, uppercased.
fn word(input: &[char], at: &mut usize, f: fn(&char) -> bool) -> String {
    let w: String = input[*at..].iter().take_while(|c| f(c)).take(20).collect();
    *at += w.chars().count();
    w.to_ascii_uppercase()
}

#[derive(Default)]
struct Parsed {
    year: Option<(i64, bool)>,
    month: Option<u32>,
    day: Option<u32>,
    yday: Option<u32>,
    hour: Option<u32>,
    minute: u32,
    second: u32,
    millis: u32,
    pm: Option<bool>,
}

/// CEEDAYS and CEESECS: a date or timestamp read by its picture. `window` is the first year of
/// the hundred a two-digit year falls in.
pub fn read(input: &[u8], picture: &[u8], reading: Reading, window: i64, page: &CodePage) -> Result<Stamp, Condition> {
    let default = if reading == Reading::Days { DEFAULT_DATE } else { DEFAULT_TIMESTAMP };
    let picture = picture_or_default(picture, default, page);
    let terms = terms(&picture, page);
    if terms.iter().any(|t| matches!(t, Term::Era | Term::Year(1))) {
        return Err(PICTURE);
    }
    let nonnumeric = if reading == Reading::Days { DAYS_NONNUMERIC } else { SECS_NONNUMERIC };
    let input: Vec<char> = input.iter().map(|&b| page.decode_byte(b)).collect();
    let lead = terms.iter().take_while(|t| **t == Term::Literal(ebcdic::SPACE)).count();
    let mut at = if lead > 0 { lead } else { input.iter().take_while(|&&c| c == ' ').count() };
    let mut p = Parsed::default();
    for (k, t) in terms.iter().enumerate().skip(lead) {
        if at >= input.len() {
            if terms[k..].iter().any(|t| !t.is_time() && !matches!(t, Term::Literal(_) | Term::Weekday | Term::WeekdayName { .. })) {
                return Err(INSUFFICIENT);
            }
            break;
        }
        let delimited = matches!(terms.get(k + 1), Some(Term::Literal(_)));
        let mut number = |width: u8| number(&input, &mut at, width as usize, delimited, nonnumeric);
        let outcome = match t {
            Term::Literal(_) => number(0).map(drop),
            Term::Year(n) => number(*n).map(|y| p.year = Some((i64::from(y), *n == 2))),
            Term::Month(_) => number(2).map(|m| p.month = Some(m)),
            Term::Day(_) => number(2).map(|d| p.day = Some(d)),
            Term::DayOfYear => number(3).map(|d| p.yday = Some(d)),
            Term::Hour(_) => number(2).map(|h| p.hour = Some(h)),
            Term::Minute => number(2).map(|m| p.minute = m),
            Term::Second => number(2).map(|s| p.second = s),
            Term::Fraction(n) => number(*n).map(|f| p.millis = f * 10u32.pow(3 - u32::from(*n))),
            Term::MonthName { .. } => {
                let word = word(&input, &mut at, |c| c.is_ascii_alphabetic());
                MONTHS.iter().position(|m| word.len() >= 3 && m.starts_with(&word)).map(|m| p.month = Some(m as u32 + 1)).ok_or(MONTH)
            }
            Term::Roman(_) => {
                let word = word(&input, &mut at, |c| matches!(c, 'I' | 'V' | 'X' | 'i' | 'v' | 'x'));
                ROMAN.iter().position(|r| *r == word).map(|m| p.month = Some(m as u32 + 1)).ok_or(MONTH)
            }
            Term::Weekday | Term::WeekdayName { .. } => {
                word(&input, &mut at, |c| c.is_ascii_alphabetic());
                Ok(())
            }
            Term::Meridiem { dots, .. } => {
                let word: String = input[at..].iter().take(if *dots { 4 } else { 2 }).collect::<String>().to_ascii_uppercase();
                at += word.chars().count();
                match word.as_str() {
                    "AM" | "A.M." => Some(false),
                    "PM" | "P.M." => Some(true),
                    _ => None,
                }
                .map(|pm| p.pm = Some(pm))
                .ok_or(nonnumeric)
            }
            Term::Era => Err(PICTURE),
        };
        if !(reading == Reading::Days && t.is_time()) {
            outcome?;
        }
    }
    let Some((year, two_digit)) = p.year else { return Err(INSUFFICIENT) };
    let year = if two_digit { window + (year - window).rem_euclid(100) } else { year };
    let lilian = match (p.month, p.day, p.yday) {
        (_, _, Some(yday)) if p.month.is_none() || p.day.is_none() => {
            let length = if leap(year) { 366 } else { 365 };
            if !(1..=length).contains(&yday) {
                return Err(DATE_VALUE);
            }
            lilian(year, 1, 1) + i64::from(yday) - 1
        }
        (Some(month), Some(day), _) => {
            if !(1..=12).contains(&month) {
                return Err(MONTH);
            }
            if !(1..=days_in_month(year, month)).contains(&day) {
                return Err(DATE_VALUE);
            }
            lilian(year, month, day)
        }
        _ => return Err(INSUFFICIENT),
    };
    if !(1582..=9999).contains(&year) || !(1..=LAST_LILIAN).contains(&lilian) {
        return Err(DATE_RANGE);
    }
    if reading == Reading::Days {
        return Ok(Stamp { lilian, millis: 0 });
    }
    let hour = match (p.hour.unwrap_or(0), p.pm) {
        (h, None) if h <= 23 => h,
        (h @ 1..=12, Some(pm)) => h % 12 + if pm { 12 } else { 0 },
        _ => return Err(HOURS),
    };
    if p.minute > 59 {
        return Err(MINUTES);
    }
    if p.second > 59 {
        return Err(SECONDS_VALUE);
    }
    let millis = ((i64::from(hour) * 60 + i64::from(p.minute)) * 60 + i64::from(p.second)) * 1000 + i64::from(p.millis);
    Ok(Stamp { lilian, millis })
}

/// What a run keeps for the services: heap storage, and which output DDs it has started.
#[derive(Debug, Default)]
pub struct State {
    /// Each CEEGTST block: where it starts, its length, and whether CEEFRST has freed it.
    pub heap: Vec<(usize, usize, bool)>,
    pub written: Vec<String>,
}

impl State {
    /// The end of the highest heap block: run-unit storage below it is not released.
    pub fn heap_end(&self) -> usize {
        self.heap.iter().map(|&(at, len, _)| at + len).max().unwrap_or(0)
    }
}

/// CEE3DMP's options: the ddname FNAME names, and whether any keyword was not one it takes.
pub fn dump_options(options: &str) -> (String, bool) {
    const KEYWORDS: &[(&str, usize)] = &[
        ("ENCLAVE", 4), ("THREAD", 3), ("TRACEBACK", 5), ("NOTRACEBACK", 7), ("FILES", 4), ("NOFILES", 6), ("VARIABLES", 3),
        ("NOVARIABLES", 5), ("BLOCKS", 5), ("NOBLOCKS", 7), ("STORAGE", 4), ("NOSTORAGE", 6), ("REGSTOR", 4), ("STACKFRAME", 2),
        ("PAGESIZE", 4), ("FNAME", 5), ("CONDITION", 4), ("NOCONDITION", 6), ("ENTRY", 5), ("NOENTRY", 7), ("GENOPTS", 4), ("NOGENOPTS", 6),
    ];
    let mut dd = "CEEDUMP".to_owned();
    let mut invalid = false;
    let upper = options.to_ascii_uppercase();
    let mut rest = upper.as_str();
    loop {
        rest = rest.trim_start_matches([' ', ',']);
        if rest.is_empty() {
            break;
        }
        let word_end = rest.find([' ', ',', '(']).unwrap_or(rest.len());
        let (word, after) = rest.split_at(word_end);
        let (argument, after) = match after.strip_prefix('(') {
            Some(inner) => match inner.split_once(')') {
                Some((a, tail)) => (Some(a.trim()), tail),
                None => {
                    invalid = true;
                    (Some(inner.trim()), "")
                }
            },
            None => (None, after),
        };
        match KEYWORDS.iter().find(|(k, min)| word.len() >= *min && k.starts_with(word)) {
            Some(("FNAME", _)) => match argument.filter(|a| !a.is_empty()) {
                Some(a) => dd = a.to_owned(),
                None => invalid = true,
            },
            Some(_) => {}
            None => invalid = true,
        }
        rest = after;
    }
    (dd, invalid)
}

#[cfg(test)]
mod tests;
