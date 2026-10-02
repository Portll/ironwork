//! `ironwork fuzz`: runs a batch program many times on generated input files and SYSIN, and keeps
//! each distinct abend with the smallest input that still causes it, the evidence journal of a run on
//! that input and its coverage, in the directory cobolwork's abend set reads (docs/evidence.md §5).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

use exec::evidence::{Value, canonical};
use rt::storage::Kind;
use rt::vocab::{AcceptFrom, OpenMode, SignPosition};
use syntax::ast::{Organization, Stmt};

pub struct Request {
    pub program: PathBuf,
    pub out: PathBuf,
    pub root: PathBuf,
    pub runs: u32,
    pub seed: u64,
    pub timeout: Duration,
    pub libraries: Vec<PathBuf>,
    pub program_dirs: Vec<PathBuf>,
    pub flags: Vec<String>,
    pub clock: String,
}

/// An elementary item of a record, by its offset in the record.
#[derive(Clone, Copy)]
struct Field {
    offset: usize,
    size: usize,
    kind: Kind,
}

/// A sequential or indexed file of fixed-length records the program reads: its DD, its records'
/// fields, one list per level-01 record, and an indexed file's RECORD KEY by offset and length.
struct Feed {
    dd: String,
    length: usize,
    layouts: Vec<Vec<Field>>,
    key: Option<(usize, usize)>,
}

/// What one run is given: each fed DD's records, and SYSIN's lines.
#[derive(Clone, Default)]
struct Inputs {
    files: BTreeMap<String, Vec<Vec<u8>>>,
    sysin: Option<Vec<Vec<u8>>>,
}

#[derive(Clone, PartialEq, Eq)]
enum Outcome {
    Clean,
    Abend { code: String, file: String, line: i64, message: String },
    Timeout,
    Refused,
}

/// Abends that say what the run's surroundings lack, not what its input did: a construct ironwork
/// does not run, and a CALL of a program no library holds.
const NOT_THE_INPUT: &[&str] = &["IRONWORK", "S806"];

impl Outcome {
    fn place(&self) -> Option<(String, String, i64)> {
        match self {
            Outcome::Abend { code, file, line, .. } if !NOT_THE_INPUT.contains(&code.as_str()) => Some((code.clone(), file.clone(), *line)),
            _ => None,
        }
    }
}

/// xorshift64*: the same seed gives the same inputs on every platform.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

const SPACE: u8 = 0x40;
const ASTERISK: u8 = 0x5C;
const PLUS: u8 = 0x4E;
const MINUS: u8 = 0x60;
const TEXT: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 ";

fn ebcdic_text(rng: &mut Rng, size: usize) -> Vec<u8> {
    let page = zarch::ebcdic::CodePage::by_ccsid(37).expect("CCSID 37");
    (0..size).map(|_| page.encode_char(char::from(TEXT[rng.below(TEXT.len())])).unwrap_or(SPACE)).collect()
}

fn zoned_digits(rng: &mut Rng, n: usize, nines: bool) -> Vec<u8> {
    (0..n).map(|_| 0xF0 | if nines { 9 } else { rng.below(10) as u8 }).collect()
}

