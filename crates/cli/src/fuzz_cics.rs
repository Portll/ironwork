//! `ironwork fuzz --cics`: runs a CICS program as the first program of a task many times, on a
//! generated COMMAREA and on an operator's generated input at a scripted 3270 terminal, and keeps
//! each distinct abend with the smallest input that still causes it, its journal and its coverage,
//! as `ironwork fuzz` does for a batch program (docs/evidence.md §5).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use exec::evidence::Value;
use rt::bms::{Map, Protection};
use rt::vocab::BinOp;
use syntax::ast::{ExecArg, ExecKind, Expr, Literal, Operand, Ref, Stmt};

use crate::fuzz::{self, Field, Header, Outcome, Rng, SPACE, Tally, abend_line, elementary, field_bytes, finish, from_root, input, journals, kept_run, neutral, prepare, write_manifest};

pub struct Request {
    pub fuzz: fuzz::Request,
    /// The cics flags every run is given: the task's identity, its transactions, files and queues.
    pub options: Vec<(String, String)>,
}

/// Abends that say what the task's surroundings lack rather than what its input did: a program no
/// library holds (PGMIDERR), a file no --file defines (FILENOTFOUND), and a remote system or a
/// transaction the region does not have (SYSIDERR, TRANSIDERR).
const NOT_THE_INPUT: &[&str] = &["AEI0", "AEIL", "AEYQ", "AEI1"];

/// A DFHCOMMAREA that is a table of bytes EIBCALEN sizes declares up to 32,767 of them; a COMMAREA
/// generated for one is no longer than this.
const COMMAREA_CAP: usize = 256;

/// The AID keys an operator can press.
const KEYS: [&str; 29] = [
    "ENTER", "CLEAR", "PA1", "PA2", "PA3", "PF1", "PF2", "PF3", "PF4", "PF5", "PF6", "PF7", "PF8", "PF9", "PF10", "PF11", "PF12", "PF13", "PF14", "PF15", "PF16", "PF17", "PF18", "PF19", "PF20",
    "PF21", "PF22", "PF23", "PF24",
];

/// The COMMAREA the program reads, by its length and fields.
struct Commarea {
    length: usize,
    fields: Vec<Field>,
}

/// An unprotected field of a map, in the order the Tab key reaches it.
#[derive(Clone, Copy)]
struct Slot {
    length: usize,
    numeric: bool,
}

/// A field of a screen whose map is unknown.
const ANY_FIELD: Slot = Slot { length: 12, numeric: false };

/// What the program does at its terminal: whether it uses one at all and reads it, the fields of
/// each map it RECEIVEs that its mapset gives, the mapsets no library holds, the transaction ids
/// RETURN names where the source gives them, and the AID keys the program names.
#[derive(Default)]
struct Terminal {
    used: bool,
    reads: bool,
    maps: Vec<Vec<Slot>>,
    missing: BTreeSet<String>,
    transids: BTreeSet<String>,
    keys: Vec<&'static str>,
}

/// One turn at the terminal: text typed into some of the screen's unprotected fields, by their Tab
/// order, then an AID key.
#[derive(Clone)]
struct Turn {
    fields: Vec<Option<String>>,
    key: &'static str,
}

