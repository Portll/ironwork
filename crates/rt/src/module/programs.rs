//! Writing and reading the programs of a module: the sections that split a `lir::Program` between them.

use super::codec::{Decode, Encode, Reader, Writer};
use super::{Module, ModuleError, ModuleWriter, Section, StringTable};
use numeric::Assumed;
use crate::bms::Mapset;
use crate::codec_struct;
use crate::lir::{AssignItem, 
    AbendText, Block, Code, Cond, Const, Debug, Edit, Expr, Item, ParaId, Paragraph, Place, Plans, Program, ProgramOptions, Range,
    Services, SqlEntry, Storage, SymId,
};

/// A program's line in the `DIRECTORY` section (load-module.md §6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectoryEntry {
    /// PROGRAM-ID exactly as written.
    pub id: String,
    /// A user-defined function's external name, which an invocation loads it by; None for a
    /// program, which a CALL loads by `id`.
    pub external: Option<String>,
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

codec_struct!(DirectoryEntry { id, external, parent, common, entries, params, returning, dynamic });

impl DirectoryEntry {
    /// The entry for a top-level program that no parse of nesting or ENTRY has refined.
    pub fn top_level(program: &Program) -> Self {
        Self {
            id: program.symbols.get(program.id as usize).cloned().unwrap_or_default(),
            external: None,
            parent: None,
            common: false,
            entries: Vec::new(),
            params: vec![false; program.storage.using.len()],
            returning: program.storage.returning.is_some(),
            dynamic: true,
        }
    }

    /// The name a CALL or a function invocation finds it by.
    pub fn load_name(&self) -> &str {
        self.external.as_deref().unwrap_or(&self.id)
    }
}

/// A file the compile read, as the run journal of a run of its source names it (load-module.md
/// §9.2): the library it was found in, 0 the source's own directory and then each `-I` in order,
/// its path from there, and its SHA-256 and length.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFile {
    pub root: u32,
    pub path: String,
    pub sha256: [u8; 32],
    pub bytes: u64,
}

codec_struct!(SourceFile { root, path, sha256, bytes } check source_file_valid);

/// A path relative to its library, with `/` between its parts, so no record of one names a place
/// outside it.
fn source_file_valid(file: &SourceFile) -> Result<(), String> {
    let relative = !file.path.contains('\\') && file.path.split('/').all(|part| !matches!(part, "" | "." | ".."));
    if relative { Ok(()) } else { Err(format!("source file {:?} is not a path within its library", file.path)) }
}

/// The programs of a module, in ordinal order, with their directory, the mapsets they use, and
/// for each program the file each source of its debug table names, None where the compiler
/// supplied the member or no file was recorded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadedModule {
    pub directory: Vec<DirectoryEntry>,
    pub programs: Vec<Program>,
    pub mapsets: Vec<Mapset>,
    pub files: Vec<Vec<Option<SourceFile>>>,
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
    edits: &'a Vec<Edit>,
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

/// One program's record in the `LAYOUT` section.
pub type LayoutRecord = (Storage, Vec<Item>, Vec<Edit>);

/// One program's record in the `LIR` section: the fields of `Program` no other section holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LirRecord {
    pub id: SymId,
    pub initial: bool,
    pub recursive: bool,
    pub paragraphs: Vec<Paragraph>,
    pub procedure_start: ParaId,
    pub ranges: Vec<Range>,
    pub blocks: Vec<Block>,
    pub places: Vec<Place>,
    pub exprs: Vec<Expr>,
    pub conds: Vec<Cond>,
    pub consts: Vec<Const>,
    pub plans: Plans,
    pub services: Services,
    pub abends: Vec<AbendText>,
    pub symbols: Vec<String>,
}

impl LirRecord {
    pub fn code(&self) -> Code<'_> {
        Code {
            id: self.id,
            initial: self.initial,
            recursive: self.recursive,
            paragraphs: &self.paragraphs,
            procedure_start: self.procedure_start,
            ranges: &self.ranges,
            blocks: &self.blocks,
            places: &self.places,
            exprs: &self.exprs,
            conds: &self.conds,
            consts: &self.consts,
            plans: &self.plans,
            services: &self.services,
            abends: &self.abends,
            symbols: &self.symbols,
        }
    }
}

