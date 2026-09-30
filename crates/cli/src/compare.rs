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

use exec::digest::{hex, sha256};
use exec::evidence::{canonical, fields, Value};

pub const PREDICATE: &str = "https://github.com/Portll/ironwork/blob/main/docs/evidence.md#equivalence-v1";
const LIMIT: &str = "Equivalence is under ironwork's model of Enterprise COBOL, on the inputs recorded here; the oracle holds no Enterprise COBOL goldens yet, and paragraph coverage is not measured (coverage null), so a verdict of equivalent covers these inputs only.";

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
struct Declared {
    what: String,
    lines: Option<(usize, usize)>,
    reason: String,
}

fn parse_declared(text: &str) -> Result<Vec<Declared>, String> {
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
            "DD" => {
                let mut w = rest.splitn(2, char::is_whitespace);
                (format!("DD {}", w.next().unwrap_or_default().to_ascii_uppercase()), w.next().unwrap_or("").trim())
            }
            "DISPLAY" | "RETURN-CODE" => (head.clone(), rest),
            _ => return Err(format!("line {}: a declaration starts DD, DISPLAY or RETURN-CODE", n + 1)),
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
}

fn scratch(side: &str) -> std::io::Result<PathBuf> {
    let nonce = hex(&sha256(format!("{:?}{}{side}", std::time::SystemTime::now(), std::process::id()).as_bytes()))[..12].to_string();
    let dir = std::env::temp_dir().join(format!("ironwork-compare-{side}-{nonce}"));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Runs one program with every DD pointed at a copy in `dir`.
fn run_side(program: &Path, req: &Request, specs: &[Spec], dir: &Path) -> Outcome {
    let mut outcome = Outcome { closure: Vec::new(), return_code: None, abend: None, display: Vec::new(), files: BTreeMap::new(), error: None };
    let fail = |mut o: Outcome, e: String| {
        o.error = Some(e);
        o
    };
    let mut local = Vec::new();
    for s in specs {
        let copy = dir.join(&s.name);
        if s.path.exists()
            && let Err(e) = fs::copy(&s.path, &copy)
        {
            return fail(outcome, format!("copying DD {}: {e}", s.name));
        }
        local.push(format!("{}={}{}", s.name, copy.display(), s.suffix));
    }
    let text = match fs::read(program) {
        Ok(b) => syntax::copy::decode(&b),
        Err(e) => return fail(outcome, format!("{}: {e}", program.display())),
    };
    let own = program.parent().map(Path::to_path_buf).unwrap_or_default();
    let libraries = syntax::copy::Libraries::new(std::iter::once(own.clone()).chain(req.libraries.iter().cloned()).collect()).with_program(program);
    let mut programs = match syntax::parse_all_with(&text, &libraries) {
        Ok(p) => p,
        Err(e) => return fail(outcome, e.place(&program.display().to_string()).to_string()),
    };
    let first = programs.remove(0);
    let roots: Vec<PathBuf> = std::iter::once(own.clone()).chain(req.libraries.iter().cloned()).collect();
    for s in std::iter::once(program.display().to_string()).chain(first.sources.iter().filter(|s| !s.is_empty() && !s.starts_with('(')).cloned()) {
        let path = PathBuf::from(&s);
        if let Ok(bytes) = fs::read(&path) {
            let name = roots.iter().find_map(|r| path.strip_prefix(r).ok()).map_or_else(|| path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), |p| p.to_string_lossy().replace('\\', "/"));
            if !outcome.closure.iter().any(|(n, _)| *n == name) {
                outcome.closure.push((name, hex(&sha256(&bytes))));
            }
        }
    }
    let library = exec::unit::Library { programs, dirs: std::iter::once(own).chain(req.program_dirs.iter().cloned()).collect(), copy: libraries, flags: req.flags.clone() };
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
    let ended = compiled.execute_with(library, dds, Some(sysin), req.clock, database, &mut outcome.display, &mut err);
    match ended {
        Ok((_, rc)) => outcome.return_code = Some(i64::from(rc)),
        Err(a) => outcome.abend = Some((a.code.to_string(), a.message.clone())),
    }
    for s in specs {
        outcome.files.insert(s.name.clone(), fs::read(dir.join(&s.name)).ok());
    }
    outcome
}

/// The first line (1-based) and byte offset where two outputs differ, or None when they do not.
fn first_difference(a: &[u8], b: &[u8]) -> Option<(usize, usize)> {
    let at = a.iter().zip(b).position(|(x, y)| x != y).or((a.len() != b.len()).then(|| a.len().min(b.len())))?;
    let line = a[..at.min(a.len())].iter().filter(|&&c| c == b'\n').count() + 1;
    Some((line, at))
}

