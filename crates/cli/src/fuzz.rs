//! `ironwork fuzz`: runs a batch program many times on generated input files and SYSIN, and keeps
//! each distinct abend with the smallest input that still causes it, the evidence journal of a run on
//! that input and its coverage, in the directory cobolwork's abend set reads (docs/evidence.md §5).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use exec::evidence::{Value, canonical};
use rt::storage::Kind;
use rt::vocab::{AcceptFrom, OpenMode, SignPosition};
use syntax::ast::{FileDecl, Organization, Paragraph, Program, Ref, Stmt};
use zarch::ebcdic::CodePage;

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

impl Request {
    /// The directories a run reads: the program's own, then each `-I` and `-L` library.
    fn roots(&self) -> Vec<PathBuf> {
        let own = self.program.parent().map(Path::to_path_buf).unwrap_or_default();
        std::iter::once(own).chain(self.libraries.iter().cloned()).chain(self.program_dirs.iter().cloned()).collect()
    }
}

/// An elementary item of a record, by its offset in the record.
#[derive(Clone, Copy)]
pub(crate) struct Field {
    pub(crate) offset: usize,
    pub(crate) size: usize,
    pub(crate) kind: Kind,
}

/// A level-01 record of a file: its length and its elementary items.
struct Shape {
    size: usize,
    fields: Vec<Field>,
}

/// A sequential, indexed or relative file the program reads: its DD, the longest record, the
/// shortest and longest a READ takes when its records have more than one length (each behind an
/// RDW in the data set), its level-01 records, an indexed file's keys by offset and length (the
/// RECORD KEY first, then each ALTERNATE RECORD KEY that allows no duplicates), and whether it is
/// relative, whose data set holds a record per slot.
struct Feed {
    dd: String,
    length: usize,
    variable: Option<(usize, usize)>,
    shapes: Vec<Shape>,
    keys: Vec<(usize, usize)>,
    relative: bool,
}

/// What one run is given: each fed DD's records, and SYSIN's lines.
#[derive(Clone, Default)]
struct Inputs {
    files: BTreeMap<String, Vec<Vec<u8>>>,
    sysin: Option<Vec<Vec<u8>>>,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    Clean,
    Abend { code: String, file: String, line: i64, message: String },
    Timeout,
    /// The run stopped for a reason of its surroundings, with the last line it said.
    Refused(String),
    /// ironwork itself failed, with where and why it panicked.
    Crash(String),
}

/// Abends that say what the run's surroundings lack, not what its input did: a construct ironwork
/// does not run, and a CALL of a program no library holds.
const NOT_THE_INPUT: &[&str] = &["IRONWORK", "S806"];

impl Outcome {
    pub(crate) fn place(&self) -> Option<(String, String, i64)> {
        match self {
            Outcome::Abend { code, file, line, .. } if !NOT_THE_INPUT.contains(&code.as_str()) => Some((code.clone(), file.clone(), *line)),
            _ => None,
        }
    }

    pub(crate) fn told(&self) -> String {
        match self {
            Outcome::Clean => "ended normally".into(),
            Outcome::Abend { code, file, line, .. } => format!("ended with {code} at {file}:{line}"),
            Outcome::Timeout => "timed out".into(),
            Outcome::Refused(why) => format!("was refused: {why}"),
            Outcome::Crash(why) => format!("crashed ironwork: {why}"),
        }
    }
}

