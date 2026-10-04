//! `ironwork run x.iwm` and `ironwork cics x.iwm`: program 0 of a load module on the VM, as the
//! first program of a run unit or of a CICS task (load-module.md §8.2), with the coverage report
//! and evidence journal a run of its source gives, from what the module records.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::rc::Rc;

use exec::abend::{AbendCode, Signal};
use exec::module::LoadedModule;
use exec::vm::Halt;

use crate::coverage::{Coverage, Outline};
use crate::evidence;
use crate::exit::{self, Outcome};

/// `--evidence` and the traces that go with it.
pub struct Evidence {
    pub dir: PathBuf,
    pub marker: Option<String>,
    pub statements: BTreeSet<(String, u32)>,
    pub input: bool,
}

pub struct Request<'a> {
    pub command: &'a str,
    pub path: &'a str,
    pub bytes: &'a [u8],
    pub library: exec::unit::Library,
    /// The directories the run reads: the module's own, each `-I`, each `-L`.
    pub reads: Vec<PathBuf>,
    pub dds: exec::files::Dds,
    pub clock: exec::unit::Clock,
    pub database: Option<Box<dyn exec::sql::Database>>,
    pub parm: Option<&'a str>,
    pub options: &'a [(String, String)],
    pub evidence: Option<Evidence>,
    pub coverage: Option<PathBuf>,
}

/// A module the reader refuses, or whose program 0 does not pass the checks a program from a module
/// must, does not run, and neither does one whose program 0 is a user-defined function.
pub fn run(r: Request<'_>) -> ExitCode {
    let path = r.path;
    let module = match exec::module::read(r.bytes) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("ironwork: {path}: {e}");
            return exit::status(Outcome::Unreadable);
        }
    };
    let Some(main) = module.programs.first() else {
        eprintln!("ironwork: {path}: the module holds no program");
        return exit::status(Outcome::Unreadable);
    };
    let symbol = |id: u32| main.symbols.get(id as usize).cloned().unwrap_or_default();
    if let Err(e) = exec::lower::verify(main) {
        eprintln!("ironwork: {path}: program {}: {e}", symbol(main.id));
        return exit::status(Outcome::Unreadable);
    }
    if main.services.function.is_some() {
        eprintln!("ironwork: {path}: FUNCTION-ID {}: the module holds user-defined functions and no program to run", symbol(main.id));
        return exit::status(Outcome::Refused);
    }
    // A program from source that CALL or a transaction loads compiles at the module's compliance level.
    let library = match main.options.options.compliance {
        numeric::Compliance::Strict => r.library,
        level => exec::unit::Library { copy: r.library.copy.with_compliance(level), flags: [r.library.flags, vec![level.flag().to_owned()]].concat(), ..r.library },
    };
    let journal = match &r.evidence {
        Some(e) => match evidence::start(&e.dir, &r.reads, r.command, path) {
            Ok(mut j) => {
                evidence::recorded_sources(&mut j, module.files.first().map(Vec::as_slice).unwrap_or_default());
                let names = recorded_names(&module);
                let run = evidence::Run::new(j, &r.reads, path, e.marker.as_deref()).with_statements(e.statements.clone()).with_input(e.input);
                Some(run.with_recorded(names.iter().map(|(name, file)| (name.as_str(), *file))))
            }
            Err(err) => {
                eprintln!("ironwork: --evidence {}: {err}", e.dir.display());
                return exit::status(Outcome::Usage);
            }
        },
        None => None,
    };
    let outlines = crate::coverage::module_outlines(&module);
    let coverage = r.coverage.as_deref().map(|file| (file, outlines.as_slice(), r.reads.as_slice()));
    if r.command == "cics" {
        let page = main.options.options.code_page();
        let id = module.directory.first().map(|e| e.id.clone()).unwrap_or_default();
        let mut tasks = ModuleTasks { module: &module, path: Path::new(path), shown: path, library, database: r.database, current: None, sources: Vec::new() };
        return crate::cics_tasks(&mut tasks, page, &id, path, r.dds, r.clock, r.options, journal, coverage);
    }
    let sysin = match crate::open_sysin(&r.dds) {
        Ok(s) => s,
        Err(code) => return code,
    };
    batch(module, library, r.dds, sysin, r.clock, r.database, r.parm, path, journal, coverage)
}

/// Each name the debug tables of the module's programs give, with the file the module records for
/// it.
fn recorded_names(module: &LoadedModule) -> Vec<(String, &Option<exec::module::SourceFile>)> {
    let mut names = Vec::new();
    for (program, files) in module.programs.iter().zip(&module.files) {
        for (&source, file) in program.debug.sources.iter().zip(files) {
            names.push((program.symbols.get(source as usize).cloned().unwrap_or_default(), file));
        }
    }
    names
}

