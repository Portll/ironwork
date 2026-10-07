//! FUNCTION (lir.md §9.9) once its arguments are evaluated: the argument counts, each function's
//! value, and reference modification of the result. The executor evaluates the arguments, as a
//! comparison evaluates an operand or expression, and ALL subscripts.

use super::numval::{self, Form};
use super::real::Real;
use super::{dates, datetime, math, text, unicode};
use crate::abend::{Abend, AbendCode};
use crate::calendar::{SECONDS_PER_DAY, civil};
use crate::display::utf16_text;
use crate::fixed::{align, compare_fixed};
use crate::lir::TrimSide;
use crate::storage::Val;
use crate::store::{ProgramFacts, compare_national};
use crate::vocab::{Figurative, Pos};
use numeric::precision::{Fixed, Places};
use numeric::{Arith, IntDate, float};
use std::cmp::Ordering;
use std::ops::{Add, Div, RangeInclusive, Sub};
use zarch::check::{ProgramCheck, ProgramMask};
use zarch::ebcdic::{self, CodePage};
use zarch::hfp::{Hfp, Precision, Rounding};
use zarch::wide::U256;

type R<T> = Result<T, Abend>;

const UTF8: u16 = 1208;

/// What a function reads beyond its arguments' values.
pub trait Evaluator {
    type Facts: ProgramFacts;
    fn facts(&self) -> Self::Facts;
    /// Argument `k` evaluated again as a subscript is: the first of CHAR, INTEGER-OF-DATE,
    /// DATE-OF-INTEGER and RANDOM, the second of NATIONAL-OF.
    fn integer(&mut self, k: usize, pos: Pos) -> R<i64>;
    /// The arguments as written, before ALL subscripts expand them.
    fn written(&self) -> usize;
    fn now(&self) -> (i64, u32);
    /// When the program was compiled, as `now` gives a time.
    fn compiled(&self) -> (i64, u32);
    /// The run unit's FUNCTION RANDOM state, from the first reference on.
    fn random(&mut self) -> &mut Option<u32>;
    /// The currency sign of NUMVAL-C and TEST-NUMVAL-C without argument-2 (assumption C102).
    fn currency(&self) -> String;
    /// The name of the program that called the running one, None in the main program.
    fn caller(&mut self) -> Option<String>;
    /// The length of the argument in USING position `position`, from 1, of the running
    /// activation: 0 for one omitted or not passed, and in the main program.
    fn argument_length(&mut self, position: usize) -> usize;
}

fn integer(n: i128, digits: u32) -> Val {
    Val::Num(Fixed::new(n, Places::new(digits, 0)))
}

fn exact_real(x: &Fixed) -> Real {
    Real::new(x.negative, x.magnitude, 0).div(Real::new(false, U256::pow10(x.places.dec), 0))
}

/// CURRENT-DATE's and WHEN-COMPILED's form of a UTC time: YYYYMMDDhhmmsshh+0000.
fn date_and_time(facts: &dyn ProgramFacts, (seconds, hundredths): (i64, u32), pos: Pos) -> R<Val> {
    let c = civil(seconds);
    text_value(facts, &format!("{:04}{:02}{:02}{:02}{:02}{:02}{hundredths:02}+0000", c.year, c.month, c.day, c.hour, c.minute, c.second), pos)
}

fn text_value(facts: &dyn ProgramFacts, s: &str, pos: Pos) -> R<Val> {
    Ok(Val::Bytes(facts.page().encode(s).map_err(|e| Abend::ironwork(e.to_string(), pos))?))
}

/// HEX-OF, BIT-OF and BYTE-LENGTH of an argument's bytes as stored, so that invalid data in a
/// numeric item is shown rather than ending the run.
pub fn storage(facts: &dyn ProgramFacts, name: &str, bytes: &[u8], pos: Pos) -> R<Val> {
    match name {
        "BYTE-LENGTH" => Ok(integer(bytes.len() as i128, 9)),
        "HEX-OF" => text_value(facts, &text::hex_of(bytes), pos),
        _ => text_value(facts, &text::bit_of(bytes), pos),
    }
}

/// The bytes of an argument that is not a data item, as DISPLAY would hold the value.
pub fn stored_bytes(facts: &dyn ProgramFacts, val: Val, pos: Pos) -> R<Vec<u8>> {
    Ok(match val {
        Val::Bytes(b) | Val::National(b) | Val::All(b) | Val::AllNational(b) | Val::Dbcs(b) => b,
        Val::Float(h) => h.to_bytes(),
        Val::Fig(fig) => vec![facts.figurative(fig)],
        Val::Address(a) => a.to_be_bytes().to_vec(),
        Val::Num(n) => {
            let digits = format!("{:0width$}", n.magnitude.to_u128().unwrap_or(0), width = n.places.total().max(1) as usize);
            let mut zoned = facts.page().encode(&digits).map_err(|e| Abend::ironwork(e.to_string(), pos))?;
            if n.negative
                && let Some(last) = zoned.last_mut()
            {
                *last = (*last & 0x0F) | 0xD0;
            }
            zoned
        }
    })
}

/// Reference modification of a result, alphanumeric or national (whose character positions are two
/// bytes); `bounds` evaluates the start and length, and is checked against the result whatever
/// SSRANGE says.
pub fn refmod(value: Val, pos: Pos, bounds: impl FnOnce() -> R<(i64, Option<i64>)>) -> R<Val> {
    let (unit, b) = match &value {
        Val::Bytes(b) => (1, b),
        Val::National(b) => (2, b),
        _ => return Err(Abend::ironwork("reference modification of a function result that is neither alphanumeric nor national", pos)),
    };
    let (start, length) = bounds()?;
    let len = length.unwrap_or((b.len() / unit) as i64 + 1 - start);
    let part = usize::try_from(start - 1)
        .ok()
        .zip(usize::try_from(len).ok())
        .and_then(|(s, l)| b.get(s * unit..(s + l) * unit))
        .map(<[u8]>::to_vec)
        .ok_or_else(|| Abend::ironwork("reference modification past the function result", pos))?;
    Ok(if unit == 2 { Val::National(part) } else { Val::Bytes(part) })
}

