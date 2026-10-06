//! The interpreter's program loader: the programs CALL can reach, read from source and compiled
//! the first time one is called.

use crate::oo::ClassCode;
use crate::unit::{LoadError, LoadedProgram, Loader, RunUnit};
use crate::Compiled;
use rt::unit::FoundClass;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use syntax::ast::Program;
use syntax::copy;

/// Where CALL finds programs: the other programs of the first program's source, then program
/// libraries searched by member name.
#[derive(Clone, Debug, Default)]
pub struct Library {
    /// The programs not yet loaded: the first program's source's own, then those read from a
    /// library file, which name that file as their first source.
    pub programs: Vec<Program>,
    pub dirs: Vec<PathBuf>,
    pub copy: copy::Libraries,
    pub flags: Vec<String>,
    /// The statements whose start the run unit tells its observer of (`RunUnit::statements`).
    pub trace_statements: Option<rt::unit::StatementFilter>,
    /// Whether the run unit follows which bytes may hold input (`RunUnit::taint`).
    pub trace_input: bool,
    /// How many statements may start before the run ends with S322 (`RunUnit::statement_limit`).
    pub statement_limit: Option<u64>,
    /// The screen positioned DISPLAY and ACCEPT use, with the operator a screen script plays;
    /// None gives each run a blank screen with no operator.
    pub screen: Option<std::rc::Rc<std::cell::RefCell<rt::crt::Crt>>>,
    /// Each PROGRAM-ID the source files of `dirs` hold, with its file, read the first time a CALL
    /// names no member.
    pub program_ids: Option<Vec<(String, PathBuf)>>,
}

/// The extensions of a program library's source members.
const SOURCE_EXTENSIONS: &[&str] = &["cbl", "CBL", "cob", "COB"];

impl Library {
    /// The program `name` in the member of that name, or else in a source file whose PROGRAM-ID
    /// it is.
    fn search(&mut self, name: &str) -> Result<(Program, PathBuf), LoadError> {
        let candidates = [name.to_owned(), name.to_ascii_lowercase()];
        let member = self
            .dirs
            .iter()
            .flat_map(|d| candidates.iter().flat_map(move |n| std::iter::once(String::new()).chain(SOURCE_EXTENSIONS.iter().map(|e| format!(".{e}"))).map(move |e| d.join(format!("{n}{e}")))))
            .find(|p| p.is_file());
        if let Some(path) = member {
            let mut programs = self.read(name, &path)?;
            let wanted = programs.iter().position(|p| loads_as(p, name)).or_else(|| programs.iter().position(|p| !p.is_prototype())).unwrap_or(0);
            let found = programs.remove(wanted);
            self.add_read(&path, programs);
            return Ok((found, path));
        }
        for path in self.holding(name) {
            let mut programs = self.read(name, &path)?;
            if let Some(wanted) = programs.iter().position(|p| loads_as(p, name)) {
                let found = programs.remove(wanted);
                self.add_read(&path, programs);
                return Ok((found, path));
            }
        }
        Err(LoadError::NotFound)
    }

    fn read(&self, name: &str, path: &Path) -> Result<Vec<Program>, LoadError> {
        let text = std::fs::read(path).map(|b| copy::decode(&b)).map_err(|e| LoadError::Compile(format!("{}: {e}", path.display())))?;
        syntax::parse_all_with(&text, &self.copy.with_program(path)).map_err(|e| LoadError::Compile(format!("{name} does not compile: {}", e.place(&path.display().to_string()))))
    }

