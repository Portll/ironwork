//! EXTERNAL and GLOBAL (Language Reference SC27-8713-03, pp. 63-66, 184-185, 197): where each may
//! be written, and what a contained program sees of the programs containing it. A GLOBAL record,
//! and an EXTERNAL one, is laid out as a LINKAGE record whose address the run unit gives it; see
//! [`Binding`]. Assumptions C180 and C181 hold what the manuals leave open.

use crate::layout::{Binding, Layout, Resolved, Section};
use syntax::ast::*;
use syntax::Error;

/// The GLOBAL records and files of the programs containing a program, as it compiles them.
#[derive(Default)]
pub(crate) struct Inherited {
    /// Their entries, laid out after the program's own LINKAGE records.
    pub entries: Vec<DataEntry>,
    /// For each of their 01 records, in order, where it is, the declaring program's depth, and
    /// for a file's record the file among the program's own.
    records: Vec<(Binding, u8, Option<usize>)>,
}

/// The rules for EXTERNAL and GLOBAL in a program's own DATA DIVISION.
pub(crate) fn rules(program: &Program, errors: &mut Vec<Error>) {
    let contains = !program.nested.is_empty();
    let sections: [(&str, &[DataEntry]); 3] = [("WORKING-STORAGE", &program.working_storage), ("LOCAL-STORAGE", &program.local_storage), ("LINKAGE", &program.linkage)];
    let mut externals: Vec<&str> = Vec::new();
    let mut globals: Vec<&str> = Vec::new();
    for (section, entries) in sections {
        let mut record: Option<&DataEntry> = None;
        for e in entries {
            if matches!(e.level, 1 | 77) {
                record = Some(e);
            }
            let name = e.name.as_deref().unwrap_or("FILLER");
            if e.external {
                if section != "WORKING-STORAGE" {
                    errors.push(syntax::messages::IWC0189.at(e.pos, format!("{name}: EXTERNAL is not allowed in the {section} SECTION")));
                } else if e.level != 1 {
                    errors.push(syntax::messages::IWC0190.at(e.pos, format!("{name}: EXTERNAL goes on a level-01 entry")));
                } else if e.redefines.is_some() {
                    errors.push(syntax::messages::IWC0191.at(e.pos, format!("{name}: EXTERNAL and REDEFINES cannot be in the same entry")));
                } else if e.name.is_none() {
                    errors.push(syntax::messages::IWC0192.at(e.pos, "an EXTERNAL record needs a data-name, not FILLER"));
                } else if externals.contains(&name) {
                    errors.push(syntax::messages::IWC0193.at(e.pos, format!("{name}: another EXTERNAL record of the program has the same name")));
                } else {
                    externals.push(name);
                }
            }
            if e.global {
                if e.level != 1 {
                    errors.push(syntax::messages::IWC0194.at(e.pos, format!("{name}: GLOBAL goes on a level-01 entry")));
                } else if e.name.is_none() {
                    errors.push(syntax::messages::IWC0195.at(e.pos, "a GLOBAL record needs a data-name, not FILLER"));
                } else if globals.contains(&name) {
                    errors.push(syntax::messages::IWC0196.at(e.pos, format!("{name}: another GLOBAL record of the DATA DIVISION has the same name")));
                } else {
                    globals.push(name);
                }
            }
            if let Some(r) = record.filter(|r| r.external && e.level != 88 && e.value.is_some()) {
                errors.push(syntax::messages::IWC0197.at(e.pos, format!("{name}: an item of EXTERNAL record {} takes no VALUE clause", r.name.as_deref().unwrap_or_default())));
            }
            if contains && !e.indexed_by.is_empty() && record.is_some_and(|r| r.global) {
                errors.push(syntax::messages::IWR0015.at(e.pos, format!("{name}: INDEXED BY in a GLOBAL record, in a program that contains others, is not supported yet")));
            }
        }
    }
    for f in &program.files {
        if (f.external || f.global) && f.records.iter().any(|e| e.level == 1 && e.name.is_none()) {
            errors.push(syntax::messages::IWC0198.at(f.pos, format!("FD {}: a record of an EXTERNAL or GLOBAL file needs a data-name, not FILLER", f.name)));
        }
        if f.external && f.linage.is_some() {
            errors.push(syntax::messages::IWR0016.at(f.pos, format!("FD {}: LINAGE on an EXTERNAL file is not supported yet", f.name)));
        }
        if f.external && !f.reports.is_empty() {
            errors.push(syntax::messages::IWR0017.at(f.pos, format!("FD {}: REPORT on an EXTERNAL file is not supported yet", f.name)));
        }
        if contains && f.global && (f.linage.is_some() || !f.reports.is_empty()) {
            errors.push(syntax::messages::IWR0018.at(f.pos, format!("FD {}: LINAGE or REPORT on a GLOBAL file, in a program that contains others, is not supported yet", f.name)));
        }
        if contains && !f.global && let Some(e) = f.records.iter().find(|e| e.global) {
            errors.push(syntax::messages::IWR0019.at(e.pos, format!("{}: a GLOBAL record of FD {}, which is not GLOBAL, in a program that contains others, is not supported yet", e.name.as_deref().unwrap_or("FILLER"), f.name)));
        }
    }
}

