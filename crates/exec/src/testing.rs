use crate::lower::{self, LowerError};
use crate::unit::{Event, Remains};
use crate::vm::{self, Code, Halt};
use crate::{Abend, Compiled, Ending, cics, compile, compile_at, files, sql, unit};
use rt::lir::{CompileTime, Program};
use rt::module::StringTable;
use rt::module::codec::{Encode, Writer, decode_all};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::{Cursor, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::rc::Rc;
use zarch::ebcdic::CodePage;

pub enum Executor {
    Interpreter,
    Vm,
}

pub struct Outcome {
    pub out: String,
    pub err: String,
    pub ending: Result<Ending, Abend>,
    pub return_code: i16,
    pub task: Option<cics::Task>,
}

/// Makes the database of one run, afresh for each executor.
type Databases = Rc<dyn Fn() -> Box<dyn sql::Database>>;

/// Parses, compiles and runs one source with what a test sets; a CICS task makes it a CICS run.
pub struct Harness {
    source: String,
    classes: Vec<String>,
    flags: Vec<String>,
    dds: Vec<String>,
    dirs: Vec<std::path::PathBuf>,
    sysin: Option<String>,
    clock: unit::Clock,
    task: Option<cics::Task>,
    commarea: Option<String>,
    when_compiled: Option<CompileTime>,
    database: Option<Databases>,
    parm: Option<String>,
    arguments: Option<Vec<Option<Vec<u8>>>>,
    statement_limit: Option<u64>,
}

impl Harness {
    pub fn source(text: &str) -> Self {
        Harness {
            source: text.to_owned(),
            classes: Vec::new(),
            flags: Vec::new(),
            dds: Vec::new(),
            dirs: Vec::new(),
            sysin: None,
            clock: unit::Clock::System,
            task: None,
            commarea: None,
            when_compiled: None,
            database: None,
            parm: None,
            arguments: None,
            statement_limit: None,
        }
    }

    /// The statements a run may start before it ends with S322 (`RunUnit::statement_limit`).
    pub fn statement_limit(mut self, limit: u64) -> Self {
        self.statement_limit = Some(limit);
        self
    }

    pub fn classes(mut self, classes: &[String]) -> Self {
        self.classes = classes.to_vec();
        self
    }

    pub fn flags(mut self, flags: &[&str]) -> Self {
        self.flags = flags.iter().map(|f| f.to_string()).collect();
        self
    }

    pub fn dds(mut self, dds: &[String]) -> Self {
        self.dds = dds.to_vec();
        self
    }

    pub fn dirs(mut self, dirs: Vec<std::path::PathBuf>) -> Self {
        self.dirs = dirs;
        self
    }

    pub fn sysin(mut self, text: &str) -> Self {
        self.sysin = Some(text.to_owned());
        self
    }

    /// Runs the program as a job step's main program with this PARM.
    pub fn parm(mut self, text: &str) -> Self {
        self.parm = Some(text.to_owned());
        self
    }

    /// Runs the program as a subprogram a caller passed these arguments to, one per USING item,
    /// None for OMITTED.
    pub fn arguments(mut self, arguments: Vec<Option<Vec<u8>>>) -> Self {
        self.arguments = Some(arguments);
        self
    }

    pub fn clock(mut self, clock: unit::Clock) -> Self {
        self.clock = clock;
        self
    }

    pub fn task(mut self, task: cics::Task) -> Self {
        self.task = Some(task);
        self
    }

    /// The main program's compile time, which WHEN-COMPILED gives, in place of the clock's.
    pub fn compiled_at(mut self, at: CompileTime) -> Self {
        self.when_compiled = Some(at);
        self
    }

    /// The task's COMMAREA, encoded in the program's code page.
    pub fn commarea(mut self, text: &str) -> Self {
        self.commarea = Some(text.to_owned());
        self
    }

    /// EXEC SQL answered by the database `make` gives each run; the differential test compares
    /// the calls each executor's run makes and the answers it takes.
    pub fn database(mut self, make: impl Fn() -> Box<dyn sql::Database> + 'static) -> Self {
        self.database = Some(Rc::new(make));
        self
    }

    /// Runs the program, as a CICS task when a test gives one. Under the interpreter, a program
    /// that lowers runs on the VM too with the same inputs, the system clock read once for both,
    /// and the two must agree in everything [`Run`] holds (docs/lir.md §12.3); what the VM does
    /// not run yet is counted, not failed.
    pub fn run(self, executor: Executor) -> Outcome {
        let copy = syntax::copy::Libraries::default().with_compliance(numeric::Compliance::of(&self.flags));
        let mut programs = syntax::parse_all_with(&self.source, &copy).unwrap_or_else(|e| panic!("{e}"));
        let main = programs.remove(0);
        let compiled = match self.when_compiled {
            Some(at) => compile_at(main, &self.flags, at),
            None => compile(main, &self.flags),
        };
        let compiled = compiled.unwrap_or_else(|e| panic!("{e:?}"));
        programs.extend(self.classes.iter().map(|c| syntax::parse(c).unwrap_or_else(|e| panic!("{e}\n{c}"))));
        let fingerprint = rt::sql::fingerprint(&format!("{}\n{}", self.source, self.flags.join(" ")));
        let library = unit::Library { programs, dirs: self.dirs, copy, flags: self.flags, trace_statements: Some(unit::StatementFilter::All), trace_input: true, statement_limit: self.statement_limit, program_ids: None };
        let lowered = check_lowering(&compiled, fingerprint, None);
        for program in &library.programs {
            // Compiled as `RunUnit::load` compiles a CALLed program; one that does not compile is left out.
            if let Ok(Ok(c)) = catch_unwind(AssertUnwindSafe(|| compile(program.clone(), &library.flags))) {
                check_lowering(&c, fingerprint, None);
            }
        }
        let clock = match self.clock {
            unit::Clock::System => {
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
                unit::Clock::Fixed(now.as_secs() as i64, now.subsec_millis() / 10)
            }
            fixed => fixed,
        };
        let task = self.task.map(|task| cics::Task { commarea: self.commarea.map(|c| compiled.options.code_page().encode(&c).unwrap()), ..task });
        let paths = paths(&self.dds, task.as_ref());
        let inputs = Inputs { compiled: &compiled, library, dds: self.dds, sysin: self.sysin, clock, database: self.database, paths, parm: self.parm, arguments: self.arguments };
        let run = match executor {
            Executor::Vm => {
                let run = inputs.vm(&vm::code(&compiled), task);
                match run.ending {
                    Err(Halt::Unimplemented(what)) => panic!("the VM does not run {what} yet"),
                    _ => run,
                }
            }
            Executor::Interpreter => {
                let twin = task.as_ref().and_then(twin);
                let terminal = task.as_ref().is_some_and(|t| t.terminal.is_some());
                let before = files(&inputs.paths);
                let walker = inputs.walker(task);
                match lowered {
                    Some(_) if terminal => report_vm(&compiled, fingerprint, "unimplemented\ta CICS task with a terminal, which the harness cannot give both executors"),
                    Some(_) => {
                        let after = files(&inputs.paths);
                        restore(&before);
                        let code = vm::code(&compiled);
                        let vm = catch_unwind(AssertUnwindSafe(|| inputs.vm(&code, twin)));
                        restore(&after);
                        differential(&compiled, fingerprint, &walker, vm);
                    }
                    None => {}
                }
                walker
            }
        };
        let ending = run.ending.map_err(|h| match h {
            Halt::Abend(a) => a,
            Halt::Unimplemented(what) => unreachable!("the interpreter stopped for {what}"),
        });
        let task = run.task.filter(|_| ending.is_ok());
        Outcome { out: run.out, err: run.err, ending, return_code: run.return_code, task }
    }
}

/// What a run takes, given alike to the interpreter and the VM, and the files the differential
/// test compares.
struct Inputs<'c> {
    compiled: &'c Compiled,
    library: unit::Library,
    dds: Vec<String>,
    sysin: Option<String>,
    clock: unit::Clock,
    database: Option<Databases>,
    paths: Vec<PathBuf>,
    parm: Option<String>,
    arguments: Option<Vec<Option<Vec<u8>>>>,
}

