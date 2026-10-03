//! Intrinsic functions (lir.md §9.9), as `Machine::function` evaluates them, and what each result
//! reads as, which decides the MOVE and comparison plans it meets.

use super::data::{Side, Value, Within};
use super::{Lower, R, push, unsupported};
use crate::layout::Resolved;
use numeric::Arith;
use numeric::precision::{Places, carried, sum_places};
use rt::lir::{Argument, Comparand, Count, Func, FunctionPlan, Mode, Odo, RefMod, TrimSide};
use syntax::Pos;
use syntax::ast::{Expr, Figurative, FunctionCall, Literal, Operand, Ref};

/// A table with ALL subscripts of at most this many elements lowers as its elements, one argument
/// each; a larger one, or one whose ALL subscripts run to an OCCURS DEPENDING ON count, as
/// `Argument::All`, which expands when the function runs.
const LISTED: u64 = 256;

/// What an argument reads as, whether it is an item with PICTURE scaling positions, whose value
/// has digits its kind does not show, and how many values it gives: None where OCCURS DEPENDING ON
/// decides, at run time.
#[derive(Clone, Copy)]
struct Arg {
    side: Side,
    scaled: bool,
    times: Option<u64>,
}

impl Lower<'_> {
    pub(super) fn function(&mut self, f: &FunctionCall) -> R<(rt::lir::Operand, Side)> {
        if let Some(udf) = self.user_defined(&f.name) {
            return self.user_function(udf, f);
        }
        let Some(func) = Func::named(&f.name) else { return unsupported("a FUNCTION the LIR does not name", f.pos) };
        let pos = f.pos;
        let (args, sides) = if matches!(func, Func::HexOf | Func::BitOf | Func::ByteLength) { self.stored_argument(f)? } else { self.arguments(f)? };
        let count = sides.iter().try_fold(0u64, |n, a| Some(n + a.times?));
        let arity = match arity_message(func, &f.name, count, &sides, pos)? {
            Some(message) => Some(self.ironwork(&message)?),
            None => None,
        };
        let again = match func {
            _ if arity.is_some() && count.is_some() => None,
            Func::Char | Func::IntegerOfDate | Func::DateOfInteger | Func::Random => f.args.first(),
            Func::NationalOf => f.args.get(1),
            _ => None,
        };
        let integer = match again {
            Some(a) => Some(self.int_expr(a, pos)?),
            None => None,
        };
        let side = match (func, f.modifier.as_deref()) {
            (Func::Trim, Some("LEADING")) => Some(TrimSide::Leading),
            (Func::Trim, Some("TRAILING")) => Some(TrimSide::Trailing),
            _ => None,
        };
        let refmod = match &f.refmod {
            None => None,
            Some(rm) => {
                let start = self.int_expr(&rm.start, pos)?;
                let length = match &rm.length {
                    Some(l) => Some(self.int_expr(l, pos)?),
                    None => None,
                };
                Some(RefMod { start, length, check: false })
            }
        };
        let result = result(func, &sides, self.c.options.arith, pos)?;
        let at = self.at(pos);
        let id = push(&mut self.plans.function, FunctionPlan { func, args, integer, side, refmod, arity, at }, "FUNCTION plans")?;
        Ok((rt::lir::Operand::Function(id), result))
    }

    /// `function_arguments`: each argument as it evaluates it, and a table written with ALL
    /// subscripts as its elements.
    fn arguments(&mut self, f: &FunctionCall) -> R<(Vec<Argument>, Vec<Arg>)> {
        let (mut args, mut sides) = (Vec::with_capacity(f.args.len()), Vec::with_capacity(f.args.len()));
        for (i, a) in f.args.iter().enumerate() {
            match (f.all_subscripts.iter().find(|(k, _)| *k == i), a) {
                (Some((_, positions)), Expr::Operand(Operand::Ref(table))) => self.all_elements(table, positions, &mut args, &mut sides)?,
                _ => {
                    let (arg, side) = self.argument(a, f.pos)?;
                    args.push(Argument::Value(arg));
                    sides.push(Arg { side, scaled: self.scaled(a), times: Some(1) });
                }
            }
        }
        Ok((args, sides))
    }

    /// One argument as `function_arguments` evaluates it: an operand as `expr_value` reads it, and
    /// an expression in the arithmetic of the expression holding the function, with its own
    /// decimal places where they are more.
    fn argument(&mut self, a: &Expr, pos: Pos) -> R<(Comparand, Side)> {
        let (mode, dmax, prepass) = match (self.within, a) {
            (_, Expr::Operand(_)) | (Within::Own, _) => return self.comparand(a, pos),
            (Within::Fixed(dmax), _) => (Mode::Fixed, dmax.max(self.dmax(a)?), self.dmax_places(a)?),
            (Within::Float(p), _) => (Mode::Float(p), 0, Vec::new()),
        };
        let expr = self.expr_within(a, pos, Within::of(mode, dmax))?;
        let value = if matches!(mode, Mode::Float(_)) { Value::Float } else { Value::Num(None) };
        Ok((Comparand::Expr { expr, dmax, mode, prepass }, Side { value, src: None, digits: 0 }))
    }

    /// `storage_function`: HEX-OF, BIT-OF and BYTE-LENGTH count their arguments as written before
    /// evaluating any, and read an item's bytes as stored; ALL subscripts are not expanded.
    fn stored_argument(&mut self, f: &FunctionCall) -> R<(Vec<Argument>, Vec<Arg>)> {
        let [a] = f.args.as_slice() else { return Ok((Vec::new(), Vec::new())) };
        let (arg, side) = self.comparand(a, f.pos)?;
        Ok((vec![Argument::Value(arg)], vec![Arg { side, scaled: self.scaled(a), times: Some(1) }]))
    }

    /// `all_elements`: the elements ALL subscripts name, the rightmost ALL varying fastest; a
    /// dimension with OCCURS DEPENDING ON runs to its object's current value.
    fn all_elements(&mut self, table: &Ref, positions: &[usize], args: &mut Vec<Argument>, sides: &mut Vec<Arg>) -> R<()> {
        let layout = self.layout;
        let Ok(Resolved::Item(index)) = layout.resolve(&table.name, &table.qualifiers, table.pos) else {
            return unsupported("ALL subscripts on a name that is not a data item", table.pos);
        };
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
            return unsupported("a reference with the wrong number of subscripts", table.pos);
        }
        let mut counts = Vec::with_capacity(positions.len());
        for &p in positions {
            let t = &layout.items[dimensions[p]];
            counts.push(match &t.depending_on {
                None => Count::Fixed(t.occurs),
                Some(object) => {
                    let object = self.int_expr(&Expr::Operand(Operand::Ref(object.clone())), table.pos)?;
                    Count::Odo(Odo { object, max: t.occurs, element: t.size, check: self.c.ssrange })
                }
            });
        }
        let fixed: Option<Vec<u32>> = counts.iter().map(|c| if let Count::Fixed(n) = c { Some(*n) } else { None }).collect();
        let total = fixed.as_ref().map(|n| n.iter().map(|&n| u64::from(n)).product::<u64>());
        let scaled = self.layout.items[index].scaling > 0;
        if let (Some(n), Some(total)) = (&fixed, total)
            && total <= LISTED
        {
            let mut current = vec![1u32; positions.len()];
            for _ in 0..total {
                let mut element = table.clone();
                for (k, &p) in positions.iter().enumerate() {
                    element.subscripts[p] = Expr::Operand(Operand::Literal(Literal::Number(current[k].to_string())));
                }
                let (arg, side) = self.comparand(&Expr::Operand(Operand::Ref(element)), table.pos)?;
                args.push(Argument::Value(arg));
                sides.push(Arg { side, scaled, times: Some(1) });
                for k in (0..positions.len()).rev() {
                    if current[k] < n[k] {
                        current[k] += 1;
                        break;
                    }
                    current[k] = 1;
                }
            }
            return Ok(());
        }
        let element = self.place(table, false)?;
        let side = self.operand(&Operand::Ref(table.clone()), table.pos)?.side;
        let all = positions.iter().map(|&p| p as u32).zip(counts).collect();
        args.push(Argument::All { element, all });
        sides.push(Arg { side, scaled, times: total });
        Ok(())
    }

    /// Whether an argument is an item with PICTURE scaling positions.
    fn scaled(&self, e: &Expr) -> bool {
        let Expr::Operand(Operand::Ref(r)) = e else { return false };
        matches!(self.layout.resolve(&r.name, &r.qualifiers, r.pos), Ok(Resolved::Item(i)) if self.layout.items[i].scaling > 0)
    }
}

