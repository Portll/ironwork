//! ironwork for COBOL: an interpreter that runs a program `ironwork-compile` has checked and laid
//! out, in EBCDIC with the numeric model of `ironwork-numeric`.

pub mod abend;
pub use rt::calendar;
pub use rt::cics;
pub use rt::digest;
pub use rt::evidence;
pub use compile::collating;
pub use compile::declaratives;
#[cfg(test)]
mod edit;
pub use rt::files;
pub use compile::layout;
pub mod le;
pub use rt::lir;
pub mod loader;
pub mod lower;
pub mod machine;
pub use rt::module;
pub mod oo;
pub use compile::picture;
pub mod printer;
pub mod report;
use compile::sort;
pub mod sql;
pub mod terminal;
#[cfg(test)]
mod testing;
pub use rt::tn3270;
pub mod unit;
pub mod vm;

pub use compile::{Compiled, compile, compile_at, compile_time, entry_points, read_lengths, variable_records};
pub(crate) use compile::{procedure, procedure_from, section_end};
pub use machine::{Abend, Ending};

use abend::AbendCode;
use unit::AddProgram;
use std::io::{BufRead, Write};
use syntax::Pos;

/// Running a compiled program, in a run unit of its own or as a CICS task.
pub trait Execute {
    fn run(&self, out: &mut dyn Write, err: &mut dyn Write) -> Result<Ending, Abend>;

    /// Runs with the DDs that ASSIGN names map to.
    fn run_with(&self, dds: files::Dds, out: &mut dyn Write, err: &mut dyn Write) -> Result<Ending, Abend>;

    /// Runs as the first program of a run unit: CALL finds other programs in `library`, ACCEPT
    /// reads `sysin`. Returns how the run ended and RETURN-CODE.
    fn execute<'w>(
        &self,
        library: unit::Library,
        dds: files::Dds,
        sysin: Option<Box<dyn BufRead + 'w>>,
        clock: unit::Clock,
        out: &'w mut dyn Write,
        err: &'w mut dyn Write,
    ) -> Result<(Ending, i16), Abend>;

    /// Runs as [`Execute::execute`] does, with EXEC SQL answered by `database`.
    #[allow(clippy::too_many_arguments)]
    fn execute_with<'w>(
        &self,
        library: unit::Library,
        dds: files::Dds,
        sysin: Option<Box<dyn BufRead + 'w>>,
        clock: unit::Clock,
        database: Option<&'w mut (dyn sql::Database + '_)>,
        out: &'w mut dyn Write,
        err: &'w mut dyn Write,
    ) -> Result<(Ending, i16), Abend>;

    /// Runs as [`Execute::execute_with`] does, telling `observer` what the run opens, closes and
    /// loads.
    #[allow(clippy::too_many_arguments)]
    fn execute_observed<'w>(
        &self,
        library: unit::Library,
        dds: files::Dds,
        sysin: Option<Box<dyn BufRead + 'w>>,
        clock: unit::Clock,
        database: Option<&'w mut (dyn sql::Database + '_)>,
        out: &'w mut dyn Write,
        err: &'w mut dyn Write,
        observer: Option<unit::Observer<'w>>,
    ) -> Result<(Ending, i16), Abend>;

    /// Runs as [`Execute::execute_observed`] does, as the main program of a job step that EXEC
    /// PGM= started with `parm`: the first PROCEDURE DIVISION USING item addresses a halfword
    /// length and the program arguments Language Environment finds in it.
    #[allow(clippy::too_many_arguments)]
    fn execute_main<'w>(
        &self,
        library: unit::Library,
        dds: files::Dds,
        sysin: Option<Box<dyn BufRead + 'w>>,
        clock: unit::Clock,
        database: Option<&'w mut (dyn sql::Database + '_)>,
        out: &'w mut dyn Write,
        err: &'w mut dyn Write,
        observer: Option<unit::Observer<'w>>,
        parm: &str,
    ) -> Result<(Ending, i16), Abend>;

    /// Runs as [`Execute::execute_observed`] does, and puts what the run left in its run unit in
    /// `kept`.
    #[allow(clippy::too_many_arguments)]
    fn execute_kept<'w>(
        &self,
        library: unit::Library,
        dds: files::Dds,
        sysin: Option<Box<dyn BufRead + 'w>>,
        clock: unit::Clock,
        database: Option<&'w mut (dyn sql::Database + '_)>,
        out: &'w mut dyn Write,
        err: &'w mut dyn Write,
        observer: Option<unit::Observer<'w>>,
        kept: &mut Option<unit::Remains>,
    ) -> Result<(Ending, i16), Abend>;

    /// Runs as the first program of a CICS task. `task` says who started it, what COMMAREA it
    /// starts with, and what files and queues it has. Returns how the run ended and the task, with
    /// RETURN TRANSID and COMMAREA if it ended that way.
    fn execute_cics<'w>(
        &self,
        library: unit::Library,
        dds: files::Dds,
        task: cics::Task,
        clock: unit::Clock,
        out: &'w mut dyn Write,
        err: &'w mut dyn Write,
    ) -> Result<(Ending, cics::Task), Abend>;

    /// A CICS task with a database: SYNCPOINT commits, and the end of the task commits, or rolls
    /// back after an abend, and closes every cursor. The database outlives the task, so a region's
    /// tasks can share one.
    #[allow(clippy::too_many_arguments)]
    fn execute_cics_with<'w>(
        &self,
        library: unit::Library,
        dds: files::Dds,
        task: cics::Task,
        clock: unit::Clock,
        database: Option<&'w mut (dyn sql::Database + '_)>,
        out: &'w mut dyn Write,
        err: &'w mut dyn Write,
    ) -> Result<(Ending, cics::Task), Abend>;

    /// Runs as [`Execute::execute_cics_with`] does, telling `observer` what the task opens, loads
    /// and passes to an operation an input could steer.
    #[allow(clippy::too_many_arguments)]
    fn execute_cics_observed<'w>(
        &self,
        library: unit::Library,
        dds: files::Dds,
        task: cics::Task,
        clock: unit::Clock,
        database: Option<&'w mut (dyn sql::Database + '_)>,
        out: &'w mut dyn Write,
        err: &'w mut dyn Write,
        observer: Option<unit::Observer<'w>>,
    ) -> Result<(Ending, cics::Task), Abend>;
}

