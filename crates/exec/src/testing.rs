use crate::lower::{self, LowerError};
use crate::unit::{Event, Remains};
use crate::vm::{self, Code, Halt};
use crate::{Abend, Compiled, Ending, Execute, cics, compile, compile_at, files, unit};
use rt::lir::{CompileTime, Program};
use rt::module::StringTable;
use rt::module::codec::{Encode, Writer, decode_all};
use std::cell::RefCell;
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
        }
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

    /// Runs the program. Under the interpreter, a program that lowers runs on the VM too with the
    /// same inputs, the system clock read once for both, and the two must agree in everything
    /// [`Run`] holds (docs/lir.md §12.3); what the VM does not run yet is counted, not failed.
    pub fn run(self, executor: Executor) -> Outcome {
        let mut programs = syntax::parse_all_with(&self.source, &syntax::copy::Libraries::default()).unwrap_or_else(|e| panic!("{e}"));
        let main = programs.remove(0);
        let compiled = match self.when_compiled {
            Some(at) => compile_at(main, &self.flags, at),
            None => compile(main, &self.flags),
        };
        let compiled = compiled.unwrap_or_else(|e| panic!("{e:?}"));
        programs.extend(self.classes.iter().map(|c| syntax::parse(c).unwrap_or_else(|e| panic!("{e}\n{c}"))));
        let fingerprint = rt::sql::fingerprint(&format!("{}\n{}", self.source, self.flags.join(" ")));
        let library = unit::Library { programs, dirs: self.dirs, flags: self.flags, trace_statements: Some(unit::StatementFilter::All), ..Default::default() };
        let lowered = check_lowering(&compiled, fingerprint, None);
        for program in &library.programs {
            // Compiled as `RunUnit::load` compiles a CALLed program; one that does not compile is left out.
            if let Ok(Ok(c)) = catch_unwind(AssertUnwindSafe(|| compile(program.clone(), &library.flags))) {
                check_lowering(&c, fingerprint, None);
            }
        }
        if let Some(task) = self.task {
            if let Executor::Vm = executor {
                panic!("the VM does not run a CICS task yet");
            }
            if lowered.is_some() {
                report_vm(&compiled, fingerprint, "unimplemented\ta CICS task");
            }
            let task = cics::Task { commarea: self.commarea.map(|c| compiled.options.code_page().encode(&c).unwrap()), ..task };
            let dds = files::Dds::new(&self.dds, false).unwrap();
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let (ending, task) = match compiled.execute_cics(library, dds, task, self.clock, &mut out, &mut err) {
                Ok((ending, task)) => (Ok(ending), Some(task)),
                Err(abend) => (Err(abend), None),
            };
            return Outcome { out: String::from_utf8(out).unwrap(), err: String::from_utf8(err).unwrap(), ending, return_code: 0, task };
        }
        let clock = match self.clock {
            unit::Clock::System => {
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
                unit::Clock::Fixed(now.as_secs() as i64, now.subsec_millis() / 10)
            }
            fixed => fixed,
        };
        let inputs = Inputs { compiled: &compiled, library, dds: self.dds, sysin: self.sysin, clock };
        match executor {
            Executor::Vm => {
                let run = inputs.vm(&vm::code(&compiled));
                let ending = match run.ending {
                    Ok(e) => Ok(e),
                    Err(Halt::Abend(a)) => Err(a),
                    Err(Halt::Unimplemented(what)) => panic!("the VM does not run {what} yet"),
                };
                Outcome { out: run.out, err: run.err, ending, return_code: run.return_code, task: None }
            }
            Executor::Interpreter => {
                let before = inputs.files();
                let walker = inputs.walker();
                if lowered.is_some() {
                    let after = inputs.files();
                    restore(&before);
                    let code = vm::code(&compiled);
                    let vm = catch_unwind(AssertUnwindSafe(|| inputs.vm(&code)));
                    restore(&after);
                    differential(&compiled, fingerprint, &walker, vm);
                }
                Outcome {
                    out: walker.out,
                    err: walker.err,
                    ending: walker.ending.map_err(|h| match h {
                        Halt::Abend(a) => a,
                        Halt::Unimplemented(what) => unreachable!("the interpreter stopped for {what}"),
                    }),
                    return_code: walker.return_code,
                    task: None,
                }
            }
        }
    }
}