/// How a finished run ended, from its exit status and standard error. A program's RETURN-CODE can
/// be any exit status, 2 included, so an abend is told by the line that reports it, a crash by
/// Rust's panic line, and ironwork's own refusal by a line that ironwork leads.
pub(crate) fn ended(code: Option<i32>, text: &str, abend: impl Fn(&str) -> Option<Outcome>) -> Outcome {
    let mut lines = text.lines();
    if let Some(at) = lines.find_map(|l| l.strip_prefix("thread '").and_then(|l| l.split_once(" panicked at ")).map(|(_, at)| at)) {
        return Outcome::Crash(format!("{at} {}", lines.next().unwrap_or("")).trim_end().to_string());
    }
    if let Some(abend) = text.lines().rev().find_map(abend) {
        return abend;
    }
    let refusal = text.lines().find(|l| l.starts_with("ironwork: ")).filter(|_| code == Some(2));
    match (refusal, code) {
        (Some(line), _) => Outcome::Refused(line.to_string()),
        (None, None) => Outcome::Refused(text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").to_string()),
        (None, Some(_)) => Outcome::Clean,
    }
}

/// xorshift64*: the same seed gives the same inputs on every platform.
pub(crate) struct Rng(pub(crate) u64);

impl Rng {
    pub(crate) fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub(crate) fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

pub(crate) const SPACE: u8 = 0x40;
const ASTERISK: u8 = 0x5C;
const PLUS: u8 = 0x4E;
const MINUS: u8 = 0x60;
const TEXT: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 ";
static CP037: LazyLock<&'static CodePage> = LazyLock::new(|| CodePage::by_ccsid(37).expect("CCSID 37"));

fn ebcdic_text(rng: &mut Rng, size: usize) -> Vec<u8> {
    (0..size).map(|_| CP037.encode_char(char::from(TEXT[rng.below(TEXT.len())])).unwrap_or(SPACE)).collect()
}

/// UTF-16 text for a national item of `size` bytes, as Enterprise COBOL holds it.
fn national_text(rng: &mut Rng, size: usize) -> Vec<u8> {
    (0..size / 2).flat_map(|_| [0, TEXT[rng.below(TEXT.len())]]).collect()
}

fn zoned_digits(rng: &mut Rng, n: usize, nines: bool) -> Vec<u8> {
    (0..n).map(|_| 0xF0 | if nines { 9 } else { rng.below(10) as u8 }).collect()
}

/// A value for one field: mostly what its PICTURE allows, sometimes the boundary or the bytes that
/// break it (spaces or asterisks in a number, an invalid packed sign).
pub(crate) fn field_bytes(rng: &mut Rng, f: Field) -> Vec<u8> {
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
        Kind::Packed { digits, signed, .. } => match class {
            0..=6 => {
                // An even digit count leaves the first nibble a pad, which the PICTURE keeps zero.
                let nibbles = 2 * f.size - 1;
                let pad = nibbles.saturating_sub(digits as usize);
                let mut n: Vec<u8> = (0..nibbles).map(|i| if i < pad { 0 } else if class == 6 { 9 } else { rng.below(10) as u8 }).collect();
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
        Kind::National => match class {
            0..=5 => national_text(rng, f.size),
            6 | 7 => [0, 0x20].repeat(f.size / 2),
            8 => vec![0xFF; f.size],
            _ => (0..f.size).map(|_| rng.next() as u8).collect(),
        },
        _ => vec![0; f.size],
    }
}

/// A field's value that breaks nothing, which minimizing puts back wherever the abend still comes.
pub(crate) fn neutral(f: Field) -> Vec<u8> {
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
        Kind::National => [0, 0x20].repeat(f.size / 2),
        _ => vec![SPACE; f.size],
    }
}

/// One record of a feed. A variable-length record takes its level-01 record's length, or one READ
/// allows, or now and then one shorter than READ allows, a record length conflict; a relative
/// file's slot is sometimes left empty.
fn record(rng: &mut Rng, feed: &Feed) -> Vec<u8> {
    if feed.relative && rng.below(5) == 0 {
        return if feed.variable.is_some() { Vec::new() } else { vec![0; feed.length] };
    }
    let mut r = vec![SPACE; feed.length];
    let shape = (!feed.shapes.is_empty()).then(|| &feed.shapes[rng.below(feed.shapes.len())]);
    for &f in shape.map_or(&[][..], |s| &s.fields) {
        let bytes = field_bytes(rng, f);
        r[f.offset..f.offset + f.size].copy_from_slice(&bytes[..f.size]);
    }
    if let Some((shortest, longest)) = feed.variable {
        let length = match rng.below(10) {
            0..=4 => shape.map_or(longest, |s| s.size),
            5..=7 => shortest + rng.below(longest - shortest + 1),
            8 => shortest,
            _ => 1 + rng.below(shortest.max(1)),
        };
        let keys_end = feed.keys.iter().map(|&(at, len)| at + len).max().unwrap_or(1);
        r.truncate(length.clamp(keys_end, longest));
    }
    r
}

/// A fed DD's data set: its records one after another, or each behind the 4-byte RDW a
/// variable-length record carries.
fn data_set(records: &[Vec<u8>], rdw: bool) -> Vec<u8> {
    if !rdw {
        return records.concat();
    }
    records.iter().flat_map(|r| ((r.len() + 4) as u16).to_be_bytes().into_iter().chain([0, 0]).chain(r.iter().copied())).collect()
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
        // An indexed file's data set holds its records in key order, no two sharing a key that allows
        // no duplicates, as REPRO unloads it.
        if let Some(&(at, len)) = f.keys.first() {
            records.sort_by(|a, b| a[at..at + len].cmp(&b[at..at + len]));
        }
        for &(at, len) in &f.keys {
            let mut seen = BTreeSet::new();
            records.retain(|r| seen.insert(r[at..at + len].to_vec()));
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

/// The DDs of a program's files other than those fed: those it reads that cannot be fed, and a DD
/// more than one file names, which get an empty data set; those it only writes, which get a new
/// one; and names `--dd` cannot carry, which get none.
#[derive(Default)]
struct Others {
    unfed: Vec<String>,
    written: Vec<String>,
    ungiven: Vec<String>,
}

/// The files a program OPENs INPUT or I-O, by name in upper case, and whether it ACCEPTs from SYSIN.
fn reads(paragraphs: &[Paragraph]) -> (BTreeSet<String>, bool) {
    let mut all = Vec::new();
    for p in paragraphs {
        statements(&p.statements, &mut all);
    }
    let read = all
        .iter()
        .filter_map(|s| if let Stmt::Open { files, .. } = s { Some(files) } else { None })
        .flatten()
        .filter(|(mode, _)| matches!(mode, OpenMode::Input | OpenMode::InputOutput))
        .map(|(_, name)| name.to_ascii_uppercase())
        .collect();
    (read, all.iter().any(|s| matches!(s, Stmt::Accept { from: AcceptFrom::Sysin, .. })))
}

/// The programs the main program contains, at any depth.
fn contained<'a>(compiled: &exec::Compiled, rest: &'a [Program]) -> Vec<&'a Program> {
    let mut out: Vec<&Program> = Vec::new();
    let mut names: Vec<&String> = compiled.program.nested.iter().collect();
    while let Some(name) = names.pop() {
        if let Some(p) = rest.iter().find(|p| p.id == *name && !out.iter().any(|o| std::ptr::eq(*o, *p))) {
            names.extend(p.nested.iter());
            out.push(p);
        }
    }
    out
}

/// The files the program reads and whether it ACCEPTs from SYSIN, and the DDs of its other files.
/// The main program's files are fed; a contained program's files get their DDs, empty where it
/// reads them.
fn inputs_of(compiled: &exec::Compiled, rest: &[Program]) -> (Vec<Feed>, bool, Others) {
    let inner = contained(compiled, rest);
    let (read, mut sysin) = reads(&compiled.program.paragraphs);
    let mut files: Vec<(Option<usize>, &FileDecl, bool)> =
        compiled.program.files.iter().enumerate().filter(|(_, f)| !f.sort).map(|(k, f)| (Some(k), f, read.contains(&f.name.to_ascii_uppercase()))).collect();
    for p in inner {
        let (read, accepts) = reads(&p.paragraphs);
        sysin |= accepts;
        files.extend(p.files.iter().filter(|f| !f.sort).map(|f| (None, f, read.contains(&f.name.to_ascii_uppercase()))));
    }
    let (mut feeds, mut others) = (Vec::new(), Others::default());
    for &(index, file, read) in &files {
        let dd = &file.assign;
        // A file assigned to SYSIN in a program that ACCEPTs from SYSIN reads the same lines.
        if sysin && dd == "SYSIN" {
            continue;
        }
        if dd.contains('=') {
            others.ungiven.push(dd.clone());
            continue;
        }
        if !read {
            others.written.push(dd.clone());
            continue;
        }
        let shared = files.iter().filter(|(_, f, _)| f.assign == *dd).count() > 1;
        match index.and_then(|k| feed(compiled, k, file)).filter(|_| !shared) {
            Some(f) => feeds.push(f),
            None => others.unfed.push(dd.clone()),
        }
    }
    for list in [&mut others.unfed, &mut others.written, &mut others.ungiven] {
        list.sort();
        list.dedup();
    }
    others.written.retain(|dd| !others.unfed.contains(dd));
    (feeds, sysin, others)
}

/// A file the program reads as a feed: sequential, relative, or indexed with its keys inside every
/// record a READ takes.
fn feed(compiled: &exec::Compiled, index: usize, file: &FileDecl) -> Option<Feed> {
    let layout = &compiled.layout;
    let &(area, _) = layout.file_areas.get(index)?;
    let variable = exec::variable_records(file, layout, index);
    let (shortest, longest) = if variable {
        exec::read_lengths(file, layout, index, compiled.options.vlr)
    } else {
        layout.record_lengths.get(index).copied().flatten().filter(|(lo, hi)| lo == hi)?
    };
    let (shortest, length) = (shortest as usize, longest as usize);
    // An RDW's halfword holds the record's length and its own four bytes.
    if length == 0 || length + 4 > usize::from(u16::MAX) {
        return None;
    }
    let span = |r: &Ref| match layout.resolve(&r.name, &r.qualifiers, r.pos) {
        Ok(exec::layout::Resolved::Item(i)) => {
            let (at, len) = (layout.items[i].offset.checked_sub(area)? as usize, layout.items[i].size as usize);
            (at + len <= if variable { shortest } else { length }).then_some((at, len))
        }
        _ => None,
    };
    let keys = match (file.organization, &file.record_key) {
        (Organization::Sequential | Organization::Relative, _) => Vec::new(),
        (Organization::Indexed, Some(prime)) => std::iter::once(prime).chain(file.alternate_keys.iter().filter(|(_, duplicates)| !duplicates).map(|(r, _)| r)).map(span).collect::<Option<_>>()?,
        _ => return None,
    };
    let shapes = layout
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.level == 1 && item.file == Some(index as u16))
        .map(|(i, item)| Shape { size: (item.size as usize).min(length), fields: elementary(layout, i, area, length) })
        .collect();
    let variable = variable.then_some((shortest.min(length), length));
    Some(Feed { dd: file.assign.clone(), length, variable, shapes, keys, relative: file.organization == Organization::Relative })
}

/// The elementary items under an item at offset `base`, each occurrence of a table its own field,
/// by offset from `base`, those within `length` bytes. Elementary items do not overlap, so there
/// are no more fields than the item has bytes.
pub(crate) fn elementary(layout: &exec::layout::Layout, root: usize, base: u32, length: usize) -> Vec<Field> {
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
        let size = item.size as usize;
        let Some(start) = item.offset.checked_sub(base).filter(|_| item.kind != Kind::Group && size > 0) else {
            continue;
        };
        let mut offsets = vec![start as usize];
        for &(stride, count) in &item.dims {
            offsets = offsets.iter().flat_map(|&o| (0..count as usize).map(move |k| o + k * stride as usize).take_while(|&at| at + size <= length)).collect();
        }
        out.extend(offsets.into_iter().filter(|&at| at + size <= length).map(|offset| Field { offset, size, kind: item.kind }));
    }
    out
}

