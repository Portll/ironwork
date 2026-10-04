//! `ironwork compare`: change assurance (cobolwork `docs/spec/evidence.md` §12). The base and head
//! programs run on the same inputs, each in its own directory holding copies of every input DD, with
//! the same clock, SYSIN and SQL recording; every output DD, the DISPLAY output, the RETURN-CODE and
//! the abend are compared byte for byte. With --expected the head's outputs are compared with given
//! files instead, which is how a translation to another language is checked against the original.
//! The result is an in-toto statement whose subjects are the two sources.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use exec::Execute;
use exec::digest::{hex, sha256};
use exec::evidence::{canonical, fields, Value};

pub const PREDICATE: &str = "https://github.com/Portll/ironwork/blob/main/docs/evidence.md#equivalence-v1";
pub(crate) const LIMIT: &str = "Equivalence is under ironwork's model of Enterprise COBOL, on the inputs recorded here; the oracle holds no Enterprise COBOL goldens yet, and coverage is of paragraphs entered, not of statements or branches within them, so a verdict of equivalent covers these inputs only.";

/// The time both sides ran at, as an ISO 8601 UTC timestamp; null for the system clock.
pub(crate) fn clock_value(clock: exec::unit::Clock) -> Value {
    match clock {
        exec::unit::Clock::Fixed(seconds, hundredths) => exec::evidence::iso(seconds, hundredths * 10).into(),
        exec::unit::Clock::System => Value::Null,
    }
}

/// The head's paragraphs the change touched, and those of them the inputs never entered. A change
/// that touched no paragraph's statements (data, or a copybook in the DATA DIVISION) is held to
/// every paragraph. Null when the head did not run.
fn coverage_of(head: &Outcome, base: Option<&Outcome>) -> Value {
    if head.error.is_some() || head.paragraphs.is_empty() {
        return Value::Null;
    }
    let all: Vec<usize> = (0..head.paragraphs.len()).collect();
    let changed: Vec<usize> = match base.filter(|b| b.error.is_none()) {
        Some(b) => {
            let before: BTreeMap<&str, &str> = b.paragraphs.iter().map(|(n, f)| (n.as_str(), f.as_str())).collect();
            all.iter().copied().filter(|&i| before.get(head.paragraphs[i].0.as_str()) != Some(&head.paragraphs[i].1.as_str())).collect()
        }
        None => all.clone(),
    };
    let (scope, changed) = if changed.is_empty() { ("all", all.clone()) } else if base.is_some() { ("changed", changed) } else { ("all", changed) };
    let name = |i: &usize| Value::Str(head.paragraphs[*i].0.clone());
    let reached = all.iter().filter(|&&i| head.coverage.reached(&head.main, i)).count();
    let unreached: Vec<Value> = changed.iter().filter(|&&i| !head.coverage.reached(&head.main, i)).map(name).collect();
    Value::Obj(fields([
        ("paragraphs", Value::Int(all.len() as i64)),
        ("reached", Value::Int(reached as i64)),
        ("scope", scope.into()),
        ("changed", Value::Arr(changed.iter().map(name).collect())),
        ("unreached", Value::Arr(unreached)),
    ]))
}

pub struct Request {
    pub base: Option<PathBuf>,
    pub head: PathBuf,
    pub dds: Vec<String>,
    pub libraries: Vec<PathBuf>,
    pub program_dirs: Vec<PathBuf>,
    pub flags: Vec<String>,
    pub clock: exec::unit::Clock,
    pub replay: Option<PathBuf>,
    pub expected: Vec<(String, PathBuf)>,
    pub declare: Option<PathBuf>,
    pub statement: Option<PathBuf>,
}

/// A divergence the change means to make: a DD (optionally one line range of it), the DISPLAY
/// output or the RETURN-CODE, with the reason.
pub(crate) struct Declared {
    pub what: String,
    pub lines: Option<(usize, usize)>,
    pub reason: String,
}