/// A value for one field: mostly what its PICTURE allows, sometimes the boundary or the bytes that
/// break it (spaces or asterisks in a number, an invalid packed sign).
fn field_bytes(rng: &mut Rng, f: Field) -> Vec<u8> {
    let class = rng.below(10);
    match f.kind {
        Kind::Zoned { signed, sign, .. } => {
            let separate = sign.is_some_and(|s| s.separate);
            let leading = sign.is_some_and(|s| s.position == SignPosition::Leading);
            let digits = if separate { f.size - 1 } else { f.size };
            let mut d = match class {
                0..=5 => zoned_digits(rng, digits, false),
                6 => zoned_digits(rng, digits, true),
                7 => vec![SPACE; digits],
                8 => vec![ASTERISK; digits],
                _ => (0..digits).map(|_| rng.next() as u8).collect(),
            };
            if signed && class <= 6 {
                let negative = rng.below(2) == 1;
                if separate {
                    let s = if negative { MINUS } else { PLUS };
                    if leading { d.insert(0, s) } else { d.push(s) }
                    return d;
                }
                let at = if leading { 0 } else { digits - 1 };
                d[at] = (d[at] & 0x0F) | if negative { 0xD0 } else { 0xC0 };
            } else if separate {
                if leading { d.insert(0, SPACE) } else { d.push(SPACE) }
            }
            d
        }
        Kind::Packed { signed, .. } => match class {
            0..=6 => {
                let nibbles = 2 * f.size - 1;
                let mut n: Vec<u8> = (0..nibbles).map(|_| if class == 6 { 9 } else { rng.below(10) as u8 }).collect();
                n.push(if !signed { 0x0F } else if rng.below(2) == 1 { 0x0D } else { 0x0C });
                n.chunks(2).map(|p| (p[0] << 4) | p[1]).collect()
            }
            7 | 8 => vec![SPACE; f.size],
            _ => (0..f.size).map(|_| rng.next() as u8).collect(),
        },
        Kind::Binary { .. } => match class {
            0..=3 => vec![0; f.size],
            4 | 5 => vec![0xFF; f.size],
            _ => (0..f.size).map(|_| rng.next() as u8).collect(),
        },
        Kind::Alnum { .. } | Kind::AlnumEdited { .. } | Kind::NumericEdited { .. } => match class {
            0..=5 => ebcdic_text(rng, f.size),
            6 | 7 => vec![SPACE; f.size],
            8 => vec![0xFF; f.size],
            _ => zoned_digits(rng, f.size, false),
        },
        _ => vec![0; f.size],
    }
}

/// A field's value that breaks nothing, which minimizing puts back wherever the abend still comes.
fn neutral(f: Field) -> Vec<u8> {
    match f.kind {
        Kind::Zoned { sign, .. } => {
            let mut d = vec![0xF0; f.size];
            if let Some(s) = sign.filter(|s| s.separate) {
                d[if s.position == SignPosition::Leading { 0 } else { f.size - 1 }] = PLUS;
            }
            d
        }
        Kind::Packed { .. } => {
            let mut d = vec![0; f.size];
            d[f.size - 1] = 0x0F;
            d
        }
        Kind::Binary { .. } | Kind::Float(_) | Kind::Pointer | Kind::Index | Kind::ObjectReference | Kind::ProgramPointer => vec![0; f.size],
        _ => vec![SPACE; f.size],
    }
}

fn record(rng: &mut Rng, feed: &Feed) -> Vec<u8> {
    let mut r = vec![SPACE; feed.length];
    if let Some(fields) = (!feed.layouts.is_empty()).then(|| &feed.layouts[rng.below(feed.layouts.len())]) {
        for &f in fields {
            let bytes = field_bytes(rng, f);
            r[f.offset..f.offset + f.size].copy_from_slice(&bytes[..f.size]);
        }
    }
    r
}

fn sysin_line(rng: &mut Rng) -> Vec<u8> {
    let length = [80, 80, 10, 1, 0][rng.below(5)];
    let pick: &[u8] = match rng.below(4) {
        0 => b"0123456789",
        1 => b"ABCDEFGHIJKLMNOPQRSTUVWXYZ ",
        2 => b"*-+. ,/0123456789",
        _ => TEXT,
    };
    (0..length).map(|_| pick[rng.below(pick.len())]).collect()
}

fn sysin_text(lines: &[Vec<u8>]) -> Vec<u8> {
    lines.iter().flat_map(|l| l.iter().copied().chain(*b"\n")).collect()
}

fn generate(rng: &mut Rng, feeds: &[Feed], sysin: bool) -> Inputs {
    let mut files = BTreeMap::new();
    for f in feeds {
        let mut records: Vec<Vec<u8>> = (0..1 + rng.below(4)).map(|_| record(rng, f)).collect();
        // An indexed file's data set holds its records in key order, one to a key, as REPRO unloads it.
        if let Some((at, len)) = f.key {
            records.sort_by(|a, b| a[at..at + len].cmp(&b[at..at + len]));
            records.dedup_by(|a, b| a[at..at + len] == b[at..at + len]);
        }
        files.insert(f.dd.clone(), records);
    }
    let sysin = sysin.then(|| (0..1 + rng.below(3)).map(|_| sysin_line(rng)).collect());
    Inputs { files, sysin }
}

/// Every statement of the program, nested ones included.
fn statements<'a>(list: &'a [Stmt], out: &mut Vec<&'a Stmt>) {
    for s in list {
        out.push(s);
        for body in exec::oo::bodies(s) {
            statements(body, out);
        }
    }
}

