//! The ops (lir.md §8.1, §9): each a call of the semantics library the walker's statement makes,
//! with the `Loc`s and values it would pass, and the `Host` the library's statements call back.

use super::{Code, Facts, R, Vm, not_yet};
use crate::abend::Abend;
use crate::accept;
use crate::display;
use crate::host::{Host, Values};
use crate::lir::{DisplayItem, InitPlan, Inspected, MovePlan, NumericFrom, Op, Operand, PlaceId, SearchAllPlan, SenderCheck, Step, StorePlan, TempId};
use crate::set;
use crate::storage::{Kind, Loc, Val};
use crate::store;
use crate::text::{self, UnstringField};
use crate::unit::Loader;
use crate::vocab::{Figurative, Pos};
use numeric::precision::Fixed;
use std::cmp::Ordering;
use std::rc::Rc;

/// Whether a store by MOVE rules passes the sender's storage with its value. Only PERFORM VARYING's
/// FROM lowers without it, into a fixed-point receiver, where its plan shows what the storage would
/// change: a group's bytes copied, a numeric-edited item de-edited, a packed item copied under
/// NUMPROC(PFD).
fn sender_kept(sender: Kind, dest: Kind, plan: &MovePlan) -> bool {
    if !matches!(dest, Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. }) {
        return true;
    }
    match (sender, plan) {
        (Kind::Group, MovePlan::Numeric { .. }) => false,
        (Kind::NumericEdited { .. }, MovePlan::Numeric { from: NumericFrom::Zoned, .. }) => false,
        (Kind::Packed { .. }, MovePlan::Numeric { from: NumericFrom::Value, .. }) => sender != dest,
        _ => true,
    }
}

/// The kind of the elementary item an INITIALIZE field's MOVE plan was made for.
fn field_kind(plan: &MovePlan) -> Option<Kind> {
    Some(match *plan {
        MovePlan::Alnum { justified, .. } => Kind::Alnum { justified },
        MovePlan::AlnumEdited { edit, .. } => Kind::AlnumEdited { edit },
        MovePlan::National(_) => Kind::National,
        MovePlan::Float { precision, .. } => Kind::Float(precision),
        MovePlan::Address => Kind::Pointer,
        MovePlan::Index => Kind::Index,
        MovePlan::Numeric { store, .. } => match store {
            StorePlan::Zoned { digits, scale, signed, sign } => Kind::Zoned { digits, scale, signed, sign },
            StorePlan::Packed { digits, scale, signed } => Kind::Packed { digits, scale, signed },
            StorePlan::Binary { digits, scale, signed, native, .. } => Kind::Binary { digits, scale, signed, native },
            StorePlan::NumericEdited { edit, digits, scale, blank_when_zero } => Kind::NumericEdited { edit, digits, scale, blank_when_zero },
            StorePlan::Float(p) => Kind::Float(p),
            StorePlan::Index => Kind::Index,
            StorePlan::Refused(_) => return None,
        },
        MovePlan::Refused(_) => return None,
    })
}

