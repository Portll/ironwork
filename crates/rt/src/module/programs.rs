//! Writing and reading the programs of a module: the sections that split a `lir::Program` between them.

use super::codec::{Decode, Encode, Reader, Writer};
use super::{Module, ModuleError, ModuleWriter, Section, StringTable};
use crate::codec_struct;
use crate::lir::{
    AbendText, Block, Cond, Const, Debug, Expr, Item, ParaId, Paragraph, Place, Plans, Program, ProgramOptions, Range,
    Services, SqlEntry, Storage, SymId,
};
use crate::picture::Sym;

/// A program's line in the `DIRECTORY` section (load-module.md §6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectoryEntry {
    /// PROGRAM-ID exactly as written.
    pub id: String,
    /// The ordinal of the containing program.
    pub parent: Option<u32>,
    pub common: bool,
    /// ENTRY names and the paragraphs they enter.
    pub entries: Vec<(String, ParaId)>,
    /// USING: true for BY VALUE, in order.
    pub params: Vec<bool>,
    pub returning: bool,
    /// Visible to a dynamic CALL.
    pub dynamic: bool,
}

codec_struct!(DirectoryEntry { id, parent, common, entries, params, returning, dynamic });

impl DirectoryEntry {
    /// The entry for a top-level program that no parse of nesting or ENTRY has refined.
    pub fn top_level(program: &Program) -> Self {
        Self {
            id: program.symbols.get(program.id as usize).cloned().unwrap_or_default(),
            parent: None,
            common: false,
            entries: Vec::new(),
            params: vec![false; program.storage.using.len()],
            returning: program.storage.returning.is_some(),
            dynamic: true,
        }
    }
}

/// The programs of a module, in ordinal order, with their directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadedModule {
    pub directory: Vec<DirectoryEntry>,
    pub programs: Vec<Program>,
}

/// Every field of a `Program`, listed once so a new field is a compile error here.
struct Parts<'a> {
    id: &'a SymId,
    options: &'a ProgramOptions,
    initial: &'a bool,
    recursive: &'a bool,
    storage: &'a Storage,
    items: &'a Vec<Item>,
    paragraphs: &'a Vec<Paragraph>,
    procedure_start: &'a ParaId,
    ranges: &'a Vec<Range>,
    blocks: &'a Vec<Block>,
    places: &'a Vec<Place>,
    exprs: &'a Vec<Expr>,
    conds: &'a Vec<Cond>,
    consts: &'a Vec<Const>,
    plans: &'a Plans,
    services: &'a Services,
    sql: &'a Vec<SqlEntry>,
    abends: &'a Vec<AbendText>,
    edits: &'a Vec<Vec<Sym>>,
    symbols: &'a Vec<String>,
    debug: &'a Debug,
}

impl<'a> Parts<'a> {
    fn of(program: &'a Program) -> Self {
        let Program {
            id, options, initial, recursive, storage, items, paragraphs, procedure_start, ranges, blocks, places,
            exprs, conds, consts, plans, services, sql, abends, edits, symbols, debug,
        } = program;
        Self {
            id, options, initial, recursive, storage, items, paragraphs, procedure_start, ranges, blocks, places,
            exprs, conds, consts, plans, services, sql, abends, edits, symbols, debug,
        }
    }

    fn encode_lir(&self, w: &mut Writer) {
        self.id.encode(w);
        self.initial.encode(w);
        self.recursive.encode(w);
        self.paragraphs.encode(w);
        self.procedure_start.encode(w);
        self.ranges.encode(w);
        self.blocks.encode(w);
        self.places.encode(w);
        self.exprs.encode(w);
        self.conds.encode(w);
        self.consts.encode(w);
        self.plans.encode(w);
        self.services.encode(w);
        self.abends.encode(w);
        self.symbols.encode(w);
    }
}

/// The `LIR` section's share of a program.
struct Body {
    id: SymId,
    initial: bool,
    recursive: bool,
    paragraphs: Vec<Paragraph>,
    procedure_start: ParaId,
    ranges: Vec<Range>,
    blocks: Vec<Block>,
    places: Vec<Place>,
    exprs: Vec<Expr>,
    conds: Vec<Cond>,
    consts: Vec<Const>,
    plans: Plans,
    services: Services,
    abends: Vec<AbendText>,
    symbols: Vec<String>,
}

impl Decode for Body {
    fn decode(r: &mut Reader<'_>) -> Result<Self, ModuleError> {
        Ok(Self {
            id: Decode::decode(r)?,
            initial: Decode::decode(r)?,
            recursive: Decode::decode(r)?,
            paragraphs: Decode::decode(r)?,
            procedure_start: Decode::decode(r)?,
            ranges: Decode::decode(r)?,
            blocks: Decode::decode(r)?,
            places: Decode::decode(r)?,
            exprs: Decode::decode(r)?,
            conds: Decode::decode(r)?,
            consts: Decode::decode(r)?,
            plans: Decode::decode(r)?,
            services: Decode::decode(r)?,
            abends: Decode::decode(r)?,
            symbols: Decode::decode(r)?,
        })
    }
}