/// The DDs of a program's files other than those fed: those it reads that cannot be fed, which get
/// an empty data set, and those it only writes, which get a new one.
#[derive(Default)]
struct Others {
    unfed: Vec<String>,
    written: Vec<String>,
}

/// The files the program reads and whether it ACCEPTs from SYSIN, and the DDs of its other files.
fn inputs_of(compiled: &exec::Compiled) -> (Vec<Feed>, bool, Others) {
    let program = &compiled.program;
    let mut all = Vec::new();
    for p in &program.paragraphs {
        statements(&p.statements, &mut all);
    }
    let read: Vec<String> = all
        .iter()
        .filter_map(|s| if let Stmt::Open { files, .. } = s { Some(files) } else { None })
        .flatten()
        .filter(|(mode, _)| matches!(mode, OpenMode::Input | OpenMode::InputOutput))
        .map(|(_, name)| name.to_ascii_uppercase())
        .collect();
    let sysin = all.iter().any(|s| matches!(s, Stmt::Accept { from: AcceptFrom::Sysin, .. }));
    let layout = &compiled.layout;
    let (mut feeds, mut others) = (Vec::new(), Others::default());
    for (index, file) in program.files.iter().enumerate() {
        if file.sort {
            continue;
        }
        if !read.contains(&file.name.to_ascii_uppercase()) {
            others.written.push(file.assign.clone());
            continue;
        }
        let fixed = layout.record_lengths.get(index).copied().flatten().filter(|(lo, hi)| lo == hi && !file.record_varying);
        let key = match (file.organization, &file.record_key) {
            (Organization::Sequential, _) => Some(None),
            (Organization::Indexed, Some(r)) => match layout.resolve(&r.name, &r.qualifiers, r.pos) {
                Ok(exec::layout::Resolved::Item(i)) => Some(Some((layout.items[i].offset, layout.items[i].size as usize))),
                _ => None,
            },
            _ => None,
        };
        let (Some((_, length)), Some(key), Some(&(area, _))) = (fixed, key, layout.file_areas.get(index)) else {
            others.unfed.push(file.assign.clone());
            continue;
        };
        let key = key.map(|(offset, size)| ((offset - area) as usize, size));
        let layouts = layout
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.level == 1 && item.file == Some(index as u16))
            .map(|(i, _)| elementary(layout, i, area))
            .collect();
        feeds.push(Feed { dd: file.assign.clone(), length: length as usize, layouts, key });
    }
    (feeds, sysin, others)
}

/// The elementary items under a record, each occurrence of a table its own field, by offset in the record.
fn elementary(layout: &exec::layout::Layout, root: usize, area: u32) -> Vec<Field> {
    const OCCURRENCES: u32 = 64;
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(i) = stack.pop() {
        let item = &layout.items[i];
        if i != root && item.redefines.is_some() {
            continue;
        }
        if !item.children.is_empty() {
            stack.extend(item.children.iter().copied());
            continue;
        }
        if item.kind == Kind::Group || item.size == 0 {
            continue;
        }
        let mut offsets = vec![item.offset - area];
        for &(stride, count) in &item.dims {
            offsets = offsets.iter().flat_map(|&o| (0..count.min(OCCURRENCES)).map(move |k| o + k * stride)).collect();
        }
        out.extend(offsets.into_iter().map(|offset| Field { offset: offset as usize, size: item.size as usize, kind: item.kind }));
    }
    out
}

/// One run as its own process, so a run that loops is stopped and one that fails stops nothing else.
struct Runner<'a> {
    req: &'a Request,
    work: PathBuf,
    count: u64,
}

