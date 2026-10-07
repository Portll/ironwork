//! Running on the VM (`rt::vm`): CALL's loader over load modules and the interpreter's program
//! library, lowering each program it compiles from source, and the run itself, of a compiled
//! program as `Execute::execute_observed` makes the interpreter's, or of a load module's.

use crate::loader::{Library, loads_as};
use crate::lower::{self, LowerError};
use crate::unit::{Clock, Observer, Remains};
use crate::{Compiled, Passed, cics, files, oo, sql};
use rt::abend::{Abend, AbendCode, Ending};
use rt::module::{LoadedModule, Modules};
use rt::oo::{ClassCode, MethodCode, Part};
use rt::unit::{FoundClass, LoadError, LoadedProgram, Loader, RunUnit};
pub use rt::vm::{Code, Halt};
use std::io::{BufRead, Write};
use std::path::Path;
use std::rc::Rc;
use syntax::Pos;
use zarch::ebcdic::CodePage;

/// Where CALL finds a program on the VM (load-module.md §8.2): among the programs already read,
/// those of the run's first source or module first; then `NAME.iwm` in the program libraries; then
/// source there, lowered as CALL loads it.
pub struct VmLibrary {
    pub source: Library,
    pub modules: Modules,
    /// The run's first program came from a module, so its programs come before any source's.
    modules_first: bool,
}

type Found = LoadedProgram<Rc<Code>>;
type FoundCode = FoundClass<Rc<ClassCode<Rc<Code>>>>;

impl VmLibrary {
    /// The library's directories are where `NAME.iwm` is looked for, in their order.
    pub fn new(source: Library) -> Self {
        let modules = Modules::new(source.dirs.clone(), lower::verify);
        Self { source, modules, modules_first: false }
    }

    /// A library for a run that begins with `module`, read from `path`, whose programs come before
    /// any other's.
    pub fn for_module(source: Library, path: &Path, module: LoadedModule) -> Self {
        let mut library = Self::new(source);
        library.modules_first = true;
        library.modules.add_first(path.to_owned(), module);
        library
    }

    /// Program 0 of the module the run began with, or the program `name` as a CALL of it finds
    /// one; Err says why there is none to run.
    fn first_program(&mut self, path: &Path, name: Option<&str>) -> Result<Found, String> {
        let found = match name {
            None => self.modules.take(0, 0).unwrap_or_else(|| Err(format!("{}: the module holds no program", path.display()))),
            Some(name) => self.program(name).map_err(|e| match e {
                LoadError::NotFound => format!("program {name} not found"),
                LoadError::Compile(message) => message,
            }),
        }?;
        match found.compiled.program() {
            Some(p) if p.services.class.is_some() => Err(format!("{} is a class definition: run a program that uses it", p.symbols.get(p.id as usize).cloned().unwrap_or_default())),
            _ => Ok(found),
        }
    }

    /// A program of a source already read, lowered; None when no source read holds one.
    fn read_source(&mut self, name: &str) -> Option<Result<Found, LoadError>> {
        self.source.programs.iter().any(|p| loads_as(p, name)).then(|| self.lowered_source(name))
    }

    fn lowered_source(&mut self, name: &str) -> Result<Found, LoadError> {
        let found = <Library as Loader<Rc<Compiled>>>::program(&mut self.source, name)?;
        Ok(LoadedProgram { compiled: Rc::new(code(&found.compiled)), name: found.name, files: found.files, size: found.size, source: found.source, recorded: Vec::new() })
    }

    /// The class the interpreter's library finds and compiles, with its FACTORY and OBJECT data and
    /// each method lowered as a program of its own, as lowering a class definition lowers them.
    fn source_class(&mut self, external: &str) -> Result<Option<FoundCode>, String> {
        let Some(found) = <Library as Loader<Rc<Compiled>>>::class(&mut self.source, external)? else { return Ok(None) };
        let class = &found.code;
        let lowered = |c: &Rc<Compiled>| Rc::new(code(c));
        let part = |p: &oo::Part| Part { data: lowered(&p.data), records: p.records.clone() };
        let methods = class
            .methods
            .iter()
            .map(|m| MethodCode { name: m.name.clone(), factory: m.factory, params: m.params.clone(), returns: m.returns.clone(), code: lowered(&m.code), own_records: m.own_records })
            .collect();
        let code = ClassCode { parent: class.parent.clone(), factory: class.factory.as_ref().map(part), object: class.object.as_ref().map(part), methods };
        Ok(Some(FoundClass { code: Rc::new(code), sources: found.sources }))
    }
}