/// A function of its arguments' values. `side` is TRIM's LEADING or TRAILING.
pub fn evaluate(x: &mut impl Evaluator, name: &str, side: Option<TrimSide>, args: &mut Vec<Val>, pos: Pos) -> R<Val> {
    let facts = x.facts();
    let page = facts.page();
    let arity = |n: RangeInclusive<usize>| {
        if n.contains(&args.len()) { Ok(()) } else { Err(Abend::ironwork(format!("FUNCTION {name} takes {n:?} arguments"), pos)) }
    };
    let bytes_of = |v: &Val| match v {
        Val::Bytes(b) | Val::All(b) => Ok(b.clone()),
        Val::Fig(fig) => Ok(vec![facts.figurative(*fig)]),
        _ => Err(Abend::ironwork(format!("FUNCTION {name} needs an alphanumeric argument"), pos)),
    };
    Ok(match name {
        "CHAR" => {
            arity(1..=1)?;
            let n = x.integer(0, pos)?;
            let c = facts.character(n).ok_or_else(|| out_of_range("IGZ0162S", "Argument-1 for function CHAR was less than 1 or greater than the number of positions in the program collating sequence.".into(), n, pos))?;
            Val::Bytes(vec![c])
        }
        "ORD" => {
            arity(1..=1)?;
            let b = bytes_of(&args[0])?;
            let first = *b.first().ok_or_else(|| Abend::ironwork("FUNCTION ORD of an empty argument", pos))?;
            Val::Num(Fixed::new(facts.ordinal(first) as i128, Places::new(3, 0)))
        }
        "NATIONAL-OF" => {
            arity(1..=2)?;
            let ccsid = if args.len() == 2 { x.integer(1, pos)? as u16 } else { facts.options().codepage };
            let page = CodePage::by_ccsid(ccsid).ok_or_else(|| Abend::ironwork(format!("CCSID {ccsid} is not a code page ironwork for COBOL carries"), pos))?;
            match &args[0] {
                Val::Dbcs(b) => Val::National(page.decode_dbcs(b).encode_utf16().flat_map(u16::to_be_bytes).collect()),
                arg => Val::National(page.to_utf16be(&bytes_of(arg)?)),
            }
        }
        "LENGTH" => {
            arity(1..=1)?;
            let n = match &args[0] {
                Val::Bytes(b) | Val::All(b) => b.len(),
                Val::National(b) | Val::Dbcs(b) => b.len() / 2,
                Val::Num(v) => v.places.total() as usize,
                _ => return Err(crate::refusal::IWR0065.abend("FUNCTION LENGTH of this argument is not supported yet", pos)),
            };
            Val::Num(Fixed::new(n as i128, Places::new(9, 0)))
        }
        "NUMVAL" | "NUMVAL-C" => {
            arity(1..=2)?;
            let currency = match args.get(1) {
                Some(v) => text_of(&facts, v, name, pos)?,
                None => x.currency(),
            };
            let text = text_of(&facts, &args[0], name, pos)?;
            let form = if name == "NUMVAL-C" { Form::Currency(&currency) } else { Form::Numval };
            let arith = facts.options().arith;
            let digits = if arith == Arith::Extend { 31 } else { 18 };
            let value = numval::parse(&text, form, digits, facts.decimal_point() == ',').map(|n| n.to_real()).unwrap_or(Real::ZERO);
            // Long floating point under ARITH(COMPAT), extended under ARITH(EXTEND) (Programming
            // Guide SC27-8714-03, p. 115).
            float_result(value, arith.float_intermediate(), pos)?
        }
        "TRIM" => {
            arity(1..=1)?;
            let b = bytes_of(&args[0])?;
            let first = b.iter().position(|&c| c != ebcdic::SPACE);
            let last = b.iter().rposition(|&c| c != ebcdic::SPACE);
            let trimmed = match (first, last, side) {
                (None, _, _) | (_, None, _) => Vec::new(),
                (Some(s), _, Some(TrimSide::Leading)) => b[s..].to_vec(),
                (_, Some(e), Some(TrimSide::Trailing)) => b[..=e].to_vec(),
                (Some(s), Some(e), _) => b[s..=e].to_vec(),
            };
            Val::Bytes(trimmed)
        }
        "MOD" | "REM" | "INTEGER" | "INTEGER-PART" | "ABS" | "MIN" | "MAX" if args.iter().any(|v| matches!(v, Val::Float(_))) => {
            arity(match name {
                "MOD" | "REM" => 2..=2,
                "MIN" | "MAX" => 1..=usize::MAX,
                _ => 1..=1,
            })?;
            float_function(&facts, name, args, pos)?
        }
        "MOD" | "REM" | "INTEGER" | "INTEGER-PART" | "ABS" => {
            arity(if matches!(name, "MOD" | "REM") { 2..=2 } else { 1..=1 })?;
            let number = |v: &Val| match v {
                Val::Num(x) => Ok(*x),
                _ => Err(Abend::ironwork(format!("FUNCTION {name} needs numeric arguments"), pos)),
            };
            let x = number(&args[0])?;
            let given = args.iter().map(number).map(|v| v.map(|v| v.places)).collect::<R<Vec<Places>>>()?;
            let dec = given.iter().map(|p| p.dec).max().unwrap_or(0);
            let scaled = |v: &Fixed| -> R<i128> {
                let m = align(v, dec, false).and_then(|m| m.to_u128()).and_then(|m| i128::try_from(m).ok()).ok_or_else(|| Abend::ironwork("an argument beyond 38 digits", pos))?;
                Ok(if v.negative { -m } else { m })
            };
            let unit = 10i128.pow(dec);
            let a = scaled(&x)?;
            let result = match name {
                "ABS" => a.abs(),
                "INTEGER" => a.div_euclid(unit) * unit,
                "INTEGER-PART" => a / unit * unit,
                other => {
                    let b = scaled(&number(&args[1])?)?;
                    if b == 0 {
                        return Err(Abend::check(ProgramCheck::DecimalDivide, pos));
                    }
                    let r = a % b;
                    if other == "MOD" && r != 0 && (r < 0) != (b < 0) { r + b } else { r }
                }
            };
            let places = super::fixed_places(name, &given, facts.options().arith).expect("MOD, REM, INTEGER, INTEGER-PART and ABS have fixed places");
            let value = Fixed::new(result / 10i128.pow(dec - places.dec.min(dec)), places);
            // MOD's value has the digits of its shorter argument, high-order digits beyond them dropped.
            Val::Num(if name == "MOD" { value.fit(places) } else { value })
        }
        "INTEGER-OF-DATE" => {
            arity(1..=1)?;
            let n = x.integer(0, pos)?;
            let intdate = facts.options().intdate;
            let first = dates::date_of_integer(1, intdate).unwrap_or_default();
            let days = dates::integer_of_date(n, intdate).ok_or_else(|| out_of_range("IGZ0160S", format!("Argument-1 for function INTEGER-OF-DATE was less than {first} or greater than 99991231."), n, pos))?;
            Val::Num(Fixed::new(days.into(), Places::new(7, 0)))
        }
        "DATE-OF-INTEGER" => {
            arity(1..=1)?;
            let n = x.integer(0, pos)?;
            let intdate = facts.options().intdate;
            let date = dates::date_of_integer(n, intdate).ok_or_else(|| out_of_range("IGZ0159S", format!("Argument-1 for function DATE-OF-INTEGER was less than 1 or greater than {}.", dates::last_integer_date(intdate)), n, pos))?;
            Val::Num(Fixed::new(date.into(), Places::new(8, 0)))
        }
        "CURRENT-DATE" => {
            arity(0..=0)?;
            date_and_time(&facts, x.now(), pos)?
        }
        "UPPER-CASE" | "LOWER-CASE" | "REVERSE" if matches!(args.first(), Some(Val::National(_))) => {
            arity(1..=1)?;
            let Val::National(b) = &args[0] else { unreachable!() };
            Val::National(national_case(&utf16_text(b), name).encode_utf16().flat_map(u16::to_be_bytes).collect())
        }
        "UPPER-CASE" | "LOWER-CASE" | "REVERSE" => {
            arity(1..=1)?;
            let text = page.decode(&bytes_of(&args[0])?);
            let changed: String = match name {
                "UPPER-CASE" => text.to_uppercase(),
                "LOWER-CASE" => text.to_lowercase(),
                _ => text.chars().rev().collect(),
            };
            Val::Bytes(page.encode(&changed).map_err(|e| Abend::ironwork(e.to_string(), pos))?)
        }
        "RANDOM" => {
            arity(0..=1)?;
            let seed = if x.written() == 0 { None } else { Some(x.integer(0, pos)?) };
            Val::Float(random(x.random(), seed, pos)?)
        }
        _ => more(x, name, args, pos)?,
    })
}

