//! The file statements (lir.md §9.4), as machine/file_io.rs runs them over `rt::fileio`: each
//! verb, then the phrase its status selects or the file's error path, which runs a USE AFTER
//! EXCEPTION/ERROR procedure (§9.10) whose leaving the statement takes as its own transfer.

use super::flow::{Arrival, Exit};
use super::{Code, Facts, Halt, R, Stop, Vm, not_yet};
use crate::abend::{Abend, AbendCode, Signal};
use crate::fileio::{self, File, Files, Outcome, Read};
use crate::files::{Dd, FileStatus, KeySpan, Keying, Open};
use crate::host::Host;
use crate::lir::{Advance, DebugId, FileOp, FileVerb, FromMove, IntExpr, Organization, Phrase, PlaceId, RangeId, RecordSpan, RelativeKey, Spacing, StartKey, Step};
use crate::sort::Active;
use crate::storage::{Loc, Val};
use crate::store;
use crate::unit::{Event, Loader};
use crate::vocab::{OpenMode, Pos};
use numeric::precision::Fixed;
use std::convert::Infallible;
use std::rc::Rc;

/// What the file statements, SORT and the Report Writer keep per activation: the SORT or MERGE
/// whose procedure is running, the file whose statement failed last, and the transfer a
/// declarative procedure left its statement by.
#[derive(Default)]
pub(super) struct State {
    pub(super) sort: Option<Active>,
    pub(super) failed: Option<usize>,
    pub(super) leaving: Option<Step>,
    /// The frames that ran a SORT or MERGE whose procedure an abend unwound, as RELEASE and RETURN
    /// do under SORT-RETURN 16. The unwinding leaves the walker's running paragraph inside the
    /// procedure, and until control reaches another paragraph under such a frame a PERFORM there
    /// finds no statement to resume at (machine/perform.rs `after`).
    pub(super) stale: Vec<u64>,
}

/// What a file verb names: a data item; the RELATIVE KEY, located as its place and read as its
/// value; a key of reference and its span in the record; or an integer.
#[derive(Clone, Copy)]
pub(super) enum Handle<'p> {
    Place(PlaceId),
    Relative(&'p RelativeKey),
    Key(usize, RecordSpan),
    Int(&'p IntExpr),
}

pub(super) type FileOf<'p> = File<'p, Handle<'p>, &'p IntExpr>;

/// The VM as the file verbs, SORT and the Report Writer's lines take it.
pub(super) struct Io<'a, 'p, 'u, 'w, L: Loader<Rc<Code>>> {
    pub(super) vm: &'a mut Vm<'p, 'u, 'w, L>,
}

/// The open modes in the order `Declaratives.modes` holds their procedures.
pub(super) fn mode_index(mode: OpenMode) -> usize {
    match mode {
        OpenMode::Input => 0,
        OpenMode::Output => 1,
        OpenMode::InputOutput => 2,
        OpenMode::Extend => 3,
    }
}

fn key_span(span: RecordSpan) -> KeySpan {
    KeySpan { offset: span.offset as usize, len: span.len as usize }
}

fn advance(a: &Advance) -> fileio::Advance<'static, &IntExpr> {
    match a {
        Advance::Lines { before, count } => fileio::Advance::Lines { before: *before, count },
        Advance::Page { before } => fileio::Advance::Page { before: *before },
        Advance::Mnemonic { before, space } => fileio::Advance::Mnemonic { before: *before, space: Some(*space), name: "", environment: "" },
    }
}