impl Loader<Rc<Code>> for VmLibrary {
    fn program(&mut self, name: &str) -> Result<Found, LoadError> {
        if !self.modules_first
            && let Some(found) = self.read_source(name)
        {
            return found;
        }
        if let Some(found) = self.modules.loaded(name) {
            return found;
        }
        if let Some(found) = self.read_source(name) {
            return found;
        }
        match self.modules.search(name) {
            Err(LoadError::NotFound) => self.lowered_source(name),
            found => found,
        }
    }

    fn holder(&self, entry: &str) -> Option<String> {
        let source = || <Library as Loader<Rc<Compiled>>>::holder(&self.source, entry);
        if self.modules_first { self.modules.holder(entry).or_else(source) } else { source().or_else(|| self.modules.holder(entry)) }
    }

    fn entry(program: &Rc<Code>, name: &str) -> Option<usize> {
        program.entry(name)
    }

    fn shape(program: &Rc<Code>) -> (usize, usize) {
        program.shape()
    }

    fn nested(program: &Rc<Code>) -> &[String] {
        program.nested()
    }

    fn source(program: &Rc<Code>, file: usize) -> Option<String> {
        let p = program.program()?;
        p.debug.sources.get(file).map(|&s| p.symbols[s as usize].clone())
    }

    fn class(&mut self, external: &str) -> Result<Option<FoundCode>, String> {
        let read_source = self.source.programs.iter().any(|p| oo::defined_class(p).as_deref() == Some(external));
        if (!read_source || self.modules_first)
            && let Some(found) = self.modules.loaded_class(external)?
        {
            return Ok(Some(found));
        }
        if !read_source && let Some(found) = self.modules.search_class(external)? {
            return Ok(Some(found));
        }
        self.source_class(external)
    }

    /// A mapset a module already read holds, else one from the copy libraries.
    fn mapset(&mut self, name: &str) -> Option<Result<rt::bms::Mapset, String>> {
        self.modules.mapset(name).map(Ok).or_else(|| self.source.mapset(name))
    }
}

/// A compiled program as the VM holds it: lowered, or with the construct lowering refused.
pub fn code(compiled: &Compiled) -> Code {
    let lowered = lower::lower(compiled).map_err(|e| match e {
        LowerError::Unsupported(what, _) => what.to_owned(),
        other => other.to_string(),
    });
    held(compiled, lowered)
}

/// A compiled program lowered for the VM, or why it does not lower, with where.
pub fn lowered(compiled: &Compiled) -> Result<Code, LowerError> {
    Ok(held(compiled, Ok(lower::lower(compiled)?)))
}

fn held(compiled: &Compiled, lowered: Result<rt::lir::Program, String>) -> Code {
    let entries = compiled.entries.iter().map(|e| e.name.clone()).collect();
    let method = compiled.program.oo.as_deref().and_then(|o| o.method()).map(|m| format!("{}.{}", m.class, m.name));
    Code::new(lowered, entries, compiled.program.files.len(), compiled.layout.size as usize, compiled.program.nested.clone(), method)
}

/// A run unit on the VM over `library`, tracing and limited as the library says.
#[allow(clippy::too_many_arguments)]
fn run_unit<'w>(
    library: VmLibrary,
    dds: files::Dds,
    sysin: Option<Box<dyn BufRead + 'w>>,
    clock: Clock,
    database: Option<&'w mut (dyn sql::Database + '_)>,
    out: &'w mut dyn Write,
    err: &'w mut dyn Write,
    observer: Option<Observer<'w>>,
) -> RunUnit<'w, Rc<Code>, VmLibrary> {
    let (statements, taint) = (library.source.trace_statements.clone(), library.source.trace_input.then(rt::taint::Taint::default));
    let (limits, screen, environment) = ((library.source.statement_limit, library.source.time_limit, library.source.storage_limit), library.source.screen.clone(), library.source.environment.clone());
    let mut run_unit = RunUnit::new(library, dds, sysin, clock, out, err);
    run_unit.crt = screen;
    run_unit.environment = rt::environment::Environment::of(environment);
    run_unit.observer = observer;
    run_unit.statements = statements;
    run_unit.taint = taint;
    run_unit.limit(limits.0, limits.1, limits.2);
    run_unit.sql = database.map(sql::Session::new);
    run_unit
}

