//! `--evidence`: what a run or check records in its journal (cobolwork `docs/spec/evidence.md`
//! §5): the source and COPY members by digest, each DD's digest as it is opened, closed and left
//! at the end, each program CALL loads, and how the run ended. Paths are recorded relative to the
//! directory that supplied them, never absolute.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use exec::digest::{hex, sha256_reader};
use exec::evidence::{fields, Journal, Ledger, Value};
use exec::unit::Event;
use syntax::ast::OpenMode;

/// A root as an absolute path. A program named without a directory has the empty path as its own,
/// which is the current directory.
fn absolute_root(root: &Path) -> PathBuf {
    let root = if root.as_os_str().is_empty() { Path::new(".") } else { root };
    std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf())
}

/// A path named relative to the first of `roots` that holds it, or by its file name.
pub fn relative(path: &Path, roots: &[PathBuf]) -> String {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    for root in roots {
        if let Ok(rest) = absolute.strip_prefix(absolute_root(root)) {
            return rest.to_string_lossy().replace('\\', "/");
        }
    }
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn digest(path: &Path) -> Option<(String, u64)> {
    File::open(path).and_then(sha256_reader).ok().map(|(d, n)| (hex(&d), n))
}

fn root_of(path: &Path, roots: &[PathBuf]) -> i64 {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    roots.iter().position(|r| absolute.starts_with(absolute_root(r))).map_or(-1, |i| i as i64)
}

/// Option names only, and the program by file name: a value may be a path or a URL with a password.
fn recorded_argv(command: &str, program: &str) -> Vec<String> {
    let mut out = vec![command.to_string()];
    let mut args = std::env::args().skip(1).peekable();
    while let Some(a) = args.next() {
        if a.starts_with('-') {
            out.push(a.clone());
            if args.peek().is_some_and(|v| !v.starts_with('-')) && !matches!(a.as_str(), "-silent" | "-strict-sort-keys") {
                args.next();
                out.push("<value>".into());
            }
        }
    }
    out.push(Path::new(program).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    out
}

pub fn start(dir: &Path, reads: &[PathBuf], command: &str, program: &str) -> std::io::Result<Journal> {
    Journal::create(dir, reads, command, &recorded_argv(command, program), env!("CARGO_PKG_VERSION"))
}

/// The program's source and every COPY member it read, by digest. A member the compiler supplies
/// itself has no file and is not recorded.
pub fn sources(journal: &mut Journal, sources: &[String], program: &str, roots: &[PathBuf]) {
    let named = std::iter::once(program.to_string()).chain(sources.iter().filter(|s| !s.is_empty() && !s.starts_with('(')).cloned());
    let mut seen = BTreeSet::new();
    for s in named {
        let path = PathBuf::from(&s);
        if !seen.insert(std::path::absolute(&path).unwrap_or_else(|_| path.clone())) {
            continue;
        }
        if let Some((sha, bytes)) = digest(&path) {
            let root = root_of(&path, roots).max(0);
            let _ = journal.append("input", fields([("root", Value::Int(root)), ("path", relative(&path, roots).into()), ("sha256", sha.into()), ("bytes", bytes.into())]));
        }
    }
}

const fn mode_name(mode: OpenMode) -> &'static str {
    match mode {
        OpenMode::Input => "INPUT",
        OpenMode::Output => "OUTPUT",
        OpenMode::Extend => "EXTEND",
        OpenMode::InputOutput => "I-O",
    }
}

/// A document the command wrote, by digest.
pub fn output(journal: &mut Journal, name: &str, bytes: &[u8], path: &Path, roots: &[PathBuf]) {
    let sha = hex(&exec::digest::sha256(bytes));
    let _ = journal.append("output", fields([("name", name.into()), ("sha256", sha.into()), ("bytes", Value::Int(bytes.len() as i64)), ("path", relative(path, roots).into())]));
}

/// A sink record already written: kind, file, line, whether the marker was reached, and under
/// `--trace-input` whether an input byte may have been in the operand.
type SinkRecord = (&'static str, String, u32, Option<bool>, Option<Option<bool>>);

/// How many starts of one listed statement a journal records.
pub const STATEMENT_CAP: u32 = 100;

/// `--trace-statements`' file: one `FILE:LINE` per line, split at the last colon, blank lines
/// left out; each statement as its file's name and its line.
pub fn listed_statements(path: &Path) -> Result<BTreeSet<(String, u32)>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut listed = BTreeSet::new();
    for (k, entry) in text.lines().map(str::trim).enumerate().filter(|(_, l)| !l.is_empty()) {
        let parsed = entry.rsplit_once(':').and_then(|(file, line)| Some((file_name(file)?, line.parse::<u32>().ok().filter(|&n| n > 0)?)));
        let Some(statement) = parsed else { return Err(format!("line {}: {entry:?} is not FILE:LINE", k + 1)) };
        listed.insert(statement);
    }
    Ok(listed)
}

fn file_name(path: &str) -> Option<String> {
    Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned())
}

/// A run in progress: the journal, every DD it opened, so their final state is recorded, for an
/// input trace the marker and each sink already recorded as reached or not, and for a statement
/// trace the statements listed and how many starts of each are recorded.
pub struct Run {
    journal: Journal,
    roots: Vec<PathBuf>,
    program: String,
    opened: BTreeSet<(String, PathBuf)>,
    marker: Option<String>,
    sinks: BTreeSet<SinkRecord>,
    input: bool,
    statements: BTreeMap<(String, u32), u32>,
    failed: Option<String>,
}