impl<'p, L: Loader<Rc<Code>>> Vm<'p, '_, '_, L> {
    pub(super) fn op(&mut self, op: &Op, at: u32) -> R<Step> {
        let p = self.p;
        let pos = self.pos(at);
        match op {
            Op::Move { from, to, plan, check } => {
                let dest = self.loc_written(*to)?;
                self.move_to(Some(*check), *from, dest, plan, at)?;
            }
            Op::Set { from, to, plan } => {
                let dest = self.loc_written(*to)?;
                self.move_to(None, *from, dest, plan, at)?;
            }
            Op::Initialize { target, plan } => self.initialize(*target, &p.plans.init[*plan as usize], pos)?,
            Op::Arith(id) => return self.arith(&p.plans.arith[*id as usize], pos),
            Op::SetAddress { records, address } => {
                let val = self.value(*address)?;
                let offset = set::address(val, self.unit.mem.len(), pos)?;
                for &r in records {
                    self.linkage[r as usize] = offset;
                }
            }
            Op::SetUpDown { by, down, targets } => {
                let by = self.int(by, pos)?;
                let places: Vec<PlaceId> = targets.iter().map(|(place, _)| *place).collect();
                let result = set::up_down(self, by, *down, &places, pos);
                self.settle(result)?;
            }
            Op::Step { var, by, plan, prepass } => {
                let dest = self.loc(*var)?;
                for &q in prepass {
                    self.loc(q)?;
                }
                let current = self.value(Operand::Load(*var))?;
                let x = crate::arith::fixed_operand(current, plan.dmax, pos)?;
                let y = self.eval_fixed(*by, plan.dmax, pos)?;
                let next = crate::arith::fixed_binop(x, crate::vocab::BinOp::Add, y, plan.dmax, p.options.options.arith, pos)?;
                store::store_fixed(&self.facts(), self.unit, dest, &next, false, pos)?;
            }
            Op::SetInt { target, value } => {
                let n = self.int(value, pos)?;
                let dest = self.loc(*target)?;
                store::set_integer(&self.facts(), self.unit, dest, n, pos)?;
            }
            Op::Inspect(id) => self.inspect(*id, pos)?,
            Op::String(id) => {
                let plan = &p.plans.string[*id as usize];
                let result = text::string(self, plan.into, plan.pointer.map(|(q, _)| q), &plan.sources, pos);
                return Ok(Step::Arm(u8::from(self.settle(result)?)));
            }
            Op::Unstring(id) => {
                let plan = &p.plans.unstring[*id as usize];
                let into: Vec<UnstringField<PlaceId>> =
                    plan.into.iter().map(|i| UnstringField { target: i.target, delimiter: i.delimiter.map(|d| d.target), count: i.count.map(|(q, _)| q) }).collect();
                let result = text::unstring(self, plan.source, plan.pointer.map(|(q, _)| q), &plan.delimiters, &into, plan.tallying.map(|(q, _)| q), pos);
                return Ok(Step::Arm(u8::from(self.settle(result)?)));
            }
            Op::SearchAll(id) => return self.search_all(&p.plans.search_all[*id as usize], pos),
            Op::Nest => self.unit.enter(pos)?,
            Op::Unnest(n) => self.unit.depth = self.unit.depth.saturating_sub(usize::from(*n)),
            Op::SetTemp(t, value) => {
                let n = self.int(value, pos)?.max(0);
                self.set_temp(*t, n)?;
            }
            Op::SetCount(t, odo) => {
                let n = self.occurrences(odo, pos)?;
                self.set_temp(*t, i64::from(n))?;
            }
            Op::DecTemp(t) => {
                if let Some(n) = self.returns.frames.last_mut().and_then(|f| f.temps.get_mut(usize::from(*t))) {
                    *n -= 1;
                }
            }
            Op::Display(id) => self.display(*id, pos)?,
            Op::Accept { target, from, .. } => {
                let dest = self.loc_written(*target)?;
                let name = self.sym(p.places[*target as usize].name);
                accept::accept(&self.facts(), self.unit, dest, *from, name, pos)?;
            }
            Op::File(id) => return self.file(&p.services.file_ops[*id as usize], at),
            Op::Call(id) => return self.call(&p.services.calls[*id as usize], pos),
            Op::Cancel(name) => self.cancel(*name, pos)?,
            Op::Sort(id) => return self.sort(&p.services.sorts[*id as usize], pos),
            Op::Release(id) => self.release(&p.services.releases[*id as usize], at)?,
            Op::Return(id) => return self.return_record(&p.services.returns[*id as usize], pos),
            Op::Report(op) => return self.report(*op, pos),
            Op::Invoke(id) => return self.invoke(&p.services.invokes[*id as usize], pos),
            Op::Cics(id) => return self.cics(*id, pos),
            Op::Sql(ordinal) => return self.sql(*ordinal, pos),
            Op::Markup(id) => return self.markup(&p.services.markup[*id as usize], at, pos),
            Op::Alter { para, to } => {
                let paragraphs = p.paragraphs.len();
                let altered = &mut self.unit.programs[self.me].altered;
                altered.resize(paragraphs, None);
                altered[*para as usize] = Some(*to as usize);
            }
            Op::EnterSegment(priority) => self.enter_segment(*priority),
            Op::DebugLine(line) => self.line = *line,
            Op::DebugAlter { range, name, contents } => {
                if self.debugging {
                    return Ok(Step::Next);
                }
                return Ok(self.run_debugging(*range, self.sym(*name), pos.line, self.sym(*contents), pos)?.unwrap_or(Step::Next));
            }
        }
        Ok(Step::Next)
    }

