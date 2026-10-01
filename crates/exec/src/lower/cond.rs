//! Conditions (lir.md §6): relations, class and sign tests, condition-names, and the branch of
//! `Machine::compare` each pair of operands takes.

use super::data::{Side, Value};
use super::{Lower, LowerError, R, push};
use crate::layout::Resolved;
use numeric::Numproc;
use rt::lir::{self, AbendId, ByteClass, Comparand, Compare, CondId, Mode, SignTest};
use rt::storage::Kind;
use syntax::Pos;
use syntax::ast::{Class, Cond, Expr, Figurative, Operand, Ref, RelOp};

/// A condition as blocks test it. `Abend` is a leaf the walker abends on when it evaluates it;
/// a test with none folds into one `Cond`.
pub(super) enum Test {
    Cond(CondId),
    Abend(AbendId, Pos),
    Not(Box<Test>),
    And(Box<Test>, Box<Test>),
    Or(Box<Test>, Box<Test>),
}

impl Test {
    pub(super) fn abends(&self) -> bool {
        match self {
            Self::Cond(_) => false,
            Self::Abend(..) => true,
            Self::Not(t) => t.abends(),
            Self::And(a, b) | Self::Or(a, b) => a.abends() || b.abends(),
        }
    }

    pub(super) fn not(self) -> Self {
        Self::Not(Box::new(self))
    }
}

