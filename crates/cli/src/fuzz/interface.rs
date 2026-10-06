//! `ironwork fuzz --interface`: a subprogram run as a caller would run it, with generated arguments
//! for its PROCEDURE DIVISION USING items (docs/evidence.md §5.2).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use exec::evidence::Value;
use exec::layout::{Layout, Resolved};
use rt::storage::Kind;
use rt::vocab::SignPosition;
use syntax::ast::{Arg, ArgMode, ExecKind, Expr, Literal, Operand, Program, Stmt};

use super::{Field, Header, Outcome, Request, Rng, SPACE, Tally, base64, elementary_items, field_bytes, neutral, obj};

/// The shape an interface run's manifest takes, which docs/fuzz-interface-manifest.schema.json
/// describes: a reader of `ironwork-fuzz/v1` would take its arguments for a main program's inputs.
const MANIFEST_FORMAT: &str = "ironwork-fuzz-interface/v1";

/// The runs minimizing one kept input may take.
const MINIMIZE_BUDGET: u32 = 200;

/// One run's arguments in USING order, None for OMITTED.
type Arguments = Vec<Option<Vec<u8>>>;

/// An abend's code, file and line.
type Place = (String, String, i64);

/// The IMS interfaces whose parameters are control blocks the IMS region supplies, not data a
/// caller passes.
const IMS_INTERFACES: &[&str] = &["CBLTDLI", "AIBTDLI", "CEETDLI"];

/// One PROCEDURE DIVISION USING item of a subprogram.
pub(crate) struct Param {
    pub(crate) name: String,
    /// Its LINKAGE record's size, every table at its most occurrences.
    pub(crate) size: usize,
    /// Its elementary items, by offset in the record.
    pub(crate) fields: Vec<Field>,
    /// Each OCCURS DEPENDING ON object in the record, with its table's fewest and most occurrences.
    pub(crate) counts: Vec<(Field, u32, u32)>,
    /// The values the subprogram compares a field with, by the field's offset (`dictionary`).
    pub(crate) values: BTreeMap<usize, Vec<Vec<u8>>>,
}

/// What one CALL passes in one USING position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Passes {
    Omitted,
    /// An item of this length.
    Item(usize),
    /// A literal's bytes, which no input varies.
    Literal(Vec<u8>),
    /// Something whose length the call site does not tell: a function's result, an item the
    /// caller's layout does not resolve.
    Unknown,
}

/// A static CALL of the subprogram: where it is and what it passes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CallSite {
    pub(crate) file: String,
    pub(crate) line: u32,
    pub(crate) passes: Vec<Passes>,
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

fn all_statements(program: &Program) -> Vec<&Stmt> {
    let mut out = Vec::new();
    for p in &program.paragraphs {
        statements(&p.statements, &mut out);
    }
    out
}

fn literal_text(l: &Literal) -> Option<&str> {
    match l {
        Literal::Alnum(s) => Some(s.as_str()),
        _ => None,
    }
}

/// Why an interface run cannot take `compiled`, or None when it can: a program that takes no
/// arguments, one with a pointer among them, an IMS program, whose parameters are control blocks,
/// one that passes an argument on to a CALL whose target nothing names, and a CICS program.
pub(crate) fn refusal(compiled: &exec::Compiled) -> Option<String> {
    let program = &compiled.program;
    let layout = &compiled.layout;
    let id = &program.id;
    if program.function.is_some() {
        return Some(format!("{id} is a user-defined function, which ironwork run does not enter: fuzz a program that invokes it"));
    }
    if program.using.is_empty() {
        return Some(format!("{id} has no PROCEDURE DIVISION USING items: fuzz it as a main program"));
    }
    for param in &program.using {
        let Some(&root) = layout.linkage_roots.iter().find(|&&i| layout.items[i].name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(&param.name))) else {
            return Some(format!("{id}: USING item {} is not a LINKAGE record", param.name));
        };
        let mut stack = vec![root];
        while let Some(i) = stack.pop() {
            let item = &layout.items[i];
            if matches!(item.kind, Kind::Pointer | Kind::ProgramPointer | Kind::ObjectReference) {
                return Some(format!("{id}: USING item {} holds {}, an address a caller supplies and fuzz cannot invent", param.name, item.name.as_deref().unwrap_or("a pointer")));
            }
            stack.extend(item.children.iter().copied());
        }
    }
    let statements = all_statements(program);
    for s in &statements {
        match s {
            Stmt::Entry { name, .. } if name.eq_ignore_ascii_case("DLITCBL") || name.eq_ignore_ascii_case("DLITPLI") => {
                return Some(format!("{id} has ENTRY '{name}': an IMS program, whose parameters are control blocks the region supplies"));
            }
            Stmt::Exec(block) if block.kind == ExecKind::Dli => return Some(format!("{id} has EXEC DLI: an IMS program, whose parameters are control blocks the region supplies")),
            Stmt::Exec(block) if block.kind == ExecKind::Cics => return Some(format!("{id} has EXEC CICS: fuzz it with --cics")),
            Stmt::Call(call) => {
                let named = match &call.target {
                    Operand::Literal(l) => literal_text(l).map(str::to_owned),
                    Operand::Ref(r) => match layout.resolve(&r.name, &r.qualifiers, r.pos) {
                        Ok(Resolved::Item(i)) => layout.items[i].value.as_ref().and_then(literal_text).map(|s| s.trim_end().to_owned()),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(target) = named.as_deref().filter(|t| IMS_INTERFACES.iter().any(|ims| t.eq_ignore_ascii_case(ims))) {
                    return Some(format!("{id} CALLs {target}: an IMS program, whose parameters are control blocks the region supplies"));
                }
                let passes_linkage = call.using.iter().filter_map(|a| match &a.value {
                    Some(Operand::Ref(r)) => match layout.resolve(&r.name, &r.qualifiers, r.pos) {
                        Ok(Resolved::Item(i)) => layout.items[i].linkage.map(|_| r.name.as_str()),
                        _ => None,
                    },
                    _ => None,
                });
                if named.is_none()
                    && let Some(item) = passes_linkage.into_iter().next()
                {
                    return Some(format!("{id} passes {item} to a CALL at line {} whose target no literal or VALUE names: it may be an IMS interface", call.pos.line));
                }
            }
            _ => {}
        }
    }
    None
}

/// The program's USING items in order, each from its LINKAGE record.
pub(crate) fn params(compiled: &exec::Compiled) -> Vec<Param> {
    let layout = &compiled.layout;
    let by_item = super::dictionary::literals_by_item(compiled);
    let record_of = |name: &str| layout.linkage_roots.iter().copied().find(|&i| layout.items[i].name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(name)));
    compiled
        .program
        .using
        .iter()
        .filter_map(|param| {
            let root = record_of(&param.name)?;
            let record = &layout.items[root];
            let size = record.size as usize;
            let mut counts = Vec::new();
            let mut stack = vec![root];
            while let Some(i) = stack.pop() {
                let table = &layout.items[i];
                if let Some(r) = &table.depending_on
                    && let Ok(Resolved::Item(c)) = layout.resolve(&r.name, &r.qualifiers, r.pos)
                {
                    let count = &layout.items[c];
                    // A DEPENDING ON object outside the record is the subprogram's own, not the caller's.
                    if count.linkage == record.linkage && count.offset >= record.offset {
                        counts.push((Field { offset: (count.offset - record.offset) as usize, size: count.size as usize, kind: count.kind }, table.occurs_min, table.occurs));
                    }
                }
                stack.extend(table.children.iter().copied());
            }
            let fields = elementary_items(layout, root, record.offset, size);
            Some(Param { name: param.name.clone(), size, fields: fields.iter().map(|(f, _)| *f).collect(), counts, values: super::dictionary::values(&fields, &by_item) })
        })
        .collect()
}