fn digest_of(bytes: Option<&[u8]>) -> Value {
    bytes.map_or(Value::Null, |b| hex(&sha256(b)).into())
}

fn subject(name: &str, path: &Path) -> Value {
    let digest = fs::read(path).map(|b| hex(&sha256(&b))).unwrap_or_default();
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
    let mut dirs = Vec::new();
    let mut side = |name: &str, program: &Path| -> Result<Outcome, String> {
        let dir = scratch(name).map_err(|e| e.to_string())?;
        dirs.push(dir.clone());
        Ok(run_side(program, &req, &specs, &dir))
    };
    let head = match side("head", &req.head) {
        Ok(o) => o,
        Err(e) => return crate::usage_error(&e),
    };
    let base = match req.base.as_deref().map(|b| side("base", b)).transpose() {
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
    let is_declared = |what: &str, line: Option<usize>| declared.iter().find(|d| d.what == what && d.lines.is_none_or(|(a, b)| line.is_some_and(|l| (a..=b).contains(&l))));
    let mut compare = |what: String, a: Option<&[u8]>, b: Option<&[u8]>, results: &mut Vec<Value>| {
        let diff = match (a, b) {
            (Some(a), Some(b)) => first_difference(a, b),
            (None, None) => None,
            _ => Some((1, 0)),
        };
        let mut r = fields([("what", what.clone().into()), ("same", diff.is_none().into()), ("expected", digest_of(a)), ("actual", digest_of(b))]);
        if let Some((line, offset)) = diff {
            r.insert("firstDifference".into(), Value::Obj(fields([("line", Value::Int(line as i64)), ("offset", Value::Int(offset as i64))])));
            match is_declared(&what, Some(line)) {
                Some(d) => {
                    declared_hit += 1;
                    r.insert("declared".into(), d.reason.clone().into());
                }
                None => undeclared += 1,
            }
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
    let expected: BTreeMap<String, Option<Vec<u8>>> = req.expected.iter().map(|(n, p)| (n.to_ascii_uppercase(), fs::read(p).ok())).collect();
    for s in &specs {
        if let Some(want) = expected.get(&s.name) {
            compare(format!("DD {}", s.name), want.as_deref(), head.files.get(&s.name).and_then(|f| f.as_deref()), &mut results);
        } else if let Some(base) = &base {
            compare(format!("DD {}", s.name), base.files.get(&s.name).and_then(|f| f.as_deref()), head.files.get(&s.name).and_then(|f| f.as_deref()), &mut results);
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
    let inputs = Value::Arr(specs.iter().filter(|s| s.path.exists()).map(|s| Value::Obj(fields([("dd", s.name.clone().into()), ("sha256", digest_of(fs::read(&s.path).ok().as_deref()))]))).collect());
    let mut subjects = vec![subject("head", &req.head)];
    if let Some(b) = &req.base {
        subjects.insert(0, subject("base", b));
    }
    let predicate = fields([
        ("verdict", verdict.into()),
        ("inputs", inputs),
        ("sqlRecording", req.replay.as_ref().map_or(Value::Null, |p| digest_of(fs::read(p).ok().as_deref()))),
        ("results", Value::Arr(results)),
        ("declared", Value::Arr(declared.iter().map(|d| Value::Obj(fields([("what", d.what.clone().into()), ("reason", d.reason.clone().into())]))).collect())),
        ("inconclusive", Value::Arr(inconclusive.iter().map(|s| Value::Str(s.clone())).collect())),
        ("closure", Value::Obj({
            let side = |o: &Outcome| Value::Arr(o.closure.iter().map(|(n, d)| Value::Obj(fields([("name", n.clone().into()), ("sha256", d.clone().into())]))).collect());
            let mut m = BTreeMap::new();
            m.insert("head".to_string(), side(&head));
            if let Some(b) = &base {
                m.insert("base".to_string(), side(b));
            }
            m
        })),
        ("coverage", Value::Null),
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
    fn the_first_difference_is_located_by_line_and_offset() {
        assert_eq!(first_difference(b"a\nbc\n", b"a\nbd\n"), Some((2, 3)));
        assert_eq!(first_difference(b"ab", b"abc"), Some((1, 2)));
        assert_eq!(first_difference(b"same", b"same"), None);
    }
}