/// Runs `command`, its standard error kept in `dir`, stops it once it has run longer than
/// `timeout`, and tells how it ended, with each data set's path in what it said replaced by the DD
/// it was given for. `dir` is removed.
pub(crate) fn finish(mut command: Command, dir: &Path, timeout: Duration, given: &[(&str, PathBuf)], abend: impl Fn(&str) -> Option<Outcome>) -> std::io::Result<Outcome> {
    let stderr = fs::File::create(dir.join("stderr"))?;
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(stderr);
    let mut child = command.spawn()?;
    let began = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if began.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let mut text = String::from_utf8_lossy(&fs::read(dir.join("stderr")).unwrap_or_default()).into_owned();
    for (dd, path) in given {
        text = text.replace(&path.display().to_string(), dd);
    }
    let _ = fs::remove_dir_all(dir);
    Ok(match status {
        None => Outcome::Timeout,
        Some(s) => ended(s.code(), &text, abend),
    })
}

/// One run as its own process, so a run that loops is stopped and one that fails stops nothing else.
/// `rdw` names the DDs whose records carry RDWs.
struct Runner<'a> {
    req: &'a Request,
    work: PathBuf,
    count: u64,
    rdw: BTreeSet<String>,
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
        // A data set is named by its place in the run's directory, never by its DD: the DD comes
        // from ASSIGN, whose literal may name a path anywhere.
        let mut given: Vec<(&str, PathBuf)> = Vec::new();
        for (dd, records) in &inputs.files {
            let path = dir.join(format!("dd{}", given.len()));
            fs::write(&path, data_set(records, self.rdw.contains(dd)))?;
            given.push((dd, path));
        }
        for dd in &others.unfed {
            let path = dir.join(format!("dd{}", given.len()));
            fs::write(&path, b"")?;
            given.push((dd, path));
        }
        for dd in &others.written {
            given.push((dd, dir.join(format!("dd{}", given.len()))));
        }
        if let Some(lines) = &inputs.sysin {
            let path = dir.join("sysin");
            fs::write(&path, sysin_text(lines))?;
            given.push(("SYSIN", path));
        }
        for (dd, path) in &given {
            command.arg("--dd").arg(format!("{dd}={}", path.display()));
        }
        if let Some((journal, coverage)) = evidence {
            command.arg("--evidence").arg(journal).arg("--coverage").arg(coverage);
        }
        let roots = self.req.roots();
        finish(command, &dir, self.req.timeout, &given, |l| abend_line(l, &roots))
    }
}