/// A numeric function with a floating-point argument. ABS, REM, MIN and MAX are then evaluated in
/// floating point and return it; INTEGER and INTEGER-PART return an integer of 30 digits, 31
/// under ARITH(EXTEND) (Programming Guide SC27-8714-03, pp. 799 and 801); MOD takes integers
/// only (Language Reference SC27-8713-03, p. 507). Assumption FLOAT_FUNCTION_ARGUMENTS.
fn float_function(facts: &dyn ProgramFacts, name: &str, args: &[Val], pos: Pos) -> R<Val> {
    let arith = facts.options().arith;
    let p = arith.float_intermediate();
    let check = |r: Result<Hfp, ProgramCheck>| r.map_err(|c| Abend::check(c, pos));
    let float = |v: &Val| match v {
        Val::Float(h) if h.precision.digits() <= p.digits() => Ok(h.lengthen(p)),
        Val::Float(h) => Ok(float::narrow(*h, p)),
        Val::Num(x) => check(float::from_fixed(*x, p, ProgramMask::default())),
        _ => Err(Abend::ironwork(format!("FUNCTION {name} needs numeric arguments"), pos)),
    };
    let whole = |h: Hfp| h.to_integer(Rounding::TowardZero).ok_or_else(|| Abend::ironwork(format!("FUNCTION {name} of a floating-point value beyond 38 digits"), pos));
    let x = float(&args[0])?;
    Ok(match name {
        "ABS" => Val::Float(Hfp { negative: false, ..x }),
        "INTEGER" | "INTEGER-PART" => {
            let t = whole(x)?;
            let below = name == "INTEGER" && x.negative && Hfp::from_integer(t, p).compare(x) != Ordering::Equal;
            let t = t - i128::from(below);
            let digits = if arith == Arith::Compat { 30 } else { 31 };
            if t.unsigned_abs() >= 10u128.pow(digits) {
                return Err(Abend::ironwork(format!("FUNCTION {name} of a floating-point value beyond {digits} digits"), pos));
            }
            Val::Num(Fixed::new(t, Places::new(digits, 0)))
        }
        "REM" => {
            let y = float(&args[1])?;
            if y.fraction == 0 {
                return Err(Abend::check(ProgramCheck::HfpDivide, pos));
            }
            let part = check(x.div(y, ProgramMask::default()))?.integer_part();
            Val::Float(check(x.sub(check(y.mul(part, p, ProgramMask::default()))?, ProgramMask::default()))?)
        }
        "MIN" | "MAX" => {
            let want = if name == "MIN" { Ordering::Less } else { Ordering::Greater };
            let mut best = x;
            for v in &args[1..] {
                let y = float(v)?;
                if y.compare(best) == want {
                    best = y;
                }
            }
            Val::Float(best)
        }
        _ => return Err(Abend::ironwork(format!("FUNCTION {name} needs integer arguments, and a floating-point argument is not one"), pos)),
    })
}

/// FUNCTION RANDOM: the next number of the run unit's sequence, as long HFP. A seed starts a
/// new sequence (Language Reference SC27-8713-03, p. 629); the generator is assumption C54.
fn random(state: &mut Option<u32>, seed: Option<i64>, pos: Pos) -> R<Hfp> {
    const MODULUS: u64 = 2_147_483_647;
    let current = match (seed, *state) {
        (Some(n), _) if n < 0 => return Err(out_of_range("IGZ0163S", "Argument-1 for function RANDOM was less than zero.".into(), n, pos)),
        (Some(n), _) => n as u64 % (MODULUS - 1) + 1,
        (None, Some(s)) => u64::from(s),
        (None, None) => 1,
    };
    let next = current * 16807 % MODULUS;
    *state = Some(next as u32);
    let (x, m) = (Hfp::from_integer(next as i128, Precision::Long), Hfp::from_integer(MODULUS as i128, Precision::Long));
    x.div(m, ProgramMask::default()).map_err(|c| Abend::check(c, pos))
}

/// UPPER-CASE, LOWER-CASE or REVERSE of national text: a surrogate pair reverses as one character,
/// and a letter whose case mapping is more than one character is left as it is (assumption C192).
fn national_case(text: &str, name: &str) -> String {
    let one = |c: char, mapped: &mut dyn ExactSizeIterator<Item = char>| if mapped.len() == 1 { mapped.next().unwrap_or(c) } else { c };
    match name {
        "UPPER-CASE" => text.chars().map(|c| one(c, &mut c.to_uppercase())).collect(),
        "LOWER-CASE" => text.chars().map(|c| one(c, &mut c.to_lowercase())).collect(),
        _ => text.chars().rev().collect(),
    }
}

