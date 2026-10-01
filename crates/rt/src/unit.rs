//! The run unit: every program a run calls, sharing one memory as they share an address space on
//! z/OS. A called program keeps its WORKING-STORAGE and open files from one CALL to the next until
//! it is cancelled. A reference or pointer can reach anywhere in this memory, but never outside it.
//!
//! `H` is the executor's handle to a loaded program and `L` the loader CALL goes through; the run
//! unit holds both without looking inside, and asks `L` what it needs to know about an `H`.

use crate::abend::Abend;
use crate::files::{Dds, Open};
use crate::vocab::{OpenMode, Pos};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

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

/// Where CALL finds programs, and what the run unit needs to know about one it loaded.
pub trait Loader<H> {
    /// The executor's handle to a loaded class definition.
    type Class;

    /// The program whose PROGRAM-ID CALL names, compiled.
    fn program(&mut self, name: &str) -> Result<LoadedProgram<H>, LoadError>;

    /// The PROGRAM-ID of a program not yet loaded that has an ENTRY of this name.
    fn holder(&self, entry: &str) -> Option<String>;

    /// Which of a loaded program's ENTRY statements has this name.
    fn entry(program: &H, name: &str) -> Option<usize>;

    /// A loaded program's file count and storage size.
    fn shape(program: &H) -> (usize, usize);
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
    /// library file or COPY member the operation is in, empty for the first program's own source.
    Sink { kind: &'static str, file: &'a str, line: u32, operand: &'a str },
}

pub type Observer<'w> = Box<dyn FnMut(Event<'_>) + 'w>;

/// How deep PERFORMs and CALLs may nest before the run abends, rather than exhaust the stack.
pub const MAX_DEPTH: usize = 100;

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
    pub oo: crate::oo::Objects<L::Class>,
    /// Told what the run opens, closes and loads, when a caller keeps evidence of it.
    pub observer: Option<Observer<'w>>,
    /// FUNCTION RANDOM's generator, one for the run unit, from the first reference on.
    pub random: Option<u32>,
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
        self.programs.push(Loaded { compiled, name, base, files: (0..files).map(|_| None).collect(), locked: vec![false; files], initialized: false, active: false, entry: None, altered: Vec::new(), source: None });
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

    /// Storage for a BY CONTENT or BY VALUE argument, at the end of memory.
    pub fn push_temporary(&mut self, bytes: &[u8]) -> usize {
        let at = self.allocate(bytes.len());
        self.mem[at..at + bytes.len()].copy_from_slice(bytes);
        at
    }

    /// Releases arguments pushed since `mark`, unless a program or heap storage was placed behind them.
    pub fn release_temporaries(&mut self, mark: usize) {
        if self.programs.iter().all(|p| p.base < mark) && self.le.heap_end() <= mark {
            self.mem.truncate(mark.max(RESERVED));
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
        Ok(())
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
