//! The run unit: every program a run calls, sharing one memory as they share an address space on
//! z/OS. A called program keeps its WORKING-STORAGE and open files from one CALL to the next until
//! it is cancelled. A reference or pointer can reach anywhere in this memory, but never outside it.
//!
//! `H` is the executor's handle to a loaded program and `L` the loader CALL goes through; the run
//! unit holds both without looking inside, and asks `L` what it needs to know about an `H`.

use crate::abend::Abend;
use crate::files::{Dds, Open};
use crate::oo::ClassCode;
use crate::storage::Loc;
use crate::taint::Taint;
use crate::vocab::{OpenMode, Pos};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// A pointer's value is its offset into run-unit memory plus this, so that no item's address is
/// NULL.
pub const ADDRESS_BASE: u32 = 0x0001_0000;
/// RETURN-CODE, a halfword shared by every program in the run unit.
pub const RETURN_CODE: usize = 0;
const RESERVED: usize = 8;
const ALIGNMENT: usize = 8;

pub struct Loaded<H> {
    /// None for the first program, which the caller of the run unit owns.
    pub compiled: Option<H>,
    pub name: String,
    pub base: usize,
    pub files: Vec<Option<Open>>,
    /// The files CLOSE WITH LOCK has closed, which OPEN refuses with status 38.
    pub locked: Vec<bool>,
    pub initialized: bool,
    pub active: bool,
    /// A dynamic CALL has entered it, so CANCEL acts on it.
    pub dynamic: bool,
    /// For a copy a dynamic CALL of an ENTRY name loaded, that entry (numbered as
    /// [`Loader::entry`] numbers them); None for the program loaded by its PROGRAM-ID.
    pub entry: Option<usize>,
    /// Where each paragraph's GO TO goes since an ALTER, by paragraph; empty until one runs.
    pub altered: Vec<Option<usize>>,
    /// The library file CALL loaded the program from; None for the programs of the first source.
    pub source: Option<PathBuf>,
}

pub enum LoadError {
    NotFound,
    Compile(String),
}

/// A program a loader found and compiled.
pub struct LoadedProgram<H> {
    pub compiled: H,
    /// The PROGRAM-ID, in upper case.
    pub name: String,
    pub files: usize,
    pub size: usize,
    /// The file it was read from, when a program library supplied it.
    pub source: Option<PathBuf>,
}

/// A class definition a loader found and compiled, and its source table, its own source first by
/// path when a program library supplied it.
pub struct FoundClass<C> {
    pub code: C,
    pub sources: Vec<String>,
}

/// Where CALL finds programs, and what the run unit needs to know about one it loaded.
pub trait Loader<H> {
    /// The program whose PROGRAM-ID CALL names, compiled.
    fn program(&mut self, name: &str) -> Result<LoadedProgram<H>, LoadError>;

    /// The PROGRAM-ID of a program not yet loaded that has an ENTRY of this name.
    fn holder(&self, entry: &str) -> Option<String>;

    /// Which of a loaded program's ENTRY statements has this name.
    fn entry(program: &H, name: &str) -> Option<usize>;

    /// A loaded program's file count and storage size.
    fn shape(program: &H) -> (usize, usize);

    /// The PROGRAM-IDs of the programs a loaded program contains, which a CANCEL of it reaches.
    fn nested(program: &H) -> &[String];

    /// Source file `file` of a loaded program's source table, by name.
    fn source(program: &H, file: usize) -> Option<String>;

    /// The COBOL class definition of this external name, its data and methods each a program the
    /// executor runs; None for a Java class.
    fn class(&mut self, external: &str) -> Result<Option<FoundClass<Rc<ClassCode<H>>>>, String>;

    /// A BMS mapset from the copy libraries, which SEND MAP and RECEIVE MAP read; None when no
    /// library holds it.
    fn mapset(&mut self, name: &str) -> Option<Result<crate::bms::Mapset, String>>;
}

/// An executor's activation, as each service's host trait reaches the run unit through it: `Program`
/// is the executor's handle to a loaded program, `Loader` the loader CALL goes through.
pub trait UnitHost<'w> {
    type Program: Clone;
    type Loader: Loader<Self::Program>;
    fn unit(&mut self) -> &mut RunUnit<'w, Self::Program, Self::Loader>;
}

#[derive(Clone, Copy, Debug)]
pub enum Clock {
    System,
    /// Seconds since 1970-01-01T00:00:00Z and hundredths, for runs that must repeat exactly.
    Fixed(i64, u32),
}

