//! Conditions, as the walker's `condition`, `class` and `compare` evaluate them (lir.md §6).

use super::value::constant;
use super::{Code, R, Vm, not_yet};
use crate::lir::{Comparand, Compare, Cond, CondId, Const, Operand, PlaceId};
use crate::oo;
use crate::storage::{Kind, Loc, Val};
use crate::store::{self, ProgramFacts};
use crate::unit::Loader;
use crate::vocab::{Figurative, Pos, RelOp};
use std::cmp::Ordering;
use std::rc::Rc;

fn holds(op: RelOp, o: Ordering) -> bool {
    match op {
        RelOp::Eq => o == Ordering::Equal,
        RelOp::Ne => o != Ordering::Equal,
        RelOp::Lt => o == Ordering::Less,
        RelOp::Le => o != Ordering::Greater,
        RelOp::Gt => o == Ordering::Greater,
        RelOp::Ge => o != Ordering::Less,
    }
}

impl<L: Loader<Rc<Code>>> Vm<'_, '_, '_, L> {
    pub(super) fn cond(&mut self, c: CondId, pos: Pos) -> R<bool> {
        let p = self.p;
        Ok(match &p.conds[c as usize] {
            Cond::Rel { a, op, b, how } => holds(*op, self.compare(a, b, *how, pos)?),
            Cond::Class { place, test } => {
                let loc = self.loc(*place)?;
                store::byte_class(&self.facts(), &self.unit.mem, loc, *test)
            }
            Cond::Sign { value, test } => {
                let v = self.comparand(value, pos)?;
                store::sign_test(v, *test, pos)?
            }
            Cond::Name { subject, values, .. } => {
                let loc = self.loc(*subject)?;
                let subject = (self.read(loc, self.pos(p.places[*subject as usize].at))?, Some(loc));
                for (low, high) in values {
                    let hit = match high {
                        None => self.compare_constant(&subject, *low, pos)? == Ordering::Equal,
                        Some(high) => self.compare_constant(&subject, *low, pos)? != Ordering::Less && self.compare_constant(&subject, *high, pos)? != Ordering::Greater,
                    };
                    if hit {
                        return Ok(true);
                    }
                }
                false
            }
            Cond::Not(inner) => !self.cond(*inner, pos)?,
            Cond::And(a, b) => self.cond(*a, pos)? && self.cond(*b, pos)?,
            Cond::Or(a, b) => self.cond(*a, pos)? || self.cond(*b, pos)?,
            Cond::Counter(t) => self.returns.frames.last().and_then(|f| f.temps.get(usize::from(*t))).is_some_and(|&n| n > 0),
            Cond::InTable { index, count } => {
                let count = i64::from(self.count(count, pos)?);
                let i = self.int_place(*index, pos)?;
                (1..=count).contains(&i)
            }
            Cond::Sql(test) => self.sql_test(*test)?,
        })
    }

    fn compare_constant(&self, subject: &(Val, Option<Loc>), c: u32, pos: Pos) -> R<Ordering> {
        let value = constant(&self.p.consts[c as usize]).map_err(|a| self.abend(a, None))?;
        Ok(store::compare(&self.facts(), &self.unit.mem, subject.clone(), (value, None), pos)?)
    }

    /// `Machine::compare`: the zoned-bytes test of each side against the other, with its locates,
    /// then the branch the plan names: a zoned integer's bytes, or both sides as they read, object
    /// references by what they identify, and anything else as `store::compare` orders it.
    pub(super) fn compare(&mut self, a: &Comparand, b: &Comparand, how: Compare, pos: Pos) -> R<Ordering> {
        let mut zoned = self.zoned_against(a, b)?;
        if how != (Compare::ZonedBytes { zoned_first: true }) {
            zoned = self.zoned_against(b, a)?;
        }
        if let Compare::ZonedBytes { zoned_first } = how {
            let (side, other) = if zoned_first { (a, b) } else { (b, a) };
            let Comparand::Operand(Operand::Load(place)) = side else { return Err(not_yet("zoned bytes of a side that is not a data item")) };
            let loc = match zoned {
                Some(loc) => loc,
                None => self.loc(*place)?,
            };
            let facts = self.facts();
            let Some(image) = store::compared_zoned_bytes(&facts, &self.unit.mem, loc) else { return Err(not_yet("zoned bytes of an item that is not a zoned integer")) };
            let other = match other {
                Comparand::Operand(Operand::Load(q)) if self.zone_sensitive(*q)? => {
                    let loc = self.loc(*q)?;
                    (Val::Bytes(Vec::new()), Some(loc))
                }
                _ => self.comparand_with_loc(other, pos)?,
            };
            return Ok(store::compare_zoned_bytes(&facts, &self.unit.mem, &image, other, zoned_first, pos)?);
        }
        let (va, la) = self.comparand_with_loc(a, pos)?;
        let (vb, lb) = self.comparand_with_loc(b, pos)?;
        let names = || (self.operand_name(a), self.operand_name(b));
        if let Some(o) = oo::compare_references(&self.unit.oo, names, (&va, la), (&vb, lb), pos)? {
            return Ok(o);
        }
        Ok(store::compare(&self.facts(), &self.unit.mem, (va, la), (vb, lb), pos)?)
    }

    fn operand_name(&self, c: &Comparand) -> String {
        match c {
            Comparand::Operand(Operand::Load(p)) => self.sym(self.p.places[*p as usize].name).to_owned(),
            _ => String::new(),
        }
    }

    /// The locates of `Machine::zoned_bytes_against(e, other)`, and `e`'s `Loc` where it reaches
    /// the zoned bytes.
    fn zoned_against(&mut self, e: &Comparand, other: &Comparand) -> R<Option<Loc>> {
        let Comparand::Operand(Operand::Load(r)) = e else { return Ok(None) };
        let nonnumeric = match other {
            Comparand::Operand(Operand::Const(c)) => match &self.p.consts[*c as usize] {
                Const::Bytes(_) | Const::All(_) | Const::Refused(_) => true,
                Const::Figurative(f) => !matches!(f, Figurative::Zero | Figurative::Null),
                Const::National(_) | Const::Number(_) => false,
            },
            Comparand::Operand(Operand::Load(o)) => {
                let kind = self.loc(*o)?.kind;
                matches!(kind, Kind::Group | Kind::Alnum { .. } | Kind::AlnumEdited { .. } | Kind::NumericEdited { .. })
            }
            _ => false,
        };
        let options = self.p.options.options;
        let zones_count = options.invdata.is_some_and(|i| !i.forcenumcmp)
            && self.zone_sensitive(*r)?
            && match other {
                Comparand::Operand(Operand::Const(c)) => matches!(self.p.consts[*c as usize], Const::Figurative(Figurative::Zero)),
                Comparand::Operand(Operand::Load(o)) => self.zone_sensitive(*o)? && self.loc(*o)?.len == self.loc(*r)?.len,
                _ => false,
            };
        if !nonnumeric && !zones_count {
            return Ok(None);
        }
        let loc = self.loc(*r)?;
        Ok(store::compared_zoned_bytes(&self.facts(), &self.unit.mem, loc).map(|_| loc))
    }

    /// `Machine::zone_sensitive`: an unsigned, unscaled zoned integer item, located.
    fn zone_sensitive(&mut self, place: PlaceId) -> R<bool> {
        let loc = self.loc(place)?;
        Ok(matches!(loc.kind, Kind::Zoned { scale: 0, signed: false, .. }) && self.facts().scaling(loc.item) == 0)
    }
}