impl<'p, L: Loader<Rc<Code>>> Vm<'p, '_, '_, L> {
    /// File k's SELECT and FD as `rt::fileio` takes them.
    pub(super) fn file_desc(&self, k: usize) -> FileOf<'p> {
        let p = self.p;
        let d = &p.services.files[k];
        let (offset, size) = p.storage.file_areas[k];
        let base = match p.services.scope.areas.iter().find(|&&(f, _)| usize::from(f) == k) {
            Some(&(_, record)) => self.linkage[usize::from(record)].unwrap_or_default(),
            None => self.base,
        };
        File {
            index: k,
            name: self.sym(d.name),
            assign: self.sym(d.assign),
            organization: d.organization,
            access: d.access,
            optional: d.optional,
            format: d.format,
            status: d.status.map(|(q, _)| Handle::Place(q)),
            relative: d.relative.as_ref().map(Handle::Relative),
            linage: d.linage.as_ref().map(|l| fileio::Linage {
                lines: &l.lines,
                footing: l.footing.as_ref(),
                top: l.top.as_ref(),
                bottom: l.bottom.as_ref(),
                counter: l.counter.map(|(q, _)| self.static_loc(q)),
            }),
            carriage: d.carriage,
            area: (base + offset as usize, size as usize),
            read_lengths: (d.read_lengths.0 as usize, d.read_lengths.1 as usize),
            depending: d.depending.map(|r| fileio::Depending { item: Handle::Place(r.item), lengths: (r.lengths.0 as usize, r.lengths.1 as usize) }),
        }
    }

    /// A place of the slab that lowering made static, located without evaluating anything.
    pub(super) fn static_loc(&self, place: PlaceId) -> Loc {
        let q = &self.p.places[place as usize];
        Loc { offset: self.base + q.offset as usize, len: q.len as usize, kind: q.kind, item: place as usize }
    }

    /// File k's EXCEPTION/ERROR procedure: its own, else the one for the mode it is open in or
    /// being opened in.
    pub(super) fn error_procedure(&self, k: usize, mode: Option<OpenMode>) -> Option<RangeId> {
        let services = &self.p.services;
        services.files[k].error.or_else(|| mode.and_then(|m| services.declaratives.modes[mode_index(m)]))
    }

    /// One file statement on one file.
    pub(super) fn file(&mut self, op: &'p FileOp, at: DebugId) -> R<Step> {
        let arm = Io { vm: self }.statement(op, at);
        self.concluded(arm.map(|arm| if op.arms() == 0 { Step::Next } else { Step::Arm(arm.unwrap_or(0)) }))
    }

    /// A statement's result as the walker's `exec` takes it: a declarative procedure that left the
    /// statement leaves it by that procedure's transfer.
    pub(super) fn concluded(&mut self, result: Result<Step, Abend>) -> R<Step> {
        match self.settle(result).map_err(Stop::halt) {
            Err(Halt::Abend(a)) if a.code == AbendCode::Signal(Signal::DeclarativeExit) => Ok(self.io.leaving.take().unwrap_or(Step::Next)),
            other => Ok(other?),
        }
    }

    /// Runs a declarative, SORT or MERGE procedure one PERFORM deeper than the statement, which
    /// runs at its own depth again however the procedure ends.
    pub(super) fn procedure(&mut self, range: RangeId, arrival: Arrival, pos: Pos) -> Result<Exit, Abend> {
        let depth = self.unit.depth;
        self.unit.enter(pos)?;
        let exit = self.run_procedure(range, arrival);
        self.unit.depth = depth;
        self.lift(exit, pos)
    }

    /// FROM's MOVE into the record, located already as its receiver.
    pub(super) fn move_from(&mut self, from: &FromMove, dest: Loc, at: DebugId) -> R<()> {
        self.move_to(Some(from.check), from.from, dest, &from.plan, at)
    }

    /// Abandons the statement running a declarative procedure, which left by `step`.
    pub(super) fn leave_statement(&mut self, step: Step, pos: Pos) -> Abend {
        self.io.leaving = Some(step);
        Abend { code: AbendCode::Signal(Signal::DeclarativeExit), message: String::new(), pos, file: None }
    }

    fn run_error_procedure(&mut self, range: RangeId, pos: Pos) -> Result<(), Abend> {
        match self.procedure(range, Arrival::Use, pos)? {
            Exit::Completed => Ok(()),
            Exit::Left(step) => Err(self.leave_statement(step, pos)),
            Exit::End(e) => Err(self.leave_statement(Step::End(e), pos)),
        }
    }
}