/// Declarations, one a line: `DD NAME`, `DATASET DSN`, `STEP NAME`, `DISPLAY` or `RETURN-CODE`, optionally
/// `lines A-B`, then the reason.
pub(crate) fn parse_declared(text: &str) -> Result<Vec<Declared>, String> {
    let mut out = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut words = line.splitn(2, char::is_whitespace);
        let head = words.next().unwrap_or_default().to_ascii_uppercase();
        let rest = words.next().unwrap_or("").trim();
        let (what, rest) = match head.as_str() {
            "DD" | "DATASET" | "STEP" => {
                let mut w = rest.splitn(2, char::is_whitespace);
                (format!("{head} {}", w.next().unwrap_or_default().to_ascii_uppercase()), w.next().unwrap_or("").trim())
            }
            "DISPLAY" | "RETURN-CODE" => (head.clone(), rest),
            _ => return Err(format!("line {}: a declaration starts DD, DATASET, STEP, DISPLAY or RETURN-CODE", n + 1)),
        };
        let (lines, reason) = match rest.strip_prefix("lines ") {
            Some(r) => {
                let mut w = r.splitn(2, char::is_whitespace);
                let range = w.next().unwrap_or_default();
                let (a, b) = range.split_once('-').unwrap_or((range, range));
                let parse = |s: &str| s.parse::<usize>().map_err(|_| format!("line {}: {range} is not a line range", n + 1));
                (Some((parse(a)?, parse(b)?)), w.next().unwrap_or("").trim())
            }
            None => (None, rest),
        };
        if reason.is_empty() {
            return Err(format!("line {}: a declaration says why", n + 1));
        }
        out.push(Declared { what, lines, reason: reason.to_string() });
    }
    Ok(out)
}

struct Spec {
    name: String,
    path: PathBuf,
    suffix: String,
}

fn parse_dd(spec: &str) -> Result<Spec, String> {
    let (name, rest) = spec.split_once('=').ok_or_else(|| format!("--dd {spec}: NAME=path"))?;
    let (path, suffix) = match rest.rsplit_once(':') {
        Some((p, s)) if matches!(s, "text" | "variable") || s.starts_with("RRDS") || s.starts_with("KSDS") => (p, format!(":{s}")),
        _ => (rest, String::new()),
    };
    Ok(Spec { name: name.to_ascii_uppercase(), path: PathBuf::from(path), suffix })
}

struct Outcome {
    /// The program and every COPY member it read, named relative to their library, by digest.
    closure: Vec<(String, String)>,
    return_code: Option<i64>,
    abend: Option<(String, String)>,
    display: Vec<u8>,
    files: BTreeMap<String, Option<Vec<u8>>>,
    error: Option<String>,
    /// The program's PROGRAM-ID and each paragraph's name and statements, positions left out.
    main: String,
    paragraphs: Vec<(String, String)>,
    coverage: crate::coverage::Coverage,
}

/// A paragraph's statements as text with every source position removed, so a paragraph that only
/// moved is the same paragraph.
fn fingerprint(statements: &[syntax::ast::Stmt]) -> String {
    let text = format!("{statements:?}");
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(at) = rest.find("Pos {") {
        out.push_str(&rest[..at]);
        rest = rest[at..].find('}').map_or("", |end| &rest[at + end + 1..]);
    }
    out.push_str(rest);
    out
}

/// A new directory under the system's temporary directory, made by this call alone: a directory
/// someone made first under the same name is never used.
pub(crate) fn scratch(label: &str) -> std::io::Result<PathBuf> {
    use std::hash::{BuildHasher, Hasher};
    for _ in 0..16 {
        let mut keyed = std::collections::hash_map::RandomState::new().build_hasher();
        keyed.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
        let nonce = hex(&sha256(format!("{}{}{label}", keyed.finish(), std::process::id()).as_bytes()))[..16].to_string();
        let dir = std::env::temp_dir().join(format!("ironwork-{label}-{nonce}"));
        match fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::other("no unused temporary directory name"))
}

pub(crate) enum Assessment {
    Same,
    Declared,
    Undeclared,
}

/// Each line where two outputs differ, 1-based, with the byte offset of its first difference.
fn line_differences(a: &[u8], b: &[u8]) -> Vec<(usize, usize)> {
    let (la, lb): (Vec<&[u8]>, Vec<&[u8]>) = (a.split_inclusive(|&c| c == b'\n').collect(), b.split_inclusive(|&c| c == b'\n').collect());
    let mut out = Vec::new();
    let mut offset = 0;
    for i in 0..la.len().max(lb.len()) {
        let (x, y) = (la.get(i).copied().unwrap_or_default(), lb.get(i).copied().unwrap_or_default());
        if x != y {
            let within = x.iter().zip(y).position(|(p, q)| p != q).unwrap_or(x.len().min(y.len()));
            out.push((i + 1, offset + within));
        }
        offset += x.len();
    }
    out
}

