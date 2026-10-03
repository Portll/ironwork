//! Running a compiled program on the VM (`rt::vm`): CALL's loader over the interpreter's program
//! library, lowering each program it compiles, and the run itself, as `Execute::execute_observed`
//! makes the interpreter's.

use crate::loader::Library;
use crate::lower::{self, LowerError};
use crate::unit::{Clock, Observer, Remains};
use crate::{Compiled, cics, files, oo, sql};
use rt::abend::{Abend, AbendCode, Ending};
use rt::oo::ClassCode;
use rt::unit::{FoundClass, LoadError, LoadedProgram, Loader};
pub use rt::vm::{Code, Halt};
use std::io::{BufRead, Write};
use std::rc::Rc;
use syntax::Pos;

/// The program library, each program lowered as CALL loads it.
pub struct VmLibrary(pub Library);

impl Loader<Rc<Code>> for VmLibrary {
    fn program(&mut self, name: &str) -> Result<LoadedProgram<Rc<Code>>, LoadError> {
        let found = <Library as Loader<Rc<Compiled>>>::program(&mut self.0, name)?;
        Ok(LoadedProgram { compiled: Rc::new(code(&found.compiled)), name: found.name, files: found.files, size: found.size, source: found.source })
    }

    fn holder(&self, entry: &str) -> Option<String> {
        <Library as Loader<Rc<Compiled>>>::holder(&self.0, entry)
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

    /// INVOKE is not run by the VM yet, so no class is loaded.
    fn class(&mut self, _external: &str) -> Result<Option<FoundClass<Rc<ClassCode<Rc<Code>>>>>, String> {
        Ok(None)
    }

    fn mapset(&mut self, name: &str) -> Option<Result<rt::bms::Mapset, String>> {
        self.0.mapset(name)
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
    Code::new(lowered, entries, compiled.program.files.len(), compiled.layout.size as usize, compiled.program.nested.clone())
}

/// Runs `compiled`, lowered as `code`, as the first program of a run unit on the VM, with a job
/// step's `parm` as `Execute::execute_main` takes it; `kept` takes what the run left in its run
/// unit.
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
    parm: Option<&str>,
    kept: &mut Option<Remains>,
) -> Result<(Ending, i16), Halt> {
    oo::refuse_to_run(&compiled.program)?;
    let (statements, taint) = (library.trace_statements.clone(), library.trace_input.then(rt::taint::Taint::default));
    let mut run_unit = rt::unit::RunUnit::new(VmLibrary(library), dds, sysin, clock, out, err);
    run_unit.observer = observer;
    run_unit.statements = statements;
    run_unit.taint = taint;
    run_unit.sql = database.map(sql::Session::new);
    let me = run_unit.add_named(None, compiled.program.id.to_ascii_uppercase(), compiled.program.files.len(), compiled.layout.size as usize);
    let parm = parm.map(|p| crate::push_parm(&mut run_unit, compiled, p));
    let ending = rt::vm::run(code, me, &mut run_unit, &parm.map_or_else(Vec::new, |p| vec![Some(p)]));
    let settled = run_unit.sql.as_mut().map_or(Ok(()), |s| s.settle(&compiled.program.id, ending.is_ok()).map(drop));
    let closed = run_unit.close_all();
    *kept = Some(Remains::of(&run_unit));
    let ending = ending?;
    settled.map_err(|a| Abend { code: a.code.into(), message: a.message, pos: Pos::default(), file: None })?;
    closed.map_err(|m| Abend { code: AbendCode::Ironwork, message: m, pos: Pos::default(), file: None })?;
    Ok((ending, run_unit.return_code()))
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
    let (statements, taint) = (library.trace_statements.clone(), library.trace_input.then(rt::taint::Taint::default));
    let mut run_unit = rt::unit::RunUnit::new(VmLibrary(library), dds, None, clock, out, err);
    run_unit.observer = observer;
    run_unit.statements = statements;
    run_unit.taint = taint;
    run_unit.sql = database.map(sql::Session::new);
    let (ending, ended, task) = crate::run_task(compiled, run_unit, task, kept, |unit, me, commarea, length| rt::vm::run_task(code, me, unit, commarea, length));
    let ending = match ending {
        Err(Halt::Abend(abend)) => Err(Halt::Abend(crate::asra(abend))),
        other => other,
    };
    (ending.and_then(|e| ended.map(|()| e).map_err(Halt::Abend)), task)
}
