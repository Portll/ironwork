//! The interpreter's program loader: the programs CALL can reach, read from source and compiled
//! the first time one is called.

use crate::oo::ClassCode;
use crate::unit::{LoadError, LoadedProgram, Loader, RunUnit};
use crate::Compiled;
use std::path::PathBuf;
use std::rc::Rc;
use syntax::ast::Program;
use syntax::copy;

/// Where CALL finds programs: the other programs of the first program's source, then program
/// libraries searched by member name.
#[derive(Clone, Debug, Default)]
pub struct Library {
    pub programs: Vec<Program>,
    pub dirs: Vec<PathBuf>,
    pub copy: copy::Libraries,
    pub flags: Vec<String>,
}

fn member_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 30 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '@' || c == '#' || c == '$')
}

impl Library {
    fn search(&mut self, name: &str) -> Result<(Program, PathBuf), LoadError> {
        let candidates = [name.to_owned(), name.to_ascii_lowercase()];
        let path = self
            .dirs
            .iter()
            .flat_map(|d| candidates.iter().flat_map(move |n| ["", ".cbl", ".CBL", ".cob", ".COB"].iter().map(move |e| d.join(format!("{n}{e}")))))
            .find(|p| p.is_file())
            .ok_or(LoadError::NotFound)?;
        let text = std::fs::read(&path).map(|b| copy::decode(&b)).map_err(|e| LoadError::Compile(format!("{}: {e}", path.display())))?;
        let mut programs = syntax::parse_all_with(&text, &self.copy.with_program(&path))
            .map_err(|e| LoadError::Compile(format!("{name} does not compile: {}", e.place(&path.display().to_string()))))?;
        let first = programs.remove(0);
        self.programs.extend(programs);
        Ok((first, path))
    }
}

impl Loader<Rc<Compiled>> for Library {
    type Class = Rc<ClassCode>;

    fn program(&mut self, name: &str) -> Result<LoadedProgram<Rc<Compiled>>, LoadError> {
        if !member_name(name) {
            return Err(LoadError::NotFound);
        }
        let (program, source) = match self.programs.iter().position(|p| p.id.eq_ignore_ascii_case(name)) {
            Some(i) => (self.programs.remove(i), None),
            None => self.search(name).map(|(p, path)| (p, Some(path)))?,
        };
        let compiled = crate::compile(program, &self.flags).map_err(|errors| {
            let first = syntax::most_severe(&errors).map(|e| e.place(name)).unwrap_or_default();
            LoadError::Compile(format!("{name} does not compile: {first}"))
        })?;
        let compiled = Rc::new(compiled);
        let (files, size) = Self::shape(&compiled);
        Ok(LoadedProgram { name: compiled.program.id.to_ascii_uppercase(), files, size, source, compiled })
    }

    fn holder(&self, entry: &str) -> Option<String> {
        self.programs.iter().find(|p| crate::entry_points(p).iter().any(|e| e.name == entry)).map(|p| p.id.to_ascii_uppercase())
    }

    fn entry(program: &Rc<Compiled>, name: &str) -> Option<usize> {
        program.entries.iter().position(|e| e.name == name)
    }

    fn shape(program: &Rc<Compiled>) -> (usize, usize) {
        (program.program.files.len(), program.layout.size as usize)
    }
}

/// Adding a program to the interpreter's run unit by its source's PROGRAM-ID and FILE-CONTROL.
pub trait AddProgram {
    fn add(&mut self, compiled: Option<Rc<Compiled>>, program: &Program, size: usize) -> usize;
}

impl AddProgram for RunUnit<'_> {
    fn add(&mut self, compiled: Option<Rc<Compiled>>, program: &Program, size: usize) -> usize {
        self.add_named(compiled, program.id.to_ascii_uppercase(), program.files.len(), size)
    }
}