impl Execute for Compiled {
    fn run(&self, out: &mut dyn Write, err: &mut dyn Write) -> Result<Ending, Abend> {
        self.run_with(files::Dds::default(), out, err)
    }

    fn run_with(&self, dds: files::Dds, out: &mut dyn Write, err: &mut dyn Write) -> Result<Ending, Abend> {
        self.execute(unit::Library::default(), dds, None, unit::Clock::System, out, err).map(|(ending, _)| ending)
    }

    fn execute<'w>(
        &self,
        library: unit::Library,
        dds: files::Dds,
        sysin: Option<Box<dyn BufRead + 'w>>,
        clock: unit::Clock,
        out: &'w mut dyn Write,
        err: &'w mut dyn Write,
    ) -> Result<(Ending, i16), Abend> {
        self.execute_with(library, dds, sysin, clock, None, out, err)
    }

    fn execute_with<'w>(
        &self,
        library: unit::Library,
        dds: files::Dds,
        sysin: Option<Box<dyn BufRead + 'w>>,
        clock: unit::Clock,
        database: Option<&'w mut (dyn sql::Database + '_)>,
        out: &'w mut dyn Write,
        err: &'w mut dyn Write,
    ) -> Result<(Ending, i16), Abend> {
        self.execute_observed(library, dds, sysin, clock, database, out, err, None)
    }

    fn execute_observed<'w>(
        &self,
        library: unit::Library,
        dds: files::Dds,
        sysin: Option<Box<dyn BufRead + 'w>>,
        clock: unit::Clock,
        database: Option<&'w mut (dyn sql::Database + '_)>,
        out: &'w mut dyn Write,
        err: &'w mut dyn Write,
        observer: Option<unit::Observer<'w>>,
    ) -> Result<(Ending, i16), Abend> {
        run_main(self, library, dds, sysin, clock, database, out, err, observer, None, &mut None)
    }

    fn execute_main<'w>(
        &self,
        library: unit::Library,
        dds: files::Dds,
        sysin: Option<Box<dyn BufRead + 'w>>,
        clock: unit::Clock,
        database: Option<&'w mut (dyn sql::Database + '_)>,
        out: &'w mut dyn Write,
        err: &'w mut dyn Write,
        observer: Option<unit::Observer<'w>>,
        parm: &str,
    ) -> Result<(Ending, i16), Abend> {
        run_main(self, library, dds, sysin, clock, database, out, err, observer, Some(parm), &mut None)
    }

    fn execute_kept<'w>(
        &self,
        library: unit::Library,
        dds: files::Dds,
        sysin: Option<Box<dyn BufRead + 'w>>,
        clock: unit::Clock,
        database: Option<&'w mut (dyn sql::Database + '_)>,
        out: &'w mut dyn Write,
        err: &'w mut dyn Write,
        observer: Option<unit::Observer<'w>>,
        kept: &mut Option<unit::Remains>,
    ) -> Result<(Ending, i16), Abend> {
        run_main(self, library, dds, sysin, clock, database, out, err, observer, None, kept)
    }

    fn execute_cics<'w>(
        &self,
        library: unit::Library,
        dds: files::Dds,
        task: cics::Task,
        clock: unit::Clock,
        out: &'w mut dyn Write,
        err: &'w mut dyn Write,
    ) -> Result<(Ending, cics::Task), Abend> {
        self.execute_cics_with(library, dds, task, clock, None, out, err)
    }

    fn execute_cics_with<'w>(
        &self,
        library: unit::Library,
        dds: files::Dds,
        task: cics::Task,
        clock: unit::Clock,
        database: Option<&'w mut (dyn sql::Database + '_)>,
        out: &'w mut dyn Write,
        err: &'w mut dyn Write,
    ) -> Result<(Ending, cics::Task), Abend> {
        self.execute_cics_observed(library, dds, task, clock, database, out, err, None)
    }

    fn execute_cics_observed<'w>(
        &self,
        library: unit::Library,
        dds: files::Dds,
        task: cics::Task,
        clock: unit::Clock,
        database: Option<&'w mut (dyn sql::Database + '_)>,
        out: &'w mut dyn Write,
        err: &'w mut dyn Write,
        observer: Option<unit::Observer<'w>>,
    ) -> Result<(Ending, cics::Task), Abend> {
        oo::refuse_to_run(&self.program)?;
        let (ending, task) = execute_task(self, library, dds, task, clock, database, out, err, observer, &mut None);
        ending.map(|e| (e, task))
    }
}

