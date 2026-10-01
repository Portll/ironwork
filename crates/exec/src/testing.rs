use crate::lower::{self, LowerError};
use crate::{Abend, Compiled, Ending, Execute, cics, compile, compile_at, files, unit};
use rt::lir::{CompileTime, Program};
use rt::module::StringTable;
use rt::module::codec::{Encode, Writer, decode_all};
use std::io::{Cursor, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use zarch::ebcdic::CodePage;

pub enum Executor {
    Interpreter,
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

    pub fn run(self, executor: Executor) -> Outcome {
        let Executor::Interpreter = executor;
        let mut programs = syntax::parse_all_with(&self.source, &syntax::copy::Libraries::default()).unwrap_or_else(|e| panic!("{e}"));
        let main = programs.remove(0);
        let compiled = match self.when_compiled {
            Some(at) => compile_at(main, &self.flags, at),
            None => compile(main, &self.flags),
        };
        let compiled = compiled.unwrap_or_else(|e| panic!("{e:?}"));
        programs.extend(self.classes.iter().map(|c| syntax::parse(c).unwrap_or_else(|e| panic!("{e}\n{c}"))));
        let fingerprint = rt::sql::fingerprint(&format!("{}\n{}", self.source, self.flags.join(" ")));
        let library = unit::Library { programs, dirs: self.dirs, flags: self.flags, ..Default::default() };
        check_lowering(&compiled, fingerprint, None);
        for program in &library.programs {
            // Compiled as `RunUnit::load` compiles a CALLed program; one that does not compile is left out.
            if let Ok(Ok(c)) = catch_unwind(AssertUnwindSafe(|| compile(program.clone(), &library.flags))) {
                check_lowering(&c, fingerprint, None);
            }
        }
        let dds = files::Dds::new(&self.dds, false).unwrap();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let (ending, return_code, task) = match self.task {
            Some(task) => {
                let task = cics::Task { commarea: self.commarea.map(|c| compiled.options.code_page().encode(&c).unwrap()), ..task };
                match compiled.execute_cics(library, dds, task, self.clock, &mut out, &mut err) {
                    Ok((ending, task)) => (Ok(ending), 0, Some(task)),
                    Err(abend) => (Err(abend), 0, None),
                }
            }
            None => {
                let sysin = self.sysin.map(|s| Box::new(Cursor::new(s.into_bytes())) as Box<dyn std::io::BufRead>);
                match compiled.execute(library, dds, sysin, self.clock, &mut out, &mut err) {
                    Ok((ending, code)) => (Ok(ending), code, None),
                    Err(abend) => (Err(abend), 0, None),
                }
            }
        };
        Outcome { out: String::from_utf8(out).unwrap(), err: String::from_utf8(err).unwrap(), ending, return_code, task }
    }
}

/// Lowers a compiled program and checks what lir.md §12.2 asks of each test program: it passes
/// `verify`, comes back equal from the load-module codec and encodes again to the same bytes, and
/// lowers again the same. `Unsupported` is accepted; any other error, or a panic, fails the test.
/// With `IRONWORK_LOWER_REPORT` set, appends a line to that file: the test (or `origin`), the
/// PROGRAM-ID, the source's fingerprint and the outcome, tab-separated.
pub fn check_lowering(compiled: &Compiled, fingerprint: u32, origin: Option<&str>) {
    let id = &compiled.program.id;
    let outcome = catch_unwind(AssertUnwindSafe(|| match lower::lower(compiled) {
        Ok(p) => lowered_soundly(compiled, &p).map(|()| None),
        Err(LowerError::Unsupported(what, _)) => Ok(Some(what)),
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
            Ok(None) => "ok".to_owned(),
            Ok(Some(what)) => format!("unsupported\t{what}"),
            Err(why) => format!("error\t{}", why.replace(['\t', '\n'], " ")),
        };
        let line = format!("{test}\t{id}\t{fingerprint:08X}\t{result}\n");
        let file = std::fs::OpenOptions::new().create(true).append(true).open(path);
        let _ = file.and_then(|mut f| f.write_all(line.as_bytes()));
    }
    if let Err(why) = outcome {
        panic!("lowering {id}: {why}");
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