/// Runs the first program of `run_unit`, `me`, as `code`, given `passed`, then settles the
/// database and closes every file, unless an abend the PARM's TRAP(OFF) keeps from Language
/// Environment ended it. `addresses` takes where each argument passed is.
fn run_main(code: &Code, id: &str, me: usize, run_unit: &mut RunUnit<'_, Rc<Code>, VmLibrary>, passed: Passed<'_>, page: &CodePage, addresses: &mut Vec<Option<usize>>) -> Result<(Ending, i16), Halt> {
    let trap_off = matches!(passed, Passed::Parm(p) if rt::le::parm::trap_off(p));
    passed.apply_parm(run_unit);
    *addresses = passed.addresses(run_unit, page);
    let last_paragraph = code.program().and_then(|p| p.paragraphs.last().and_then(|para| p.debug.positions.get(para.at as usize).copied()));
    let ending = rt::vm::run(code, me, run_unit, addresses, passed.main()).and_then(|e| crate::past_the_end(e, passed.main(), id, last_paragraph).map_err(Halt::Abend));
    let settled = run_unit.sql.as_mut().map_or(Ok(()), |s| s.settle(id, ending.is_ok()).map(drop));
    let closed = run_unit.close_all(matches!(&ending, Err(Halt::Abend(a)) if a.code.skips_termination(trap_off)));
    let ending = ending?;
    settled.map_err(|a| Abend { code: a.code.into(), message: a.message, pos: Pos::default(), file: None })?;
    closed.map_err(|m| Abend { code: AbendCode::Ironwork, message: m, pos: Pos::default(), file: None })?;
    Ok((ending, run_unit.return_code()))
}

/// Runs `compiled`, lowered as `code`, as the first program of a run unit on the VM, given
/// `passed` as `Execute::execute_main` and `Execute::execute_with_arguments` take it; `kept`
/// takes what the run left in its run unit.
#[allow(clippy::too_many_arguments)]
pub fn execute<'w>(
    compiled: &Compiled,
    code: &Code,
    library: Library,
    dds: files::Dds,
    sysin: Option<Box<dyn BufRead + 'w>>,
    clock: Clock,
    database: Option<&'w mut (dyn sql::Database + '_)>,
    out: &'w mut dyn Write,
    err: &'w mut dyn Write,
    observer: Option<Observer<'w>>,
    passed: Passed<'_>,
    kept: &mut Option<Remains>,
) -> Result<(Ending, i16), Halt> {
    oo::refuse_to_run(&compiled.program)?;
    let mut run_unit = run_unit(VmLibrary::new(library), dds, sysin, clock, database, out, err, observer);
    let me = run_unit.add_named(None, compiled.program.id.to_ascii_uppercase(), compiled.program.files.len(), compiled.layout.size as usize);
    let mut addresses = Vec::new();
    let ran = run_main(code, &compiled.program.id, me, &mut run_unit, passed, compiled.options.code_page(), &mut addresses);
    *kept = Some(Remains { arguments: passed.returned(&addresses, &run_unit.mem), ..Remains::of(&run_unit) });
    ran
}

fn refused(message: String) -> Halt {
    Halt::Abend(Abend::ironwork(message, Pos::default()))
}

/// The source names of a program's debug table, which an abend's position indexes.
fn sources(program: &rt::lir::Program) -> Vec<String> {
    program.debug.sources.iter().map(|&s| program.symbols.get(s as usize).cloned().unwrap_or_default()).collect()
}

/// Runs program 0 of `module`, read from `path`, as the first program of a run unit on the VM,
/// with a job step's `parm`, telling `observer` what the run does. CALL finds the module's other
/// programs before any other, then as [`VmLibrary`] finds them in `library`'s directories.
#[allow(clippy::too_many_arguments)]
pub fn execute_module<'w>(
    module: LoadedModule,
    path: &Path,
    library: Library,
    dds: files::Dds,
    sysin: Option<Box<dyn BufRead + 'w>>,
    clock: Clock,
    database: Option<&'w mut (dyn sql::Database + '_)>,
    out: &'w mut dyn Write,
    err: &'w mut dyn Write,
    observer: Option<Observer<'w>>,
    parm: Option<&str>,
) -> Result<(Ending, i16), Halt> {
    let mut library = VmLibrary::for_module(library, path, module);
    let main = library.first_program(path, None).map_err(refused)?;
    let code = main.compiled;
    let Some(program) = code.program() else { return Err(refused(format!("{}: program 0 is not lowered", path.display()))) };
    let id = program.symbols.get(program.id as usize).cloned().unwrap_or_default();
    let page = program.options.options.code_page();
    let mut run_unit = run_unit(library, dds, sysin, clock, database, out, err, observer);
    let me = run_unit.add_named(None, main.name, main.files, main.size);
    run_main(&code, &id, me, &mut run_unit, parm.map_or(Passed::Nothing, Passed::Parm), page, &mut Vec::new())
}