impl<'p, L: Loader<Rc<Code>>> Io<'_, 'p, '_, '_, L> {
    /// The verb, then the arm of the phrase its status selects: 1 ON, 2 NOT ON, 3 END-OF-PAGE,
    /// 4 NOT END-OF-PAGE; None when no phrase written runs.
    fn statement(&mut self, op: &'p FileOp, at: DebugId) -> Result<Option<u8>, Abend> {
        let k = usize::from(op.file);
        let pos = self.vm.pos(at);
        let file = self.vm.file_desc(k);
        match &op.verb {
            FileVerb::Open(mode) => {
                let outcome = fileio::open(self, &file, *mode, pos)?;
                self.settle(&file, outcome, None, pos)
            }
            FileVerb::Close => {
                let outcome = fileio::close(self, &file, None, pos)?;
                self.settle(&file, outcome, None, pos)
            }
            FileVerb::CloseWith(closing) => {
                let outcome = fileio::close(self, &file, Some(*closing), pos)?;
                self.settle(&file, outcome, None, pos)
            }
            FileVerb::Read { sequential, previous, into, key } => {
                // Without KEY a READ reads by the prime key, key 0, so only an alternate is named.
                let keys = self.vm.p.services.files[k].keys.as_ref();
                let alternate = usize::from(*key).checked_sub(1).and_then(|n| keys?.alternates.get(n));
                let key = alternate.map(|&(span, _)| Handle::Key(usize::from(*key), span));
                let read = Read { sequential: *sequential, previous: *previous, into: into.map(|(q, _)| Handle::Place(q)), key };
                match fileio::read(self, &file, read, pos)? {
                    // Lowering keeps only the phrase `sequential` names; a status for the other runs none.
                    Outcome::Status { status, at_end } => {
                        let phrase = op.phrase.filter(|_| at_end == *sequential);
                        self.conclude(&file, status, phrase, if at_end { '1' } else { '2' }, "READ", pos)
                    }
                    outcome => self.settle(&file, outcome, None, pos),
                }
            }
            FileVerb::Write { record, from, advancing } => {
                let loc = self.record(*record, from.as_ref(), at)?;
                match fileio::write(self, &file, loc, advancing.as_ref().map(advance), pos)? {
                    Outcome::Status { status, .. } => self.conclude(&file, status, op.phrase, '2', "WRITE", pos),
                    outcome => self.settle(&file, outcome, op.end_of_page, pos),
                }
            }
            FileVerb::Rewrite { record, from } => {
                let loc = self.record(*record, from.as_ref(), at)?;
                let status = fileio::rewrite(self, &file, loc, pos)?;
                self.conclude(&file, status, op.phrase, '2', "REWRITE", pos)
            }
            FileVerb::Delete => {
                let status = fileio::delete(self, &file, pos)?;
                self.conclude(&file, status, op.phrase, '2', "DELETE", pos)
            }
            FileVerb::Start { rel, key } => {
                let key = match key {
                    StartKey::Prime | StartKey::RelativeKey => None,
                    StartKey::Named { key, span } => Some(Handle::Key(usize::from(*key), *span)),
                    StartKey::Relative(value) => Some(Handle::Int(value)),
                };
                let status = fileio::start(self, &file, *rel, key, pos)?;
                self.conclude(&file, status, op.phrase, '2', "START", pos)
            }
        }
    }

    /// The record WRITE or REWRITE writes, located once FROM has moved into it.
    fn record(&mut self, record: PlaceId, from: Option<&FromMove>, at: DebugId) -> Result<Loc, Abend> {
        let pos = self.vm.pos(at);
        let located = match from {
            Some(f) => self.vm.loc(f.to).and_then(|dest| self.vm.move_from(f, dest, at)).and_then(|()| self.vm.loc(record)),
            None => self.vm.loc(record),
        };
        self.vm.lift(located, pos)
    }

    /// `conclude`: FILE STATUS and the phrase of the status's class, AT END ('1') or INVALID KEY
    /// ('2'), or NOT on success; with no such phrase written, the file's error path.
    pub(super) fn conclude(&mut self, file: &FileOf<'p>, status: FileStatus, phrase: Option<Phrase>, class: char, verb: &str, pos: Pos) -> Result<Option<u8>, Abend> {
        let arm = match phrase {
            Some(p) if status.covers('0') => p.not_on.then_some(2),
            Some(p) if status.covers(class) => p.on.then_some(1),
            _ => None,
        };
        if arm.is_some() {
            fileio::set_status(self, file, status, pos)?;
            return Ok(arm);
        }
        let message = format!("{verb} {}: file status {}: {}", file.name, status.as_str(), status.meaning());
        self.io_status(file, status, message, pos)?;
        Ok(None)
    }

    /// `settle`: what a verb that returned no status for its phrases leaves to run.
    pub(super) fn settle(&mut self, file: &FileOf<'p>, outcome: Outcome, end_of_page: Option<Phrase>, pos: Pos) -> Result<Option<u8>, Abend> {
        Ok(match outcome {
            Outcome::Failed(f) => {
                self.io_failure(file, f.status, f.mode, f.message, pos)?;
                None
            }
            Outcome::Page { end_of_page: true } => end_of_page.filter(|p| p.on).map(|_| 3),
            Outcome::Page { end_of_page: false } => end_of_page.filter(|p| p.not_on).map(|_| 4),
            Outcome::Done | Outcome::Status { .. } => None,
        })
    }

    fn io_status(&mut self, file: &FileOf<'p>, status: FileStatus, message: String, pos: Pos) -> Result<(), Abend> {
        let mode = self.slot(file.index).as_ref().map(|f| f.mode);
        self.io_failure(file, status, mode, message, pos)
    }

    /// `io_failure`: FILE STATUS, then for a failing status the file's EXCEPTION/ERROR procedure,
    /// or a containing program's GLOBAL one, or with none and no FILE STATUS the run's end.
    pub(super) fn io_failure(&mut self, file: &FileOf<'p>, status: FileStatus, mode: Option<OpenMode>, message: String, pos: Pos) -> Result<(), Abend> {
        fileio::set_status(self, file, status, pos)?;
        if status.covers('0') {
            return Ok(());
        }
        let k = file.index;
        self.vm.io.failed = Some(k);
        if let Some(range) = self.vm.error_procedure(k, mode) {
            return self.vm.run_error_procedure(range, pos);
        }
        if self.vm.global_procedure(k, mode, pos)? {
            return Ok(());
        }
        if file.status.is_none() && status.ends_the_run() {
            return Err(Abend { code: AbendCode::Io(status), message, pos, file: None });
        }
        Ok(())
    }

    /// WRITE ... AFTER ADVANCING of a report line, as `write_stream` writes one.
    pub(super) fn write_line(&mut self, k: usize, loc: Loc, space: Spacing, pos: Pos) -> Result<(), Abend> {
        let file = self.vm.file_desc(k);
        let outcome = fileio::write_stream(self, &file, loc, false, space, pos)?;
        self.settle(&file, outcome, None, pos).map(drop)
    }

    fn not_a_place(&mut self, pos: Pos) -> Abend {
        let Err(stopped) = self.vm.lift::<Infallible>(Err(not_yet("a file key or integer used as a data item")), pos);
        stopped
    }
}

