//! The load modules a run has read and the directories it reads more from (load-module.md §8.2):
//! a program found by the name a CALL or a function invocation gives, and a class by its external
//! name, each as the VM holds it.

use super::{DirectoryEntry, LoadedModule, read};
use crate::bms::Mapset;
use crate::lir::{Class, ClassPart, Program, SymId};
use crate::oo::{ClassCode, JAVA_LANG_OBJECT, MethodCode, Part};
use crate::unit::{FoundClass, LoadError, LoadedProgram};
use crate::vm::Code;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// What a program from a module must pass before the VM runs it, since a module is untrusted input.
pub type Check = fn(&Program) -> Result<(), String>;

/// Whether `name` can name a library member; any other name never reaches the filesystem.
pub fn member_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 30 && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '@' | '#' | '$'))
}

pub struct Modules {
    /// Searched in order for `NAME.iwm`.
    pub dirs: Vec<PathBuf>,
    check: Check,
    read: Vec<Read>,
}

/// A module the run has read: its directory, the programs no CALL has taken yet, and its mapsets.
struct Read {
    path: PathBuf,
    directory: Vec<DirectoryEntry>,
    programs: Vec<Option<Program>>,
    mapsets: Vec<Mapset>,
}

type Found = LoadedProgram<Rc<Code>>;
type FoundCode = FoundClass<Rc<ClassCode<Rc<Code>>>>;

impl Modules {
    pub fn new(dirs: Vec<PathBuf>, check: Check) -> Self {
        Self { dirs, check, read: Vec::new() }
    }

    /// Registers a module read from `path`, whose programs are found by name from now on; its
    /// number is what [`Modules::take`] takes.
    pub fn add(&mut self, path: PathBuf, module: LoadedModule) -> usize {
        let LoadedModule { directory, programs, mapsets, .. } = module;
        self.read.push(Read { path, directory, programs: programs.into_iter().map(Some).collect(), mapsets });
        self.read.len() - 1
    }

    /// Program `ordinal` of module `k` for the run unit, checked; None once taken.
    pub fn take(&mut self, k: usize, ordinal: usize) -> Option<Result<Found, String>> {
        let held = self.read.get_mut(k)?;
        let program = held.programs.get_mut(ordinal)?.take()?;
        let entry = &held.directory[ordinal];
        let nested = held.directory.iter().filter(|e| e.parent == Some(ordinal as u32)).map(|e| e.id.clone()).collect();
        let name = entry.load_name().to_ascii_uppercase();
        let checked = (self.check)(&program).map_err(|e| format!("{}: program {}: {e}", held.path.display(), entry.id));
        Some(checked.map(|()| {
            let code = code(program, nested, None);
            let (files, size) = code.shape();
            LoadedProgram { compiled: Rc::new(code), name, files, size, source: None }
        }))
    }

    /// A program of a module already read that no CALL has taken, by the name a CALL gives: the
    /// first such module, and in it the lowest ordinal.
    pub fn loaded(&mut self, name: &str) -> Option<Result<Found, LoadError>> {
        let (k, ordinal) = self.read.iter().enumerate().find_map(|(k, m)| m.untaken(|e| e.load_name().eq_ignore_ascii_case(name)).map(|o| (k, o)))?;
        self.take(k, ordinal).map(|found| found.map_err(LoadError::Compile))
    }

    /// `NAME.iwm`, then `name.iwm`, in each directory in turn. A module found that cannot be read,
    /// or that holds no program `NAME`, is an error, not "not found".
    pub fn search(&mut self, name: &str) -> Result<Found, LoadError> {
        if !member_name(name) {
            return Err(LoadError::NotFound);
        }
        let Some(path) = self.file(&[name.to_owned(), name.to_ascii_lowercase()]) else { return Err(LoadError::NotFound) };
        let module = open(&path).map_err(LoadError::Compile)?;
        let Some(ordinal) = module.directory.iter().position(|e| e.load_name().eq_ignore_ascii_case(name)) else {
            return Err(LoadError::Compile(format!("{}: the module holds no program {name}", path.display())));
        };
        let k = self.add(path, module);
        match self.take(k, ordinal) {
            Some(found) => found.map_err(LoadError::Compile),
            None => Err(LoadError::NotFound),
        }
    }

    /// The PROGRAM-ID of a program not yet taken, of a module already read, with this ENTRY name.
    pub fn holder(&self, entry: &str) -> Option<String> {
        self.read.iter().find_map(|m| m.untaken(|e| e.entries.iter().any(|(n, _)| n == entry)).map(|o| m.directory[o].id.to_ascii_uppercase()))
    }

    /// The COBOL class definition of this external name in a module already read, not yet taken.
    pub fn loaded_class(&mut self, external: &str) -> Result<Option<FoundCode>, String> {
        match self.read.iter().enumerate().find_map(|(k, m)| m.defining(external).map(|o| (k, o))) {
            Some(found) => self.take_class(found).map(Some),
            None => Ok(None),
        }
    }