/// What the differential test compares of a run.
struct Run {
    out: String,
    err: String,
    ending: Result<Ending, Halt>,
    return_code: i16,
    events: Events,
    remains: Option<Remains>,
    files: Vec<(PathBuf, Option<Vec<u8>>)>,
    task: Option<cics::Task>,
    /// The recording of the run's EXEC SQL calls and the database's answers.
    sql: String,
}

/// The events a run told its observer: how many, a digest of them all, and the first few.
#[derive(Default)]
struct Events {
    count: usize,
    digest: u64,
    first: Vec<String>,
}

const EVENTS_KEPT: usize = 4000;

impl Events {
    fn record(&mut self, event: &Event<'_>) {
        let text = match event {
            Event::Open { dd, mode, path } => format!("open {dd} {mode:?} {}", path.display()),
            Event::Close { dd, path } => format!("close {dd} {}", path.display()),
            Event::Load { program, source, .. } => format!("load {program} {}", source.map(|s| s.display().to_string()).unwrap_or_default()),
            Event::Paragraph { program, name, index } => format!("paragraph {program} {name} {index}"),
            Event::Sink { kind, file, line, operand, input } => format!("sink {kind} {file}:{line} {operand} {input:?}"),
            Event::Statement { file, line } => format!("statement {file}:{line}"),
        };
        for b in text.bytes().chain([0]) {
            self.digest = (self.digest ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
        self.count += 1;
        if self.first.len() < EVENTS_KEPT {
            self.first.push(text);
        }
    }
}

impl Inputs<'_> {
    fn passed(&self) -> crate::Passed<'_> {
        match (&self.parm, &self.arguments) {
            (_, Some(arguments)) => crate::Passed::Arguments(arguments),
            (Some(parm), None) => crate::Passed::Parm(parm),
            (None, None) => crate::Passed::Nothing,
        }
    }