/// `file:line:col: ABEND code: message`, the place as the run's journal records it, the file
/// relative to the root it was read from.
pub(crate) fn abend_line(line: &str, roots: &[PathBuf]) -> Option<Outcome> {
    let (place, rest) = line.split_once(": ABEND ")?;
    let (code, message) = rest.split_once(": ").unwrap_or((rest, ""));
    let mut parts = place.rsplitn(3, ':');
    let (_col, number, file) = (parts.next()?, parts.next()?.parse().ok()?, parts.next()?);
    Some(Outcome::Abend { code: code.to_string(), file: crate::evidence::relative(Path::new(file), roots), line: number, message: message.to_string() })
}

/// The smallest input found that still ends at the same abend, records and lines dropped and then
/// each field put back to a value that breaks nothing, and whether the search finished within its
/// budget of runs.
fn minimize(runner: &mut Runner, feeds: &[Feed], others: &Others, mut inputs: Inputs, place: &(String, String, i64), budget: u32) -> (Inputs, bool) {
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
            let record = &inputs.files[&feed.dd][r];
            let size = record.len();
            if feed.relative && record.iter().all(|&b| b == 0) {
                continue;
            }
            let fields = feed.shapes.iter().flat_map(|s| &s.fields).filter(|f| f.offset + f.size <= size && feed.keys.iter().all(|&(at, len)| f.offset + f.size <= at || at + len <= f.offset));
            for &f in fields {
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
    (inputs, left > 0)
}

const BASE64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub(crate) fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            out.push(if i <= chunk.len() { char::from(BASE64[((n >> shift) & 63) as usize]) } else { '=' });
        }
    }
    out
}

