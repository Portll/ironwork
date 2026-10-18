//! GnuCOBOL's ANY NUMERIC parameter under `--compliance extended`: an item of the LINKAGE SECTION
//! that takes the class, digits, scale, sign and usage of the argument each CALL passes. The
//! parameter's uses are read as functions of the argument's field (assumption C510); no use that
//! would write it, or pass it on, is read.

use syntax::ast::*;
use syntax::messages::{IWR0095, IWX0120};
use syntax::{Error, Pos};

const VALUE: &str = "ARGUMENT NUMERIC";
const CLASS: &str = "ARGUMENT NUMERIC CLASS";
const SHOWN: &str = "ARGUMENT NUMERIC DISPLAY";

/// Gives each ANY NUMERIC parameter a one-character record, which only binds the argument's
/// address, and rewrites each statement that reads it to ask for the argument's field. Refuses each
/// ANY NUMERIC entry it does not read so, and each use of one it does not read.
pub(crate) fn rewrite(program: &mut Program, errors: &mut Vec<Error>) {
    let elsewhere = program.working_storage.iter().chain(&program.local_storage).chain(program.files.iter().flat_map(|f| &f.records));
    for e in elsewhere.filter(|e| e.any_numeric) {
        errors.push(refusal(e.name.as_deref(), e.pos, "only a parameter, an item of the LINKAGE SECTION, takes the class of its argument"));
    }
    if !program.linkage.iter().any(|e| e.any_numeric) {
        return;
    }
    let mut entry_using = Vec::new();
    for p in &program.paragraphs {
        crate::oo::each(&p.statements, &mut |s| {
            if let Stmt::Entry { using, .. } = s {
                entry_using.extend(using.iter().map(|u| u.name.clone()));
            }
        });
    }
    let mut parameters = Vec::new();
    for at in 0..program.linkage.len() {
        if !program.linkage[at].any_numeric {
            continue;
        }
        let e = &program.linkage[at];
        let name = e.name.clone().unwrap_or_default();
        let group = program.linkage.get(at + 1).is_some_and(|next| next.level > e.level && next.level != 88 && e.level != 77);
        let position = program.using.iter().position(|u| u.name == name);
        let why = if !matches!(e.level, 1 | 77) || e.name.is_none() {
            Some("it is not an 01 or 77 entry with a name")
        } else if group || e.picture.is_some() || e.usage.is_some() || e.any_length {
            Some("it has a PICTURE, USAGE, ANY LENGTH or items below it")
        } else if e.occurs.is_some() || e.redefines.is_some() {
            Some("it has OCCURS or REDEFINES")
        } else if program.returning.as_deref() == Some(name.as_str()) {
            Some("it is the RETURNING item")
        } else if program.function.is_some() {
            Some("it is a parameter of a user-defined function")
        } else if entry_using.contains(&name) {
            Some("an ENTRY statement names it, and the program is entered there with no field")
        } else if position.is_none() {
            Some("PROCEDURE DIVISION USING does not name it")
        } else if program.using.get(position.unwrap_or_default()).is_some_and(|u| u.by_value) {
            Some("it is received BY VALUE")
        } else {
            None
        };
        if let Some(why) = why {
            errors.push(refusal(e.name.as_deref(), e.pos, why));
            continue;
        }
        errors.push(IWX0120.at(e.pos, format!("ANY NUMERIC (GnuCOBOL; Enterprise COBOL's parameters have the class their entries give): {name} takes the class, digits, scale, sign and usage of the argument each CALL passes for it, and is read, never written")));
        parameters.push((name, position.unwrap_or_default() + 1, e.pos));
    }
    for (name, position, _) in &parameters {
        let record = program.linkage.iter_mut().find(|e| e.any_numeric && e.name.as_deref() == Some(name.as_str()));
        if let Some(record) = record {
            record.any_numeric = false;
            record.picture = Some("X".into());
        }
        let mut reader = Reader { name, position: *position };
        for p in &mut program.paragraphs {
            crate::oo::each_mut(&mut p.statements, &mut |s| reader.statement(s));
        }
    }
    for p in &program.paragraphs {
        let seen = format!("{:?}", p.statements);
        for (name, ..) in &parameters {
            if let Some(pos) = left(&seen, name) {
                errors.push(refusal(Some(name), pos.unwrap_or(p.pos), "this statement writes it, passes it on or uses it in a way ironwork does not read"));
            }
        }
    }
}

fn refusal(name: Option<&str>, pos: Pos, why: &str) -> Error {
    IWR0095.at(pos, format!("ANY NUMERIC on {}: ironwork reads it as a parameter of a 01 or 77 entry that is compared, tested, moved from, used in arithmetic or displayed, and {why}", name.unwrap_or("FILLER")))
}

/// Where the first reference to `name` that remains in `debug`, a statement's Debug text, is (the
/// outer None when there is none; the inner when the text does not give it). ADDRESS OF it, which
/// the OMITTED test makes, is no use of its field.
fn left(debug: &str, name: &str) -> Option<Option<Pos>> {
    let head = format!("Ref {{ name: {name:?}, ");
    let allowed = format!("AddressOf({head}");
    let mut from = 0;
    while let Some(found) = debug[from..].find(&head) {
        let at = from + found;
        if !debug[..at].ends_with(&allowed[..allowed.len() - head.len()]) {
            return Some(position(&debug[at..]));
        }
        from = at + head.len();
    }
    None
}