/// Runs `compiled` on the interpreter as the first program of a CICS task, as
/// [`Execute::execute_cics_observed`] does; `kept` takes what the run left in its run unit, and the
/// task comes back however the run ended.
#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_task<'w>(
    compiled: &Compiled,
    library: unit::Library,
    dds: files::Dds,
    task: cics::Task,
    clock: unit::Clock,
    database: Option<&'w mut (dyn sql::Database + '_)>,
    out: &'w mut dyn Write,
    err: &'w mut dyn Write,
    observer: Option<unit::Observer<'w>>,
    kept: &mut Option<unit::Remains>,
) -> (Result<Ending, Abend>, cics::Task) {
    let (statements, taint) = (library.trace_statements.clone(), library.trace_input.then(rt::taint::Taint::default));
    let limit = library.statement_limit;
    let mut run_unit = unit::RunUnit::new(library, dds, None, clock, out, err);
    run_unit.observer = observer;
    run_unit.statements = statements;
    run_unit.taint = taint;
    run_unit.statement_limit = limit;
    run_unit.sql = database.map(sql::Session::new);
    let (ending, ended, task) = run_task(compiled, run_unit, task, kept, |unit, me, commarea, length| {
        machine::Machine::activation(compiled, me, unit, true).and_then(|mut m| {
            m.begin_task(commarea, length);
            m.run_level()
        })
    });
    (ending.map_err(asra).and_then(|e| ended.map(|()| e)), task)
}