    /// MOVE of `from` into `dest`, located already, by `plan`, a data item sender tested as `check`
    /// says (`Machine::move_source`); without `check`, as SET TO moves it.
    pub(super) fn move_to(&mut self, check: Option<SenderCheck>, from: Operand, dest: Loc, plan: &MovePlan, at: u32) -> R<()> {
        let (val, src) = match (check, from) {
            (Some(check), Operand::Load(p)) => {
                let src = self.loc(p)?;
                let pos = self.pos(self.p.places[p as usize].at);
                self.numcheck(src, check, pos)?;
                (store::move_sender(&self.facts(), &self.unit.mem, src, dest, pos)?, Some(src))
            }
            _ => self.value_with_loc(from)?,
        };
        if let MovePlan::Refused(abend) = plan {
            return Err(self.abend(*abend, Some(at)).into());
        }
        let src = src.filter(|s| sender_kept(s.kind, dest.kind, plan));
        Ok(store::assign(&self.facts(), self.unit, dest, val, src, self.pos(at))?)
    }

    fn set_temp(&mut self, t: TempId, n: i64) -> R<()> {
        let Some(frame) = self.returns.frames.last_mut() else { return Err(not_yet("a TIMES counter outside a frame")) };
        let t = usize::from(t);
        if frame.temps.len() <= t {
            frame.temps.resize(t + 1, 0);
        }
        frame.temps[t] = n;
        Ok(())
    }

    /// `Machine::enter_segment`: an independent segment entered from another is in its initial
    /// state, its altered GO TOs as written (assumption C52).
    pub(super) fn enter_segment(&mut self, priority: u8) {
        if priority == self.segment {
            return;
        }
        self.segment = priority;
        if priority >= 50 {
            let p = self.p;
            for (i, target) in self.unit.programs[self.me].altered.iter_mut().enumerate() {
                if p.paragraphs[i].priority == priority {
                    *target = None;
                }
            }
        }
    }

    /// `Machine::initialize`: each elementary item the walk reaches given SPACE, ZERO or NULL by
    /// MOVE rules.
    fn initialize(&mut self, target: PlaceId, plan: &InitPlan, pos: Pos) -> R<()> {
        let loc = self.loc_written(target)?;
        for field in &plan.fields {
            let Some(kind) = field_kind(&field.store) else { return Err(not_yet("an INITIALIZE field with no MOVE plan")) };
            let at = Loc { offset: loc.offset + field.offset as usize, len: field.len as usize, kind, item: usize::MAX };
            let val = match field.value {
                Figurative::Null => Val::Address(0),
                other => Val::Fig(other),
            };
            store::assign(&self.facts(), self.unit, at, val, None, pos)?;
        }
        Ok(())
    }

    fn inspect(&mut self, id: u32, pos: Pos) -> R<()> {
        let plan = &self.p.plans.inspect[id as usize];
        let phrase = |p: &crate::lir::InspectPhrase| text::InspectPhrase {
            mode: p.mode,
            pattern: p.pattern.clone(),
            by: p.by.clone(),
            counter: p.counter.map(|(q, _)| q),
            bounds: p.bounds.clone(),
        };
        let tallying: Vec<_> = plan.tallying.iter().map(phrase).collect();
        let result = match &plan.target {
            Inspected::Item(target) => {
                let replacing: Vec<_> = plan.replacing.iter().map(phrase).collect();
                text::inspect(self, *target, &tallying, &replacing, plan.converting.as_ref(), pos)
            }
            Inspected::Value(subject) => text::tally(self, subject, &tallying, pos),
        };
        self.settle(result)
    }

    /// `Machine::display`: each item as its kind or value shows, the line told to an observer,
    /// then written.
    fn display(&mut self, id: u32, pos: Pos) -> R<()> {
        let p = self.p;
        let plan = &p.plans.display[id as usize];
        let mut shown = String::new();
        for item in &plan.items {
            shown.push_str(&match item {
                DisplayItem::Bytes(place) | DisplayItem::National(place) | DisplayItem::Digits { place, .. } | DisplayItem::Refused { place, .. } => {
                    let loc = self.loc(*place)?;
                    display::place(&self.facts(), &self.unit.mem, loc, self.pos(p.places[*place as usize].at))?
                }
                DisplayItem::Text(text) => self.sym(*text).to_owned(),
                DisplayItem::Value(o) => {
                    let val = self.value(*o)?;
                    display::value(&self.facts(), val, pos)?
                }
            });
        }
        if self.unit.observed() {
            self.sink("log", pos, &shown);
        }
        Ok(display::write(&mut *self.unit.out, &shown, plan.no_advancing, pos)?)
    }