/// `n` stored in field `f` as its kind stores a number: zoned digits, packed decimal or a
/// big-endian binary; high digits that do not fit are dropped.
fn count_bytes(f: Field, n: u32) -> Vec<u8> {
    match f.kind {
        Kind::Zoned { signed, sign, .. } => {
            let separate = sign.is_some_and(|s| s.separate);
            let leading = sign.is_some_and(|s| s.position == SignPosition::Leading);
            let digits = if separate { f.size.saturating_sub(1) } else { f.size };
            let mut value = n;
            let mut d = vec![0xF0; digits];
            for at in d.iter_mut().rev() {
                *at = 0xF0 | (value % 10) as u8;
                value /= 10;
            }
            if separate {
                if leading { d.insert(0, super::PLUS) } else { d.push(super::PLUS) }
            } else if signed && digits > 0 {
                let at = if leading { 0 } else { digits - 1 };
                d[at] = (d[at] & 0x0F) | 0xC0;
            }
            d
        }
        Kind::Packed { signed, .. } => {
            let mut value = n;
            let mut nibbles = vec![0u8; 2 * f.size - 1];
            for at in nibbles.iter_mut().rev() {
                *at = (value % 10) as u8;
                value /= 10;
            }
            nibbles.push(if signed { 0x0C } else { 0x0F });
            nibbles.chunks(2).map(|p| (p[0] << 4) | p[1]).collect()
        }
        Kind::Binary { .. } => {
            let bytes = u64::from(n).to_be_bytes();
            let kept = f.size.min(bytes.len());
            let mut out = vec![0; f.size];
            out[f.size - kept..].copy_from_slice(&bytes[bytes.len() - kept..]);
            out
        }
        _ => neutral(f),
    }
}

/// A record of `param`'s size: every field within the first `varied` bytes generated, every other
/// one neutral, and each DEPENDING ON object at a count its table allows, drawn or at its most.
fn record(rng: &mut Rng, param: &Param, varied: usize, draw_counts: bool) -> Vec<u8> {
    let mut buf = vec![SPACE; param.size];
    for &f in &param.fields {
        let bytes = match param.values.get(&f.offset) {
            _ if f.offset + f.size > varied => neutral(f),
            Some(values) if rng.below(super::DICTIONARY_ODDS) == 0 => values[rng.below(values.len())].clone(),
            _ => field_bytes(rng, f),
        };
        buf[f.offset..f.offset + f.size].copy_from_slice(&bytes);
    }
    for &(f, min, max) in &param.counts {
        let n = if draw_counts { min + rng.below((max - min + 1) as usize) as u32 } else { max };
        buf[f.offset..f.offset + f.size].copy_from_slice(&count_bytes(f, n));
    }
    buf
}

/// One argument per param. With `site`, each takes what the CALL passes in its position: OMITTED
/// stays OMITTED, a literal is passed as written, padded with spaces, and an item of a known length
/// has only the fields within that length varied.
pub(crate) fn arguments(rng: &mut Rng, params: &[Param], site: Option<&CallSite>) -> Arguments {
    params
        .iter()
        .enumerate()
        .map(|(i, p)| match site.and_then(|s| s.passes.get(i)).unwrap_or(&Passes::Unknown) {
            Passes::Omitted => None,
            Passes::Literal(bytes) => Some(bytes.iter().copied().chain(std::iter::repeat(SPACE)).take(p.size).collect()),
            Passes::Item(len) => Some(record(rng, p, *len, true)),
            Passes::Unknown => Some(record(rng, p, usize::MAX, true)),
        })
        .collect()
}

/// `parent` changed in one passed argument: one field drawn again, from the dictionary or as fields
/// are, or every field the subprogram does not compare with a literal drawn again, keeping the
/// path the compared ones chose while the data along it changes.
fn mutated(rng: &mut Rng, params: &[Param], parent: &Arguments) -> Arguments {
    let mut child = parent.clone();
    let passed: Vec<usize> = (0..child.len()).filter(|&i| child[i].is_some() && params.get(i).is_some_and(|p| !p.fields.is_empty())).collect();
    if passed.is_empty() {
        return child;
    }
    let i = passed[rng.below(passed.len())];
    let (param, Some(argument)) = (&params[i], child[i].as_mut()) else { return child };
    let fields: Vec<&Field> = param.fields.iter().filter(|f| f.offset + f.size <= argument.len()).collect();
    if rng.below(2) == 0 {
        for &&f in fields.iter().filter(|f| !param.values.contains_key(&f.offset)) {
            argument[f.offset..f.offset + f.size].copy_from_slice(&field_bytes(rng, f)[..f.size]);
        }
        return child;
    }
    let compared: Vec<&Field> = fields.iter().copied().filter(|f| param.values.contains_key(&f.offset)).collect();
    // A field the program compares with a literal gets the change half the time it has one.
    let pool = if !compared.is_empty() && rng.below(2) == 0 { &compared } else { &fields };
    if let Some(&f) = (!pool.is_empty()).then(|| pool[rng.below(pool.len())]) {
        let bytes = match param.values.get(&f.offset) {
            Some(values) if rng.below(4) != 0 => values[rng.below(values.len())].clone(),
            _ => field_bytes(rng, f),
        };
        argument[f.offset..f.offset + f.size].copy_from_slice(&bytes[..f.size]);
    }
    child
}

