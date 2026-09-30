//! SET (lir.md §9.1), each form as `Machine::set` runs it: condition-names TO TRUE and TO FALSE as
//! MOVEs, TO as a MOVE with pointer receivers taking only addresses, ADDRESS OF as one binding of
//! each LINKAGE record, and UP BY and DOWN BY with the step evaluated once.

use super::data::Value;
use super::{Lower, R, unsupported};
use crate::layout::Resolved;
use rt::lir::{MovePlan, Op, StepPlan, Terminator, UpDown};
use rt::storage::Kind;
use syntax::Pos;
use syntax::ast::{Figurative, SetStmt};

impl Lower<'_> {
    pub(super) fn set(&mut self, set: &SetStmt, pos: Pos) -> R<()> {
        match set {
            SetStmt::ConditionTrue(targets) | SetStmt::ConditionFalse(targets) => {
                let truth = matches!(set, SetStmt::ConditionTrue(_));
                for r in targets {
                    let index = match self.layout.resolve(&r.name, &r.qualifiers, r.pos) {
                        Ok(Resolved::Condition(index)) => index,
                        Ok(Resolved::Item(_)) => {
                            let abend = self.ironwork(&format!("SET {} TO {}: not a condition-name", r.name, if truth { "TRUE" } else { "FALSE" }))?;
                            return self.end(Terminator::Abend(abend), pos);
                        }
                        Err(e) => {
                            let abend = self.ironwork(&e.message)?;
                            return self.end(Terminator::Abend(abend), r.pos);
                        }
                    };
                    let condition = &self.layout.conditions[index];
                    let value = if truth { condition.values.first().map(|(v, _)| v) } else { condition.false_value.as_ref() };
                    let Some(value) = value else { continue };
                    let to = match self.conditional_variable(index, r, pos)? {
                        Ok(place) => place,
                        Err((abend, at)) => return self.end(Terminator::Abend(abend), at),
                    };
                    let (from, side) = self.literal_const(value, pos)?;
                    let plan = self.move_plan(&side, self.kind_of(to), self.place_items[to as usize])?;
                    self.op(Op::Move { from: rt::lir::Operand::Const(from), to, plan }, pos)?;
                }
            }
            SetStmt::To { targets, value } => {
                for r in targets {
                    let to = self.place(r, false)?;
                    let sender = self.operand(value, pos)?;
                    let kind = self.kind_of(to);
                    let plan = match (kind, sender.side.value) {
                        (Kind::Pointer, Value::Address | Value::Fig(Figurative::Null)) => MovePlan::Address,
                        (Kind::Pointer, _) => MovePlan::Refused(self.ironwork("SET a pointer TO ADDRESS OF, NULL or another pointer")?),
                        _ => self.move_plan(&sender.side, kind, self.place_items[to as usize])?,
                    };
                    self.op(Op::Move { from: sender.operand, to, plan }, pos)?;
                }
            }
            SetStmt::AddressOf { targets, value } => {
                let address = self.operand(value, pos)?.operand;
                let mut records = Vec::with_capacity(targets.len());
                for r in targets {
                    let refusal = match self.layout.resolve(&r.name, &r.qualifiers, r.pos) {
                        Ok(Resolved::Item(i)) => match self.layout.items[i].linkage.filter(|_| self.layout.items[i].parent.is_none()) {
                            Some(record) => {
                                records.push(record);
                                continue;
                            }
                            None => (format!("SET ADDRESS OF {}: only a LINKAGE record can be given an address", r.name), pos),
                        },
                        Ok(Resolved::Condition(_)) => (format!("SET ADDRESS OF {}: not a data item", r.name), pos),
                        Err(e) => (e.message, r.pos),
                    };
                    self.op(Op::SetAddress { records, address }, pos)?;
                    let abend = self.ironwork(&refusal.0)?;
                    return self.end(Terminator::Abend(abend), refusal.1);
                }
                self.op(Op::SetAddress { records, address }, pos)?;
            }
            SetStmt::UpDown { targets, down, by } => {
                let by = self.int_expr(by, pos)?;
                let mut moved = Vec::with_capacity(targets.len());
                for r in targets {
                    let place = self.place(r, false)?;
                    let kind = self.kind_of(place);
                    let how = match kind {
                        Kind::Pointer | Kind::ObjectReference | Kind::ProgramPointer => UpDown::Pointer,
                        Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. } | Kind::Index => {
                            UpDown::Number(StepPlan { dmax: 0, store: self.store_plan(kind, self.place_items[place as usize])? })
                        }
                        _ => UpDown::Refused(self.ironwork("SET UP BY and DOWN BY take an index, integer or pointer")?),
                    };
                    moved.push((place, how));
                }
                self.op(Op::SetUpDown { by, down: *down, targets: moved }, pos)?;
            }
        }
        Ok(())
    }

    /// A SET or test's conditional variable, found again by its unqualified name with the
    /// condition-name's subscripts, as the walker finds it; or the abend that finding gives, with
    /// where it names. `at` is the position the walker's reference to the variable carries.
    pub(super) fn conditional_variable(&mut self, index: usize, r: &syntax::ast::Ref, at: Pos) -> R<Result<rt::lir::PlaceId, (rt::lir::AbendId, Pos)>> {
        let layout = self.layout;
        let condition = &layout.conditions[index];
        let item = &layout.items[condition.item];
        let name = item.name.clone().unwrap_or_default();
        let found = match layout.resolve(&name, &[], at) {
            Ok(Resolved::Item(i)) if i == condition.item => None,
            Ok(Resolved::Item(_)) => return unsupported("a conditional variable whose name finds another item", r.pos),
            Ok(Resolved::Condition(_)) => Some(format!("{name} is a condition-name, not a data item")),
            Err(e) => Some(e.message),
        };
        let found = found.or_else(|| (r.subscripts.len() != item.dims.len()).then(|| format!("{name} takes {} subscripts, not {}", item.dims.len(), r.subscripts.len())));
        if let Some(message) = found {
            return Ok(Err((self.ironwork(&message)?, at)));
        }
        let subject = syntax::ast::Ref { name, qualifiers: Vec::new(), subscripts: r.subscripts.clone(), refmod: None, pos: at };
        Ok(Ok(self.item_place(condition.item, &subject, false)?))
    }
}
