//! What an operand may be: an arithmetic expression or numeric function is compared only with a
//! numeric operand, and a function's arguments are what the function takes.

use crate::Check;
use crate::layout::Resolved;
use rt::storage::Kind;
use syntax::Error;
use syntax::ast::{Expr, Figurative, FunctionCall, Literal, Operand};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    Numeric,
    Alphabetic,
    Alphanumeric,
    National,
}

impl Class {
    fn word(self) -> &'static str {
        match self {
            Class::Numeric => "numeric",
            Class::Alphabetic => "alphabetic",
            Class::Alphanumeric => "alphanumeric",
            Class::National => "national",
        }
    }
}

impl Check<'_> {
    /// ALL subscripts stand for arguments only where one may be repeated (Language Reference
    /// SC27-8713-03, p. 501), and a figurative constant is an intrinsic function's argument only
    /// inside an arithmetic expression (p. 16); `function::check_invocation` has a user-defined
    /// function's rule. The arguments of MAX, MIN, ORD-MAX and ORD-MIN are alphabetic,
    /// alphanumeric, national or numeric, all of one class, alphabetic and alphanumeric together
    /// aside (pp. 591, 599, 613, 615).
    pub(crate) fn function_arguments(&mut self, f: &FunctionCall) {
        if !f.all_subscripts.is_empty()
            && let Some(func) = rt::lir::Func::named(&f.name)
            && *func.arity().end() != usize::MAX
        {
            let (least, most) = (*func.arity().start(), *func.arity().end());
            let takes = match most - least {
                0 => format!("{least}"),
                1 => format!("{least} or {most}"),
                _ => format!("{least} to {most}"),
            };
            let message = format!("FUNCTION {}: an ALL subscript stands for a varying number of arguments, and {} takes {takes}", f.name, f.name);
            self.errors.push(Error::at(f.pos, message));
        }
        let intrinsic = crate::FUNCTIONS.contains(&f.name.as_str()) || rt::intrinsic::FUNCTIONS.contains(&f.name.as_str());
        if intrinsic && f.args.iter().any(|a| matches!(a, Expr::Operand(Operand::Literal(Literal::Figurative(_) | Literal::All(_))))) {
            self.errors.push(Error::at(f.pos, format!("FUNCTION {}: a figurative constant is an argument only inside an arithmetic expression", f.name)));
        }
        if !matches!(f.name.as_str(), "MAX" | "MIN" | "ORD-MAX" | "ORD-MIN") {
            return;
        }
        for a in &f.args {
            if let Expr::Operand(Operand::Ref(r)) = a
                && let Ok(Resolved::Item(i)) = self.layout.resolve(&r.name, &r.qualifiers, r.pos)
                && matches!(self.layout.items[i].kind, Kind::Pointer | Kind::ObjectReference | Kind::ProgramPointer)
            {
                self.errors.push(Error::at(f.pos, format!("FUNCTION {}: {} is a pointer or object reference, where an argument is alphabetic, alphanumeric, national or numeric", f.name, r.name)));
            }
        }
        let classes: Vec<Class> = f.args.iter().filter_map(|a| self.class(a)).collect();
        let character = |c: Class| if c == Class::Alphabetic { Class::Alphanumeric } else { c };
        if let Some((first, rest)) = classes.split_first()
            && let Some(other) = rest.iter().find(|&&c| character(c) != character(*first))
        {
            let message = format!("FUNCTION {}: {} and {} arguments, where all must be of the same class", f.name, first.word(), other.word());
            self.errors.push(Error::at(f.pos, message));
        }
    }

    /// An arithmetic expression or a numeric function is compared only with a numeric operand, an
    /// index or ZERO (Language Reference SC27-8713-03, pp. 275-276).
    pub(crate) fn comparison(&mut self, a: &Expr, b: &Expr) {
        for (x, y) in [(a, b), (b, a)] {
            if let (Some(left), Some(right)) = (self.native_numeric(x), self.nonnumeric(y)) {
                let (first, second) = if std::ptr::eq(x, a) { (left, right) } else { (right, left) };
                self.errors.push(Error::at(self.at, format!("{first} compared with {second}: an arithmetic expression or a numeric function is compared only with a numeric operand")));
                return;
            }
        }
    }

    /// How a message names `e` when it is an arithmetic expression or a numeric function.
    fn native_numeric(&self, e: &Expr) -> Option<String> {
        match e {
            Expr::Bin(..) | Expr::Neg(_) => Some("an arithmetic expression".into()),
            Expr::Operand(Operand::Function(f)) if self.function_class(f) == Some(Class::Numeric) => Some(format!("FUNCTION {}", f.name)),
            _ => None,
        }
    }

    /// How a message names `e` when it is an operand no numeric comparison takes.
    fn nonnumeric(&self, e: &Expr) -> Option<String> {
        let Expr::Operand(op) = e else { return None };
        match op {
            Operand::Literal(l) => nonnumeric_literal(l).map(str::to_owned),
            Operand::Ref(_) if matches!(self.class(e), Some(Class::Numeric) | None) => None,
            Operand::Ref(r) => Some(r.name.clone()),
            Operand::Function(f) if matches!(self.function_class(f), Some(Class::Alphabetic | Class::Alphanumeric | Class::National)) => Some(format!("FUNCTION {}", f.name)),
            _ => None,
        }
    }

    fn class(&self, e: &Expr) -> Option<Class> {
        let Expr::Operand(op) = e else { return Some(Class::Numeric) };
        match op {
            Operand::Literal(Literal::Number(_)) | Operand::LengthOf(_) => Some(Class::Numeric),
            Operand::Literal(Literal::Alnum(_) | Literal::Hex(_)) => Some(Class::Alphanumeric),
            Operand::Literal(Literal::National(_)) => Some(Class::National),
            Operand::Literal(_) | Operand::AddressOf(_) => None,
            Operand::Ref(r) => match self.layout.resolve(&r.name, &r.qualifiers, r.pos) {
                Ok(Resolved::Item(i)) => {
                    let item = &self.layout.items[i];
                    // A reference-modified item is of category alphanumeric, or national (p. 76).
                    match item.kind {
                        Kind::National if r.refmod.is_some() => return Some(Class::National),
                        _ if r.refmod.is_some() => return Some(Class::Alphanumeric),
                        _ => {}
                    }
                    match item.kind {
                        Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. } | Kind::Float(_) | Kind::Index => Some(Class::Numeric),
                        Kind::Alnum { .. } if item.alphabetic => Some(Class::Alphabetic),
                        Kind::Alnum { .. } | Kind::AlnumEdited { .. } | Kind::NumericEdited { .. } | Kind::Group => Some(Class::Alphanumeric),
                        Kind::National => Some(Class::National),
                        Kind::Pointer | Kind::ObjectReference | Kind::ProgramPointer => None,
                    }
                }
                _ => None,
            },
            Operand::Function(f) => self.function_class(f),
        }
    }

    /// An intrinsic function's class where its name or its first argument decides it.
    fn function_class(&self, f: &FunctionCall) -> Option<Class> {
        let name = f.name.as_str();
        let intrinsic = crate::FUNCTIONS.contains(&name) || rt::intrinsic::FUNCTIONS.contains(&name);
        if !intrinsic {
            return None;
        }
        if !rt::intrinsic::CHARACTER_VALUED.contains(&name) {
            return Some(Class::Numeric);
        }
        match name {
            "MAX" | "MIN" | "UPPER-CASE" | "LOWER-CASE" | "REVERSE" | "TRIM" => f.args.first().and_then(|a| self.class(a)),
            "NATIONAL-OF" => Some(Class::National),
            "CONTENT-OF" | "USUBSTR" => None,
            _ => Some(Class::Alphanumeric),
        }
    }
}

/// How a message names a literal no numeric comparison takes: anything but a number, ZERO and NULL.
fn nonnumeric_literal(l: &Literal) -> Option<&'static str> {
    Some(match l {
        Literal::Alnum(_) | Literal::Hex(_) => "an alphanumeric literal",
        Literal::National(_) => "a national literal",
        Literal::Figurative(Figurative::Space) => "SPACE",
        Literal::Figurative(Figurative::HighValue) => "HIGH-VALUE",
        Literal::Figurative(Figurative::LowValue) => "LOW-VALUE",
        Literal::Figurative(Figurative::Quote) => "QUOTE",
        Literal::Figurative(Figurative::Zero | Figurative::Null) | Literal::Number(_) => return None,
        Literal::All(inner) => match &**inner {
            Literal::Figurative(_) => return nonnumeric_literal(inner),
            Literal::National(_) => "an ALL national literal",
            _ => "an ALL literal",
        },
    })
}