pub(crate) fn obj(pairs: Vec<(&str, Value)>) -> Value {
    Value::Obj(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// One of a kept run's inputs as the manifest lists it.
pub(crate) fn input(id: &str, kind: &str, name: &str, bytes: &[u8], minimized: bool) -> Value {
    obj(vec![("id", id.into()), ("kind", kind.into()), ("name", name.into()), ("bytes", base64(bytes).into()), ("minimized", minimized.into())])
}

/// A kept run as the manifest lists it: its inputs by id, its abend, and its journal and coverage.
pub(crate) fn kept_run(ids: Vec<Value>, abend: &Outcome, journal: String, n: usize) -> Value {
    let Outcome::Abend { code, file, line, message } = abend else { unreachable!("only an abend is kept") };
    obj(vec![
        ("input", Value::Arr(ids)),
        ("outcome", "abend".into()),
        ("abend", obj(vec![("code", code.as_str().into()), ("file", file.as_str().into()), ("line", (*line).into()), ("message", message.as_str().into())])),
        ("journal", journal.into()),
        ("coverage", format!("coverage/{n}.json").into()),
    ])
}

/// A directory resolved, `..` and links included; an empty path is the current directory.
pub(crate) fn resolved(p: &Path) -> PathBuf {
    fs::canonicalize(if p.as_os_str().is_empty() { Path::new(".") } else { p }).unwrap_or_else(|_| p.to_path_buf())
}

/// A source's path from `root`, both resolved but the source keeping its own name, or None when it
/// is not under `root`.
pub(crate) fn from_root(source: &Path, root: &Path) -> Option<String> {
    let path = source.file_name().map(|name| resolved(source.parent().unwrap_or(Path::new(""))).join(name))?;
    path.strip_prefix(resolved(root)).ok().map(|p| p.to_string_lossy().replace('\\', "/"))
}

/// What a manifest says of a fuzz run besides its inputs, counts and kept runs (docs/evidence.md
/// §5). `roots` are the directories its runs read, in the order a journal's `input` records number
/// them.
pub(crate) struct Header<'a> {
    pub(crate) seed: u64,
    pub(crate) clock: &'a str,
    pub(crate) file: &'a str,
    pub(crate) id: &'a str,
    pub(crate) root: &'a Path,
    pub(crate) roots: &'a [PathBuf],
    pub(crate) entry: &'a str,
}

pub(crate) fn write_manifest(out: &Path, header: &Header, inputs: Vec<Value>, tally: &Tally, runs: Vec<Value>) -> std::io::Result<()> {
    // Each root by its path from --root, null outside it: cobolwork finds an abend's file under the
    // root that supplied it.
    let top = resolved(header.root);
    let roots = header
        .roots
        .iter()
        .map(|r| match resolved(r).strip_prefix(&top) {
            Ok(rest) if rest.as_os_str().is_empty() => ".".into(),
            Ok(rest) => rest.to_string_lossy().replace('\\', "/").into(),
            Err(_) => Value::Null,
        })
        .collect();
    let manifest = obj(vec![
        ("tool", "ironwork-fuzz".into()),
        ("version", env!("CARGO_PKG_VERSION").into()),
        ("seed", Value::from(header.seed)),
        ("strategy", "fields".into()),
        ("clock", header.clock.into()),
        ("program", obj(vec![("file", header.file.into()), ("id", header.id.into())])),
        ("roots", Value::Arr(roots)),
        ("entry", header.entry.into()),
        ("inputs", Value::Arr(inputs)),
        ("counts", Value::Obj(tally.counts.iter().map(|(k, v)| (k.to_string(), Value::Int(*v))).collect())),
        ("runs", Value::Arr(runs)),
    ]);
    fs::write(out.join("manifest.json"), format!("{}\n", canonical(&manifest)))
}

/// What a fuzz run's runs came to: a count per outcome, the first refusal's reason and how many
/// runs crashed ironwork, with the first crash.
pub(crate) struct Tally {
    pub(crate) counts: BTreeMap<&'static str, i64>,
    refused: Option<String>,
    crashes: Option<(usize, String)>,
}

impl Tally {
    pub(crate) fn new() -> Tally {
        Tally { counts: [("runs", 0), ("clean", 0), ("abend", 0), ("timeout", 0), ("refused", 0)].into_iter().collect(), refused: None, crashes: None }
    }

    pub(crate) fn add(&mut self, outcome: &Outcome) {
        let tally = match outcome {
            Outcome::Clean => "clean",
            Outcome::Timeout => "timeout",
            Outcome::Refused(why) => {
                self.refused.get_or_insert_with(|| why.clone());
                "refused"
            }
            Outcome::Crash(why) => {
                self.crashes.get_or_insert_with(|| (0, why.clone())).0 += 1;
                "refused"
            }
            Outcome::Abend { code, file, line, message } if NOT_THE_INPUT.contains(&code.as_str()) => {
                self.refused.get_or_insert_with(|| format!("{file}:{line} {code} {message}"));
                "refused"
            }
            Outcome::Abend { .. } => "abend",
        };
        for key in ["runs", tally] {
            *self.counts.get_mut(key).expect("counted") += 1;
        }
    }

    /// Standard error's account of the runs that say nothing of the program's input.
    pub(crate) fn report(&self) {
        if let Some(first) = &self.refused {
            eprintln!("ironwork fuzz: {} runs refused; the first: {first}", self.counts["refused"] - self.crashes.as_ref().map_or(0, |c| c.0 as i64));
        }
        if let Some((n, first)) = &self.crashes {
            eprintln!("ironwork fuzz: {n} runs crashed ironwork itself and are counted as refused; the first panicked at {first}");
        }
    }

    pub(crate) fn summary(&self, kept: usize, out: &Path) -> String {
        let c = &self.counts;
        format!("ironwork fuzz: {} runs, {} clean, {} abend ({kept} kept), {} timeout, {} refused: {}", c["runs"], c["clean"], c["abend"], c["timeout"], c["refused"], out.display())
    }
}

/// An empty directory for a fuzz run's output, with `evidence` prepared for the runs that keep
/// abends and `coverage`, and the `.work` directory the runs use while it runs.
pub(crate) fn prepare(out: &Path, roots: &[PathBuf]) -> Result<PathBuf, String> {
    if out.exists() && fs::read_dir(out).map(|mut d| d.next().is_some()).unwrap_or(true) {
        return Err(format!("-o {} is not empty: each fuzz run gets a directory of its own", out.display()));
    }
    // Each kept abend rests on a run with --evidence, which refuses a directory inside one it reads.
    exec::evidence::prepare(&out.join("evidence"), roots).and_then(|_| fs::create_dir_all(out.join("coverage"))).map_err(|e| format!("-o {}: {e}", out.display()))?;
    let work = out.join(".work");
    match fs::create_dir(&work) {
        Ok(()) => Ok(work),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(format!("-o {} is in use by another fuzz run", out.display())),
        Err(e) => Err(format!("-o {}: {e}", out.display())),
    }
}

/// The program compiled, and the other programs of its source, those it contains among them.
fn compile(req: &Request) -> Result<(exec::Compiled, Vec<Program>), String> {
    let path = req.program.display().to_string();
    let bytes = fs::read(&req.program).map_err(|e| format!("{path}: {e}"))?;
    let text = syntax::copy::decode(&bytes);
    let own = req.program.parent().map(Path::to_path_buf).unwrap_or_default();
    let libraries = syntax::copy::Libraries::new(std::iter::once(own).chain(req.libraries.iter().cloned()).collect()).with_program(&req.program);
    let mut programs = syntax::parse_all_with(&text, &libraries).map_err(|e| e.place(&path))?;
    let first = programs.remove(0);
    let compiled = exec::compile(first, &req.flags).map_err(|messages| messages.iter().map(|m| m.place(&path)).collect::<Vec<_>>().join("\n"))?;
    Ok((compiled, programs))
}

pub fn run(req: Request) -> ExitCode {
    let fail = |message: String| {
        eprintln!("ironwork fuzz: {message}");
        ExitCode::from(2)
    };
    let (compiled, rest) = match compile(&req) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(12);
        }
    };
    let Some(file) = from_root(&req.program, &req.root) else {
        return fail(format!("{} is not under --root {}", req.program.display(), req.root.display()));
    };
    if !compiled.program.using.is_empty() {
        return fail(format!("{} takes PROCEDURE DIVISION USING parameters: fuzz runs a main program", compiled.program.id));
    }
    let (feeds, sysin, others) = inputs_of(&compiled, &rest);
    if feeds.is_empty() && !sysin {
        let unfed = if others.unfed.is_empty() { String::new() } else { format!(" (not varied: {})", others.unfed.join(", ")) };
        return fail(format!("{} reads no sequential, indexed or relative file on a DD of its own{unfed} and no SYSIN, so there is nothing to vary", compiled.program.id));
    }
    let evidence = req.out.join("evidence");
    let coverage = req.out.join("coverage");
    let work = match prepare(&req.out, &req.roots()) {
        Ok(w) => w,
        Err(e) => return fail(e),
    };
    let rdw = feeds.iter().filter(|f| f.variable.is_some()).map(|f| f.dd.clone()).collect();
    let mut runner = Runner { req: &req, work, count: 0, rdw };

    // What the program does on empty input is no input's doing, so an abend it gives then is not kept.
    let empty = Inputs { files: feeds.iter().map(|f| (f.dd.clone(), Vec::new())).collect(), sysin: sysin.then(Vec::new) };
    let baseline = match runner.run(&empty, &others, None) {
        Ok(o) => o.place(),
        Err(e) => return fail(format!("a run could not start: {e}")),
    };
    let mut rng = Rng(req.seed.max(1));
    let mut tally = Tally::new();
    let mut kept: Vec<((String, String, i64), Inputs)> = Vec::new();
    for _ in 0..req.runs {
        let inputs = generate(&mut rng, &feeds, sysin);
        let outcome = match runner.run(&inputs, &others, None) {
            Ok(o) => o,
            Err(e) => return fail(format!("a run could not start: {e}")),
        };
        tally.add(&outcome);
        if let Some(place) = outcome.place()
            && Some(&place) != baseline.as_ref()
            && !kept.iter().any(|(p, _)| *p == place)
        {
            kept.push((place, inputs));
        }
    }

    let (mut inputs_out, mut runs_out) = (Vec::new(), Vec::new());
    for (n, (place, found)) in kept.into_iter().enumerate() {
        let (small, minimized) = minimize(&mut runner, &feeds, &others, found, &place, 200);
        let before = journals(&evidence);
        let cover = coverage.join(format!("{n}.json"));
        let outcome = match runner.run(&small, &others, Some((&evidence, &cover))) {
            Ok(o) => o,
            Err(e) => return fail(format!("a run could not start: {e}")),
        };
        let came_again = outcome.place().as_ref() == Some(&place);
        let journal = journals(&evidence).into_iter().find(|j| !before.contains(j)).filter(|_| came_again);
        let Some(journal) = journal else {
            let why = if came_again { "wrote no journal".to_string() } else { outcome.told() };
            eprintln!("ironwork fuzz: {} at {}:{} is not kept: its run on the smallest input {why}", place.0, place.1, place.2);
            continue;
        };
        let mut ids = Vec::new();
        for (dd, records) in &small.files {
            let id = format!("r{n}-{dd}");
            inputs_out.push(input(&id, "dd", dd, &data_set(records, runner.rdw.contains(dd)), minimized));
            ids.push(Value::from(id));
        }
        if let Some(lines) = &small.sysin {
            let id = format!("r{n}-SYSIN");
            inputs_out.push(input(&id, "sysin", "SYSIN", &sysin_text(lines), minimized));
            ids.push(Value::from(id));
        }
        runs_out.push(kept_run(ids, &outcome, journal, n));
    }
    let _ = fs::remove_dir_all(&runner.work);

    let kept = runs_out.len();
    let header = Header { seed: req.seed, clock: &req.clock, file: &file, id: &compiled.program.id, root: &req.root, roots: &req.roots(), entry: "run" };
    if let Err(e) = write_manifest(&req.out, &header, inputs_out, &tally, runs_out) {
        return fail(format!("-o {}: {e}", req.out.display()));
    }
    if !others.unfed.is_empty() {
        eprintln!("ironwork fuzz: not varied, given empty: {}", others.unfed.join(", "));
    }
    if !others.ungiven.is_empty() {
        eprintln!("ironwork fuzz: no --dd can carry these names, so they are given no data set: {}", others.ungiven.join(", "));
    }
    tally.report();
    if let Some((code, file, line)) = baseline {
        eprintln!("ironwork fuzz: the program ends with {code} at {file}:{line} on empty input; that abend is not kept");
    }
    println!("{}", tally.summary(kept, &req.out));
    ExitCode::SUCCESS
}

