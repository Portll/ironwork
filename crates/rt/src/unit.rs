//! The run unit: every program a run calls, sharing one memory as they share an address space on
//! z/OS. A called program keeps its WORKING-STORAGE and open files from one CALL to the next until
//! it is cancelled, or in a CICS task until the LINK or XCTL that started its run unit ends it
//! (C145). A reference or pointer can reach anywhere in this memory, but never outside it.
//!
//! `H` is the executor's handle to a loaded program and `L` the loader CALL goes through; the run
//! unit holds both without looking inside, and asks `L` what it needs to know about an `H`.

use crate::abend::Abend;
use crate::files::{Dds, Open};
use crate::oo::ClassCode;
use crate::storage::Loc;
use crate::taint::Taint;
use crate::vocab::{OpenMode, Pos};
use numeric::Dialect;
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
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
    pub size: usize,
    /// False once a CICS run unit has set the program's storage at `base` aside, or has ended and
    /// released it: its next activation gets storage of its own.
    pub placed: bool,
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

impl<H> Loaded<H> {
    /// A program with `files` files and `size` bytes of storage at `base`, in its initial state.
    pub fn new(compiled: Option<H>, name: String, base: usize, size: usize, files: usize) -> Self {
        let (locked, files) = (vec![false; files], (0..files).map(|_| None).collect());
        Self { compiled, name, base, size, placed: true, files, locked, initialized: false, active: false, dynamic: false, entry: None, altered: Vec::new(), source: None }
    }

    /// What the program's activations have left, taken away: it is in its initial state, with its
    /// storage set aside.
    fn set_aside(&mut self) -> Held {
        let files = self.files.iter_mut().map(Option::take).collect();
        let locked = std::mem::replace(&mut self.locked, vec![false; self.files.len()]);
        let held = Held { base: self.base, placed: self.placed, files, locked, initialized: self.initialized, active: self.active, dynamic: self.dynamic, altered: std::mem::take(&mut self.altered) };
        (self.placed, self.initialized, self.active, self.dynamic) = (false, false, false, false);
        held
    }

    fn restore(&mut self, held: Held) {
        (self.base, self.placed, self.files, self.locked, self.altered) = (held.base, held.placed, held.files, held.locked, held.altered);
        (self.initialized, self.active, self.dynamic) = (held.initialized, held.active, held.dynamic);
    }
}

/// What a CICS run unit holds that the LINK or XCTL starting another sets aside until it ends:
/// each program's state, the EXTERNAL data and files with the connectors to them, the Language
/// Environment heap (C126), FUNCTION RANDOM's sequence (C104) and RETURN-CODE (C105), which
/// belong to the enclave.
struct Enclave {
    programs: Vec<Held>,
    externals: Externals,
    connectors: HashMap<(usize, usize), Connector>,
    heap: Vec<(usize, usize, bool)>,
    random: Option<u32>,
    /// RETURN-CODE's bytes, and whether they may hold input.
    return_code: ([u8; 2], bool),
}

/// A program's state in a CICS run unit that a LINK or XCTL has set aside.
struct Held {
    base: usize,
    placed: bool,
    files: Vec<Option<Open>>,
    locked: Vec<bool>,
    initialized: bool,
    active: bool,
    dynamic: bool,
    altered: Vec<Option<usize>>,
}