/// What one task is given: its COMMAREA, and the operator's turns when the program uses a terminal.
#[derive(Clone, Default)]
struct Inputs {
    commarea: Option<Vec<u8>>,
    turns: Option<Vec<Turn>>,
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

/// An option's argument as text: a literal, or a data item's VALUE.
fn named(layout: &exec::layout::Layout, arg: Option<&ExecArg>) -> Option<String> {
    let text = match arg? {
        ExecArg::Operand(Operand::Literal(Literal::Alnum(s))) => s.clone(),
        ExecArg::Operand(Operand::Ref(r)) => match layout.resolve(&r.name, &r.qualifiers, r.pos) {
            Ok(exec::layout::Resolved::Item(i)) => match &layout.items[i].value {
                Some(Literal::Alnum(s)) => s.clone(),
                _ => return None,
            },
            _ => return None,
        },
        _ => return None,
    };
    Some(text.trim().to_ascii_uppercase()).filter(|t| !t.is_empty())
}

fn item_of(layout: &exec::layout::Layout, r: &Ref) -> Option<usize> {
    match layout.resolve(&r.name, &r.qualifiers, r.pos) {
        Ok(exec::layout::Resolved::Item(i)) => Some(i),
        _ => None,
    }
}

/// An expression's value where it is a number, a LENGTH OF, or a sum or difference of those.
fn constant(layout: &exec::layout::Layout, e: &Expr) -> Option<i64> {
    match e {
        Expr::Operand(Operand::Literal(Literal::Number(n))) => n.parse().ok(),
        Expr::Operand(Operand::LengthOf(r)) => item_of(layout, r).map(|i| i64::from(layout.items[i].size)),
        Expr::Bin(a, BinOp::Add, b) => Some(constant(layout, a)? + constant(layout, b)?),
        Expr::Bin(a, BinOp::Sub, b) => Some(constant(layout, a)? - constant(layout, b)?),
        _ => None,
    }
}

/// DFHCOMMAREA's fields, or, where DFHCOMMAREA is a table EIBCALEN sizes, the fields of each item
/// the program MOVEs a part of it into, placed where that part starts (after the one before where
/// its start is not a constant); None for the one byte the CICS translator declares for a program
/// that declares none.
fn commarea_of(compiled: &exec::Compiled, all: &[&Stmt], upper_source: &str) -> Option<Commarea> {
    let layout = &compiled.layout;
    let is_commarea = |name: &str| name.eq_ignore_ascii_case("DFHCOMMAREA");
    let root = *layout.linkage_roots.iter().find(|&&i| layout.items[i].name.as_deref().is_some_and(is_commarea))?;
    let item = &layout.items[root];
    if item.children.is_empty() && item.size == 1 && !names_word(upper_source, "DFHCOMMAREA") {
        return None;
    }
    if item.odo.is_empty() {
        let length = item.size as usize;
        return Some(Commarea { length, fields: elementary(layout, root, item.offset, length) });
    }
    let (mut length, mut fields) = (0, Vec::<Field>::new());
    for s in all {
        let Stmt::Move { from: Operand::Ref(from), to, .. } = s else { continue };
        let Some(target) = to.first().and_then(|r| item_of(layout, r)).filter(|_| is_commarea(&from.name)) else { continue };
        let refmod = from.refmod.as_ref();
        let at = match refmod {
            None => 0,
            Some(m) => constant(layout, &m.start).and_then(|s| usize::try_from(s - 1).ok()).unwrap_or(length),
        };
        let mut size = layout.items[target].size as usize;
        if let Some(n) = refmod.and_then(|m| m.length.as_deref()).and_then(|l| constant(layout, l)).and_then(|n| usize::try_from(n).ok()) {
            size = size.min(n);
        }
        for f in elementary(layout, target, layout.items[target].offset, size) {
            let f = Field { offset: f.offset + at, ..f };
            if !fields.iter().any(|p| p.offset < f.offset + f.size && f.offset < p.offset + p.size) {
                fields.push(f);
            }
        }
        length = length.max(at + size);
    }
    if length == 0 {
        length = (item.size as usize).min(COMMAREA_CAP);
        fields = elementary(layout, root, item.offset, length);
    }
    (length > 0).then_some(Commarea { length, fields })
}

/// A map's unprotected fields in buffer order, each OCCURS copy its own.
fn slots(map: &Map) -> Vec<Slot> {
    let mut placed: Vec<(usize, usize, Slot)> = Vec::new();
    for f in map.fields.iter().filter(|f| f.attrb.protection == Protection::Unprot) {
        let copies = if f.group.is_none() { f.occurs.max(1) } else { 1 };
        for occurrence in 0..copies {
            let row = usize::from(map.line) + usize::from(f.line);
            let column = usize::from(map.column) + usize::from(f.column) + usize::from(occurrence) * (usize::from(f.length) + 1);
            placed.push((row, column, Slot { length: usize::from(f.length), numeric: f.attrb.numeric }));
        }
    }
    placed.sort_by_key(|&(row, column, _)| (row, column));
    placed.into_iter().map(|(_, _, slot)| slot).collect()
}

/// Whether `text` names `word` as a whole word: DFHPF1 is not DFHPF12.
fn names_word(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(at, _)| !text[at + word.len()..].starts_with(|c: char| c.is_ascii_alphanumeric() || c == '-'))
}

