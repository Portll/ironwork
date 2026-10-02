//! USE AFTER EXCEPTION/ERROR and USE FOR DEBUGGING procedures at compile time: the files, open
//! mode or procedures each serves, and the rules the Language Reference (SC27-8713-03) sets for
//! them. They run in `machine::declaratives`.

use crate::layout::{Layout, Resolved};
use numeric::Options;
use syntax::ast::*;
use syntax::{Error, Pos};

/// A section's paragraphs, first to last.
pub type Span = (usize, usize);

/// DEBUG-ITEM as the Language Reference describes it (p. 19), with a DEBUG-CONTENTS of 30
/// characters ([`numeric::assumptions::DEBUG_CONTENTS_LENGTH`]).
const DEBUG_ITEM: &str = concat!(
    "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. REGISTERS.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n",
    "       01  DEBUG-ITEM.\n",
    "           02  DEBUG-LINE PIC X(6).\n",
    "           02  FILLER PIC X VALUE SPACE.\n",
    "           02  DEBUG-NAME PIC X(30).\n",
    "           02  FILLER PIC X VALUE SPACE.\n",
    "           02  DEBUG-SUB-1 PIC S9999 SIGN IS LEADING SEPARATE CHARACTER.\n",
    "           02  FILLER PIC X VALUE SPACE.\n",
    "           02  DEBUG-SUB-2 PIC S9999 SIGN IS LEADING SEPARATE CHARACTER.\n",
    "           02  FILLER PIC X VALUE SPACE.\n",
    "           02  DEBUG-SUB-3 PIC S9999 SIGN IS LEADING SEPARATE CHARACTER.\n",
    "           02  FILLER PIC X VALUE SPACE.\n",
    "           02  DEBUG-CONTENTS PIC X(30).\n",
);

/// Where each field of DEBUG-ITEM starts, and its length.
pub const DEBUG_LINE: (usize, usize) = (0, 6);
pub const DEBUG_NAME: (usize, usize) = (7, 30);
pub const DEBUG_CONTENTS: (usize, usize) = (56, 30);

/// DEBUG-ITEM and its fields, which only a debugging section can reference (p. 771).
pub(crate) const DEBUG_ITEM_NAMES: [&str; 7] = ["DEBUG-ITEM", "DEBUG-LINE", "DEBUG-NAME", "DEBUG-SUB-1", "DEBUG-SUB-2", "DEBUG-SUB-3", "DEBUG-CONTENTS"];

/// Where each declarative runs.
#[derive(Debug, Default)]
pub struct Table {
    /// Each file's own EXCEPTION/ERROR procedure.
    pub files: Vec<Option<Span>>,
    /// The EXCEPTION/ERROR procedures for files open INPUT, OUTPUT, I-O and EXTEND.
    pub modes: [Option<Span>; 4],
    /// Those of `files` and `modes` declared GLOBAL, which serve the programs this one contains.
    pub global_files: Vec<Option<Span>>,
    pub global_modes: [Option<Span>; 4],
    /// Under the DEBUG runtime option: for each paragraph, the debugging section that runs before
    /// it and the name DEBUG-NAME gives. Empty otherwise.
    pub triggers: Vec<Option<(Span, String)>>,
    /// DEBUG-ITEM's item in the layout.
    pub debug_item: Option<usize>,
    /// Under ALL PROCEDURES, where each ALTER in the declaratives is: those run no debugging section.
    pub declarative_alters: Vec<Pos>,
}

pub fn mode_index(mode: OpenMode) -> usize {
    match mode {
        OpenMode::Input => 0,
        OpenMode::Output => 1,
        OpenMode::InputOutput => 2,
        OpenMode::Extend => 3,
    }
}

fn mode_name(mode: OpenMode) -> &'static str {
    ["INPUT", "OUTPUT", "I-O", "EXTEND"][mode_index(mode)]
}

/// A program with debugging sections gets DEBUG-ITEM after its own WORKING-STORAGE.
pub(crate) fn with_debug_item(mut program: Program) -> Program {
    if program.declaratives.debugging.is_empty() || program.working_storage.iter().any(|e| e.name.as_deref() == Some("DEBUG-ITEM")) {
        return program;
    }
    let Ok(registers) = syntax::parse(DEBUG_ITEM) else { return program };
    for mut entry in registers.working_storage {
        entry.pos = Pos::default();
        program.working_storage.push(entry);
    }
    program
}