impl Run {
    pub fn new(journal: Journal, roots: &[PathBuf], program: &str, marker: Option<&str>) -> Self {
        Self {
            journal,
            roots: roots.to_vec(),
            program: program.to_string(),
            opened: BTreeSet::new(),
            marker: marker.map(str::to_string),
            sinks: BTreeSet::new(),
            input: false,
            statements: BTreeMap::new(),
            failed: None,
        }
    }

    /// Records at each sink whether an input byte may be in its operand, as the run unit's taint
    /// says.
    pub fn with_input(mut self, input: bool) -> Self {
        self.input = input;
        self
    }

    /// Records each start of these statements, by file name and line, up to [`STATEMENT_CAP`].
    pub fn with_statements(mut self, listed: BTreeSet<(String, u32)>) -> Self {
        self.statements = listed.into_iter().map(|s| (s, 0)).collect();
        self
    }

    fn dd(&mut self, dd: &str, event: &str, mode: Option<&str>, path: &Path) {
        let mut f = fields([("dd", dd.into()), ("event", event.into())]);
        if let Some(m) = mode {
            f.insert("mode".into(), m.into());
        }
        if let Some((sha, bytes)) = digest(path) {
            f.insert("sha256".into(), sha.into());
            f.insert("bytes".into(), bytes.into());
        }
        self.write("dd", f);
    }

    fn write(&mut self, kind: &str, f: std::collections::BTreeMap<String, Value>) {
        if let Err(e) = self.journal.append(kind, f) {
            self.failed.get_or_insert_with(|| e.to_string());
        }
    }

    pub fn observe(&mut self, event: Event<'_>) {
        match event {
            Event::Open { dd, mode, path } => {
                self.opened.insert((dd.to_string(), path.to_path_buf()));
                self.dd(dd, "open", Some(mode_name(mode)), path);
            }
            Event::Close { dd, path } => self.dd(dd, "close", None, path),
            Event::Paragraph { .. } => {}
            Event::Load { program, source } => {
                let mut f = fields([("program", program.into())]);
                if let Some((sha, _)) = source.and_then(digest) {
                    f.insert("sha256".into(), sha.into());
                }
                if let Some(p) = source {
                    f.insert("from".into(), relative(p, &self.roots).into());
                }
                self.write("call", f);
            }
            Event::Statement { file, line } => {
                let file = if file.is_empty() { self.program.as_str() } else { file };
                let Some(name) = file_name(file) else { return };
                let Some(count) = self.statements.get_mut(&(name, line)) else { return };
                if *count == STATEMENT_CAP {
                    return;
                }
                *count += 1;
                let capped = *count == STATEMENT_CAP;
                let mut f = fields([("file", relative(Path::new(file), &self.roots).into()), ("line", i64::from(line).into())]);
                if capped {
                    f.insert("capped".into(), true.into());
                }
                self.write("statement", f);
            }
            Event::Sink { kind, file, line, operand, input } => {
                if self.marker.is_none() && !self.input {
                    return;
                }
                let reached = self.marker.as_ref().map(|m| operand.contains(m.as_str()));
                let input = self.input.then_some(input);
                if self.sinks.insert((kind, file.to_string(), line, reached, input)) {
                    let file = relative(Path::new(if file.is_empty() { self.program.as_str() } else { file }), &self.roots);
                    let mut f = fields([("sink", kind.into()), ("file", file.into()), ("line", i64::from(line).into())]);
                    if let (Some(marker), Some(reached)) = (&self.marker, reached) {
                        f.insert("marker".into(), marker.clone().into());
                        f.insert("reached".into(), reached.into());
                    }
                    if let Some(input) = input {
                        f.insert("input".into(), input.map_or(Value::Null, Value::Bool));
                    }
                    self.write("sink", f);
                }
            }
        }
    }

    /// A DD whose data set is recorded as the step left it, whether or not a program opened it.
    pub fn track(&mut self, dd: &str, path: &Path) {
        self.opened.insert((dd.to_string(), path.to_path_buf()));
    }

    pub fn journal_mut(&mut self) -> &mut Journal {
        &mut self.journal
    }

    /// Records each opened DD as the run left it, and the abend if there was one.
    pub fn end(mut self, abend: Option<(String, Option<&str>, i64)>) -> Journal {
        for (dd, path) in std::mem::take(&mut self.opened) {
            self.dd(&dd, "end", None, &path);
        }
        if let Some((code, file, line)) = abend {
            let mut f = fields([("code", code.into())]);
            // The program's own source is file 0, which the compile leaves unnamed.
            if let Some(file) = file.map(|f| if f.is_empty() { self.program.as_str() } else { f }).filter(|f| !f.is_empty() && line > 0) {
                f.insert("file".into(), relative(Path::new(file), &self.roots).into());
                f.insert("line".into(), Value::Int(line));
            }
            self.write("abend", f);
        }
        if let Some(reason) = self.failed {
            eprintln!("ironwork: --evidence: a record could not be written: {reason}");
        }
        self.journal
    }
}

/// Closes the journal and returns the command's own status; a ledger that could not be written is
/// said on standard error and does not change it.
pub fn finish(journal: Option<Journal>, status: i64) -> ExitCode {
    if let Some(j) = journal {
        let id = j.id.clone();
        match j.close(Some(status)) {
            Ok(Ledger::Recorded) => {}
            Ok(Ledger::Unrecorded(reason)) => eprintln!("ironwork: --evidence: run {id} was not recorded in the ledger: {reason}"),
            Err(e) => eprintln!("ironwork: --evidence: run {id} could not be closed: {e}"),
        }
    }
    ExitCode::from(u8::try_from(status.clamp(0, 255)).unwrap_or(2))
}