/// Takes in the GLOBAL records and files of the programs containing `program`, innermost first; a
/// file-name the program or a nearer program declares hides a farther one (pp. 66, 556 of the
/// Programming Guide SC27-8714-03). Their files join the program's own, after them.
pub(crate) fn inherit(program: &mut Program) -> Inherited {
    let mut inherited = Inherited::default();
    let containers = std::mem::take(&mut program.containers);
    for (n, c) in containers.iter().enumerate() {
        let depth = u8::try_from(n + 1).unwrap_or(u8::MAX);
        let sections = [(Section::WorkingStorage, &c.working_storage), (Section::LocalStorage, &c.local_storage), (Section::Linkage, &c.linkage)];
        for (section, entries) in sections {
            for e in entries {
                if matches!(e.level, 1 | 77) {
                    let record = e.name.clone().unwrap_or_default();
                    let binding = if e.external { Binding::External { name: record, size: 0 } } else { Binding::Global { program: c.id.clone(), record, section: section.clone() } };
                    inherited.records.push((binding, depth, None));
                }
                inherited.entries.push(e.clone());
            }
        }
        for f in &c.files {
            if program.files.iter().any(|own| own.name == f.name) {
                continue;
            }
            let k = program.files.len();
            let mut copy = f.clone();
            copy.declared_in = Some(c.id.clone());
            copy.linage = None;
            copy.reports.clear();
            for e in std::mem::take(&mut copy.records) {
                if matches!(e.level, 1 | 77) {
                    let binding = if f.external { Binding::ExternalFile(k as u16) } else { Binding::Global { program: c.id.clone(), record: String::new(), section: Section::File(f.name.clone()) } };
                    inherited.records.push((binding, depth, Some(k)));
                }
                inherited.entries.push(e);
            }
            program.files.push(copy);
        }
    }
    program.containers = containers;
    inherited
}

/// Gives the inherited records, which follow the program's `own` LINKAGE records, their places,
/// and an inherited file its record area.
pub(crate) fn bind(layout: &mut Layout, own: usize, inherited: Inherited, files: &[FileDecl]) {
    for (n, (binding, depth, file)) in inherited.records.into_iter().enumerate() {
        let ordinal = own + n;
        let Some(&root) = layout.linkage_roots.get(ordinal) else { continue };
        let size = layout.items[root].size;
        layout.bindings[ordinal] = match binding {
            Binding::External { name, .. } => Binding::External { name, size },
            other => other,
        };
        layout.depths[ordinal] = depth;
        let Some(k) = file else { continue };
        for i in 0..layout.items.len() {
            if layout.items[i].linkage == Some(ordinal as u16) {
                layout.items[i].file = Some(k as u16);
            }
        }
        layout.bound_areas[k].get_or_insert(ordinal as u16);
        let area = layout.file_areas[k].1.max(size).max(files[k].record_max.unwrap_or(0));
        layout.file_areas[k] = (0, area);
    }
}

/// The rules that need the layout: a record redefining an EXTERNAL one is no larger (p. 226), and
/// a GLOBAL file's FILE STATUS and keys, which the declaring program resolves, are its GLOBAL
/// names.
pub(crate) fn check(program: &Program, layout: &Layout, errors: &mut Vec<Error>) {
    for (&root, binding) in layout.linkage_roots.iter().zip(&layout.bindings) {
        let item = &layout.items[root];
        if let Binding::External { name, size } = binding
            && item.redefines.is_some()
            && item.size > *size
        {
            errors.push(syntax::messages::IWC0199.at(item.pos, format!("{}: {} bytes, larger than the EXTERNAL record {name} it redefines", item.name.as_deref().unwrap_or_default(), item.size)));
        }
    }
    for f in &program.files {
        let Some(declarer) = &f.declared_in else { continue };
        let refs = f.status.iter().map(|r| ("FILE STATUS", r)).chain(f.record_key.iter().map(|r| ("RECORD KEY", r))).chain(f.alternate_keys.iter().map(|(r, _)| ("ALTERNATE RECORD KEY", r))).chain(f.relative_key.iter().map(|r| ("RELATIVE KEY", r)));
        for (clause, r) in refs {
            if !declared_by(layout, r, declarer) {
                errors.push(syntax::messages::IWR0020.at(r.pos, format!("{}, a GLOBAL file of {declarer}: its {clause} {} is not a GLOBAL name of {declarer}, which is not supported yet", f.name, r.name)));
            }
        }
    }
}

fn declared_by(layout: &Layout, r: &Ref, program: &str) -> bool {
    let Ok(Resolved::Item(mut i)) = layout.resolve(&r.name, &r.qualifiers, r.pos) else { return false };
    while let Some(p) = layout.items[i].parent {
        i = p;
    }
    let binding = layout.items[i].linkage.and_then(|l| layout.bindings.get(l as usize));
    matches!(binding, Some(Binding::Global { program: p, .. }) if p == program)
}

/// SET ADDRESS OF gives an address to a LINKAGE record only, not to one the run unit places.
pub(crate) fn set_address(layout: &Layout, r: &Ref, errors: &mut Vec<Error>) {
    let Ok(Resolved::Item(i)) = layout.resolve(&r.name, &r.qualifiers, r.pos) else { return };
    let Some(l) = layout.items[i].linkage.filter(|_| layout.items[i].parent.is_none()) else { return };
    match &layout.bindings[l as usize] {
        Binding::Argument => {}
        Binding::Global { section: Section::Linkage, program, .. } => {
            errors.push(syntax::messages::IWR0021.at(r.pos, format!("SET ADDRESS OF {}, a GLOBAL LINKAGE record of {program}, in a program it contains is not supported yet", r.name)));
        }
        _ => errors.push(syntax::messages::IWC0200.at(r.pos, format!("SET ADDRESS OF {}: an EXTERNAL or GLOBAL record is not a LINKAGE record of the program", r.name))),
    }
}

/// How many LINKAGE records the program declares itself: its 01 and 77 entries there.
pub(crate) fn own_linkage(program: &Program) -> usize {
    program.linkage.iter().filter(|e| matches!(e.level, 1 | 77)).count()
}