    /// `Machine::search` of SEARCH ALL: a binary search setting the index to each occurrence
    /// tried; Arm(0) at one whose keys equal their WHEN terms, Arm(1) when there is none.
    fn search_all(&mut self, plan: &SearchAllPlan, pos: Pos) -> R<Step> {
        let count = i64::from(self.count(&plan.count, pos)?);
        let (mut low, mut high) = (1i64, count);
        while low <= high {
            let mid = (low + high) / 2;
            let dest = self.loc(plan.index)?;
            store::set_integer(&self.facts(), self.unit, dest, mid, pos)?;
            let mut order = Ordering::Equal;
            for key in &plan.keys {
                let o = self.compare(&key.key, &key.value, key.how, pos)?;
                order = if key.ascending { o } else { o.reverse() };
                if order != Ordering::Equal {
                    break;
                }
            }
            match order {
                Ordering::Less => low = mid + 1,
                Ordering::Greater => high = mid - 1,
                Ordering::Equal => return Ok(Step::Arm(0)),
            }
        }
        Ok(Step::Arm(1))
    }
}

impl<'p, L: Loader<Rc<Code>>> Host<PlaceId> for Vm<'p, '_, '_, L> {
    type Facts = Facts<'p>;

    fn facts(&self) -> Facts<'p> {
        Vm::facts(self)
    }

    fn mem(&mut self) -> &mut [u8] {
        &mut self.unit.mem
    }

    fn taint(&mut self) -> Option<&mut crate::taint::Taint> {
        self.unit.taint.as_mut()
    }

    fn locate(&mut self, place: PlaceId, _receiving: bool) -> Result<Loc, Abend> {
        let pos = self.pos(self.p.places[place as usize].at);
        let loc = self.loc(place);
        self.lift(loc, pos)
    }

    fn integer(&mut self, place: PlaceId, pos: Pos) -> Result<i64, Abend> {
        let n = self.int_place(place, pos);
        self.lift(n, pos)
    }

    fn assign(&mut self, dest: Loc, val: Val, src: Option<Loc>, pos: Pos) -> Result<(), Abend> {
        store::assign(&Vm::facts(self), self.unit, dest, val, src, pos)
    }

    fn store_fixed(&mut self, dest: Loc, value: &Fixed, pos: Pos) -> Result<(), Abend> {
        store::store_fixed(&Vm::facts(self), self.unit, dest, value, false, pos)
    }
}

impl<L: Loader<Rc<Code>>> Values<PlaceId, Operand> for Vm<'_, '_, '_, L> {
    fn value(&mut self, operand: &Operand, pos: Pos) -> Result<Val, Abend> {
        let val = Vm::value(self, *operand);
        self.lift(val, pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lir::Image;

    #[test]
    fn only_varying_from_moves_without_the_sender_s_storage() {
        let zoned = Kind::Zoned { digits: 3, scale: 0, signed: false, sign: None };
        let packed = Kind::Packed { digits: 5, scale: 0, signed: true };
        let store = StorePlan::Packed { digits: 5, scale: 0, signed: true };
        let bytes = MovePlan::Alnum { image: Image::Bytes, justified: false };
        assert!(sender_kept(Kind::Group, packed, &bytes));
        assert!(!sender_kept(Kind::Group, packed, &MovePlan::Numeric { from: NumericFrom::Zoned, store }));
        assert!(sender_kept(packed, packed, &MovePlan::Numeric { from: NumericFrom::PackedCopy, store }));
        assert!(!sender_kept(packed, packed, &MovePlan::Numeric { from: NumericFrom::Value, store }));
        assert!(sender_kept(zoned, packed, &MovePlan::Numeric { from: NumericFrom::Value, store }));
        assert!(sender_kept(Kind::Group, Kind::Alnum { justified: false }, &bytes));
    }
}
