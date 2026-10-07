//! What the walker and code generation both read from a program as written: NUMVAL-C's default
//! currency sign, SEARCH ALL's key conditions and DISPLAY UPON CONSOLE.

use syntax::ast::{Cond, CurrencySign, Expr, Operand, RelOp, Upon};

/// The cs of NUMVAL-C and TEST-NUMVAL-C without argument-2 (assumption C102).
pub fn numval_currency(signs: &[CurrencySign]) -> String {
    match signs {
        [only] => only.value.clone(),
        _ => "$".to_owned(),
    }
}

pub fn flatten_and<'c>(cond: &'c Cond, out: &mut Vec<&'c Cond>) {
    match cond {
        Cond::And(a, b) => {
            flatten_and(a, out);
            flatten_and(b, out);
        }
        other => out.push(other),
    }
}

/// In a SEARCH ALL condition, the key item and the value it must equal.
pub fn key_term<'c>(terms: &[&'c Cond], key: &str) -> Option<(&'c Expr, &'c Expr)> {
    let is_key = |e: &Expr| matches!(e, Expr::Operand(Operand::Ref(r)) if r.name == key);
    terms.iter().find_map(|t| match t {
        Cond::Rel(a, RelOp::Eq, b) if is_key(a) => Some((a, b)),
        Cond::Rel(a, RelOp::Eq, b) if is_key(b) => Some((b, a)),
        _ => None,
    })
}

/// Whether DISPLAY writes to the console, whose national data is converted (Language Reference
/// SC27-8713-03, p. 333).
pub fn upon_console(upon: Option<&Upon>) -> bool {
    upon.is_some_and(|u| u.device == "CONSOLE")
}