/// A CICS task's run unit given the task, its EXEC interface block and its COMMAREA, and the task's
/// first program added as `me` and run by `run` with the COMMAREA's address and length; then the
/// task's unit of work ended, its files closed and its transient data written. Returns how the run
/// ended, how ending the task went, and the task; `kept` takes what the run left in its run unit.
pub(crate) fn run_task<'w, H: Clone, L: unit::Loader<H>, E>(
    compiled: &Compiled,
    mut run_unit: rt::unit::RunUnit<'w, H, L>,
    mut task: cics::Task,
    kept: &mut Option<unit::Remains>,
    run: impl FnOnce(&mut rt::unit::RunUnit<'w, H, L>, usize, Option<usize>, usize) -> Result<Ending, E>,
) -> (Result<Ending, E>, Result<(), Abend>, cics::Task) {
    let me = run_unit.add_named(None, compiled.program.id.to_ascii_uppercase(), compiled.program.files.len(), compiled.layout.size as usize);
    run_unit.eib = run_unit.push_temporary(&[0; cics::EIB_LEN]);
    let commarea = task.commarea.take();
    let length = commarea.as_ref().map_or(0, Vec::len);
    let commarea = commarea.map(|c| {
        let at = run_unit.push_temporary(&c);
        run_unit.mark_input(at, c.len(), true);
        at
    });
    run_unit.cics = Some(task);
    let ending = run(&mut run_unit, me, commarea, length);
    let settled = run_unit.sql.as_mut().map_or(Ok(()), |s| s.end_task(&compiled.program.id, ending.is_ok()).map(drop));
    let mut closed = run_unit.close_all(false);
    for (name, f) in run_unit.cics_files.drain() {
        if let Err(e) = f.close() {
            closed = closed.and(Err(format!("closing CICS file {name}: {e}")));
        }
    }
    *kept = Some(unit::Remains::of(&run_unit));
    let mut task = run_unit.cics.take().unwrap_or_default();
    if let Err(e) = task.flush_td(compiled.options.code_page()) {
        closed = closed.and(Err(format!("writing transient data: {e}")));
    }
    let settled = settled.map_err(|a| Abend { code: a.code.into(), message: a.message, pos: Pos::default(), file: None });
    let closed = closed.map_err(|m| Abend { code: AbendCode::Ironwork, message: m, pos: Pos::default(), file: None });
    (ending, settled.and(closed), task)
}

/// A program check in a CICS task, which CICS reports as ASRA.
pub(crate) fn asra(a: Abend) -> Abend {
    match a.code {
        AbendCode::Check(_) | AbendCode::Protection => Abend { message: format!("{} ({}, which CICS reports as ASRA)", a.message, a.code), code: AbendCode::Cics("ASRA".into()), pos: a.pos, file: a.file },
        _ => a,
    }
}

/// Runs `compiled` as the first program of a batch run unit; with `parm`, as a job step's main
/// program.
#[allow(clippy::too_many_arguments)]
fn run_main<'w>(
    compiled: &Compiled,
    library: unit::Library,
    dds: files::Dds,
    sysin: Option<Box<dyn BufRead + 'w>>,
    clock: unit::Clock,
    database: Option<&'w mut (dyn sql::Database + '_)>,
    out: &'w mut dyn Write,
    err: &'w mut dyn Write,
    observer: Option<unit::Observer<'w>>,
    parm: Option<&str>,
    kept: &mut Option<unit::Remains>,
) -> Result<(Ending, i16), Abend> {
    oo::refuse_to_run(&compiled.program)?;
    let (statements, taint) = (library.trace_statements.clone(), library.trace_input.then(rt::taint::Taint::default));
    let limit = library.statement_limit;
    let mut run_unit = unit::RunUnit::new(library, dds, sysin, clock, out, err);
    run_unit.observer = observer;
    run_unit.statements = statements;
    run_unit.taint = taint;
    run_unit.statement_limit = limit;
    run_unit.sql = database.map(sql::Session::new);
    let me = run_unit.add(None, &compiled.program, compiled.layout.size as usize);
    let trap_off = parm.is_some_and(rt::le::parm::trap_off);
    let parm = parm.map(|p| push_parm(&mut run_unit, compiled.options.code_page(), p));
    let ending = machine::Machine::activation(compiled, me, &mut run_unit, true).and_then(|mut m| {
        if parm.is_some() {
            m.bind(&[parm]);
        }
        m.run_procedure()
    });
    let settled = run_unit.sql.as_mut().map_or(Ok(()), |s| s.settle(&compiled.program.id, ending.is_ok()).map(drop));
    let closed = run_unit.close_all(trap_off && ending.as_ref().is_err_and(|a| a.code.bypasses_trap_off()));
    *kept = Some(unit::Remains::of(&run_unit));
    let ending = ending?;
    settled.map_err(|a| Abend { code: a.code.into(), message: a.message, pos: Pos::default(), file: None })?;
    closed.map_err(|m| Abend { code: AbendCode::Ironwork, message: m, pos: Pos::default(), file: None })?;
    Ok((ending, run_unit.return_code()))
}

/// A job step's PARM as Language Environment passes it, at the end of memory: input.
pub(crate) fn push_parm<H: Clone, L: rt::unit::Loader<H>>(run_unit: &mut rt::unit::RunUnit<'_, H, L>, page: &zarch::ebcdic::CodePage, parm: &str) -> usize {
    let area = rt::le::parm::parameter_area(rt::le::parm::program_arguments(parm), page);
    let at = run_unit.push_temporary(&area);
    run_unit.mark_input(at, area.len(), true);
    at
}

#[cfg(test)]
mod tests;
