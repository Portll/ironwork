//! `--evidence`: what a run or check records in its journal (cobolwork `docs/spec/evidence.md`
//! §5): the source and COPY members by digest, each DD's digest as it is opened, closed and left
//! at the end, each program CALL loads, and how the run ended. Paths are recorded relative to the
//! directory that supplied them, never absolute.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::path::{Path, PathBuf};

use exec::digest::{hex, sha256_reader};
use exec::evidence::{fields, Journal, Ledger, Value};
use exec::module::SourceFile;
use exec::unit::Event;
use numeric::governs::Facts;
use syntax::ast::OpenMode;

/// A root as an absolute path. A program named without a directory has the empty path as its own,
/// which is the current directory.
fn absolute_root(root: &Path) -> PathBuf {
    let root = if root.as_os_str().is_empty() { Path::new(".") } else { root };
    std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf())
}

/// The root that supplied `path`, as its index and the path from it: the innermost of `roots` that
/// holds it, so a library inside the program's directory, or inside another library, names its own
/// members.
fn holder(path: &Path, roots: &[PathBuf]) -> Option<(usize, String)> {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut found: Option<(usize, usize, String)> = None;
    for (k, root) in roots.iter().enumerate() {
        let root = absolute_root(root);
        if let Ok(rest) = absolute.strip_prefix(&root) {
            let depth = root.components().count();
            if found.as_ref().is_none_or(|(deepest, _, _)| depth > *deepest) {
                found = Some((depth, k, rest.to_string_lossy().replace('\\', "/")));
            }
        }
    }
    found.map(|(_, k, rest)| (k, rest))
}

/// A path named relative to the root that supplied it, or by its file name.
pub fn relative(path: &Path, roots: &[PathBuf]) -> String {
    match holder(path, roots) {
        Some((_, rest)) => rest,
        None => path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
    }
}

pub fn digest_bytes(path: &Path) -> Option<([u8; 32], u64)> {
    File::open(path).and_then(sha256_reader).ok()
}

fn digest(path: &Path) -> Option<(String, u64)> {
    digest_bytes(path).map(|(d, n)| (hex(&d), n))
}

pub fn root_of(path: &Path, roots: &[PathBuf]) -> i64 {
    holder(path, roots).map_or(-1, |(k, _)| k as i64)
}