fn text_of(facts: &dyn ProgramFacts, v: &Val, name: &str, pos: Pos) -> R<String> {
    match v {
        Val::Bytes(b) | Val::All(b) => Ok(facts.page().decode(b)),
        Val::National(b) => Ok(utf16_text(b)),
        _ => Err(Abend::ironwork(format!("FUNCTION {name} needs an alphanumeric or national argument"), pos)),
    }
}

/// A numeric argument as a floating-point function takes it: fixed point becomes HFP of the
/// function's precision first (assumption C5).
fn real(v: &Val, p: Precision, name: &str, pos: Pos) -> R<Real> {
    match v {
        Val::Num(x) => Ok(Real::from_hfp(float::from_fixed(*x, p, ProgramMask::default()).map_err(|c| Abend::check(c, pos))?)),
        Val::Float(h) => Ok(Real::from_hfp(*h)),
        Val::Fig(Figurative::Zero) => Ok(Real::ZERO),
        _ => Err(Abend::ironwork(format!("FUNCTION {name} needs numeric arguments"), pos)),
    }
}

fn reals(args: &[Val], p: Precision, name: &str, pos: Pos) -> R<Vec<Real>> {
    args.iter().map(|v| real(v, p, name, pos)).collect()
}

fn float_result(r: Real, p: Precision, pos: Pos) -> R<Val> {
    Ok(Val::Float(r.to_hfp(p).map_err(|c| Abend::check(c, pos))?))
}

/// An integer argument's value, any fraction truncated.
fn whole(v: &Val, name: &str, pos: Pos) -> R<i128> {
    let n = match v {
        Val::Num(x) => x.magnitude.div_rem(U256::pow10(x.places.dec)).0.to_u128().and_then(|m| i128::try_from(m).ok()).map(|m| if x.negative { -m } else { m }),
        Val::Float(h) => h.to_integer(Rounding::TowardZero),
        Val::Fig(Figurative::Zero) => Some(0),
        _ => return Err(Abend::ironwork(format!("FUNCTION {name} needs integer arguments"), pos)),
    };
    n.ok_or_else(|| Abend::ironwork(format!("FUNCTION {name}: an argument beyond 38 digits"), pos))
}

fn compare_arguments(facts: &dyn ProgramFacts, a: &Val, b: &Val, name: &str, pos: Pos) -> R<Ordering> {
    let numeric = |v: &Val| matches!(v, Val::Num(_) | Val::Float(_) | Val::Fig(Figurative::Zero));
    let real = |v: &Val| match v {
        Val::Num(x) => exact_real(x),
        Val::Float(h) => Real::from_hfp(*h),
        _ => Real::ZERO,
    };
    let bytes = |v: &Val| match v {
        Val::Bytes(b) | Val::All(b) => Some(b.clone()),
        Val::Fig(fig) => Some(vec![facts.figurative(*fig)]),
        _ => None,
    };
    Ok(match (a, b) {
        (Val::Num(x), Val::Num(y)) => compare_fixed(x, y),
        (x, y) if numeric(x) && numeric(y) => real(x).compare(real(y)),
        (Val::National(x), Val::National(y)) => compare_national(x, y),
        (x, y) => match (bytes(x), bytes(y)) {
            (Some(x), Some(y)) => ebcdic::compare_alphanumeric(&x, &y, facts.collation()),
            _ => return Err(Abend::ironwork(format!("FUNCTION {name} needs arguments of one class"), pos)),
        },
    })
}

/// The places of a fixed-point MAX, MIN or RANGE value, when every argument is fixed point; ZERO
/// is a one-digit integer.
fn values_places(args: &[Val], name: &str, arith: Arith) -> Option<Places> {
    let places = args
        .iter()
        .map(|v| match v {
            Val::Num(x) => Some(x.places),
            Val::Fig(Figurative::Zero) => Some(Places::new(1, 0)),
            _ => None,
        })
        .collect::<Option<Vec<Places>>>()?;
    super::fixed_places(name, &places, arith)
}

/// A fixed-point argument at `places`, its decimal places filled out with zeros.
fn widened(v: &Val, places: Places) -> Fixed {
    match v {
        Val::Num(x) => Fixed { magnitude: align(x, places.dec, false).unwrap_or(x.magnitude), places, ..*x },
        _ => Fixed::new(0, places),
    }
}

/// The leftmost argument with the greatest (`Greater`) or least (`Less`) value.
fn extreme(facts: &dyn ProgramFacts, args: &[Val], want: Ordering, name: &str, pos: Pos) -> R<usize> {
    let mut best = 0;
    for i in 1..args.len() {
        if compare_arguments(facts, &args[i], &args[best], name, pos)? == want {
            best = i;
        }
    }
    Ok(best)
}

fn format_argument(facts: &dyn ProgramFacts, v: &Val, name: &str, pos: Pos) -> R<datetime::Format> {
    let written = text_of(facts, v, name, pos)?;
    datetime::Format::parse(&written).ok_or_else(|| Abend::ironwork(format!("FUNCTION {name}: {written} is not a date and time format (Language Reference SC27-8713-03, p. 504)"), pos))
}

/// The Language Environment math-services condition an argument outside a function's domain
/// signals, of severity 2, which ends the run U4038 when nothing handles it (assumption C112): the
/// routine is CEESD's long-precision service, CEESQ's under ARITH(EXTEND), and SIN, COS and TAN
/// signal CEE2017E from pi*(2**50), and 2**100 for CEESQ.
fn math_condition(name: &str, x: Real, p: Precision, pos: Pos) -> Option<Abend> {
    let (id, routine, condition) = match name {
        "SQRT" if x.is_negative() => ("CEE2010E", "SQT", "The argument was less than 0"),
        "LOG" if x.is_negative() || x.is_zero() => ("CEE2012E", "LOG", "The argument was less than or equal to 0"),
        "LOG10" if x.is_negative() || x.is_zero() => ("CEE2012E", "LG1", "The argument was less than or equal to 0"),
        "ASIN" | "ACOS" if x.abs().compare(Real::ONE) == Ordering::Greater => ("CEE2016E", if name == "ASIN" { "ASN" } else { "ACS" }, "The absolute value of the argument was greater than 1"),
        "SIN" | "COS" | "TAN" => {
            let long = p == Precision::Long;
            let limit = if long { math::pi().scaled(50) } else { Real::from_u128(1 << 100) };
            if x.abs().compare(limit) == Ordering::Less {
                return None;
            }
            let condition = if long { "The absolute value of the argument was greater than or equal to pi*(2**50)" } else { "The absolute value of the argument was greater than or equal to 2**100" };
            ("CEE2017E", &name[..3], condition)
        }
        _ => return None,
    };
    let service = if p == Precision::Long { "CEESD" } else { "CEESQ" };
    Some(Abend { code: AbendCode::user(4038), message: format!("{id} {condition} in math routine {service}{routine}. ({})", x.to_f64()), pos, file: None })
}