/// The debugging sections, as spans of paragraphs.
pub(crate) fn debugging_sections(program: &Program) -> Vec<Span> {
    program.declaratives.debugging.iter().map(|u| (u.section, crate::section_end(program, u.section))).collect()
}

/// Resolves every USE AFTER EXCEPTION/ERROR and USE FOR DEBUGGING, with the rules of pp. 714-716.
pub(crate) fn resolve(program: &Program, layout: &Layout, options: &Options, errors: &mut Vec<Error>) -> Table {
    let span = |section: usize| (section, crate::section_end(program, section));
    let mut table = Table { files: vec![None; program.files.len()], global_files: vec![None; program.files.len()], ..Table::default() };
    for u in &program.declaratives.errors {
        match &u.on {
            ErrorUse::Files(names) => {
                for name in names {
                    let error = |why: &str| Error::at(u.pos, format!("USE AFTER EXCEPTION/ERROR ON {name}: {why}"));
                    match program.files.iter().position(|f| f.name == *name) {
                        None => errors.push(error("no file has that name")),
                        Some(k) if program.files[k].sort => errors.push(error("a sort or merge file takes no EXCEPTION/ERROR procedure")),
                        Some(k) if table.files[k].is_some() => errors.push(error("the file has another EXCEPTION/ERROR procedure")),
                        Some(k) => {
                            table.files[k] = Some(span(u.section));
                            if u.global {
                                table.global_files[k] = Some(span(u.section));
                            }
                        }
                    }
                }
            }
            ErrorUse::Mode(mode) => match &mut table.modes[mode_index(*mode)] {
                Some(_) => errors.push(Error::at(u.pos, format!("USE AFTER EXCEPTION/ERROR ON {}: another procedure is for the same open mode", mode_name(*mode)))),
                free => {
                    *free = Some(span(u.section));
                    if u.global {
                        table.global_modes[mode_index(*mode)] = Some(span(u.section));
                    }
                }
            },
        }
    }
    let debugging = debugging_sections(program);
    let in_debugging = |i: usize| debugging.iter().any(|&(first, last)| (first..=last).contains(&i));
    let uses = &program.declaratives.debugging;
    if let Some(u) = uses.first().filter(|_| options.thread) {
        errors.push(Error::at(u.pos, "USE FOR DEBUGGING is not allowed in a program compiled with THREAD"));
    }
    let every = uses.iter().filter(|u| u.procedures.is_empty()).count();
    if let Some(u) = uses.iter().find(|u| u.procedures.is_empty()).filter(|_| every > 1 || every < uses.len()) {
        errors.push(Error::at(u.pos, "USE FOR DEBUGGING ON ALL PROCEDURES: it may be written once, and no other USE FOR DEBUGGING may name a procedure"));
    }
    let mut triggers: Vec<Option<(Span, String)>> = vec![None; program.paragraphs.len()];
    for (u, &section) in uses.iter().zip(&debugging) {
        if u.procedures.is_empty() {
            for (i, p) in program.paragraphs.iter().enumerate() {
                if !p.name.is_empty() && !in_debugging(i) {
                    triggers[i] = Some((section, p.name.clone()));
                }
            }
            continue;
        }
        for name in &u.procedures {
            let error = |why: &str| Error::at(u.pos, format!("USE FOR DEBUGGING ON {}: {why}", name.name));
            match crate::procedure_from(program, name, section.0) {
                Err(m) => errors.push(error(&m)),
                Ok((i, _)) if in_debugging(i) => errors.push(error("the procedure is in a debugging section")),
                Ok((i, _)) if triggers[i].is_some() => errors.push(error("the procedure is named in another USE FOR DEBUGGING, or twice in this one")),
                Ok((i, _)) => triggers[i] = Some((section, debug_name(name))),
            }
        }
    }
    references(program, &debugging, errors);
    if options.debug && triggers.iter().any(Option::is_some) {
        table.triggers = triggers;
        if every > 0 {
            for p in program.paragraphs.iter().take(program.report_writer.procedure_start) {
                alters(&p.statements, &mut table.declarative_alters);
            }
        }
    }
    table.debug_item = match layout.resolve("DEBUG-ITEM", &[], Pos::default()) {
        Ok(Resolved::Item(i)) if !uses.is_empty() => Some(i),
        _ => None,
    };
    table
}

