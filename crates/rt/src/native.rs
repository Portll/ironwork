//! A compiled program's own executable (codegen-runtime.md §14 step 6): what `ironwork run
//! module.iwm` does for a batch run, from the runtime alone. The program's load module is held in
//! the executable; CALL finds a program in it, then as `NAME.iwm` in the `-L` directories. With no
//! compiler at hand, a program a library holds only as source is not found.

use crate::abend::{AbendCode, Ending, Signal};
use crate::batch::{Passed, past_the_end};
use crate::exit::{self, Convention, Outcome};
use crate::files::Dds;
use crate::lir::verify::verify;
use crate::module::{LoadedModule, Modules, read};
use crate::oo::ClassCode;
use crate::refusal::IWR0073;
use crate::unit::{Clock, FoundClass, LoadError, LoadedProgram, Loader, RunUnit};
use crate::vm::{Code, Halt, Native};
use crate::vocab::Pos;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::rc::Rc;

type Found = LoadedProgram<Rc<Code>>;
type FoundCode = FoundClass<Rc<ClassCode<Rc<Code>>>>;

/// Where CALL finds a program in a compiled program's run: the module it began with first, then
/// `NAME.iwm` in the directories, as `exec::vm::VmLibrary` finds modules for a run that began with one.
pub struct ModuleLoader {
    modules: Modules,
}

impl ModuleLoader {
    /// The module a run begins with, each of its programs run by its generated code in `natives`.
    pub fn new(dirs: Vec<PathBuf>, path: &Path, module: LoadedModule, natives: Vec<Option<Native>>) -> Self {
        let mut modules = Modules::new(dirs, verify);
        modules.add_first_native(path.to_owned(), module, natives);
        Self { modules }
    }
}

impl Loader<Rc<Code>> for ModuleLoader {
    fn program(&mut self, name: &str) -> Result<Found, LoadError> {
        self.modules.loaded(name).unwrap_or_else(|| self.modules.search(name))
    }

    fn holder(&self, entry: &str) -> Option<String> {
        self.modules.holder(entry)
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

    fn facts(program: &Rc<Code>) -> numeric::governs::Facts {
        program.facts()
    }

    fn source(program: &Rc<Code>, file: usize) -> Option<String> {
        let p = program.program()?;
        p.debug.sources.get(file).map(|&s| p.symbols[s as usize].clone())
    }

    fn class(&mut self, external: &str) -> Result<Option<FoundCode>, String> {
        if let Some(found) = self.modules.loaded_class(external)? {
            return Ok(Some(found));
        }
        self.modules.search_class(external)
    }

    fn mapset(&mut self, name: &str) -> Option<Result<crate::bms::Mapset, String>> {
        self.modules.mapset(name).map(Ok)
    }
}

/// What a batch run of a compiled program is given besides its module.
pub struct Request<'a> {
    /// The module's name, which a message names it by where an abend has no source file.
    pub path: &'a str,
    pub dirs: Vec<PathBuf>,
    pub dds: Dds,
    pub clock: Clock,
    pub parm: Option<&'a str>,
    /// The generated code of the module's programs, by ordinal.
    pub natives: Vec<Option<Native>>,
}

fn refused(message: String) -> Halt {
    Halt::Abend(crate::abend::Abend::ironwork(message, Pos::default()))
}

/// Runs program 0 of `module` as the first program of a run unit on the VM, then settles the run as
/// `exec::vm::execute_module` does: control past the end of the main program, and every file closed
/// unless an abend the PARM's TRAP(OFF) keeps from Language Environment ended it.
pub fn run<'w>(module: LoadedModule, r: Request<'_>, sysin: Box<dyn BufRead + 'w>, out: &'w mut dyn Write, err: &'w mut dyn Write) -> Result<(Ending, i16), Halt> {
    let path = Path::new(r.path);
    let mut loader = ModuleLoader::new(r.dirs, path, module, r.natives);
    let main = loader.modules.take(0, 0).unwrap_or_else(|| Err(format!("{}: the module holds no program", path.display()))).map_err(refused)?;
    if let Some(p) = main.compiled.program()
        && p.services.class.is_some()
    {
        return Err(refused(format!("{} is a class definition: run a program that uses it", p.symbols.get(p.id as usize).cloned().unwrap_or_default())));
    }
    let code = main.compiled;
    let Some(program) = code.program() else { return Err(refused(format!("{}: program 0 is not lowered", path.display()))) };
    let id = program.symbols.get(program.id as usize).cloned().unwrap_or_default();
    let page = program.options.options.code_page();
    let last_paragraph = program.paragraphs.last().and_then(|para| program.debug.positions.get(para.at as usize).copied());
    let mut run_unit = RunUnit::new(loader, r.dds, Some(sysin), r.clock, out, err);
    let me = run_unit.add_named(None, main.name, main.files, main.size);
    let passed = r.parm.map_or(Passed::Nothing, Passed::Parm);
    let trap_off = matches!(passed, Passed::Parm(p) if crate::le::parm::trap_off(p));
    passed.apply_parm(&mut run_unit);
    let addresses = passed.addresses(&mut run_unit, page);
    let ending = crate::vm::run(&code, me, &mut run_unit, &addresses, passed.main()).and_then(|e| past_the_end(e, passed.main(), &id, last_paragraph).map_err(Halt::Abend));
    let settled = run_unit.sql.as_mut().map_or(Ok(()), |s| s.settle(&id, ending.is_ok()).map(drop));
    let closed = run_unit.close_all(matches!(&ending, Err(Halt::Abend(a)) if a.code.skips_termination(trap_off)));
    let ending = ending?;
    settled.map_err(|a| crate::abend::Abend { code: a.code.into(), message: a.message, pos: Pos::default(), file: None })?;
    closed.map_err(|m| crate::abend::Abend { code: AbendCode::Ironwork, message: m, pos: Pos::default(), file: None })?;
    Ok((ending, run_unit.return_code()))
}