    /// The COBOL class definition of this external name in a module named as the source search
    /// names the class's source (`numeric::assumptions::CLASS_SEARCH`), in each directory in turn.
    pub fn search_class(&mut self, external: &str) -> Result<Option<FoundCode>, String> {
        if external == JAVA_LANG_OBJECT {
            return Ok(None);
        }
        let member = |n: &str| !n.is_empty() && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
        let simple = external.rsplit('.').next().unwrap_or(external).to_owned();
        let mut names = vec![simple, external.replace('.', "_")];
        names.dedup();
        names.retain(|n| member(n));
        let variants: Vec<String> = names.iter().flat_map(|n| [n.clone(), n.to_ascii_lowercase(), n.to_ascii_uppercase()]).collect();
        for dir in self.dirs.clone() {
            for variant in &variants {
                let path = dir.join(format!("{variant}.iwm"));
                if !path.is_file() {
                    continue;
                }
                let module = open(&path)?;
                let Some(ordinal) = module.programs.iter().position(|p| defines(p, external)) else { continue };
                let k = self.add(path, module);
                return self.take_class((k, ordinal)).map(Some);
            }
        }
        Ok(None)
    }

    /// A mapset a module already read holds.
    pub fn mapset(&self, name: &str) -> Option<Mapset> {
        self.read.iter().find_map(|m| m.mapsets.iter().find(|s| s.name.eq_ignore_ascii_case(name))).cloned()
    }

    fn file(&self, names: &[String]) -> Option<PathBuf> {
        self.dirs.iter().flat_map(|d| names.iter().map(move |n| d.join(format!("{n}.iwm")))).find(|p| p.is_file())
    }

    /// The class program at `(k, ordinal)`, checked, as the run unit's class table holds it, with
    /// its source table.
    fn take_class(&mut self, (k, ordinal): (usize, usize)) -> Result<FoundCode, String> {
        let held = &mut self.read[k];
        let Some(mut program) = held.programs[ordinal].take() else { return Err(format!("{}: program {ordinal} is taken", held.path.display())) };
        (self.check)(&program).map_err(|e| format!("{}: class {}: {e}", held.path.display(), held.directory[ordinal].id))?;
        let Some(class) = program.services.class.take() else { return Err(format!("{}: program {ordinal} is not a class", held.path.display())) };
        let Class { parent, factory, object, methods, .. } = *class;
        let sym = |id: SymId| symbol(&program, id);
        let name = sym(program.id);
        let part = |p: ClassPart| Part { data: Rc::new(code(p.data, Vec::new(), None)), records: p.records };
        let methods = methods
            .into_iter()
            .map(|m| {
                let method = sym(m.name);
                MethodCode {
                    factory: m.factory,
                    params: m.params.iter().map(|&p| sym(p)).collect(),
                    returns: m.returns.map(sym),
                    own_records: usize::from(m.own_records),
                    code: Rc::new(code(m.code, Vec::new(), Some(format!("{name}.{method}")))),
                    name: method,
                }
            })
            .collect();
        let code = ClassCode { parent: sym(parent), factory: factory.map(part), object: object.map(part), methods };
        let sources = program.debug.sources.iter().map(|&s| sym(s)).collect();
        Ok(FoundClass { code: Rc::new(code), sources })
    }
}

impl Read {
    /// The lowest ordinal not yet taken whose directory entry `wanted` accepts.
    fn untaken(&self, wanted: impl Fn(&DirectoryEntry) -> bool) -> Option<usize> {
        self.directory.iter().zip(&self.programs).position(|(e, p)| p.is_some() && wanted(e))
    }

    fn defining(&self, external: &str) -> Option<usize> {
        self.programs.iter().position(|p| p.as_ref().is_some_and(|p| defines(p, external)))
    }
}

fn defines(program: &Program, external: &str) -> bool {
    program.services.class.as_ref().is_some_and(|c| symbol(program, c.external) == external)
}

fn symbol(program: &Program, id: SymId) -> String {
    program.symbols.get(id as usize).cloned().unwrap_or_default()
}

/// A program from a module as the VM holds it, with the PROGRAM-IDs of the programs it directly
/// contains and, for a method, the `Class.method` a dump lists it by.
fn code(program: Program, nested: Vec<String>, method: Option<String>) -> Code {
    let entries = program.services.entries.iter().map(|e| symbol(&program, e.name)).collect();
    let (files, size) = (program.services.files.len(), program.storage.size as usize);
    Code::new(Ok(program), entries, files, size, nested, method)
}

/// The module at `path`, read and checked, or why not with the path in front.
fn open(path: &Path) -> Result<LoadedModule, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    read(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}
