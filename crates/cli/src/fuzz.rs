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
use syntax::ast::{FileDecl, Literal, Organization, Paragraph, Program, Ref, Stmt};
use zarch::ebcdic::CodePage;

pub(crate) mod dictionary;
pub(crate) mod differential;
pub(crate) mod interface;
pub(crate) mod job;

#[derive(Clone)]
pub struct Request {
    pub program: PathBuf,
    pub out: PathBuf,
    pub root: PathBuf,
    pub runs: u32,
    pub seed: u64,
    pub timeout: Duration,
    /// The statements a timed-out input may start when it is re-checked for a hang.
    pub hang_limit: u64,
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
#[derive(Clone)]
struct Shape {
    size: usize,
    fields: Vec<Field>,
    /// The values the program compares a field with, by the field's offset (`dictionary`).
    values: BTreeMap<usize, Vec<Vec<u8>>>,
}

/// A sequential, indexed or relative file the program reads: its DD, the longest record, the
/// shortest and longest a READ takes when its records have more than one length (each behind an
/// RDW in the data set), its level-01 records, an indexed file's keys by offset and length (the
/// RECORD KEY first, then each ALTERNATE RECORD KEY that allows no duplicates), and whether it is
/// relative, whose data set holds a record per slot.
#[derive(Clone)]
struct Feed {
    dd: String,
    length: usize,
    variable: Option<(usize, usize)>,
    shapes: Vec<Shape>,
    keys: Vec<(usize, usize)>,
    relative: bool,
}

/// What one run is given, each input by the name its manifest entry takes: the records of each fed
/// data set (by DD, or by data set name in a job), lines (SYSIN, or a job step's in-stream DD as
/// STEP.DD), and PARMs (PARM, or a job step's by its name).
#[derive(Clone, Default)]
struct Inputs {
    files: BTreeMap<String, Vec<Vec<u8>>>,
    lines: BTreeMap<String, Vec<Vec<u8>>>,
    parms: BTreeMap<String, Vec<u8>>,
    /// The statement limit a timed-out input is re-checked under, which its runs keep.
    limit: Option<u64>,
    /// The marker put in place of the program name a CEE3501S named, which its evidence run traces.
    marker: Option<String>,
    /// The run is compiled with OPTIMIZE(2): a kept abend's re-check.
    optimized: bool,
}

/// How many times --timeout a run may take. Each run also stops at --hang-limit statements, which
/// ends a loop the input caused in S322 however loaded the machine is; the clock stops only a run
/// too slow to reach the limit.
const HANG_PATIENCE: u32 = 6;
/// The CEE3501S endings a fuzz run tries a marker on, each at a CALL of its own.
const CHOSEN_CHECKS: usize = 5;

/// A run stopped at its timeout or its statement limit after ACCEPT found SYSIN at its end was
/// waiting for input it was not given, not looping on input it was given.
fn waited(outcome: Outcome, text: &str) -> Outcome {
    let waiting = text.contains("ACCEPT found SYSIN at its end");
    match outcome {
        Outcome::Timeout if waiting => Outcome::Waiting,
        Outcome::Abend { code, .. } if code == "S322" && waiting => Outcome::Waiting,
        outcome => outcome,
    }
}

/// The lines an S322's message says its loop runs, from "in the loop over lines 12, 14 and 3 more".
fn loop_lines(message: &str) -> BTreeSet<i64> {
    let Some((_, list)) = message.rsplit_once("in the loop over lines ") else { return BTreeSet::new() };
    list.split(" and ").next().unwrap_or("").split(", ").filter_map(|n| n.trim().parse().ok()).collect()
}

/// Whether two S322s are one loop: in one file, and running a line in common.
fn same_loop(a: &Outcome, b: &Outcome) -> bool {
    match (a, b) {
        (Outcome::Abend { code: c, file: f, line: l, message: m }, Outcome::Abend { code: d, file: g, line: k, message: n }) if c == "S322" && d == "S322" && f == g => {
            l == k || !loop_lines(m).is_disjoint(&loop_lines(n))
        }
        _ => false,
    }
}

/// The module a dynamic CALL found nowhere, when an ending is CEE3501S's (assumption C450).
fn missing_module<'a>(code: &str, message: &'a str) -> Option<&'a str> {
    (code == "U4038").then(|| rt::le::module_not_found(message)).flatten()
}

/// A marker as long as `name`, of characters fuzz never generates, so it can come only from where
/// fuzz puts it.
fn marker_for(name: &str) -> String {
    "@#$".chars().cycle().take(name.chars().count()).collect()
}