    fn sysin(&self) -> Option<Box<dyn std::io::BufRead>> {
        self.sysin.clone().map(|s| Box::new(Cursor::new(s.into_bytes())) as Box<dyn std::io::BufRead>)
    }

    /// The database of one run, recording what it is asked and answers.
    fn database(&self) -> Option<(rt::sql::Recorder<'static>, Recording)> {
        let make = self.database.as_ref()?;
        let recording = Recording::default();
        let recorder = rt::sql::Recorder::new(make(), Box::new(recording.clone()), "the test harness").unwrap();
        Some((recorder, recording))
    }

    fn walker(&self, task: Option<cics::Task>) -> Run {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let (events, observer) = observed();
        let mut remains = None;
        let dds = files::Dds::new(&self.dds, false).unwrap();
        let (mut database, recording) = self.database().unzip();
        let db = database.as_mut().map(|d| d as &mut dyn sql::Database);
        let (ending, return_code, task) = match task {
            Some(task) => {
                let (ending, task) = crate::execute_task(self.compiled, self.library.clone(), dds, task, self.clock, db, &mut out, &mut err, Some(observer), &mut remains);
                (ending.map_err(Halt::Abend), 0, Some(task))
            }
            None => match crate::run_main(self.compiled, self.library.clone(), dds, self.sysin(), self.clock, db, &mut out, &mut err, Some(observer), self.passed(), &mut remains) {
                Ok((ending, code)) => (Ok(ending), code, None),
                Err(abend) => (Err(Halt::Abend(abend)), 0, None),
            },
        };
        drop(database);
        let (out, err) = (String::from_utf8(out).unwrap(), String::from_utf8(err).unwrap());
        Run { out, err, ending, return_code, events: events.take(), remains, files: files(&self.paths), task, sql: recording.map(|r| r.text()).unwrap_or_default() }
    }

    fn vm(&self, code: &Code, task: Option<cics::Task>) -> Run {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let (events, observer) = observed();
        let mut remains = None;
        let dds = files::Dds::new(&self.dds, false).unwrap();
        let (mut database, recording) = self.database().unzip();
        let db = database.as_mut().map(|d| d as &mut dyn sql::Database);
        let (ending, return_code, task) = match task {
            Some(task) => {
                let (ending, task) = vm::execute_cics(self.compiled, code, self.library.clone(), dds, task, self.clock, db, &mut out, &mut err, Some(observer), &mut remains);
                (ending, 0, Some(task))
            }
            None => match vm::execute(self.compiled, code, self.library.clone(), dds, self.sysin(), self.clock, db, &mut out, &mut err, Some(observer), self.passed(), &mut remains) {
                Ok((ending, code)) => (Ok(ending), code, None),
                Err(halt) => (Err(halt), 0, None),
            },
        };
        drop(database);
        let (out, err) = (String::from_utf8_lossy(&out).into_owned(), String::from_utf8_lossy(&err).into_owned());
        Run { out, err, ending, return_code, events: events.take(), remains, files: files(&self.paths), task, sql: recording.map(|r| r.text()).unwrap_or_default() }
    }
}

