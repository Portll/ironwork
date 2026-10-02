//! INITIATE, GENERATE, TERMINATE and SUPPRESS PRINTING (lir.md §9.6), as machine/report.rs runs
//! them over `rt::report`: the lowered report model's comparands, places and constants evaluated
//! by the VM, its lines written through the file verbs, and each USE BEFORE REPORTING section run
//! as a procedure.

use super::files::Io;
use super::flow::{Arrival, Exit};
use super::value::constant;
use super::{Code, Facts, R, Vm};
use crate::abend::Abend;
use crate::files::Format;
use crate::host::Host;
use crate::lir::{Comparand, ConstId, PlaceId, RangeId, ReportOp, Spacing, Step};
use crate::report::{self, ReportFile, ReportHost, UseEnd};
use crate::storage::{Loc, Val};
use crate::store;
use crate::unit::Loader;
use crate::vocab::Pos;
use numeric::precision::Fixed;
use std::io::Write;
use std::rc::Rc;

/// The VM as the report writer takes it, its CONTROL items by reference.
struct Reports<'a, 'p, 'u, 'w, L: Loader<Rc<Code>>> {
    vm: &'a mut Vm<'p, 'u, 'w, L>,
}

impl<L: Loader<Rc<Code>>> Vm<'_, '_, '_, L> {
    pub(super) fn report(&mut self, op: ReportOp, pos: Pos) -> R<Step> {
        let writer = &self.p.services.report;
        let ran = report::run(&mut Reports { vm: self }, writer, op, pos);
        self.concluded(ran.map(|e| e.map_or(Step::Next, Step::End)))
    }
}

impl<'p, L: Loader<Rc<Code>>> Host<&'p PlaceId> for Reports<'_, 'p, '_, '_, L> {
    type Facts = Facts<'p>;

    fn facts(&self) -> Facts<'p> {
        self.vm.facts()
    }

    fn mem(&mut self) -> &mut [u8] {
        &mut self.vm.unit.mem
    }

    fn locate(&mut self, place: &'p PlaceId, receiving: bool) -> Result<Loc, Abend> {
        Host::<PlaceId>::locate(&mut *self.vm, *place, receiving)
    }

    fn integer(&mut self, place: &'p PlaceId, pos: Pos) -> Result<i64, Abend> {
        Host::<PlaceId>::integer(&mut *self.vm, *place, pos)
    }

    fn assign(&mut self, dest: Loc, val: Val, src: Option<Loc>, pos: Pos) -> Result<(), Abend> {
        store::assign(&self.vm.facts(), self.vm.unit, dest, val, src, pos)
    }

    fn store_fixed(&mut self, dest: Loc, value: &Fixed, pos: Pos) -> Result<(), Abend> {
        store::store_fixed(&self.vm.facts(), self.vm.unit, dest, value, false, pos)
    }
}

impl<'p, L: Loader<Rc<Code>>> ReportHost<'p, Comparand, PlaceId, ConstId, RangeId> for Reports<'_, 'p, '_, '_, L> {
    fn value(&mut self, expr: &Comparand, pos: Pos) -> Result<Val, Abend> {
        let value = self.vm.comparand(expr, pos);
        self.vm.lift(value, pos)
    }

    fn operand(&mut self, expr: &Comparand, pos: Pos) -> Option<Result<(Val, Option<Loc>), Abend>> {
        let Comparand::Operand(o) = expr else { return None };
        let read = self.vm.value_with_loc(*o);
        Some(self.vm.lift(read, pos))
    }

    fn literal(&mut self, value: &ConstId, _pos: Pos) -> Result<Val, Abend> {
        Ok(constant(&self.vm.p.consts[*value as usize]))
    }

    fn item(&self, item: usize) -> Loc {
        self.vm.static_loc(item as PlaceId)
    }

    fn store_value(&mut self, dest: Loc, value: Val, rounded: bool, keep_on_size_error: bool, pos: Pos) -> Result<bool, Abend> {
        store::store_value(&self.vm.facts(), self.vm.unit, dest, value, rounded, keep_on_size_error, pos)
    }

    fn store_checked(&mut self, dest: Loc, value: &Fixed, pos: Pos) -> Result<bool, Abend> {
        store::store_fixed_checked(&self.vm.facts(), self.vm.unit, dest, value, false, true, pos)
    }

    fn report_file(&mut self, k: usize) -> ReportFile {
        let d = &self.vm.p.services.files[k];
        ReportFile {
            area: self.vm.file_desc(k).area,
            variable: self.vm.unit.programs[self.vm.me].files[k].as_ref().is_some_and(|f| f.format == Format::Variable),
            reserved: d.carriage.is_some_and(|c| c.reserved),
            record_min: d.record_min,
        }
    }

    fn write_line(&mut self, k: usize, loc: Loc, space: Spacing, pos: Pos) -> Result<(), Abend> {
        Io { vm: &mut *self.vm }.write_line(k, loc, space, pos)
    }

    fn use_before_reporting(&mut self, procedure: &RangeId, pos: Pos) -> Result<UseEnd, Abend> {
        Ok(match self.vm.procedure(*procedure, Arrival::Use, pos)? {
            Exit::Completed => UseEnd::Completed,
            Exit::End(e) => UseEnd::End(e),
            Exit::Left(Step::GoTo(_)) => UseEnd::GoTo,
            Exit::Left(step) => {
                self.vm.io.leaving = Some(step);
                UseEnd::Left
            }
        })
    }

    fn err(&mut self) -> &mut dyn Write {
        &mut *self.vm.unit.err
    }
}
