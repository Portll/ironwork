//! Conditions, as the walker's `condition`, `class` and `compare` evaluate them (lir.md §6).

use super::value::constant;
use super::{Code, R, Vm, not_yet};
use crate::lir::{Base, Comparand, Compare, Cond, CondId, Const, Item, Operand, PlaceId, Program, SenderCheck};
use crate::oo;
use crate::storage::{Kind, Loc, Val};
use crate::store::{self, ProgramFacts};
use crate::unit::Loader;
use crate::vocab::{Figurative, Pos, RelOp};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::rc::Rc;

/// The name NUMCHECK's message gives the conditional variable of each condition-name test, whose
/// place bears the condition-name's: the one name of the items of its place's storage and shape,
/// or None where items of other names share them.
pub(super) fn conditional_variables(p: &Program) -> HashMap<PlaceId, Option<String>> {
    let mut names = HashMap::new();
    for c in &p.conds {
        let Cond::Name { subject, .. } = c else { continue };
        let place = &p.places[*subject as usize];
        let base = |i: &Item| match i.linkage {
            Some(record) => Base::Linkage(record),
            None if i.local => Base::Local,
            None => Base::Program,
        };
        let mut shaped = p.items.iter().filter(|i| base(i) == place.base && i.offset == place.offset && i.size == place.len && i.kind == place.kind && i.dims.len() == place.subscripts.len());
        let name = |i: &Item| i.name.map_or("FILLER", |n| p.symbols[n as usize].as_str());
        let first = shaped.next().map(name);
        let one = first.filter(|&n| shaped.all(|i| name(i) == n));
        names.entry(*subject).or_insert(one.map(str::to_owned));
    }
    names
}

/// A condition-name's variable as its values are compared with it.
enum Variable {
    Read(Val, Loc),
    Bytes(Vec<u8>),
}

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
            Cond::Name { subject: place, values, how } => {
                let loc = self.loc(*place)?;
                if self.code.variables.get(place).is_some_and(Option::is_none) && store::numcheck_fault(&self.facts(), &self.unit.mem, loc, false).is_some() && !self.facts().numcheck_removed(loc.item, pos) {
                    return Err(not_yet("NUMCHECK of a conditional variable whose storage another item of its shape names"));
                }
                let subject = if *how == (Compare::ZonedBytes { zoned_first: true }) {
                    self.numcheck(loc, SenderCheck::Item, self.pos(p.places[*place as usize].at))?;
                    let Some(image) = store::compared_zoned_bytes(&self.facts(), &self.unit.mem, loc) else { return Err(not_yet("zoned bytes of an item that is not a zoned integer")) };
                    Variable::Bytes(image)
                } else {
                    Variable::Read(self.read_tested(*place, loc)?, loc)
                };
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

    /// Constant `c` compared with a condition-name's variable: its value as read, or, where the plan
    /// compares the variable's bytes, those bytes.
    fn compare_constant(&self, subject: &Variable, c: u32, pos: Pos) -> R<Ordering> {
        let value = constant(&self.p.consts[c as usize]).map_err(|a| self.abend(a, None))?;
        Ok(match subject {
            Variable::Read(v, loc) => store::compare(&self.facts(), &self.unit.mem, (v.clone(), Some(*loc)), (value, None), pos)?,
            Variable::Bytes(image) => store::compare_zoned_bytes(&self.facts(), &self.unit.mem, image, (value, None), true, pos)?,
        })
    }

    /// `Machine::compare`: the zoned-bytes test of each side against the other, with its locates,
    /// then the branch the plan names: a zoned integer's bytes, or both sides as they read, object
    /// references by what they identify, and anything else as `store::compare` orders it. NUMCHECK
    /// tests a data item side unless `checks_against` the other says not.
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
            if self.checks_against(other)? {
                let loc = self.loc(*place)?;
                self.numcheck(loc, SenderCheck::Item, self.pos(self.p.places[*place as usize].at))?;
            }
            let other = match other {
                Comparand::Operand(Operand::Load(q)) if self.zone_sensitive(*q)? => {
                    let loc = self.loc(*q)?;
                    self.numcheck(loc, SenderCheck::Item, self.pos(self.p.places[*q as usize].at))?;
                    (Val::Bytes(Vec::new()), Some(loc))
                }
                _ => self.comparand_against(other, side, pos)?,
            };
            return Ok(store::compare_zoned_bytes(&facts, &self.unit.mem, &image, other, zoned_first, pos)?);
        }
        let (va, la) = self.comparand_against(a, b, pos)?;
        let (vb, lb) = self.comparand_against(b, a, pos)?;
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

    /// `Machine::comparand_against`: a data item read untested where `checks_against` the other
    /// operand says not, anything else as it reads.
    fn comparand_against(&mut self, c: &Comparand, other: &Comparand, pos: Pos) -> R<(Val, Option<Loc>)> {
        match c {
            Comparand::Operand(Operand::Load(p)) if !self.checks_against(other)? => {
                let loc = self.loc(*p)?;
                Ok((self.read(loc, self.pos(self.p.places[*p as usize].at))?, Some(loc)))
            }
            _ => self.comparand_with_loc(c, pos),
        }
    }

    /// `Machine::checks_against`: NUMCHECK tests an item compared with `other` unless ZON(NOALPHNUM)
    /// spares it a nonnumeric one, which is located to find its kind.
    fn checks_against(&mut self, other: &Comparand) -> R<bool> {
        Ok(!store::noalphnum(&self.p.options.options) || !self.nonnumeric(other)?)
    }

    /// `Machine::nonnumeric`: an alphanumeric literal, a figurative constant other than ZERO and
    /// NULL, or an item of a kind `store::nonnumeric` names, located. Lowering refuses ALL ZERO and
    /// ALL NULL where this decides, as the LIR keeps them as ZERO and NULL.
    fn nonnumeric(&mut self, e: &Comparand) -> R<bool> {
        Ok(match e {
            Comparand::Operand(Operand::Const(c)) => match &self.p.consts[*c as usize] {
                Const::Bytes(_) | Const::All(_) | Const::AllNational(_) | Const::Dbcs(_) | Const::Refused(_) => true,
                Const::Figurative(f) => !matches!(f, Figurative::Zero | Figurative::Null),
                Const::National(_) | Const::Number(_) => false,
            },
            Comparand::Operand(Operand::Load(o)) => store::nonnumeric(self.loc(*o)?.kind),
            _ => false,
        })
    }

    /// The locates of `Machine::zoned_bytes_against(e, other)`, and `e`'s `Loc` where it reaches
    /// the zoned bytes.
    fn zoned_against(&mut self, e: &Comparand, other: &Comparand) -> R<Option<Loc>> {
        let Comparand::Operand(Operand::Load(r)) = e else { return Ok(None) };
        let nonnumeric = self.nonnumeric(other)?;
        let options = self.p.options.options;
        let zones_count = (options.zones_compared_with_zero() || options.zones_compared_between_items())
            && self.zone_sensitive(*r)?
            && match other {
                Comparand::Operand(Operand::Const(c)) => options.zones_compared_with_zero() && constant(&self.p.consts[*c as usize]).is_ok_and(|v| store::zero(&v)),
                Comparand::Operand(Operand::Load(o)) => options.zones_compared_between_items() && self.zone_sensitive(*o)? && self.loc(*o)?.len == self.loc(*r)?.len,
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