/// What a run did that its evidence journal records: each file as it is opened and closed, each
/// program CALL loads, with the source it was read from when a library supplied it, and each
/// operation an input could steer, with its operand as the program's code page reads it.
pub enum Event<'a> {
    Open { dd: &'a str, mode: OpenMode, path: &'a Path },
    Close { dd: &'a str, path: &'a Path },
    Load { program: &'a str, source: Option<&'a Path> },
    /// Control entering paragraph (or section header) `index` of `program` at its start.
    Paragraph { program: &'a str, name: &'a str, index: usize },
    /// `kind` is cobolwork's name for the sink (`dynamic-program-load`, `log`, ...); `file` is the
    /// library file or COPY member the operation is in, empty for the first program's own source;
    /// `input`, under [`RunUnit::taint`], whether an input byte may be in the operand
    /// ([`crate::taint::Taint::at_sink`]).
    Sink { kind: &'static str, file: &'a str, line: u32, operand: &'a str, input: Option<bool> },
    /// A statement starting, under [`RunUnit::statements`]; `file` as `Sink`'s.
    Statement { file: &'a str, line: u32 },
}

/// The statements whose start a run tells its observer of: every one, or those on these lines of
/// any source, which the observer narrows to their files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StatementFilter {
    All,
    Lines(HashSet<u32>),
}

pub type Observer<'w> = Box<dyn FnMut(Event<'_>) + 'w>;

/// How deep PERFORMs and CALLs may nest before the run abends, rather than exhaust the stack.
pub const MAX_DEPTH: usize = 100;

/// EXTERNAL data records and file connectors, which belong to the run unit rather than to a
/// program: one of each name, whichever program first describes it (Language Reference
/// SC27-8713-03, pp. 65, 184, 197).
#[derive(Default)]
pub struct Externals {
    /// Each EXTERNAL data record, and each EXTERNAL file's record area, by name: where it is and
    /// how many bytes it has.
    storage: HashMap<(bool, String), (usize, usize)>,
    files: Vec<Option<Open>>,
    /// Whether each EXTERNAL file was closed WITH LOCK.
    locked: Vec<bool>,
    file_names: HashMap<String, usize>,
}

/// A file of a program that is another's file connector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connector {
    /// The run unit's EXTERNAL file of this number.
    External(usize),
    /// File `k` of loaded program `p`: a GLOBAL file of a program containing this one.
    Program(usize, usize),
}

/// The routines cobolwork reads a CALL of as running an operating-system command, whose arguments
/// the input trace checks.
pub const OS_COMMAND_ROUTINES: &[&str] = &["SYSTEM", "C$SYSTEM", "CBL_EXEC_RUN_UNIT", "CBL_GC_HOSTED", "BXPSYSTM"];

pub struct RunUnit<'w, H, L: Loader<H>> {
    pub mem: Vec<u8>,
    /// PERFORMs and CALLs in progress, across every program.
    pub depth: usize,
    pub programs: Vec<Loaded<H>>,
    names: HashMap<String, usize>,
    pub library: L,
    pub dds: Dds,
    pub sysin: Option<Box<dyn BufRead + 'w>>,
    pub clock: Clock,
    pub out: &'w mut dyn Write,
    pub err: &'w mut dyn Write,
    /// The CICS task a harness run stands in for, with its EXEC interface block and open files.
    pub cics: Option<crate::cics::Task>,
    pub eib: usize,
    pub cics_files: HashMap<String, Open>,
    /// The database EXEC SQL statements reach, when the run has one.
    pub sql: Option<crate::sql::Session<'w>>,
    /// Language Environment's heap storage and message files.
    pub le: crate::le::State,
    /// Classes, objects and the JNI environment of the run unit's object-oriented programs.
    pub oo: crate::oo::Objects<Rc<ClassCode<H>>>,
    /// Told what the run opens, closes and loads, when a caller keeps evidence of it.
    pub observer: Option<Observer<'w>>,
    /// FUNCTION RANDOM's generator, one for the run unit, from the first reference on.
    pub random: Option<u32>,
    externals: Externals,
    /// The files of loaded programs that are another's connector, by program and file.
    connectors: HashMap<(usize, usize), Connector>,
    /// The entries SET TO ENTRY has named, which function-pointers and procedure-pointers hold.
    pub entries: Vec<crate::set::Entry>,
    /// The statements an observer is told of as each starts; None tells it of none.
    pub statements: Option<StatementFilter>,
    /// Which bytes may hold input, when the run traces input.
    pub taint: Option<Taint>,
    /// How many more statements may start before the run ends with S322; None for no limit.
    pub statement_limit: Option<u64>,
}