/// Every param passed, every field neutral and every count at its most: the input whose abend is
/// not the input's doing.
pub(crate) fn neutral_arguments(params: &[Param]) -> Arguments {
    params.iter().map(|p| Some(record(&mut Rng(0), p, 0, false))).collect()
}

/// What one CALL argument passes, as the caller's `layout` and code page tell it.
fn passes_of(arg: &Arg, layout: &Layout, compiled: &exec::Compiled) -> Passes {
    match &arg.value {
        None => Passes::Omitted,
        Some(Operand::Ref(r)) => {
            let refmod_length = r.refmod.as_ref().and_then(|m| m.length.as_deref()).and_then(|e| match e {
                Expr::Operand(Operand::Literal(Literal::Number(n))) => n.parse::<usize>().ok(),
                _ => None,
            });
            match (layout.resolve(&r.name, &r.qualifiers, r.pos), refmod_length) {
                (Ok(Resolved::Item(_)), Some(n)) => Passes::Item(n),
                (Ok(Resolved::Item(i)), None) => Passes::Item(layout.items[i].size as usize),
                _ => Passes::Unknown,
            }
        }
        Some(Operand::Literal(Literal::Alnum(s) | Literal::Number(s))) => compiled.options.code_page().encode(s).map_or(Passes::Unknown, Passes::Literal),
        Some(Operand::Literal(Literal::Hex(bytes))) => Passes::Literal(bytes.clone()),
        Some(Operand::LengthOf(_) | Operand::AddressOf(_)) => Passes::Item(4),
        Some(_) => Passes::Unknown,
    }
}

/// Every CALL of `name` by a literal in `callers` (each a source file and its program), with what it
/// passes, in caller order and then statement order. A CALL through a data item is not one.
pub(crate) fn call_sites(name: &str, callers: &[(String, exec::Compiled)]) -> Vec<CallSite> {
    let mut sites = Vec::new();
    for (file, compiled) in callers {
        let program = &compiled.program;
        for s in all_statements(program) {
            let Stmt::Call(call) = s else { continue };
            let Operand::Literal(Literal::Alnum(target)) = &call.target else { continue };
            if !target.trim().eq_ignore_ascii_case(name) {
                continue;
            }
            let file = match call.pos.file {
                0 => file.clone(),
                n => program.sources.get(n as usize).cloned().unwrap_or_else(|| file.clone()),
            };
            let passes = call.using.iter().map(|a| passes_of(a, &compiled.layout, compiled)).collect();
            sites.push(CallSite { file, line: call.pos.line, passes });
        }
    }
    sites
}

/// Each argument a CALL of `name` in `callers` passes BY REFERENCE or BY CONTENT that is shorter
/// than the USING item `params` describes in its place, told with both. The Language Reference's
/// CALL statement says the called program "must describe the same number of character positions"
/// as the caller, and that an alphanumeric literal's parameter is PIC X(n) of the literal's length:
/// a longer item reads past what was passed.
pub(crate) fn short_arguments(name: &str, callers: &[(String, &exec::Compiled)], params: &[Param]) -> Vec<String> {
    let mut out = Vec::new();
    for (file, compiled) in callers {
        let program = &compiled.program;
        for s in all_statements(program) {
            let Stmt::Call(call) = s else { continue };
            let Operand::Literal(Literal::Alnum(target)) = &call.target else { continue };
            if !target.trim().eq_ignore_ascii_case(name) {
                continue;
            }
            for (arg, param) in call.using.iter().zip(params).filter(|(a, _)| a.mode != ArgMode::Value) {
                let (what, len) = match passes_of(arg, &compiled.layout, compiled) {
                    Passes::Item(n) => ("an item", n),
                    Passes::Literal(bytes) => ("a literal", bytes.len()),
                    _ => continue,
                };
                if len < param.size {
                    let file = match call.pos.file {
                        0 => file.clone(),
                        n => program.sources.get(n as usize).cloned().unwrap_or_else(|| file.clone()),
                    };
                    out.push(format!("{file}:{} passes {what} of {len} bytes as {}'s {} of {}: the called program reads past it", call.pos.line, name.trim(), param.name, param.size));
                }
            }
        }
    }
    out
}

/// `short_arguments` for each CALL of a literal in `compiled` and the programs it reaches, against
/// the program it names among `rest`.
pub(crate) fn short_calls(file: &str, compiled: &exec::Compiled, rest: &[Program], flags: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for s in all_statements(&compiled.program) {
        let Stmt::Call(call) = s else { continue };
        let Operand::Literal(Literal::Alnum(target)) = &call.target else { continue };
        let name = target.trim().to_ascii_uppercase();
        if !seen.insert(name.clone()) {
            continue;
        }
        let Some(callee) = rest.iter().find(|p| !p.is_prototype() && (p.id.eq_ignore_ascii_case(&name) || p.load_name().eq_ignore_ascii_case(&name))) else { continue };
        let Ok(callee) = exec::compile(callee.clone(), flags) else { continue };
        out.extend(short_arguments(&name, &[(file.to_owned(), compiled)], &params(&callee)));
    }
    out
}

