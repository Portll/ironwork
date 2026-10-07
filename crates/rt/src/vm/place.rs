//! Places to `Loc`s, and the integers subscripts, bounds and counts take, in the walker's order of
//! locates, reads and checks (lir.md §5.4, §7.5).

use super::value::Number;
use super::{Code, R, Vm, not_yet};
use crate::abend::Abend;
use crate::arith;
use crate::fixed::{align, places_of};
use crate::lir::{Base, Count, Expr, ExprId, IntExpr, Odo, Operand, Place, PlaceId, Program, SenderCheck};
use crate::loc;
use crate::oo;
use crate::storage::{Kind, Loc};
use crate::store;
use crate::unit::{Loader, RETURN_CODE};
use crate::vocab::Pos;
use numeric::precision::Fixed;
use std::rc::Rc;

/// A place whose `Loc` has the place's own kind: one in the program's storage, LOCAL-STORAGE, a
/// LINKAGE record or RETURN-CODE.
pub(super) fn plain(place: &Place) -> bool {
    matches!(place.base, Base::Program | Base::Local | Base::Linkage(_) | Base::ReturnCode)
}

/// A place that cannot abend: its address is its base's plus a constant.
pub(super) fn is_static(place: &Place) -> bool {
    matches!(place.base, Base::Program | Base::Local | Base::ReturnCode) && place.moved.is_empty() && place.subscripts.is_empty() && place.odo.is_empty() && place.refmod.is_none()
}

#[inline]
fn scale(kind: Kind) -> u32 {
    kind.digits_scale().map_or(0, |(_, s)| s)
}

/// `Machine::integer`'s whole part, refused past 64 bits.
pub(super) fn whole(v: &Fixed, pos: Pos) -> Result<i64, Abend> {
    let m = align(v, 0, false).and_then(|m| m.to_u128()).and_then(|m| i64::try_from(m).ok());
    let m = m.ok_or_else(|| Abend::ironwork("an integer operand beyond 64 bits", pos))?;
    Ok(if v.negative { -m } else { m })
}

/// Whether each place locates by reading storage alone: its base is the program's, LOCAL-STORAGE,
/// a LINKAGE record or RETURN-CODE, and its subscripts, OCCURS DEPENDING ON objects and reference
/// modification are integers, literals and expressions over such places. Located twice with no
/// write between, such a place gives the same `Loc`, or the same abend.
pub(super) fn pure_places(p: &Program) -> Vec<bool> {
    let mut pure = vec![None; p.places.len()];
    for id in 0..p.places.len() {
        pure_place(p, id as PlaceId, &mut pure);
    }
    pure.into_iter().map(|known| known == Some(true)).collect()
}

fn pure_place(p: &Program, id: PlaceId, known: &mut [Option<bool>]) -> bool {
    match known.get(id as usize) {
        Some(Some(pure)) => return *pure,
        Some(None) => {}
        None => return false,
    }
    known[id as usize] = Some(false);
    let place = &p.places[id as usize];
    let int = |e: &IntExpr, known: &mut [Option<bool>]| pure_int(p, e, known);
    let pure = plain(place)
        && place.moved.iter().chain(&place.odo).all(|odo| int(&odo.object, known))
        && place.subscripts.iter().all(|s| int(&s.value, known))
        && place.refmod.as_ref().is_none_or(|rm| int(&rm.start, known) && rm.length.as_ref().is_none_or(|l| int(l, known)));
    known[id as usize] = Some(pure);
    pure
}

fn pure_int(p: &Program, e: &IntExpr, known: &mut [Option<bool>]) -> bool {
    match e {
        IntExpr::Const(_) => true,
        IntExpr::Item(q) => pure_place(p, *q, known),
        IntExpr::Fixed { expr, prepass, .. } => prepass.iter().all(|&q| pure_place(p, q, known)) && pure_expr(p, *expr, known),
        IntExpr::Walk(_) => false,
    }
}

fn pure_expr(p: &Program, e: ExprId, known: &mut [Option<bool>]) -> bool {
    match &p.exprs[e as usize] {
        Expr::Operand(Operand::Const(_)) => true,
        Expr::Operand(Operand::Load(q) | Operand::LengthOf(q)) => pure_place(p, *q, known),
        Expr::Operand(_) => false,
        Expr::Neg(inner) => pure_expr(p, *inner, known),
        Expr::Bin(a, _, b) => pure_expr(p, *a, known) && pure_expr(p, *b, known),
        Expr::Pow(base, exponent) => pure_expr(p, *base, known) && pure_int(p, exponent, known),
    }
}

/// A place whose address is its base's plus a constant: no subscripts, OCCURS DEPENDING ON or
/// reference modification, on a base `evaluate` takes without a check.
pub(super) fn direct(place: &Place) -> bool {
    plain(place) && place.moved.is_empty() && place.subscripts.is_empty() && place.odo.is_empty() && place.refmod.is_none()
}

