//! Places to `Loc`s, and the integers subscripts, bounds and counts take, in the walker's order of
//! locates, reads and checks (lir.md §5.4, §7.5).

use super::{Code, R, Vm, not_yet};
use crate::abend::Abend;
use crate::arith;
use crate::fixed::align;
use crate::lir::{Base, Count, Expr, IntExpr, Odo, Operand, Place, PlaceId};
use crate::loc;
use crate::oo;
use crate::storage::{Kind, Loc};
use crate::unit::{Loader, RETURN_CODE};
use crate::vocab::Pos;
use numeric::precision::Fixed;
use std::rc::Rc;

/// A place that cannot abend: its address is its base's plus a constant.
pub(super) fn is_static(place: &Place) -> bool {
    matches!(place.base, Base::Program | Base::Local | Base::ReturnCode) && place.moved.is_empty() && place.subscripts.is_empty() && place.odo.is_empty() && place.refmod.is_none()
}

fn scale(kind: Kind) -> u32 {
    kind.digits_scale().map_or(0, |(_, s)| s)
}

/// `Machine::integer`'s whole part, refused past 64 bits.
pub(super) fn whole(v: &Fixed, pos: Pos) -> Result<i64, Abend> {
    let m = align(v, 0, false).and_then(|m| m.to_u128()).and_then(|m| i64::try_from(m).ok());
    let m = m.ok_or_else(|| Abend::ironwork("an integer operand beyond 64 bits", pos))?;
    Ok(if v.negative { -m } else { m })
}

impl<'p, L: Loader<Rc<Code>>> Vm<'p, '_, '_, L> {
    pub(super) fn loc(&mut self, place: PlaceId) -> R<Loc> {
        self.loc_with(place, &[])
    }

    /// `Machine::locate_written`: a receiver the op only writes, whose old bytes are not read.
    pub(super) fn loc_written(&mut self, place: PlaceId) -> R<Loc> {
        let was = self.unit.writing(true);
        let loc = self.loc(place);
        self.unit.writing(was);
        loc
    }

    /// `place` with each subscript `fixed` names set to its constant, as the walker writes an ALL
    /// subscript as a literal for each element.
    pub(super) fn loc_with(&mut self, place: PlaceId, fixed: &[(u32, i64)]) -> R<Loc> {
        self.locating += 1;
        let loc = self.evaluate(place, fixed);
        self.locating -= 1;
        loc
    }

    fn evaluate(&mut self, id: PlaceId, fixed: &[(u32, i64)]) -> R<Loc> {
        let place = &self.p.places[id as usize];
        let pos = self.pos(place.at);
        let name = self.sym(place.name);
        let base = match place.base {
            Base::Program => self.base,
            Base::Local => self.local_base,
            Base::Linkage(record) => loc::linkage_base(self.linkage[record as usize], name, pos)?,
            Base::ReturnCode => RETURN_CODE,
            Base::Eib => return Err(not_yet("EXEC CICS")),
            Base::SelfRef | Base::JniEnv => {
                let loc = match place.base {
                    Base::SelfRef => oo::self_reference(self.unit, self.method, pos)?,
                    _ => Loc { offset: oo::jni_environment(self.unit, pos)?, len: 4, kind: Kind::Pointer, item: usize::MAX },
                };
                self.unit.taint_read(loc);
                return Ok(loc);
            }
            Base::Xml(register) => return self.xml_register(register, id, pos),
        };
        let mut offset = (base + place.offset as usize) as i64 - self.unused(&place.moved, pos)?;
        for (k, s) in place.subscripts.iter().enumerate() {
            let value = match fixed.iter().find(|&&(at, _)| at as usize == k) {
                Some(&(_, v)) => v,
                None => self.int(&s.value, pos)?,
            };
            offset += loc::subscript(value, s.stride, s.check, name, pos)?;
        }
        let mut len = i64::from(place.len);
        for odo in &place.odo {
            let current = self.occurrences(odo, pos)?;
            len = loc::odo_len(len, odo.max, current, odo.element);
        }
        if let Some(rm) = &place.refmod {
            let start = self.int(&rm.start, pos)?;
            let length = match &rm.length {
                Some(l) => Some(self.int(l, pos)?),
                None => None,
            };
            let unit = if matches!(place.kind, Kind::National | Kind::Dbcs { .. }) { 2 } else { 1 };
            let (from, length) = loc::refmod(len / unit, start, length, rm.check, name, pos)?;
            offset += from * unit;
            len = length * unit;
        }
        let (offset, len) = loc::within(offset, len, self.unit.mem.len(), name, pos)?;
        let loc = Loc { offset, len, kind: place.kind, item: id as usize };
        self.unit.taint_read(loc);
        Ok(loc)
    }

