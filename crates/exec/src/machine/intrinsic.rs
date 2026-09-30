//! The intrinsic functions beyond the first twenty-one: their arguments are evaluated here, their
//! semantics are `rt::intrinsic`'s.

use super::*;
use numeric::Arith;
use rt::intrinsic::numval::Form;
use rt::intrinsic::real::Real;
use rt::intrinsic::{self, dates, datetime, math, numval, text};
use std::ops::{Add, Div, RangeInclusive, Sub};

/// EBCDIC's substitution character, which DISPLAY-OF gives for a character the code page lacks
/// (Language Reference SC27-8713-03, p. 551).
const SUBSTITUTE: u8 = 0x3F;
const UTF8: u16 = 1208;

fn integer(n: i128, digits: u32) -> Val {
    Val::Num(Fixed::new(n, Places::new(digits, 0)))
}

fn exact_real(x: &Fixed) -> Real {
    Real::new(x.negative, x.magnitude, 0).div(Real::new(false, U256::pow10(x.places.dec), 0))
}

impl<'p> Machine<'p, '_, '_> {
    /// Whether an expression holding `f` is evaluated in floating point.
    pub(super) fn is_floating_point(&mut self, f: &FunctionCall) -> R<bool> {
        if intrinsic::FLOATING_POINT.contains(&f.name.as_str()) {
            return Ok(true);
        }
        if intrinsic::MIXED.contains(&f.name.as_str()) {
            for a in &f.args {
                if self.uses_float(a)? {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// The arguments' values; a table written with ALL subscripts gives one per element
    /// (Language Reference SC27-8713-03, pp. 501-502).
    pub(super) fn function_arguments(&mut self, f: &FunctionCall) -> R<Vec<Val>> {
        let mut out = Vec::with_capacity(f.args.len());
        for (i, a) in f.args.iter().enumerate() {
            match (f.all_subscripts.iter().find(|(k, _)| *k == i), a) {
                (Some((_, positions)), Expr::Operand(Operand::Ref(table))) => {
                    for element in self.all_elements(table, positions)? {
                        out.push(self.operand(&Operand::Ref(element), f.pos)?);
                    }
                }
                _ => out.push(self.expr_value(a, f.pos)?),
            }
        }
        Ok(out)
    }

    /// The elements ALL subscripts name, the rightmost ALL varying fastest; a dimension with
    /// OCCURS DEPENDING ON runs to its object's current value.
    fn all_elements(&mut self, table: &Ref, positions: &[usize]) -> R<Vec<Ref>> {
        let Resolved::Item(index) = self.resolve(table)? else {
            return Err(Abend::ironwork(format!("{} is a condition-name, not a table", table.name), table.pos));
        };
        let layout = self.layout;
        let mut dimensions = Vec::new();
        let mut at = Some(index);
        while let Some(i) = at {
            if layout.items[i].table {
                dimensions.push(i);
            }
            at = layout.items[i].parent;
        }
        dimensions.reverse();
        if dimensions.len() != table.subscripts.len() {
            return Err(Abend::ironwork(format!("{} takes {} subscripts, not {}", table.name, dimensions.len(), table.subscripts.len()), table.pos));
        }
        let mut counts = Vec::with_capacity(positions.len());
        for &p in positions {
            counts.push(self.occurrences(dimensions[p], table.pos)?);
        }
        let mut elements = Vec::new();
        if counts.contains(&0) {
            return Ok(elements);
        }
        let mut current = vec![1u32; positions.len()];
        loop {
            let mut element = table.clone();
            for (k, &p) in positions.iter().enumerate() {
                element.subscripts[p] = Expr::Operand(Operand::Literal(Literal::Number(current[k].to_string())));
            }
            elements.push(element);
            let mut k = positions.len();
            loop {
                if k == 0 {
                    return Ok(elements);
                }
                k -= 1;
                if current[k] < counts[k] {
                    current[k] += 1;
                    break;
                }
                current[k] = 1;
            }
        }
    }

    /// HEX-OF, BIT-OF and BYTE-LENGTH, which read an argument's bytes as stored, so that invalid
    /// data in a numeric item is shown rather than ending the run.
    pub(super) fn storage_function(&mut self, f: &FunctionCall) -> R<Option<Val>> {
        if !matches!(f.name.as_str(), "HEX-OF" | "BIT-OF" | "BYTE-LENGTH") {
            return Ok(None);
        }
        if f.args.len() != 1 {
            return Err(Abend::ironwork(format!("FUNCTION {} takes one argument", f.name), f.pos));
        }
        let bytes = self.stored_bytes(&f.args[0], f.pos)?;
        Ok(Some(match f.name.as_str() {
            "BYTE-LENGTH" => integer(bytes.len() as i128, 9),
            "HEX-OF" => self.text_value(&text::hex_of(&bytes), f.pos)?,
            _ => self.text_value(&text::bit_of(&bytes), f.pos)?,
        }))
    }

    /// An argument's bytes: an item's storage, or a value as DISPLAY would hold it.
    fn stored_bytes(&mut self, e: &Expr, pos: Pos) -> R<Vec<u8>> {
        if let Expr::Operand(Operand::Ref(r)) = e {
            let loc = self.locate(r)?;
            return Ok(self.bytes(loc).to_vec());
        }
        Ok(match self.expr_value(e, pos)? {
            Val::Bytes(b) | Val::National(b) | Val::All(b) => b,
            Val::Float(h) => h.to_bytes(),
            Val::Fig(fig) => vec![self.collating.figurative(fig)],
            Val::Address(a) => a.to_be_bytes().to_vec(),
            Val::Num(n) => {
                let digits = format!("{:0width$}", n.magnitude.to_u128().unwrap_or(0), width = n.places.total().max(1) as usize);
                let mut zoned = self.page.encode(&digits).map_err(|e| Abend::ironwork(e.to_string(), pos))?;
                if n.negative
                    && let Some(last) = zoned.last_mut()
                {
                    *last = (*last & 0x0F) | 0xD0;
                }
                zoned
            }
        })
    }

    fn text_value(&self, s: &str, pos: Pos) -> R<Val> {
        Ok(Val::Bytes(self.page.encode(s).map_err(|e| Abend::ironwork(e.to_string(), pos))?))
    }

    fn text_of(&self, v: &Val, name: &str, pos: Pos) -> R<String> {
        match v {
            Val::Bytes(b) | Val::All(b) => Ok(self.page.decode(b)),
            Val::National(b) => Ok(utf16_text(b)),
            _ => Err(Abend::ironwork(format!("FUNCTION {name} needs an alphanumeric or national argument"), pos)),
        }
    }

    /// A numeric argument as a floating-point function takes it: fixed point becomes HFP of the
    /// function's precision first (assumption C5).
    fn real(&self, v: &Val, p: Precision, name: &str, pos: Pos) -> R<Real> {
        match v {
            Val::Num(x) => Ok(Real::from_hfp(float::from_fixed(*x, p, ProgramMask::default()).map_err(|c| Abend::check(c, pos))?)),
            Val::Float(h) => Ok(Real::from_hfp(*h)),
            Val::Fig(Figurative::Zero) => Ok(Real::ZERO),
            _ => Err(Abend::ironwork(format!("FUNCTION {name} needs numeric arguments"), pos)),
        }
    }

    fn reals(&self, args: &[Val], p: Precision, name: &str, pos: Pos) -> R<Vec<Real>> {
        args.iter().map(|v| self.real(v, p, name, pos)).collect()
    }

    fn float_result(r: Real, p: Precision, pos: Pos) -> R<Val> {
        Ok(Val::Float(r.to_hfp(p).map_err(|c| Abend::check(c, pos))?))
    }

    /// An integer argument's value, any fraction truncated.
    fn whole(v: &Val, name: &str, pos: Pos) -> R<i128> {
        let n = match v {
            Val::Num(x) => x.magnitude.div_rem(U256::pow10(x.places.dec)).0.to_u128().and_then(|m| i128::try_from(m).ok()).map(|m| if x.negative { -m } else { m }),
            Val::Float(h) => h.to_integer(zarch::hfp::Rounding::TowardZero),
            Val::Fig(Figurative::Zero) => Some(0),
            _ => return Err(Abend::ironwork(format!("FUNCTION {name} needs integer arguments"), pos)),
        };
        n.ok_or_else(|| Abend::ironwork(format!("FUNCTION {name}: an argument beyond 38 digits"), pos))
    }

    fn compare_arguments(&self, a: &Val, b: &Val, name: &str, pos: Pos) -> R<Ordering> {
        let numeric = |v: &Val| matches!(v, Val::Num(_) | Val::Float(_) | Val::Fig(Figurative::Zero));
        let real = |v: &Val| match v {
            Val::Num(x) => exact_real(x),
            Val::Float(h) => Real::from_hfp(*h),
            _ => Real::ZERO,
        };
        let bytes = |v: &Val| match v {
            Val::Bytes(b) | Val::All(b) => Some(b.clone()),
            Val::Fig(fig) => Some(vec![self.collating.figurative(*fig)]),
            _ => None,
        };
        Ok(match (a, b) {
            (Val::Num(x), Val::Num(y)) => compare_fixed(x, y),
            (x, y) if numeric(x) && numeric(y) => real(x).compare(real(y)),
            (Val::National(x), Val::National(y)) => compare_national(x, y),
            (x, y) => match (bytes(x), bytes(y)) {
                (Some(x), Some(y)) => ebcdic::compare_alphanumeric(&x, &y, self.collating.collation()),
                _ => return Err(Abend::ironwork(format!("FUNCTION {name} needs arguments of one class"), pos)),
            },
        })
    }

    /// The leftmost argument with the greatest (`Greater`) or least (`Less`) value.
    fn extreme(&self, args: &[Val], want: Ordering, name: &str, pos: Pos) -> R<usize> {
        let mut best = 0;
        for i in 1..args.len() {
            if self.compare_arguments(&args[i], &args[best], name, pos)? == want {
                best = i;
            }
        }
        Ok(best)
    }

    fn format_argument(&self, v: &Val, name: &str, pos: Pos) -> R<datetime::Format> {
        let written = self.text_of(v, name, pos)?;
        datetime::Format::parse(&written).ok_or_else(|| Abend::ironwork(format!("FUNCTION {name}: {written} is not a date and time format (Language Reference SC27-8713-03, p. 504)"), pos))
    }

    fn integer_date(&self, v: &Val, name: &str, pos: Pos) -> R<i64> {
        let n = Self::whole(v, name, pos)?;
        if !(1..=i128::from(dates::LAST_INTEGER_DATE)).contains(&n) {
            return Err(Abend::ironwork(format!("FUNCTION {name}: the integer date {n} is outside 1 to {}", dates::LAST_INTEGER_DATE), pos));
        }
        Ok(n as i64)
    }

    /// Standard numeric time, zero to below 86,400 seconds, in nanoseconds with the rest truncated.
    fn nanos_of_day(&self, v: &Val, name: &str, pos: Pos) -> R<u64> {
        let scaled = match v {
            Val::Num(x) if x.places.dec <= 9 => x.magnitude.checked_mul(U256::pow10(9 - x.places.dec)).map(|m| (x.negative, m)),
            Val::Num(x) => Some((x.negative, x.magnitude.div_rem(U256::pow10(x.places.dec - 9)).0)),
            Val::Float(h) => h.to_scaled_integer(9, zarch::hfp::Rounding::TowardZero),
            Val::Fig(Figurative::Zero) => Some((false, U256::ZERO)),
            _ => return Err(Abend::ironwork(format!("FUNCTION {name} needs a numeric time"), pos)),
        };
        match scaled.and_then(|(negative, m)| m.to_u128().filter(|&m| !negative && m < u128::from(datetime::NANOS_PER_DAY))) {
            Some(m) => Ok(m as u64),
            None => Err(Abend::ironwork(format!("FUNCTION {name}: the time must be from 0 to below 86400 seconds"), pos)),
        }
    }

    fn utc_offset(&self, v: Option<&Val>, name: &str, pos: Pos) -> R<i32> {
        let Some(v) = v else { return Ok(0) };
        let minutes = Self::whole(v, name, pos)?;
        if !(-1439..=1439).contains(&minutes) {
            return Err(Abend::ironwork(format!("FUNCTION {name}: the offset {minutes} is outside -1439 to 1439 minutes"), pos));
        }
        Ok(minutes as i32)
    }

    fn current_year(&self) -> i64 {
        civil(self.unit.now().0).year
    }

    pub(super) fn more_function(&mut self, f: &FunctionCall, mut args: Vec<Val>) -> R<Val> {
        let (pos, name) = (f.pos, f.name.as_str());
        let p = self.options.arith.float_intermediate();
        let arity = |n: RangeInclusive<usize>, args: &[Val]| {
            if n.contains(&args.len()) { Ok(()) } else { Err(Abend::ironwork(format!("FUNCTION {name} takes {n:?} arguments, not {}", args.len()), pos)) }
        };
        let series = |args: &[Val]| if args.is_empty() { Err(Abend::ironwork(format!("FUNCTION {name} needs arguments"), pos)) } else { Ok(()) };
        let outside = |x: Real, why: &str| Abend::ironwork(format!("FUNCTION {name}({}): {why}", x.to_f64()), pos);
        match name {
            "SQRT" | "EXP" | "EXP10" | "LOG" | "LOG10" | "SIN" | "COS" | "TAN" | "ASIN" | "ACOS" | "ATAN" => {
                arity(1..=1, &args)?;
                let x = self.real(&args[0], p, name, pos)?;
                let (value, why) = match name {
                    "SQRT" => (math::sqrt(x), "the argument must be zero or positive"),
                    "EXP" => (Some(math::exp(x)), ""),
                    "EXP10" => (Some(math::exp10(x)), ""),
                    "LOG" => (math::ln(x), "the argument must be greater than zero"),
                    "LOG10" => (math::log10(x), "the argument must be greater than zero"),
                    "SIN" => (math::sin(x), "the argument is beyond the range ironwork for COBOL reduces"),
                    "COS" => (math::cos(x), "the argument is beyond the range ironwork for COBOL reduces"),
                    "TAN" => (math::tan(x), "the argument is beyond the range ironwork for COBOL reduces"),
                    "ASIN" => (math::asin(x), "the argument must be from -1 to +1"),
                    "ACOS" => (math::acos(x), "the argument must be from -1 to +1"),
                    _ => (Some(math::atan(x)), ""),
                };
                Self::float_result(value.ok_or_else(|| outside(x, why))?, p, pos)
            }
            "E" | "PI" => {
                arity(0..=0, &args)?;
                Self::float_result(if name == "E" { math::e() } else { math::pi() }, p, pos)
            }
            "ANNUITY" => {
                arity(2..=2, &args)?;
                let rate = self.real(&args[0], p, name, pos)?;
                let periods = Self::whole(&args[1], name, pos)?;
                let value = u128::try_from(periods).ok().and_then(|n| math::annuity(rate, n));
                Self::float_result(value.ok_or_else(|| outside(rate, "the rate must be zero or positive and the periods a positive integer"))?, p, pos)
            }
            "PRESENT-VALUE" => {
                if args.len() < 2 {
                    return Err(Abend::ironwork("FUNCTION PRESENT-VALUE needs a rate and at least one amount", pos));
                }
                let values = self.reals(&args, p, name, pos)?;
                let value = math::present_value(values[0], &values[1..]).ok_or_else(|| outside(values[0], "the rate must be greater than -1"))?;
                Self::float_result(value, p, pos)
            }
            "MEAN" | "MEDIAN" | "MIDRANGE" | "VARIANCE" | "STANDARD-DEVIATION" => {
                series(&args)?;
                let values = self.reals(&args, p, name, pos)?;
                let value = match name {
                    "MEAN" => math::mean(&values),
                    "MEDIAN" => math::median(&values),
                    "MIDRANGE" => math::midrange(&values),
                    "VARIANCE" => math::variance(&values),
                    _ => math::variance(&values).map(Real::sqrt),
                };
                Self::float_result(value.expect("a series with at least one value"), p, pos)
            }
            "MIN" | "MAX" | "ORD-MIN" | "ORD-MAX" | "RANGE" => {
                series(&args)?;
                let greatest = name.ends_with("MAX") || name == "RANGE";
                let best = self.extreme(&args, if greatest { Ordering::Greater } else { Ordering::Less }, name, pos)?;
                let floating = args.iter().any(|v| matches!(v, Val::Float(_)));
                match name {
                    "ORD-MIN" | "ORD-MAX" => Ok(integer(best as i128 + 1, 9)),
                    "RANGE" => {
                        let least = self.extreme(&args, Ordering::Less, name, pos)?;
                        match (&args[best], &args[least]) {
                            (Val::Num(hi), Val::Num(lo)) => {
                                let dmax = hi.places.dec.max(lo.places.dec);
                                Ok(Val::Num(hi.sub(*lo, dmax, self.options.arith).map_err(|_| Abend::ironwork("FUNCTION RANGE: a result beyond 256 bits", pos))?))
                            }
                            (hi, lo) => {
                                let (hi, lo) = (self.real(hi, p, name, pos)?, self.real(lo, p, name, pos)?);
                                Self::float_result(hi.sub(lo), p, pos)
                            }
                        }
                    }
                    _ if floating => Self::float_result(self.real(&args[best], p, name, pos)?, p, pos),
                    _ => Ok(args.swap_remove(best)),
                }
            }
            "SUM" => {
                series(&args)?;
                if args.iter().any(|v| matches!(v, Val::Float(_))) {
                    let values = self.reals(&args, p, name, pos)?;
                    return Self::float_result(values.iter().fold(Real::ZERO, |s, v| s.add(*v)), p, pos);
                }
                let mut fixed = Vec::with_capacity(args.len());
                for v in &args {
                    match v {
                        Val::Num(x) => fixed.push(*x),
                        Val::Fig(Figurative::Zero) => fixed.push(Fixed::new(0, Places::new(1, 0))),
                        _ => return Err(Abend::ironwork("FUNCTION SUM needs numeric arguments", pos)),
                    }
                }
                let dmax = fixed.iter().map(|x| x.places.dec).max().unwrap_or(0);
                let mut sum = Fixed::new(0, Places::new(1, 0));
                for x in fixed {
                    sum = sum.add(x, dmax, self.options.arith).map_err(|_| Abend::ironwork("FUNCTION SUM: a result beyond 256 bits", pos))?;
                }
                Ok(Val::Num(sum))
            }
            "SIGN" => {
                arity(1..=1, &args)?;
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
                arity(1..=1, &args)?;
                let n = Self::whole(&args[0], name, pos)?;
                let (most, digits) = if self.options.arith == Arith::Extend { (29, 31) } else { (28, 30) };
                if !(0..=most).contains(&n) {
                    return Err(Abend::ironwork(format!("FUNCTION FACTORIAL({n}): the argument must be from 0 to {most}"), pos));
                }
                Ok(integer((1..=n).product(), digits))
            }
            "DAY-OF-INTEGER" | "INTEGER-OF-DAY" | "TEST-DATE-YYYYMMDD" | "TEST-DAY-YYYYDDD" => {
                arity(1..=1, &args)?;
                let n = i64::try_from(Self::whole(&args[0], name, pos)?).unwrap_or(i64::MAX);
                match name {
                    "DAY-OF-INTEGER" => Ok(integer(dates::day_of_integer(n).ok_or_else(|| Abend::ironwork(format!("FUNCTION DAY-OF-INTEGER({n}): outside 1 to {}", dates::LAST_INTEGER_DATE), pos))?.into(), 7)),
                    "INTEGER-OF-DAY" => Ok(integer(dates::integer_of_day(n).ok_or_else(|| Abend::ironwork(format!("FUNCTION INTEGER-OF-DAY({n}): not a date from 1601001 to 9999365"), pos))?.into(), 7)),
                    "TEST-DATE-YYYYMMDD" => Ok(integer(dates::test_date(n).into(), 1)),
                    _ => Ok(integer(dates::test_day(n).into(), 1)),
                }
            }
            "YEAR-TO-YYYY" | "DATE-TO-YYYYMMDD" | "DAY-TO-YYYYDDD" => {
                arity(1..=2, &args)?;
                let n = i64::try_from(Self::whole(&args[0], name, pos)?).unwrap_or(i64::MAX);
                let window = match args.get(1) {
                    Some(v) => i64::try_from(Self::whole(v, name, pos)?).unwrap_or(i64::MAX),
                    None => 50,
                };
                let year = self.current_year();
                let (value, digits) = match name {
                    "YEAR-TO-YYYY" => (dates::year_to_yyyy(n, window, year), 4),
                    "DATE-TO-YYYYMMDD" => (dates::date_to_yyyymmdd(n, window, year), 8),
                    _ => (dates::day_to_yyyyddd(n, window, year), 7),
                };
                let value = value.ok_or_else(|| Abend::ironwork(format!("FUNCTION {name}({n} {window}): the argument is out of range, or the window's end year {} is not from 1700 to 9999", year.saturating_add(window)), pos))?;
                Ok(integer(value.into(), digits))
            }
            "SECONDS-PAST-MIDNIGHT" => {
                arity(0..=0, &args)?;
                let (seconds, hundredths) = self.unit.now();
                let c = civil(seconds);
                let of_day = i128::from((c.hour * 3600 + c.minute * 60 + c.second) * 100 + hundredths);
                Self::float_result(Real::from_i128(of_day).div(Real::from_u128(100)), p, pos)
            }
            "NUMVAL-F" | "TEST-NUMVAL" | "TEST-NUMVAL-C" | "TEST-NUMVAL-F" => {
                arity(if name == "TEST-NUMVAL-C" { 1..=2 } else { 1..=1 }, &args)?;
                let text = self.text_of(&args[0], name, pos)?;
                let currency = match args.get(1) {
                    Some(v) => self.text_of(v, name, pos)?,
                    None => "$".to_owned(),
                };
                let form = match name {
                    "TEST-NUMVAL" => Form::Numval,
                    "TEST-NUMVAL-C" => Form::Currency(&currency),
                    _ => Form::Exponent,
                };
                let digits = if self.options.arith == Arith::Extend { 31 } else { 18 };
                let comma = self.program.environment.decimal_point_comma;
                if name == "NUMVAL-F" {
                    let value = numval::parse(&text, form, digits, comma).map(|n| n.to_real()).unwrap_or(Real::ZERO);
                    return Self::float_result(value, p, pos);
                }
                Ok(integer(numval::test(&text, form, digits, comma) as i128, 9))
            }
            "HEX-TO-CHAR" | "BIT-TO-CHAR" => {
                arity(1..=1, &args)?;
                let text = self.text_of(&args[0], name, pos)?;
                let parsed = if name == "HEX-TO-CHAR" { text::hex_to_char(&text) } else { text::bit_to_char(&text) };
                parsed.map(Val::Bytes).map_err(|at| match at {
                    0 => Abend::ironwork(format!("FUNCTION {name}: the argument's length must be a multiple of {}", if name == "HEX-TO-CHAR" { 2 } else { 8 }), pos),
                    at => Abend::ironwork(format!("FUNCTION {name}: character {at} of the argument is not a {}", if name == "HEX-TO-CHAR" { "hexadecimal digit" } else { "0 or 1" }), pos),
                })
            }
            "DISPLAY-OF" => {
                arity(1..=2, &args)?;
                let Val::National(units) = &args[0] else {
                    return Err(Abend::ironwork("FUNCTION DISPLAY-OF needs a national argument", pos));
                };
                let ccsid = match args.get(1) {
                    Some(v) => u16::try_from(Self::whole(v, name, pos)?).unwrap_or(0),
                    None => self.options.codepage,
                };
                let chars = utf16_text(units);
                if ccsid == UTF8 {
                    return Ok(Val::Bytes(chars.into_bytes()));
                }
                let page = CodePage::by_ccsid(ccsid).ok_or_else(|| Abend::ironwork(format!("FUNCTION DISPLAY-OF: CCSID {ccsid} is not a code page ironwork for COBOL carries"), pos))?;
                Ok(Val::Bytes(chars.chars().map(|c| page.encode_char(c).unwrap_or(SUBSTITUTE)).collect()))
            }
            "FORMATTED-CURRENT-DATE" | "FORMATTED-DATE" | "FORMATTED-TIME" | "FORMATTED-DATETIME" => {
                arity(
                    match name {
                        "FORMATTED-CURRENT-DATE" => 1..=1,
                        "FORMATTED-DATE" => 2..=2,
                        "FORMATTED-TIME" => 2..=3,
                        _ => 3..=4,
                    },
                    &args,
                )?;
                let format = self.format_argument(&args[0], name, pos)?;
                let (date, time) = match name {
                    "FORMATTED-CURRENT-DATE" => (true, true),
                    "FORMATTED-DATE" => (true, false),
                    "FORMATTED-TIME" => (false, true),
                    _ => (true, true),
                };
                if format.has_date() != date || format.has_time() != time {
                    return Err(Abend::ironwork(format!("FUNCTION {name}: {} is not the format it takes", self.text_of(&args[0], name, pos)?), pos));
                }
                let (integer_date, nanos, offset) = match name {
                    "FORMATTED-CURRENT-DATE" => {
                        let (seconds, hundredths) = self.unit.now();
                        let integer_date = seconds.div_euclid(SECONDS_PER_DAY) + days_from_civil(1970, 1, 1) - days_from_civil(1600, 12, 31);
                        (integer_date, seconds.rem_euclid(SECONDS_PER_DAY) as u64 * datetime::NANOS_PER_SECOND + u64::from(hundredths) * 10_000_000, 0)
                    }
                    "FORMATTED-DATE" => (self.integer_date(&args[1], name, pos)?, 0, 0),
                    "FORMATTED-TIME" => (1, self.nanos_of_day(&args[1], name, pos)?, self.utc_offset(args.get(2), name, pos)?),
                    _ => (self.integer_date(&args[1], name, pos)?, self.nanos_of_day(&args[2], name, pos)?, self.utc_offset(args.get(3), name, pos)?),
                };
                let (integer_date, nanos) = if format.is_utc() {
                    let total = i128::from(integer_date) * i128::from(datetime::NANOS_PER_DAY) + i128::from(nanos) - i128::from(offset) * 60 * i128::from(datetime::NANOS_PER_SECOND);
                    let day = i128::from(datetime::NANOS_PER_DAY);
                    (total.div_euclid(day) as i64, total.rem_euclid(day) as u64)
                } else {
                    (integer_date, nanos)
                };
                let text = format.render(integer_date.clamp(1, dates::LAST_INTEGER_DATE), nanos, offset);
                match &args[0] {
                    Val::National(_) => Ok(Val::National(text.encode_utf16().flat_map(u16::to_be_bytes).collect())),
                    _ => self.text_value(&text, pos),
                }
            }
            "INTEGER-OF-FORMATTED-DATE" | "SECONDS-FROM-FORMATTED-TIME" | "TEST-FORMATTED-DATETIME" => {
                arity(2..=2, &args)?;
                let written = self.text_of(&args[0], name, pos)?;
                let format = self.format_argument(&args[0], name, pos)?;
                let value = self.text_of(&args[1], name, pos)?;
                match name {
                    "TEST-FORMATTED-DATETIME" => Ok(integer(format.read(&value).err().unwrap_or(0) as i128, 9)),
                    "INTEGER-OF-FORMATTED-DATE" => {
                        let date_part = written.split('T').next().unwrap_or_default();
                        let date = datetime::Format::parse(date_part).filter(|f| f.has_date()).ok_or_else(|| Abend::ironwork(format!("FUNCTION {name}: {written} has no date"), pos))?;
                        let prefix: String = value.chars().take(date.len()).collect();
                        let reading = date.read(&prefix).map_err(|at| Abend::ironwork(format!("FUNCTION {name}: character {at} of {value} does not fit {written}"), pos))?;
                        Ok(integer(reading.integer_date.unwrap_or(0).into(), 7))
                    }
                    _ => {
                        if !format.has_time() {
                            return Err(Abend::ironwork(format!("FUNCTION {name}: {written} has no time"), pos));
                        }
                        let reading = format.read(&value).map_err(|at| Abend::ironwork(format!("FUNCTION {name}: character {at} of {value} does not fit {written}"), pos))?;
                        let (seconds, fraction, digits) = reading.seconds.unwrap_or_default();
                        let scale = 10u128.pow(u32::from(digits));
                        let value = Real::from_u128(u128::from(seconds) * scale + u128::from(fraction)).div(Real::from_u128(scale));
                        Self::float_result(value, p, pos)
                    }
                }
            }
            "UUID4" => {
                arity(0..=0, &args)?;
                use std::hash::BuildHasher;
                let state = std::collections::hash_map::RandomState::new();
                let (seconds, hundredths) = self.unit.now();
                let random = u128::from(state.hash_one((seconds, hundredths, 1u8))) << 64 | u128::from(state.hash_one((seconds, hundredths, 2u8)));
                self.text_value(&text::uuid4(random), pos)
            }
            other => Err(Abend::ironwork(format!("FUNCTION {other} is not supported yet"), pos)),
        }
    }
}
