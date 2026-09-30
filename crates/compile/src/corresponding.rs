//! MOVE, ADD and SUBTRACT CORRESPONDING, expanded against the layout into the statements they
//! stand for: [`numeric::assumptions::CORRESPONDING_PAIRS`] and
//! [`numeric::assumptions::CORRESPONDING_CHOICES`].

use crate::layout::{Kind, Layout, Resolved};
use syntax::ast::*;
use syntax::{Error, Pos};

/// Replaces each CORRESPONDING statement with a MOVE per pair of corresponding items, or with one
/// ADD or SUBTRACT that has a computation per pair and the statement's SIZE ERROR phrases.
pub fn expand(program: &mut Program, layout: &Layout, errors: &mut Vec<Error>) {
    let alphabetic: Vec<Pos> = program
        .working_storage
        .iter()
        .chain(&program.local_storage)
        .chain(&program.linkage)
        .chain(program.files.iter().flat_map(|f| &f.records))
        .filter(|e| e.picture.as_deref().is_some_and(is_alphabetic))
        .map(|e| e.pos)
        .collect();
    let context = Context { layout, alphabetic: &alphabetic };
    for p in &mut program.paragraphs {
        context.expand_in(&mut p.statements, errors);
    }
}

/// A PICTURE of the symbol A alone, repeated or with a count.
fn is_alphabetic(picture: &str) -> bool {
    let mut outside = picture.split(['(', ')']).step_by(2);
    outside.all(|s| s.chars().all(|c| c.eq_ignore_ascii_case(&'A'))) && picture.contains(['A', 'a'])
}

struct Context<'a> {
    layout: &'a Layout,
    /// Where each item of category alphabetic is declared.
    alphabetic: &'a [Pos],
}

/// The categories of IBM's table of valid and invalid elementary moves that ironwork's items fall in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Category {
    Alphabetic,
    Alphanumeric,
    AlphanumericEdited,
    Integer,
    Noninteger,
    NumericEdited,
    Float,
    National,
}

impl Category {
    fn numeric(self) -> bool {
        matches!(self, Self::Integer | Self::Noninteger | Self::NumericEdited | Self::Float)
    }

    /// Whether the table allows an elementary move from `self` to `to`.
    fn moves_to(self, to: Category) -> bool {
        match self {
            Self::Alphanumeric => true,
            Self::Alphabetic | Self::AlphanumericEdited => !to.numeric(),
            Self::Integer | Self::NumericEdited => to != Self::Alphabetic,
            Self::Noninteger | Self::Float => to.numeric(),
            Self::National => to.numeric() || to == Self::National,
        }
    }
}