/// Program count, then one record per program.
fn per_program(w: &mut Writer, programs: &[Program], record: impl Fn(&Parts<'_>, &mut Writer)) {
    w.count(programs.len());
    for program in programs {
        record(&Parts::of(program), w);
    }
}

fn encode_module(programs: &[Program], directory: &[DirectoryEntry]) -> Vec<u8> {
    let mut m = ModuleWriter::new();
    m.section(Section::DIRECTORY, |w| {
        w.count(directory.len());
        for entry in directory {
            entry.encode(w);
        }
    });
    m.section(Section::OPTIONS, |w| per_program(w, programs, |p, w| p.options.encode(w)));
    m.section(Section::LAYOUT, |w| {
        per_program(w, programs, |p, w| {
            p.storage.encode(w);
            p.items.encode(w);
            p.edits.encode(w);
        });
    });
    m.section(Section::LIR, |w| per_program(w, programs, |p, w| p.encode_lir(w)));
    m.section(Section::SQL, |w| per_program(w, programs, |p, w| p.sql.encode(w)));
    m.section(Section::BMS, |w| w.count(0));
    m.section(Section::DEBUG, |w| per_program(w, programs, |p, w| p.debug.encode(w)));
    m.finish()
}

/// A module of `programs`, each a top-level program in the directory. Same input, same bytes.
pub fn write(programs: &[Program]) -> Vec<u8> {
    let directory: Vec<_> = programs.iter().map(DirectoryEntry::top_level).collect();
    encode_module(programs, &directory)
}

/// A module with the caller's directory, refused (as the reader would) if it or a program is invalid.
pub fn write_with(programs: &[Program], directory: &[DirectoryEntry]) -> Result<Vec<u8>, ModuleError> {
    check_directory(directory, programs)?;
    for program in programs {
        crate::lir::program_valid(program).map_err(|reason| bad("LIR", reason))?;
    }
    Ok(encode_module(programs, directory))
}

fn bad(section: &'static str, reason: impl Into<String>) -> ModuleError {
    ModuleError::Malformed { section, offset: 0, reason: reason.into() }
}

fn check_directory(directory: &[DirectoryEntry], programs: &[Program]) -> Result<(), ModuleError> {
    let name = Section::DIRECTORY.name;
    if directory.len() != programs.len() {
        return Err(bad(name, format!("{} entries for {} programs", directory.len(), programs.len())));
    }
    for (ordinal, (entry, program)) in directory.iter().zip(programs).enumerate() {
        let symbol = program.symbols.get(program.id as usize);
        if symbol != Some(&entry.id) {
            return Err(bad(name, format!("program {ordinal} is {symbol:?} in its symbols, {:?} in the directory", entry.id)));
        }
        if entry.parent.is_some_and(|p| p as usize >= ordinal) {
            return Err(bad(name, format!("program {ordinal} has parent {}, which does not precede it", entry.parent.unwrap_or(0))));
        }
        if let Some((entry_name, _)) = entry.entries.iter().find(|(_, para)| *para as usize >= program.paragraphs.len()) {
            return Err(bad(name, format!("program {ordinal} ENTRY {entry_name} names a paragraph it lacks")));
        }
    }
    Ok(())
}

/// A section of a count and that many records, the count being the directory's.
fn records<T: Decode>(module: &Module<'_>, strings: &StringTable, section: Section, expected: usize) -> Result<Vec<T>, ModuleError> {
    let mut r = module.reader(section, strings)?;
    let at = r.position();
    let count = r.count()?;
    if count != expected {
        return Err(r.malformed(at, format!("{count} records for {expected} programs")));
    }
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        out.push(T::decode(&mut r)?);
    }
    r.finish()?;
    Ok(out)
}

/// Reads and checks a module; the bytes of every section are checksummed.
pub fn read(bytes: &[u8]) -> Result<LoadedModule, ModuleError> {
    let module = Module::read(bytes)?;
    let strings = module.strings()?;
    let mut r = module.reader(Section::DIRECTORY, &strings)?;
    let directory = Vec::<DirectoryEntry>::decode(&mut r)?;
    r.finish()?;
    let count = directory.len();

    let options = records::<ProgramOptions>(&module, &strings, Section::OPTIONS, count)?;
    let layouts = records::<(Storage, Vec<Item>, Vec<Vec<Sym>>)>(&module, &strings, Section::LAYOUT, count)?;
    let bodies = records::<Body>(&module, &strings, Section::LIR, count)?;
    let sql = records::<Vec<SqlEntry>>(&module, &strings, Section::SQL, count)?;
    let debug = records::<Debug>(&module, &strings, Section::DEBUG, count)?;

    let mut r = module.reader(Section::BMS, &strings)?;
    let at = r.position();
    let maps = r.count()?;
    if maps != 0 {
        return Err(r.malformed(at, format!("{maps} mapsets, which this ironwork cannot hold")));
    }
    r.finish()?;

    let parts = options.into_iter().zip(layouts).zip(bodies).zip(sql).zip(debug);
    let mut programs = Vec::with_capacity(count);
    for ((((options, (storage, items, edits)), body), sql), debug) in parts {
        let Body {
            id, initial, recursive, paragraphs, procedure_start, ranges, blocks, places, exprs, conds, consts, plans,
            services, abends, symbols,
        } = body;
        let program = Program {
            id, options, initial, recursive, storage, items, paragraphs, procedure_start, ranges, blocks, places, exprs,
            conds, consts, plans, services, sql, abends, edits, symbols, debug,
        };
        crate::lir::program_valid(&program).map_err(|reason| bad(Section::SQL.name, reason))?;
        programs.push(program);
    }
    check_directory(&directory, &programs)?;
    Ok(LoadedModule { directory, programs })
}