impl Decode for LirRecord {
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

fn encode_module(programs: &[Program], directory: &[DirectoryEntry], mapsets: &[Mapset], files: &[Vec<Option<SourceFile>>]) -> Vec<u8> {
    let mut m = ModuleWriter::new();
    m.section(Section::DIRECTORY, |w| {
        w.count(directory.len());
        for entry in directory {
            entry.encode(w);
        }
    });
    m.section(Section::OPTIONS, |w| {
        per_program(w, programs, |p, w| p.options.encode(w));
        let assumed = assumed_options(programs);
        if !assumed.is_empty() {
            assumed.encode(w);
        }
    });
    m.section(Section::LAYOUT, |w| {
        per_program(w, programs, |p, w| {
            p.storage.encode(w);
            p.items.encode(w);
            p.edits.encode(w);
        });
    });
    m.section(Section::LIR, |w| {
        per_program(w, programs, |p, w| p.encode_lir(w));
        let assigned = assign_items(programs);
        if !assigned.is_empty() {
            assigned.encode(w);
        }
    });
    m.section(Section::SQL, |w| per_program(w, programs, |p, w| p.sql.encode(w)));
    m.section(Section::BMS, |w| {
        w.count(mapsets.len());
        for mapset in mapsets {
            mapset.encode(w);
        }
    });
    m.section(Section::DEBUG, |w| {
        w.count(programs.len());
        for (program, files) in programs.iter().zip(files) {
            Parts::of(program).debug.encode(w);
            files.encode(w);
        }
    });
    m.finish()
}

/// Each program compiled with `--assume`, with its choices: the OPTIONS section's last field,
/// written only when there is one, so a module without one keeps 0.5's shape (load-module.md §5.1).
fn assumed_options(programs: &[Program]) -> Vec<(u32, Assumed)> {
    programs.iter().enumerate().filter(|(_, p)| p.options.options.assumed != Assumed::default()).map(|(n, p)| (n as u32, p.options.options.assumed)).collect()
}

/// The OPTIONS section's body: a record per program, then the choices [`assumed_options`] wrote.
pub struct OptionRecords(pub Vec<ProgramOptions>);

impl Decode for OptionRecords {
    fn decode(r: &mut Reader<'_>) -> Result<Self, ModuleError> {
        let mut options = Vec::<ProgramOptions>::decode(r)?;
        if r.remaining() > 0 {
            let at = r.position();
            for (n, assumed) in Vec::<(u32, Assumed)>::decode(r)? {
                let count = options.len();
                let program = options.get_mut(n as usize).ok_or_else(|| r.malformed(at, format!("--assume choices for program {n} of {count}")))?;
                program.options.assumed = assumed;
            }
        }
        Ok(Self(options))
    }
}

fn option_records(module: &Module<'_>, strings: &StringTable, expected: usize) -> Result<Vec<ProgramOptions>, ModuleError> {
    let mut r = module.reader(Section::OPTIONS, strings)?;
    let at = r.position();
    let OptionRecords(options) = OptionRecords::decode(&mut r)?;
    if options.len() != expected {
        return Err(r.malformed(at, format!("{} records for {expected} programs", options.len())));
    }
    r.finish()?;
    Ok(options)
}

/// Each file that takes its name from a data item, as (program, file, item): the LIR section's
/// last field, written only when there is one, so a module without one keeps 0.5's shape
/// (load-module.md §3.4).
fn assign_items(programs: &[Program]) -> Vec<(u32, u32, AssignItem)> {
    let mut out = Vec::new();
    for (n, program) in programs.iter().enumerate() {
        for (k, file) in program.services.files.iter().enumerate() {
            if let Some(item) = file.assign_item {
                out.push((n as u32, k as u32, item));
            }
        }
    }
    out
}

/// The LIR section's body: a record per program, then the files' data items [`assign_items`] wrote.
pub struct LirRecords(pub Vec<LirRecord>);

impl Decode for LirRecords {
    fn decode(r: &mut Reader<'_>) -> Result<Self, ModuleError> {
        let mut bodies = Vec::<LirRecord>::decode(r)?;
        if r.remaining() > 0 {
            let at = r.position();
            let count = bodies.len();
            for (n, k, item) in Vec::<(u32, u32, AssignItem)>::decode(r)? {
                let body = bodies.get_mut(n as usize).ok_or_else(|| r.malformed(at, format!("an assign item for program {n} of {count}")))?;
                let places = body.places.len();
                let file = body.services.files.get_mut(k as usize).ok_or_else(|| r.malformed(at, format!("an assign item for file {k} of program {n}")))?;
                if item.place as usize >= places {
                    return Err(r.malformed(at, format!("an assign item's place {} of {places}", item.place)));
                }
                file.assign_item = Some(item);
            }
        }
        Ok(Self(bodies))
    }
}

fn lir_records(module: &Module<'_>, strings: &StringTable, expected: usize) -> Result<Vec<LirRecord>, ModuleError> {
    let mut r = module.reader(Section::LIR, strings)?;
    let at = r.position();
    let LirRecords(bodies) = LirRecords::decode(&mut r)?;
    if bodies.len() != expected {
        return Err(r.malformed(at, format!("{} records for {expected} programs", bodies.len())));
    }
    r.finish()?;
    Ok(bodies)
}

/// For each program, no file recorded for any source of its debug table.
fn unrecorded(programs: &[Program]) -> Vec<Vec<Option<SourceFile>>> {
    programs.iter().map(|p| vec![None; p.debug.sources.len()]).collect()
}

/// A module of `programs`, each a top-level program in the directory, with no mapsets and no files
/// recorded. Same input, same bytes.
pub fn write(programs: &[Program]) -> Vec<u8> {
    let directory: Vec<_> = programs.iter().map(DirectoryEntry::top_level).collect();
    encode_module(programs, &directory, &[], &unrecorded(programs))
}

/// A module with the caller's directory and mapsets and no files recorded, refused (as the reader
/// would) if either, or a program, is invalid.
pub fn write_with(programs: &[Program], directory: &[DirectoryEntry], mapsets: &[Mapset]) -> Result<Vec<u8>, ModuleError> {
    let files = unrecorded(programs);
    write_module(&LoadedModule { directory: directory.to_vec(), programs: programs.to_vec(), mapsets: mapsets.to_vec(), files })
}

/// The module `module` describes, refused (as the reader would) if its directory, mapsets, a
/// program or a program's files are invalid.
pub fn write_module(module: &LoadedModule) -> Result<Vec<u8>, ModuleError> {
    let LoadedModule { directory, programs, mapsets, files } = module;
    check_directory(directory, programs)?;
    for program in programs {
        crate::lir::program_valid(program).map_err(|reason| bad("LIR", reason))?;
    }
    check_mapsets(mapsets).map_err(|reason| bad(Section::BMS.name, reason))?;
    check_files(files, programs).map_err(|reason| bad(Section::DEBUG.name, reason))?;
    Ok(encode_module(programs, directory, mapsets, files))
}

/// One file, or none, for each source of each program's debug table.
fn check_files(files: &[Vec<Option<SourceFile>>], programs: &[Program]) -> Result<(), String> {
    if files.len() != programs.len() {
        return Err(format!("files for {} programs of {}", files.len(), programs.len()));
    }
    for (ordinal, (files, program)) in files.iter().zip(programs).enumerate() {
        if files.len() != program.debug.sources.len() {
            return Err(format!("program {ordinal} records {} files for {} sources", files.len(), program.debug.sources.len()));
        }
        files.iter().flatten().try_for_each(source_file_valid)?;
    }
    Ok(())
}

/// Mapsets are held once each, in ascending order of name (load-module.md §5.3).
fn check_mapsets(mapsets: &[Mapset]) -> Result<(), String> {
    match mapsets.windows(2).find(|pair| pair[0].name >= pair[1].name) {
        Some(pair) => Err(format!("mapset {} follows mapset {}", pair[1].name, pair[0].name)),
        None => Ok(()),
    }
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

    let options = option_records(&module, &strings, count)?;
    let layouts = records::<LayoutRecord>(&module, &strings, Section::LAYOUT, count)?;
    let bodies = lir_records(&module, &strings, count)?;
    let sql = records::<Vec<SqlEntry>>(&module, &strings, Section::SQL, count)?;
    let (debug, files): (Vec<Debug>, Vec<Vec<Option<SourceFile>>>) = records::<(Debug, Vec<Option<SourceFile>>)>(&module, &strings, Section::DEBUG, count)?.into_iter().unzip();

    let mut r = module.reader(Section::BMS, &strings)?;
    let mapsets = Vec::<Mapset>::decode(&mut r)?;
    r.finish()?;
    check_mapsets(&mapsets).map_err(|reason| bad(Section::BMS.name, reason))?;

    let parts = options.into_iter().zip(layouts).zip(bodies).zip(sql).zip(debug);
    let mut programs = Vec::with_capacity(count);
    for ((((options, (storage, items, edits)), body), sql), debug) in parts {
        let LirRecord {
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
    check_files(&files, &programs).map_err(|reason| bad(Section::DEBUG.name, reason))?;
    Ok(LoadedModule { directory, programs, mapsets, files })
}