impl<H, L: Loader<H>> RunUnit<'_, H, L> {
    /// Copies `bytes` into memory at `offset`; under taint they may hold input when the running
    /// statement has read a byte that may. Every write of data to memory goes through here or
    /// [`RunUnit::write_input`], or marks its bytes with [`RunUnit::mark`].
    pub fn write(&mut self, offset: usize, bytes: &[u8]) {
        self.mem[offset..offset + bytes.len()].copy_from_slice(bytes);
        self.mark(offset, bytes.len());
    }

    /// Copies input into memory at `offset`: bytes a READ, ACCEPT or row brought in.
    pub fn write_input(&mut self, offset: usize, bytes: &[u8]) {
        self.mem[offset..offset + bytes.len()].copy_from_slice(bytes);
        self.mark_input(offset, bytes.len(), true);
    }

    /// Marks bytes written outside [`RunUnit::write`] as it would.
    pub fn mark(&mut self, offset: usize, len: usize) {
        if let Some(t) = self.taint.as_mut() {
            let pending = t.pending();
            t.set(offset, len, pending);
        }
    }

    /// Marks bytes as input, or as holding none: initial values, which are constants.
    pub fn mark_input(&mut self, offset: usize, len: usize, input: bool) {
        if let Some(t) = self.taint.as_mut() {
            t.set(offset, len, input);
        }
    }

    /// A read of `loc` by the running statement.
    pub fn taint_read(&mut self, loc: Loc) {
        if let Some(t) = self.taint.as_mut() {
            t.read(loc.offset, loc.len);
        }
    }

    /// [`Taint::writing`], when the run traces input.
    pub fn writing(&mut self, on: bool) -> bool {
        self.taint.as_mut().is_some_and(|t| t.writing(on))
    }

    /// A statement with a position starts: its writes carry only what it reads.
    pub fn statement_starts(&mut self) {
        if let Some(t) = self.taint.as_mut() {
            t.start_statement();
        }
    }

    /// Whether the running statement has read a byte that may hold input.
    pub fn pending(&self) -> bool {
        self.taint.as_ref().is_some_and(Taint::pending)
    }

    /// [`Taint::resume_statement`], when the run traces input.
    pub fn resume_statement(&mut self, read_before: bool) {
        if let Some(t) = self.taint.as_mut() {
            t.resume_statement(read_before);
        }
    }

    /// The run did `what`, which taint does not follow.
    pub fn unfollowed(&mut self, what: &'static str) {
        if let Some(t) = self.taint.as_mut() {
            t.unfollowed(what);
        }
    }

    /// Whether an input byte may be in a sink's operand; None without taint.
    pub fn input_at_sink(&self) -> Option<bool> {
        self.taint.as_ref().and_then(Taint::at_sink)
    }
}

impl<'w, H: Clone, L: Loader<H>> RunUnit<'w, H, L> {
    pub fn new(library: L, dds: Dds, sysin: Option<Box<dyn BufRead + 'w>>, clock: Clock, out: &'w mut dyn Write, err: &'w mut dyn Write) -> Self {
        Self {
            mem: vec![0; RESERVED],
            depth: 0,
            programs: Vec::new(),
            names: HashMap::new(),
            library,
            dds,
            sysin,
            clock,
            out,
            err,
            cics: None,
            eib: 0,
            cics_files: HashMap::new(),
            sql: None,
            le: crate::le::State::default(),
            oo: Default::default(),
            observer: None,
            random: None,
            externals: Externals::default(),
            connectors: HashMap::new(),
            entries: Vec::new(),
            statements: None,
            taint: None,
            statement_limit: None,
        }
    }

    fn allocate(&mut self, size: usize) -> usize {
        let base = self.mem.len().div_ceil(ALIGNMENT) * ALIGNMENT;
        self.mem.resize(base + size, 0);
        base
    }

    /// Adds a program to the run unit under `name` and gives it its storage.
    pub fn add_named(&mut self, compiled: Option<H>, name: String, files: usize, size: usize) -> usize {
        let base = self.allocate(size);
        let index = self.programs.len();
        self.names.insert(name.clone(), index);
        self.programs.push(Loaded { compiled, name, base, files: (0..files).map(|_| None).collect(), locked: vec![false; files], initialized: false, active: false, dynamic: false, entry: None, altered: Vec::new(), source: None });
        index
    }