/// The walker's message when the arguments give a number of values `func` does not take, or None
/// where they cannot. A count known only at run time is refused where the message names it.
fn arity_message(func: Func, name: &str, count: Option<u64>, sides: &[Arg], pos: Pos) -> R<Option<String>> {
    let range = func.arity();
    let (least, most) = (*range.start() as u64, *range.end() as u64);
    let fewest: u64 = sides.iter().filter_map(|a| a.times).sum();
    let fits = match count {
        Some(n) => (least..=most).contains(&n),
        None => fewest >= least && range.end() == &usize::MAX,
    };
    if fits {
        return Ok(None);
    }
    Ok(Some(match func {
        Func::HexOf | Func::BitOf | Func::ByteLength => format!("FUNCTION {name} takes one argument"),
        Func::PresentValue => "FUNCTION PRESENT-VALUE needs a rate and at least one amount".to_owned(),
        _ if range.end() == &usize::MAX => format!("FUNCTION {name} needs arguments"),
        Func::Char
        | Func::Ord
        | Func::NationalOf
        | Func::Length
        | Func::UpperCase
        | Func::LowerCase
        | Func::Reverse
        | Func::CurrentDate
        | Func::Numval
        | Func::NumvalC
        | Func::Trim
        | Func::Mod
        | Func::Rem
        | Func::Integer
        | Func::IntegerPart
        | Func::Abs
        | Func::IntegerOfDate
        | Func::DateOfInteger
        | Func::Random => format!("FUNCTION {name} takes {range:?} arguments"),
        _ => match count {
            Some(n) => format!("FUNCTION {name} takes {range:?} arguments, not {n}"),
            None => return unsupported("a FUNCTION of fixed arguments given a table whose ALL subscripts run to an OCCURS DEPENDING ON count", pos),
        },
    }))
}