/// One output compared: every line that differs must fall in a declaration of `what` (one
/// without lines covers the whole output), and the first difference no declaration covers is the
/// one reported.
pub(crate) fn assess(what: &str, a: Option<&[u8]>, b: Option<&[u8]>, declared: &[Declared]) -> (BTreeMap<String, Value>, Assessment) {
    let diffs = match (a, b) {
        (Some(a), Some(b)) => line_differences(a, b),
        (None, None) => Vec::new(),
        _ => vec![(1, 0)],
    };
    let mut r = fields([("what", what.into()), ("same", diffs.is_empty().into()), ("expected", digest_of(a)), ("actual", digest_of(b))]);
    if diffs.is_empty() {
        return (r, Assessment::Same);
    }
    let covering = |line: usize| declared.iter().find(|d| d.what == what && d.lines.is_none_or(|(x, y)| (x..=y).contains(&line)));
    let uncovered = diffs.iter().find(|(line, _)| covering(*line).is_none());
    let (line, offset) = uncovered.copied().unwrap_or(diffs[0]);
    r.insert("firstDifference".into(), Value::Obj(fields([("line", Value::Int(line as i64)), ("offset", Value::Int(offset as i64))])));
    r.insert("differingLines".into(), Value::Int(diffs.len() as i64));
    if uncovered.is_some() {
        return (r, Assessment::Undeclared);
    }
    let mut reasons: Vec<String> = diffs.iter().filter_map(|(l, _)| covering(*l)).map(|d| d.reason.clone()).collect();
    reasons.dedup();
    r.insert("declared".into(), reasons.join("; ").into());
    (r, Assessment::Declared)
}