    /// The program a CALL of `name` enters, and which of its ENTRY statements when `name` is not
    /// its PROGRAM-ID. A static CALL of an entry name enters the one copy of the program; a dynamic
    /// CALL gets a copy of its own for each entry name (assumption C51).
    pub fn load_entry(&mut self, name: &str, dynamic: bool) -> Result<(usize, Option<usize>), LoadError> {
        let name = name.to_ascii_uppercase();
        if let Some(i) = self.find(&name) {
            return Ok((i, self.programs[i].entry));
        }
        let index = match self.programs.iter().position(|p| p.compiled.as_ref().is_some_and(|c| L::entry(c, &name).is_some())) {
            Some(i) => i,
            None => {
                let holder = self.library.holder(&name);
                self.load(holder.as_deref().unwrap_or(&name))?
            }
        };
        let Some(compiled) = self.programs[index].compiled.clone() else { return Ok((index, None)) };
        let Some(entry) = L::entry(&compiled, &name) else { return Ok((index, None)) };
        if !dynamic {
            return Ok((index, Some(entry)));
        }
        let (files, size) = L::shape(&compiled);
        let copy = self.add_named(Some(compiled), name, files, size);
        self.programs[copy].entry = Some(entry);
        self.programs[copy].source = self.programs[index].source.clone();
        Ok((copy, Some(entry)))
    }

    /// Storage for a BY CONTENT or BY VALUE argument, at the end of memory, written as
    /// [`RunUnit::write`] writes.
    pub fn push_temporary(&mut self, bytes: &[u8]) -> usize {
        let at = self.allocate(bytes.len());
        self.write(at, bytes);
        at
    }

    /// Releases arguments pushed since `mark`, unless a program, heap storage or EXTERNAL storage
    /// was placed behind them.
    pub fn release_temporaries(&mut self, mark: usize) {
        let external = self.externals.storage.values().all(|&(at, _)| at < mark);
        if self.programs.iter().all(|p| p.base < mark) && self.le.heap_end() <= mark && external {
            self.mem.truncate(mark.max(RESERVED));
            if let Some(t) = self.taint.as_mut() {
                t.truncate(self.mem.len());
            }
        }
    }

    /// One more PERFORM or CALL in progress, refused past `MAX_DEPTH` rather than exhaust the stack.
    pub fn enter(&mut self, pos: Pos) -> Result<(), Abend> {
        if self.depth >= MAX_DEPTH {
            return Err(Abend::ironwork(format!("PERFORM and CALL nest deeper than {MAX_DEPTH}"), pos));
        }
        self.depth += 1;
        Ok(())
    }

    /// Marks program `me` active: where its storage starts, and whether the activation starts from
    /// fresh storage, as its first does, the first after a CANCEL, and every one of an INITIAL
    /// program.
    pub fn activate(&mut self, me: usize, initial: bool) -> (usize, bool) {
        let program = &mut self.programs[me];
        program.active = true;
        (program.base, !program.initialized || initial)
    }

    /// Program `me`'s storage holds its initial values, and its GO TOs go where they are written.
    pub fn initialized(&mut self, me: usize) {
        self.programs[me].initialized = true;
        self.programs[me].altered.clear();
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        self.names.get(&name.to_ascii_uppercase()).copied()
    }

    /// The program CALL names, compiling and loading it the first time.
    pub fn load(&mut self, name: &str) -> Result<usize, LoadError> {
        let name = name.to_ascii_uppercase();
        if let Some(i) = self.find(&name) {
            return Ok(i);
        }
        let loaded = self.library.program(&name)?;
        self.notify(Event::Load { program: &name, source: loaded.source.as_deref() });
        let index = self.add_named(Some(loaded.compiled), loaded.name, loaded.files, loaded.size);
        self.programs[index].source = loaded.source;
        Ok(index)
    }

    pub const fn observed(&self) -> bool {
        self.observer.is_some()
    }

    /// Whether a statement starting on `line` is told to the observer.
    /// Counts the start of the statement at `pos` against the statement limit: once it is spent the
    /// run ends there with S322, as z/OS ends a step that runs past its TIME= (assumption C241).
    pub fn start_statement(&mut self, pos: Pos) -> Result<(), Abend> {
        match self.statement_limit.as_mut() {
            Some(0) => Err(Abend { code: crate::abend::AbendCode::TimeLimit, message: "the run reached its statement limit, as a step past its TIME= ends".into(), pos, file: None }),
            Some(left) => {
                *left -= 1;
                Ok(())
            }
            None => Ok(()),
        }
    }