/// Every program that could CALL the subprogram: each COBOL source in its own directory and the -L
/// libraries but itself, compiled as `ironwork run` would, named by its path from --root.
fn callers(req: &Request) -> Vec<(String, exec::Compiled)> {
    let me = super::resolved(&req.program);
    let own = req.program.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for dir in std::iter::once(own).chain(req.program_dirs.iter().cloned()) {
        if !seen.insert(super::resolved(&dir)) {
            continue;
        }
        let Ok(entries) = fs::read_dir(if dir.as_os_str().is_empty() { Path::new(".") } else { &dir }) else { continue };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().and_then(|e| e.to_str()).is_some_and(|e| ["cbl", "cob", "cobol"].contains(&e.to_ascii_lowercase().as_str())))
            .collect();
        paths.sort();
        for path in paths {
            if super::resolved(&path) == me {
                continue;
            }
            let Ok(bytes) = fs::read(&path) else { continue };
            let libraries = syntax::copy::Libraries::new(std::iter::once(dir.clone()).chain(req.libraries.iter().cloned()).collect()).with_program(&path);
            let Ok(mut programs) = syntax::parse_all_with(&syntax::copy::decode(&bytes), &libraries) else { continue };
            let Ok(compiled) = exec::compile(programs.remove(0), &req.flags) else { continue };
            let name = super::from_root(&path, &req.root).unwrap_or_else(|| path.display().to_string());
            out.push((name, compiled));
        }
    }
    out
}

/// The data sets an interface run gives the subprogram's files, as the main fuzz gives those it
/// does not vary: an empty one for each file it reads and a new one for each it only writes. The
/// runs vary the arguments alone, which the manifest records.
struct DataSets {
    empty: Vec<String>,
    new: Vec<String>,
}

/// One run as its own `ironwork run` with the arguments in files of the run's directory.
struct Runner<'a> {
    req: &'a Request,
    work: PathBuf,
    count: u64,
    covered: super::RunCoverage,
    data_sets: DataSets,
}

impl Runner<'_> {
    fn run(&mut self, arguments: &[Option<Vec<u8>>], optimized: bool, evidence: Option<(&Path, &Path)>) -> std::io::Result<Outcome> {
        self.count += 1;
        let dir = self.work.join(format!("run-{}", self.count));
        fs::create_dir_all(&dir)?;
        let mut command = Command::new(std::env::current_exe()?);
        command.arg("run").arg(&self.req.program).arg("--clock").arg(&self.req.clock);
        command.args(&self.req.flags);
        if optimized {
            command.arg("--optimize=2");
        }
        for d in &self.req.libraries {
            command.arg("-I").arg(d);
        }
        for d in &self.req.program_dirs {
            command.arg("-L").arg(d);
        }
        // A data set is named by its place in the run's directory, never by its DD, as the main
        // fuzz names one.
        let mut given: Vec<(&str, PathBuf)> = Vec::new();
        for dd in &self.data_sets.empty {
            let path = dir.join(format!("dd{}", given.len()));
            fs::write(&path, b"")?;
            given.push((dd, path));
        }
        for dd in &self.data_sets.new {
            given.push((dd, dir.join(format!("dd{}", given.len()))));
        }
        for (dd, path) in &given {
            command.arg("--dd").arg(format!("{dd}={}", path.display()));
        }
        for (i, argument) in arguments.iter().enumerate() {
            command.arg("--argument");
            match argument {
                Some(bytes) => {
                    let path = dir.join(format!("arg{i}"));
                    fs::write(&path, bytes)?;
                    command.arg(path);
                }
                None => {
                    command.arg("OMITTED");
                }
            }
        }
        let cover = super::coverage_path(evidence, &self.work, self.count);
        command.arg("--coverage").arg(&cover);
        if let Some((journal, _)) = evidence {
            command.arg("--evidence").arg(journal);
        }
        let roots = self.req.roots();
        command.arg("--statement-limit").arg(self.req.hang_limit.to_string());
        let outcome = super::finish(command, &dir, self.req.timeout * super::HANG_PATIENCE, &given, |l| super::abend_line(l, &roots));
        self.covered.take(&cover, evidence.is_some());
        outcome
    }
}

/// The smallest arguments found that still end at `place`: each field of each argument passed put
/// back to a value that breaks nothing wherever the abend still comes, and whether that finished
/// within [`MINIMIZE_BUDGET`] runs.
fn minimize(runner: &mut Runner, params: &[Param], mut arguments: Arguments, place: &Place) -> std::io::Result<(Arguments, bool)> {
    let mut budget = MINIMIZE_BUDGET;
    for (i, param) in params.iter().enumerate() {
        for &f in &param.fields {
            let value = neutral(f);
            let Some(Some(bytes)) = arguments.get(i) else { continue };
            if bytes.get(f.offset..f.offset + f.size).is_none_or(|now| now == value) {
                continue;
            }
            if budget == 0 {
                return Ok((arguments, false));
            }
            budget -= 1;
            let mut trial = arguments.clone();
            if let Some(Some(b)) = trial.get_mut(i) {
                b[f.offset..f.offset + f.size].copy_from_slice(&value);
            }
            if runner.run(&trial, false, None)?.place().as_ref() == Some(place) {
                arguments = trial;
            }
        }
    }
    Ok((arguments, true))
}

/// Lists each of a kept run's arguments in `out` as the manifest gives them, and returns their ids.
fn listed(params: &[Param], arguments: &[Option<Vec<u8>>], n: usize, minimized: bool, out: &mut Vec<Value>) -> Vec<Value> {
    params
        .iter()
        .zip(arguments)
        .enumerate()
        .map(|(position, (param, argument))| {
            let id = format!("r{n}-{}", param.name);
            let mut pairs = vec![
                ("id", Value::from(id.as_str())),
                ("kind", "argument".into()),
                ("name", param.name.as_str().into()),
                ("position", Value::from(position as i64)),
                ("bytes", base64(argument.as_deref().unwrap_or_default()).into()),
                ("minimized", minimized.into()),
            ];
            if argument.is_none() {
                pairs.push(("omitted", true.into()));
            }
            out.push(obj(pairs));
            Value::from(id)
        })
        .collect()
}

/// The kept abends' arguments and runs as the manifest lists them and their codes, what the runs
/// came to, and the abend the neutral arguments gave.
struct Found {
    inputs: Vec<Value>,
    runs: Vec<Value>,
    codes: Vec<String>,
    tally: Tally,
    baseline: Option<Place>,
}

/// A run on arguments that break nothing, whose abend is no input's doing; `req.runs` generated
/// argument sets, each shaped by a CALL site drawn from `sites` where there are any; then each new
/// abend once, on the smallest arguments that still give it, run with evidence and coverage, and
/// once more compiled with OPTIMIZE(2).
/// A program name a run may give a CALL whose target an argument supplies, with the items the
/// sources beside the subprogram store it in.
struct Candidate {
    name: String,
    receivers: BTreeSet<String>,
}