impl Runner<'_> {
    fn run(&mut self, inputs: &Inputs, others: &Others, evidence: Option<(&Path, &Path)>) -> std::io::Result<Outcome> {
        self.count += 1;
        let dir = self.work.join(format!("run-{}", self.count));
        fs::create_dir_all(&dir)?;
        let mut command = Command::new(std::env::current_exe()?);
        command.arg("run").arg(&self.req.program).arg("--clock").arg(&self.req.clock);
        command.args(&self.req.flags);
        for d in &self.req.libraries {
            command.arg("-I").arg(d);
        }
        for d in &self.req.program_dirs {
            command.arg("-L").arg(d);
        }
        for (dd, records) in &inputs.files {
            let path = dir.join(dd);
            fs::write(&path, records.concat())?;
            command.arg("--dd").arg(format!("{dd}={}", path.display()));
        }
        for dd in &others.unfed {
            let path = dir.join(dd);
            fs::write(&path, b"")?;
            command.arg("--dd").arg(format!("{dd}={}", path.display()));
        }
        for dd in &others.written {
            command.arg("--dd").arg(format!("{dd}={}", dir.join(dd).display()));
        }
        if let Some(lines) = &inputs.sysin {
            let path = dir.join("SYSIN");
            fs::write(&path, sysin_text(lines))?;
            command.arg("--dd").arg(format!("SYSIN={}", path.display()));
        }
        if let Some((journal, coverage)) = evidence {
            command.arg("--evidence").arg(journal).arg("--coverage").arg(coverage);
        }
        let stderr = fs::File::create(dir.join("stderr"))?;
        command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(stderr);
        let mut child = command.spawn()?;
        let began = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break Some(status);
            }
            if began.elapsed() > self.req.timeout {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        let text = fs::read_to_string(dir.join("stderr")).unwrap_or_default();
        let _ = fs::remove_dir_all(&dir);
        // A program's RETURN-CODE can be any exit status, so an abend is told by the line that reports it.
        Ok(match status {
            None => Outcome::Timeout,
            Some(s) => match text.lines().rev().find_map(|l| self.abend(l)) {
                Some(abend) => abend,
                None if s.code() == Some(2) || s.code().is_none() => Outcome::Refused,
                None => Outcome::Clean,
            },
        })
    }

    /// `file:line:col: ABEND code: message`, the place as the run's journal records it.
    fn abend(&self, line: &str) -> Option<Outcome> {
        let (place, rest) = line.split_once(": ABEND ")?;
        let (code, message) = rest.split_once(": ").unwrap_or((rest, ""));
        let mut parts = place.rsplitn(3, ':');
        let (_col, number, file) = (parts.next()?, parts.next()?.parse().ok()?, parts.next()?);
        Some(Outcome::Abend { code: code.to_string(), file: self.relative(file), line: number, message: message.to_string() })
    }

    fn relative(&self, file: &str) -> String {
        let own = self.req.program.parent().map(Path::to_path_buf).unwrap_or_default();
        let roots: Vec<PathBuf> = std::iter::once(own).chain(self.req.libraries.iter().cloned()).chain(self.req.program_dirs.iter().cloned()).collect();
        crate::evidence::relative(Path::new(file), &roots)
    }
}

/// The smallest input found that still ends at the same abend: records and lines dropped, then each
/// field put back to a value that breaks nothing.
fn minimize(runner: &mut Runner, feeds: &[Feed], others: &Others, mut inputs: Inputs, place: &(String, String, i64), budget: u32) -> Inputs {
    let mut left = budget;
    let mut holds = |runner: &mut Runner, candidate: &Inputs| -> bool {
        if left == 0 {
            return false;
        }
        left -= 1;
        runner.run(candidate, others, None).ok().and_then(|o| o.place()).as_ref() == Some(place)
    };
    for dd in inputs.files.keys().cloned().collect::<Vec<_>>() {
        let mut k = inputs.files[&dd].len();
        while k > 0 {
            k -= 1;
            let mut candidate = inputs.clone();
            if let Some(records) = candidate.files.get_mut(&dd) {
                records.remove(k);
            }
            if holds(runner, &candidate) {
                inputs = candidate;
            }
        }
    }
    if let Some(lines) = inputs.sysin.clone() {
        for k in (0..lines.len()).rev() {
            let mut candidate = inputs.clone();
            if let Some(l) = candidate.sysin.as_mut() {
                l.remove(k);
            }
            if holds(runner, &candidate) {
                inputs = candidate;
            }
        }
    }
    for feed in feeds {
        for r in 0..inputs.files.get(&feed.dd).map_or(0, Vec::len) {
            for &f in feed.layouts.iter().flatten().filter(|f| feed.key.is_none_or(|(at, len)| f.offset + f.size <= at || at + len <= f.offset)) {
                let current = &inputs.files[&feed.dd][r][f.offset..f.offset + f.size];
                let quiet = neutral(f);
                if current == quiet.as_slice() {
                    continue;
                }
                let mut candidate = inputs.clone();
                candidate.files.get_mut(&feed.dd).expect("fed")[r][f.offset..f.offset + f.size].copy_from_slice(&quiet);
                if holds(runner, &candidate) {
                    inputs = candidate;
                }
            }
        }
    }
    inputs
}