fn terminal_of(compiled: &exec::Compiled, all: &[&Stmt], upper_source: &str, libraries: &syntax::copy::Libraries) -> Terminal {
    let layout = &compiled.layout;
    let mut t = Terminal::default();
    let mut received = BTreeSet::new();
    for block in all.iter().filter_map(|s| if let Stmt::Exec(b) = s { Some(b) } else { None }).filter(|b| b.kind == ExecKind::Cics) {
        let option = |name: &str| block.options.iter().find(|(n, _)| n == name).map(|(_, a)| a.as_ref());
        let command = block.command.as_str();
        match command {
            "RECEIVE" | "CONVERSE" => {
                t.reads = true;
                if let Some(map) = option("MAP").and_then(|a| named(layout, a)) {
                    let set = option("MAPSET").and_then(|a| named(layout, a)).unwrap_or_else(|| map.clone());
                    received.insert((set, map));
                }
            }
            "RETURN" => t.transids.extend(option("TRANSID").and_then(|a| named(layout, a))),
            "HANDLE AID" => t.keys.extend(block.options.iter().filter_map(|(n, _)| KEYS.iter().find(|k| **k == n.as_str()).copied())),
            _ => {}
        }
        t.used |= matches!(command, "RECEIVE" | "CONVERSE" | "SEND" | "SEND MAP" | "SEND TEXT" | "SEND CONTROL" | "SEND PAGE");
    }
    for (set, map) in received {
        match syntax::bms::find_mapset(libraries, &set) {
            Some(Ok(mapset)) => t.maps.extend(mapset.maps.iter().filter(|m| m.name == map).map(slots)),
            _ => {
                t.missing.insert(set);
            }
        }
    }
    t.keys.extend(KEYS.iter().filter(|k| names_word(upper_source, &format!("DFH{k}"))));
    t.keys.sort();
    t.keys.dedup();
    t
}

fn commarea(rng: &mut Rng, shape: &Commarea) -> Option<Vec<u8>> {
    if rng.below(10) < 3 {
        return None;
    }
    let mut bytes = vec![SPACE; shape.length];
    for &f in &shape.fields {
        bytes[f.offset..f.offset + f.size].copy_from_slice(&field_bytes(rng, f)[..f.size]);
    }
    // EIBCALEN short of what the program expects.
    if shape.length > 1 && rng.below(10) == 0 {
        bytes.truncate(1 + rng.below(shape.length - 1));
    }
    Some(bytes)
}

/// Text an operator types into a field: mostly within its length, digits more often where it is
/// numeric.
fn typed(rng: &mut Rng, slot: Slot) -> String {
    let pick: &[u8] = match rng.below(5) {
        0 | 1 if slot.numeric => b"0123456789",
        0 => b"0123456789",
        1 => b"ABCDEFGHIJKLMNOPQRSTUVWXYZ ",
        2 => b"*-+. ,/0123456789",
        3 => b"abcdefghijklmnopqrstuvwxyz0123456789 ",
        _ => b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 ",
    };
    let length = if rng.below(4) == 0 { slot.length } else { 1 + rng.below(slot.length.max(1)) };
    (0..length).map(|_| char::from(pick[rng.below(pick.len())])).collect()
}

fn turn(rng: &mut Rng, terminal: &Terminal) -> Turn {
    let slots = if terminal.maps.is_empty() { vec![ANY_FIELD; 1 + rng.below(4)] } else { terminal.maps[rng.below(terminal.maps.len())].clone() };
    let fields = slots.iter().map(|&s| (rng.below(10) >= 3).then(|| typed(rng, s))).collect();
    let key = match rng.below(10) {
        0..=5 => "ENTER",
        6..=8 if !terminal.keys.is_empty() => terminal.keys[rng.below(terminal.keys.len())],
        _ => KEYS[rng.below(KEYS.len())],
    };
    Turn { fields, key }
}