pub enum LoadError {
    NotFound,
    /// Found, but its source does not compile or its load module does not load.
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
    /// For a program a load module holds, unless of the source the run began with: each source of
    /// its debug table by name, with the file the module records for it, its own source first.
    pub recorded: Vec<(String, Option<crate::module::SourceFile>)>,
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
    /// `recorded` is [`LoadedProgram::recorded`], empty for a program read from source.
    Load { program: &'a str, source: Option<&'a Path>, recorded: &'a [(String, Option<crate::module::SourceFile>)] },
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

/// Every kind of [`Event::Sink`] ironwork raises. A sink record joins a cobolwork finding by its
/// kind, so each is one cobolwork's `lib/dataflow.mjs` names (fixtures/cobolwork/evidence/sinks.tsv).
pub const SINK_KINDS: [&str; 16] = [
    "cics-dynamic-transfer",
    "cics-sysid",
    "connection-target",
    "dynamic-file-path",
    "dynamic-program-load",
    "dynamic-sql",
    "http-header",
    "log",
    "os-command",
    "outbound-host",
    "outbound-http",
    "queue-name",
    "record-key",
    "record-update",
    "screen",
    "web-response",
];

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

/// The EXTERNAL record of the run unit that holds UPSI switch `n`: one byte, 1 when the switch is
/// on and 0 when it is off (assumption C410). No program can spell the name, so only a
/// SPECIAL-NAMES entry for the switch reaches it.
pub fn switch_record(n: u8) -> String {
    format!("UPSI-{n} SWITCH")
}

/// The UPSI switch whose [`switch_record`] is named `name`.
pub fn switch_of_record(name: &str) -> Option<u8> {
    match name.strip_prefix("UPSI-")?.strip_suffix(" SWITCH")?.as_bytes() {
        [d @ b'0'..=b'7'] => Some(d - b'0'),
        _ => None,
    }
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
    /// The job step's program arguments, which ACCEPT ... FROM COMMAND-LINE and ARGUMENT-VALUE read
    /// under `--compliance extended`; empty without a PARM.
    pub arguments: crate::le::parm::Arguments,
    /// The screen positioned DISPLAY and ACCEPT use under `--compliance extended`, and the operator
    /// a screen script plays; None until one is given or the run first uses the screen.
    pub crt: Option<Rc<std::cell::RefCell<crate::crt::Crt>>>,
    /// The environment variables ACCEPT ... FROM ENVIRONMENT reads under `--compliance extended`.
    pub environment: crate::environment::Environment,
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
    /// When the run ends with S322 for its time limit, and that limit in seconds.
    deadline: Option<(Instant, u64)>,
    /// Statement starts since the clock was last read against `deadline`.
    unclocked: u32,
    /// The bytes of storage the run unit may hold before the run ends; None for no limit.
    storage_limit: Option<usize>,
    /// The last statements started under a statement limit, oldest first, and once it is spent the
    /// loop statement the S322 waits for.
    recent: VecDeque<Started>,
    overrun: Option<Overrun>,
    /// The ACCEPTs, by position, that have said they found SYSIN at its end; a loop around one
    /// would otherwise write a line each time round.
    pub(crate) sysin_ended: HashSet<(u16, u32, u32)>,
    /// The CICS run units the LINKs and XCTLs running have set aside, the innermost last.
    set_aside: Vec<Enclave>,
}

fn end_file(f: Open, unclosed: bool) -> std::io::Result<()> {
    if unclosed { f.abandon() } else { f.close() }
}

/// How many statement starts pass between readings of the clock under a time limit.
const CLOCK_EVERY: u32 = 256;

/// How many of the last statement starts an S322 looks back over for the loop the run is in, and
/// how many more it lets start while it waits for that loop's first statement.
const LOOP_WINDOW: usize = 4096;

/// A statement start: the loaded program, how deep PERFORMs and CALLs had nested, and where.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Started {
    program: usize,
    depth: usize,
    pos: Pos,
}

/// A spent statement limit: the loop's first statement, the S322's place, its lines, and the starts
/// left before the run ends wherever it is.
struct Overrun {
    head: Started,
    lines: Vec<u32>,
    grace: usize,
}

/// The loop the last starts ran: the statements that started more than once among them, those of
/// their outermost frame, in its program's own source before any COPY member, and the first of
/// them by position. A loop's statements recur at one depth however many statements ran before
/// it, and anything it PERFORMs or CALLs runs deeper. With none recurring, the statement starting
/// now and no lines.
fn loop_of(recent: &VecDeque<Started>, now: Started) -> Overrun {
    let key = |s: &Started| (s.program, s.depth, s.pos.file, s.pos.line, s.pos.col);
    let mut seen: HashMap<_, usize> = HashMap::new();
    for s in recent {
        *seen.entry(key(s)).or_default() += 1;
    }
    let recurring: Vec<&Started> = recent.iter().filter(|s| seen[&key(s)] > 1).collect();
    let Some(outer) = recurring.iter().map(|s| s.depth).min() else { return Overrun { head: now, lines: Vec::new(), grace: 0 } };
    let program = recurring.iter().rev().find(|s| s.depth == outer).map_or(now.program, |s| s.program);
    let frame: Vec<&Started> = recurring.into_iter().filter(|s| s.depth == outer && s.program == program).collect();
    let file = frame.iter().map(|s| s.pos.file).min().unwrap_or(now.pos.file);
    let head = frame.iter().filter(|s| s.pos.file == file).min_by_key(|s| (s.pos.line, s.pos.col)).map_or(now, |s| **s);
    let mut lines: Vec<u32> = frame.iter().filter(|s| s.pos.file == file).map(|s| s.pos.line).collect();
    lines.sort_unstable();
    lines.dedup();
    Overrun { head, lines, grace: LOOP_WINDOW }
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

    /// [`Taint::take_input`], when the run traces input.
    pub fn take_input(&mut self) {
        if let Some(t) = self.taint.as_mut() {
            t.take_input();
        }
    }

    /// Whether any byte of the range may hold input; false without taint.
    pub fn holds_input(&self, offset: usize, len: usize) -> bool {
        self.taint.as_ref().is_some_and(|t| t.any(offset, len))
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
            arguments: Default::default(),
            crt: None,
            environment: Default::default(),
            externals: Externals::default(),
            connectors: HashMap::new(),
            entries: Vec::new(),
            statements: None,
            taint: None,
            statement_limit: None,
            deadline: None,
            unclocked: 0,
            storage_limit: None,
            recent: VecDeque::new(),
            overrun: None,
            sysin_ended: HashSet::new(),
            set_aside: Vec::new(),
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
        self.programs.push(Loaded::new(compiled, name, base, size, files));
        index
    }

    /// A CICS LINK or XCTL starts a run unit of its own (C145): every program starts in it in its
    /// initial state, with storage of its own, it has no EXTERNAL data or files and an empty heap
    /// (C126), FUNCTION RANDOM has not been referenced (C104) and RETURN-CODE is zero (C105), and
    /// the state of the run unit that issued it is set aside until [`RunUnit::end_cics_run_unit`].
    pub fn begin_cics_run_unit(&mut self) {
        let programs = self.programs.iter_mut().map(Loaded::set_aside).collect();
        let (externals, connectors) = (std::mem::take(&mut self.externals), std::mem::take(&mut self.connectors));
        let return_code = ([self.mem[RETURN_CODE], self.mem[RETURN_CODE + 1]], self.holds_input(RETURN_CODE, 2));
        self.mem[RETURN_CODE..RETURN_CODE + 2].fill(0);
        self.mark_input(RETURN_CODE, 2, false);
        let (heap, random) = (std::mem::take(&mut self.le.heap), self.random.take());
        self.set_aside.push(Enclave { programs, externals, connectors, heap, random, return_code });
    }

    /// Ends the run unit [`RunUnit::begin_cics_run_unit`] started, closing the files its programs
    /// and its EXTERNAL files left open as Language Environment closes an enclave's, and dropping
    /// its programs' storage, EXTERNAL data and heap: a program it loaded stays loaded, in its
    /// initial state with no storage (C129), and the run unit set aside has everything back but,
    /// after `xctl`, RETURN-CODE, the program XCTL started having taken the issuer's place (C105).
    pub fn end_cics_run_unit(&mut self, xctl: bool) -> Result<(), String> {
        let mut closed = Ok(());
        for program in &mut self.programs {
            for f in program.files.iter_mut().filter_map(Option::take) {
                if let Err(e) = f.close() {
                    closed = closed.and(Err(format!("closing a file of {}: {e}", program.name)));
                }
            }
            drop(program.set_aside());
        }
        closed = closed.and(self.close_external_files(false));
        let Some(enclave) = self.set_aside.pop() else { return closed };
        for (program, held) in self.programs.iter_mut().zip(enclave.programs) {
            program.restore(held);
        }
        (self.externals, self.connectors, self.le.heap, self.random) = (enclave.externals, enclave.connectors, enclave.heap, enclave.random);
        if !xctl {
            let (bytes, input) = enclave.return_code;
            self.mem[RETURN_CODE..RETURN_CODE + 2].copy_from_slice(&bytes);
            self.mark_input(RETURN_CODE, 2, input);
        }
        closed
    }

    /// The program a CALL of `name` enters, and which of its ENTRY statements when `name` is not
    /// its PROGRAM-ID: the one copy of the program, or with `copy` a copy of its own for the entry
    /// name ([`crate::callee::entry_copy`]).
    pub fn load_entry(&mut self, name: &str, copy: bool) -> Result<(usize, Option<usize>), LoadError> {
        if let Some(i) = self.find(name) {
            return Ok((i, self.programs[i].entry));
        }
        let name = name.to_ascii_uppercase();
        let index = match self.programs.iter().position(|p| p.compiled.as_ref().is_some_and(|c| L::entry(c, &name).is_some())) {
            Some(i) => i,
            None => {
                let holder = self.library.holder(&name);
                self.load(holder.as_deref().unwrap_or(&name))?
            }
        };
        let Some(compiled) = self.programs[index].compiled.clone() else { return Ok((index, None)) };
        let Some(entry) = L::entry(&compiled, &name) else { return Ok((index, None)) };
        if !copy {
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

    /// Releases arguments pushed since `mark`, unless a program's storage, heap storage or EXTERNAL
    /// storage was placed behind them.
    pub fn release_temporaries(&mut self, mark: usize) {
        let external = self.externals.storage.values().all(|&(at, _)| at < mark);
        if self.programs.iter().all(|p| !p.placed || p.base + p.size <= mark) && self.le.heap_end() <= mark && external {
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
        if !self.programs[me].placed {
            let base = self.allocate(self.programs[me].size);
            (self.programs[me].base, self.programs[me].placed) = (base, true);
        }
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
        if name.bytes().any(|b| b.is_ascii_lowercase()) {
            return self.names.get(&name.to_ascii_uppercase()).copied();
        }
        self.names.get(name).copied()
    }

    /// The program CALL names, compiling and loading it the first time.
    pub fn load(&mut self, name: &str) -> Result<usize, LoadError> {
        let name = name.to_ascii_uppercase();
        if let Some(i) = self.find(&name) {
            return Ok(i);
        }
        let loaded = self.library.program(&name)?;
        self.notify(Event::Load { program: &name, source: loaded.source.as_deref(), recorded: &loaded.recorded });
        let index = self.add_named(Some(loaded.compiled), loaded.name, loaded.files, loaded.size);
        self.programs[index].source = loaded.source;
        Ok(index)
    }

    pub const fn observed(&self) -> bool {
        self.observer.is_some()
    }

    /// Whether a statement starting on `line` is told to the observer.
    /// Sets the run's limits: statements that may start, seconds from now, and bytes of storage.
    /// Each is checked as a statement starts; a request for storage past the limit is granted, and
    /// the run ends at the next statement.
    pub fn limit(&mut self, statements: Option<u64>, seconds: Option<u64>, storage: Option<u64>) {
        self.statement_limit = statements;
        self.deadline = seconds.and_then(|s| Some((Instant::now().checked_add(Duration::from_secs(s))?, s)));
        self.storage_limit = storage.map(|b| usize::try_from(b).unwrap_or(usize::MAX));
    }

    /// Whether statement starts are checked against a limit.
    pub const fn limited(&self) -> bool {
        self.statement_limit.is_some() || self.deadline.is_some() || self.storage_limit.is_some()
    }

    /// Counts the start of `program`'s statement at `pos` against the run's limits. Storage past
    /// its limit ends the run there, and so does the time limit, read every `CLOCK_EVERY` starts.
    /// Once the statement limit is spent the run ends with S322, as z/OS ends a step that runs past its TIME=, at the next start
    /// of the loop it is in, so the place does not depend on how many statements ran before the
    /// loop (assumption C241).
    pub fn start_statement(&mut self, program: usize, pos: Pos) -> Result<(), Abend> {
        if let Some(limit) = self.storage_limit
            && self.mem.len() > limit
        {
            return Err(Abend::ironwork(format!("the run unit's storage reached {} bytes, past its storage limit of {limit}", self.mem.len()), pos));
        }
        if let Some((deadline, seconds)) = self.deadline {
            self.unclocked += 1;
            if self.unclocked == CLOCK_EVERY {
                self.unclocked = 0;
                if Instant::now() >= deadline {
                    let unit = if seconds == 1 { "second" } else { "seconds" };
                    let message = format!("the run reached its time limit of {seconds} {unit}, as a step past its TIME= ends");
                    return Err(Abend { code: crate::abend::AbendCode::TimeLimit, message, pos, file: None });
                }
            }
        }
        let Some(left) = self.statement_limit.as_mut() else { return Ok(()) };
        let now = Started { program, depth: self.depth, pos };
        if *left > 0 {
            *left -= 1;
            if self.recent.len() == LOOP_WINDOW {
                self.recent.pop_front();
            }
            self.recent.push_back(now);
            return Ok(());
        }
        let overrun = self.overrun.get_or_insert_with(|| loop_of(&self.recent, now));
        let message = "the run reached its statement limit, as a step past its TIME= ends";
        if now == overrun.head && !overrun.lines.is_empty() {
            const SHOWN: usize = 24;
            let mut lines = overrun.lines.iter().take(SHOWN).map(u32::to_string).collect::<Vec<_>>().join(", ");
            if overrun.lines.len() > SHOWN {
                lines += &format!(" and {} more", overrun.lines.len() - SHOWN);
            }
            return Err(Abend { code: crate::abend::AbendCode::TimeLimit, message: format!("{message}, in the loop over lines {lines}"), pos, file: None });
        }
        if overrun.grace == 0 {
            return Err(Abend { code: crate::abend::AbendCode::TimeLimit, message: message.into(), pos, file: None });
        }
        overrun.grace -= 1;
        Ok(())
    }

    pub fn traces(&self, line: u32) -> bool {
        match &self.statements {
            None => false,
            Some(StatementFilter::All) => self.observer.is_some(),
            Some(StatementFilter::Lines(lines)) => self.observer.is_some() && lines.contains(&line),
        }
    }

    pub fn notify(&mut self, event: Event<'_>) {
        if let Event::Sink { kind, .. } = &event {
            debug_assert!(SINK_KINDS.contains(kind), "the sink kind {kind} is not in rt::unit::SINK_KINDS");
        }
        if let Some(observer) = self.observer.as_mut() {
            observer(event);
        }
    }

    /// Closes every file any program left open, as the runtime does when the run unit ends, normally
    /// or by an abend under TRAP(ON). An abend TRAP(OFF) keeps from Language Environment closes
    /// none (`unclosed`), and each VSAM data set stays marked open for output
    /// ([`numeric::assumptions::TRAP_OFF_LEAVES_FILES_OPEN`]).
    pub fn close_all(&mut self, unclosed: bool) -> Result<(), String> {
        for program in &mut self.programs {
            for f in program.files.iter_mut().filter_map(Option::take) {
                end_file(f, unclosed).map_err(|e| format!("closing a file of {}: {e}", program.name))?;
            }
        }
        self.close_external_files(unclosed)
    }

    /// Closes the EXTERNAL files left open, giving the first failure.
    fn close_external_files(&mut self, unclosed: bool) -> Result<(), String> {
        let mut closed = Ok(());
        for (name, &k) in &self.externals.file_names {
            if let Some(f) = self.externals.files[k].take()
                && let Err(e) = end_file(f, unclosed)
            {
                closed = closed.and(Err(format!("closing EXTERNAL file {name}: {e}")));
            }
        }
        closed
    }

    /// Where EXTERNAL record `name` is, or EXTERNAL file `name`'s record area when `file`: storage
    /// of `size` bytes, zeroed, the first time a program describes it. A description of another
    /// size ends the run U4038 with IGZ0066S, or IGZ0075S for a file (assumption C180), except under
    /// gnucobol a shorter record's, which shares the storage with a warning, as cobc's does.
    pub fn external(&mut self, name: &str, file: bool, size: usize, dialect: Dialect, program: &str) -> Result<usize, Abend> {
        let key = (file, name.to_owned());
        if let Some(&(at, had)) = self.externals.storage.get(&key) {
            return if had == size {
                Ok(at)
            } else if size < had && !file && dialect == Dialect::Gnucobol {
                let _ = writeln!(self.err, "ironwork: EXTERNAL record {name} has {had} bytes in the run unit, and this program describes {size}");
                Ok(at)
            } else {
                let message = if file {
                    format!("IGZ0075S Inconsistencies were found in EXTERNAL file {name} in program {program}. The following file attributes did not match those of the established external file: the record length. ({had} bytes in the run unit, {size} here)")
                } else {
                    format!("IGZ0066S The length of external data record {name} in program {program} did not match the existing length of the record. ({had} bytes in the run unit, {size} here)")
                };
                Err(Abend { code: crate::abend::AbendCode::user(4038), message, pos: Pos::default(), file: None })
            };
        }
        let at = self.allocate(size);
        self.externals.storage.insert(key, (at, size));
        Ok(at)
    }

    /// Sets the eight UPSI switches before a program runs, each [`switch_record`] holding 1 for
    /// on, and marks them as input, since the PARM that sets them is (assumption C411).
    pub fn set_switches(&mut self, on: [bool; 8]) {
        for (n, on) in (0..).zip(on) {
            let at = self.push_temporary(&[u8::from(on)]);
            self.externals.storage.insert((false, switch_record(n)), (at, 1));
            self.mark_input(at, 1, true);
        }
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