/// Option names only, and the program by file name: a value may be a path or a URL with a password.
/// A statement, time or storage limit is kept, since whether and where the run ends depends on it.
/// The compliance level's and source format's values are kept, being words that decide what
/// compiles.
/// The dialect's value is kept, being one of two words that change the run's results, and so is
/// each `--assume` ID=VALUE.
fn recorded_argv(command: &str, program: &str) -> Vec<String> {
    let mut out = vec![command.to_string()];
    let mut args = std::env::args().skip(1).peekable();
    while let Some(a) = args.next() {
        if a.starts_with('-') {
            out.push(a.clone());
            if a == "--compliance" || a == "--source-format" || a == "--dialect" || a == "--assume" {
                out.extend(args.next());
            } else if args.peek().is_some_and(|v| !v.starts_with('-')) && !matches!(a.as_str(), "-silent" | "-strict-sort-keys" | "--exit-code") {
                let value = args.next().unwrap_or_default();
                let limit = matches!(a.as_str(), "--statement-limit" | "--time-limit" | "--storage-limit") && value.bytes().all(|b| b.is_ascii_alphanumeric());
                out.push(if limit { value } else { "<value>".into() });
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

/// The files a load module records its program's compile read, as `sources` records a source's:
/// the source, then each COPY member, each once.
pub fn recorded_sources(journal: &mut Journal, files: &[Option<SourceFile>]) {
    let mut seen = BTreeSet::new();
    for file in files.iter().flatten() {
        if seen.insert((file.root, file.path.as_str())) {
            let _ = journal.append("input", fields([("root", Value::Int(i64::from(file.root))), ("path", file.path.clone().into()), ("sha256", hex(&file.sha256).into()), ("bytes", file.bytes.into())]));
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
/// input trace the marker and each sink already recorded as reached or not, for a statement trace
/// the statements listed and how many starts of each are recorded, and the path the journal gives
/// each debug-table name of a load module's programs.
pub struct Run {
    journal: Journal,
    roots: Vec<PathBuf>,
    program: String,
    recorded: BTreeMap<String, String>,
    opened: BTreeSet<(String, PathBuf)>,
    marker: Option<String>,
    sinks: BTreeSet<SinkRecord>,
    input: bool,
    statements: BTreeMap<(String, u32), u32>,
    programs: Vec<Facts>,
    run_facts: Facts,
    failed: Option<String>,
}

impl Run {
    pub fn new(journal: Journal, roots: &[PathBuf], program: &str, marker: Option<&str>) -> Self {
        Self {
            journal,
            roots: roots.to_vec(),
            program: program.to_string(),
            recorded: BTreeMap::new(),
            opened: BTreeSet::new(),
            marker: marker.map(str::to_string),
            sinks: BTreeSet::new(),
            input: false,
            statements: BTreeMap::new(),
            programs: Vec::new(),
            run_facts: Facts::default(),
            failed: None,
        }
    }

    /// What the run's first program holds and how the run was made; each program and class the
    /// run loads adds its own.
    pub fn with_facts(mut self, program: Facts, run: Facts) -> Self {
        self.add_program(program);
        self.add_run_facts(run);
        self
    }

    pub fn add_program(&mut self, program: Facts) {
        self.programs.push(program);
    }

    pub fn add_run_facts(&mut self, run: Facts) {
        self.run_facts.union(run);
    }

    /// Records at each sink whether an input byte may be in its operand, as the run unit's taint
    /// says.
    pub fn with_input(mut self, input: bool) -> Self {
        self.input = input;
        self
    }

    /// Names each file a load module's program names by the path the module records for it, where
    /// it records one: what its compile read, relative to the library it was found in.
    pub fn with_recorded<'a>(mut self, names: impl IntoIterator<Item = (&'a str, &'a Option<SourceFile>)>) -> Self {
        self.record(names);
        self
    }

    pub fn record<'a>(&mut self, names: impl IntoIterator<Item = (&'a str, &'a Option<SourceFile>)>) {
        for (name, file) in names {
            if let Some(file) = file {
                self.recorded.insert(name.to_owned(), file.path.clone());
            }
        }
    }

    /// A file an event or an abend names, as the journal records it: by the path a load module
    /// records for it, else relative to its root, the run's own program for the empty name.
    fn named(&self, file: &str) -> String {
        match self.recorded.get(file) {
            Some(path) => path.clone(),
            None => relative(Path::new(if file.is_empty() { self.program.as_str() } else { file }), &self.roots),
        }
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
            Event::Class { facts, .. } => self.add_program(facts),
            Event::Load { program, source, recorded, facts } => {
                self.add_program(facts);
                let mut f = fields([("program", program.into())]);
                if let Some((sha, _)) = source.and_then(digest) {
                    f.insert("sha256".into(), sha.into());
                }
                if let Some(p) = source {
                    f.insert("from".into(), relative(p, &self.roots).into());
                }
                if let Some((_, Some(own))) = recorded.first() {
                    f.insert("sha256".into(), hex(&own.sha256).into());
                    f.insert("from".into(), own.path.clone().into());
                }
                self.record(recorded.iter().map(|(name, file)| (name.as_str(), file)));
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
                let mut f = fields([("file", self.named(file).into()), ("line", i64::from(line).into())]);
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
                    let mut f = fields([("sink", kind.into()), ("file", self.named(file).into()), ("line", i64::from(line).into())]);
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

    /// Records each opened DD as the run left it, and the abend if there was one, and gives the
    /// close record the assumptions met within some program the run entered, with those a job's
    /// earlier steps met.
    pub fn end(mut self, abend: Option<(String, Option<&str>, i64)>) -> Journal {
        if !self.programs.is_empty() {
            let met = numeric::assumptions::governed_by_programs(&self.programs, self.run_facts);
            let ids = self.journal.assumptions.get_or_insert_default();
            ids.extend(met);
            ids.sort_unstable();
            ids.dedup();
        }
        for (dd, path) in std::mem::take(&mut self.opened) {
            self.dd(&dd, "end", None, &path);
        }
        if let Some((code, file, line)) = abend {
            let mut f = fields([("code", code.into())]);
            // The program's own source is file 0, which the compile leaves unnamed.
            if let Some(file) = file.filter(|f| (!f.is_empty() || !self.program.is_empty()) && line > 0) {
                f.insert("file".into(), self.named(file).into());
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

/// Closes the journal with the command's status; a ledger that could not be written is said on
/// standard error and does not change the command's exit status.
pub fn finish(journal: Option<Journal>, status: i64) {
    if let Some(j) = journal {
        let id = j.id.clone();
        match j.close(Some(status)) {
            Ok(Ledger::Recorded) => {}
            Ok(Ledger::Unrecorded(reason)) => eprintln!("ironwork: --evidence: run {id} was not recorded in the ledger: {reason}"),
            Err(e) => eprintln!("ironwork: --evidence: run {id} could not be closed: {e}"),
        }
    }
}
