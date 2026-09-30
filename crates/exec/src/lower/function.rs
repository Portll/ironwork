//! Intrinsic functions (lir.md §9.9), as `Machine::function` evaluates them, and what each result
//! reads as, which decides the MOVE and comparison plans it meets.

use super::data::{Side, Value};
use super::{Lower, R, push, unsupported};
use numeric::Arith;
use rt::lir::{Func, FunctionId, FunctionPlan, RefMod, TrimSide};
use syntax::ast::{Expr, FunctionCall};

impl Lower<'_> {
    pub(super) fn function(&mut self, f: &FunctionCall) -> R<(FunctionId, Side)> {
        let Some(func) = Func::named(&f.name) else { return unsupported("a FUNCTION the LIR does not name", f.pos) };
        if !f.all_subscripts.is_empty() {
            return unsupported("FUNCTION arguments with ALL subscripts", f.pos);
        }
        let pos = f.pos;
        let mut args = Vec::with_capacity(f.args.len());
        let mut sides = Vec::with_capacity(f.args.len());
        for a in &f.args {
            let (arg, side) = self.comparand(a, pos)?;
            args.push(arg);
            sides.push((side, self.scaled(a)));
        }
        let arity = match func.arity() {
            n if n.contains(&args.len()) => None,
            _ if matches!(func, Func::Min | Func::Max) => Some(self.ironwork(&format!("FUNCTION {} needs arguments", f.name))?),
            n => Some(self.ironwork(&format!("FUNCTION {} takes {n:?} arguments", f.name))?),
        };
        let again = match func {
            _ if arity.is_some() => None,
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
        Ok((id, result))
    }

    /// Whether an argument is an item with PICTURE scaling positions, whose value has digits its
    /// kind does not show.
    fn scaled(&self, e: &Expr) -> bool {
        let Expr::Operand(syntax::ast::Operand::Ref(r)) = e else { return false };
        matches!(self.layout.resolve(&r.name, &r.qualifiers, r.pos), Ok(crate::layout::Resolved::Item(i)) if self.layout.items[i].scaling > 0)
    }
}

/// What the function's value reads as: alphanumeric bytes, national units, a float, or a number
/// with its decimal places and digits where they are the same on every call (`Num(None)` where
/// they depend on the argument's text or on which argument wins). A floating-point argument makes
/// ABS, REM, MIN and MAX floating-point and INTEGER and INTEGER-PART 30 digits, 31 under
/// ARITH(EXTEND) (`float_function`).
fn result(func: Func, args: &[(Side, bool)], arith: Arith, pos: syntax::Pos) -> R<Side> {
    let num = |dec: Option<u32>, digits: u32| Side { value: Value::Num(dec), src: None, digits };
    let float = args.iter().any(|(s, _)| s.value == Value::Float);
    Ok(match func {
        Func::Abs | Func::Rem | Func::Min | Func::Max if float => Side { value: Value::Float, src: None, digits: 0 },
        Func::Integer | Func::IntegerPart if float => num(Some(0), if arith == Arith::Compat { 30 } else { 31 }),
        Func::Mod if float => num(None, 0),
        Func::Char | Func::UpperCase | Func::LowerCase | Func::Reverse | Func::CurrentDate | Func::Trim => Side { value: Value::Bytes, src: None, digits: 0 },
        Func::NationalOf => Side { value: Value::National, src: None, digits: 0 },
        Func::Random => Side { value: Value::Float, src: None, digits: 0 },
        Func::Ord => num(Some(0), 3),
        Func::Length => num(Some(0), 9),
        Func::IntegerOfDate => num(Some(0), 7),
        Func::DateOfInteger => num(Some(0), 8),
        Func::Numval | Func::NumvalC => num(None, 0),
        // The result has 31 digits, as many decimal places as the arguments have at most.
        Func::Mod | Func::Rem | Func::Integer | Func::IntegerPart | Func::Abs => {
            let decs: Option<Vec<u32>> = args.iter().map(|(s, _)| if let Value::Num(Some(d)) = s.value { Some(d) } else { None }).collect();
            match decs.and_then(|d| d.into_iter().max()) {
                Some(dec) if dec <= 31 => num(Some(dec), 31),
                _ => num(None, 0),
            }
        }
        // The winning argument's own value.
        Func::Min | Func::Max => {
            let Some(((first, _), rest)) = args.split_first() else { return Ok(num(None, 0)) };
            let same = |v: Value| rest.iter().all(|(s, _)| s.value == v);
            match first.value {
                Value::Num(_) if args.iter().all(|(s, scaled)| matches!(s.value, Value::Num(_)) && !scaled) => {
                    if same(first.value) && rest.iter().all(|(s, _)| s.digits == first.digits) { num(first.value_dec(), first.digits) } else { num(None, 0) }
                }
                Value::Num(_) if args.iter().all(|(s, _)| matches!(s.value, Value::Num(_))) => num(None, 0),
                v @ (Value::Bytes | Value::National | Value::Float) if same(v) => Side { value: v, src: None, digits: 0 },
                _ => return unsupported("FUNCTION MIN or MAX of arguments of different kinds", pos),
            }
        }
    })
}

impl Side {
    fn value_dec(&self) -> Option<u32> {
        if let Value::Num(d) = self.value { d } else { None }
    }
}