/// Whether a CICS task of a run that began with `module` can begin with the program `name`, as a
/// CALL of it finds one, and if so the files a module records for it ([`LoadedProgram::recorded`]);
/// Err says why not.
pub fn module_program(module: &LoadedModule, path: &Path, library: Library, name: &str) -> Result<Vec<(String, Option<rt::module::SourceFile>)>, String> {
    VmLibrary::for_module(library, path, module.clone()).first_program(path, Some(name)).map(|found| found.recorded)
}

/// Runs a CICS task on the VM whose first program is program 0 of `module`, read from `path`, or
/// with `name` the program a CALL of that name finds, as [`execute_cics`] runs a compiled one.
/// Returns the task however the run ended, and the source names of its first program's debug
/// table, its own source by path when a program library supplied it.
#[allow(clippy::too_many_arguments)]
pub fn execute_module_cics<'w>(
    module: &LoadedModule,
    path: &Path,
    name: Option<&str>,
    library: Library,
    dds: files::Dds,
    task: cics::Task,
    clock: Clock,
    database: Option<&'w mut (dyn sql::Database + '_)>,
    out: &'w mut dyn Write,
    err: &'w mut dyn Write,
    observer: Option<Observer<'w>>,
) -> (Result<Ending, Halt>, cics::Task, Vec<String>) {
    let mut library = VmLibrary::for_module(library, path, module.clone());
    let found = match library.first_program(path, name) {
        Ok(found) => found,
        Err(message) => return (Err(refused(message)), task, Vec::new()),
    };
    let code = found.compiled;
    let program = code.program();
    let mut names = program.map(sources).unwrap_or_default();
    if let (Some(own), Some(path)) = (names.first_mut(), &found.source) {
        *own = path.display().to_string();
    }
    let id = program.and_then(|p| p.symbols.get(p.id as usize).cloned()).unwrap_or_default();
    let page = program.map_or_else(|| numeric::Options::default().code_page(), |p| p.options.options.code_page());
    let first = crate::First { id: &id, name: found.name, files: found.files, size: found.size, page, source: found.source };
    let run_unit = run_unit(library, dds, None, clock, database, out, err, observer);
    let (ending, ended, task) = crate::run_task(first, run_unit, task, &mut None, |unit, me, commarea, length| rt::vm::run_task(&code, me, unit, commarea, length));
    let ending = match ending {
        Err(Halt::Abend(abend)) => Err(Halt::Abend(crate::asra(abend))),
        other => other,
    };
    (ending.and_then(|e| ended.map(|()| e).map_err(Halt::Abend)), task, names)
}

/// Runs `compiled`, lowered as `code`, on the VM as the first program of a CICS task, as
/// `Execute::execute_cics_observed` runs it on the interpreter; `kept` takes what the run left in
/// its run unit, and the task comes back however the run ended.
#[allow(clippy::too_many_arguments)]
pub fn execute_cics<'w>(
    compiled: &Compiled,
    code: &Code,
    library: Library,
    dds: files::Dds,
    task: cics::Task,
    clock: Clock,
    database: Option<&'w mut (dyn sql::Database + '_)>,
    out: &'w mut dyn Write,
    err: &'w mut dyn Write,
    observer: Option<Observer<'w>>,
    kept: &mut Option<Remains>,
) -> (Result<Ending, Halt>, cics::Task) {
    if let Err(abend) = oo::refuse_to_run(&compiled.program) {
        return (Err(abend.into()), task);
    }
    let run_unit = run_unit(VmLibrary::new(library), dds, None, clock, database, out, err, observer);
    let (ending, ended, task) = crate::run_task(crate::First::of(compiled), run_unit, task, kept, |unit, me, commarea, length| rt::vm::run_task(code, me, unit, commarea, length));
    let ending = match ending {
        Err(Halt::Abend(abend)) => Err(Halt::Abend(crate::asra(abend))),
        other => other,
    };
    (ending.and_then(|e| ended.map(|()| e).map_err(Halt::Abend)), task)
}