/// The places of a number an argument gives, where lowering knows them.
fn places(a: &Arg) -> Option<Places> {
    match a.side.value {
        _ if a.scaled => None,
        Value::Num(Some(dec)) => Some(Places::new(a.side.digits.saturating_sub(dec), dec)),
        Value::Fig(Figurative::Zero) => Some(Places::new(1, 0)),
        _ => None,
    }
}

fn num(dec: Option<u32>, digits: u32) -> Side {
    Side { value: Value::Num(dec), src: None, digits }
}

fn of(value: Value) -> Side {
    Side { value, src: None, digits: 0 }
}

/// What the function's value reads as: alphanumeric bytes, national units, a float, or a number
/// with its decimal places and digits where they are the same on every call (`Num(None)` where
/// they depend on the argument's text, on which argument wins or on how many values an OCCURS
/// DEPENDING ON table gives). A floating-point argument makes ABS, REM, MIN, MAX and SUM
/// floating-point and INTEGER and INTEGER-PART 30 digits, 31 under ARITH(EXTEND) (`float_function`).
fn result(func: Func, args: &[Arg], arith: Arith, pos: Pos) -> R<Side> {
    if let Some(d) = args.iter().find(|a| a.times.is_none())
        && args.iter().any(|a| a.side.value != d.side.value)
    {
        return unsupported("FUNCTION arguments of different kinds with a table whose ALL subscripts run to an OCCURS DEPENDING ON count", pos);
    }
    let integer = |digits: u32| num(Some(0), digits);
    let float = args.iter().any(|a| a.side.value == Value::Float);
    Ok(match func {
        Func::Abs | Func::Rem | Func::Min | Func::Max | Func::Sum if float => of(Value::Float),
        Func::Integer | Func::IntegerPart if float => integer(if arith == Arith::Compat { 30 } else { 31 }),
        Func::Mod if float => num(None, 0),
        Func::Char
        | Func::CurrentDate
        | Func::HexOf
        | Func::BitOf
        | Func::HexToChar
        | Func::BitToChar
        | Func::DisplayOf
        | Func::Uuid4
        | Func::WhenCompiled => of(Value::Bytes),
        Func::NationalOf => of(Value::National),
        Func::UpperCase
        | Func::LowerCase
        | Func::Reverse
        | Func::Trim
        | Func::FormattedCurrentDate
        | Func::FormattedDate
        | Func::FormattedTime
        | Func::FormattedDatetime
        | Func::Usubstr => {
            of(if args.first().is_some_and(|a| a.side.value == Value::National) { Value::National } else { Value::Bytes })
        }
        Func::Random
        | Func::Numval
        | Func::NumvalC
        | Func::CombinedDatetime
        | Func::Acos
        | Func::Annuity
        | Func::Asin
        | Func::Atan
        | Func::Cos
        | Func::E
        | Func::Exp
        | Func::Exp10
        | Func::Log
        | Func::Log10
        | Func::Mean
        | Func::Median
        | Func::Midrange
        | Func::NumvalF
        | Func::Pi
        | Func::PresentValue
        | Func::SecondsFromFormattedTime
        | Func::SecondsPastMidnight
        | Func::Sin
        | Func::Sqrt
        | Func::StandardDeviation
        | Func::Tan
        | Func::Variance => of(Value::Float),
        Func::Sign | Func::TestDateYyyymmdd | Func::TestDayYyyyddd => integer(1),
        Func::Ord => integer(3),
        Func::YearToYyyy => integer(4),
        Func::IntegerOfDate | Func::DayOfInteger | Func::IntegerOfDay | Func::DayToYyyyddd | Func::IntegerOfFormattedDate => integer(7),
        Func::DateOfInteger | Func::DateToYyyymmdd => integer(8),
        Func::Length
        | Func::ByteLength
        | Func::OrdMin
        | Func::OrdMax
        | Func::TestNumval
        | Func::TestNumvalC
        | Func::TestNumvalF
        | Func::TestFormattedDatetime
        | Func::Ulength
        | Func::Upos
        | Func::Usupplementary
        | Func::Uvalid
        | Func::Uwidth => integer(9),
        Func::Factorial => integer(if arith == Arith::Extend { 31 } else { 30 }),
        // The argument's own value.
        Func::ContentOf => match args.first() {
            Some(a) if a.scaled => num(None, 0),
            Some(a) => Side { src: None, ..a.side },
            None => num(None, 0),
        },
        Func::Sum => sum(args, arith),
        Func::Range => range(args, arith, pos)?,
        // The result has 31 digits, as many decimal places as the arguments have at most.
        Func::Mod | Func::Rem | Func::Integer | Func::IntegerPart | Func::Abs => {
            let decs: Option<Vec<u32>> = args.iter().map(|a| if let Value::Num(Some(d)) = a.side.value { Some(d) } else { None }).collect();
            match decs.and_then(|d| d.into_iter().max()) {
                Some(dec) if dec <= 31 => num(Some(dec), 31),
                _ => num(None, 0),
            }
        }
        // The winning argument's own value.
        Func::Min | Func::Max => {
            let Some((first, rest)) = args.split_first() else { return Ok(num(None, 0)) };
            let first = first.side;
            let same = |v: Value| rest.iter().all(|a| a.side.value == v);
            match first.value {
                Value::Num(_) if args.iter().all(|a| matches!(a.side.value, Value::Num(_)) && !a.scaled) => {
                    if same(first.value) && rest.iter().all(|a| a.side.digits == first.digits) {
                        num(if let Value::Num(d) = first.value { d } else { None }, first.digits)
                    } else {
                        num(None, 0)
                    }
                }
                Value::Num(_) if args.iter().all(|a| matches!(a.side.value, Value::Num(_))) => num(None, 0),
                v @ (Value::Bytes | Value::National | Value::Float) if same(v) => of(v),
                _ => return unsupported("FUNCTION MIN or MAX of arguments of different kinds", pos),
            }
        }
    })
}