/// An argument outside what a function takes: IBM's message `id` and its text, a severity-3
/// condition that ends the run U4038 (assumption C452), what the argument held in parentheses.
fn out_of_range(id: &str, text: String, held: impl std::fmt::Display, pos: Pos) -> Abend {
    Abend { code: AbendCode::user(4038), message: format!("{id} {text} ({held})"), pos, file: None }
}

/// Argument `argument` of `name` as an integer date, refused with IGZ0372S outside 1 to the last.
fn integer_date(v: &Val, argument: usize, intdate: IntDate, name: &str, pos: Pos) -> R<i64> {
    let n = whole(v, name, pos)?;
    let last = dates::last_integer_date(intdate);
    if !(1..=i128::from(last)).contains(&n) {
        return Err(out_of_range("IGZ0372S", format!("Argument {argument} for function {name} was less than 1 or greater than {last}."), n, pos));
    }
    Ok(n as i64)
}

/// Standard numeric time, zero to below 86,400 seconds, in nanoseconds with the rest truncated.
fn nanos_of_day(v: &Val, argument: usize, name: &str, pos: Pos) -> R<u64> {
    let scaled = match v {
        Val::Num(x) if x.places.dec <= 9 => x.magnitude.checked_mul(U256::pow10(9 - x.places.dec)).map(|m| (x.negative, m)),
        Val::Num(x) => Some((x.negative, x.magnitude.div_rem(U256::pow10(x.places.dec - 9)).0)),
        Val::Float(h) => h.to_scaled_integer(9, Rounding::TowardZero),
        Val::Fig(Figurative::Zero) => Some((false, U256::ZERO)),
        _ => return Err(Abend::ironwork(format!("FUNCTION {name} needs a numeric time"), pos)),
    };
    match scaled.and_then(|(negative, m)| m.to_u128().filter(|&m| !negative && m < u128::from(datetime::NANOS_PER_DAY))) {
        Some(m) => Ok(m as u64),
        None => Err(out_of_range("IGZ0373S", format!("Argument {argument} for function {name} was less than 0 or greater than or equal to 86400."), "a time outside a day", pos)),
    }
}

fn utc_offset(v: Option<&Val>, argument: usize, name: &str, pos: Pos) -> R<i32> {
    let Some(v) = v else { return Ok(0) };
    let minutes = whole(v, name, pos)?;
    if !(-1439..=1439).contains(&minutes) {
        return Err(out_of_range("IGZ0374S", format!("Argument {argument} for function {name} was less than -1439 or greater than 1439."), minutes, pos));
    }
    Ok(minutes as i32)
}