impl Context<'_> {
    fn expand_in(&self, stmts: &mut Vec<Stmt>, errors: &mut Vec<Error>) {
        let mut k = 0;
        while k < stmts.len() {
            if let Stmt::Corresponding(c) = &stmts[k] {
                let expansion = self.statements(c, errors);
                stmts.splice(k..=k, expansion);
                continue;
            }
            for body in crate::oo::bodies_mut(&mut stmts[k]) {
                self.expand_in(body, errors);
            }
            k += 1;
        }
    }

    fn statements(&self, c: &Corresponding, errors: &mut Vec<Error>) -> Vec<Stmt> {
        let verb = match c.verb {
            CorrespondingVerb::Move => "MOVE",
            CorrespondingVerb::Add => "ADD",
            CorrespondingVerb::Subtract => "SUBTRACT",
        };
        let (Some(from), Some(to)) = (self.group(verb, &c.from, errors), self.group(verb, &c.to, errors)) else { return Vec::new() };
        let mut pairs = Vec::new();
        self.pairs(c.verb, from, to, &mut pairs);
        let mut refs = Vec::with_capacity(pairs.len());
        for (a, b) in pairs {
            match (self.reference(a, from, &c.from, c.pos), self.reference(b, to, &c.to, c.pos)) {
                (Ok(ra), Ok(rb)) => refs.push((ra, rb)),
                (Err(e), _) | (_, Err(e)) => errors.push(e),
            }
        }
        let (arith, op) = match c.verb {
            CorrespondingVerb::Move => {
                return refs.into_iter().map(|(ra, rb)| Stmt::Move { from: Operand::Ref(ra), to: vec![rb], pos: c.pos }).collect();
            }
            CorrespondingVerb::Add => (ArithVerb::Add, BinOp::Add),
            CorrespondingVerb::Subtract => (ArithVerb::Subtract, BinOp::Sub),
        };
        if refs.is_empty() && c.size_error.is_none() {
            return Vec::new();
        }
        let computations = refs
            .into_iter()
            .map(|(ra, rb)| {
                let current = Expr::Operand(Operand::Ref(rb.clone()));
                (Target { r: rb, rounded: c.rounded }, Expr::Bin(Box::new(current), op, Box::new(Expr::Operand(Operand::Ref(ra)))))
            })
            .collect();
        vec![Stmt::Arith(Box::new(Arith { verb: arith, computations, remainder: None, size_error: c.size_error.clone(), pos: c.pos }))]
    }

    /// The group item `r` names, when it is one CORRESPONDING may name.
    fn group(&self, verb: &str, r: &Ref, errors: &mut Vec<Error>) -> Option<usize> {
        let why = match self.layout.resolve(&r.name, &r.qualifiers, r.pos) {
            Err(e) => {
                errors.push(e);
                return None;
            }
            Ok(Resolved::Condition(_)) => "a condition-name, not a group item",
            Ok(Resolved::Item(_)) if r.refmod.is_some() => "reference-modified",
            Ok(Resolved::Item(i)) if self.layout.items[i].level == 66 => "a level-66 item, not a group item",
            Ok(Resolved::Item(i)) if self.layout.items[i].kind != Kind::Group => "not a group item",
            Ok(Resolved::Item(i)) => return Some(i),
        };
        errors.push(Error::at(r.pos, format!("{verb} CORRESPONDING {}: {why}", r.name)));
        None
    }

    /// The pairs of corresponding items under groups `from` and `to`, in the order of `from`'s
    /// entries: same-named groups are searched in turn, and other same-named items are a pair when
    /// the verb takes them.
    fn pairs(&self, verb: CorrespondingVerb, from: usize, to: usize, out: &mut Vec<(usize, usize)>) {
        let items = &self.layout.items;
        for a in self.subordinates(from) {
            let Some(b) = self.subordinates(to).find(|&b| items[b].name == items[a].name) else { continue };
            let (ka, kb) = (items[a].kind, items[b].kind);
            if ka == Kind::Group && kb == Kind::Group {
                self.pairs(verb, a, b, out);
                continue;
            }
            let corresponds = match verb {
                CorrespondingVerb::Move => ka == Kind::Group || kb == Kind::Group || self.category(a).moves_to(self.category(b)),
                CorrespondingVerb::Add | CorrespondingVerb::Subtract => ka.is_numeric() && kb.is_numeric(),
            };
            if corresponds {
                out.push((a, b));
            }
        }
    }

    /// The items directly under `group` that CORRESPONDING considers: named, and described without
    /// RENAMES, REDEFINES, OCCURS or a USAGE of INDEX, POINTER, FUNCTION-POINTER,
    /// PROCEDURE-POINTER or OBJECT REFERENCE.
    fn subordinates(&self, group: usize) -> impl Iterator<Item = usize> + '_ {
        self.layout.items[group].children.iter().copied().filter(|&c| {
            let it = &self.layout.items[c];
            it.name.is_some()
                && it.level != 66
                && !it.table
                && it.redefines.is_none()
                && !matches!(it.kind, Kind::Index | Kind::Pointer | Kind::ProgramPointer | Kind::ObjectReference)
        })
    }

    fn category(&self, i: usize) -> Category {
        let it = &self.layout.items[i];
        match it.kind {
            Kind::Alnum { .. } if self.alphabetic.contains(&it.pos) => Category::Alphabetic,
            Kind::AlnumEdited { .. } => Category::AlphanumericEdited,
            Kind::National => Category::National,
            Kind::NumericEdited { .. } => Category::NumericEdited,
            Kind::Float(_) => Category::Float,
            Kind::Zoned { scale, .. } | Kind::Packed { scale, .. } | Kind::Binary { scale, .. } if scale > 0 => Category::Noninteger,
            Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. } => Category::Integer,
            _ => Category::Alphanumeric,
        }
    }

    /// Item `i` under `group`, named by its ancestors up to the group, then as the statement named
    /// the group, with the group's subscripts.
    fn reference(&self, i: usize, group: usize, group_ref: &Ref, pos: Pos) -> Result<Ref, Error> {
        let items = &self.layout.items;
        let mut qualifiers = Vec::new();
        let mut at = items[i].parent;
        while let Some(p) = at.filter(|&p| p != group) {
            qualifiers.extend(items[p].name.clone());
            at = items[p].parent;
        }
        qualifiers.push(group_ref.name.clone());
        qualifiers.extend(group_ref.qualifiers.iter().cloned());
        let name = items[i].name.clone().unwrap_or_default();
        match self.layout.resolve(&name, &qualifiers, pos) {
            Ok(Resolved::Item(j)) if j == i => Ok(Ref { name, qualifiers, subscripts: group_ref.subscripts.clone(), refmod: None, pos }),
            _ => Err(Error::at(pos, format!("CORRESPONDING {}: {name} in it cannot be named uniquely", group_ref.name))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_of_a_alone_is_alphabetic() {
        assert!(is_alphabetic("A(4)") && is_alphabetic("AAA") && is_alphabetic("A(2)A"));
        assert!(!is_alphabetic("X(4)") && !is_alphabetic("A9") && !is_alphabetic("AB") && !is_alphabetic("9(2)"));
    }

    #[test]
    fn the_move_table_rows() {
        use Category::*;
        let all = [Alphabetic, Alphanumeric, AlphanumericEdited, Integer, Noninteger, NumericEdited, Float, National];
        let row = |from: Category| all.map(|to| from.moves_to(to));
        let (y, n) = (true, false);
        assert_eq!(row(Alphabetic), [y, y, y, n, n, n, n, y]);
        assert_eq!(row(Alphanumeric), [y, y, y, y, y, y, y, y]);
        assert_eq!(row(AlphanumericEdited), [y, y, y, n, n, n, n, y]);
        assert_eq!(row(Integer), [n, y, y, y, y, y, y, y]);
        assert_eq!(row(Noninteger), [n, n, n, y, y, y, y, n]);
        assert_eq!(row(NumericEdited), [n, y, y, y, y, y, y, y]);
        assert_eq!(row(Float), [n, n, n, y, y, y, y, n]);
        assert_eq!(row(National), [n, n, n, y, y, y, y, y]);
    }
}