fn generate(rng: &mut Rng, shape: Option<&Commarea>, terminal: &Terminal) -> Inputs {
    let commarea = shape.and_then(|s| commarea(rng, s));
    let turns = terminal.used.then(|| if terminal.reads || !terminal.transids.is_empty() { (0..1 + rng.below(4)).map(|_| turn(rng, terminal)).collect() } else { Vec::new() });
    Inputs { commarea, turns }
}

/// The operator's turns as a screen script: each field reached from the first by Home and Tab,
/// its text typed there, then the turn's AID key.
fn script(turns: &[Turn]) -> String {
    let mut out = String::new();
    for t in turns {
        for (k, text) in t.fields.iter().enumerate() {
            if let Some(text) = text {
                out.push_str("home\n");
                out.push_str(&"tab\n".repeat(k));
                out.push_str(&format!("string {text}\n"));
            }
        }
        out.push_str(t.key);
        out.push('\n');
    }
    out
}

/// An abend line as `ironwork fuzz` reads one, an abend of the task's surroundings told as a refusal.
fn abend(line: &str, roots: &[PathBuf]) -> Option<Outcome> {
    abend_line(line, roots).map(|o| match o {
        Outcome::Abend { code, file, line, message } if NOT_THE_INPUT.contains(&code.as_str()) => Outcome::Refused(format!("{file}:{line} {code} {message}")),
        o => o,
    })
}

/// A --file the runs are given: its name, its data set, and the rest of its spec. Each run gets its
/// own copy of the data set, since a task writes its files back when it ends.
struct GivenFile {
    name: String,
    data_set: PathBuf,
    spec: String,
}

/// One task as its own process, so a task that loops is stopped and one that fails stops nothing else.
struct Runner<'a> {
    req: &'a Request,
    roots: Vec<PathBuf>,
    work: PathBuf,
    count: u64,
    passed: Vec<(String, String)>,
    files: Vec<GivenFile>,
    queues: Vec<String>,
}

impl Runner<'_> {
    fn run(&mut self, inputs: &Inputs, evidence: Option<(&Path, &Path)>) -> std::io::Result<Outcome> {
        self.count += 1;
        let dir = self.work.join(format!("run-{}", self.count));
        fs::create_dir_all(&dir)?;
        let f = &self.req.fuzz;
        let mut command = Command::new(std::env::current_exe()?);
        command.arg("cics").arg(&f.program).arg("--clock").arg(&f.clock);
        command.args(&f.flags);
        for d in &f.libraries {
            command.arg("-I").arg(d);
        }
        for d in &f.program_dirs {
            command.arg("-L").arg(d);
        }
        for (name, value) in &self.passed {
            command.arg(name).arg(value);
        }
        let mut given: Vec<(&str, PathBuf)> = Vec::new();
        for (k, file) in self.files.iter().enumerate() {
            let path = dir.join(format!("file{k}"));
            if file.data_set.exists() {
                fs::copy(&file.data_set, &path)?;
            }
            command.arg("--file").arg(format!("{}={},{}", file.name, path.display(), file.spec));
            given.push((file.name.as_str(), path));
        }
        for (k, queue) in self.queues.iter().enumerate() {
            let path = dir.join(format!("td{k}"));
            command.arg("--td").arg(format!("{queue}={}", path.display()));
            given.push((queue.as_str(), path));
        }
        if let Some(bytes) = &inputs.commarea {
            let path = dir.join("commarea");
            fs::write(&path, bytes)?;
            command.arg("--commarea").arg(&path);
            given.push(("DFHCOMMAREA", path));
        }
        if let Some(turns) = &inputs.turns {
            let path = dir.join("screens");
            fs::write(&path, script(turns))?;
            command.arg("--screens").arg(&path);
            given.push(("the screen script", path));
        }
        if let Some((journal, coverage)) = evidence {
            command.arg("--evidence").arg(journal).arg("--coverage").arg(coverage);
        }
        let roots = &self.roots;
        finish(command, &dir, f.timeout, &given, |l| abend(l, roots))
    }
}