/// SUM of fixed-point values: `Fixed::add` from a one-digit zero, each argument in turn, the
/// arguments' most decimal places kept.
fn sum(args: &[Arg], arith: Arith) -> Side {
    let terms: Option<Vec<(Places, u64)>> = args.iter().map(|a| Some((places(a)?, a.times?))).collect();
    let Some(terms) = terms else { return num(None, 0) };
    let dmax = terms.iter().map(|(p, _)| p.dec).max().unwrap_or(0);
    let mut total = Places::new(1, 0);
    for (p, times) in terms {
        for _ in 0..times {
            let next = carried(sum_places(total, p), dmax, arith);
            if next == total {
                break;
            }
            total = next;
        }
    }
    num(Some(total.dec), total.total())
}

/// RANGE: the greatest less the least, in fixed point when both are fixed-point, and otherwise in
/// floating point.
fn range(args: &[Arg], arith: Arith, pos: Pos) -> R<Side> {
    let fixed = args.iter().filter(|a| matches!(a.side.value, Value::Num(_))).count();
    if fixed == 0 {
        return Ok(of(Value::Float));
    }
    if fixed < args.len() {
        return unsupported("FUNCTION RANGE of fixed-point arguments with floating-point or other ones", pos);
    }
    let places: Option<Vec<Places>> = args.iter().map(places).collect();
    Ok(match places.as_deref() {
        Some([p, rest @ ..]) if rest.iter().all(|q| q == p) => {
            let r = carried(sum_places(*p, *p), p.dec, arith);
            num(Some(r.dec), r.total())
        }
        _ => num(None, 0),
    })
}