/// What a batch run takes, given alike to the interpreter and the VM.
struct Inputs<'c> {
    compiled: &'c Compiled,
    library: unit::Library,
    dds: Vec<String>,
    sysin: Option<String>,
    clock: unit::Clock,
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
            Event::Load { program, source } => format!("load {program} {}", source.map(|s| s.display().to_string()).unwrap_or_default()),
            Event::Paragraph { program, name, index } => format!("paragraph {program} {name} {index}"),
            Event::Sink { kind, file, line, operand } => format!("sink {kind} {file}:{line} {operand}"),
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
    fn sysin(&self) -> Option<Box<dyn std::io::BufRead>> {
        self.sysin.clone().map(|s| Box::new(Cursor::new(s.into_bytes())) as Box<dyn std::io::BufRead>)
    }

    /// The files the DDs name, as they stand, None where there is none.
    fn files(&self) -> Vec<(PathBuf, Option<Vec<u8>>)> {
        let dds = files::Dds::new(&self.dds, false).unwrap();
        let names = self.dds.iter().filter_map(|spec| spec.split_once('=')).map(|(name, _)| name.to_ascii_uppercase());
        names.filter_map(|name| dds.get(&name)).map(|dd| (dd.path.clone(), std::fs::read(&dd.path).ok())).collect()
    }

    fn walker(&self) -> Run {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let events = Rc::new(RefCell::new(Events::default()));
        let recorder = events.clone();
        let observer: unit::Observer<'_> = Box::new(move |e: Event<'_>| recorder.borrow_mut().record(&e));
        let mut remains = None;
        let dds = files::Dds::new(&self.dds, false).unwrap();
        let ended = self.compiled.execute_kept(self.library.clone(), dds, self.sysin(), self.clock, None, &mut out, &mut err, Some(observer), &mut remains);
        let (ending, return_code) = match ended {
            Ok((ending, code)) => (Ok(ending), code),
            Err(abend) => (Err(Halt::Abend(abend)), 0),
        };
        let events = Rc::try_unwrap(events).map(RefCell::into_inner).unwrap_or_default();
        Run { out: String::from_utf8(out).unwrap(), err: String::from_utf8(err).unwrap(), ending, return_code, events, remains, files: self.files() }
    }

    fn vm(&self, code: &Code) -> Run {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let events = Rc::new(RefCell::new(Events::default()));
        let recorder = events.clone();
        let observer: unit::Observer<'_> = Box::new(move |e: Event<'_>| recorder.borrow_mut().record(&e));
        let mut remains = None;
        let dds = files::Dds::new(&self.dds, false).unwrap();
        let ended = vm::execute(self.compiled, code, self.library.clone(), dds, self.sysin(), self.clock, None, &mut out, &mut err, Some(observer), None, &mut remains);
        let (ending, return_code) = match ended {
            Ok((ending, code)) => (Ok(ending), code),
            Err(halt) => (Err(halt), 0),
        };
        let events = Rc::try_unwrap(events).map(RefCell::into_inner).unwrap_or_default();
        Run { out: String::from_utf8_lossy(&out).into_owned(), err: String::from_utf8_lossy(&err).into_owned(), ending, return_code, events, remains, files: self.files() }
    }
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
    for (what, a, b) in [("DISPLAY output", &walker.out, &vm.out), ("standard error", &walker.err, &vm.err)] {
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
    let owner = |offset: usize| a.programs.iter().filter(|(_, base)| *base <= offset).max_by_key(|(_, base)| *base).map_or_else(|| "the reserved area".to_owned(), |(name, base)| format!("{name}+{}", offset - base));
    let differing: Vec<String> = (0..a.mem.len().max(b.mem.len()))
        .filter(|&i| a.mem.get(i) != b.mem.get(i))
        .take(12)
        .map(|i| format!("{} ({i}): {:02X?} / {:02X?}", owner(i), a.mem.get(i), b.mem.get(i)))
        .collect();
    format!("run-unit memory differs ({} bytes from the interpreter, {} from the VM), interpreter / VM: {}", a.mem.len(), b.mem.len(), differing.join(", "))
}

/// Lowers a compiled program and checks what lir.md §12.2 asks of each test program: it passes
/// `verify`, comes back equal from the load-module codec and encodes again to the same bytes, and
/// lowers again the same. `Unsupported` is accepted; any other error, or a panic, fails the test.
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
    if lower::lower(compiled).as_ref() != Ok(p) {
        return Err("lowering the program again gives a different LIR".into());
    }
    Ok(())
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