/// The run journals an evidence directory holds, by run id.
pub(crate) fn journals(evidence: &Path) -> Vec<String> {
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

    #[test]
    fn a_run_that_panics_is_a_crash_not_a_clean_run() {
        let panic = "thread '<unnamed>' (42) panicked at crates/exec/src/x.rs:9:5:\nindex out of bounds\nnote: run with `RUST_BACKTRACE=1`\n";
        assert!(matches!(ended(Some(16), panic, |_| None), Outcome::Crash(why) if why == "crates/exec/src/x.rs:9:5: index out of bounds"));
        assert!(matches!(ended(Some(16), "", |_| None), Outcome::Clean));
        assert!(matches!(ended(Some(2), "ironwork: no such file\n\n", |_| None), Outcome::Refused(why) if why == "ironwork: no such file"));
        assert!(matches!(ended(None, "", |_| None), Outcome::Refused(_)));
    }

    #[test]
    fn a_return_code_of_2_is_the_program_s_and_not_a_refusal() {
        assert!(matches!(ended(Some(2), "", |_| None), Outcome::Clean));
        assert!(matches!(ended(Some(2), "TOTAL 2\n", |_| None), Outcome::Clean));
        assert!(matches!(ended(Some(2), "ironwork: bad DD\nusage: ...\n", |_| None), Outcome::Refused(why) if why == "ironwork: bad DD"));
    }

    #[test]
    fn a_packed_field_of_even_digits_keeps_its_pad_nibble_zero_and_a_national_one_holds_utf_16() {
        let packed = Field { offset: 0, size: 3, kind: Kind::Packed { digits: 4, scale: 0, signed: true } };
        let national = Field { offset: 0, size: 6, kind: Kind::National };
        let mut rng = Rng(5);
        let mut valid = 0;
        for _ in 0..300 {
            let p = field_bytes(&mut rng, packed);
            if p[..2].iter().all(|b| b >> 4 <= 9 && b & 0x0F <= 9) && matches!(p[2] & 0x0F, 0x0C | 0x0D) {
                valid += 1;
                assert_eq!(p[0] >> 4, 0, "{p:02X?}");
            }
            let n = field_bytes(&mut rng, national);
            assert_eq!(n.len(), 6);
        }
        assert!(valid > 100);
        assert_eq!(neutral(national), [0, 0x20, 0, 0x20, 0, 0x20]);
    }

    #[test]
    fn variable_records_lie_within_the_longest_and_carry_rdws_and_relative_slots_may_be_empty() {
        let alnum = |offset, size| Field { offset, size, kind: Kind::Alnum { justified: false } };
        let shapes = vec![Shape { size: 4, fields: vec![alnum(0, 4)] }, Shape { size: 10, fields: vec![alnum(0, 4), alnum(4, 6)] }];
        let feed = Feed { dd: "VB".into(), length: 10, variable: Some((4, 10)), shapes, keys: Vec::new(), relative: true };
        let mut rng = Rng(9);
        let (mut lengths, mut empty) = (BTreeSet::new(), 0);
        for _ in 0..400 {
            let r = record(&mut rng, &feed);
            assert!(r.len() <= 10);
            if r.is_empty() {
                empty += 1;
            }
            lengths.insert(r.len());
        }
        assert!(empty > 0 && lengths.contains(&4) && lengths.contains(&10) && lengths.iter().any(|&n| (1..4).contains(&n)), "{lengths:?}");
        assert_eq!(data_set(&[vec![0xC1, 0xC2], vec![]], true), [0, 6, 0, 0, 0xC1, 0xC2, 0, 4, 0, 0]);
        assert_eq!(data_set(&[vec![0xC1], vec![0xC2]], false), [0xC1, 0xC2]);
    }

    #[test]
    fn an_indexed_feed_holds_its_records_in_key_order_and_repeats_no_unique_key() {
        let alnum = |offset, size| Field { offset, size, kind: Kind::Alnum { justified: false } };
        let shapes = vec![Shape { size: 3, fields: vec![alnum(0, 2), alnum(2, 1)] }];
        let feeds = [Feed { dd: "KS".into(), length: 3, variable: None, shapes, keys: vec![(0, 2), (2, 1)], relative: false }];
        let mut rng = Rng(3);
        for _ in 0..200 {
            let records = &generate(&mut rng, &feeds, false).files["KS"];
            assert!(records.windows(2).all(|w| w[0][..2] < w[1][..2]));
            assert_eq!(records.iter().map(|r| r[2]).collect::<BTreeSet<_>>().len(), records.len());
        }
    }
}