/// An observer that keeps what it is told, and what it has kept.
fn observed<'w>() -> (Rc<RefCell<Events>>, unit::Observer<'w>) {
    let events = Rc::new(RefCell::new(Events::default()));
    let recorder = events.clone();
    (events, Box::new(move |e: Event<'_>| recorder.borrow_mut().record(&e)))
}

/// A recording's text, as the recorder writes it.
#[derive(Clone, Default)]
struct Recording(Rc<RefCell<Vec<u8>>>);

impl Recording {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.borrow()).into_owned()
    }
}

impl Write for Recording {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The files the DDs name and their open marks, then a task's data sets and the host files its
/// transient-data queues are written to.
fn paths(specs: &[String], task: Option<&cics::Task>) -> Vec<PathBuf> {
    let dds = files::Dds::new(specs, false).unwrap();
    let names = specs.iter().filter_map(|spec| spec.split_once('=')).map(|(name, _)| name.to_ascii_uppercase());
    let mut paths: Vec<PathBuf> = names.filter_map(|name| dds.get(&name)).flat_map(|dd| [files::open_mark(&dd.path), dd.path]).collect();
    if let Some(t) = task {
        let mut held: Vec<PathBuf> = t.files.values().map(|f| f.dd.path.clone()).chain(t.td_files.values().cloned()).collect();
        held.sort();
        paths.extend(held);
    }
    paths
}

/// Each file as it stands, None where there is none.
fn files(paths: &[PathBuf]) -> Vec<(PathBuf, Option<Vec<u8>>)> {
    paths.iter().map(|path| (path.clone(), std::fs::read(path).ok())).collect()
}

fn restore(files: &[(PathBuf, Option<Vec<u8>>)]) {
    for (path, content) in files {
        match content {
            Some(bytes) => std::fs::write(path, bytes).unwrap_or_else(|e| panic!("restoring {}: {e}", path.display())),
            None => {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

/// A copy of a task for the second executor's run; None for one with a terminal, which a run
/// consumes.
fn twin(t: &cics::Task) -> Option<cics::Task> {
    if t.terminal.is_some() {
        return None;
    }
    Some(cics::Task {
        transid: t.transid.clone(),
        termid: t.termid.clone(),
        userid: t.userid.clone(),
        applid: t.applid.clone(),
        sysid: t.sysid.clone(),
        number: t.number,
        commarea: t.commarea.clone(),
        files: t.files.clone(),
        ts: t.ts.clone(),
        td: t.td.clone(),
        td_files: t.td_files.clone(),
        held: t.held.clone(),
        browses: t.browses.clone(),
        terminal: None,
        initial_aid: t.initial_aid,
        mapsets: t.mapsets.clone(),
        next_transid: t.next_transid.clone(),
        returned_commarea: t.returned_commarea.clone(),
        abcode: t.abcode.clone(),
        cancelling: t.cancelling,
        activations: t.activations,
        links: t.links,
        ending_level: t.ending_level,
    })
}

/// What a task ended with that the differential test compares: RETURN's TRANSID and COMMAREA, the
/// queues, the records READ UPDATE holds, the open browses, and the abend code ASSIGN gives.
fn task_state(t: &cics::Task) -> String {
    let held: BTreeMap<_, _> = t.held.iter().collect();
    let browses: BTreeMap<_, _> = t.browses.iter().collect();
    format!("{:?}", (&t.next_transid, &t.returned_commarea, &t.ts, &t.td, held, browses, &t.abcode, t.cancelling))
}

/// With `IRONWORK_VM_REPORT` set, appends a line to that file: the test, the PROGRAM-ID, the
/// source's fingerprint and how the VM fared, tab-separated.
fn report_vm(compiled: &Compiled, fingerprint: u32, outcome: &str) {
    let Some(path) = std::env::var_os("IRONWORK_VM_REPORT") else { return };
    let thread = std::thread::current();
    let line = format!("{}\t{}\t{fingerprint:08X}\t{}\n", thread.name().unwrap_or("-"), compiled.program.id, outcome.replace('\n', " "));
    let file = std::fs::OpenOptions::new().create(true).append(true).open(path);
    let _ = file.and_then(|mut f| f.write_all(line.as_bytes()));
}

/// Fails the test, with what differs, when the VM's run differs from the interpreter's; counts a
/// run the VM stopped for something it does not run yet.
fn differential(compiled: &Compiled, fingerprint: u32, walker: &Run, vm: std::thread::Result<Run>) {
    let id = &compiled.program.id;
    let vm = match vm {
        Ok(run) => run,
        Err(panic) => {
            let message = panic.downcast_ref::<String>().cloned().or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            report_vm(compiled, fingerprint, &format!("panicked\t{message}"));
            panic!("the VM panicked running {id}: {message}");
        }
    };
    if let Err(Halt::Unimplemented(what)) = &vm.ending {
        report_vm(compiled, fingerprint, &format!("unimplemented\t{what}"));
        return;
    }
    let differences = differences(walker, &vm);
    if differences.is_empty() {
        report_vm(compiled, fingerprint, "ran");
        return;
    }
    report_vm(compiled, fingerprint, &format!("differs\t{}", differences[0]));
    panic!("the VM's run of {id} differs from the interpreter's:\n{}", differences.join("\n"));
}

fn differences(walker: &Run, vm: &Run) -> Vec<String> {
    let mut found = Vec::new();
    for (what, a, b) in [("DISPLAY output", &walker.out, &vm.out), ("standard error", &walker.err, &vm.err), ("the SQL recording", &walker.sql, &vm.sql)] {
        if a != b {
            let (x, y) = (a.split_inclusive('\n').collect::<Vec<_>>(), b.split_inclusive('\n').collect::<Vec<_>>());
            let k = x.iter().zip(&y).position(|(p, q)| p != q).unwrap_or(x.len().min(y.len()));
            found.push(format!("{what} differs at line {}: interpreter {:?}, VM {:?}", k + 1, x.get(k), y.get(k)));
        }
    }
    if walker.ending != vm.ending {
        found.push(format!("the ending differs: interpreter {:?}, VM {:?}", walker.ending, vm.ending));
    }
    if walker.return_code != vm.return_code {
        found.push(format!("RETURN-CODE differs: interpreter {}, VM {}", walker.return_code, vm.return_code));
    }
    let (a, b) = (walker.task.as_ref().map(task_state), vm.task.as_ref().map(task_state));
    if a != b {
        found.push(format!("the task differs: interpreter {a:?}, VM {b:?}"));
    }
    let (e, f) = (&walker.events, &vm.events);
    if (e.count, e.digest) != (f.count, f.digest) {
        let k = e.first.iter().zip(&f.first).position(|(p, q)| p != q).unwrap_or(e.first.len().min(f.first.len()));
        found.push(format!(
            "the events differ ({} from the interpreter, {} from the VM), first at event {}: interpreter {:?}, VM {:?}",
            e.count,
            f.count,
            k + 1,
            e.first.get(k),
            f.first.get(k)
        ));
    }
    match (&walker.remains, &vm.remains) {
        (Some(a), Some(b)) if a != b => found.push(storage_difference(a, b)),
        (a, b) if a.is_some() != b.is_some() => found.push(format!("run-unit memory kept by the interpreter: {}, by the VM: {}", a.is_some(), b.is_some())),
        _ => {}
    }
    for ((path, a), (_, b)) in walker.files.iter().zip(&vm.files) {
        if a != b {
            found.push(format!("{} differs: {} bytes from the interpreter, {} from the VM", path.display(), a.as_ref().map_or(0, Vec::len), b.as_ref().map_or(0, Vec::len)));
        }
    }
    found
}

/// The programs and the first bytes of run-unit memory that differ, each named by the program
/// whose storage holds it.
fn storage_difference(a: &Remains, b: &Remains) -> String {
    if a.programs != b.programs {
        return format!("the run unit's programs differ: interpreter {:?}, VM {:?}", a.programs, b.programs);
    }
    if a.mem == b.mem {
        let bytes = |t: &Option<(Vec<u64>, Option<&'static str>)>| t.as_ref().map(|(w, u)| ((0..w.len() * 64).filter(|&i| w[i / 64] >> (i % 64) & 1 == 1).take(12).collect::<Vec<_>>(), *u));
        return format!("taint differs: interpreter {:?}, VM {:?} (input bytes, first not followed)", bytes(&a.taint), bytes(&b.taint));
    }
    let owner = |offset: usize| a.programs.iter().filter(|(_, base)| *base <= offset).max_by_key(|(_, base)| *base).map_or_else(|| "the reserved area".to_owned(), |(name, base)| format!("{name}+{}", offset - base));
    let differing: Vec<String> = (0..a.mem.len().max(b.mem.len()))
        .filter(|&i| a.mem.get(i) != b.mem.get(i))
        .take(12)
        .map(|i| format!("{} ({i}): {:02X?} / {:02X?}", owner(i), a.mem.get(i), b.mem.get(i)))
        .collect();
    format!("run-unit memory differs ({} bytes from the interpreter, {} from the VM), interpreter / VM: {}", a.mem.len(), b.mem.len(), differing.join(", "))
}

/// Lowers a compiled program and checks what lir.md §12.2 asks of each test program: it passes
/// `verify`, comes back equal from the load-module codec and encodes again to the same bytes, prints
/// with every reference resolved and the same once decoded, and lowers again the same.
/// `Unsupported` is accepted; any other error, or a panic, fails the test.
/// With `IRONWORK_LOWER_REPORT` set, appends a line to that file: the test (or `origin`), the
/// PROGRAM-ID, the source's fingerprint and the outcome, tab-separated. The lowered program, when
/// it lowers.
pub fn check_lowering(compiled: &Compiled, fingerprint: u32, origin: Option<&str>) -> Option<Program> {
    let id = &compiled.program.id;
    let outcome = catch_unwind(AssertUnwindSafe(|| match lower::lower(compiled) {
        Ok(p) => lowered_soundly(compiled, &p).map(|()| Ok(p)),
        Err(LowerError::Unsupported(what, _)) => Ok(Err(what)),
        Err(e) => Err(e.to_string()),
    }));
    let outcome = outcome.unwrap_or_else(|panic| {
        let message = panic.downcast_ref::<String>().cloned().or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()));
        Err(format!("panicked: {}", message.unwrap_or_default()))
    });
    if let Some(path) = std::env::var_os("IRONWORK_LOWER_REPORT") {
        let thread = std::thread::current();
        let test = origin.or(thread.name()).unwrap_or("-");
        let result = match &outcome {
            Ok(Ok(_)) => "ok".to_owned(),
            Ok(Err(what)) => format!("unsupported\t{what}"),
            Err(why) => format!("error\t{}", why.replace(['\t', '\n'], " ")),
        };
        let line = format!("{test}\t{id}\t{fingerprint:08X}\t{result}\n");
        let file = std::fs::OpenOptions::new().create(true).append(true).open(path);
        let _ = file.and_then(|mut f| f.write_all(line.as_bytes()));
    }
    match outcome {
        Ok(lowered) => lowered.ok(),
        Err(why) => panic!("lowering {id}: {why}"),
    }
}

fn lowered_soundly(compiled: &Compiled, p: &Program) -> Result<(), String> {
    lower::verify(p).map_err(|e| format!("the lowered program fails verify: {e}"))?;
    let (bytes, strings) = encoded(p);
    let decoded = decode_all::<Program>("LIR", &bytes, &strings).map_err(|e| format!("the lowered program does not decode: {e}"))?;
    if decoded != *p || encoded(&decoded) != (bytes, strings) {
        return Err("the lowered program does not round-trip through the load-module codec".into());
    }
    let listing = p.to_string();
    if let Some(reference) = unresolved(&listing) {
        return Err(format!("the lowered program prints {reference}, a reference to nothing"));
    }
    if decoded.to_string() != listing {
        return Err("the lowered program prints differently once decoded".into());
    }
    if lower::lower(compiled).as_ref() != Ok(p) {
        return Err("lowering the program again gives a different LIR".into());
    }
    Ok(())
}

/// The first reference in a listing that names nothing, which the printer shows as an id and a
/// question mark: `p12?`, `symbol3?`.
fn unresolved(listing: &str) -> Option<&str> {
    listing.split(|c: char| c.is_whitespace() || "()[]{},".contains(c)).find(|word| {
        word.strip_suffix('?').is_some_and(|w| {
            let stem = w.trim_end_matches(|c: char| c.is_ascii_digit());
            stem.len() < w.len() && !stem.is_empty() && stem.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
        })
    })
}

pub fn encoded(p: &Program) -> (Vec<u8>, StringTable) {
    let mut w = Writer::new();
    p.encode(&mut w);
    (w.take(), w.strings().clone())
}

pub fn line(s: &str) -> String {
    format!("           {s}\n")
}

pub fn page() -> &'static CodePage {
    CodePage::by_ccsid(1140).unwrap()
}

pub fn ebcdic(text: &str) -> Vec<u8> {
    page().encode(text).unwrap()
}

/// Every message the compiler gives, one to a line, a warning's or informational message's after
/// its label; empty when it compiles without one.
pub fn compile_errors(source: &str) -> String {
    let parsed = syntax::parse(source).unwrap_or_else(|e| panic!("{e}"));
    let messages = compile(parsed, &[]).map_or_else(|errors| errors, |c| c.diagnostics);
    messages.iter().map(syntax::Error::labelled).collect::<Vec<_>>().join("\n")
}