/// The functions beyond the first twenty-one.
fn more(x: &mut impl Evaluator, name: &str, args: &mut Vec<Val>, pos: Pos) -> R<Val> {
    let facts = x.facts();
    let facts: &dyn ProgramFacts = &facts;
    let arith = facts.options().arith;
    let intdate = facts.options().intdate;
    let p = arith.float_intermediate();
    let arity = |n: RangeInclusive<usize>, args: &[Val]| {
        if n.contains(&args.len()) { Ok(()) } else { Err(Abend::ironwork(format!("FUNCTION {name} takes {n:?} arguments, not {}", args.len()), pos)) }
    };
    let series = |args: &[Val]| if args.is_empty() { Err(Abend::ironwork(format!("FUNCTION {name} needs arguments"), pos)) } else { Ok(()) };
    let outside = |x: Real, why: &str| Abend::ironwork(format!("FUNCTION {name}({}): {why}", x.to_f64()), pos);
    match name {
        "SQRT" | "EXP" | "EXP10" | "LOG" | "LOG10" | "SIN" | "COS" | "TAN" | "ASIN" | "ACOS" | "ATAN" => {
            arity(1..=1, args)?;
            let x = real(&args[0], p, name, pos)?;
            if let Some(abend) = math_condition(name, x, p, pos) {
                return Err(abend);
            }
            let value = match name {
                "SQRT" => math::sqrt(x),
                "EXP" => Some(math::exp(x)),
                "EXP10" => Some(math::exp10(x)),
                "LOG" => math::ln(x),
                "LOG10" => math::log10(x),
                "SIN" => math::sin(x),
                "COS" => math::cos(x),
                "TAN" => math::tan(x),
                "ASIN" => math::asin(x),
                "ACOS" => math::acos(x),
                _ => Some(math::atan(x)),
            };
            float_result(value.ok_or_else(|| outside(x, "the argument is beyond the range ironwork for COBOL reduces"))?, p, pos)
        }
        "E" | "PI" => {
            arity(0..=0, args)?;
            float_result(if name == "E" { math::e() } else { math::pi() }, p, pos)
        }
        "ANNUITY" => {
            arity(2..=2, args)?;
            let rate = real(&args[0], p, name, pos)?;
            let periods = whole(&args[1], name, pos)?;
            if rate.is_negative() {
                return Err(out_of_range("IGZ0029S", "Argument-1 for function ANNUITY was less than zero.".into(), rate.to_f64(), pos));
            }
            let value = u128::try_from(periods).ok().and_then(|n| math::annuity(rate, n));
            float_result(value.ok_or_else(|| out_of_range("IGZ0030S", "Argument-2 for function ANNUITY was not a positive integer.".into(), periods, pos))?, p, pos)
        }
        "PRESENT-VALUE" => {
            if args.len() < 2 {
                return Err(Abend::ironwork("FUNCTION PRESENT-VALUE needs a rate and at least one amount", pos));
            }
            let values = reals(args, p, name, pos)?;
            let value = math::present_value(values[0], &values[1..]).ok_or_else(|| out_of_range("IGZ0100S", "Argument-1 for function PRESENT-VALUE was less than or equal to -1.".into(), values[0].to_f64(), pos))?;
            float_result(value, p, pos)
        }
        "MEAN" | "MEDIAN" | "MIDRANGE" | "VARIANCE" | "STANDARD-DEVIATION" => {
            series(args)?;
            let values = reals(args, p, name, pos)?;
            let value = match name {
                "MEAN" => math::mean(&values),
                "MEDIAN" => math::median(&values),
                "MIDRANGE" => math::midrange(&values),
                "VARIANCE" => math::variance(&values),
                _ => math::variance(&values).map(Real::sqrt),
            };
            float_result(value.expect("a series with at least one value"), p, pos)
        }
        "MIN" | "MAX" | "ORD-MIN" | "ORD-MAX" | "RANGE" => {
            series(args)?;
            let greatest = name.ends_with("MAX") || name == "RANGE";
            let best = extreme(facts, args, if greatest { Ordering::Greater } else { Ordering::Less }, name, pos)?;
            let floating = args.iter().any(|v| matches!(v, Val::Float(_)));
            match name {
                "ORD-MIN" | "ORD-MAX" => Ok(integer(best as i128 + 1, 9)),
                "RANGE" => {
                    let least = extreme(facts, args, Ordering::Less, name, pos)?;
                    match (values_places(args, "MAX", arith), &args[best], &args[least]) {
                        (Some(widest), hi, lo) if !floating => {
                            let (hi, lo) = (widened(hi, widest), widened(lo, widest));
                            Ok(Val::Num(hi.sub(lo, widest.dec, arith).map_err(|_| Abend::ironwork("FUNCTION RANGE: a result beyond 256 bits", pos))?))
                        }
                        (_, hi, lo) => {
                            let (hi, lo) = (real(hi, p, name, pos)?, real(lo, p, name, pos)?);
                            float_result(hi.sub(lo), p, pos)
                        }
                    }
                }
                _ if floating => float_result(real(&args[best], p, name, pos)?, p, pos),
                _ => Ok(match values_places(args, name, arith) {
                    Some(widest) => Val::Num(widened(&args[best], widest)),
                    None => args.swap_remove(best),
                }),
            }
        }
        "SUM" => {
            series(args)?;
            if args.iter().any(|v| matches!(v, Val::Float(_))) {
                let values = reals(args, p, name, pos)?;
                return float_result(values.iter().fold(Real::ZERO, |s, v| s.add(*v)), p, pos);
            }
            let mut fixed = Vec::with_capacity(args.len());
            for v in args.iter() {
                match v {
                    Val::Num(x) => fixed.push(*x),
                    Val::Fig(Figurative::Zero) => fixed.push(Fixed::new(0, Places::new(1, 0))),
                    _ => return Err(Abend::ironwork("FUNCTION SUM needs numeric arguments", pos)),
                }
            }
            let dmax = fixed.iter().map(|x| x.places.dec).max().unwrap_or(0);
            let mut sum = Fixed::new(0, Places::new(1, 0));
            for x in fixed {
                sum = sum.add(x, dmax, arith).map_err(|_| Abend::ironwork("FUNCTION SUM: a result beyond 256 bits", pos))?;
            }
            Ok(Val::Num(sum))
        }
        "SIGN" => {
            arity(1..=1, args)?;
            let sign = match &args[0] {
                Val::Num(x) if x.magnitude.is_zero() => 0,
                Val::Num(x) => if x.negative { -1 } else { 1 },
                Val::Float(h) if h.fraction == 0 => 0,
                Val::Float(h) => if h.negative { -1 } else { 1 },
                Val::Fig(Figurative::Zero) => 0,
                _ => return Err(Abend::ironwork("FUNCTION SIGN needs a numeric argument", pos)),
            };
            Ok(integer(sign, 1))
        }
        "FACTORIAL" => {
            arity(1..=1, args)?;
            let n = whole(&args[0], name, pos)?;
            let (most, digits) = if arith == Arith::Extend { (29, 31) } else { (28, 30) };
            if !(0..=most).contains(&n) {
                let id = if most == 29 { "IGZ0223S" } else { "IGZ0156S" };
                return Err(out_of_range(id, format!("Argument-1 for function FACTORIAL was less than zero or greater than {most}."), n, pos));
            }
            Ok(integer((1..=n).product(), digits))
        }
        "DAY-OF-INTEGER" | "INTEGER-OF-DAY" | "TEST-DATE-YYYYMMDD" | "TEST-DAY-YYYYDDD" => {
            arity(1..=1, args)?;
            let n = i64::try_from(whole(&args[0], name, pos)?).unwrap_or(i64::MAX);
            match name {
                "DAY-OF-INTEGER" => Ok(integer(dates::day_of_integer(n, intdate).ok_or_else(|| out_of_range("IGZ0159S", format!("Argument-1 for function DAY-OF-INTEGER was less than 1 or greater than {}.", dates::last_integer_date(intdate)), n, pos))?.into(), 7)),
                "INTEGER-OF-DAY" => {
                    let first = dates::day_of_integer(1, intdate).unwrap_or_default();
                    Ok(integer(dates::integer_of_day(n, intdate).ok_or_else(|| out_of_range("IGZ0161S", format!("Argument-1 for function INTEGER-OF-DAY was less than {first} or greater than 9999365."), n, pos))?.into(), 7))
                }
                "TEST-DATE-YYYYMMDD" => Ok(integer(dates::test_date(n).into(), 1)),
                _ => Ok(integer(dates::test_day(n).into(), 1)),
            }
        }
        "YEAR-TO-YYYY" | "DATE-TO-YYYYMMDD" | "DAY-TO-YYYYDDD" => {
            arity(1..=2, args)?;
            let n = i64::try_from(whole(&args[0], name, pos)?).unwrap_or(i64::MAX);
            let window = match args.get(1) {
                Some(v) => i64::try_from(whole(v, name, pos)?).unwrap_or(i64::MAX),
                None => 50,
            };
            let year = civil(x.now().0).year;
            let (value, digits) = match name {
                "YEAR-TO-YYYY" => (dates::year_to_yyyy(n, window, year), 4),
                "DATE-TO-YYYYMMDD" => (dates::date_to_yyyymmdd(n, window, year), 8),
                _ => (dates::day_to_yyyyddd(n, window, year), 7),
            };
            let (id, most) = match name {
                "YEAR-TO-YYYY" => ("IGZ0215S", 99),
                "DATE-TO-YYYYMMDD" => ("IGZ0217S", 991_231),
                _ => ("IGZ0216S", 99_366),
            };
            if !(0..=most).contains(&n) {
                return Err(out_of_range(id, format!("Argument-1 for function {name} was less than 0 or greater than {most}."), n, pos));
            }
            let value = value.ok_or_else(|| out_of_range("IGZ0218S", format!("The sum of the year at the time of execution and the value of argument-2 was less than 1700 or greater than 10000 for function {name}."), year.saturating_add(window), pos))?;
            Ok(integer(value.into(), digits))
        }
        "SECONDS-PAST-MIDNIGHT" => {
            arity(0..=0, args)?;
            let (seconds, hundredths) = x.now();
            let c = civil(seconds);
            let of_day = i128::from((c.hour * 3600 + c.minute * 60 + c.second) * 100 + hundredths);
            float_result(Real::from_i128(of_day).div(Real::from_u128(100)), p, pos)
        }
        "NUMVAL-F" | "TEST-NUMVAL" | "TEST-NUMVAL-C" | "TEST-NUMVAL-F" => {
            arity(if name == "TEST-NUMVAL-C" { 1..=2 } else { 1..=1 }, args)?;
            let text = text_of(facts, &args[0], name, pos)?;
            let currency = match args.get(1) {
                Some(v) => text_of(facts, v, name, pos)?,
                None => x.currency(),
            };
            let form = match name {
                "TEST-NUMVAL" => Form::Numval,
                "TEST-NUMVAL-C" => Form::Currency(&currency),
                _ => Form::Exponent,
            };
            let digits = if arith == Arith::Extend { 31 } else { 18 };
            let comma = facts.decimal_point() == ',';
            if name == "NUMVAL-F" {
                let value = numval::parse(&text, form, digits, comma).map(|n| n.to_real()).unwrap_or(Real::ZERO);
                return float_result(value, p, pos);
            }
            Ok(integer(numval::test(&text, form, digits, comma) as i128, 9))
        }
        "HEX-TO-CHAR" | "BIT-TO-CHAR" => {
            arity(1..=1, args)?;
            let text = text_of(facts, &args[0], name, pos)?;
            let parsed = if name == "HEX-TO-CHAR" { text::hex_to_char(&text) } else { text::bit_to_char(&text) };
            parsed.map(Val::Bytes).map_err(|at| match at {
                0 => out_of_range("IGZ0348S", format!("Argument-1 for function {name} had a length that was not a multiple of {} bytes.", if name == "HEX-TO-CHAR" { 2 } else { 8 }), text.len(), pos),
                at => {
                    let c = text.chars().nth(at - 1).unwrap_or(' ');
                    Abend { code: AbendCode::user(4038), message: format!("IGZ0152S Invalid character {c} was found in column {at} in argument-1 for function {name}."), pos, file: None }
                }
            })
        }
        "DISPLAY-OF" => {
            arity(1..=2, args)?;
            let Val::National(units) = &args[0] else {
                return Err(Abend::ironwork("FUNCTION DISPLAY-OF needs a national argument", pos));
            };
            let ccsid = match args.get(1) {
                Some(v) => u16::try_from(whole(v, name, pos)?).unwrap_or(0),
                None => facts.options().codepage,
            };
            let chars = utf16_text(units);
            if ccsid == UTF8 {
                return Ok(Val::Bytes(chars.into_bytes()));
            }
            let page = CodePage::by_ccsid(ccsid).ok_or_else(|| Abend::ironwork(format!("FUNCTION DISPLAY-OF: CCSID {ccsid} is not a code page ironwork for COBOL carries"), pos))?;
            Ok(Val::Bytes(crate::display::to_page(page, units)))
        }
        "FORMATTED-CURRENT-DATE" | "FORMATTED-DATE" | "FORMATTED-TIME" | "FORMATTED-DATETIME" => {
            arity(
                match name {
                    "FORMATTED-CURRENT-DATE" => 1..=1,
                    "FORMATTED-DATE" => 2..=2,
                    "FORMATTED-TIME" => 2..=3,
                    _ => 3..=4,
                },
                args,
            )?;
            let format = format_argument(facts, &args[0], name, pos)?;
            let (date, time) = match name {
                "FORMATTED-CURRENT-DATE" => (true, true),
                "FORMATTED-DATE" => (true, false),
                "FORMATTED-TIME" => (false, true),
                _ => (true, true),
            };
            if format.has_date() != date || format.has_time() != time {
                return Err(Abend::ironwork(format!("FUNCTION {name}: {} is not the format it takes", text_of(facts, &args[0], name, pos)?), pos));
            }
            let (integer_date, nanos, offset) = match name {
                "FORMATTED-CURRENT-DATE" => {
                    let (seconds, hundredths) = x.now();
                    let integer_date = seconds.div_euclid(SECONDS_PER_DAY) - dates::day_zero(intdate);
                    (integer_date, seconds.rem_euclid(SECONDS_PER_DAY) as u64 * datetime::NANOS_PER_SECOND + u64::from(hundredths) * 10_000_000, 0)
                }
                "FORMATTED-DATE" => (self::integer_date(&args[1], 2, intdate, name, pos)?, 0, 0),
                "FORMATTED-TIME" => (1, nanos_of_day(&args[1], 2, name, pos)?, utc_offset(args.get(2), 3, name, pos)?),
                _ => (self::integer_date(&args[1], 2, intdate, name, pos)?, nanos_of_day(&args[2], 3, name, pos)?, utc_offset(args.get(3), 4, name, pos)?),
            };
            let (integer_date, nanos) = if format.is_utc() {
                let total = i128::from(integer_date) * i128::from(datetime::NANOS_PER_DAY) + i128::from(nanos) - i128::from(offset) * 60 * i128::from(datetime::NANOS_PER_SECOND);
                let day = i128::from(datetime::NANOS_PER_DAY);
                (total.div_euclid(day) as i64, total.rem_euclid(day) as u64)
            } else {
                (integer_date, nanos)
            };
            let text = format.render(integer_date.clamp(1, dates::last_integer_date(intdate)), nanos, offset, intdate);
            match &args[0] {
                Val::National(_) => Ok(Val::National(text.encode_utf16().flat_map(u16::to_be_bytes).collect())),
                _ => text_value(facts, &text, pos),
            }
        }
        "INTEGER-OF-FORMATTED-DATE" | "SECONDS-FROM-FORMATTED-TIME" | "TEST-FORMATTED-DATETIME" => {
            arity(2..=2, args)?;
            let written = text_of(facts, &args[0], name, pos)?;
            let format = format_argument(facts, &args[0], name, pos)?;
            let value = text_of(facts, &args[1], name, pos)?;
            match name {
                "TEST-FORMATTED-DATETIME" => Ok(integer(format.read(&value, IntDate::Ansi).err().unwrap_or(0) as i128, 9)),
                "INTEGER-OF-FORMATTED-DATE" => {
                    let date_part = written.split('T').next().unwrap_or_default();
                    let date = datetime::Format::parse(date_part).filter(|f| f.has_date()).ok_or_else(|| Abend::ironwork(format!("FUNCTION {name}: {written} has no date"), pos))?;
                    let prefix: String = value.chars().take(date.len()).collect();
                    let reading = date.read(&prefix, intdate).map_err(|at| Abend::ironwork(format!("FUNCTION {name}: character {at} of {value} does not fit {written}"), pos))?;
                    Ok(integer(reading.integer_date.unwrap_or(0).into(), 7))
                }
                _ => {
                    if !format.has_time() {
                        return Err(Abend::ironwork(format!("FUNCTION {name}: {written} has no time"), pos));
                    }
                    let reading = format.read(&value, IntDate::Ansi).map_err(|at| Abend::ironwork(format!("FUNCTION {name}: character {at} of {value} does not fit {written}"), pos))?;
                    let (seconds, fraction, digits) = reading.seconds.unwrap_or_default();
                    let scale = 10u128.pow(u32::from(digits));
                    let value = Real::from_u128(u128::from(seconds) * scale + u128::from(fraction)).div(Real::from_u128(scale));
                    float_result(value, p, pos)
                }
            }
        }
        "COMBINED-DATETIME" => {
            arity(2..=2, args)?;
            let date = integer_date(&args[0], 1, intdate, name, pos)?;
            let seconds = match &args[1] {
                Val::Num(x) => exact_real(x),
                Val::Float(h) => Real::from_hfp(*h),
                _ => return Err(Abend::ironwork(format!("FUNCTION {name} needs numeric arguments"), pos)),
            };
            if seconds.is_negative() || seconds.compare(Real::from_u128(86_400)) != Ordering::Less {
                return Err(out_of_range("IGZ0373S", format!("Argument 2 for function {name} was less than 0 or greater than or equal to 86400."), seconds.to_f64(), pos));
            }
            // A long-precision result whatever ARITH says (Language Reference SC27-8713-03, p. 541).
            let long = (Real::from_u128(date as u128) + seconds / Real::from_u128(100_000)).to_hfp(Precision::Long).map_err(|c| Abend::check(c, pos))?;
            Ok(Val::Float(if p.digits() > Precision::Long.digits() { long.lengthen(p) } else { long }))
        }
        "CONTENT-OF" => {
            arity(1..=1, args)?;
            Ok(args.swap_remove(0))
        }
        "ULENGTH" | "UPOS" | "USUBSTR" | "USUPPLEMENTARY" | "UVALID" | "UWIDTH" => {
            let n = match name {
                "UPOS" | "UWIDTH" => 2,
                "USUBSTR" => 3,
                _ => 1,
            };
            arity(n..=n, args)?;
            let (bytes, utf16) = match &args[0] {
                Val::National(b) => (b.as_slice(), true),
                Val::Bytes(b) | Val::All(b) => (b.as_slice(), false),
                _ => return Err(Abend::ironwork(format!("FUNCTION {name} needs an alphanumeric or national argument"), pos)),
            };
            let nth = |k: usize| -> R<Option<(usize, usize)>> {
                let n = whole(&args[k], name, pos)?;
                Ok(usize::try_from(n).ok().filter(|&n| n > 0).and_then(|n| unicode::characters(bytes, utf16).get(n - 1).copied()))
            };
            match name {
                "ULENGTH" => Ok(integer(unicode::characters(bytes, utf16).len() as i128, 9)),
                "UPOS" => Ok(integer(nth(1)?.map_or(0, |(at, _)| at + 1) as i128, 9)),
                "UWIDTH" => Ok(integer(nth(1)?.map_or(0, |(_, width)| width) as i128, 9)),
                "USUPPLEMENTARY" => Ok(integer(unicode::supplementary(bytes, utf16) as i128, 9)),
                "UVALID" => Ok(integer(unicode::invalid(bytes, utf16).unwrap_or(0) as i128, 9)),
                _ => {
                    let part = unicode::substring(bytes, utf16, whole(&args[1], name, pos)?, whole(&args[2], name, pos)?)
                        .ok_or_else(|| Abend::ironwork(format!("FUNCTION {name}: the substring reaches past argument-1's characters"), pos))?
                        .to_vec();
                    Ok(if utf16 { Val::National(part) } else { Val::Bytes(part) })
                }
            }
        }
        "WHEN-COMPILED" => {
            arity(0..=0, args)?;
            date_and_time(facts, x.compiled(), pos)
        }
        "MODULE-CALLER-ID" => {
            arity(0..=0, args)?;
            let caller = x.caller().unwrap_or_default();
            text_value(facts, &caller, pos)
        }
        "STORED-CHAR-LENGTH" => {
            arity(1..=1, args)?;
            let n = match &args[0] {
                Val::Bytes(b) | Val::All(b) => b.iter().rposition(|&c| c != ebcdic::SPACE).map_or(0, |last| last + 1),
                Val::National(b) => b.chunks(2).rposition(|c| c != [0x00, 0x20]).map_or(0, |last| last + 1),
                Val::Dbcs(b) => b.chunks(2).rposition(|c| c != [ebcdic::SPACE, ebcdic::SPACE]).map_or(0, |last| last + 1),
                _ => return Err(crate::refusal::IWR0065.abend("FUNCTION STORED-CHAR-LENGTH of this argument is not supported yet", pos)),
            };
            Ok(Val::Num(Fixed::new(n as i128, Places::new(9, 0))))
        }
        "ARGUMENT LENGTH" => {
            arity(1..=1, args)?;
            let position = usize::try_from(whole(&args[0], name, pos)?).unwrap_or_default();
            Ok(Val::Num(Fixed::new(x.argument_length(position) as i128, Places::new(9, 0))))
        }
        "UUID4" => {
            arity(0..=0, args)?;
            use std::hash::BuildHasher;
            let state = std::collections::hash_map::RandomState::new();
            let (seconds, hundredths) = x.now();
            let random = u128::from(state.hash_one((seconds, hundredths, 1u8))) << 64 | u128::from(state.hash_one((seconds, hundredths, 2u8)));
            text_value(facts, &text::uuid4(random), pos)
        }
        other => Err(crate::refusal::IWR0064.abend(format_args!("FUNCTION {other} is not supported yet"), pos)),
    }
}

#[cfg(test)]
mod tests {
    use super::national_case;

    #[test]
    fn national_reverse_keeps_surrogate_pairs_and_case_keeps_the_length() {
        assert_eq!(national_case("Tö\u{21DF3}b", "REVERSE"), "b\u{21DF3}öT");
        assert_eq!(national_case("straße", "UPPER-CASE"), "STRAßE");
        assert_eq!(national_case("ÄB\u{130}", "LOWER-CASE"), "äb\u{130}");
    }
}