/// Runs one program, read from `source`, with every DD pointed at a copy in `dir` of the inputs
/// as they were when the comparison began.
fn run_side(program: &Path, source: &[u8], req: &Request, specs: &[Spec], snapshot: &BTreeMap<String, Option<Vec<u8>>>, dir: &Path) -> Outcome {
    let mut outcome = Outcome { closure: Vec::new(), return_code: None, abend: None, display: Vec::new(), files: BTreeMap::new(), error: None, main: String::new(), paragraphs: Vec::new(), coverage: Default::default() };
    let fail = |mut o: Outcome, e: String| {
        o.error = Some(e);
        o
    };
    let mut local = Vec::new();
    for s in specs {
        let copy = dir.join(&s.name);
        if let Some(Some(bytes)) = snapshot.get(&s.name)
            && let Err(e) = fs::write(&copy, bytes)
        {
            return fail(outcome, format!("copying DD {}: {e}", s.name));
        }
        local.push(format!("{}={}{}", s.name, copy.display(), s.suffix));
    }
    let text = syntax::copy::decode(source);
    let own = program.parent().map(Path::to_path_buf).unwrap_or_default();
    let libraries = syntax::copy::Libraries::new(std::iter::once(own.clone()).chain(req.libraries.iter().cloned()).collect()).with_program(program).with_compliance(numeric::Compliance::of(&req.flags));
    let mut programs = match syntax::parse_all_with(&text, &libraries) {
        Ok(p) => p,
        Err(e) => return fail(outcome, e.place(&program.display().to_string()).to_string()),
    };
    let first = programs.remove(0);
    outcome.main = first.id.clone();
    outcome.paragraphs = first.paragraphs.iter().map(|p| (p.name.clone(), fingerprint(&p.statements))).collect();
    let roots: Vec<PathBuf> = std::iter::once(own.clone()).chain(req.libraries.iter().cloned()).collect();
    let own_name = program.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    outcome.closure.push((own_name, hex(&sha256(source))));
    for s in first.sources.iter().filter(|s| !s.is_empty() && !s.starts_with('(') && Path::new(s) != program) {
        let path = PathBuf::from(s);
        if let Ok(bytes) = fs::read(&path) {
            let name = roots.iter().find_map(|r| path.strip_prefix(r).ok()).map_or_else(|| path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), |p| p.to_string_lossy().replace('\\', "/"));
            if !outcome.closure.iter().any(|(n, _)| *n == name) {
                outcome.closure.push((name, hex(&sha256(&bytes))));
            }
        }
    }
    let library = exec::unit::Library { programs, dirs: std::iter::once(own).chain(req.program_dirs.iter().cloned()).collect(), copy: libraries, flags: req.flags.clone(), trace_statements: None, trace_input: false, statement_limit: None, program_ids: None };
    let compiled = match exec::compile(first, &req.flags) {
        Ok(c) => c,
        Err(errors) => return fail(outcome, syntax::most_severe(&errors).map(|e| e.place(&program.display().to_string()).to_string()).unwrap_or_default()),
    };
    let dds = match exec::files::Dds::new(&local, false) {
        Ok(d) => d,
        Err(e) => return fail(outcome, e),
    };
    let sysin: Box<dyn std::io::BufRead> = match dds.get("SYSIN").and_then(|d| fs::File::open(d.path).ok()) {
        Some(f) => Box::new(std::io::BufReader::new(f)),
        None => Box::new(std::io::empty()),
    };
    let mut replay = match &req.replay {
        Some(file) => match fs::read_to_string(file).map_err(|e| e.to_string()).and_then(|t| exec::sql::Replay::parse(&t, false)) {
            Ok(r) => Some(r),
            Err(e) => return fail(outcome, format!("--sql-replay {}: {e}", file.display())),
        },
        None => None,
    };
    let database = replay.as_mut().map(|r| r as &mut dyn exec::sql::Database);
    let mut err = Vec::new();
    let called = std::cell::RefCell::new(Vec::new());
    let coverage = std::cell::RefCell::new(crate::coverage::Coverage::default());
    let observer: exec::unit::Observer<'_> = Box::new(|event| {
        coverage.borrow_mut().observe(&event);
        if let exec::unit::Event::Load { source: Some(path), .. } = event {
            called.borrow_mut().push(path.to_path_buf());
        }
    });
    let ended = compiled.execute_observed(library, dds, Some(sysin), req.clock, database, &mut outcome.display, &mut err, Some(observer));
    outcome.coverage = coverage.into_inner();
    for path in called.into_inner() {
        if let Ok(bytes) = fs::read(&path) {
            let name = format!("called:{}", roots.iter().chain(&req.program_dirs).find_map(|r| path.strip_prefix(r).ok()).map_or_else(|| path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), |p| p.to_string_lossy().replace('\\', "/")));
            if !outcome.closure.iter().any(|(n, _)| *n == name) {
                outcome.closure.push((name, hex(&sha256(&bytes))));
            }
        }
    }
    match ended {
        Ok((_, rc)) => outcome.return_code = Some(i64::from(rc)),
        Err(a) => outcome.abend = Some((a.code.to_string(), a.message.clone())),
    }
    for s in specs {
        outcome.files.insert(s.name.clone(), fs::read(dir.join(&s.name)).ok());
    }
    outcome
}

pub(crate) fn digest_of(bytes: Option<&[u8]>) -> Value {
    bytes.map_or(Value::Null, |b| hex(&sha256(b)).into())
}

