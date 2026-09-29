use crate::{Abend, Ending, cics, compile, files, unit};
use std::io::Cursor;
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

    /// The task's COMMAREA, encoded in the program's code page.
    pub fn commarea(mut self, text: &str) -> Self {
        self.commarea = Some(text.to_owned());
        self
    }

    pub fn run(self, executor: Executor) -> Outcome {
        let Executor::Interpreter = executor;
        let mut programs = syntax::parse_all_with(&self.source, &syntax::copy::Libraries::default()).unwrap_or_else(|e| panic!("{e}"));
        let compiled = compile(programs.remove(0), &self.flags).unwrap_or_else(|e| panic!("{e:?}"));
        programs.extend(self.classes.iter().map(|c| syntax::parse(c).unwrap_or_else(|e| panic!("{e}\n{c}"))));
        let library = unit::Library { programs, dirs: self.dirs, ..Default::default() };
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

pub fn line(s: &str) -> String {
    format!("           {s}\n")
}

pub fn page() -> &'static CodePage {
    CodePage::by_ccsid(1140).unwrap()
}

pub fn ebcdic(text: &str) -> Vec<u8> {
    page().encode(text).unwrap()
}

/// Every message the compiler gives, one to a line; empty when it compiles.
pub fn compile_errors(source: &str) -> String {
    let parsed = syntax::parse(source).unwrap_or_else(|e| panic!("{e}"));
    compile(parsed, &[]).err().map(|e| e.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n")).unwrap_or_default()
}
