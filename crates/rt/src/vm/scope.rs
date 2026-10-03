//! EXTERNAL and GLOBAL (lir.md §9.16), as machine/scope.rs runs them: the records and file
//! connectors an activation binds from the run unit and from the programs containing it, and a
//! containing program's GLOBAL EXCEPTION/ERROR procedure run over its storage for a file of a
//! program it contains.

use super::files::mode_index;
use super::flow::{Arrival, Exit};
use super::{Code, Halt, Lowered, Vm};
use crate::abend::{Abend, Ending};
use crate::lir::{Binding, GlobalAt, Program, RangeId, Section, Step, SymId};
use crate::unit::{Connector, Loader};
use crate::vocab::{OpenMode, Pos};
use std::rc::Rc;

/// A program containing the running one, as it was when control left it for a program it
/// contains.
#[derive(Clone)]
pub(super) struct Container<'p> {
    code: &'p Lowered,
    me: usize,
    base: usize,
    local_base: usize,
    linkage: Vec<Option<usize>>,
}

impl Container<'_> {
    fn id(&self) -> &str {
        text(&self.code.program, self.code.program.id)
    }
}

fn text(p: &Program, id: SymId) -> &str {
    &p.symbols[id as usize]
}

/// The PROGRAM-ID of the program that declares file k of `p`: a containing program that declares
/// it GLOBAL, else `p`.
fn declarer(p: &Program, k: usize) -> &str {
    let declared_in = p.services.scope.files.iter().find(|f| usize::from(f.file) == k).and_then(|f| f.declared_in);
    text(p, declared_in.unwrap_or(p.id))
}