/// A procedure-name as DEBUG-NAME and DEBUG-CONTENTS give it, a qualifier after OF (p. 19).
pub fn debug_name(name: &ProcName) -> String {
    name.section.as_ref().map_or_else(|| name.name.clone(), |s| format!("{} OF {s}", name.name))
}

/// Where each ALTER in `stmts` is.
fn alters(stmts: &[Stmt], out: &mut Vec<Pos>) {
    for s in stmts {
        if let Stmt::Alter { pos, .. } = s {
            out.push(*pos);
        }
        for body in crate::oo::bodies(s) {
            alters(body, out);
        }
    }
}

/// Each PERFORM, GO TO, ALTER and SORT or MERGE procedure in `stmts`: the procedures it names,
/// whether it is a PERFORM, and where it is.
fn procedure_references<'a>(stmts: &'a [Stmt], out: &mut Vec<(&'a ProcName, Option<&'a ProcName>, bool, Pos)>) {
    for s in stmts {
        match s {
            Stmt::PerformProc { from, thru, pos, .. } => out.push((from, thru.as_ref(), true, *pos)),
            Stmt::GoTo { target: Some(target), pos } => out.push((target, None, false, *pos)),
            Stmt::GoToDepending { targets, pos, .. } => out.extend(targets.iter().map(|t| (t, None, false, *pos))),
            Stmt::Alter { pairs, pos } => out.extend(pairs.iter().flat_map(|(from, to)| [from, to]).map(|t| (t, None, false, *pos))),
            Stmt::Sorting(sorting) => {
                if let Sorting::Sort(st) = &**sorting {
                    for io in [&st.input, &st.output].into_iter().flatten() {
                        if let SortIo::Procedure { from, thru } = io {
                            out.push((from, thru.as_ref(), false, st.pos));
                        }
                    }
                }
            }
            _ => {}
        }
        for body in crate::oo::bodies(s) {
            procedure_references(body, out);
        }
    }
}

/// A debugging section refers to no nondeclarative procedure (p. 716), and nothing outside the
/// debugging sections refers into one (p. 771). A PERFORM ... THRU that names a declarative
/// procedure names two in the same declarative section (p. 418). See
/// [`numeric::assumptions::DEBUGGING_SECTION_REFERENCES`].
fn references(program: &Program, debugging: &[Span], errors: &mut Vec<Error>) {
    let declaratives_end = program.report_writer.procedure_start;
    if declaratives_end == 0 {
        return;
    }
    let debugging_section =|i: usize| debugging.iter().position(|&(first, last)| (first..=last).contains(&i));
    let section_of = |i: usize| (0..=i).rev().find(|&j| program.paragraphs[j].is_section);
    for (i, p) in program.paragraphs.iter().enumerate() {
        let mut named = Vec::new();
        procedure_references(&p.statements, &mut named);
        for (from, thru, perform, pos) in named {
            let first = |n: &ProcName| crate::procedure(program, n).ok().map(|(at, _)| at);
            let (Some(a), b) = (first(from), thru.and_then(first)) else { continue };
            for (target, name) in std::iter::once((a, from)).chain(b.zip(thru)) {
                if debugging_section(i).is_some() && target >= declaratives_end {
                    errors.push(Error::at(pos, format!("{}: a debugging section may refer only to declarative procedures", name.name)));
                } else if debugging_section(i).is_none() && debugging_section(target).is_some() {
                    errors.push(Error::at(pos, format!("{}: only a debugging section may refer to a procedure in a debugging section", name.name)));
                }
            }
            if let Some(b) = b.filter(|&b| perform && (a < declaratives_end || b < declaratives_end))
                && (b >= declaratives_end || a >= declaratives_end || section_of(a) != section_of(b))
            {
                errors.push(Error::at(pos, "PERFORM ... THRU: a declarative procedure and the other end of the range must be in the same declarative section"));
            }
        }
    }
}