fn subject(name: &str, path: &Path, source: &[u8]) -> Value {
    let digest = hex(&sha256(source));
    Value::Obj(fields([("name", format!("{name}:{}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()).into()), ("digest", Value::Obj(fields([("sha256", digest.into())])))]))
}

pub fn run(req: Request) -> ExitCode {
    let specs = match req.dds.iter().map(|d| parse_dd(d)).collect::<Result<Vec<_>, _>>() {
        Ok(s) => s,
        Err(e) => return crate::usage_error(&e),
    };
    let declared = match &req.declare {
        Some(file) => match fs::read_to_string(file).map_err(|e| e.to_string()).and_then(|t| parse_declared(&t)) {
            Ok(d) => d,
            Err(e) => return crate::usage_error(&format!("--declare {}: {e}", file.display())),
        },
        None => Vec::new(),
    };
    if req.base.is_none() && req.expected.is_empty() {
        return crate::usage_error("compare needs --base, or --expected for each output to check");
    }
    let mut expected: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (n, p) in &req.expected {
        match fs::read(p) {
            Ok(b) => {
                expected.insert(n.to_ascii_uppercase(), b);
            }
            Err(e) => return crate::usage_error(&format!("--expected {n}={}: {e}", p.display())),
        }
    }
    let snapshot: BTreeMap<String, Option<Vec<u8>>> = specs.iter().map(|s| (s.name.clone(), fs::read(&s.path).ok())).collect();
    let read_source = |p: &Path| fs::read(p).map_err(|e| format!("{}: {e}", p.display()));
    let head_source = match read_source(&req.head) {
        Ok(b) => b,
        Err(e) => return crate::usage_error(&e),
    };
    let base_source = match req.base.as_deref().map(read_source).transpose() {
        Ok(b) => b,
        Err(e) => return crate::usage_error(&e),
    };
    let mut dirs = Vec::new();
    let mut side = |name: &str, program: &Path, source: &[u8]| -> Result<Outcome, String> {
        let dir = scratch(&format!("compare-{name}")).map_err(|e| e.to_string())?;
        dirs.push(dir.clone());
        Ok(run_side(program, source, &req, &specs, &snapshot, &dir))
    };
    let head = match side("head", &req.head, &head_source) {
        Ok(o) => o,
        Err(e) => return crate::usage_error(&e),
    };
    let base = match req.base.as_deref().zip(base_source.as_deref()).map(|(b, src)| side("base", b, src)).transpose() {
        Ok(o) => o,
        Err(e) => return crate::usage_error(&e),
    };
    for d in &dirs {
        let _ = fs::remove_dir_all(d);
    }

    let mut results = Vec::new();
    let mut undeclared = 0usize;
    let mut declared_hit = 0usize;
    let mut inconclusive = Vec::new();
    let mut compare = |what: String, a: Option<&[u8]>, b: Option<&[u8]>, results: &mut Vec<Value>| {
        let (r, assessment) = assess(&what, a, b, &declared);
        match assessment {
            Assessment::Declared => declared_hit += 1,
            Assessment::Undeclared => undeclared += 1,
            Assessment::Same => {}
        }
        results.push(Value::Obj(r));
    };

    for (label, o) in [("head", Some(&head)), ("base", base.as_ref())] {
        if let Some(e) = o.and_then(|o| o.error.as_ref()) {
            inconclusive.push(format!("the {label} program could not be run: {e}"));
        }
    }
    let abend_code = |o: &Outcome| o.abend.as_ref().map(|(c, _)| c.clone());
    for (label, o) in [("head", Some(&head)), ("base", base.as_ref())] {
        if let Some((code, message)) = o.and_then(|o| o.abend.as_ref())
            && code == "IRONWORK"
        {
            inconclusive.push(format!("the {label} program reached what ironwork does not model: {message}"));
        }
    }
    if let Some(base) = &base {
        compare("RETURN-CODE".into(), base.return_code.map(|v| v.to_string().into_bytes()).as_deref(), head.return_code.map(|v| v.to_string().into_bytes()).as_deref(), &mut results);
        compare("ABEND".into(), abend_code(base).map(String::into_bytes).as_deref(), abend_code(&head).map(String::into_bytes).as_deref(), &mut results);
        compare("DISPLAY".into(), Some(&base.display), Some(&head.display), &mut results);
    }
    let mut unchecked = Vec::new();
    for s in &specs {
        if let Some(want) = expected.get(&s.name) {
            compare(format!("DD {}", s.name), Some(want), head.files.get(&s.name).and_then(|f| f.as_deref()), &mut results);
        } else if let Some(base) = &base {
            compare(format!("DD {}", s.name), base.files.get(&s.name).and_then(|f| f.as_deref()), head.files.get(&s.name).and_then(|f| f.as_deref()), &mut results);
        } else {
            unchecked.push(Value::Str(s.name.clone()));
        }
    }

    let verdict = if !inconclusive.is_empty() {
        "inconclusive"
    } else if undeclared > 0 {
        "diverged"
    } else if declared_hit > 0 {
        "equivalent-as-declared"
    } else {
        "equivalent"
    };
    let inputs = Value::Arr(specs.iter().filter_map(|s| snapshot.get(&s.name).and_then(|b| b.as_deref()).map(|b| Value::Obj(fields([("dd", s.name.clone().into()), ("sha256", digest_of(Some(b)))])))).collect());
    let mut subjects = vec![subject("head", &req.head, &head_source)];
    if let (Some(b), Some(src)) = (&req.base, &base_source) {
        subjects.insert(0, subject("base", b, src));
    }
    let predicate = fields([
        ("verdict", verdict.into()),
        ("inputs", inputs),
        ("sqlRecording", req.replay.as_ref().map_or(Value::Null, |p| digest_of(fs::read(p).ok().as_deref()))),
        ("clock", clock_value(req.clock)),
        ("results", Value::Arr(results)),
        ("declared", Value::Arr(declared.iter().map(|d| Value::Obj(fields([("what", d.what.clone().into()), ("reason", d.reason.clone().into())]))).collect())),
        ("inconclusive", Value::Arr(inconclusive.iter().map(|s| Value::Str(s.clone())).collect())),
        ("unchecked", Value::Arr(unchecked)),
        ("closure", Value::Obj({
            let side = |o: &Outcome| Value::Arr(o.closure.iter().map(|(n, d)| Value::Obj(fields([("name", n.clone().into()), ("sha256", d.clone().into())]))).collect());
            let mut m = BTreeMap::new();
            m.insert("head".to_string(), side(&head));
            if let Some(b) = &base {
                m.insert("base".to_string(), side(b));
            }
            m
        })),
        ("coverage", coverage_of(&head, base.as_ref())),
        ("ironwork", env!("CARGO_PKG_VERSION").into()),
        ("limit", LIMIT.into()),
    ]);
    let statement = Value::Obj(fields([("_type", "https://in-toto.io/Statement/v1".into()), ("subject", Value::Arr(subjects)), ("predicateType", PREDICATE.into()), ("predicate", Value::Obj(predicate))]));
    let text = format!("{}\n", canonical(&statement));
    match &req.statement {
        Some(file) => {
            if let Err(e) = fs::write(file, &text) {
                eprintln!("ironwork: --statement {}: {e}", file.display());
                return ExitCode::from(2);
            }
        }
        None => print!("{text}"),
    }
    eprintln!("ironwork compare: {verdict}{}", if undeclared > 0 { format!(", {undeclared} undeclared divergence(s)") } else { String::new() });
    ExitCode::from(match verdict {
        "equivalent" | "equivalent-as-declared" => 0,
        "diverged" => 1,
        _ => 3,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_declaration_names_what_where_and_why() {
        let d = parse_declared("# intended\nDD RPTOUT lines 3-5 the heading gained a column\nDISPLAY the banner names the release\n").unwrap();
        assert_eq!((d[0].what.as_str(), d[0].lines, d[0].reason.as_str()), ("DD RPTOUT", Some((3, 5)), "the heading gained a column"));
        assert_eq!(d[1].what, "DISPLAY");
        assert!(parse_declared("DD RPTOUT\n").is_err(), "a declaration says why");
        assert!(parse_declared("FILE X because\n").is_err());
    }

    #[test]
    fn each_differing_line_is_located_by_line_and_offset() {
        assert_eq!(line_differences(b"a\nbc\n", b"a\nbd\n"), [(2, 3)]);
        assert_eq!(line_differences(b"ab", b"abc"), [(1, 2)]);
        assert_eq!(line_differences(b"x\ny\nz\n", b"x\nY\nz\nw\n"), [(2, 2), (4, 6)]);
        assert!(line_differences(b"same", b"same").is_empty());
    }

    #[test]
    fn a_declaration_covers_only_the_lines_it_names() {
        let declared = parse_declared("DD OUT lines 2-2 the field was renamed\n").unwrap();
        let (base, head) = (b"HEADER\nOLD\nFOOTER\nDATA\n".as_slice(), b"HEADER\nNEW\nFOOTER\nMORE\n".as_slice());
        let (r, a) = assess("DD OUT", Some(base), Some(head), &declared);
        assert!(matches!(a, Assessment::Undeclared));
        assert_eq!(r.get("firstDifference"), Some(&Value::Obj(fields([("line", Value::Int(4)), ("offset", Value::Int(18))]))));
        let (_, a) = assess("DD OUT", Some(base), Some(b"HEADER\nNEW\nFOOTER\nDATA\n"), &declared);
        assert!(matches!(a, Assessment::Declared));
        let whole = parse_declared("DD OUT the report was redesigned\n").unwrap();
        assert!(matches!(assess("DD OUT", Some(base), Some(head), &whole).1, Assessment::Declared));
    }
}