    pub fn traces(&self, line: u32) -> bool {
        match &self.statements {
            None => false,
            Some(StatementFilter::All) => self.observer.is_some(),
            Some(StatementFilter::Lines(lines)) => self.observer.is_some() && lines.contains(&line),
        }
    }

    pub fn notify(&mut self, event: Event<'_>) {
        if let Some(observer) = self.observer.as_mut() {
            observer(event);
        }
    }

    /// Closes every file any program left open, as the runtime does when the run unit ends.
    pub fn close_all(&mut self) -> Result<(), String> {
        for program in &mut self.programs {
            for f in program.files.iter_mut().filter_map(Option::take) {
                f.close().map_err(|e| format!("closing a file of {}: {e}", program.name))?;
            }
        }
        for (name, &k) in &self.externals.file_names {
            if let Some(f) = self.externals.files[k].take() {
                f.close().map_err(|e| format!("closing EXTERNAL file {name}: {e}"))?;
            }
        }
        Ok(())
    }

    /// Where EXTERNAL record `name` is, or EXTERNAL file `name`'s record area when `file`: storage
    /// of `size` bytes, zeroed, the first time a program describes it. A description of another
    /// size is refused (assumption C180).
    pub fn external(&mut self, name: &str, file: bool, size: usize) -> Result<usize, String> {
        let key = (file, name.to_owned());
        if let Some(&(at, had)) = self.externals.storage.get(&key) {
            return if had == size {
                Ok(at)
            } else {
                let what = if file { "the record area of EXTERNAL file" } else { "EXTERNAL record" };
                Err(format!("{what} {name} has {had} bytes in the run unit, and this program describes {size}"))
            };
        }
        let at = self.allocate(size);
        self.externals.storage.insert(key, (at, size));
        Ok(at)
    }

    /// The run unit's connector for EXTERNAL file `name`.
    pub fn external_file(&mut self, name: &str) -> Connector {
        let (files, locked) = (&mut self.externals.files, &mut self.externals.locked);
        let k = *self.externals.file_names.entry(name.to_owned()).or_insert_with(|| {
            files.push(None);
            locked.push(false);
            files.len() - 1
        });
        Connector::External(k)
    }

    /// Program `me`'s file k is the connector `to`, for as long as the run unit lasts.
    pub fn connect(&mut self, me: usize, k: usize, to: Connector) {
        self.connectors.insert((me, k), to);
    }

    fn connector(&self, mut me: usize, mut k: usize) -> Option<Connector> {
        let mut to = None;
        while let Some(&c) = self.connectors.get(&(me, k)) {
            to = Some(c);
            match c {
                Connector::External(_) => break,
                Connector::Program(p, j) => (me, k) = (p, j),
            }
        }
        to
    }

    /// Program `me`'s file k, open or not: its own, or the connector it shares.
    pub fn file(&mut self, me: usize, k: usize) -> &mut Option<Open> {
        match self.connector(me, k) {
            None => &mut self.programs[me].files[k],
            Some(Connector::External(e)) => &mut self.externals.files[e],
            Some(Connector::Program(p, j)) => &mut self.programs[p].files[j],
        }
    }

    /// Whether program `me`'s file k, or the connector it shares, was closed WITH LOCK.
    pub fn locked(&mut self, me: usize, k: usize) -> &mut bool {
        match self.connector(me, k) {
            None => &mut self.programs[me].locked[k],
            Some(Connector::External(e)) => &mut self.externals.locked[e],
            Some(Connector::Program(p, j)) => &mut self.programs[p].locked[j],
        }
    }

    pub fn file_ref(&self, me: usize, k: usize) -> &Option<Open> {
        match self.connector(me, k) {
            None => &self.programs[me].files[k],
            Some(Connector::External(e)) => &self.externals.files[e],
            Some(Connector::Program(p, j)) => &self.programs[p].files[j],
        }
    }

    pub fn return_code(&self) -> i16 {
        i16::from_be_bytes([self.mem[RETURN_CODE], self.mem[RETURN_CODE + 1]])
    }

    /// The current time: seconds since the epoch, and hundredths.
    pub fn now(&self) -> (i64, u32) {
        match self.clock {
            Clock::Fixed(s, h) => (s, h),
            Clock::System => {
                let d = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
                (d.as_secs() as i64, d.subsec_millis() / 10)
            }
        }
    }
}