impl<'p, L: Loader<Rc<Code>>> Vm<'p, '_, '_, L> {
    /// The running programs that contain `callee`, innermost first: this one, and those
    /// containing it.
    pub(super) fn containers_of(&self, callee: &Program) -> Vec<Container<'p>> {
        let ids = &callee.services.scope.containers;
        if ids.is_empty() {
            return Vec::new();
        }
        let me = Container { code: self.code, me: self.me, base: self.base, local_base: self.local_base, linkage: self.linkage.clone() };
        let running: Vec<Container<'p>> = std::iter::once(me).chain(self.containers.iter().cloned()).collect();
        ids.iter().filter_map(|&id| running.iter().find(|f| f.id() == text(callee, id)).cloned()).collect()
    }

    fn frame(&self, id: &str) -> Result<&Container<'p>, Abend> {
        self.containers.iter().find(|f| f.id() == id).ok_or_else(|| {
            Abend::ironwork(format!("{} uses the GLOBAL names of {id}, which contains it and is not running", self.sym(self.p.id)), Pos::default())
        })
    }

    /// Gives each record the run unit or a containing program holds its address, and each
    /// EXTERNAL or GLOBAL file its connector.
    pub(super) fn bind_shared(&mut self) -> Result<(), Abend> {
        let p = self.p;
        let scope = &p.services.scope;
        let external = |m: String| Abend::ironwork(m, Pos::default());
        for (ordinal, binding) in &scope.records {
            let address = match *binding {
                Binding::External { name, size } => Some(self.unit.external(self.sym(name), false, size as usize, p.options.options.dialect).map_err(external)?),
                Binding::ExternalFile(k) => {
                    let k = usize::from(k);
                    Some(self.unit.external(self.sym(p.services.files[k].name), true, p.storage.file_areas[k].1 as usize, p.options.options.dialect).map_err(external)?)
                }
                Binding::Global { program, section, name } => self.global_address(self.sym(program), section, self.sym(name))?,
            };
            self.linkage[usize::from(*ordinal)] = address;
        }
        for shared in &scope.files {
            let k = usize::from(shared.file);
            let file = &p.services.files[k];
            let name = self.sym(file.name);
            let to = match shared.declared_in {
                _ if shared.external => self.unit.external_file(name),
                None => continue,
                Some(declarer) => {
                    let declarer = self.sym(declarer);
                    let frame = self.frame(declarer)?;
                    let theirs = &frame.code.program;
                    let own = |j: usize| !theirs.services.scope.files.iter().any(|f| usize::from(f.file) == j && f.declared_in.is_some());
                    let Some(j) = (0..theirs.services.files.len()).find(|&j| text(theirs, theirs.services.files[j].name) == name && own(j)) else {
                        return Err(Abend::ironwork(format!("{declarer} has no file {name}"), Pos::default()));
                    };
                    if file.carriage != theirs.services.files[j].carriage {
                        let message = format!("{name}, a GLOBAL file of {declarer}, is written as a print file in one of {declarer} and {} and not the other, which is not supported yet", self.sym(p.id));
                        return Err(Abend::ironwork(message, Pos::default()));
                    }
                    Connector::Program(frame.me, j)
                }
            };
            self.unit.connect(self.me, k, to);
        }
        Ok(())
    }

    /// Where containing program `id` holds its GLOBAL `name` of `section`; None for a LINKAGE
    /// record of that program with no address yet.
    fn global_address(&self, id: &str, section: Section, name: &str) -> Result<Option<usize>, Abend> {
        let frame = self.frame(id)?;
        let theirs = &frame.code.program;
        let Some(global) = theirs.services.scope.globals.iter().find(|g| g.section == section && text(theirs, g.name) == name) else {
            let record = if section == Section::File { "" } else { name };
            return Err(Abend::ironwork(format!("{id} has no GLOBAL record {record}"), Pos::default()));
        };
        Ok(match global.at {
            GlobalAt::Program(offset) => Some(frame.base + offset as usize),
            GlobalAt::Local(offset) => Some(frame.local_base + offset as usize),
            GlobalAt::Linkage(record) => frame.linkage[usize::from(record)],
        })
    }

    /// With no EXCEPTION/ERROR procedure of its own for file k, the first GLOBAL one of a
    /// containing program, innermost out: for the file, then for the mode it is open in. It runs
    /// as its own program's, and true once it has.
    pub(super) fn global_procedure(&mut self, k: usize, mode: Option<OpenMode>, pos: Pos) -> Result<bool, Abend> {
        let p = self.p;
        let (name, declared) = (text(p, p.services.files[k].name), declarer(p, k));
        let found = self.containers.iter().enumerate().find_map(|(n, frame)| {
            let theirs = &frame.code.program;
            let scope = &theirs.services.scope;
            let file = (0..theirs.services.files.len()).find(|&j| text(theirs, theirs.services.files[j].name) == name && declarer(theirs, j) == declared);
            let range = file.and_then(|j| scope.global_files.iter().find(|&&(f, _)| usize::from(f) == j)).map(|&(_, r)| r);
            range.or_else(|| mode.and_then(|m| scope.global_modes[mode_index(m)])).map(|r| (n, r))
        });
        let Some((n, range)) = found else { return Ok(false) };
        self.run_global_procedure(n, range, pos)?;
        Ok(true)
    }

    /// Runs procedure `range` of the n-th containing program as that program would, over its
    /// storage, one PERFORM deeper than the statement.
    fn run_global_procedure(&mut self, n: usize, range: RangeId, pos: Pos) -> Result<(), Abend> {
        let frame = self.containers[n].clone();
        let outer = self.containers[n + 1..].to_vec();
        let (me, program) = (self.sym(self.p.id), text(&frame.code.program, frame.code.program.id));
        let ran = {
            let mut vm = Vm::over(frame.code, frame.me, frame.base, &mut *self.unit, false, outer);
            (vm.linkage, vm.first) = (frame.linkage, self.first);
            vm.local_base = frame.local_base;
            let depth = vm.unit.depth;
            let ran = match vm.unit.enter(pos) {
                Ok(()) => vm.run_procedure(range, Arrival::Use),
                Err(abend) => Err(abend.into()),
            };
            vm.unit.depth = depth;
            match (ran, vm.pending.take()) {
                (_, Some(what)) => Err(Halt::Unimplemented(what)),
                (ran, None) => ran,
            }
        };
        match self.lift(ran, pos)? {
            Exit::Completed => Ok(()),
            Exit::End(Ending::StopRun) | Exit::Left(Step::End(Ending::StopRun)) => Err(self.leave_statement(Step::End(Ending::StopRun), pos)),
            Exit::End(_) | Exit::Left(_) => Err(Abend::ironwork(
                format!("the GLOBAL EXCEPTION/ERROR procedure of {program}, run for {me}, left by GO TO, GOBACK or EXIT PROGRAM, which is not supported yet"),
                pos,
            )),
        }
    }
}