impl<'p, L: Loader<Rc<Code>>> Host<Handle<'p>> for Io<'_, 'p, '_, '_, L> {
    type Facts = Facts<'p>;

    fn facts(&self) -> Facts<'p> {
        self.vm.facts()
    }

    fn mem(&mut self) -> &mut [u8] {
        &mut self.vm.unit.mem
    }

    fn taint(&mut self) -> Option<&mut crate::taint::Taint> {
        self.vm.unit.taint.as_mut()
    }

    fn locate(&mut self, handle: Handle<'p>, receiving: bool) -> Result<Loc, Abend> {
        match handle {
            Handle::Place(place) | Handle::Relative(&RelativeKey { place, .. }) => Host::<PlaceId>::locate(&mut *self.vm, place, receiving),
            Handle::Key(..) | Handle::Int(_) => Err(self.not_a_place(Pos::default())),
        }
    }

    fn integer(&mut self, handle: Handle<'p>, pos: Pos) -> Result<i64, Abend> {
        let n = match handle {
            Handle::Place(place) => self.vm.int_place(place, pos),
            Handle::Relative(r) => self.vm.int(&r.value, pos),
            Handle::Int(value) => self.vm.int(value, pos),
            Handle::Key(..) => return Err(self.not_a_place(pos)),
        };
        self.vm.lift(n, pos)
    }

    fn assign(&mut self, dest: Loc, val: Val, src: Option<Loc>, pos: Pos) -> Result<(), Abend> {
        store::assign(&self.vm.facts(), self.vm.unit, dest, val, src, pos)
    }

    fn store_fixed(&mut self, dest: Loc, value: &Fixed, pos: Pos) -> Result<(), Abend> {
        store::store_fixed(&self.vm.facts(), self.vm.unit, dest, value, false, pos)
    }
}

impl<'p, L: Loader<Rc<Code>>> Files<Handle<'p>, &'p IntExpr> for Io<'_, 'p, '_, '_, L> {
    fn slot(&mut self, k: usize) -> &mut Option<Open> {
        self.vm.unit.file(self.vm.me, k)
    }

    fn locked(&mut self, k: usize) -> &mut bool {
        self.vm.unit.locked(self.vm.me, k)
    }

    fn dd(&self, assign: &str) -> Option<Dd> {
        self.vm.unit.dds.get(assign)
    }

    fn notify(&mut self, event: Event<'_>) {
        self.vm.unit.notify(event);
    }

    fn int(&mut self, value: &'p IntExpr, pos: Pos) -> Result<i64, Abend> {
        let n = self.vm.int(value, pos);
        self.vm.lift(n, pos)
    }

    fn keying(&mut self, k: usize, _pos: Pos) -> Result<Keying, Abend> {
        let d = &self.vm.p.services.files[k];
        Ok(match (d.organization, &d.keys) {
            (Organization::Relative, _) => Keying::Relative,
            (Organization::Indexed, Some(keys)) => Keying::Indexed { prime: key_span(keys.prime), alternates: keys.alternates.iter().map(|&(span, duplicates)| (key_span(span), duplicates)).collect() },
            _ => Keying::Position,
        })
    }

    fn key_value(&mut self, k: usize, _keying: &Keying, key: Handle<'p>, _partial: bool, pos: Pos) -> Result<(usize, Vec<u8>), Abend> {
        let Handle::Key(which, span) = key else { return Err(self.not_a_place(pos)) };
        let (offset, size) = self.vm.file_desc(k).area;
        let span = key_span(span);
        if let Some(taint) = self.taint() {
            taint.read(offset + span.offset, span.len);
        }
        Ok((which, span.of(&self.vm.unit.mem[offset..offset + size])))
    }
}
