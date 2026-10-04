//! EXTERNAL and GLOBAL at run time: the records and file connectors a program shares with the run
//! unit or with the programs containing it, bound as it is activated, and the GLOBAL
//! EXCEPTION/ERROR procedures of those programs (Language Reference SC27-8713-03, pp. 63-66,
//! 184-185, 197, 714-715; assumptions C69, C180 and C181).

use super::*;
use crate::declaratives::{Span, mode_index};
use crate::layout::{Binding, Section};
use rt::unit::Connector;

/// A program containing the running one, as it was when control left it for a program it
/// contains.
#[derive(Clone)]
pub(super) struct Frame<'p> {
    compiled: &'p Compiled,
    me: usize,
    base: usize,
    local_base: usize,
    linkage: Vec<Option<usize>>,
}

impl<'p> Machine<'p, '_, '_> {
    /// The running programs that contain `callee`, innermost first: this one, and those
    /// containing it.
    pub(super) fn containers_of(&self, callee: &Program) -> Vec<Frame<'p>> {
        if callee.containers.is_empty() {
            return Vec::new();
        }
        let me = Frame { compiled: self.compiled, me: self.me, base: self.base, local_base: self.local_base, linkage: self.linkage.clone() };
        let running: Vec<Frame<'p>> = std::iter::once(me).chain(self.containers.iter().cloned()).collect();
        callee.containers.iter().filter_map(|c| running.iter().find(|f| f.compiled.program.id == c.id).cloned()).collect()
    }

    fn frame(&self, id: &str) -> R<&Frame<'p>> {
        self.containers.iter().find(|f| f.compiled.program.id == id).ok_or_else(|| {
            Abend::ironwork(format!("{} uses the GLOBAL names of {id}, which contains it and is not running", self.program.id), Pos::default())
        })
    }

    /// Gives each record the run unit or a containing program holds its address, and each
    /// EXTERNAL or GLOBAL file its connector.
    pub(super) fn bind_shared(&mut self) -> R<()> {
        let layout = self.layout;
        for (ordinal, binding) in layout.bindings.iter().enumerate() {
            let external = |m: String| Abend::ironwork(m, Pos::default());
            self.linkage[ordinal] = match binding {
                Binding::Argument => continue,
                Binding::External { name, size } => Some(self.unit.external(name, false, *size as usize, self.options.dialect_of(Switched::ExternalStorage)).map_err(external)?),
                Binding::ExternalFile(k) => {
                    let k = usize::from(*k);
                    Some(self.unit.external(&self.program.files[k].name, true, layout.file_areas[k].1 as usize, self.options.dialect_of(Switched::ExternalStorage)).map_err(external)?)
                }
                Binding::Global { program, record, section } => self.global_address(program, record, section)?,
            };
        }
        let program = self.program;
        for (k, f) in program.files.iter().enumerate() {
            if f.external {
                let to = self.unit.external_file(&f.name);
                self.unit.connect(self.me, k, to);
            } else if let Some(declarer) = &f.declared_in {
                let frame = self.frame(declarer)?;
                let theirs = &frame.compiled.program;
                let Some(j) = theirs.files.iter().position(|g| g.name == f.name && g.declared_in.is_none()) else {
                    return Err(Abend::ironwork(format!("{declarer} has no file {}", f.name), Pos::default()));
                };
                if self.carriage[k] != frame.compiled.carriage[j] {
                    let message = format!("{}, a GLOBAL file of {declarer}, is written as a print file in one of {declarer} and {} and not the other, which is not supported yet", f.name, program.id);
                    return Err(Abend::ironwork(message, Pos::default()));
                }
                let to = Connector::Program(frame.me, j);
                self.unit.connect(self.me, k, to);
            }
        }
        Ok(())
    }

    /// Where GLOBAL record `record` of the containing program `id` is, in `section`; None for a
    /// LINKAGE record of that program with no address yet.
    fn global_address(&self, id: &str, record: &str, section: &Section) -> R<Option<usize>> {
        let frame = self.frame(id)?;
        let layout = &frame.compiled.layout;
        let root = |local: bool| {
            layout.items.iter().find(|i| i.parent.is_none() && i.linkage.is_none() && i.file.is_none() && i.local == local && i.name.as_deref() == Some(record))
        };
        let missing = || Abend::ironwork(format!("{id} has no GLOBAL record {record}"), Pos::default());
        Ok(match section {
            Section::WorkingStorage => Some(frame.base + root(false).ok_or_else(missing)?.offset as usize),
            Section::LocalStorage => Some(frame.local_base + root(true).ok_or_else(missing)?.offset as usize),
            Section::Linkage => {
                let ordinal = layout.linkage_roots.iter().position(|&i| layout.items[i].name.as_deref() == Some(record)).ok_or_else(missing)?;
                frame.linkage[ordinal]
            }
            Section::File(name) => {
                let j = frame.compiled.program.files.iter().position(|f| f.name == *name).ok_or_else(missing)?;
                match layout.bound_areas[j] {
                    Some(ordinal) => frame.linkage[ordinal as usize],
                    None => Some(frame.base + layout.file_areas[j].0 as usize),
                }
            }
        })
    }

    /// With no EXCEPTION/ERROR procedure of its own for file k, the first GLOBAL one of a
    /// containing program, innermost out: for the file, then for the mode it is open in (p. 715).
    /// It runs as its own program's, and true once it has.
    pub(super) fn global_declarative(&mut self, k: usize, mode: Option<OpenMode>, pos: Pos) -> R<bool> {
        let program = self.program;
        let decl = &program.files[k];
        let declarer = decl.declared_in.as_deref().unwrap_or(&program.id);
        let found = self.containers.iter().enumerate().find_map(|(n, frame)| {
            let theirs = frame.compiled;
            let table = &theirs.declaratives;
            let file = theirs.program.files.iter().position(|g| g.name == decl.name && g.declared_in.as_deref().unwrap_or(&theirs.program.id) == declarer);
            let span = file.and_then(|j| table.global_files.get(j).copied().flatten()).or_else(|| mode.and_then(|m| table.global_modes[mode_index(m)]));
            span.map(|span| (n, span))
        });
        let Some((n, span)) = found else { return Ok(false) };
        self.run_global_declarative(n, span, pos)?;
        Ok(true)
    }

    /// Runs `span` of the n-th containing program as that program would, over its storage.
    fn run_global_declarative(&mut self, n: usize, span: Span, pos: Pos) -> R<()> {
        let frame = self.containers[n].clone();
        let outer = self.containers[n + 1..].to_vec();
        let (me, program) = (self.program.id.clone(), frame.compiled.program.id.clone());
        let ran = {
            let mut m = Machine::over(frame.compiled, frame.me, frame.base, &mut *self.unit, false);
            (m.linkage, m.first) = (frame.linkage, self.first);
            m.local_base = frame.local_base;
            m.containers = outer;
            m.run_error_declarative(span, pos).map_err(|a| (a, m.uses.leaving.take()))
        };
        match ran {
            Ok(()) => Ok(()),
            Err((Abend { code: AbendCode::Signal(Signal::DeclarativeExit), .. }, Some(Flow::End(Ending::StopRun)))) => {
                self.uses.leaving = Some(Flow::End(Ending::StopRun));
                Err(Abend { code: AbendCode::Signal(Signal::DeclarativeExit), message: String::new(), pos, file: None })
            }
            Err((Abend { code: AbendCode::Signal(Signal::DeclarativeExit), .. }, _)) => Err(Abend::ironwork(
                format!("the GLOBAL EXCEPTION/ERROR procedure of {program}, run for {me}, left by GO TO, GOBACK or EXIT PROGRAM, which is not supported yet"),
                pos,
            )),
            Err((a, _)) => Err(a),
        }
    }
}
