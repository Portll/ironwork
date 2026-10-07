//! What an operand may be: an arithmetic expression or numeric function is compared only with a
//! numeric operand, and a function's arguments are what the function takes.

use crate::Check;
use crate::layout::{Layout, Resolved};
use rt::storage::Kind;
use syntax::ast::{Expr, Figurative, FunctionCall, Literal, Operand, Ref, RefMod};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    Numeric,
    Alphabetic,
    Alphanumeric,
    National,
    Dbcs,
}

impl Class {
    fn word(self) -> &'static str {
        match self {
            Class::Numeric => "numeric",
            Class::Alphabetic => "alphabetic",
            Class::Alphanumeric => "alphanumeric",
            Class::National => "national",
            Class::Dbcs => "DBCS",
        }
    }
}

/// Functions whose argument-1 is alphabetic, alphanumeric, national or UTF-8, and whose value is
/// of its class (Language Reference SC27-8713-03, pp. 589, 627, 657, 663).
const CHARACTER_ARGUMENT: [&str; 4] = ["LOWER-CASE", "REVERSE", "TRIM", "UPPER-CASE"];

/// The argument both executors evaluate in place of the item written, reference-modified from
/// its first byte: for an unsigned integer DISPLAY item given to one of `CHARACTER_ARGUMENT`,
/// which only `--compliance extended` compiles (IWX0018-W), its digits as GnuCOBOL reads them;
/// for LENGTH of a numeric or pointer item, its bytes, which LENGTH counts (Language Reference
/// SC27-8713-03, LENGTH: "in alphanumeric character positions or bytes for all other arguments").
pub fn as_characters(layout: &Layout, f: &FunctionCall) -> Option<Expr> {
    let Some(Expr::Operand(Operand::Ref(r))) = f.args.first() else { return None };
    let Ok(Resolved::Item(i)) = layout.resolve(&r.name, &r.qualifiers, r.pos) else { return None };
    let kind = &layout.items[i].kind;
    let rewritten = match f.name.as_str() {
        "LENGTH" => matches!(
            kind,
            Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. } | Kind::Float(_) | Kind::Pointer | Kind::Index | Kind::ObjectReference | Kind::ProgramPointer
        ),
        name => CHARACTER_ARGUMENT.contains(&name) && matches!(kind, Kind::Zoned { scale: 0, signed: false, .. }),
    };
    if r.refmod.is_some() || !rewritten {
        return None;
    }
    let start = Box::new(Expr::Operand(Operand::Literal(Literal::Number("1".into()))));
    Some(Expr::Operand(Operand::Ref(Ref { refmod: Some(RefMod { start, length: None }), ..r.clone() })))
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
            self.errors.push(syntax::messages::IWC0287.at(f.pos, message));
        }
        let intrinsic = self.intrinsic(&f.name);
        if intrinsic && f.args.iter().any(|a| matches!(a, Expr::Operand(Operand::Literal(Literal::Figurative(_) | Literal::All(_))))) {
            self.errors.push(syntax::messages::IWC0141.at(f.pos, format!("FUNCTION {}: a figurative constant is an argument only inside an arithmetic expression", f.name)));
        }
        if intrinsic
            && CHARACTER_ARGUMENT.contains(&f.name.as_str())
            && let Some(first) = f.args.first()
            && self.class(first) == Some(Class::Numeric)
        {
            let argument = match first {
                Expr::Operand(Operand::Ref(r)) => r.name.clone(),
                Expr::Operand(Operand::Literal(_)) => "a numeric literal".into(),
                Expr::Operand(Operand::Function(g)) => format!("FUNCTION {}", g.name),
                _ => "an arithmetic expression".into(),
            };
            if self.extended && as_characters(self.layout, f).is_some() {
                let message = format!("a numeric argument to FUNCTION {} (GnuCOBOL; Enterprise COBOL takes an alphabetic, alphanumeric or national one): {argument}'s digits are read as its characters", f.name);
                self.errors.push(syntax::messages::IWX0018.at(f.pos, message));
            } else {
                let message = format!("FUNCTION {}: {argument} is numeric, where {} takes an alphabetic, alphanumeric or national argument", f.name, f.name);
                self.errors.push(syntax::messages::IWC0297.at(f.pos, message));
            }
        }
        if !matches!(f.name.as_str(), "MAX" | "MIN" | "ORD-MAX" | "ORD-MIN") {
            return;
        }
        for a in &f.args {
            if let Expr::Operand(Operand::Ref(r)) = a
                && let Ok(Resolved::Item(i)) = self.layout.resolve(&r.name, &r.qualifiers, r.pos)
                && matches!(self.layout.items[i].kind, Kind::Pointer | Kind::ObjectReference | Kind::ProgramPointer)
            {
                self.errors.push(syntax::messages::IWC0142.at(f.pos, format!("FUNCTION {}: {} is a pointer or object reference, where an argument is alphabetic, alphanumeric, national or numeric", f.name, r.name)));
            }
        }
        let classes: Vec<Class> = f.args.iter().filter_map(|a| self.class(a)).collect();
        let character = |c: Class| if c == Class::Alphabetic { Class::Alphanumeric } else { c };
        if let Some((first, rest)) = classes.split_first()
            && let Some(other) = rest.iter().find(|&&c| character(c) != character(*first))
        {
            let message = format!("FUNCTION {}: {} and {} arguments, where all must be of the same class", f.name, first.word(), other.word());
            self.errors.push(syntax::messages::IWC0288.at(f.pos, message));
        }
    }

    /// An arithmetic expression or a numeric function is compared only with a numeric operand, an
    /// index or ZERO (Language Reference SC27-8713-03, pp. 275-276).
    pub(crate) fn comparison(&mut self, a: &Expr, b: &Expr) {
        for (x, y) in [(a, b), (b, a)] {
            if let (Some(left), Some(right)) = (self.native_numeric(x), self.nonnumeric(y)) {
                let (first, second) = if std::ptr::eq(x, a) { (left, right) } else { (right, left) };
                self.errors.push(syntax::messages::IWC0143.at(self.at, format!("{first} compared with {second}: an arithmetic expression or a numeric function is compared only with a numeric operand")));
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
            Operand::Literal(Literal::Dbcs(_)) => Some(Class::Dbcs),
            Operand::Literal(_) | Operand::AddressOf(_) => None,
            Operand::Ref(r) => match self.layout.resolve(&r.name, &r.qualifiers, r.pos) {
                Ok(Resolved::Item(i)) => {
                    let item = &self.layout.items[i];
                    // A reference-modified item is of category alphanumeric, or national or DBCS (p. 75).
                    match item.kind {
                        Kind::National if r.refmod.is_some() => return Some(Class::National),
                        Kind::Dbcs { .. } if r.refmod.is_some() => return Some(Class::Dbcs),
                        _ if r.refmod.is_some() => return Some(Class::Alphanumeric),
                        _ => {}
                    }
                    match item.kind {
                        Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. } | Kind::Float(_) | Kind::Index => Some(Class::Numeric),
                        Kind::Alnum { .. } if item.alphabetic => Some(Class::Alphabetic),
                        Kind::Alnum { .. } | Kind::AlnumEdited { .. } | Kind::NumericEdited { .. } | Kind::Group => Some(Class::Alphanumeric),
                        Kind::National => Some(Class::National),
                        Kind::Dbcs { .. } => Some(Class::Dbcs),
                        Kind::Pointer | Kind::ObjectReference | Kind::ProgramPointer => None,
                    }
                }
                _ => None,
            },
            Operand::Function(f) => self.function_class(f),
        }
    }

    /// Whether `f` is MAX or MIN of numeric arguments, a numeric function (Language Reference
    /// SC27-8713-03, pp. 591, 599).
    pub(crate) fn numeric_max_or_min(&self, f: &FunctionCall) -> bool {
        matches!(f.name.as_str(), "MAX" | "MIN") && self.function_class(f) == Some(Class::Numeric)
    }

    /// An intrinsic function's class where its name or its first argument decides it.
    fn function_class(&self, f: &FunctionCall) -> Option<Class> {
        let name = f.name.as_str();
        if !self.intrinsic(name) {
            return None;
        }
        if !rt::intrinsic::CHARACTER_VALUED.contains(&name) {
            return Some(Class::Numeric);
        }
        match name {
            "MAX" | "MIN" => f.args.first().and_then(|a| self.class(a)),
            _ if CHARACTER_ARGUMENT.contains(&name) => f.args.first().and_then(|a| self.class(a)).map(|c| if c == Class::Numeric { Class::Alphanumeric } else { c }),
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
        Literal::Dbcs(_) => "a DBCS literal",
        Literal::Figurative(Figurative::Space) => "SPACE",
        Literal::Figurative(Figurative::HighValue) => "HIGH-VALUE",
        Literal::Figurative(Figurative::LowValue) => "LOW-VALUE",
        Literal::Figurative(Figurative::Quote) => "QUOTE",
        Literal::Figurative(Figurative::Zero | Figurative::Null) | Literal::Number(_) => return None,
        Literal::All(inner) => match &**inner {
            Literal::Figurative(_) => return nonnumeric_literal(inner),
            Literal::National(_) => "an ALL national literal",
            Literal::Dbcs(_) => "an ALL DBCS literal",
            _ => "an ALL literal",
        },
    })
}