const USAGE: &str = "usage: <program> [-L <dir>]... [--dd NAME=path[:format][:mod]]... [--clock <time>] [--parm TEXT] [--exit-code]";

fn usage(message: &str) -> ExitCode {
    eprintln!("{message}\n{USAGE}");
    exit::status(Outcome::Usage)
}

/// The executable's whole run, as `ironwork run module.iwm` gives it: on a thread with the driver's
/// stack, a panic exiting as ironwork's internal error.
pub fn main(path: &'static str, module: &'static [u8], natives: &'static [Option<Native>]) -> ExitCode {
    match std::thread::Builder::new().stack_size(64 << 20).spawn(move || batch(path, module, natives)).map(|t| t.join()) {
        Ok(Ok(code)) => code,
        _ => exit::status(Outcome::Internal),
    }
}

/// Its arguments read, the module read and its first program checked, the run, an abend or a
/// construct the VM does not run said on standard error, and the exit status.
fn batch(path: &str, module: &[u8], natives: &[Option<Native>]) -> ExitCode {
    exit::follow(Convention::Band);
    let (mut dirs, mut dds, mut clock, mut parm) = (Vec::new(), Vec::new(), Clock::System, None);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--exit-code" {
            exit::follow(Convention::Verdict);
            continue;
        }
        let Some(value) = args.next() else { return usage(&format!("{arg} needs a value")) };
        match arg.as_str() {
            "-L" => dirs.push(PathBuf::from(value)),
            "--dd" => dds.push(value),
            "--clock" => match Clock::parse(&value) {
                Some(c) => clock = c,
                None => return usage("--clock needs YYYY-MM-DDTHH:MM:SS[.hh]"),
            },
            "--parm" => parm = Some(value),
            _ => return usage(&format!("unexpected argument {arg}")),
        }
    }
    let dds = match Dds::new(&dds, true) {
        Ok(d) => d,
        Err(e) => return usage(&e),
    };
    let module = match read(module) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("ironwork: {path}: {e}");
            return exit::status(Outcome::Unreadable);
        }
    };
    let Some(first) = module.programs.first() else {
        eprintln!("ironwork: {path}: the module holds no program");
        return exit::status(Outcome::Unreadable);
    };
    let symbol = |id: u32| first.symbols.get(id as usize).cloned().unwrap_or_default();
    if let Err(e) = verify(first) {
        eprintln!("ironwork: {path}: program {}: {e}", symbol(first.id));
        return exit::status(Outcome::Unreadable);
    }
    if first.services.function.is_some() {
        eprintln!("ironwork: {path}: FUNCTION-ID {}: the module holds user-defined functions and no program to run", symbol(first.id));
        return exit::status(Outcome::Refused);
    }
    let sources: Vec<String> = first.debug.sources.iter().map(|&s| first.symbols.get(s as usize).cloned().unwrap_or_default()).collect();
    let sysin: Box<dyn BufRead> = match dds.get("SYSIN") {
        Some(dd) => match std::fs::File::open(&dd.path) {
            Ok(f) => Box::new(io::BufReader::new(f)),
            Err(e) => {
                eprintln!("ironwork: DD SYSIN {}: {e}", dd.path.display());
                return exit::status(Outcome::Usage);
            }
        },
        None => Box::new(io::stdin().lock()),
    };
    let (mut out, mut err) = (io::stdout().lock(), io::stderr());
    let ended = run(module, Request { path, dirs, dds, clock, parm: parm.as_deref(), natives: natives.to_vec() }, sysin, &mut out, &mut err);
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
            eprintln!("ironwork: {path}: {}", IWR0073.message(format_args!("the VM does not run {what} yet; run the source with --interpret")));
            Outcome::Stopped
        }
    };
    exit::status(outcome)
}