/// The program names a run may give a CALL whose target an argument supplies: the alphanumeric
/// literals that the subprogram, and each source beside it that names the subprogram in a literal,
/// MOVE or give as a VALUE, each a program a CALL finds in the run's program libraries and that
/// compiles, the subprogram excepted. With them, the programs those names and their CALLs of a
/// literal reach, whose files a run gives data sets.
fn candidates(compiled: &exec::Compiled, callers: &[(String, exec::Compiled)], req: &Request) -> (Vec<Candidate>, Vec<Program>) {
    let own_name = compiled.program.load_name();
    let mut stored_in: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for c in std::iter::once(compiled).chain(callers.iter().map(|(_, c)| c)) {
        let (stored, called) = literals(c);
        if std::ptr::eq(c, compiled) || stored.keys().chain(&called).any(|l| l.eq_ignore_ascii_case(own_name)) {
            for (name, receivers) in stored {
                stored_in.entry(name).or_default().extend(receivers);
            }
        }
    }
    let mut library = super::library_of(req);
    let mut out = Vec::new();
    for (name, receivers) in stored_in {
        if name.len() > NAME_WIDTH || name.eq_ignore_ascii_case(own_name) {
            continue;
        }
        if library.find(&name).is_some_and(|p| exec::compile(p.clone(), &req.flags).is_ok()) {
            out.push(Candidate { name, receivers });
        }
    }
    super::read_called(&mut library, out.iter().map(|c| c.name.clone()).collect());
    (out, library.programs)
}

/// A program's alphanumeric literals in upper case, trimmed: each one a MOVE sends or a VALUE clause
/// gives, with the names of the items it goes to, and the targets of its CALLs of a literal.
fn literals(compiled: &exec::Compiled) -> (BTreeMap<String, BTreeSet<String>>, BTreeSet<String>) {
    let (mut stored, mut called): (BTreeMap<String, BTreeSet<String>>, _) = (BTreeMap::new(), BTreeSet::new());
    for s in all_statements(&compiled.program) {
        match s {
            Stmt::Move { from: Operand::Literal(Literal::Alnum(text)), to, .. } => {
                stored.entry(text.trim().to_ascii_uppercase()).or_default().extend(to.iter().map(|r| r.name.to_ascii_uppercase()));
            }
            Stmt::Call(call) => {
                if let Operand::Literal(Literal::Alnum(text)) = &call.target {
                    called.insert(text.trim().to_ascii_uppercase());
                }
            }
            _ => {}
        }
    }
    for item in &compiled.layout.items {
        if let Some(text) = item.value.as_ref().and_then(literal_text) {
            stored.entry(text.trim().to_ascii_uppercase()).or_default().extend(item.name.as_ref().map(|n| n.to_ascii_uppercase()));
        }
    }
    (stored, called)
}

/// The names a slot is given: those stored in an item named as the CALL's target is, or in one
/// whose name ends in `-` and that name (WS-PUT-MESSAGE for PUT-MESSAGE), or every candidate when
/// none is or the target is not known.
fn names_for(candidates: &[Candidate], target: Option<&str>) -> Vec<String> {
    let matches = |c: &&Candidate| target.is_some_and(|t| c.receivers.iter().any(|r| r == t || r.strip_suffix(t).is_some_and(|head| head.ends_with('-'))));
    let chosen: Vec<String> = candidates.iter().filter(matches).map(|c| c.name.clone()).collect();
    if chosen.is_empty() { candidates.iter().map(|c| c.name.clone()).collect() } else { chosen }
}

/// The width a program name is given in an argument: a z/OS member name's eight characters.
const NAME_WIDTH: usize = 8;

/// Where an argument held the program name a CALL took: the argument's place in USING order and
/// the offset in it.
type NameSlot = (usize, usize);

/// Where each CALL of a data item takes its program name from in the arguments, with the item's
/// name in upper case: the item itself where it lies in a USING record, or the field a chain of
/// MOVEs, of up to `MOVE_HOPS`, carries into it from one.
fn linkage_targets(compiled: &exec::Compiled) -> Vec<(NameSlot, String)> {
    let layout = &compiled.layout;
    let records: Vec<usize> = compiled.program.using.iter().filter_map(|p| layout.linkage_roots.iter().copied().find(|&i| layout.items[i].name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(&p.name)))).collect();
    let statements = all_statements(&compiled.program);
    let item_of = |r: &syntax::ast::Ref| match layout.resolve(&r.name, &r.qualifiers, r.pos) {
        Ok(Resolved::Item(i)) if r.refmod.is_none() => Some(i),
        _ => None,
    };
    let moves: Vec<(usize, usize)> = statements
        .iter()
        .filter_map(|s| if let Stmt::Move { from: Operand::Ref(from), to, .. } = s { item_of(from).map(|f| (f, to)) } else { None })
        .flat_map(|(f, to)| to.iter().filter_map(&item_of).map(move |t| (f, t)))
        .collect();
    let mut slots: Vec<(NameSlot, String)> = Vec::new();
    for s in &statements {
        let Stmt::Call(call) = s else { continue };
        let Operand::Ref(r) = &call.target else { continue };
        let Some(i) = item_of(r) else { continue };
        let item = &layout.items[i];
        for slot in origins(layout, &records, &moves, Byte { linkage: item.linkage, file: item.file, offset: item.offset }, MOVE_HOPS) {
            if !slots.iter().any(|(s, _)| *s == slot) {
                slots.push((slot, r.name.to_ascii_uppercase()));
            }
        }
    }
    slots
}

/// How many MOVEs a CALL target's value is traced back through to an argument.
const MOVE_HOPS: u8 = 4;

/// A byte of the program's storage: the area it is in and its offset there.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Byte {
    linkage: Option<u16>,
    file: Option<u16>,
    offset: u32,
}

/// Whether item `i` holds `place`.
fn holds(layout: &Layout, i: usize, place: Byte) -> bool {
    let item = &layout.items[i];
    item.linkage == place.linkage && item.file == place.file && place.offset >= item.offset && place.offset < item.offset + item.size
}