/// A static place whose kind `store::read_digits` can read.
pub(super) fn number_item(place: &Place) -> bool {
    let digits = matches!(place.kind, Kind::Index | Kind::Binary { .. } | Kind::Packed { .. } | Kind::Zoned { .. });
    digits && place.scaling == 0 && is_static(place)
}

/// A subscripted place whose address is its base's plus a constant plus what each subscript adds,
/// each subscript a literal or a data item with a constant address: located from these alone while
/// nothing watches the walker's locates (`Vm::quick`).
pub(super) struct Quick {
    subscripts: Vec<(QuickSubscript, u32)>,
}

enum QuickSubscript {
    Const(i64),
    Item { base: Base, offset: u32, len: u32, kind: Kind, item: PlaceId },
}

pub(super) fn quick_places(p: &Program) -> Vec<Option<Quick>> {
    p.places.iter().map(|place| quick_place(p, place)).collect()
}

fn quick_place(p: &Program, place: &Place) -> Option<Quick> {
    let fixed = matches!(place.base, Base::Program | Base::Local | Base::ReturnCode);
    if !fixed || place.subscripts.is_empty() || !place.moved.is_empty() || !place.odo.is_empty() || place.refmod.is_some() {
        return None;
    }
    let subscripts = place.subscripts.iter().map(|s| {
        let value = match s.value {
            IntExpr::Const(n) => QuickSubscript::Const(n),
            IntExpr::Item(q) if is_static(&p.places[q as usize]) => {
                let item = &p.places[q as usize];
                QuickSubscript::Item { base: item.base, offset: item.offset, len: item.len, kind: item.kind, item: q }
            }
            _ => return None,
        };
        Some((value, s.stride))
    });
    Some(Quick { subscripts: subscripts.collect::<Option<_>>()? })
}

/// The `Loc`s of pure places located while a comparison runs, which writes nothing: the walker
/// locates its operands again, and those locates are taken from here. Held only while neither
/// taint nor NUMCHECK, whose reads a locate makes, can tell.
pub(super) struct Memo {
    places: [PlaceId; 4],
    locs: [Loc; 4],
    len: usize,
}

impl Default for Memo {
    fn default() -> Self {
        Self { places: [0; 4], locs: [Loc { offset: 0, len: 0, kind: Kind::Group, item: 0 }; 4], len: 0 }
    }
}

impl Memo {
    fn get(&self, place: PlaceId) -> Option<Loc> {
        self.places[..self.len].iter().position(|&p| p == place).map(|k| self.locs[k])
    }