/// The module's program 0 as the first program of a run unit, CALL finding the module's other
/// programs first.
#[allow(clippy::too_many_arguments)]
fn batch(
    module: LoadedModule,
    library: exec::unit::Library,
    dds: exec::files::Dds,
    sysin: Box<dyn io::BufRead>,
    clock: exec::unit::Clock,
    mut database: Option<Box<dyn exec::sql::Database>>,
    parm: Option<&str>,
    path: &str,
    journal: Option<evidence::Run>,
    coverage: Option<(&Path, &[Outline], &[PathBuf])>,
) -> ExitCode {
    let sources: Vec<String> = module.programs[0].debug.sources.iter().map(|&s| module.programs[0].symbols.get(s as usize).cloned().unwrap_or_default()).collect();
    let shared = journal.map(|run| Rc::new(RefCell::new(run)));
    let covered = coverage.map(|(_, _, roots)| Rc::new(RefCell::new(Coverage::naming(path, roots))));
    let observer = observer(&shared, &covered);
    let (mut out, mut err) = (io::stdout().lock(), io::stderr());
    let ended = exec::vm::execute_module(module, Path::new(path), library, dds, Some(sysin), clock, database.as_deref_mut(), &mut out, &mut err, observer, parm);
    drop(out);
    let abend = match &ended {
        Err(Halt::Abend(a)) if !matches!(a.code, AbendCode::Signal(Signal::ClosedOutput)) => Some(a),
        _ => None,
    };
    let file = abend.and_then(|a| a.file.clone().or_else(|| sources.get(a.pos.file as usize).cloned())).filter(|f| !f.is_empty());
    let outcome = match &ended {
        Ok((_, return_code)) => Outcome::Ended(i64::from(*return_code)),
        Err(Halt::Abend(_)) if abend.is_none() => Outcome::Ended(0),
        Err(Halt::Abend(a)) => {
            eprintln!("{}:{}: ABEND {}: {}", file.as_deref().unwrap_or(path), a.pos, a.code, a.message);
            Outcome::of_abend(&a.code)
        }
        Err(Halt::Unimplemented(what)) => {
            eprintln!("ironwork: {path}: the VM does not run {what} yet; run the source without --vm");
            Outcome::Stopped
        }
    };
    write_coverage(coverage, &covered);
    if let Some(run) = shared.and_then(|r| Rc::try_unwrap(r).ok()) {
        let journal = run.into_inner().end(abend.map(|a| (a.code.to_string(), file.as_deref(), i64::from(a.pos.line))));
        evidence::finish(Some(journal), exit::recorded(outcome));
    }
    exit::status(outcome)
}

/// What the run unit tells the journal and the coverage report.
fn observer<'w>(run: &Option<Rc<RefCell<evidence::Run>>>, covered: &Option<Rc<RefCell<Coverage>>>) -> Option<exec::unit::Observer<'w>> {
    (run.is_some() || covered.is_some()).then(|| {
        let (run, cov) = (run.clone(), covered.clone());
        Box::new(move |event: exec::unit::Event<'_>| {
            if let Some(c) = &cov {
                c.borrow_mut().observe(&event);
            }
            if let Some(r) = &run {
                r.borrow_mut().observe(event);
            }
        }) as exec::unit::Observer<'w>
    })
}

fn write_coverage(coverage: Option<(&Path, &[Outline], &[PathBuf])>, covered: &Option<Rc<RefCell<Coverage>>>) {
    if let (Some((file, outlines, _)), Some(c)) = (coverage, covered) {
        let text = format!("{}\n", exec::evidence::canonical(&c.borrow().report(outlines)));
        if let Err(e) = std::fs::write(file, text) {
            eprintln!("ironwork: --coverage {}: {e}", file.display());
        }
    }
}

/// CICS tasks that begin with the module's program 0, then with the program each transaction names
/// as a CALL of it finds one on the VM: in the module first, then as `NAME.iwm` or source in the
/// libraries.
struct ModuleTasks<'a> {
    module: &'a LoadedModule,
    path: &'a Path,
    shown: &'a str,
    library: exec::unit::Library,
    database: Option<Box<dyn exec::sql::Database>>,
    current: Option<String>,
    /// The source names of the last task's first program, which an abend's position indexes.
    sources: Vec<String>,
}

impl crate::Tasks for ModuleTasks<'_> {
    fn begin_with(&mut self, name: &str) -> Result<Vec<(String, Option<exec::module::SourceFile>)>, String> {
        let recorded = exec::vm::module_program(self.module, self.path, self.library.clone(), name)?;
        self.current = Some(name.to_owned());
        Ok(recorded)
    }

    fn run<'w>(
        &'w mut self,
        dds: exec::files::Dds,
        task: exec::cics::Task,
        clock: exec::unit::Clock,
        out: &'w mut dyn io::Write,
        err: &'w mut dyn io::Write,
        observer: Option<exec::unit::Observer<'w>>,
    ) -> Result<Result<(exec::Ending, exec::cics::Task), exec::Abend>, (Outcome, String)> {
        let (ended, task, sources) = exec::vm::execute_module_cics(self.module, self.path, self.current.as_deref(), self.library.clone(), dds, task, clock, self.database.as_deref_mut(), out, err, observer);
        self.sources = sources;
        match ended {
            Ok(ending) => Ok(Ok((ending, task))),
            Err(Halt::Abend(abend)) => Ok(Err(abend)),
            Err(Halt::Unimplemented(what)) => Err((Outcome::Stopped, format!("ironwork: {}: the VM does not run {what} yet; run the source without --vm", self.shown))),
        }
    }

    fn abend_file(&self, abend: &exec::Abend) -> Option<String> {
        abend.file.clone().or_else(|| self.sources.get(abend.pos.file as usize).cloned())
    }
}