/// The argument places `place`'s value comes from: its own where a USING record holds it, else,
/// through each MOVE whose receiver holds it, the matching byte of the sender's, `hops` deep.
fn origins(layout: &Layout, records: &[usize], moves: &[(usize, usize)], place: Byte, hops: u8) -> Vec<NameSlot> {
    if let Some(k) = records.iter().position(|&r| holds(layout, r, place)) {
        return vec![(k, (place.offset - layout.items[records[k]].offset) as usize)];
    }
    if hops == 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for &(from, to) in moves.iter().filter(|&&(_, to)| holds(layout, to, place)) {
        let (sender, shift) = (&layout.items[from], place.offset - layout.items[to].offset);
        if shift < sender.size {
            let back = Byte { linkage: sender.linkage, file: sender.file, offset: sender.offset + shift };
            out.extend(origins(layout, records, moves, back, hops - 1).into_iter().filter(|slot| !out.contains(slot)).collect::<Vec<_>>());
        }
    }
    out
}

/// Where `name` first stands in `arguments`, as EBCDIC.
fn name_slot(arguments: &Arguments, name: &str) -> Option<NameSlot> {
    let bytes = super::ebcdic(name);
    arguments.iter().enumerate().find_map(|(i, a)| a.as_ref()?.windows(bytes.len()).position(|w| w == bytes.as_slice()).map(|at| (i, at)))
}

/// `arguments` with `name` at `slot`, padded with spaces to the elementary item that starts there
/// when it is narrower than a program name, to a program name's width otherwise, and never past
/// the argument's end.
fn with_name(arguments: &mut Arguments, params: &[Param], (i, at): NameSlot, name: &str) {
    let Some(Some(argument)) = arguments.get_mut(i) else { return };
    let field = params.get(i).and_then(|p| p.fields.iter().find(|f| f.offset == at)).map_or(NAME_WIDTH, |f| f.size.min(NAME_WIDTH));
    let width = field.min(argument.len().saturating_sub(at));
    let bytes = super::ebcdic(name);
    if bytes.len() <= width {
        let padded = bytes.into_iter().chain(std::iter::repeat(SPACE)).take(width);
        argument[at..at + width].iter_mut().zip(padded).for_each(|(b, n)| *b = n);
    }
}

fn drive(req: &Request, compiled: &exec::Compiled, params: &[Param], sites: &[CallSite], candidates: &[Candidate], runner: &mut Runner) -> Result<Found, String> {
    let started = |e: std::io::Error| format!("a run could not start: {e}");
    let baseline = runner.run(&neutral_arguments(params), false, None).map_err(started)?.place();
    let mut rng = Rng(req.seed.max(1));
    let mut tally = Tally::new();
    let mut kept: Vec<(Place, Arguments)> = Vec::new();
    let mut slots: Vec<(NameSlot, Vec<String>)> = if candidates.is_empty() { Vec::new() } else { linkage_targets(compiled).into_iter().map(|(slot, target)| (slot, names_for(candidates, Some(&target)))).collect() };
    for ((i, at), names) in &slots {
        eprintln!("ironwork fuzz: a CALL takes its program name from {} at offset {at}; runs give it one of {}", params[*i].name, names.join(", "));
    }
    let mut corpus: Vec<Arguments> = Vec::new();
    for _ in 0..req.runs {
        let site = (!sites.is_empty()).then(|| &sites[rng.below(sites.len())]);
        // Half the runs change arguments that reached what no earlier run did, once there are any.
        let mut generated = if corpus.is_empty() || rng.below(2) == 0 {
            arguments(&mut rng, params, site)
        } else {
            let parent = super::parent_of(&mut rng, corpus.len());
            mutated(&mut rng, params, &corpus[parent])
        };
        for (slot, names) in &slots {
            with_name(&mut generated, params, *slot, &names[rng.below(names.len())]);
        }
        let outcome = runner.run(&generated, false, None).map_err(started)?;
        if runner.covered.novel.get() && corpus.len() < super::CORPUS_LIMIT {
            corpus.push(generated.clone());
        }
        tally.add(&outcome);
        // A CALL that took its program name from the arguments ends CEE3501S on a generated name;
        // later runs give that place a name the libraries hold, as a caller would.
        if let Outcome::Abend { code, file, line, message } = &outcome
            && !candidates.is_empty()
            && let Some(slot) = super::missing_module(code, message).and_then(|name| name_slot(&generated, name))
            && !slots.iter().any(|(s, _)| *s == slot)
        {
            let names = names_for(candidates, None);
            eprintln!("ironwork fuzz: the CALL at {file}:{line} takes its program name from {} at offset {}; later runs give it one of {}", params[slot.0].name, slot.1, names.join(", "));
            slots.push((slot, names));
        }
        if let Some(place) = outcome.place()
            && Some(&place) != baseline.as_ref()
            && !kept.iter().any(|(p, _)| *p == place)
        {
            kept.push((place, generated));
        }
    }
    let (evidence, coverage) = (req.out.join("evidence"), req.out.join("coverage"));
    let (mut inputs, mut runs, mut codes) = (Vec::new(), Vec::new(), Vec::new());
    for (n, (place, found)) in kept.into_iter().enumerate() {
        let (small, minimized) = minimize(runner, params, found, &place).map_err(started)?;
        let before = super::journals(&evidence);
        let cover = coverage.join(format!("{n}.json"));
        let outcome = runner.run(&small, false, Some((&evidence, &cover))).map_err(started)?;
        let came_again = outcome.place().as_ref() == Some(&place);
        match super::journals(&evidence).into_iter().find(|j| !before.contains(j)).filter(|_| came_again) {
            Some(journal) => {
                let optimized = runner.run(&small, true, None).map_err(started)?.place().as_ref() == Some(&place);
                runs.push(super::kept_run(listed(params, &small, n, minimized, &mut inputs), &outcome, optimized, None, journal, n));
                codes.push(place.0.clone());
            }
            None => {
                let why = if came_again { "wrote no journal".to_string() } else { outcome.told() };
                eprintln!("ironwork fuzz: {} at {}:{} is not kept: its run on the smallest arguments {why}", place.0, place.1, place.2);
            }
        }
    }
    Ok(Found { inputs, runs, codes, tally, baseline })
}

