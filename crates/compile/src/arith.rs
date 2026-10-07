//! Which arithmetic an expression is evaluated in, read from its operands' descriptions: its
//! dmax, and the exponents that make it floating point.

use crate::function::Udf;
use crate::layout::{Layout, Resolved};
use rt::intrinsic;
use rt::storage::literal_fixed;
use syntax::ast::{BinOp, Expr, FunctionCall, Literal, Operand};

/// The decimal places a FUNCTION operand contributes to the dmax of an expression holding it: a
/// user-defined function's RETURNING item's, and an intrinsic function's outer-dmax from its
/// arguments' descriptions (`rt::intrinsic::outer_dmax`). No argument is located, so the walker
/// and the lowering, which fixes dmax in the plan, read the same descriptions.
pub fn function_dmax(layout: &Layout, functions: &[Udf], f: &FunctionCall) -> u32 {
    if let Some(u) = functions.iter().find(|u| u.name == f.name) {
        return u.result.kind.digits_scale().map_or(0, |(_, s)| s);
    }
    let inner = f.args.iter().map(|a| argument_dmax(layout, functions, a)).max().unwrap_or(0);
    intrinsic::outer_dmax(&f.name, inner)
}

/// An argument's dmax by the Programming Guide's terms (SC27-8714-03, p. 794): an elementary
/// item's or literal's decimal places, an expression's dmax, an embedded function's outer-dmax.
fn argument_dmax(layout: &Layout, functions: &[Udf], e: &Expr) -> u32 {
    match e {
        Expr::Operand(Operand::Literal(Literal::Number(t))) => literal_fixed(t).map_or(0, |f| f.places.dec),
        Expr::Operand(Operand::Ref(r)) if r.refmod.is_none() => match layout.resolve(&r.name, &r.qualifiers, r.pos) {
            Ok(Resolved::Item(i)) => layout.items[i].kind.digits_scale().map_or(0, |(_, s)| s),
            _ => 0,
        },
        Expr::Operand(Operand::Function(g)) => function_dmax(layout, functions, g),
        Expr::Operand(_) => 0,
        Expr::Neg(inner) => argument_dmax(layout, functions, inner),
        Expr::Bin(a, BinOp::Div | BinOp::Pow, _) => argument_dmax(layout, functions, a),
        Expr::Bin(a, _, b) => argument_dmax(layout, functions, a).max(argument_dmax(layout, functions, b)),
    }
}

/// Whether an exponent has decimal places, a literal or an operand with any by `scale`: such an
/// exponent makes its expression floating point (Programming Guide SC27-8714-03, pp. 796, 800).
pub fn decimal_exponent<E>(e: &Expr, scale: &mut impl FnMut(&Operand) -> Result<u32, E>) -> Result<bool, E> {
    Ok(match e {
        Expr::Operand(Operand::Literal(Literal::Number(t))) => literal_fixed(t).is_some_and(|f| f.places.dec > 0),
        Expr::Operand(op) => scale(op)? > 0,
        Expr::Neg(inner) => decimal_exponent(inner, scale)?,
        Expr::Bin(a, _, b) => decimal_exponent(a, scale)? || decimal_exponent(b, scale)?,
    })
}

/// Whether an exponent in `e` holds a division or an exponentiation, which makes `e` floating
/// point when its dmax is above zero (Programming Guide SC27-8714-03, p. 800).
pub fn divided_exponent(e: &Expr) -> bool {
    fn quotient_or_power(e: &Expr) -> bool {
        match e {
            Expr::Operand(_) => false,
            Expr::Neg(inner) => quotient_or_power(inner),
            Expr::Bin(_, BinOp::Div | BinOp::Pow, _) => true,
            Expr::Bin(a, _, b) => quotient_or_power(a) || quotient_or_power(b),
        }
    }
    match e {
        Expr::Operand(_) => false,
        Expr::Neg(inner) => divided_exponent(inner),
        Expr::Bin(a, op, b) => divided_exponent(a) || divided_exponent(b) || (*op == BinOp::Pow && quotient_or_power(b)),
    }
}