/// The smallest input found that still ends at the same abend: turns dropped, the COMMAREA
/// dropped or each of its fields put back to a value that breaks nothing, then each typed field
/// left untyped; and whether the search finished within its budget of runs.
fn minimize(runner: &mut Runner, shape: Option<&Commarea>, mut inputs: Inputs, place: &(String, String, i64), budget: u32) -> (Inputs, bool) {
    let mut left = budget;
    let mut holds = |runner: &mut Runner, candidate: &Inputs| -> bool {
        if left == 0 {
            return false;
        }
        left -= 1;
        runner.run(candidate, None).ok().and_then(|o| o.place()).as_ref() == Some(place)
    };
    for k in (0..inputs.turns.as_ref().map_or(0, Vec::len)).rev() {
        let mut candidate = inputs.clone();
        candidate.turns.as_mut().expect("turns").remove(k);
        if holds(runner, &candidate) {
            inputs = candidate;
        }
    }
    if inputs.commarea.is_some() {
        let candidate = Inputs { commarea: None, ..inputs.clone() };
        if holds(runner, &candidate) {
            inputs = candidate;
        }
    }
    let size = inputs.commarea.as_ref().map_or(0, Vec::len);
    for &f in shape.map_or(&[][..], |s| &s.fields).iter().filter(|f| f.offset + f.size <= size) {
        let quiet = neutral(f);
        if inputs.commarea.as_ref().is_some_and(|c| c[f.offset..f.offset + f.size] == quiet[..]) {
            continue;
        }
        let mut candidate = inputs.clone();
        candidate.commarea.as_mut().expect("commarea")[f.offset..f.offset + f.size].copy_from_slice(&quiet);
        if holds(runner, &candidate) {
            inputs = candidate;
        }
    }
    let typed: Vec<(usize, usize)> = inputs.turns.iter().flatten().enumerate().flat_map(|(t, turn)| turn.fields.iter().enumerate().filter(|(_, f)| f.is_some()).map(move |(k, _)| (t, k))).collect();
    for (t, k) in typed {
        let mut candidate = inputs.clone();
        candidate.turns.as_mut().expect("turns")[t].fields[k] = None;
        if holds(runner, &candidate) {
            inputs = candidate;
        }
    }
    (inputs, left > 0)
}

/// The program compiled, and its source text and copy libraries.
fn compile(req: &fuzz::Request) -> Result<(exec::Compiled, String, syntax::copy::Libraries), String> {
    let path = req.program.display().to_string();
    let bytes = fs::read(&req.program).map_err(|e| format!("{path}: {e}"))?;
    let text = syntax::copy::decode(&bytes);
    let own = req.program.parent().map(Path::to_path_buf).unwrap_or_default();
    let libraries = syntax::copy::Libraries::new(std::iter::once(own).chain(req.libraries.iter().cloned()).collect()).with_program(&req.program);
    let mut programs = syntax::parse_all_with(&text, &libraries).map_err(|e| e.place(&path))?;
    let compiled = exec::compile(programs.remove(0), &req.flags).map_err(|messages| messages.iter().map(|m| m.place(&path)).collect::<Vec<_>>().join("\n"))?;
    Ok((compiled, text, libraries))
}