/// `ironwork fuzz --interface`: runs the subprogram `req` names as a caller would, many times, and
/// keeps each abend the arguments caused (docs/evidence.md §5.2).
pub fn run(req: Request) -> ExitCode {
    let fail = |message: String| {
        eprintln!("ironwork fuzz: {message}");
        ExitCode::from(2)
    };
    let (compiled, mut rest) = match super::compile(&req) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(12);
        }
    };
    let Some(file) = super::from_root(&req.program, &req.root) else {
        return fail(format!("{} is not under --root {}", req.program.display(), req.root.display()));
    };
    if let Some(why) = refusal(&compiled) {
        return fail(why);
    }
    let params = params(&compiled);
    let callers = callers(&req);
    let sites = call_sites(&compiled.program.id, &callers);
    let callers_compiled: Vec<(String, &exec::Compiled)> = callers.iter().map(|(f, c)| (f.clone(), c)).collect();
    for short in short_arguments(compiled.program.load_name(), &callers_compiled, &params).into_iter().chain(short_calls(&file, &compiled, &rest, &req.flags)) {
        eprintln!("ironwork fuzz: {short}");
    }
    let (candidates, named) = candidates(&compiled, &callers, &req);
    rest.extend(named);
    let names: Vec<String> = candidates.iter().map(|c| c.name.clone()).collect();
    let work = match super::prepare(&req.out, &req.roots()) {
        Ok(w) => w,
        Err(e) => return fail(e),
    };
    let (feeds, _, others) = super::inputs_reaching(&compiled, &rest, &names);
    let mut empty: Vec<String> = feeds.into_iter().map(|f| f.dd).chain(others.unfed.iter().cloned()).collect();
    empty.sort();
    empty.dedup();
    let data_sets = DataSets { empty, new: others.written.clone() };
    if !data_sets.empty.is_empty() {
        eprintln!("ironwork fuzz: not varied, given empty: {}", data_sets.empty.join(", "));
    }
    if !others.ungiven.is_empty() {
        eprintln!("ironwork fuzz: no --dd can carry these names, so they are given no data set: {}", others.ungiven.join(", "));
    }
    let mut runner = Runner { req: &req, work, count: 0, covered: super::RunCoverage::default(), data_sets };
    let found = drive(&req, &compiled, &params, &sites, &candidates, &mut runner);
    let _ = fs::remove_dir_all(&runner.work);
    let found = match found {
        Ok(f) => f,
        Err(e) => return fail(e),
    };
    let header = Header { seed: req.seed, clock: &req.clock, file: &file, id: &compiled.program.id, root: &req.root, roots: &req.roots(), entry: "interface" };
    let callers = Value::Arr(sites.iter().map(|s| obj(vec![("file", s.file.as_str().into()), ("line", Value::from(i64::from(s.line)))])).collect());
    if let Err(e) = runner.covered.write(&req.out).and_then(|key| super::write_manifest_in(MANIFEST_FORMAT, &req.out, &header, found.inputs, &found.tally, found.runs, vec![("callers", callers), key])) {
        return fail(format!("-o {}: {e}", req.out.display()));
    }
    found.tally.report();
    if let Some((code, file, line)) = found.baseline {
        eprintln!("ironwork fuzz: the subprogram ends with {code} at {file}:{line} on arguments that break nothing; that abend is not kept");
    }
    println!("{}", found.tally.summary(&found.codes, &req.out));
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compiled(text: &str) -> exec::Compiled {
        let mut programs = syntax::parse_all_with(text, &syntax::copy::Libraries::default()).unwrap_or_else(|e| panic!("{e}"));
        exec::compile(programs.remove(0), &[]).unwrap_or_else(|e| panic!("{e:?}"))
    }

    fn subprogram(linkage: &str, procedure: &str) -> String {
        format!(
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. SUB.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  WS-FN PIC X(8) VALUE 'CBLTDLI'.\n       01  WS-ANY PIC X(8).\n       LINKAGE SECTION.\n{linkage}       PROCEDURE DIVISION USING ARG.\n{procedure}           GOBACK.\n"
        )
    }

    const ARG: &str = "       01  ARG.\n           05 ARG-QTY PIC 9(5).\n";

    #[test]
    fn a_subprogram_taking_data_is_taken() {
        assert_eq!(refusal(&compiled(&subprogram(ARG, "           ADD 1 TO ARG-QTY\n"))), None);
    }

    #[test]
    fn a_pointer_among_the_arguments_is_refused() {
        let linkage = "       01  ARG.\n           05 ARG-PTR USAGE POINTER.\n";
        assert!(refusal(&compiled(&subprogram(linkage, ""))).is_some_and(|r| r.contains("ARG-PTR")));
    }

    #[test]
    fn every_form_of_ims_program_is_refused() {
        for procedure in [
            "           CALL 'CBLTDLI' USING ARG\n",
            "           CALL 'AIBTDLI' USING ARG\n",
            "           CALL 'CEETDLI' USING ARG\n",
            "           CALL WS-FN USING ARG\n",
            "           ENTRY 'DLITCBL' USING ARG\n",
        ] {
            let why = refusal(&compiled(&subprogram(ARG, procedure)));
            assert!(why.as_deref().is_some_and(|r| r.contains("IMS")), "{procedure}: {why:?}");
        }
    }

    #[test]
    fn an_argument_passed_on_to_a_call_nothing_names_is_refused() {
        let why = refusal(&compiled(&subprogram(ARG, "           CALL WS-ANY USING ARG\n")));
        assert!(why.as_deref().is_some_and(|r| r.contains("may be an IMS interface")), "{why:?}");
    }

    #[test]
    fn a_cics_program_is_sent_to_the_cics_entry() {
        let why = refusal(&compiled(&subprogram(ARG, "           EXEC CICS RETURN END-EXEC\n")));
        assert!(why.as_deref().is_some_and(|r| r.contains("--cics")), "{why:?}");
    }

    fn with_linkage(working: &str, linkage: &str, using: &str) -> exec::Compiled {
        compiled(&format!(
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. SUB.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n{working}       LINKAGE SECTION.\n{linkage}       PROCEDURE DIVISION USING {using}.\n           GOBACK.\n"
        ))
    }

    #[test]
    fn each_using_item_is_a_param_of_its_record_s_size_and_fields() {
        let c = with_linkage("", "       01  ARG.\n           05 ARG-QTY PIC 9(5).\n           05 ARG-NAME PIC X(10).\n       01  ARG2.\n           05 ARG2-FLAG PIC X.\n", "ARG ARG2");
        let ps = params(&c);
        assert_eq!(ps.iter().map(|p| (p.name.as_str(), p.size, p.fields.len())).collect::<Vec<_>>(), [("ARG", 15, 2), ("ARG2", 1, 1)]);
    }

    const ODO: &str = "       01  ARG.\n           05 ARG-CNT PIC 9(2).\n           05 ARG-TBL OCCURS 1 TO 10 DEPENDING ON ARG-CNT.\n               10 ARG-ITEM PIC X(4).\n";

    #[test]
    fn a_depending_on_object_in_the_record_always_holds_a_count_its_table_allows() {
        let ps = params(&with_linkage("", ODO, "ARG"));
        assert_eq!(ps[0].counts.iter().map(|(f, min, max)| (f.offset, f.size, *min, *max)).collect::<Vec<_>>(), [(0, 2, 1, 10)]);
        let mut rng = Rng(12345);
        for _ in 0..200 {
            let args = arguments(&mut rng, &ps, None);
            let buf = args[0].as_ref().expect("passed");
            let n = u32::from(buf[0] & 0x0F) * 10 + u32::from(buf[1] & 0x0F);
            assert!((1..=10).contains(&n), "count {n}");
        }
    }

    #[test]
    fn a_depending_on_object_outside_the_record_is_the_subprogram_s_own() {
        let linkage = "       01  ARG.\n           05 ARG-TBL OCCURS 1 TO 10 DEPENDING ON WS-CNT.\n               10 ARG-ITEM PIC X(4).\n";
        let ps = params(&with_linkage("       01  WS-CNT PIC 9(2) VALUE 5.\n", linkage, "ARG"));
        assert!(ps[0].counts.is_empty());
    }

    fn site(passes: Vec<Passes>) -> CallSite {
        CallSite { file: "CALLER.cbl".to_string(), line: 1, passes }
    }

    #[test]
    fn a_call_site_s_omitted_literal_and_shorter_item_shape_the_arguments() {
        let ps = params(&with_linkage("", "       01  ARG.\n           05 ARG-QTY PIC 9(5).\n           05 ARG-NAME PIC X(10).\n", "ARG"));
        let neutral = neutral_arguments(&ps);
        let mut rng = Rng(42);
        assert_eq!(arguments(&mut rng, &ps, Some(&site(vec![Passes::Omitted]))), [None]);
        assert_eq!(arguments(&mut rng, &ps, Some(&site(vec![Passes::Literal(vec![0xC1, 0xC2])]))), [Some([vec![0xC1, 0xC2], vec![SPACE; 13]].concat())]);
        for _ in 0..50 {
            let shorter = arguments(&mut rng, &ps, Some(&site(vec![Passes::Item(5)])));
            assert_eq!(shorter[0].as_ref().map(|b| b[5..].to_vec()), neutral[0].as_ref().map(|b| b[5..].to_vec()));
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_arguments() {
        let ps = params(&with_linkage("", ODO, "ARG"));
        assert_eq!(arguments(&mut Rng(777), &ps, None), arguments(&mut Rng(777), &ps, None));
    }

    #[test]
    fn a_count_is_stored_as_its_item_stores_a_number() {
        let field = |kind| Field { offset: 0, size: if matches!(kind, Kind::Zoned { .. }) { 3 } else { 2 }, kind };
        assert_eq!(count_bytes(field(Kind::Zoned { digits: 3, scale: 0, signed: true, sign: None }), 42), [0xF0, 0xF4, 0xC2]);
        assert_eq!(count_bytes(field(Kind::Zoned { digits: 3, scale: 0, signed: false, sign: None }), 1234), [0xF2, 0xF3, 0xF4]);
        assert_eq!(count_bytes(field(Kind::Packed { digits: 3, scale: 0, signed: true }), 42), [0x04, 0x2C]);
        assert_eq!(count_bytes(field(Kind::Binary { digits: 4, scale: 0, signed: true, native: numeric::Native::No }), 42), [0x00, 0x2A]);
    }

    const SUB_PROGRAM: &str = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. CALLER.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  A PIC X(5).\n       01  B PIC X(10).\n       01  FN PIC X(8) VALUE 'SUB'.\n       01  FLAG PIC X VALUE 'Y'.\n       PROCEDURE DIVISION.\n";

    fn caller(procedure: &str) -> exec::Compiled {
        compiled(&format!("{SUB_PROGRAM}{procedure}           GOBACK.\n"))
    }

    #[test]
    fn a_call_site_says_what_it_passes_in_each_position() {
        let c = caller("           CALL 'SUB' USING A OMITTED 'XY' BY CONTENT LENGTH OF B\n");
        let xy = c.options.code_page().encode("XY").expect("encodes");
        let sites = call_sites("SUB", &[("CALLER.cbl".to_string(), c)]);
        assert_eq!(sites, [CallSite { file: "CALLER.cbl".to_string(), line: 10, passes: vec![Passes::Item(5), Passes::Omitted, Passes::Literal(xy), Passes::Item(4)] }]);
    }

    #[test]
    fn a_nested_call_a_lower_case_name_and_a_reference_modification_are_found() {
        let c = caller("           IF FLAG = 'Y'\n               CALL 'sub' USING B(1:3)\n           END-IF\n");
        let sites = call_sites("SUB", &[("CALLER.cbl".to_string(), c)]);
        assert_eq!(sites.iter().map(|s| s.passes.clone()).collect::<Vec<_>>(), [vec![Passes::Item(3)]]);
    }

    #[test]
    fn a_call_of_another_name_or_through_a_data_item_is_not_a_call_site() {
        let c = caller("           CALL 'OTHER' USING A\n           CALL FN USING A\n");
        assert!(call_sites("SUB", &[("CALLER.cbl".to_string(), c)]).is_empty());
    }

    #[test]
    fn call_sites_come_in_caller_order() {
        let sites = call_sites("SUB", &[("C1.cbl".to_string(), caller("           CALL 'SUB' USING A\n")), ("C2.cbl".to_string(), caller("           CALL 'SUB' USING B\n"))]);
        assert_eq!(sites.iter().map(|s| (s.file.as_str(), s.passes.clone())).collect::<Vec<_>>(), [("C1.cbl", vec![Passes::Item(5)]), ("C2.cbl", vec![Passes::Item(10)])]);
    }
}