impl Lower<'_> {
    pub(super) fn cond(&mut self, c: lir::Cond) -> R<CondId> {
        push(&mut self.conds, c, "conditions")
    }

    /// A test with no abending leaf as one condition.
    pub(super) fn fold(&mut self, t: &Test) -> R<CondId> {
        match t {
            Test::Cond(c) => Ok(*c),
            Test::Abend(..) => Err(LowerError::Invalid("an abending test folded into a condition".into())),
            Test::Not(a) => {
                let a = self.fold(a)?;
                self.cond(lir::Cond::Not(a))
            }
            Test::And(a, b) => {
                let (a, b) = (self.fold(a)?, self.fold(b)?);
                self.cond(lir::Cond::And(a, b))
            }
            Test::Or(a, b) => {
                let (a, b) = (self.fold(a)?, self.fold(b)?);
                self.cond(lir::Cond::Or(a, b))
            }
        }
    }

    /// `Machine::condition`: `pos` is the statement's, which the walker names in a refused
    /// comparison.
    pub(super) fn test(&mut self, c: &Cond, pos: Pos) -> R<Test> {
        Ok(match c {
            Cond::Rel(a, op, b) => self.relation(a, *op, b, pos)?,
            Cond::Not(inner) => self.test(inner, pos)?.not(),
            Cond::And(a, b) => Test::And(Box::new(self.test(a, pos)?), Box::new(self.test(b, pos)?)),
            Cond::Or(a, b) => Test::Or(Box::new(self.test(a, pos)?), Box::new(self.test(b, pos)?)),
            Cond::Class(e, class) => self.class(e, *class, pos)?,
            Cond::NameOrRel { subject, op, negated, name } => match self.layout.resolve(&name.name, &name.qualifiers, name.pos) {
                Ok(Resolved::Condition(_)) => self.condition_name(name, pos)?,
                Ok(Resolved::Item(_)) if *negated => self.relation(subject, *op, &Expr::Operand(Operand::Ref(name.clone())), pos)?.not(),
                Ok(Resolved::Item(_)) => self.relation(subject, *op, &Expr::Operand(Operand::Ref(name.clone())), pos)?,
                Err(e) => Test::Abend(self.ironwork(&e.message)?, name.pos),
            },
            Cond::Name(r) => self.condition_name(r, pos)?,
        })
    }

    pub(super) fn relation(&mut self, a: &Expr, op: RelOp, b: &Expr, pos: Pos) -> R<Test> {
        let (a, x) = self.comparand(a, pos)?;
        let (b, y) = self.comparand(b, pos)?;
        let how = self.compare(&x, &y, pos)?;
        Ok(Test::Cond(self.cond(lir::Cond::Rel { a, op, b, how })?))
    }

    /// An operand keeps its location for the comparison; an expression does not.
    pub(super) fn comparand(&mut self, e: &Expr, pos: Pos) -> R<(Comparand, Side)> {
        if let Expr::Operand(op) = e {
            let lowered = self.operand(op, pos)?;
            return Ok((Comparand::Operand(lowered.operand), lowered.side));
        }
        let computed = self.computed(e, pos)?;
        let value = match computed {
            Comparand::Expr { mode: Mode::Float(_), .. } => Value::Float,
            _ => Value::Num(None),
        };
        Ok((computed, Side { value, src: None, digits: 0 }))
    }

    /// The branch of `Machine::compare` two operands take, decided by what each reads as.
    pub(super) fn compare(&mut self, x: &Side, y: &Side, pos: Pos) -> R<Compare> {
        if let (Some(Kind::Packed { .. }), Numproc::Pfd) = (x.src, self.c.options.numproc)
            && x.src == y.src
        {
            return Ok(Compare::PackedPfd);
        }
        let reference = |s: &Side| s.src == Some(Kind::ObjectReference);
        if x.value == Value::Address && y.value == Value::Address && (reference(x) || reference(y)) {
            return Ok(Compare::References);
        }
        let address = |v: Value| matches!(v, Value::Address | Value::Fig(Figurative::Null));
        if x.value == Value::Address || y.value == Value::Address {
            return Ok(if address(x.value) && address(y.value) {
                Compare::Address
            } else {
                Compare::Refused(self.ironwork("a pointer compared with something other than a pointer or NULL")?)
            });
        }
        let numeric = |v: Value| matches!(v, Value::Num(_) | Value::Float | Value::Fig(Figurative::Zero));
        Ok(match (x.value, y.value) {
            (Value::Float, _) | (_, Value::Float) if numeric(x.value) && numeric(y.value) => Compare::Float,
            (Value::Num(_), Value::Num(_)) | (Value::Num(_), Value::Fig(Figurative::Zero)) | (Value::Fig(Figurative::Zero), Value::Num(_)) => Compare::Fixed,
            (Value::National, Value::National) => Compare::National,
            _ => match alphanumeric_refusal(x, pos)?.or(alphanumeric_refusal(y, pos)?) {
                Some(message) => Compare::Refused(self.ironwork(message)?),
                None => Compare::Alphanumeric,
            },
        })
    }

    /// `Cond::Class`: NUMERIC or ALPHABETIC of a data item tests its bytes; anything else is a sign
    /// test of the value, where NUMERIC and ALPHABETIC fall through to ZERO as in the walker.
    fn class(&mut self, e: &Expr, class: Class, pos: Pos) -> R<Test> {
        if let (Class::Numeric | Class::Alphabetic, Expr::Operand(Operand::Ref(r))) = (class, e) {
            let place = self.place(r, false)?;
            let test = match (class, self.kind_of(place)) {
                (Class::Numeric, Kind::Packed { signed, .. }) => ByteClass::Packed { signed },
                (Class::Numeric, Kind::Zoned { signed, sign: None, .. }) => ByteClass::Zoned { signed },
                (Class::Numeric, _) => ByteClass::Digits,
                _ => ByteClass::Alphabetic,
            };
            return Ok(Test::Cond(self.cond(lir::Cond::Class { place, test })?));
        }
        let value = match e {
            Expr::Operand(op) => Comparand::Operand(self.operand(op, pos)?.operand),
            _ => self.computed(e, pos)?,
        };
        let test = match class {
            Class::Positive => SignTest::Positive,
            Class::Negative => SignTest::Negative,
            Class::Numeric | Class::Alphabetic | Class::Zero => SignTest::Zero,
        };
        Ok(Test::Cond(self.cond(lir::Cond::Sign { value, test })?))
    }

    /// A level-88 name, tested against its conditional variable by item index.
    fn condition_name(&mut self, r: &Ref, pos: Pos) -> R<Test> {
        let layout = self.layout;
        let index = match layout.resolve(&r.name, &r.qualifiers, r.pos) {
            Ok(Resolved::Condition(i)) => i,
            Ok(Resolved::Item(_)) => return Ok(Test::Abend(self.ironwork(&format!("{} is a data item, not a condition", r.name))?, r.pos)),
            Err(e) => return Ok(Test::Abend(self.ironwork(&e.message)?, r.pos)),
        };
        let subject = match self.conditional_variable(index, r)? {
            Ok(place) => place,
            Err((abend, at)) => return Ok(Test::Abend(abend, at)),
        };
        self.condition_values(index, subject, pos)
    }

    /// Condition-name `index` tested against its conditional variable at `subject`.
    pub(super) fn condition_values(&mut self, index: usize, subject: lir::PlaceId, pos: Pos) -> R<Test> {
        let condition = &self.layout.conditions[index];
        let kind = self.kind_of(subject);
        let x = Side { value: super::data::value_of(kind), src: Some(kind), digits: kind.digits_scale().map_or(0, |(d, _)| d) };
        let mut values = Vec::new();
        let mut hows = Vec::new();
        for (low, high) in &condition.values {
            let (low, side) = self.literal_const(low, pos)?;
            hows.push(self.compare(&x, &side, pos)?);
            let high = match high {
                Some(h) => {
                    let (h, side) = self.literal_const(h, pos)?;
                    hows.push(self.compare(&x, &side, pos)?);
                    Some(h)
                }
                None => None,
            };
            values.push((low, high));
        }
        let how = hows.first().copied().unwrap_or(Compare::Alphanumeric);
        if hows.iter().all(|&h| h == how) {
            return Ok(Test::Cond(self.cond(lir::Cond::Name { subject, values, how })?));
        }
        // Values of different categories: each compared as the walker compares it, in its order.
        let mut hows = hows.into_iter();
        let mut rel = |lower: &mut Self, op: RelOp, value| -> R<Test> {
            let how = hows.next().unwrap_or(how);
            Ok(Test::Cond(lower.cond(lir::Cond::Rel { a: Comparand::Operand(lir::Operand::Load(subject)), op, b: Comparand::Operand(lir::Operand::Const(value)), how })?))
        };
        let mut alternatives = Vec::new();
        for (low, high) in values {
            alternatives.push(match high {
                None => rel(self, RelOp::Eq, low)?,
                Some(high) => {
                    let ge = rel(self, RelOp::Ge, low)?;
                    Test::And(Box::new(ge), Box::new(rel(self, RelOp::Le, high)?))
                }
            });
        }
        let mut alternatives = alternatives.into_iter().rev();
        let last = alternatives.next().ok_or_else(|| LowerError::Invalid("a condition-name of mixed values with none".into()))?;
        Ok(alternatives.fold(last, |rest, t| Test::Or(Box::new(t), Box::new(rest))))
    }
}

/// Why `alnum_image` refuses a side, when it does.
fn alphanumeric_refusal(side: &Side, pos: Pos) -> R<Option<&'static str>> {
    Ok(match side.value {
        Value::Bytes | Value::All | Value::Fig(_) | Value::Num(Some(0)) => None,
        Value::Num(None) => return super::unsupported("an arithmetic expression compared with a non-numeric operand", pos),
        Value::National => Some("a national value cannot be moved to an alphanumeric item"),
        Value::Num(Some(_)) | Value::Float | Value::Address => Some("only an integer numeric value can be moved to an alphanumeric item"),
    })
}