    fn put(&mut self, place: PlaceId, loc: Loc) {
        if self.len < self.places.len() {
            (self.places[self.len], self.locs[self.len]) = (place, loc);
            self.len += 1;
        }
    }
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
        if self.code.direct[place as usize]
            && let Some(loc) = self.direct_loc(place)
        {
            self.unit.taint_read(loc);
            return Ok(loc);
        }
        let memo = fixed.is_empty() && self.memo.is_some() && self.code.pure[place as usize];
        if memo && let Some(loc) = self.memo.as_ref().and_then(|m| m.get(place)) {
            return Ok(loc);
        }
        self.locating += 1;
        let loc = self.evaluate(place, fixed);
        self.locating -= 1;
        if memo && let (Some(m), Ok(loc)) = (self.memo.as_mut(), &loc) {
            m.put(place, *loc);
        }
        loc
    }

    /// Whether neither taint nor NUMCHECK, whose reads a locate makes, can tell a pure place
    /// located once from the walker's locates of it again.
    pub(super) fn unseen(&self) -> bool {
        self.unit.taint.is_none() && self.p.options.options.numcheck.is_none()
    }

    /// Runs `f` with pure places' locates held, where nothing can tell them apart from the
    /// walker's repeated locates.
    pub(super) fn memoized<T>(&mut self, f: impl FnOnce(&mut Self) -> R<T>) -> R<T> {
        if self.memo.is_some() || !self.unseen() {
            return f(self);
        }
        self.memo = Some(Memo::default());
        let result = f(self);
        self.memo = None;
        result
    }

    fn evaluate(&mut self, id: PlaceId, fixed: &[(u32, i64)]) -> R<Loc> {
        let place = &self.p.places[id as usize];
        let base = match place.base {
            Base::Program => Some(self.base),
            Base::Local => Some(self.local_base),
            Base::Linkage(record) => self.linkage[record as usize],
            Base::ReturnCode => Some(RETURN_CODE),
            _ => None,
        };
        if let Some(base) = base
            && place.moved.is_empty()
            && place.subscripts.is_empty()
            && place.odo.is_empty()
            && place.refmod.is_none()
        {
            let (offset, len) = (base + place.offset as usize, place.len as usize);
            if offset + len <= self.unit.mem.len() {
                let loc = Loc { offset, len, kind: place.kind, item: id as usize };
                self.unit.taint_read(loc);
                return Ok(loc);
            }
        }
        if let Some(quick) = &self.code.quick[id as usize]
            && fixed.is_empty()
            && self.unseen()
            && let Some(loc) = self.quick(id, place, quick)
        {
            return Ok(loc);
        }
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
        let mut composed = 0;
        for (k, s) in place.subscripts.iter().enumerate() {
            let value = match fixed.iter().find(|&&(at, _)| at as usize == k) {
                Some(&(_, v)) => v,
                None => self.int(&s.value, pos)?,
            };
            composed += loc::subscript(value, s.stride);
        }
        if let Some(t) = place.table {
            loc::table_reference(i64::from(t.displacement) + composed, i64::from(place.len), i64::from(t.extent), name, pos)?;
        }
        offset += composed;
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

    /// `evaluate`'s first step for a direct place, without its memo and counting: its base's
    /// address plus its offset, None where a LINKAGE record has no address or the place lies
    /// outside storage, which `evaluate` then reports.
    fn direct_loc(&self, id: PlaceId) -> Option<Loc> {
        let place = &self.p.places[id as usize];
        let base = match place.base {
            Base::Linkage(record) => self.linkage[record as usize]?,
            base => self.base_of(base),
        };
        let (offset, len) = (base + place.offset as usize, place.len as usize);
        (offset + len <= self.unit.mem.len()).then_some(Loc { offset, len, kind: place.kind, item: id as usize })
    }

    fn base_of(&self, base: Base) -> usize {
        match base {
            Base::Program => self.base,
            Base::Local => self.local_base,
            _ => RETURN_CODE,
        }
    }

    /// `evaluate` of a quick place, where taint and NUMCHECK are off: each subscript item is read
    /// where it lies. None where an item is not a plain integer or a check fails, which `evaluate`
    /// then gives in full, as these places are pure.
    fn quick(&self, id: PlaceId, place: &Place, quick: &Quick) -> Option<Loc> {
        let facts = self.facts();
        let mut composed = 0;
        for (subscript, stride) in &quick.subscripts {
            let value = match *subscript {
                QuickSubscript::Const(n) => n,
                QuickSubscript::Item { base, offset, len, kind, item } => {
                    let offset = self.base_of(base) + offset as usize;
                    if offset + len as usize > self.unit.mem.len() {
                        return None;
                    }
                    store::read_integer(&facts, &self.unit.mem, Loc { offset, len: len as usize, kind, item: item as usize })?
                }
            };
            composed += loc::subscript(value, *stride);
        }
        if let Some(t) = place.table {
            let from = i64::from(t.displacement) + composed;
            if from < 0 || from + i64::from(place.len) > i64::from(t.extent) {
                return None;
            }
        }
        let offset = i64::try_from(self.base_of(place.base) + place.offset as usize).ok()? + composed;
        let len = place.len as usize;
        let fits = offset >= 0 && offset as usize + len <= self.unit.mem.len();
        fits.then_some(Loc { offset: offset as usize, len, kind: place.kind, item: id as usize })
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
                match self.eval_number(*expr, *dmax, pos)? {
                    Number::Int(n, places) if places.dec == 0 => Ok(n),
                    v => Ok(whole(&v.fixed(), pos)?),
                }
            }
            IntExpr::Walk(k) => match self.markup.walk.get(usize::from(*k)) {
                Some(&s) => Ok(i64::from(s)),
                None => Err(not_yet("a JSON walk subscript outside the walk")),
            },
        }
    }

    /// A static numeric item's value read where it lies, while taint and NUMCHECK, which its locate
    /// and read would tell, are off; None where `loc` and `read` must take it.
    #[inline]
    pub(super) fn static_number(&self, p: PlaceId) -> Option<Number> {
        let n = self.static_digits(p)?;
        Some(Number::Int(n, places_of(self.p.places[p as usize].kind)))
    }

    #[inline]
    pub(super) fn static_integer(&self, p: PlaceId) -> Option<i64> {
        if scale(self.p.places[p as usize].kind) != 0 {
            return None;
        }
        self.static_digits(p)
    }

    #[inline]
    fn static_digits(&self, p: PlaceId) -> Option<i64> {
        if !self.code.numbers[p as usize] || !self.unseen() {
            return None;
        }
        let place = &self.p.places[p as usize];
        let (offset, len) = (self.base_of(place.base) + place.offset as usize, place.len as usize);
        if offset + len > self.unit.mem.len() {
            return None;
        }
        store::read_digits(&self.facts(), &self.unit.mem, Loc { offset, len, kind: place.kind, item: p as usize })
    }

    /// `Machine::integer` of a data item: located for its dmax, then located and read.
    pub(super) fn int_place(&mut self, p: PlaceId, pos: Pos) -> R<i64> {
        if let Some(n) = self.static_integer(p) {
            return Ok(n);
        }
        let place = &self.p.places[p as usize];
        if !is_static(place) {
            self.loc(p)?;
        }
        let loc = self.loc(p)?;
        let at = self.pos(place.at);
        self.numcheck(loc, SenderCheck::Item, at)?;
        if plain(place)
            && let Some(n) = store::read_integer(&self.facts(), &self.unit.mem, loc)
        {
            return Ok(n);
        }
        let val = self.read(loc, at)?;
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