    /// `Machine::integer`: the dmax pass's locates, then the value, its whole part.
    pub(super) fn int(&mut self, e: &IntExpr, pos: Pos) -> R<i64> {
        match e {
            IntExpr::Const(n) => Ok(*n),
            IntExpr::Item(p) => self.int_place(*p, pos),
            IntExpr::Fixed { expr, dmax, prepass } => {
                for &q in prepass {
                    self.loc(q)?;
                }
                let v = self.eval_fixed(*expr, *dmax, pos)?;
                Ok(whole(&v, pos)?)
            }
            IntExpr::Walk(k) => match self.markup.walk.get(usize::from(*k)) {
                Some(&s) => Ok(i64::from(s)),
                None => Err(not_yet("a JSON walk subscript outside the walk")),
            },
        }
    }

    /// `Machine::integer` of a data item: located for its dmax, then located and read.
    pub(super) fn int_place(&mut self, p: PlaceId, pos: Pos) -> R<i64> {
        let place = &self.p.places[p as usize];
        if !is_static(place) {
            self.loc(p)?;
        }
        let val = self.value(Operand::Load(p))?;
        let v = arith::fixed_operand(val, scale(place.kind), pos)?;
        Ok(whole(&v, pos)?)
    }

    /// The bytes these OCCURS DEPENDING ON tables' occurrences past their current counts take,
    /// which an item after them in its record is moved back by, counted in turn.
    pub(super) fn unused(&mut self, tables: &[Odo], pos: Pos) -> R<i64> {
        let mut unused = 0;
        for odo in tables {
            let current = self.occurrences(odo, pos)?;
            unused += loc::unused(odo.max, current, odo.element);
        }
        Ok(unused)
    }

    /// An OCCURS DEPENDING ON table's current count, kept within its maximum.
    pub(super) fn occurrences(&mut self, odo: &Odo, pos: Pos) -> R<u32> {
        let count = self.int(&odo.object, pos)?;
        Ok(loc::occurrences(count, odo.max, odo.check, self.int_name(&odo.object), pos)?)
    }

    pub(super) fn count(&mut self, count: &Count, pos: Pos) -> R<u32> {
        match count {
            Count::Fixed(n) => Ok(*n),
            Count::Odo(odo) => self.occurrences(odo, pos),
            Count::Temp(t) => {
                let held = self.returns.frames.last().and_then(|f| f.temps.get(usize::from(*t))).and_then(|&n| u32::try_from(n).ok());
                held.ok_or_else(|| not_yet("a SEARCH count read before SetCount held it"))
            }
        }
    }

    /// The data item an integer reads, as messages name it.
    fn int_name(&self, e: &IntExpr) -> &'p str {
        let place = match e {
            IntExpr::Item(p) => Some(*p),
            IntExpr::Fixed { expr, .. } => match self.p.exprs[*expr as usize] {
                Expr::Operand(Operand::Load(p)) => Some(p),
                _ => None,
            },
            _ => None,
        };
        place.map_or("", |p| self.sym(self.p.places[p as usize].name))
    }
}