/// `bytes` with every `from` given as `to`, which is as long.
fn swapped(bytes: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if !from.is_empty() && bytes[i..].starts_with(from) {
            out.extend_from_slice(to);
            i += from.len();
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

/// `text` in EBCDIC as records and arguments hold it, a character CCSID 37 lacks as a space.
fn ebcdic(text: &str) -> Vec<u8> {
    text.chars().map(|c| CP037.encode_char(c).unwrap_or(SPACE)).collect()
}

/// `inputs` with every `name` in them, as text in lines and PARMs and in EBCDIC in records, given as
/// `to` instead.
fn renamed(inputs: &Inputs, name: &str, to: &str) -> Inputs {
    let (from_e, to_e) = (ebcdic(name), ebcdic(to));
    Inputs {
        files: inputs.files.iter().map(|(k, records)| (k.clone(), records.iter().map(|r| swapped(r, &from_e, &to_e)).collect())).collect(),
        lines: inputs.lines.iter().map(|(k, lines)| (k.clone(), lines.iter().map(|l| swapped(l, name.as_bytes(), to.as_bytes())).collect())).collect(),
        parms: inputs.parms.iter().map(|(k, text)| (k.clone(), swapped(text, name.as_bytes(), to.as_bytes()))).collect(),
        ..inputs.clone()
    }
}

/// Whether `name` is in `inputs` as `renamed` finds it.
fn named_in(inputs: &Inputs, name: &str) -> bool {
    let to = marker_for(name);
    let after = renamed(inputs, name, &to);
    after.files != inputs.files || after.lines != inputs.lines || after.parms != inputs.parms
}

/// Whether the run journal records a dynamic program load at `line` whose operand held the marker.
fn marker_reached(evidence: &Path, journal: &str, line: i64) -> bool {
    let at = format!("\"line\":{line}");
    fs::read_to_string(evidence.join("runs").join(format!("{journal}.jsonl"))).is_ok_and(|text| {
        text.lines().any(|l| {
            l.contains("\"kind\":\"sink\"")
                && l.contains("\"sink\":\"dynamic-program-load\"")
                && l.contains("\"reached\":true")
                && l.match_indices(&at).any(|(i, _)| !l[i + at.len()..].starts_with(|c: char| c.is_ascii_digit()))
        })
    })
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    Clean,
    Abend { code: String, file: String, line: i64, message: String },
    Timeout,
    /// Stopped at its timeout or statement limit while ACCEPT kept finding SYSIN at its end.
    Waiting,
    /// The run stopped for a reason of its surroundings, with the last line it said.
    Refused(String),
    /// ironwork itself failed, with where and why it panicked.
    Crash(String),
}

/// Abends that say what the run's surroundings lack, not what its input did: a construct ironwork
/// does not run, a job step's program no library holds, an EXEC statement with no database or
/// region behind it. A dynamic CALL of a program no library holds and an OPEN of a file no DD
/// gives, which end U4038, are ones too (`missing_module`, `missing_data_set`).
const NOT_THE_INPUT: &[&str] = &["IRONWORK", "S806", "EXEC"];

/// Whether an abend says what the run's surroundings lack rather than what its input did.
fn not_the_input(code: &str, message: &str) -> bool {
    NOT_THE_INPUT.contains(&code) || missing_module(code, message).is_some() || missing_data_set(code, message)
}

/// Whether an ending is IGZ0035S's for an OPEN that found no data set, status 35 (assumption C451).
fn missing_data_set(code: &str, message: &str) -> bool {
    code == "U4038" && message.starts_with("IGZ0035S ") && message.contains(" The status code was 35.")
}

impl Outcome {
    pub(crate) fn place(&self) -> Option<(String, String, i64)> {
        match self {
            Outcome::Abend { code, file, line, message } if !not_the_input(code, message) => Some((code.clone(), file.clone(), *line)),
            _ => None,
        }
    }

    pub(crate) fn told(&self) -> String {
        match self {
            Outcome::Clean => "ended normally".into(),
            Outcome::Abend { code, file, line, .. } => format!("ended with {code} at {file}:{line}"),
            Outcome::Timeout => "timed out".into(),
            Outcome::Waiting => "kept waiting for input after SYSIN's end".into(),
            Outcome::Refused(why) => format!("was refused: {why}"),
            Outcome::Crash(why) => format!("crashed ironwork: {why}"),
        }
    }
}

/// How a finished run ended, from its exit status in the reserved band (`crate::exit`): a
/// RETURN-CODE is a clean end, an abend is told by the line that reports it, a panic by Rust's
/// panic line, and ironwork's other refusals by the first line it leads.
pub(crate) fn ended(code: Option<i32>, text: &str, abend: impl Fn(&str) -> Option<Outcome>) -> Outcome {
    use crate::exit::Outcome as Exit;
    let last = text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").to_string();
    match code.and_then(|c| u8::try_from(c).ok()).and_then(Exit::read) {
        Some(Exit::Ended(_)) => Outcome::Clean,
        Some(Exit::Abend | Exit::NotRun) => text.lines().rev().find_map(abend).unwrap_or(Outcome::Refused(last)),
        Some(Exit::Internal) => {
            let mut lines = text.lines();
            let at = lines.find_map(|l| l.strip_prefix("thread '").and_then(|l| l.split_once(" panicked at ")).map(|(_, at)| at));
            Outcome::Crash(at.map_or(last, |at| format!("{at} {}", lines.next().unwrap_or("")).trim_end().to_string()))
        }
        Some(_) => Outcome::Refused(text.lines().find(|l| l.starts_with("ironwork: ")).map_or(last, str::to_string)),
        None => Outcome::Refused(last),
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
/// One field in this many whose item the program compares with literals takes one of them.
pub(crate) const DICTIONARY_ODDS: usize = 4;
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
        let bytes = match shape.and_then(|s| s.values.get(&f.offset)) {
            Some(values) if rng.below(DICTIONARY_ODDS) == 0 => values[rng.below(values.len())].clone(),
            _ => field_bytes(rng, f),
        };
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

/// A SYSIN card. A z/OS SYSIN record has 80 bytes, so a blank card is spaces, never empty; and the
/// reader ends in-stream data at a card that starts `/*` or `//`, so no card does.
fn sysin_line(rng: &mut Rng) -> Vec<u8> {
    let mut line = match text_line(rng, &[80, 80, 10, 1, 0]) {
        line if line.is_empty() => vec![b' '; 80],
        line => line,
    };
    if line.starts_with(b"/*") || line.starts_with(b"//") {
        line[0] = b' ';
    }
    line
}

/// A PARM of up to the 100 characters JCL allows; a slash in it may set runtime options apart.
fn parm_text(rng: &mut Rng) -> Vec<u8> {
    text_line(rng, &[0, 1, 8, 20, rt::le::parm::PARM_LIMIT])
}

/// A line of one of `lengths`, of digits, letters, number punctuation or all of them.
fn text_line(rng: &mut Rng, lengths: &[usize]) -> Vec<u8> {
    let length = lengths[rng.below(lengths.len())];
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

fn generate(rng: &mut Rng, varied: &Varied) -> Inputs {
    let mut files = BTreeMap::new();
    for f in &varied.feeds {
        let records: Vec<Vec<u8>> = (0..1 + rng.below(4)).map(|_| record(rng, f)).collect();
        files.insert(f.dd.clone(), in_key_order(f, records));
    }
    let lines = varied.lines.iter().map(|k| (k.clone(), (0..1 + rng.below(3)).map(|_| sysin_line(rng)).collect())).collect();
    let parms = varied.parms.iter().map(|k| (k.clone(), parm_text(rng))).collect();
    Inputs { files, lines, parms, ..Default::default() }
}

/// An indexed file's data set holds its records in key order, no two sharing a key that allows no
/// duplicates, as REPRO unloads it.
fn in_key_order(f: &Feed, mut records: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    if let Some(&(at, len)) = f.keys.first() {
        records.sort_by(|a, b| a[at..at + len].cmp(&b[at..at + len]));
    }
    for &(at, len) in &f.keys {
        let mut seen = BTreeSet::new();
        records.retain(|r| seen.insert(r[at..at + len].to_vec()));
    }
    records
}

/// `parent` changed in one place: a field of one record drawn again (from the dictionary or as a
/// record's fields are), every field of one record the program does not compare with a literal
/// drawn again, a record added or dropped, a SYSIN line or a PARM drawn again.
fn mutate(rng: &mut Rng, varied: &Varied, parent: &Inputs) -> Inputs {
    let mut child = Inputs { files: parent.files.clone(), lines: parent.lines.clone(), parms: parent.parms.clone(), ..Default::default() };
    let places = varied.feeds.len() + varied.lines.len() + varied.parms.len();
    let mut k = rng.below(places);
    if let Some(f) = varied.feeds.get(k) {
        let records = child.files.entry(f.dd.clone()).or_default();
        match rng.below(4) {
            0 => records.push(record(rng, f)),
            1 if !records.is_empty() => {
                let at = rng.below(records.len());
                records.remove(at);
            }
            _ if !records.is_empty() && !f.shapes.is_empty() => {
                let r = rng.below(records.len());
                let shape = &f.shapes[rng.below(f.shapes.len())];
                let fields: Vec<&Field> = shape.fields.iter().filter(|g| g.offset + g.size <= records[r].len()).collect();
                let compared: Vec<&Field> = fields.iter().copied().filter(|g| shape.values.contains_key(&g.offset)).collect();
                // A field the program compares with a literal gets the change half the time there is one.
                let pool = if !compared.is_empty() && rng.below(2) == 0 { &compared } else { &fields };
                if rng.below(2) == 0 {
                    for &&g in fields.iter().filter(|g| !shape.values.contains_key(&g.offset)) {
                        records[r][g.offset..g.offset + g.size].copy_from_slice(&field_bytes(rng, g)[..g.size]);
                    }
                } else if !pool.is_empty() {
                    let g = *pool[rng.below(pool.len())];
                    let bytes = match shape.values.get(&g.offset) {
                        Some(values) if rng.below(4) != 0 => values[rng.below(values.len())].clone(),
                        _ => field_bytes(rng, g),
                    };
                    records[r][g.offset..g.offset + g.size].copy_from_slice(&bytes[..g.size]);
                }
            }
            _ => records.push(record(rng, f)),
        }
        let ordered = in_key_order(f, std::mem::take(records));
        child.files.insert(f.dd.clone(), ordered);
        return child;
    }
    k -= varied.feeds.len();
    if let Some(key) = varied.lines.get(k) {
        let lines = child.lines.entry(key.clone()).or_default();
        if lines.is_empty() || rng.below(4) == 0 {
            lines.push(sysin_line(rng));
        } else {
            // ACCEPT reads from the first line, which every run that reads at all reaches.
            let at = if rng.below(2) == 0 { 0 } else { rng.below(lines.len()) };
            lines[at] = sysin_line(rng);
        }
        return child;
    }
    k -= varied.lines.len();
    if let Some(key) = varied.parms.get(k) {
        child.parms.insert(key.clone(), parm_text(rng));
    }
    child
}

/// The most inputs a fuzz run keeps for reaching what no earlier run did, which later runs change.
pub(crate) const CORPUS_LIMIT: usize = 64;

/// Which of `kept` inputs a run changes: the newest, which reached furthest, half the time.
pub(crate) fn parent_of(rng: &mut Rng, kept: usize) -> usize {
    if rng.below(2) == 0 { kept - 1 } else { rng.below(kept) }
}

/// Whether the main program's PROCEDURE DIVISION USING takes the parameter Language Environment
/// gives an EXEC PGM=,PARM=: one item, a group whose first elementary item is a halfword binary
/// length.
fn takes_parm(compiled: &exec::Compiled) -> bool {
    let [param] = compiled.program.using.as_slice() else { return false };
    let layout = &compiled.layout;
    let Some(&item) = layout.linkage_roots.iter().find(|&&i| layout.items[i].name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(&param.name))) else { return false };
    let mut at = item;
    while let Some(&first) = layout.items[at].children.first() {
        at = first;
    }
    at != item && matches!(layout.items[at].kind, Kind::Binary { .. }) && layout.items[at].size == 2
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

/// The DDs of a program's files other than those fed: those it reads that cannot be fed, those it
/// extends, and a DD more than one file names, which get an empty data set; those it only writes,
/// which get a new one; and names `--dd` cannot carry, which get none.
#[derive(Default)]
struct Others {
    unfed: Vec<String>,
    written: Vec<String>,
    ungiven: Vec<String>,
}

/// How a program's OPEN statements use a file: OPEN EXTEND needs a data set that exists, as OPEN
/// INPUT and I-O do, and OPEN OUTPUT alone does not.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Opened {
    Read,
    Extended,
    Written,
}

/// How a program OPENs each file it OPENs other than OUTPUT alone, by name in upper case, and
/// whether it ACCEPTs from SYSIN.
fn opens(paragraphs: &[Paragraph]) -> (BTreeMap<String, Opened>, bool) {
    let mut all = Vec::new();
    for p in paragraphs {
        statements(&p.statements, &mut all);
    }
    let mut opened = BTreeMap::new();
    for (mode, name) in all.iter().filter_map(|s| if let Stmt::Open { files, .. } = s { Some(files) } else { None }).flatten() {
        let how = match mode {
            OpenMode::Input | OpenMode::InputOutput => Opened::Read,
            OpenMode::Extend => Opened::Extended,
            OpenMode::Output => continue,
        };
        let entry = opened.entry(name.to_ascii_uppercase()).or_insert(how);
        if how == Opened::Read {
            *entry = how;
        }
    }
    (opened, all.iter().any(|s| matches!(s, Stmt::Accept { from: AcceptFrom::Sysin, .. })))
}

/// The literal targets of a program's CALL statements.
fn call_targets(program: &Program) -> Vec<String> {
    let mut all = Vec::new();
    for p in &program.paragraphs {
        statements(&p.statements, &mut all);
    }
    all.iter()
        .filter_map(|s| match s {
            Stmt::Call(call) => match &call.target {
                syntax::ast::Operand::Literal(syntax::ast::Literal::Alnum(target)) => Some(target.trim().to_owned()),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// The programs a run of the main program may enter besides it: those it contains, at any depth,
/// those a CALL of a literal names, and those `roots` names, among the other programs of its
/// source and those read from the program libraries, with the programs they contain and call in
/// turn.
fn reached<'a>(compiled: &exec::Compiled, rest: &'a [Program], roots: &[String]) -> Vec<&'a Program> {
    let mut out: Vec<&Program> = Vec::new();
    let mut names: Vec<String> = compiled.program.nested.iter().cloned().chain(call_targets(&compiled.program)).chain(roots.iter().cloned()).collect();
    while let Some(name) = names.pop() {
        let answers = |p: &Program| !p.is_prototype() && (p.id.eq_ignore_ascii_case(&name) || p.load_name().eq_ignore_ascii_case(&name));
        if let Some(p) = rest.iter().find(|p| answers(p) && !out.iter().any(|o| std::ptr::eq(*o, *p))) {
            names.extend(p.nested.iter().cloned().chain(call_targets(p)));
            out.push(p);
        }
    }
    out
}

/// The files the program reads and whether it ACCEPTs from SYSIN, and the DDs of its other files.
/// The main program's files are fed; the files of a program it contains or calls get their DDs,
/// empty where it reads or extends them.
fn inputs_of(compiled: &exec::Compiled, rest: &[Program]) -> (Vec<Feed>, bool, Others) {
    inputs_reaching(compiled, rest, &[])
}

/// `inputs_of`, with the programs `roots` names entered as a CALL of a literal enters them.
fn inputs_reaching(compiled: &exec::Compiled, rest: &[Program], roots: &[String]) -> (Vec<Feed>, bool, Others) {
    let by_item = dictionary::literals_by_item(compiled);
    let how = |opened: &BTreeMap<String, Opened>, f: &FileDecl| opened.get(&f.name.to_ascii_uppercase()).copied().unwrap_or(Opened::Written);
    let (opened, mut sysin) = opens(&compiled.program.paragraphs);
    let mut files: Vec<(Option<usize>, &FileDecl, Opened)> = compiled.program.files.iter().enumerate().filter(|(_, f)| !f.sort).map(|(k, f)| (Some(k), f, how(&opened, f))).collect();
    for p in reached(compiled, rest, roots) {
        let (opened, accepts) = opens(&p.paragraphs);
        sysin |= accepts;
        files.extend(p.files.iter().filter(|f| !f.sort).map(|f| (None, f, how(&opened, f))));
    }
    let (mut feeds, mut others) = (Vec::new(), Others::default());
    for &(index, file, opened) in &files {
        let dd = &file.assign;
        // A file assigned to SYSIN in a program that ACCEPTs from SYSIN reads the same lines.
        if sysin && dd == "SYSIN" {
            continue;
        }
        if dd.contains('=') {
            others.ungiven.push(dd.clone());
            continue;
        }
        match opened {
            Opened::Written => others.written.push(dd.clone()),
            Opened::Extended => others.unfed.push(dd.clone()),
            Opened::Read => {
                let shared = files.iter().filter(|(_, f, _)| f.assign == *dd).count() > 1;
                match index.and_then(|k| feed(compiled, k, file, &by_item)).filter(|_| !shared) {
                    Some(f) => feeds.push(f),
                    None => others.unfed.push(dd.clone()),
                }
            }
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
fn feed(compiled: &exec::Compiled, index: usize, file: &FileDecl, by_item: &BTreeMap<usize, Vec<Literal>>) -> Option<Feed> {
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
        .map(|(i, item)| {
            let fields = elementary_items(layout, i, area, length);
            Shape { size: (item.size as usize).min(length), fields: fields.iter().map(|(f, _)| *f).collect(), values: dictionary::values(&fields, by_item) }
        })
        .collect();
    let variable = variable.then_some((shortest.min(length), length));
    Some(Feed { dd: file.assign.clone(), length, variable, shapes, keys, relative: file.organization == Organization::Relative })
}

/// The elementary items under an item at offset `base`, each occurrence of a table its own field,
/// by offset from `base`, those within `length` bytes. Elementary items do not overlap, so there
/// are no more fields than the item has bytes.
pub(crate) fn elementary(layout: &exec::layout::Layout, root: usize, base: u32, length: usize) -> Vec<Field> {
    elementary_items(layout, root, base, length).into_iter().map(|(f, _)| f).collect()
}

/// `elementary`, each field with the index of its item in the layout.
pub(crate) fn elementary_items(layout: &exec::layout::Layout, root: usize, base: u32, length: usize) -> Vec<(Field, usize)> {
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
        out.extend(offsets.into_iter().filter(|&at| at + size <= length).map(|offset| (Field { offset, size, kind: item.kind }, i)));
    }
    out
}

/// Runs `command`, its standard error kept in `dir`, stops it once it has run longer than
/// `timeout`, and tells how it ended, with each data set's path in what it said replaced by the DD
/// it was given for. `dir` is removed.
pub(crate) fn finish(command: Command, dir: &Path, timeout: Duration, given: &[(&str, PathBuf)], abend: impl Fn(&str) -> Option<Outcome>) -> std::io::Result<Outcome> {
    Ok(match wait_for(command, dir, timeout, given)? {
        (None, _) => Outcome::Timeout,
        (Some(code), text) => ended(code, &text, abend),
    })
}

/// As [`finish`]: the exit code of a run that ended, Some(None) when a signal ended it and None
/// when it was stopped at `timeout`, and its standard error either way, at most [`STDERR_KEPT`]
/// bytes from each end.
fn wait_for(mut command: Command, dir: &Path, timeout: Duration, given: &[(&str, PathBuf)]) -> std::io::Result<(Option<Option<i32>>, String)> {
    // nosemgrep: rust.actix.path-traversal.tainted-path.tainted-path -- a fixed name in the run's own scratch directory
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
    // nosemgrep: rust.actix.path-traversal.tainted-path.tainted-path -- a fixed name in the run's own scratch directory
    let mut text = String::from_utf8_lossy(&ends_of(&dir.join("stderr"))?).into_owned();
    for (dd, path) in given {
        text = text.replace(&path.display().to_string(), dd);
    }
    let _ = fs::remove_dir_all(dir);
    Ok((status.map(|s| s.code()), text))
}

/// How much of a run's standard error is read from each end: a refusal leads it, an abend or a
/// panic ends it, and a program can write without bound between.
const STDERR_KEPT: u64 = 1 << 20;

/// A file's bytes, or its first and last [`STDERR_KEPT`] bytes when it holds more.
fn ends_of(path: &Path) -> std::io::Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = fs::File::open(path)?;
    let size = file.metadata()?.len();
    if size <= 2 * STDERR_KEPT {
        let mut all = Vec::new();
        file.read_to_end(&mut all)?;
        return Ok(all);
    }
    let mut out = vec![0; STDERR_KEPT as usize];
    file.read_exact(&mut out)?;
    out.push(b'\n');
    file.seek(SeekFrom::End(-(STDERR_KEPT as i64)))?;
    file.read_to_end(&mut out)?;
    Ok(out)
}

/// What the runs of a fuzz run covered, from each run's `--coverage` report: how many runs started
/// each statement and entered each paragraph, and how often in all. A run stopped at --timeout
/// writes no report and is not counted.
#[derive(Default)]
pub(crate) struct RunCoverage {
    runs: i64,
    statements: BTreeMap<(String, i64), (i64, i64)>,
    paragraphs: BTreeMap<(String, String, i64), (i64, i64)>,
    /// Whether the last report taken started a statement or entered a paragraph no earlier one had.
    pub(crate) novel: std::rc::Rc<std::cell::Cell<bool>>,
}

/// Where run `count` writes its coverage: a kept run's report under the output directory, any
/// other run's a scratch file in `work`.
pub(crate) fn coverage_path(evidence: Option<(&Path, &Path)>, work: &Path, count: u64) -> PathBuf {
    evidence.map_or_else(|| work.join(format!("coverage-{count}.json")), |(_, coverage)| coverage.to_path_buf())
}

impl RunCoverage {
    /// Adds the report at `path`, then removes it unless it is a kept run's.
    pub(crate) fn take(&mut self, path: &Path, kept: bool) {
        let report = fs::read_to_string(path).ok().and_then(|t| rt::json::parse::parse(&t).ok());
        if !kept {
            let _ = fs::remove_file(path);
        }
        self.novel.set(false);
        let Some(report) = report else { return };
        self.runs += 1;
        let mut novel = false;
        for s in json_items(&report, "statements") {
            if let (Some(file), Some(line), Some(n)) = (json_text(s, "file"), json_int(s, "line"), json_int(s, "started")) {
                let e = self.statements.entry((file.to_string(), line)).or_default();
                novel |= e.1 == 0 && n > 0;
                e.0 += 1;
                e.1 += n;
            }
        }
        for p in json_items(&report, "programs") {
            let Some(program) = json_text(p, "program") else { continue };
            for d in json_items(p, "detail") {
                if let (Some(name), Some(line), Some(n)) = (json_text(d, "name"), json_int(d, "line"), json_int(d, "entered")) {
                    let e = self.paragraphs.entry((program.to_string(), name.to_string(), line)).or_default();
                    novel |= e.1 == 0 && n > 0;
                    e.0 += i64::from(n > 0);
                    e.1 += n;
                }
            }
        }
        self.novel.set(novel);
    }

    /// Writes the coverage as `coverage/runs.json` under `out` (docs/evidence.md §5) and returns the
    /// manifest's `runCoverage` naming it.
    pub(crate) fn write(&self, out: &Path) -> std::io::Result<(&'static str, Value)> {
        let statements = self.statements.iter().map(|((file, line), (runs, n))| obj(vec![("file", file.as_str().into()), ("line", (*line).into()), ("runs", (*runs).into()), ("started", (*n).into())])).collect();
        let paragraphs = self
            .paragraphs
            .iter()
            .map(|((program, name, line), (runs, n))| obj(vec![("program", program.as_str().into()), ("name", name.as_str().into()), ("line", (*line).into()), ("runs", (*runs).into()), ("entered", (*n).into())]))
            .collect();
        let doc = obj(vec![("runs", self.runs.into()), ("statements", Value::Arr(statements)), ("paragraphs", Value::Arr(paragraphs))]);
        fs::write(out.join("coverage").join("runs.json"), format!("{}\n", canonical(&doc)))?;
        Ok(("runCoverage", "coverage/runs.json".into()))
    }
}

fn json_field<'a>(v: &'a rt::json::parse::Value, key: &str) -> Option<&'a rt::json::parse::Value> {
    match v {
        rt::json::parse::Value::Object(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
        _ => None,
    }
}

fn json_items<'a>(v: &'a rt::json::parse::Value, key: &str) -> &'a [rt::json::parse::Value] {
    match json_field(v, key) {
        Some(rt::json::parse::Value::Array(items)) => items,
        _ => &[],
    }
}

fn json_text<'a>(v: &'a rt::json::parse::Value, key: &str) -> Option<&'a str> {
    match json_field(v, key) {
        Some(rt::json::parse::Value::String(s)) => Some(s),
        _ => None,
    }
}

fn json_int(v: &rt::json::parse::Value, key: &str) -> Option<i64> {
    match json_field(v, key) {
        Some(rt::json::parse::Value::Number(n)) => n.parse().ok(),
        _ => None,
    }
}

/// One run as its own process, so a run that loops is stopped and one that fails stops nothing else.
/// `rdw` names the DDs whose records carry RDWs.
struct Runner<'a> {
    req: &'a Request,
    work: PathBuf,
    count: u64,
    rdw: BTreeSet<String>,
    covered: RunCoverage,
}

impl Runner<'_> {
    fn run(&mut self, inputs: &Inputs, others: &Others, evidence: Option<(&Path, &Path)>) -> std::io::Result<Outcome> {
        self.count += 1;
        let dir = self.work.join(format!("run-{}", self.count));
        fs::create_dir_all(&dir)?;
        let mut command = Command::new(std::env::current_exe()?);
        command.arg("run").arg(&self.req.program).arg("--clock").arg(&self.req.clock);
        command.args(&self.req.flags);
        if inputs.optimized {
            command.arg("--optimize=2");
        }
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
        if let Some(lines) = inputs.lines.get("SYSIN") {
            let path = dir.join("sysin");
            fs::write(&path, sysin_text(lines))?;
            given.push(("SYSIN", path));
        }
        if let Some(parm) = inputs.parms.get("PARM") {
            command.arg("--parm").arg(String::from_utf8_lossy(parm).as_ref());
        }
        if let Some(limit) = inputs.limit {
            command.arg("--statement-limit").arg(limit.to_string());
        }
        for (dd, path) in &given {
            command.arg("--dd").arg(format!("{dd}={}", path.display()));
        }
        let cover = coverage_path(evidence, &self.work, self.count);
        command.arg("--coverage").arg(&cover);
        if let Some((journal, _)) = evidence {
            command.arg("--evidence").arg(journal);
            if let Some(marker) = &inputs.marker {
                command.arg("--trace-marker").arg(marker);
            }
        }
        let roots = self.req.roots();
        let timeout = if inputs.limit.is_some() { self.req.timeout * HANG_PATIENCE } else { self.req.timeout };
        let (exit, text) = wait_for(command, &dir, timeout, &given)?;
        self.covered.take(&cover, evidence.is_some());
        let outcome = exit.map_or(Outcome::Timeout, |code| ended(code, &text, |l| abend_line(l, &roots)));
        Ok(waited(outcome, &text))
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

/// Runs one input, with evidence and coverage going where the second argument says.
type RunInput<'a> = dyn FnMut(&Inputs, Option<(&Path, &Path)>) -> std::io::Result<Outcome> + 'a;

/// The smallest input found that still ends at the same abend, records and lines dropped, PARMs
/// cut short, and then each field put back to a value that breaks nothing, and whether the search
/// finished within its budget of runs.
fn minimize(run: &mut RunInput, feeds: &[Feed], mut inputs: Inputs, place: &(String, String, i64), budget: u32) -> (Inputs, bool) {
    let mut left = budget;
    let mut holds = |run: &mut RunInput, candidate: &Inputs| -> bool {
        if left == 0 {
            return false;
        }
        left -= 1;
        run(candidate, None).ok().and_then(|o| o.place()).as_ref() == Some(place)
    };
    for key in inputs.files.keys().cloned().collect::<Vec<_>>() {
        for k in (0..inputs.files[&key].len()).rev() {
            let mut candidate = inputs.clone();
            candidate.files.get_mut(&key).expect("fed").remove(k);
            if holds(run, &candidate) {
                inputs = candidate;
            }
        }
    }
    for key in inputs.lines.keys().cloned().collect::<Vec<_>>() {
        for k in (0..inputs.lines[&key].len()).rev() {
            let mut candidate = inputs.clone();
            candidate.lines.get_mut(&key).expect("given").remove(k);
            if holds(run, &candidate) {
                inputs = candidate;
            }
        }
    }
    for key in inputs.parms.keys().cloned().collect::<Vec<_>>() {
        let parm = inputs.parms[&key].clone();
        for keep in [0, parm.len() / 4, parm.len() / 2, 3 * parm.len() / 4].into_iter().filter(|&k| k < parm.len()) {
            let mut candidate = inputs.clone();
            candidate.parms.insert(key.clone(), parm[..keep].to_vec());
            if holds(run, &candidate) {
                inputs = candidate;
                break;
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
                if holds(run, &candidate) {
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

/// A kept run as the manifest lists it: its inputs by id, its abend and whether the same input gives
/// it again compiled with OPTIMIZE(2), the statement limit it ran under, and its journal and
/// coverage.
pub(crate) fn kept_run(ids: Vec<Value>, abend: &Outcome, optimized: bool, limit: Option<u64>, journal: String, n: usize) -> Value {
    let Outcome::Abend { code, file, line, message } = abend else { unreachable!("only an abend is kept") };
    let mut pairs = vec![
        ("input", Value::Arr(ids)),
        ("outcome", "abend".into()),
        ("abend", obj(vec![("code", code.as_str().into()), ("file", file.as_str().into()), ("line", (*line).into()), ("message", message.as_str().into()), ("optimized", optimized.into())])),
        ("journal", journal.into()),
        ("coverage", format!("coverage/{n}.json").into()),
    ];
    if let Some(limit) = limit {
        pairs.push(("limit", Value::from(limit)));
    }
    obj(pairs)
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

/// The manifest's shape, which docs/fuzz-manifest.schema.json describes. A key added keeps it; a key
/// removed, renamed or given another meaning takes a new one.
pub(crate) const MANIFEST_FORMAT: &str = "ironwork-fuzz/v1";

/// The manifest in `format`, with `extra` keys beside the ones every format has.
pub(crate) fn write_manifest_in(format: &str, out: &Path, header: &Header, inputs: Vec<Value>, tally: &Tally, runs: Vec<Value>, extra: Vec<(&str, Value)>) -> std::io::Result<()> {
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
    let mut pairs = vec![
        ("tool", "ironwork-fuzz".into()),
        ("format", format.into()),
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
    ];
    pairs.extend(extra);
    fs::write(out.join("manifest.json"), format!("{}\n", canonical(&obj(pairs))))
}

/// What a fuzz run's runs came to: a count per outcome, the first refusal's reason and how many
/// runs crashed ironwork, with the first crash.
pub(crate) struct Tally {
    pub(crate) counts: BTreeMap<&'static str, i64>,
    /// Each place or reason runs were refused at, in the order first met: the first refusal's
    /// account and how many runs it took.
    refusals: Vec<(String, String, usize)>,
    crashes: Option<(usize, String)>,
}

/// How many kinds of refusal standard error lists one by one.
const REFUSALS_LISTED: usize = 10;

impl Tally {
    pub(crate) fn new() -> Tally {
        Tally { counts: [("runs", 0), ("clean", 0), ("abend", 0), ("timeout", 0), ("refused", 0)].into_iter().collect(), refusals: Vec::new(), crashes: None }
    }

    pub(crate) fn add(&mut self, outcome: &Outcome) {
        let tally = match outcome {
            Outcome::Clean => "clean",
            Outcome::Timeout | Outcome::Waiting => "timeout",
            Outcome::Refused(why) => {
                self.refusal(why.clone(), why.clone());
                "refused"
            }
            Outcome::Crash(why) => {
                self.crashes.get_or_insert_with(|| (0, why.clone())).0 += 1;
                "refused"
            }
            Outcome::Abend { code, file, line, message } if not_the_input(code, message) => {
                self.refusal(format!("{file}:{line} {code}"), format!("{file}:{line} {code} {message}"));
                "refused"
            }
            Outcome::Abend { .. } => "abend",
        };
        for key in ["runs", tally] {
            *self.counts.get_mut(key).expect("counted") += 1;
        }
    }

    fn refusal(&mut self, kind: String, told: String) {
        match self.refusals.iter_mut().find(|(k, _, _)| *k == kind) {
            Some((_, _, n)) => *n += 1,
            None => self.refusals.push((kind, told, 1)),
        }
    }

    /// Standard error's account of the runs that say nothing of the program's input.
    pub(crate) fn report(&self) {
        for line in self.report_lines() {
            eprintln!("ironwork fuzz: {line}");
        }
    }

    /// The first refusal, then, where runs stopped at more than one place or for more than one
    /// reason, each with its count, the most frequent first; then the runs that crashed ironwork.
    fn report_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some((_, first, _)) = self.refusals.first() {
            lines.push(format!("{} runs refused; the first: {first}", self.counts["refused"] - self.crashes.as_ref().map_or(0, |c| c.0 as i64)));
        }
        if self.refusals.len() > 1 {
            let mut kinds: Vec<&(String, String, usize)> = self.refusals.iter().collect();
            kinds.sort_by_key(|(_, _, n)| std::cmp::Reverse(*n));
            lines.push(format!("the refused runs stopped in {} ways:", kinds.len()));
            lines.extend(kinds.iter().take(REFUSALS_LISTED).map(|(_, told, n)| format!("  {n} {}: {told}", if *n == 1 { "run" } else { "runs" })));
            if kinds.len() > REFUSALS_LISTED {
                lines.push(format!("  and {} more", kinds.len() - REFUSALS_LISTED));
            }
        }
        if let Some((n, first)) = &self.crashes {
            lines.push(format!("{n} runs crashed ironwork itself and are counted as refused; the first panicked at {first}"));
        }
        lines
    }

    /// The last line a fuzz run prints. A kept S322 came from a timeout and a kept CEE3501S from a
    /// refusal, so the kept runs are counted apart from the outcomes.
    pub(crate) fn summary(&self, kept: &[String], out: &Path) -> String {
        let c = &self.counts;
        let of = |code: &str| kept.iter().filter(|k| *k == code).count();
        let (hangs, chosen) = (of("S322"), of(rt::le::MODULE_NOT_FOUND));
        let kinds: Vec<String> = [(hangs, "hang", "hangs"), (chosen, "CALL the input named", "CALLs the input named")]
            .into_iter()
            .filter(|&(n, _, _)| n > 0)
            .map(|(n, one, many)| format!("{n} {}", if n == 1 { one } else { many }))
            .collect();
        let kinds = if kinds.is_empty() { String::new() } else { format!(" ({})", kinds.join(", ")) };
        format!("ironwork fuzz: {} runs, {} clean, {} abend, {} timeout, {} refused; {} kept{kinds}: {}", c["runs"], c["clean"], c["abend"], c["timeout"], c["refused"], kept.len(), out.display())
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

/// The program compiled, and the other programs of its source, those it contains among them, with
/// each program a CALL of a literal reaches in the program libraries, as `ironwork run` finds it.
fn compile(req: &Request) -> Result<(exec::Compiled, Vec<Program>), String> {
    let path = req.program.display().to_string();
    let bytes = fs::read(&req.program).map_err(|e| format!("{path}: {e}"))?;
    let text = syntax::copy::decode(&bytes);
    let own = req.program.parent().map(Path::to_path_buf).unwrap_or_default();
    let libraries = syntax::copy::Libraries::new(std::iter::once(own.clone()).chain(req.libraries.iter().cloned()).collect()).with_program(&req.program).with_compliance(numeric::Compliance::of(&req.flags));
    let mut programs = syntax::parse_all_with(&text, &libraries).map_err(|e| e.place(&path))?;
    let first = programs.remove(0);
    let compiled = exec::compile(first, &req.flags).map_err(|messages| messages.iter().map(|m| m.place(&path)).collect::<Vec<_>>().join("\n"))?;
    let mut library = exec::unit::Library { programs, dirs: std::iter::once(own).chain(req.program_dirs.iter().cloned()).collect(), copy: libraries, flags: req.flags.clone(), ..Default::default() };
    read_called(&mut library, call_targets(&compiled.program));
    Ok((compiled, library.programs))
}

/// The program libraries `ironwork run` searches for `req`'s program, holding no program yet.
fn library_of(req: &Request) -> exec::unit::Library {
    let own = req.program.parent().map(Path::to_path_buf).unwrap_or_default();
    let copy = syntax::copy::Libraries::new(std::iter::once(own.clone()).chain(req.libraries.iter().cloned()).collect()).with_program(&req.program).with_compliance(numeric::Compliance::of(&req.flags));
    exec::unit::Library { dirs: std::iter::once(own).chain(req.program_dirs.iter().cloned()).collect(), copy, flags: req.flags.clone(), ..Default::default() }
}

/// Reads into `library` the program a CALL of each of `names` finds, and the programs their CALLs
/// of a literal reach in turn.
fn read_called(library: &mut exec::unit::Library, mut names: Vec<String>) {
    let mut seen = BTreeSet::new();
    while let Some(name) = names.pop() {
        if seen.insert(name.to_ascii_uppercase())
            && let Some(p) = library.find(&name)
        {
            names.extend(call_targets(p));
        }
    }
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
    let parm = takes_parm(&compiled);
    if !compiled.program.using.is_empty() && !parm {
        return fail(format!("{} takes PROCEDURE DIVISION USING parameters that are not a PARM's halfword length and text: fuzz runs a main program", compiled.program.id));
    }
    let (feeds, sysin, others) = inputs_of(&compiled, &rest);
    for short in interface::short_calls(&file, &compiled, &rest, &req.flags) {
        eprintln!("ironwork fuzz: {short}");
    }
    if feeds.is_empty() && !sysin && !parm {
        let unfed = if others.unfed.is_empty() { String::new() } else { format!(" (not varied: {})", others.unfed.join(", ")) };
        return fail(format!("{} reads no sequential, indexed or relative file on a DD of its own{unfed}, no SYSIN and no PARM, so there is nothing to vary", compiled.program.id));
    }
    let work = match prepare(&req.out, &req.roots()) {
        Ok(w) => w,
        Err(e) => return fail(e),
    };
    let rdw = feeds.iter().filter(|f| f.variable.is_some()).map(|f| f.dd.clone()).collect();
    let mut runner = Runner { req: &req, work, count: 0, rdw, covered: RunCoverage::default() };
    let varied = Varied { feeds, lines: sysin.then(|| "SYSIN".to_string()).into_iter().collect(), parms: parm.then(|| "PARM".to_string()).into_iter().collect() };
    let novel = runner.covered.novel.clone();
    let found = drive(&req.out, req.runs, req.seed, req.hang_limit, &varied, &novel, &mut |inputs, evidence| runner.run(inputs, &others, evidence));
    let _ = fs::remove_dir_all(&runner.work);
    let found = match found {
        Ok(f) => f,
        Err(e) => return fail(e),
    };
    let header = Header { seed: req.seed, clock: &req.clock, file: &file, id: &compiled.program.id, root: &req.root, roots: &req.roots(), entry: "run" };
    if let Err(e) = runner.covered.write(&req.out).and_then(|key| write_manifest_in(MANIFEST_FORMAT, &req.out, &header, found.inputs, &found.tally, found.runs, vec![key])) {
        return fail(format!("-o {}: {e}", req.out.display()));
    }
    if !others.unfed.is_empty() {
        eprintln!("ironwork fuzz: not varied, given empty: {}", others.unfed.join(", "));
    }
    if !others.ungiven.is_empty() {
        eprintln!("ironwork fuzz: no --dd can carry these names, so they are given no data set: {}", others.ungiven.join(", "));
    }
    found.tally.report();
    if let Some((code, file, line)) = found.baseline {
        eprintln!("ironwork fuzz: the program ends with {code} at {file}:{line} on empty input; that abend is not kept");
    }
    println!("{}", found.tally.summary(&found.codes, &req.out));
    ExitCode::SUCCESS
}

/// What a fuzz run varies, each by the name its inputs go under: the fed data sets, the inputs read
/// as lines, and the PARMs.
struct Varied {
    feeds: Vec<Feed>,
    lines: Vec<String>,
    parms: Vec<String>,
}

/// The kept abends' inputs and runs as the manifest lists them and their codes, what the runs came
/// to, and the abend the run on empty input gave.
struct Found {
    inputs: Vec<Value>,
    runs: Vec<Value>,
    codes: Vec<String>,
    tally: Tally,
    baseline: Option<(String, String, i64)>,
}

/// The loop every entry shares: a run on empty input, whose abend is no input's doing and is not
/// kept; `runs` generated inputs; then each new abend once, on the smallest input that still gives
/// it, run with evidence and coverage in `out`, and once more compiled with OPTIMIZE(2).
fn drive(out: &Path, runs: u32, seed: u64, hang_limit: u64, varied: &Varied, novel: &std::cell::Cell<bool>, run: &mut RunInput) -> Result<Found, String> {
    let evidence = out.join("evidence");
    let coverage = out.join("coverage");
    let started = |e: std::io::Error| format!("a run could not start: {e}");
    let empty = Inputs {
        files: varied.feeds.iter().map(|f| (f.dd.clone(), Vec::new())).collect(),
        lines: varied.lines.iter().map(|k| (k.clone(), Vec::new())).collect(),
        parms: varied.parms.iter().map(|k| (k.clone(), Vec::new())).collect(),
        limit: Some(hang_limit),
        ..Default::default()
    };
    let ended_empty = run(&empty, None).map_err(started)?;
    let baseline = ended_empty.place();
    let mut rng = Rng(seed.max(1));
    let mut tally = Tally::new();
    let mut kept: Vec<((String, String, i64), Inputs, Outcome)> = Vec::new();
    let mut chosen: Vec<((String, String, i64), String, Inputs)> = Vec::new();
    let mut corpus: Vec<Inputs> = Vec::new();
    for _ in 0..runs {
        // Half the runs change an input that reached what no earlier run did, once there is one.
        let drawn = if corpus.is_empty() || rng.below(2) == 0 {
            generate(&mut rng, varied)
        } else {
            let parent = parent_of(&mut rng, corpus.len());
            mutate(&mut rng, varied, &corpus[parent])
        };
        let inputs = Inputs { limit: Some(hang_limit), ..drawn };
        let outcome = run(&inputs, None).map_err(started)?;
        if novel.get() && corpus.len() < CORPUS_LIMIT {
            corpus.push(inputs.clone());
        }
        tally.add(&outcome);
        let found = match &outcome {
            // A CALL of a missing program is a finding only where the name came from the input,
            // which a marker in the name's place shows at the end.
            Outcome::Abend { code, file, line, message } if missing_module(code, message).is_some() => {
                let place = (rt::le::MODULE_NOT_FOUND.to_string(), file.clone(), *line);
                if let Some(name) = missing_module(code, message).filter(|n| named_in(&inputs, n))
                    && chosen.len() < CHOSEN_CHECKS
                    && !chosen.iter().any(|(p, _, _)| *p == place)
                {
                    chosen.push((place, name.to_string(), inputs));
                }
                None
            }
            // An S322 is a finding only in a loop the empty input does not run.
            outcome => outcome.place().filter(|p| p.0 != "S322" || !same_loop(outcome, &ended_empty)).map(|p| (p, inputs, outcome.clone())),
        };
        if let Some((place, inputs, outcome)) = found
            && Some(&place) != baseline.as_ref()
            && !kept.iter().any(|(p, _, o)| *p == place || same_loop(o, &outcome))
        {
            kept.push((place, inputs, outcome));
        }
    }

    let (mut inputs_out, mut runs_out, mut codes) = (Vec::new(), Vec::new(), Vec::new());
    let mut n = 0;
    for (place, found, _) in kept {
        let hang = place.0 == "S322";
        let budget = if hang { 10 } else { 200 };
        let (small, minimized) = minimize(run, &varied.feeds, found, &place, budget);
        let before = journals(&evidence);
        let cover = coverage.join(format!("{n}.json"));
        let outcome = run(&small, Some((&evidence, &cover))).map_err(started)?;
        let came_again = outcome.place().as_ref() == Some(&place);
        let journal = journals(&evidence).into_iter().find(|j| !before.contains(j)).filter(|_| came_again);
        match journal {
            Some(journal) => {
                let optimized = place.0 == "S322" || gives_again(run, &small, &place).map_err(started)?;
                runs_out.push(kept_run(listed(&small, varied, n, minimized, &mut inputs_out), &outcome, optimized, small.limit.filter(|_| hang), journal, n));
                codes.push(place.0.clone());
            }
            None => {
                let why = if came_again { "wrote no journal".to_string() } else { outcome.told() };
                eprintln!("ironwork fuzz: {} at {}:{} is not kept: its run on the smallest input {why}", place.0, place.1, place.2);
            }
        }
        n += 1;
    }
    for (place, name, found) in chosen {
        let marker = marker_for(&name);
        let marked = Inputs { marker: Some(marker.clone()), ..renamed(&found, &name, &marker) };
        let before = journals(&evidence);
        let cover = coverage.join(format!("{n}.json"));
        let outcome = run(&marked, Some((&evidence, &cover))).map_err(started)?;
        let named = matches!(&outcome, Outcome::Abend { code, file, line, message } if *file == place.1 && *line == place.2 && missing_module(code, message) == Some(marker.as_str()));
        let journal = journals(&evidence).into_iter().find(|j| !before.contains(j)).filter(|j| named && marker_reached(&evidence, j, place.2));
        match journal {
            Some(journal) => {
                runs_out.push(kept_run(listed(&marked, varied, n, false, &mut inputs_out), &outcome, true, None, journal, n));
                codes.push(place.0.clone());
            }
            None => eprintln!("ironwork fuzz: CEE3501S at {}:{} is not kept: its run with {marker} in place of {name} did not show the CALL took the name from the input", place.1, place.2),
        }
        n += 1;
    }
    Ok(Found { inputs: inputs_out, runs: runs_out, codes, tally, baseline })
}

/// Whether `inputs` end at `place` again with the program compiled at OPTIMIZE(2), where IBM may
/// compare some invalid data by its bytes rather than end in a data exception (assumption C262).
fn gives_again(run: &mut RunInput, inputs: &Inputs, place: &(String, String, i64)) -> std::io::Result<bool> {
    Ok(run(&Inputs { optimized: true, ..inputs.clone() }, None)?.place().as_ref() == Some(place))
}

/// Lists each of a kept run's inputs in `out` as the manifest gives them, and returns their ids.
fn listed(small: &Inputs, varied: &Varied, n: usize, minimized: bool, out: &mut Vec<Value>) -> Vec<Value> {
    let mut ids = Vec::new();
    let mut list = |key: &str, kind: &str, bytes: &[u8]| {
        let id = format!("r{n}-{key}");
        out.push(input(&id, kind, key, bytes, minimized));
        ids.push(Value::from(id));
    };
    for (key, records) in &small.files {
        list(key, "dd", &data_set(records, varied.feeds.iter().any(|f| f.dd == *key && f.variable.is_some())));
    }
    for (key, lines) in &small.lines {
        list(key, if key == "SYSIN" || key.ends_with(".SYSIN") { "sysin" } else { "dd" }, &sysin_text(lines));
    }
    for (key, text) in &small.parms {
        list(key, "parm", text);
    }
    ids
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
    fn refusals_are_reported_by_where_they_stopped_the_most_frequent_first() {
        let abend = |code: &str, line: i64, message: &str| Outcome::Abend { code: code.into(), file: "P.cbl".into(), line, message: message.into() };
        let mut tally = Tally::new();
        tally.add(&abend("U4038", 9, "IGZ0035S There was an unsuccessful OPEN or CLOSE of file IN-FILE in program P. Neither FILE STATUS nor an ERROR declarative were specified. The status code was 35. (no DD INDD was given)"));
        for _ in 0..3 {
            tally.add(&abend("U4038", 14, "CEE3501S The module LOGGER was not found."));
        }
        tally.add(&abend("S0C7", 20, "data exception"));
        tally.add(&Outcome::Clean);
        assert_eq!(
            tally.report_lines(),
            [
                "4 runs refused; the first: P.cbl:9 U4038 IGZ0035S There was an unsuccessful OPEN or CLOSE of file IN-FILE in program P. Neither FILE STATUS nor an ERROR declarative were specified. The status code was 35. (no DD INDD was given)",
                "the refused runs stopped in 2 ways:",
                "  3 runs: P.cbl:14 U4038 CEE3501S The module LOGGER was not found.",
                "  1 run: P.cbl:9 U4038 IGZ0035S There was an unsuccessful OPEN or CLOSE of file IN-FILE in program P. Neither FILE STATUS nor an ERROR declarative were specified. The status code was 35. (no DD INDD was given)",
            ]
        );
        let mut one_way = Tally::new();
        one_way.add(&abend("IRONWORK", 3, "CALL X: IEW2456E SYMBOL X UNRESOLVED"));
        assert_eq!(one_way.report_lines().len(), 1);
    }

    #[test]
    fn no_sysin_card_starts_as_the_end_of_in_stream_data() {
        let mut rng = Rng(11);
        for _ in 0..20_000 {
            let card = sysin_line(&mut rng);
            assert!(!card.starts_with(b"/*") && !card.starts_with(b"//"), "{}", String::from_utf8_lossy(&card));
        }
    }

    #[test]
    fn two_s322s_are_one_loop_in_one_file_with_a_line_in_common() {
        let s322 = |file: &str, line: i64, lines: &str| Outcome::Abend { code: "S322".into(), file: file.into(), line, message: format!("the run reached its statement limit, as a step past its TIME= ends, in the loop over lines {lines}") };
        assert_eq!(loop_lines("…, in the loop over lines 12, 14 and 3 more"), [12, 14].into());
        assert!(same_loop(&s322("P.cbl", 30, "30, 31, 32"), &s322("P.cbl", 25, "25, 26, 30")));
        assert!(!same_loop(&s322("P.cbl", 30, "30, 31"), &s322("P.cbl", 40, "40, 41")));
        assert!(!same_loop(&s322("P.cbl", 30, "30"), &s322("Q.cbl", 30, "30")));
        assert!(!same_loop(&s322("P.cbl", 30, "30"), &Outcome::Timeout));
    }

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
        assert!(matches!(ended(Some(255), panic, |_| None), Outcome::Crash(why) if why == "crates/exec/src/x.rs:9:5: index out of bounds"));
        assert!(matches!(ended(Some(255), "", |_| None), Outcome::Crash(_)));
        assert!(matches!(ended(Some(245), "ironwork: no such file\n\n", |_| None), Outcome::Refused(why) if why == "ironwork: no such file"));
        assert!(matches!(ended(None, "", |_| None), Outcome::Refused(_)));
    }

    #[test]
    fn a_status_below_240_is_the_program_s_return_code_whatever_it_wrote() {
        for code in [0, 2, 12, 16, 239] {
            assert!(matches!(ended(Some(code), "ironwork: bad DD\nthread 'x' panicked at y\n", |_| None), Outcome::Clean), "{code}");
        }
        assert!(matches!(ended(Some(246), "ironwork: bad DD\nusage: ...\n", |_| None), Outcome::Refused(why) if why == "ironwork: bad DD"));
    }

    #[test]
    fn an_abend_is_told_by_its_line_and_one_ironwork_does_not_run_too() {
        let line = "P.cbl:12:8: ABEND S0C7: data exception\n";
        assert!(matches!(ended(Some(240), line, |l| abend_line(l, &[])), Outcome::Abend { code, line: 12, .. } if code == "S0C7"));
        let line = "P.cbl:4:8: ABEND EXEC: EXEC DLI GN was reached\n";
        assert!(matches!(ended(Some(244), line, |l| abend_line(l, &[])), Outcome::Abend { code, .. } if code == "EXEC"));
        assert!(matches!(ended(Some(244), "ironwork: job.jcl: line 2: PGM=ICETOOL is not supported yet\n", |l| abend_line(l, &[])), Outcome::Refused(_)));
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
        let shapes = vec![Shape { size: 4, fields: vec![alnum(0, 4)], values: BTreeMap::new() }, Shape { size: 10, fields: vec![alnum(0, 4), alnum(4, 6)], values: BTreeMap::new() }];
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
        let shapes = vec![Shape { size: 3, fields: vec![alnum(0, 2), alnum(2, 1)], values: BTreeMap::new() }];
        let varied = Varied { feeds: vec![Feed { dd: "KS".into(), length: 3, variable: None, shapes, keys: vec![(0, 2), (2, 1)], relative: false }], lines: Vec::new(), parms: Vec::new() };
        let mut rng = Rng(3);
        for _ in 0..200 {
            let records = &generate(&mut rng, &varied).files["KS"];
            assert!(records.windows(2).all(|w| w[0][..2] < w[1][..2]));
            assert_eq!(records.iter().map(|r| r[2]).collect::<BTreeSet<_>>().len(), records.len());
        }
    }
}