const BASE64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            out.push(if i <= chunk.len() { char::from(BASE64[((n >> shift) & 63) as usize]) } else { '=' });
        }
    }
    out
}

fn obj(pairs: Vec<(&str, Value)>) -> Value {
    Value::Obj(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn compile(req: &Request) -> Result<exec::Compiled, String> {
    let path = req.program.display().to_string();
    let bytes = fs::read(&req.program).map_err(|e| format!("{path}: {e}"))?;
    let text = syntax::copy::decode(&bytes);
    let own = req.program.parent().map(Path::to_path_buf).unwrap_or_default();
    let libraries = syntax::copy::Libraries::new(std::iter::once(own).chain(req.libraries.iter().cloned()).collect()).with_program(&req.program);
    let mut programs = syntax::parse_all_with(&text, &libraries).map_err(|e| e.place(&path))?;
    exec::compile(programs.remove(0), &req.flags).map_err(|messages| messages.iter().map(|m| m.place(&path)).collect::<Vec<_>>().join("\n"))
}

pub fn run(req: Request) -> ExitCode {
    let fail = |message: String| {
        eprintln!("ironwork fuzz: {message}");
        ExitCode::from(2)
    };
    let absolute = |p: &Path| std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let Ok(file) = absolute(&req.program).strip_prefix(absolute(&req.root)).map(|p| p.to_string_lossy().replace('\\', "/")) else {
        return fail(format!("{} is not under --root {}", req.program.display(), req.root.display()));
    };
    let compiled = match compile(&req) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(12);
        }
    };
    if !compiled.program.using.is_empty() {
        return fail(format!("{} takes PROCEDURE DIVISION USING parameters: fuzz runs a main program", compiled.program.id));
    }
    let (feeds, sysin, others) = inputs_of(&compiled);
    if feeds.is_empty() && !sysin {
        return fail(format!("{} reads no sequential or indexed file of fixed-length records and no SYSIN, so there is nothing to vary", compiled.program.id));
    }
    let evidence = req.out.join("evidence");
    let coverage = req.out.join("coverage");
    if req.out.exists() && fs::read_dir(&req.out).map(|mut d| d.next().is_some()).unwrap_or(true) {
        return fail(format!("-o {} is not empty: each fuzz run gets a directory of its own", req.out.display()));
    }
    if let Err(e) = fs::create_dir_all(&coverage) {
        return fail(format!("-o {}: {e}", req.out.display()));
    }
    let mut runner = Runner { req: &req, work: req.out.join(".work"), count: 0 };

    // What the program does on empty input is no input's doing, so an abend it gives then is not kept.
    let empty = Inputs { files: feeds.iter().map(|f| (f.dd.clone(), Vec::new())).collect(), sysin: sysin.then(Vec::new) };
    let baseline = match runner.run(&empty, &others, None) {
        Ok(o) => o.place(),
        Err(e) => return fail(format!("a run could not start: {e}")),
    };
    let mut rng = Rng(req.seed.max(1));
    let mut counts: BTreeMap<&str, i64> = [("runs", 0), ("clean", 0), ("abend", 0), ("timeout", 0), ("refused", 0)].into_iter().collect();
    let mut kept: Vec<((String, String, i64), Inputs)> = Vec::new();
    for _ in 0..req.runs {
        let inputs = generate(&mut rng, &feeds, sysin);
        let outcome = match runner.run(&inputs, &others, None) {
            Ok(o) => o,
            Err(e) => return fail(format!("a run could not start: {e}")),
        };
        *counts.get_mut("runs").expect("counted") += 1;
        let tally = match &outcome {
            Outcome::Clean => "clean",
            Outcome::Timeout => "timeout",
            Outcome::Refused => "refused",
            Outcome::Abend { code, .. } if NOT_THE_INPUT.contains(&code.as_str()) => "refused",
            Outcome::Abend { .. } => "abend",
        };
        *counts.get_mut(tally).expect("counted") += 1;
        if let Some(place) = outcome.place()
            && Some(&place) != baseline.as_ref()
            && !kept.iter().any(|(p, _)| *p == place)
        {
            kept.push((place, inputs));
        }
    }

    let (mut inputs_out, mut runs_out) = (Vec::new(), Vec::new());
    for (n, (place, found)) in kept.into_iter().enumerate() {
        let small = minimize(&mut runner, &feeds, &others, found, &place, 200);
        let before = journals(&evidence);
        let cover = coverage.join(format!("{n}.json"));
        let outcome = match runner.run(&small, &others, Some((&evidence, &cover))) {
            Ok(o) => o,
            Err(e) => return fail(format!("a run could not start: {e}")),
        };
        let journal = journals(&evidence).into_iter().find(|j| !before.contains(j));
        let (Some(journal), Outcome::Abend { code, file, line, message }) = (journal, outcome.clone()) else { continue };
        if outcome.place().as_ref() != Some(&place) {
            eprintln!("ironwork fuzz: {} at {}:{} did not come again on its smallest input, so it is not kept", place.0, place.1, place.2);
            continue;
        }
        let mut ids = Vec::new();
        for (dd, records) in &small.files {
            let id = format!("r{n}-{dd}");
            inputs_out.push(obj(vec![("id", id.as_str().into()), ("kind", "dd".into()), ("name", dd.as_str().into()), ("bytes", base64(&records.concat()).into()), ("minimized", true.into())]));
            ids.push(Value::from(id));
        }
        if let Some(lines) = &small.sysin {
            let id = format!("r{n}-SYSIN");
            inputs_out.push(obj(vec![("id", id.as_str().into()), ("kind", "sysin".into()), ("name", "SYSIN".into()), ("bytes", base64(&sysin_text(lines)).into()), ("minimized", true.into())]));
            ids.push(Value::from(id));
        }
        runs_out.push(obj(vec![
            ("input", Value::Arr(ids)),
            ("outcome", "abend".into()),
            ("abend", obj(vec![("code", code.into()), ("file", file.into()), ("line", line.into()), ("message", message.into())])),
            ("journal", journal.into()),
            ("coverage", format!("coverage/{n}.json").into()),
        ]));
    }
    let _ = fs::remove_dir_all(&runner.work);

    let kept = runs_out.len();
    let manifest = obj(vec![
        ("tool", "ironwork-fuzz".into()),
        ("version", env!("CARGO_PKG_VERSION").into()),
        ("seed", Value::from(req.seed)),
        ("strategy", "fields".into()),
        ("clock", req.clock.as_str().into()),
        ("program", obj(vec![("file", file.into()), ("id", compiled.program.id.as_str().into())])),
        ("entry", "run".into()),
        ("inputs", Value::Arr(inputs_out)),
        ("counts", Value::Obj(counts.iter().map(|(k, v)| (k.to_string(), Value::Int(*v))).collect())),
        ("runs", Value::Arr(runs_out)),
    ]);
    if let Err(e) = fs::write(req.out.join("manifest.json"), format!("{}\n", canonical(&manifest))) {
        return fail(format!("-o {}: {e}", req.out.display()));
    }
    if !others.unfed.is_empty() {
        eprintln!("ironwork fuzz: not varied, given empty: {}", others.unfed.join(", "));
    }
    if let Some((code, file, line)) = baseline {
        eprintln!("ironwork fuzz: the program ends with {code} at {file}:{line} on empty input; that abend is not kept");
    }
    println!("ironwork fuzz: {} runs, {} clean, {} abend ({kept} kept), {} timeout, {} refused: {}", counts["runs"], counts["clean"], counts["abend"], counts["timeout"], counts["refused"], req.out.display());
    ExitCode::SUCCESS
}

/// The run journals an evidence directory holds, by run id.
fn journals(evidence: &Path) -> Vec<String> {
    fs::read_dir(evidence.join("runs"))
        .map(|d| d.filter_map(|e| e.ok()?.file_name().to_str()?.strip_suffix(".jsonl").map(str::to_string)).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_pads_as_rfc_4648_does() {
        assert_eq!([base64(b""), base64(b"f"), base64(b"fo"), base64(b"foo"), base64(b"foob")], ["", "Zg==", "Zm8=", "Zm9v", "Zm9vYg=="]);
    }

    #[test]
    fn a_seed_gives_the_same_values_every_time() {
        let field = Field { offset: 0, size: 5, kind: Kind::Zoned { digits: 5, scale: 0, signed: false, sign: None } };
        let draw = |seed| {
            let mut rng = Rng(seed);
            (0..20).map(|_| field_bytes(&mut rng, field)).collect::<Vec<_>>()
        };
        assert_eq!(draw(7), draw(7));
        assert_ne!(draw(7), draw(8));
    }
}