/// The first `pos: Pos { file: _, line: _, col: _ }` in `debug`.
fn position(debug: &str) -> Option<Pos> {
    let rest = &debug[debug.find("pos: Pos { file: ")? + "pos: Pos { file: ".len()..];
    let number = |text: &str, key: &str| -> Option<u32> {
        let tail = &text[text.find(key)? + key.len()..];
        tail[..tail.find([',', ' '])?].parse().ok()
    };
    let file = rest[..rest.find(',')?].parse().ok()?;
    Some(Pos { file, line: number(rest, "line: ")?, col: number(rest, "col: ")? })
}

struct Reader<'a> {
    name: &'a str,
    position: usize,
}

impl Reader<'_> {
    fn is_it(&self, r: &Ref) -> bool {
        r.name == self.name && r.qualifiers.is_empty() && r.subscripts.is_empty() && r.refmod.is_none()
    }

    fn call(&self, name: &str, pos: Pos) -> FunctionCall {
        let position = Expr::Operand(Operand::Literal(Literal::Number(self.position.to_string())));
        FunctionCall { name: name.into(), args: vec![position], modifier: None, refmod: None, all_subscripts: Vec::new(), pos }
    }

    fn value(&self, o: &mut Operand) {
        if let Operand::Ref(r) = o
            && self.is_it(r)
        {
            *o = Operand::Function(self.call(VALUE, r.pos));
        }
    }

    fn expr(&self, e: &mut Expr) {
        match e {
            Expr::Operand(o) => self.value(o),
            Expr::Neg(inner) => self.expr(inner),
            Expr::Bin(a, _, b) => {
                self.expr(a);
                self.expr(b);
            }
        }
    }

    fn is_it_expr(&self, e: &Expr) -> Option<Pos> {
        match e {
            Expr::Operand(Operand::Ref(r)) if self.is_it(r) => Some(r.pos),
            _ => None,
        }
    }

    /// Whether `e` adds the parameter alone to the target, or takes it alone from it: ADD or
    /// SUBTRACT with one operand and no GIVING, which cobc does by another rule (assumption C512).
    fn alone(&self, target: &Target, e: &Expr) -> bool {
        let Expr::Bin(a, BinOp::Add | BinOp::Sub, b) = e else { return false };
        matches!(&**a, Expr::Operand(Operand::Ref(r)) if r.name == target.r.name && r.qualifiers == target.r.qualifiers) && self.is_it_expr(b).is_some()
    }

    fn cond(&self, c: &mut Cond) {
        match c {
            Cond::Rel(a, _, b) => {
                self.expr(a);
                self.expr(b);
            }
            Cond::Class(e, class) => match (self.is_it_expr(e), &*class) {
                (Some(pos), Class::Numeric) => *c = Cond::Rel(Expr::Operand(Operand::Function(self.call(CLASS, pos))), RelOp::Eq, Expr::Operand(Operand::Literal(Literal::Number("1".into())))),
                (Some(_), Class::Positive | Class::Negative | Class::Zero) => self.expr(e),
                _ => {}
            },
            Cond::NameOrRel { subject, .. } => self.expr(subject),
            Cond::Not(inner) => self.cond(inner),
            Cond::And(a, b) | Cond::Or(a, b) => {
                self.cond(a);
                self.cond(b);
            }
            Cond::Name(_) => {}
        }
    }

    fn repeat(&self, repeat: &mut Loop) {
        match repeat {
            Loop::Times(e) => self.expr(e),
            Loop::Until { cond, .. } => self.cond(cond),
            Loop::Varying { varying, after, .. } => {
                for v in std::iter::once(&mut **varying).chain(after.iter_mut()) {
                    self.expr(&mut v.from);
                    self.expr(&mut v.by);
                    self.cond(&mut v.until);
                }
            }
            Loop::Once | Loop::Forever => {}
        }
    }

    fn statement(&mut self, s: &mut Stmt) {
        match s {
            Stmt::Move { from, .. } => self.value(from),
            Stmt::Compute { expr, .. } => self.expr(expr),
            Stmt::Arith(a) => {
                for (target, e) in &mut a.computations {
                    if !self.alone(target, e) {
                        self.expr(e);
                    }
                }
                if let Some((_, quotient, dividend)) = &mut a.remainder {
                    self.expr(quotient);
                    self.expr(dividend);
                }
            }
            Stmt::If { cond, .. } => self.cond(cond),
            Stmt::PerformInline { repeat, .. } | Stmt::PerformProc { repeat, .. } => self.repeat(repeat),
            Stmt::Evaluate { subjects, whens, .. } => {
                for subject in subjects {
                    match subject {
                        Subject::Expr(e) => self.expr(e),
                        Subject::Cond(c) => self.cond(c),
                        Subject::Bool(_) => {}
                    }
                }
                for object in whens.iter_mut().flat_map(|w| w.alternatives.iter_mut().flatten()) {
                    match object {
                        Object::Cond(c) => self.cond(c),
                        Object::Value { from, thru, .. } => {
                            self.expr(from);
                            thru.iter_mut().for_each(|t| self.expr(t));
                        }
                        Object::Any | Object::Bool(_) => {}
                    }
                }
            }
            Stmt::Display { items, .. } => {
                for item in items {
                    if let Operand::Ref(r) = item
                        && self.is_it(r)
                    {
                        *item = Operand::Function(self.call(SHOWN, r.pos));
                    }
                }
            }
            _ => {}
        }
    }
}
