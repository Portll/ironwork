//! EXTERNAL and GLOBAL (lir.md §9.16): the bindings machine/scope.rs makes as a program is
//! activated, and what a program that contains others gives them, each found here once.

use super::{Lower, LowerError, R};
use crate::layout::{Binding, Section};
use rt::lir::{self, GlobalAt, RangeKind};
use syntax::Pos;
use syntax::ast::DataEntry;

fn ordinal(n: usize) -> R<u16> {
    u16::try_from(n).map_err(|_| LowerError::Exceeds("LINKAGE records", Pos::default()))
}

fn file_index(k: usize) -> R<u16> {
    u16::try_from(k).map_err(|_| LowerError::Exceeds("files", Pos::default()))
}

/// The index-names of the tables in the GLOBAL records among `entries`, or in every record when
/// `all`, as a GLOBAL file's records are.
fn global_index_names(entries: &[DataEntry], all: bool) -> impl Iterator<Item = &str> {
    let mut global = false;
    entries
        .iter()
        .filter(move |e| {
            if matches!(e.level, 1 | 77) {
                global = all || e.global;
            }
            global
        })
        .flat_map(|e| e.indexed_by.iter().map(String::as_str))
}

/// The names of the GLOBAL records among `entries`.
fn global_names(entries: &[DataEntry]) -> impl Iterator<Item = &str> {
    entries.iter().filter(|e| matches!(e.level, 1 | 77) && e.global).filter_map(|e| e.name.as_deref())
}

impl Lower<'_> {
    pub(super) fn scope(&mut self) -> R<lir::Scope> {
        let (layout, program) = (self.layout, self.program);
        let containers = program.containers.iter().map(|c| self.sym(&c.id)).collect();
        let mut records = Vec::new();
        for (n, binding) in layout.bindings.iter().enumerate() {
            let binding = match binding {
                Binding::Argument => continue,
                Binding::External { name, size } => lir::Binding::External { name: self.sym(name), size: *size },
                Binding::ExternalFile(k) => lir::Binding::ExternalFile(*k),
                Binding::Global { program, record, section } => {
                    let (section, name) = match section {
                        Section::WorkingStorage => (lir::Section::WorkingStorage, record),
                        Section::LocalStorage => (lir::Section::LocalStorage, record),
                        Section::Linkage => (lir::Section::Linkage, record),
                        Section::File(file) => (lir::Section::File, file),
                    };
                    lir::Binding::Global { program: self.sym(program), section, name: self.sym(name) }
                }
            };
            records.push((ordinal(n)?, binding));
        }
        let mut files = Vec::new();
        for (k, f) in program.files.iter().enumerate().filter(|(_, f)| f.external || f.declared_in.is_some()) {
            let declared_in = f.declared_in.as_deref().map(|d| self.sym(d));
            files.push(lir::SharedFile { file: file_index(k)?, external: f.external, declared_in });
        }
        let mut areas = Vec::new();
        for (k, area) in layout.bound_areas.iter().enumerate() {
            if let Some(record) = area {
                areas.push((file_index(k)?, *record));
            }
        }
        let callable = program.callable.iter().map(|n| self.sym(n)).collect();
        let hidden = program.hidden.iter().map(|n| self.sym(n)).collect();
        let mut scope = lir::Scope { containers, callable, hidden, records, files, areas, ..lir::Scope::default() };
        if !program.nested.is_empty() {
            scope.globals = self.globals()?;
            let table = &self.c.declaratives;
            for (k, span) in table.global_files.iter().enumerate() {
                if let Some(span) = span {
                    scope.global_files.push((file_index(k)?, self.span_range(*span, RangeKind::UseProcedure)?));
                }
            }
            for (mode, span) in scope.global_modes.iter_mut().zip(table.global_modes) {
                *mode = span.map(|s| self.span_range(s, RangeKind::UseProcedure)).transpose()?;
            }
        }
        Ok(scope)
    }

    /// Each GLOBAL record and file this program declares, where `global_address` finds it in an
    /// activation of this program: the first record of its name in its section, the first file.
    fn globals(&mut self) -> R<Vec<lir::Global>> {
        let (layout, program) = (self.layout, self.program);
        let root = |name: &str, local: bool| {
            layout.items.iter().find(|i| i.parent.is_none() && i.linkage.is_none() && i.file.is_none() && i.local == local && i.name.as_deref() == Some(name))
        };
        let mut found = Vec::new();
        for (entries, local) in [(&program.working_storage, false), (&program.local_storage, true)] {
            for name in global_names(entries) {
                if let Some(item) = root(name, local) {
                    let (section, at) = if local { (lir::Section::LocalStorage, GlobalAt::Local(item.offset)) } else { (lir::Section::WorkingStorage, GlobalAt::Program(item.offset)) };
                    found.push((section, name, at));
                }
            }
        }
        for name in global_names(&program.linkage) {
            if let Some(o) = layout.linkage_roots.iter().position(|&i| layout.items[i].name.as_deref() == Some(name)) {
                found.push((lir::Section::Linkage, name, GlobalAt::Linkage(ordinal(o)?)));
            }
        }
        // An index is a WORKING-STORAGE item whatever section its table is in.
        let sections = [&program.working_storage, &program.local_storage, &program.linkage].into_iter().map(|e| (e, false));
        let files = program.files.iter().filter(|f| f.global && f.declared_in.is_none()).map(|f| (&f.records, true));
        for (entries, all) in sections.chain(files) {
            for name in global_index_names(entries, all) {
                if let Some(item) = root(name, false) {
                    found.push((lir::Section::WorkingStorage, name, GlobalAt::Program(item.offset)));
                }
            }
        }
        for f in program.files.iter().filter(|f| f.global && f.declared_in.is_none()) {
            let Some(j) = program.files.iter().position(|g| g.name == f.name) else { continue };
            let at = match layout.bound_areas[j] {
                Some(record) => GlobalAt::Linkage(record),
                None => GlobalAt::Program(layout.file_areas[j].0),
            };
            found.push((lir::Section::File, &f.name, at));
        }
        Ok(found.into_iter().map(|(section, name, at)| lir::Global { section, name: self.sym(name), at }).collect())
    }
}