pub fn run(req: Request) -> ExitCode {
    let fail = |message: String| {
        eprintln!("ironwork fuzz: {message}");
        ExitCode::from(2)
    };
    let f = &req.fuzz;
    let (compiled, source, libraries) = match compile(f) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(12);
        }
    };
    let Some(file) = from_root(&f.program, &f.root) else {
        return fail(format!("{} is not under --root {}", f.program.display(), f.root.display()));
    };
    if let Some((name, _)) = req.options.iter().find(|(n, _)| matches!(n.as_str(), "--commarea" | "--commarea-out" | "--screens" | "--serve")) {
        return fail(format!("fuzz --cics makes each task's COMMAREA and terminal input itself, so it takes no {name}"));
    }
    let mut all = Vec::new();
    for p in &compiled.program.paragraphs {
        statements(&p.statements, &mut all);
    }
    let upper_source = source.to_ascii_uppercase();
    let shape = commarea_of(&compiled, &all, &upper_source);
    let terminal = terminal_of(&compiled, &all, &upper_source, &libraries);
    if shape.is_none() && !terminal.reads {
        return fail(format!("{} declares no DFHCOMMAREA and reads no terminal, so there is nothing to vary", compiled.program.id));
    }

    let mut passed = Vec::new();
    let (mut files, mut queues) = (Vec::new(), Vec::new());
    for (name, value) in &req.options {
        match name.as_str() {
            "--file" => {
                let Some((file, rest)) = value.split_once('=') else { return fail(format!("--file {value}: expected NAME=path,...")) };
                let (data_set, spec) = rest.split_once(',').unwrap_or((rest, ""));
                files.push(GivenFile { name: file.to_string(), data_set: data_set.into(), spec: spec.to_string() });
            }
            "--td" => match value.split_once('=') {
                Some((queue, _)) => queues.push(queue.to_string()),
                None => return fail(format!("--td {value}: expected QUEUE=path")),
            },
            _ => passed.push((name.clone(), value.clone())),
        }
    }
    // A pseudo-conversation goes on only when RETURN TRANSID names a transaction the task's region
    // runs this program for.
    let inferred = (terminal.transids.len() == 1 && !req.options.iter().any(|(n, _)| n == "--transid" || n == "--csd")).then(|| terminal.transids.first().cloned()).flatten();
    if let Some(t) = &inferred {
        passed.push(("--transid".into(), t.clone()));
    }
    let termid = req.options.iter().rev().find(|(n, _)| n == "--termid").map_or_else(|| "TERM".to_string(), |(_, v)| v.to_ascii_uppercase());

    let roots: Vec<PathBuf> = std::iter::once(f.program.parent().map(Path::to_path_buf).unwrap_or_default()).chain(f.libraries.iter().cloned()).chain(f.program_dirs.iter().cloned()).collect();
    let evidence = f.out.join("evidence");
    let coverage = f.out.join("coverage");
    let work = match prepare(&f.out, &roots) {
        Ok(w) => w,
        Err(e) => return fail(e),
    };
    let mut runner = Runner { req: &req, roots: roots.clone(), work, count: 0, passed, files, queues };

    // What the task does with no COMMAREA and no operator input is no input's doing, so an abend it
    // gives then is not kept.
    let empty = Inputs { commarea: None, turns: terminal.used.then(Vec::new) };
    let baseline = match runner.run(&empty, None) {
        Ok(o) => o.place(),
        Err(e) => return fail(format!("a run could not start: {e}")),
    };
    let mut rng = Rng(f.seed.max(1));
    let mut tally = Tally::new();
    let mut kept: Vec<((String, String, i64), Inputs)> = Vec::new();
    for _ in 0..f.runs {
        let inputs = generate(&mut rng, shape.as_ref(), &terminal);
        let outcome = match runner.run(&inputs, None) {
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
        let (small, minimized) = minimize(&mut runner, shape.as_ref(), found, &place, 200);
        let before = journals(&evidence);
        let cover = coverage.join(format!("{n}.json"));
        let outcome = match runner.run(&small, Some((&evidence, &cover))) {
            Ok(o) => o,
            Err(e) => return fail(format!("a run could not start: {e}")),
        };
        let came_again = outcome.place().as_ref() == Some(&place);
        let journal = journals(&evidence).into_iter().find(|j| !before.contains(j)).filter(|_| came_again);
        let Some(journal) = journal else {
            let why = if came_again { "wrote no journal".to_string() } else { outcome.told() };
            eprintln!("ironwork fuzz: {} at {}:{} is not kept: its task on the smallest input {why}", place.0, place.1, place.2);
            continue;
        };
        let mut ids = Vec::new();
        if let Some(bytes) = &small.commarea {
            let id = format!("r{n}-DFHCOMMAREA");
            inputs_out.push(input(&id, "commarea", "DFHCOMMAREA", bytes, minimized));
            ids.push(Value::from(id));
        }
        if let Some(turns) = small.turns.as_ref().filter(|t| !t.is_empty()) {
            let id = format!("r{n}-{termid}");
            inputs_out.push(input(&id, "terminal", &termid, script(turns).as_bytes(), minimized));
            ids.push(Value::from(id));
        }
        runs_out.push(kept_run(ids, &outcome, journal, n));
    }
    let _ = fs::remove_dir_all(&runner.work);

    let kept = runs_out.len();
    let header = Header { seed: f.seed, clock: &f.clock, file: &file, id: &compiled.program.id, root: &f.root, roots: &roots, entry: "cics" };
    if let Err(e) = write_manifest(&f.out, &header, inputs_out, &tally, runs_out) {
        return fail(format!("-o {}: {e}", f.out.display()));
    }
    if let Some(t) = inferred {
        eprintln!("ironwork fuzz: each task runs as transaction {t}, the one RETURN TRANSID names");
    }
    if !terminal.missing.is_empty() {
        eprintln!("ironwork fuzz: no library holds mapset {}, so the fields of its maps are typed as text of no map", terminal.missing.iter().cloned().collect::<Vec<_>>().join(", "));
    }
    tally.report();
    if let Some((code, file, line)) = baseline {
        eprintln!("ironwork fuzz: the task ends with {code} at {file}:{line} with no COMMAREA and no operator input; that abend is not kept");
    }
    println!("{}", tally.summary(kept, &f.out));
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_typed_field_is_reached_from_home_by_its_tab_order() {
        let turns = [Turn { fields: vec![Some("ACME".into()), None, Some(" 7".into())], key: "ENTER" }, Turn { fields: vec![None], key: "PF3" }];
        assert_eq!(script(&turns), "home\nstring ACME\nhome\ntab\ntab\nstring  7\nENTER\nPF3\n");
        let parsed = rt::terminal::parse_script(&script(&turns)).unwrap();
        assert_eq!(parsed[5], rt::terminal::Action::Text(" 7".into()));
    }

    #[test]
    fn a_key_the_program_names_is_told_from_a_longer_one() {
        assert!(names_word("IF EIBAID = DFHPF12", "DFHPF12"));
        assert!(!names_word("IF EIBAID = DFHPF12", "DFHPF1"));
        assert!(names_word("WHEN DFHPF1\n", "DFHPF1"));
    }

    #[test]
    fn a_surroundings_abend_is_a_refusal_and_a_data_exception_is_kept() {
        let roots = [PathBuf::from("src")];
        assert!(matches!(abend("src/P.cbl:12:8: ABEND AEI0: PGMIDERR", &roots), Some(Outcome::Refused(_))));
        let asra = abend("src/P.cbl:30:12: ABEND ASRA: not numeric (S0C7, which CICS reports as ASRA)", &roots).and_then(|o| o.place());
        assert_eq!(asra.map(|p| (p.0, p.2)), Some(("ASRA".to_string(), 30)));
    }

    #[test]
    fn a_seed_gives_the_same_commarea_and_turns_every_time() {
        let shape = Commarea { length: 8, fields: vec![Field { offset: 0, size: 3, kind: rt::storage::Kind::Zoned { digits: 3, scale: 0, signed: false, sign: None } }] };
        let terminal = Terminal { used: true, reads: true, maps: vec![vec![Slot { length: 5, numeric: true }, ANY_FIELD]], keys: vec!["PF3"], ..Default::default() };
        let draw = |seed| {
            let mut rng = Rng(seed);
            (0..20)
                .map(|_| {
                    let i = generate(&mut rng, Some(&shape), &terminal);
                    (i.commarea, i.turns.map(|t| script(&t)))
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(draw(4), draw(4));
        assert_ne!(draw(4), draw(5));
        let all = draw(4);
        assert!(all.iter().any(|(c, _)| c.is_none()) && all.iter().any(|(c, _)| c.as_ref().is_some_and(|c| c.len() == 8)));
        assert!(all.iter().all(|(_, s)| s.as_ref().is_some_and(|s| !s.is_empty())));
    }
}