    /// The source files of the program libraries whose PROGRAM-ID is `name`, the directories in
    /// order and each one's files by name (assumption C441).
    fn holding(&mut self, name: &str) -> Vec<PathBuf> {
        let (dirs, compliance) = (&self.dirs, self.copy.compliance());
        let index = self.program_ids.get_or_insert_with(|| {
            let mut index = Vec::new();
            for dir in dirs {
                let Ok(entries) = std::fs::read_dir(dir) else { continue };
                let mut files: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_file() && p.extension().is_some_and(|e| SOURCE_EXTENSIONS.iter().any(|x| e == *x))).collect();
                files.sort();
                for file in files {
                    let Ok(bytes) = std::fs::read(&file) else { continue };
                    index.extend(syntax::program_ids(&copy::decode(&bytes), compliance).into_iter().map(|id| (id, file.clone())));
                }
            }
            index
        });
        let mut paths: Vec<PathBuf> = index.iter().filter(|(id, _)| id.eq_ignore_ascii_case(name)).map(|(_, path)| path.clone()).collect();
        paths.dedup();
        paths
    }

    /// The program a CALL of `name` would load, parsed, not compiled, and kept for that CALL; None
    /// when no program answers it or its source does not parse.
    pub fn find(&mut self, name: &str) -> Option<&Program> {
        if !rt::module::member_name(name) {
            return None;
        }
        if !self.programs.iter().any(|p| loads_as(p, name)) {
            let (program, path) = self.search(name).ok()?;
            self.add_read(&path, vec![program]);
        }
        self.programs.iter().find(|p| loads_as(p, name))
    }

    /// Keeps programs read from the library file `path` for a later CALL.
    pub fn add_read(&mut self, path: &Path, programs: Vec<Program>) {
        let shown = path.display().to_string();
        self.programs.extend(programs.into_iter().map(|mut p| {
            if let Some(own) = p.sources.first_mut() {
                own.clone_from(&shown);
            }
            p
        }));
    }
}

/// Whether a CALL or function invocation of `name` loads `program`: by PROGRAM-ID, or a function
/// definition by its external name; a prototype has no code to load.
pub(crate) fn loads_as(program: &Program, name: &str) -> bool {
    !program.is_prototype() && program.load_name().eq_ignore_ascii_case(name)
}

impl Loader<Rc<Compiled>> for Library {
    fn program(&mut self, name: &str) -> Result<LoadedProgram<Rc<Compiled>>, LoadError> {
        if !rt::module::member_name(name) {
            return Err(LoadError::NotFound);
        }
        let (program, source) = match self.programs.iter().position(|p| loads_as(p, name)) {
            Some(i) => {
                let program = self.programs.remove(i);
                let source = program.sources.first().filter(|s| !s.is_empty()).map(PathBuf::from);
                (program, source)
            }
            None => self.search(name).map(|(p, path)| (p, Some(path)))?,
        };
        let compiled = crate::compile(program, &self.flags).map_err(|errors| {
            let first = syntax::most_severe(&errors).map(|e| e.place(name)).unwrap_or_default();
            LoadError::Compile(format!("{name} does not compile: {first}"))
        })?;
        let compiled = Rc::new(compiled);
        let (files, size) = Self::shape(&compiled);
        Ok(LoadedProgram { name: compiled.program.load_name().to_ascii_uppercase(), files, size, source, compiled, recorded: Vec::new() })
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

    fn nested(program: &Rc<Compiled>) -> &[String] {
        &program.program.nested
    }

    fn source(program: &Rc<Compiled>, file: usize) -> Option<String> {
        program.program.sources.get(file).cloned()
    }

    fn class(&mut self, external: &str) -> Result<Option<FoundClass<Rc<ClassCode>>>, String> {
        let Some((program, path)) = crate::oo::find_class(self, external)? else { return Ok(None) };
        let at = crate::compile_time().map_err(|m| format!("class {external} does not compile: {m}"))?;
        let (code, _) = crate::oo::class_code(&program, &self.flags, at).map_err(|errors| {
            let first = syntax::most_severe(&errors).map(|e| e.place(external)).unwrap_or_default();
            format!("class {external} does not compile: {first}")
        })?;
        let mut sources = program.sources;
        if let (Some(own), Some(path)) = (sources.first_mut(), path) {
            *own = path;
        }
        Ok(Some(FoundClass { code: Rc::new(code), sources }))
    }

    fn mapset(&mut self, name: &str) -> Option<Result<rt::bms::Mapset, String>> {
        syntax::bms::find_mapset(&self.copy, name).map(|found| found.map_err(|e| e.message))
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
