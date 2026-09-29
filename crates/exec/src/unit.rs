//! The run unit: every program a run calls, sharing one memory as they share an address space on
//! z/OS. A called program keeps its WORKING-STORAGE and open files from one CALL to the next until
//! it is cancelled. A reference or pointer can reach anywhere in this memory, but never outside it.

use crate::files::{Dds, Open};
use crate::Compiled;
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::rc::Rc;
use syntax::ast::Program;
use syntax::copy;

/// A pointer's value is its offset into run-unit memory plus this, so that no item's address is
/// NULL.
pub const ADDRESS_BASE: u32 = 0x0001_0000;
/// RETURN-CODE, a halfword shared by every program in the run unit.
pub const RETURN_CODE: usize = 0;
const RESERVED: usize = 8;
const ALIGNMENT: usize = 8;

pub struct Loaded {
    /// None for the first program, which the caller of the run unit owns.
    pub compiled: Option<Rc<Compiled>>,
    pub name: String,
    pub base: usize,
    pub files: Vec<Option<Open>>,
    pub initialized: bool,
    pub active: bool,
}

/// Where CALL finds programs: the other programs of the first program's source, then program
/// libraries searched by member name.
#[derive(Clone, Debug, Default)]
pub struct Library {
    pub programs: Vec<Program>,
    pub dirs: Vec<PathBuf>,
    pub copy: copy::Libraries,
    pub flags: Vec<String>,
}

pub enum LoadError {
    NotFound,
    Compile(String),
}

#[derive(Clone, Copy, Debug)]
pub enum Clock {
    System,
    /// Seconds since 1970-01-01T00:00:00Z and hundredths, for runs that must repeat exactly.
    Fixed(i64, u32),
}

/// How deep PERFORMs and CALLs may nest before the run abends, rather than exhaust the stack.
pub const MAX_DEPTH: usize = 100;

pub struct RunUnit<'w> {
    pub mem: Vec<u8>,
    /// PERFORMs and CALLs in progress, across every program.
    pub depth: usize,
    pub programs: Vec<Loaded>,
    names: HashMap<String, usize>,
    library: Library,
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
}

fn member_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 30 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '@' || c == '#' || c == '$')
}

impl<'w> RunUnit<'w> {
    pub fn new(library: Library, dds: Dds, sysin: Option<Box<dyn BufRead + 'w>>, clock: Clock, out: &'w mut dyn Write, err: &'w mut dyn Write) -> Self {
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
        }
    }

    fn allocate(&mut self, size: usize) -> usize {
        let base = self.mem.len().div_ceil(ALIGNMENT) * ALIGNMENT;
        self.mem.resize(base + size, 0);
        base
    }

    /// Adds a program to the run unit and gives it its storage.
    pub fn add(&mut self, compiled: Option<Rc<Compiled>>, program: &Program, size: usize) -> usize {
        let base = self.allocate(size);
        let index = self.programs.len();
        let name = program.id.to_ascii_uppercase();
        self.names.insert(name.clone(), index);
        self.programs.push(Loaded { compiled, name, base, files: program.files.iter().map(|_| None).collect(), initialized: false, active: false });
        index
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

    pub fn find(&self, name: &str) -> Option<usize> {
        self.names.get(&name.to_ascii_uppercase()).copied()
    }

    /// The program CALL names, compiling and loading it the first time.
    pub fn load(&mut self, name: &str) -> Result<usize, LoadError> {
        let name = name.to_ascii_uppercase();
        if let Some(i) = self.find(&name) {
            return Ok(i);
        }
        if !member_name(&name) {
            return Err(LoadError::NotFound);
        }
        let program = match self.library.programs.iter().position(|p| p.id.eq_ignore_ascii_case(&name)) {
            Some(i) => self.library.programs.remove(i),
            None => self.search_libraries(&name)?,
        };
        let compiled = crate::compile(program, &self.library.flags).map_err(|errors| {
            let first = errors.first().map(|e| e.place(&name)).unwrap_or_default();
            LoadError::Compile(format!("{name} does not compile: {first}"))
        })?;
        let compiled = Rc::new(compiled);
        let size = compiled.layout.size as usize;
        let program = compiled.program.clone();
        Ok(self.add(Some(compiled), &program, size))
    }

    fn search_libraries(&mut self, name: &str) -> Result<Program, LoadError> {
        let candidates = [name.to_owned(), name.to_ascii_lowercase()];
        let path = self
            .library
            .dirs
            .iter()
            .flat_map(|d| candidates.iter().flat_map(move |n| ["", ".cbl", ".CBL", ".cob", ".COB"].iter().map(move |e| d.join(format!("{n}{e}")))))
            .find(|p| p.is_file())
            .ok_or(LoadError::NotFound)?;
        let text = std::fs::read(&path).map(|b| copy::decode(&b)).map_err(|e| LoadError::Compile(format!("{}: {e}", path.display())))?;
        let mut programs = syntax::parse_all_with(&text, &self.library.copy)
            .map_err(|e| LoadError::Compile(format!("{name} does not compile: {}", e.place(&path.display().to_string()))))?;
        let first = programs.remove(0);
        self.library.programs.extend(programs);
        Ok(first)
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

    pub fn copy_libraries(&self) -> &copy::Libraries {
        &self.library.copy
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

/// Calendar fields of a UTC time: year, month, day, hour, minute, second, day of year (1-based),
/// and day of week (1 Monday to 7 Sunday).
pub fn civil(seconds: i64) -> (i64, u32, u32, u32, u32, u32, u32, u32) {
    let days = seconds.div_euclid(86_400);
    let secs = seconds.rem_euclid(86_400) as u32;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy_march = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy_march + 2) / 153;
    let day = (doy_march - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let before = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334][month as usize - 1] + u32::from(leap && month > 2);
    let weekday = ((days + 3).rem_euclid(7) + 1) as u32;
    (year, month, day, secs / 3600, secs / 60 % 60, secs % 60, before + day, weekday)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil(0), (1970, 1, 1, 0, 0, 0, 1, 4));
        let t = 1_790_510_400;
        let (y, m, d, _, _, _, yday, wday) = civil(t);
        assert_eq!((y, m, d, yday, wday), (2026, 9, 27, 270, 7));
        assert_eq!(civil(951_782_400).6, 60);
    }
}
