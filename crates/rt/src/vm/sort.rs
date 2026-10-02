//! SORT, MERGE, RELEASE and RETURN (lir.md §9.6), as machine/sort.rs runs them over `rt::sort`:
//! the SD, files, keys and special registers lowering resolved, and each INPUT or OUTPUT PROCEDURE
//! run as a procedure whose region is the whole program.

use super::files::{FileOf, Handle, Io};
use super::flow::{Arrival, Exit};
use super::{Code, R, Vm};
use crate::abend::{Abend, Ending};
use crate::fileio::{self, Outcome};
use crate::files::FileStatus;
use crate::host::Host;
use crate::lir::{DebugId, IntExpr, PlaceId, RangeId, ReleasePlan, ReturnPlan, SortKeys, SortPlan, Step};
use crate::sort::{self, Active, ItemKey, Procedure, SortFile, SortHost};
use crate::storage::Loc;
use crate::unit::Loader;
use crate::vocab::{OpenMode, Pos};
use std::io::Write;
use std::rc::Rc;

/// The keys as `rt::sort` reads them. A key's item is a layout index, which the VM's facts do not
/// know; it gives only P scaling, which multiplies every record's key alike and so orders none
/// differently.
fn item_keys(keys: &SortKeys) -> Vec<ItemKey> {
    sort::item_keys(keys).into_iter().map(|k| ItemKey { item: usize::MAX, ..k }).collect()
}

impl<'p, L: Loader<Rc<Code>>> Vm<'p, '_, '_, L> {
    pub(super) fn sort(&mut self, plan: &'p SortPlan, pos: Pos) -> R<Step> {
        match plan {
            SortPlan::File(f) => {
                let ended = sort::sort(&mut Io { vm: self }, f, pos);
                self.concluded(ended.map(|e| e.map_or(Step::Next, Step::End)))
            }
            SortPlan::Table(t) => {
                let count = self.count(&t.count, pos)?;
                let base = self.loc(t.first)?.offset;
                let name = self.sym(t.name);
                let keys = item_keys(&t.keys);
                let sorted = sort::sort_table::<PlaceId, _>(self, base, count as usize, t.stride as usize, |_| Ok(keys), name, pos);
                self.settle(sorted)?;
                Ok(Step::Next)
            }
        }
    }

    /// RELEASE: the record located, as FROM's receiver when FROM is written; the sort's checks;
    /// FROM's MOVE into that location; the record located again and released.
    pub(super) fn release(&mut self, plan: &'p ReleasePlan, at: DebugId) -> R<()> {
        let pos = self.pos(at);
        let name = self.sym(plan.name);
        let dest = self.loc(plan.from.map_or(plan.record, |f| f.to))?;
        let ready = sort::release_ready(&mut Io { vm: self }, plan.file.map(usize::from), plan.sort_return, name, pos);
        self.settle(ready)?;
        let loc = match &plan.from {
            Some(f) => {
                self.move_from(f, dest, at)?;
                self.loc(plan.record)?
            }
            None => dest,
        };
        let released = sort::release(&mut Io { vm: self }, loc, name, pos);
        self.settle(released)
    }

    /// RETURN: Arm(1) when a record came, Arm(0) at end.
    pub(super) fn return_record(&mut self, plan: &'p ReturnPlan, pos: Pos) -> R<Step> {
        let name = self.sym(plan.name);
        let into = plan.into.map(|(q, _)| Handle::Place(q));
        let returned = sort::return_record(&mut Io { vm: self }, plan.file.map(usize::from), into, plan.sort_return, name, pos);
        Ok(Step::Arm(u8::from(self.settle(returned)?)))
    }
}

impl<'p, L: Loader<Rc<Code>>> Io<'_, 'p, '_, '_, L> {
    /// Runs `op`; true when a statement on file k failed in it.
    fn fails(&mut self, k: usize, op: impl FnOnce(&mut Self, &FileOf<'p>) -> Result<Option<u8>, Abend>) -> Result<bool, Abend> {
        self.vm.io.failed = None;
        let file = self.vm.file_desc(k);
        op(self, &file)?;
        Ok(self.vm.io.failed == Some(k))
    }
}

impl<'p, L: Loader<Rc<Code>>> SortHost<'p, Handle<'p>, &'p IntExpr> for Io<'_, 'p, '_, '_, L> {
    type Register = PlaceId;
    type Procedure = RangeId;
    type File = u16;
    type Keys = SortKeys;

    fn locate_register(&mut self, register: PlaceId) -> Result<Loc, Abend> {
        Host::<PlaceId>::locate(&mut *self.vm, register, false)
    }

    fn register_value(&mut self, register: PlaceId, pos: Pos) -> Result<i64, Abend> {
        Host::<PlaceId>::integer(&mut *self.vm, register, pos)
    }

    fn file_index(&self, file: &u16, _pos: Pos) -> Result<usize, Abend> {
        Ok(usize::from(*file))
    }

    fn keys(&mut self, keys: &SortKeys, _pos: Pos) -> Result<Vec<ItemKey>, Abend> {
        Ok(item_keys(keys))
    }

    fn sort_file(&self, k: usize) -> SortFile<'p, Handle<'p>, &'p IntExpr> {
        let p = self.vm.p;
        let d = &p.services.files[k];
        let name = |q: PlaceId| p.symbols[p.places[q as usize].name as usize].as_str();
        SortFile { file: self.vm.file_desc(k), fixed: d.fixed, status_name: d.status.map(|(q, _)| name(q)), relative_name: d.relative.as_ref().map(|r| name(r.place)) }
    }

    fn active(&mut self) -> &mut Option<Active> {
        &mut self.vm.io.sort
    }

    fn open(&mut self, k: usize, mode: OpenMode, pos: Pos) -> Result<(), Abend> {
        let file = self.vm.file_desc(k);
        let outcome = fileio::open(self, &file, mode, pos)?;
        self.settle(&file, outcome, None, pos).map(drop)
    }

    fn close(&mut self, k: usize, pos: Pos) -> Result<bool, Abend> {
        self.fails(k, |io, file| {
            let outcome = fileio::close(io, file, None, pos)?;
            io.settle(file, outcome, None, pos)
        })
    }

    fn put(&mut self, k: usize, loc: Loc, pos: Pos) -> Result<bool, Abend> {
        self.fails(k, |io, file| match fileio::write(io, file, loc, None, pos)? {
            Outcome::Status { status, .. } => io.conclude(file, status, None, '2', "WRITE", pos),
            outcome => io.settle(file, outcome, None, pos),
        })
    }

    fn fail(&mut self, k: usize, status: FileStatus, mode: Option<OpenMode>, message: String, pos: Pos) -> Result<(), Abend> {
        let file = self.vm.file_desc(k);
        self.io_failure(&file, status, mode, message, pos)
    }

    fn has_error_procedure(&self, k: usize, mode: OpenMode) -> bool {
        self.vm.error_procedure(k, Some(mode)).is_some()
    }

    /// Any other leaving is a return to an active PERFORM, which ends the procedure as its end does.
    fn run_procedure(&mut self, range: RangeId, kind: Procedure, pos: Pos) -> Result<Option<Ending>, Abend> {
        Ok(match self.vm.procedure(range, Arrival::Sort(kind.name()), pos)? {
            Exit::End(e) => Some(e),
            Exit::Completed | Exit::Left(_) => None,
        })
    }

    fn err(&mut self) -> &mut dyn Write {
        &mut *self.vm.unit.err
    }
}
