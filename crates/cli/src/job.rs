//! `ironwork job`: a job's steps run in order against a directory of data sets, procedures
//! expanded into the steps they run. Each EXEC PGM=
//! runs a COBOL program from the program libraries, or IEFBR14 or IEBGENER; DD statements become
//! the files a step's DDs stand for, and dispositions create, keep and delete them as the step
//! ends. COND and IF/THEN/ELSE decide which steps run, from the return codes and abends before.

use crate::exit::{self, Outcome};
use exec::abend::{AbendCode, Signal};
use jcl::cond::{self, Ran};
use jcl::{Dd, End, Item, Job, Source, Status, Step};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use zarch::ebcdic::CodePage;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use exec::Execute;
use exec::evidence::{canonical, fields, Value};

pub struct Request {
    pub jcl: PathBuf,
    pub datasets: PathBuf,
    /// Data sets hold UTF-8 lines rather than z/OS records.
    pub text: bool,
    pub libraries: Vec<PathBuf>,
    pub program_dirs: Vec<PathBuf>,
    /// Procedure libraries searched after the job's JCLLIB, each a directory of members.
    pub proclibs: Vec<PathBuf>,
    /// The user ID that submitted the job.
    pub user: Option<String>,
    pub flags: Vec<String>,
    pub clock: exec::unit::Clock,
    pub replay: Option<PathBuf>,
    /// Production's outputs, a directory laid out as --datasets is: the job runs on a copy of the
    /// data sets and each file here is compared with the data set the job left.
    pub expected: Option<PathBuf>,
    /// Production's step outcomes, one a line: `STEP RC=nnnn` or `STEP ABEND code`.
    pub expected_steps: Option<PathBuf>,
    pub declare: Option<PathBuf>,
    pub statement: Option<PathBuf>,
    /// Where the job's hash-chained journal goes, as `run --evidence` writes one for a program.
    pub evidence: Option<PathBuf>,
    /// The text an input trace looks for in each sink's operand, recorded in the journal.
    pub trace_marker: Option<String>,
    /// PARMs given in place of the EXEC's, by step as the job log names it.
    pub parms: Vec<(String, String)>,
    /// In-stream data given in place of the JCL's, by step and DD (`STEP.DD`), each a file of lines.
    pub instream: Vec<(String, PathBuf)>,
    /// Where the paragraphs the job's programs entered are written, as `run --coverage` writes them.
    pub coverage: Option<PathBuf>,
    /// How many statements each COBOL step may start before it ends with S322.
    pub statement_limit: Option<u64>,
}

/// The job in `jcl`, its procedures and INCLUDE members found in the data sets JCLLIB names under
/// `datasets` and then in `proclibs`; Err with why, as a JCL that cannot be read or one ironwork
/// refuses.
pub(crate) fn parse(jcl: &Path, datasets: &Path, proclibs: &[PathBuf], user: Option<&str>) -> Result<Job, (Outcome, String)> {
    let shown = jcl.display().to_string();
    let text = fs::read_to_string(jcl).map_err(|e| (Outcome::Unreadable, format!("{shown}: {e}")))?;
    let libraries = |order: &[String], member: &str| -> Result<Option<String>, String> {
        let dirs = order.iter().map(|dsn| datasets.join(dsn)).chain(proclibs.iter().cloned());
        for dir in dirs {
            for path in [dir.join(member), dir.join(format!("{member}.jcl"))] {
                if path.is_file() {
                    return fs::read_to_string(&path).map(Some).map_err(|e| format!("{}: {e}", path.display()));
                }
            }
        }
        Ok(None)
    };
    jcl::parse_with(&text, &libraries, user).map_err(|e| (Outcome::NotRun, format!("{shown}:{}: {}", e.line, e.message)))
}

/// The job with the request's PARMs and in-stream data in place of the JCL's.
fn override_steps(job: &mut Job, req: &Request) -> Result<(), String> {
    let mut steps = job.items.iter_mut().filter_map(|i| if let Item::Step(s) = i { Some(s) } else { None }).collect::<Vec<_>>();
    for (name, parm) in &req.parms {
        let step = steps.iter_mut().find(|s| s.shown() == *name).ok_or_else(|| format!("--step-parm {name}: the job has no such step"))?;
        step.parm = Some(parm.clone());
    }
    for (key, path) in &req.instream {
        let (name, dd) = key.rsplit_once('.').ok_or_else(|| format!("--instream {key}: needs STEP.DD"))?;
        let step = steps.iter_mut().find(|s| s.shown() == name).ok_or_else(|| format!("--instream {key}: the job has no step {name}"))?;
        let part = step.dds.iter_mut().find(|d| d.name == dd).and_then(|d| d.parts.iter_mut().find(|p| matches!(p.source, Source::InStream(_))));
        let part = part.ok_or_else(|| format!("--instream {key}: step {name} has no in-stream DD {dd}"))?;
        let text = fs::read_to_string(path).map_err(|e| format!("--instream {key}: {}: {e}", path.display()))?;
        part.source = Source::InStream(text.lines().map(str::to_string).collect());
    }
    Ok(())
}

/// IBM programs a job can name that ironwork does not run; each is refused before the job starts.
const NOT_SUPPORTED: &[&str] = &[
    "ICETOOL", "IEBCOPY", "IEBUPDTE", "IEBPTPCH", "IEBCOMPR", "IEBDG", "IEHLIST", "IEHPROGM", "IEHMOVE", "IKJEFT01", "IKJEFT1A", "IKJEFT1B", "IRXJCL", "BPXBATCH", "BPXBATSL", "FTP", "DSNUTILB", "DSNUPROC", "DSNTEP2", "DSNTEP4", "DSNTIAUL", "DSNTIAD", "DFSRRC00", "ADRDSSU", "IEWL", "IEWBLINK", "HEWL", "IGYCRCTL", "ASMA90", "DFHECP1$", "DFHEAP1$", "IEBEDIT", "AMASPZAP",
];

pub(crate) enum Program {
    Iefbr14,
    Iebgener,
    Idcams,
    Sort,
    Cobol(PathBuf),
    Missing,
}

pub(crate) fn program_of(pgm: &str, dirs: &[PathBuf]) -> Program {
    match pgm {
        "IEFBR14" => return Program::Iefbr14,
        "IEBGENER" | "ICEGENER" => return Program::Iebgener,
        "IDCAMS" => return Program::Idcams,
        "SORT" | "ICEMAN" | "DFSORT" | "SYNCSORT" => return Program::Sort,
        _ => {}
    }
    let lower = pgm.to_ascii_lowercase();
    for dir in dirs {
        for name in [pgm, lower.as_str()] {
            for ext in ["cbl", "cob", "CBL", "COB"] {
                let path = dir.join(format!("{name}.{ext}"));
                if path.is_file() {
                    return Program::Cobol(path);
                }
            }
        }
    }
    Program::Missing
}

/// The step's DDs a run allocates: STEPLIB, like JOBLIB, names load libraries, and programs come
/// from the program libraries instead.
fn allocated_dds(step: &Step) -> impl Iterator<Item = &Dd> {
    step.dds.iter().filter(|d| d.name != "STEPLIB")
}

fn is_in_stream(dd: &Dd) -> bool {
    dd.parts.iter().any(|p| matches!(p.source, Source::InStream(_)))
}

/// Everything the job asks for that this runner cannot do, found before any step runs.
fn refusals(job: &Job, req: &Request) -> Vec<String> {
    let mut out = Vec::new();
    for item in &job.items {
        let Item::Step(step) = item else { continue };
        let at = |m: String| format!("line {}: {m}", step.line);
        if NOT_SUPPORTED.contains(&step.pgm.as_str()) {
            out.push(at(format!("PGM={} is not supported yet", step.pgm)));
        }
        let program = program_of(&step.pgm, &req.program_dirs);
        if step.parm.is_some() && !matches!(program, Program::Iefbr14 | Program::Cobol(_) | Program::Missing) {
            out.push(at(format!("PARM for PGM={} is not supported yet", step.pgm)));
        }
        for dd in allocated_dds(step) {
            let text_parts = dd.parts.iter().filter(|p| matches!(p.source, Source::InStream(_))).count();
            if dd.parts.len() > 1 && text_parts > 0 && text_parts < dd.parts.len() && !req.text {
                out.push(at(format!("DD {} concatenates in-stream data with data sets of z/OS records", dd.name)));
            }
        }
        if matches!(program, Program::Sort)
            && let Some([jcl::Part { source: Source::InStream(cards), .. }]) = step.dds.iter().find(|d| d.name == "SYSIN").map(|d| &d.parts[..])
            && let Err(e) = jcl::sort::parse(cards)
        {
            out.push(at(format!("{}: {e}", step.pgm)));
        }
        if matches!(program, Program::Idcams) {
            match step.dds.iter().find(|d| d.name == "SYSIN").map(|d| &d.parts[..]) {
                Some([jcl::Part { source: Source::InStream(cards), .. }]) => {
                    if let Err(e) = jcl::idcams::parse(cards) {
                        out.push(at(format!("IDCAMS: {e}")));
                    }
                }
                Some(_) if !req.text => out.push(at("IDCAMS SYSIN from data sets of z/OS records is not supported yet".into())),
                _ => {}
            }
        }
        if matches!(program, Program::Iebgener) {
            let dd = |n: &str| step.dds.iter().find(|d| d.name == n);
            if let Some(sysin) = dd("SYSIN")
                && sysin.parts.iter().any(|p| matches!(&p.source, Source::InStream(l) if l.iter().any(|c| !c.trim().is_empty())))
            {
                out.push(at("IEBGENER control statements are not supported yet".into()));
            }
            let text_of = |d: Option<&Dd>| d.map(|d| is_in_stream(d) || d.parts.iter().any(|p| p.source == Source::Sysout) || req.text);
            if let (Some(from), Some(to)) = (text_of(dd("SYSUT1")), text_of(dd("SYSUT2")))
                && from != to
            {
                out.push(at("IEBGENER between UTF-8 lines and z/OS records needs a record length, which DCB is not read for yet".into()));
            }
        }
    }
    out
}

/// A DD's file for one step, and what becomes of each data set in it when the step ends.
struct Allocated {
    /// The DD is one data set, whose state after the step the evidence journal records.
    dataset: bool,
    name: String,
    path: PathBuf,
    text: bool,
    sysout: bool,
    /// DISP=MOD: OPEN OUTPUT keeps the data set's records and writes after them.
    append: bool,
    /// The record format and length the DD gives.
    recfm: Option<String>,
    lrecl: Option<usize>,
}

struct Disposal {
    path: PathBuf,
    disp: jcl::Disp,
    created: bool,
    temporary: bool,
    /// The generation data group a new generation joins when it is kept.
    group: Option<String>,
}

/// A generation data group's base, as IDCAMS DEFINE GDG left it.
struct Gdg {
    limit: usize,
    empty: bool,
}

const GDG_MAGIC: &str = "IRONWORK-GDG";

fn gdg_text(limit: u16, scratch: bool, empty: bool) -> String {
    format!("{GDG_MAGIC} LIMIT={limit} {} {}\n", if scratch { "SCRATCH" } else { "NOSCRATCH" }, if empty { "EMPTY" } else { "NOEMPTY" })
}

struct Runner<'a> {
    req: &'a Request,
    datasets: PathBuf,
    scratch: PathBuf,
    temporaries: BTreeMap<String, PathBuf>,
    /// Each generation data group's generations when the job first named it.
    gdg_start: BTreeMap<String, Vec<u32>>,
    /// Data sets this job created that are only passed so far: deleted when the job ends.
    passed_new: BTreeSet<PathBuf>,
    files: usize,
    /// With --coverage, the paragraphs the job's programs entered and each program's outline.
    coverage: Option<RefCell<(crate::coverage::Coverage, Vec<crate::coverage::Outline>)>>,
}

fn fresh_name(dir: &Path, n: &mut usize, what: &str) -> PathBuf {
    *n += 1;
    dir.join(format!("{n:04}-{what}"))
}

impl Runner<'_> {
    /// The file of a catalogued data set, or of one member: NAME or NAME(MEMBER).
    fn catalog_path(&self, name: &str) -> PathBuf {
        match name.split_once('(') {
            Some((dsn, member)) => self.datasets.join(dsn).join(member.trim_end_matches(')')),
            None => self.datasets.join(name),
        }
    }

    fn dataset_path(&self, source: &Source) -> Option<PathBuf> {
        match source {
            Source::Dataset { dsn, member } => Some(member.as_ref().map_or_else(|| self.datasets.join(dsn), |m| self.datasets.join(dsn).join(m))),
            Source::Temporary { name, member } => {
                let base = self.temporaries.get(name).cloned().unwrap_or_else(|| self.scratch.join(format!("temp-{name}")));
                Some(member.as_ref().map_or(base.clone(), |m| base.join(m)))
            }
            _ => None,
        }
    }

    /// The DD's file: the data set itself, or a scratch file for in-stream data, DUMMY, SYSOUT
    /// or a concatenation. A data set that must exist and does not, or must not and does, is a
    /// JCL error.
    fn allocate(&mut self, step: &Step, dd: &Dd, disposals: &mut Vec<Disposal>) -> Result<Allocated, String> {
        let label = format!("{}.{}", step.shown(), dd.name);
        let mut paths = Vec::new();
        let mut text = self.req.text;
        let mut sysout = false;
        for part in &dd.parts {
            match &part.source {
                Source::InStream(lines) => {
                    let path = fresh_name(&self.scratch, &mut self.files, &label);
                    let mut body = lines.join("\n");
                    if !lines.is_empty() {
                        body.push('\n');
                    }
                    fs::write(&path, body).map_err(|e| format!("DD {}: {e}", dd.name))?;
                    text = true;
                    paths.push(path);
                }
                Source::Dummy => {
                    let path = fresh_name(&self.scratch, &mut self.files, &label);
                    fs::write(&path, b"").map_err(|e| format!("DD {}: {e}", dd.name))?;
                    paths.push(path);
                }
                Source::Sysout => {
                    let path = fresh_name(&self.scratch, &mut self.files, &label);
                    fs::write(&path, b"").map_err(|e| format!("DD {}: {e}", dd.name))?;
                    text = true;
                    sysout = true;
                    paths.push(path);
                }
                Source::Dataset { dsn, member: None } if self.gdg(dsn).is_some() => {
                    for generation in self.generations(dsn).into_iter().rev() {
                        paths.push(self.datasets.join(format!("{dsn}.G{generation:04}V00")));
                    }
                    if paths.is_empty() {
                        return Err(format!("DD {}: {dsn} has no generations", dd.name));
                    }
                }
                Source::Generation { base, relative } => {
                    let shown = format!("{base}({relative:+})").replace("(+0)", "(0)");
                    let path = self.generation_path(base, *relative).map_err(|e| format!("DD {}: {shown}: {e}", dd.name))?;
                    let fresh = !path.exists();
                    if part.disp.status == Status::New && !fresh {
                        return Err(format!("DD {}: {shown} already exists, and DISP=NEW creates it", dd.name));
                    }
                    let created = part.disp.status == Status::New || (part.disp.status == Status::Mod && fresh);
                    if created {
                        fs::write(&path, b"").map_err(|e| format!("DD {}: {e}", dd.name))?;
                    } else if fresh {
                        return Err(format!("DD {}: {shown} was not found", dd.name));
                    }
                    let disp = if created { jcl::Disp { status: Status::New, ..part.disp } } else { part.disp };
                    disposals.push(Disposal { path: path.clone(), disp, created, temporary: false, group: created.then(|| base.clone()) });
                    paths.push(path);
                }
                Source::Refer(path) => return Err(format!("DD {}: *.{path} was not resolved", dd.name)),
                source @ (Source::Dataset { .. } | Source::Temporary { .. }) => {
                    let path = self.dataset_path(source).expect("a data set has a path");
                    let (member, shown) = match source {
                        Source::Dataset { dsn, member } => (member.is_some(), member.as_ref().map_or(dsn.clone(), |m| format!("{dsn}({m})"))),
                        Source::Temporary { name, member } => (member.is_some(), format!("&&{name}")),
                        _ => unreachable!(),
                    };
                    let whole = if member { path.parent().map(Path::to_path_buf).unwrap_or_default() } else { path.clone() };
                    let temporary = matches!(source, Source::Temporary { .. });
                    let fresh = !whole.exists();
                    if part.disp.status == Status::New && !fresh {
                        return Err(format!("DD {}: {shown} already exists, and DISP=NEW creates it", dd.name));
                    }
                    // DISP=MOD creates a data set that is not there, and then disposes of it as NEW would.
                    let created = part.disp.status == Status::New || (part.disp.status == Status::Mod && fresh);
                    if created {
                        if member {
                            fs::create_dir_all(&whole).map_err(|e| format!("DD {}: {e}", dd.name))?;
                        } else {
                            fs::write(&path, b"").map_err(|e| format!("DD {}: {e}", dd.name))?;
                        }
                        if let Source::Temporary { name, .. } = source {
                            self.temporaries.insert(name.clone(), whole.clone());
                        }
                    } else if fresh {
                        return Err(format!("DD {}: {shown} was not found", dd.name));
                    } else if member != whole.is_dir() {
                        return Err(format!("DD {}: {shown} {}", dd.name, if member { "names a member of a data set that has none" } else { "is a partitioned data set; name a member" }));
                    }
                    let disp = if created { jcl::Disp { status: Status::New, ..part.disp } } else { part.disp };
                    disposals.push(Disposal { path: whole, disp, created, temporary, group: None });
                    paths.push(path);
                }
            }
        }
        if paths.len() == 1 {
            let dataset = matches!(dd.parts[0].source, Source::Dataset { .. } | Source::Temporary { .. } | Source::Generation { .. });
            let append = dd.parts[0].disp.status == Status::Mod;
            return Ok(Allocated { dataset, name: dd.name.clone(), path: paths.remove(0), text, sysout, append, recfm: dd.parts[0].recfm.clone(), lrecl: dd.parts[0].lrecl });
        }
        let joined = fresh_name(&self.scratch, &mut self.files, &label);
        let mut bytes = Vec::new();
        for p in &paths {
            bytes.extend(fs::read(p).map_err(|e| format!("DD {}: {e}", dd.name))?);
        }
        fs::write(&joined, bytes).map_err(|e| format!("DD {}: {e}", dd.name))?;
        Ok(Allocated { dataset: false, name: dd.name.clone(), path: joined, text, sysout, append: false, recfm: dd.parts[0].recfm.clone(), lrecl: dd.parts[0].lrecl })
    }

    fn dispose(&mut self, disposals: Vec<Disposal>, abended: bool) {
        for d in disposals {
            match d.disp.at_end(abended) {
                End::Delete => {
                    let _ = delete_data_set(&d.path);
                    self.passed_new.remove(&d.path);
                }
                End::Pass if d.created && !d.temporary => {
                    self.passed_new.insert(d.path);
                }
                End::Pass => {}
                End::Keep | End::Catlg | End::Uncatlg => {
                    self.passed_new.remove(&d.path);
                    if let Some(base) = &d.group {
                        self.roll_off(base);
                    }
                }
            }
        }
    }

    /// The group's base, when `base` names one.
    fn gdg(&self, base: &str) -> Option<Gdg> {
        let text = fs::read_to_string(self.datasets.join(base)).ok()?;
        let mut words = text.lines().next()?.split_whitespace();
        if words.next()? != GDG_MAGIC {
            return None;
        }
        let limit = words.next()?.strip_prefix("LIMIT=")?.parse().ok()?;
        let rest: Vec<&str> = words.collect();
        Some(Gdg { limit, empty: rest.contains(&"EMPTY") })
    }

    /// The generation numbers catalogued for `base`, oldest first.
    fn generations(&self, base: &str) -> Vec<u32> {
        let prefix = format!("{base}.G");
        let mut out: Vec<u32> = fs::read_dir(&self.datasets)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let rest = name.strip_prefix(&prefix)?;
                let (number, version) = rest.split_once('V')?;
                (number.len() == 4 && version == "00").then(|| number.parse().ok()).flatten()
            })
            .collect();
        out.sort_unstable();
        out
    }

    /// The file of a relative generation. Numbers are relative to the generations the job began
    /// with, so every (+1) in a job is the same new generation and (0) stays the one that was
    /// newest when the job began.
    fn generation_path(&mut self, base: &str, relative: i32) -> Result<PathBuf, String> {
        if self.gdg(base).is_none() {
            return Err(format!("{base} is not a generation data group; IDCAMS DEFINE GDG makes one"));
        }
        if !self.gdg_start.contains_key(base) {
            let found = self.generations(base);
            self.gdg_start.insert(base.to_string(), found);
        }
        let start = &self.gdg_start[base];
        let newest = start.last().copied().unwrap_or(0);
        let number = if relative > 0 {
            (newest + relative as u32 - 1) % 9999 + 1
        } else {
            let back = relative.unsigned_abs() as usize;
            if back >= start.len() {
                return Err("no such generation".into());
            }
            start[start.len() - 1 - back]
        };
        Ok(self.datasets.join(format!("{base}.G{number:04}V00")))
    }

    /// After a new generation is kept: past the limit, the oldest roll off, or all but the newest
    /// under EMPTY. A generation that rolls off is deleted, SCRATCH or not.
    fn roll_off(&self, base: &str) {
        let Some(g) = self.gdg(base) else { return };
        let all = self.generations(base);
        if all.len() <= g.limit {
            return;
        }
        let keep = if g.empty { 1 } else { g.limit };
        for number in &all[..all.len() - keep] {
            let _ = fs::remove_file(self.datasets.join(format!("{base}.G{number:04}V00")));
        }
    }
}

/// Runs a COBOL program with the step's DDs and PARM: its return code, or the abend's code and
/// message, with its file and line in `place` and on standard error as `run` gives them. Each
/// program CALL loads from a library goes into `called`.
#[allow(clippy::too_many_arguments)]
fn run_cobol(path: &Path, parm: &str, req: &Request, dds: &[Allocated], database: Option<&mut (dyn exec::sql::Database + '_)>, out: &mut dyn Write, called: &mut BTreeSet<PathBuf>, evidence: Option<&Rc<RefCell<crate::evidence::Run>>>, roots: &[PathBuf], place: &mut Option<(Option<String>, i64)>, coverage: Option<&RefCell<(crate::coverage::Coverage, Vec<crate::coverage::Outline>)>>) -> Result<i16, Failed> {
    let text = fs::read(path).map(|b| syntax::copy::decode(&b)).map_err(|e| Failed::before(Outcome::Unreadable, format!("{}: {e}", path.display())))?;
    let own = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let libraries = syntax::copy::Libraries::new(std::iter::once(own.clone()).chain(req.libraries.iter().cloned()).collect()).with_program(path);
    let mut programs = syntax::parse_all_with(&text, &libraries).map_err(|e| Failed::before(Outcome::Refused, e.place(&path.display().to_string()).to_string()))?;
    if let Some(c) = coverage {
        let outlines = &mut c.borrow_mut().1;
        for p in &programs {
            if !outlines.iter().any(|o| o.program == p.id) {
                outlines.push(crate::coverage::Outline::of(p));
            }
        }
    }
    let first = programs.remove(0);
    if let Some(run) = evidence {
        crate::evidence::sources(run.borrow_mut().journal_mut(), &first.sources, &path.display().to_string(), roots);
    }
    let library = exec::unit::Library { programs, dirs: std::iter::once(own).chain(req.program_dirs.iter().cloned()).collect(), copy: libraries, flags: req.flags.clone(), trace_statements: None, trace_input: false, statement_limit: req.statement_limit };
    let compiled = exec::compile(first, &req.flags).map_err(|errors| Failed::before(Outcome::Refused, syntax::most_severe(&errors).map(|e| e.place(&path.display().to_string()).to_string()).unwrap_or_default()))?;
    let specs: Vec<String> = dds.iter().map(|d| format!("{}={}{}{}", d.name, d.path.display(), if d.text { ":text" } else { "" }, if d.append { ":mod" } else { "" })).collect();
    let dds = exec::files::Dds::new(&specs, false).map_err(|m| Failed::abend(AbendCode::Ironwork, m))?;
    let sysin: Box<dyn std::io::BufRead> = match dds.get("SYSIN").and_then(|d| fs::File::open(d.path).ok()) {
        Some(f) => Box::new(std::io::BufReader::new(f)),
        None => Box::new(std::io::empty()),
    };
    let mut err = std::io::stderr();
    let loads = std::cell::RefCell::new(Vec::new());
    let observer: exec::unit::Observer<'_> = Box::new(|event| {
        if let exec::unit::Event::Load { source: Some(p), .. } = &event {
            loads.borrow_mut().push(p.to_path_buf());
        }
        if let Some(c) = coverage {
            c.borrow_mut().0.observe(&event);
        }
        if let Some(run) = evidence {
            run.borrow_mut().observe(event);
        }
    });
    let ended = compiled.execute_main(library, dds, Some(sysin), req.clock, database, out, &mut err, Some(observer), parm);
    called.extend(loads.into_inner());
    match ended {
        Ok((_, rc)) => Ok(rc),
        Err(exec::Abend { code: AbendCode::Signal(Signal::ClosedOutput), .. }) => Ok(0),
        Err(a) => {
            let (file, shown) = (crate::abend_file(&compiled, &a), path.display().to_string());
            eprintln!("{}:{}: ABEND {}: {}", file.filter(|f| !f.is_empty()).unwrap_or(&shown), a.pos, a.code, a.message);
            *place = Some((file.map(str::to_string), i64::from(a.pos.line)));
            Err(Failed::abend(a.code, a.message))
        }
    }
}

/// Deletes a data set, and the mark beside it that says a run left it open for output
/// ([`exec::files::open_mark`]).
fn delete_data_set(path: &Path) -> std::io::Result<()> {
    let gone = if path.is_dir() { fs::remove_dir_all(path) } else { fs::remove_file(path) };
    let unmarked = exec::files::clear_open_mark(path);
    gone.and(unmarked)
}

/// A step that ended without a return code: the abend its log line, COND and IF see, and what the
/// job's exit status makes of it.
struct Failed {
    code: AbendCode,
    message: String,
    outcome: Outcome,
}

impl Failed {
    fn abend(code: AbendCode, message: String) -> Self {
        Self { outcome: Outcome::of_abend(&code), code, message }
    }

    /// What kept a COBOL step's program from running, which COND and IF see as an IRONWORK abend.
    fn before(outcome: Outcome, message: String) -> Self {
        Self { code: AbendCode::Ironwork, message, outcome }
    }
}

/// Writes `bytes` to `path`, after what it holds when `append`.
fn put(path: &Path, bytes: &[u8], append: bool) -> std::io::Result<()> {
    if !append {
        return fs::write(path, bytes);
    }
    fs::OpenOptions::new().append(true).create(true).open(path)?.write_all(bytes)
}

/// IEBGENER with no control statements: SYSUT1 copied to SYSUT2 as it stands. Without either
/// DD it ends with return code 12.
fn iebgener(dds: &[Allocated]) -> Result<i16, Failed> {
    let dd = |n: &str| dds.iter().find(|d| d.name == n);
    let (Some(from), Some(to)) = (dd("SYSUT1"), dd("SYSUT2")) else { return Ok(12) };
    let bytes = fs::read(&from.path).map_err(|e| Failed::abend(AbendCode::Ironwork, format!("SYSUT1: {e}")))?;
    put(&to.path, &bytes, to.append).map_err(|e| Failed::abend(AbendCode::Ironwork, format!("SYSUT2: {e}")))?;
    Ok(0)
}

/// IDCAMS over the step's SYSIN: each command's condition code is LASTCC, the highest is MAXCC
/// and the step's return code, and a code of 16 ends the commands. Messages go to SYSPRINT.
fn idcams(runner: &Runner<'_>, dds: &[Allocated]) -> i16 {
    use jcl::idcams::{Command, Target};
    let dd = |n: &str| dds.iter().find(|d| d.name == n);
    let mut print = Vec::new();
    let cards = dd("SYSIN").and_then(|d| fs::read_to_string(&d.path).ok()).map(|t| t.lines().map(str::to_string).collect::<Vec<_>>()).unwrap_or_default();
    let commands = match jcl::idcams::parse(&cards) {
        Ok(c) => c,
        Err(e) => {
            print.push(format!("IDCAMS: {e}"));
            print.push("IDC0002I IDCAMS PROCESSING COMPLETE. MAXIMUM CONDITION CODE WAS 12".into());
            write_print(dd("SYSPRINT"), &print);
            return 12;
        }
    };
    let text_of = |t: &Target| match t {
        Target::Dd(n) => dd(n).map(|d| (d.path.clone(), d.text, d.append)),
        Target::Dataset(name) => Some((runner.catalog_path(name), runner.req.text, false)),
    };
    let shown = |t: &Target| match t {
        Target::Dd(n) => format!("DD {n}"),
        Target::Dataset(n) => n.clone(),
    };
    fn run(commands: &[Command], cc: &mut (u16, u16), print: &mut Vec<String>, act: &mut dyn FnMut(&Command, &mut Vec<String>) -> u16) {
        for c in commands {
            if cc.1 >= 16 {
                return;
            }
            match c {
                Command::Set { max: true, value } => cc.1 = *value,
                Command::Set { max: false, value } => {
                    cc.0 = *value;
                    cc.1 = cc.1.max(*value);
                }
                Command::If { max, op, value, then, otherwise } => {
                    let tested = if *max { cc.1 } else { cc.0 };
                    let branch = if op.holds(tested, *value) { then } else { otherwise };
                    run(branch, cc, print, act);
                }
                other => {
                    let code = act(other, print);
                    print.push(format!("IDC0001I FUNCTION COMPLETED, HIGHEST CONDITION CODE WAS {code}"));
                    cc.0 = code;
                    cc.1 = cc.1.max(code);
                }
            }
        }
    }
    let mut act = |c: &Command, print: &mut Vec<String>| -> u16 {
        match c {
            Command::Delete(names) => {
                let mut code = 0;
                for name in names {
                    if runner.gdg(name).is_some() {
                        for number in runner.generations(name) {
                            let _ = fs::remove_file(runner.datasets.join(format!("{name}.G{number:04}V00")));
                        }
                    }
                    match delete_data_set(&runner.catalog_path(name)) {
                        Ok(()) => print.push(format!("IDC0550I ENTRY (A) {name} DELETED")),
                        Err(_) => {
                            print.push(format!("IDC3012I ENTRY {name} NOT FOUND"));
                            code = 8;
                        }
                    }
                }
                code
            }
            Command::DefineGdg { name, limit, scratch, empty } => {
                let path = runner.catalog_path(name);
                if path.exists() {
                    print.push(format!("ironwork: DEFINE GDG {name}: the name is in use"));
                    return 12;
                }
                match fs::write(&path, gdg_text(*limit, *scratch, *empty)) {
                    Ok(()) => 0,
                    Err(e) => {
                        print.push(format!("ironwork: DEFINE GDG {name}: {e}"));
                        12
                    }
                }
            }
            Command::DefineCluster(name) => {
                let path = runner.catalog_path(name);
                if path.exists() {
                    print.push(format!("ironwork: DEFINE CLUSTER {name}: the data set exists"));
                    return 12;
                }
                match fs::write(&path, b"").and_then(|()| exec::files::clear_open_mark(&path)) {
                    Ok(()) => 0,
                    Err(e) => {
                        print.push(format!("ironwork: DEFINE CLUSTER {name}: {e}"));
                        12
                    }
                }
            }
            Command::Repro { from, to } => {
                let (Some((source, source_text, _)), Some((target, target_text, append))) = (text_of(from), text_of(to)) else {
                    print.push(format!("ironwork: REPRO: {} or {} is not allocated to the step", shown(from), shown(to)));
                    return 12;
                };
                if source_text != target_text {
                    print.push(format!("ironwork: REPRO from {} to {}: one holds UTF-8 lines and the other z/OS records", shown(from), shown(to)));
                    return 12;
                }
                if !target.is_file() {
                    print.push(format!("ironwork: REPRO: {} does not exist", shown(to)));
                    return 12;
                }
                match fs::read(&source).and_then(|b| put(&target, &b, append)).and_then(|()| exec::files::clear_open_mark(&target)) {
                    Ok(()) => 0,
                    Err(e) => {
                        print.push(format!("ironwork: REPRO from {} to {}: {e}", shown(from), shown(to)));
                        12
                    }
                }
            }
            Command::Set { .. } | Command::If { .. } => 0,
        }
    };
    let mut cc = (0u16, 0u16);
    run(&commands, &mut cc, &mut print, &mut act);
    print.push(format!("IDC0002I IDCAMS PROCESSING COMPLETE. MAXIMUM CONDITION CODE WAS {}", cc.1));
    write_print(dd("SYSPRINT"), &print);
    cc.1 as i16
}

fn write_print(dd: Option<&Allocated>, lines: &[String]) {
    if let Some(d) = dd {
        let mut text = fs::read_to_string(&d.path).unwrap_or_default();
        for l in lines {
            text.push_str(l);
            text.push('\n');
        }
        let _ = fs::write(&d.path, text);
    }
}

/// A data set's records: lines of a text data set as EBCDIC bytes, variable-length records with
/// their RDWs, or fixed-length ones.
enum Layout {
    Lines,
    Variable,
    Fixed(usize),
}

fn layout(dd: &Allocated, record: Option<jcl::sort::Record>) -> Result<Layout, String> {
    if dd.text {
        return Ok(Layout::Lines);
    }
    match (dd.recfm.as_deref().map(|r| r.starts_with('V')), dd.lrecl, record) {
        (Some(true), _, _) => Ok(Layout::Variable),
        (Some(false), Some(n), _) => Ok(Layout::Fixed(n)),
        (None, _, Some(r)) if r.variable => Ok(Layout::Variable),
        (None, _, Some(jcl::sort::Record { length: Some(n), .. })) => Ok(Layout::Fixed(n)),
        _ => Err(format!("the record format of DD {} is not known: give DCB=(RECFM=FB,LRECL=n), RECFM=VB, or a RECORD statement", dd.name)),
    }
}

fn read_records(dd: &Allocated, layout: &Layout, page: &CodePage) -> Result<Vec<Vec<u8>>, String> {
    let bytes = fs::read(&dd.path).map_err(|e| format!("DD {}: {e}", dd.name))?;
    match layout {
        Layout::Lines => Ok(String::from_utf8_lossy(&bytes).lines().map(|l| page.encode_lossy(l)).collect()),
        Layout::Fixed(n) => {
            if !bytes.len().is_multiple_of(*n) {
                return Err(format!("DD {} holds {} bytes, not a whole number of {n}-byte records", dd.name, bytes.len()));
            }
            Ok(bytes.chunks(*n).map(<[u8]>::to_vec).collect())
        }
        Layout::Variable => {
            let mut out = Vec::new();
            let mut at = 0;
            while at < bytes.len() {
                let len = bytes.get(at..at + 2).map(|b| usize::from(u16::from_be_bytes([b[0], b[1]]))).filter(|&l| l >= 4 && at + l <= bytes.len()).ok_or_else(|| format!("DD {} has a record descriptor word that is not valid at byte {at}", dd.name))?;
                out.push(bytes[at..at + len].to_vec());
                at += len;
            }
            Ok(out)
        }
    }
}

/// The records as SORTOUT holds them: lines decoded through the code page with trailing blanks
/// dropped (and a variable record's RDW left out), or the record bytes as they are.
fn write_records(dd: &Allocated, records: &[Vec<u8>], variable: bool, page: &CodePage) -> Result<(), String> {
    let mut bytes = Vec::new();
    for r in records {
        if dd.text {
            let body = if variable { &r[4.min(r.len())..] } else { &r[..] };
            bytes.extend(page.decode(body).trim_end().as_bytes());
            bytes.push(b'\n');
        } else {
            bytes.extend(r);
        }
    }
    put(&dd.path, &bytes, dd.append).map_err(|e| format!("DD {}: {e}", dd.name))
}

/// DFSORT over the step's DDs: SORT, MERGE or COPY from SORTIN (or SORTIN01-99 for MERGE) to
/// SORTOUT or the OUTFIL groups' DDs, in DFSORT's order: INCLUDE or OMIT, INREC, the sort, SUM
/// FIELDS=NONE keeping the first of records with equal keys, OUTREC, then each OUTFIL group's
/// selection and reformatting. Text records are EBCDIC through the code page while they are
/// sorted, so CH keys collate as on z/OS, and are padded with blanks to the longest so a key past
/// a line's end reads blanks. Return code 0, or 16 with the reason on SYSOUT.
fn sort_step(dds: &[Allocated]) -> i16 {
    let dd = |n: &str| dds.iter().find(|d| d.name == n);
    let fail = |why: String| {
        write_print(dd("SYSOUT"), &[format!("ironwork SORT: {why}")]);
        16
    };
    let cards = dd("SYSIN").and_then(|d| fs::read_to_string(&d.path).ok()).map(|t| t.lines().map(str::to_string).collect::<Vec<_>>()).unwrap_or_default();
    let control = match jcl::sort::parse(&cards) {
        Ok(c) => c,
        Err(e) => return fail(e),
    };
    let page = numeric::options::Options::default().code_page();
    let inputs: Vec<&Allocated> = match control.kind {
        jcl::sort::Kind::Merge => (1..=99).filter_map(|n| dd(&format!("SORTIN{n:02}"))).collect(),
        _ => dd("SORTIN").into_iter().collect(),
    };
    if inputs.is_empty() {
        return fail(format!("no {} DD", if control.kind == jcl::sort::Kind::Merge { "SORTIN01" } else { "SORTIN" }));
    }
    let groups: Vec<jcl::sort::Outfil> = if control.outfil.is_empty() {
        vec![jcl::sort::Outfil { names: vec!["SORTOUT".into()], selection: None, save: false, edit: None }]
    } else {
        control.outfil.clone()
    };
    let mut targets: Vec<Vec<&Allocated>> = Vec::new();
    for group in &groups {
        let mut these = Vec::new();
        for name in &group.names {
            match dd(name) {
                Some(d) => these.push(d),
                None => return fail(format!("no {name} DD")),
            }
        }
        targets.push(these);
    }
    let keys = rt::sort::Keys::new(
        control
            .fields
            .iter()
            .map(|f| rt::sort::Key {
                position: f.position - 1,
                length: f.length,
                format: match f.format {
                    jcl::sort::Format::Ch => rt::sort::Format::Ch,
                    jcl::sort::Format::Ac => rt::sort::Format::Ac,
                    jcl::sort::Format::Zd => rt::sort::Format::Zd,
                    jcl::sort::Format::Clo => rt::sort::Format::Clo,
                    jcl::sort::Format::Csl => rt::sort::Format::Csl,
                    jcl::sort::Format::Cst => rt::sort::Format::Cst,
                    jcl::sort::Format::Pd => rt::sort::Format::Pd,
                    jcl::sort::Format::Bi => rt::sort::Format::Bi,
                    jcl::sort::Format::Fi => rt::sort::Format::Fi,
                },
                ascending: f.ascending,
            })
            .collect(),
        page,
    );
    let mut records = Vec::new();
    let mut variable = false;
    for input in &inputs {
        let shape = match layout(input, control.record) {
            Ok(l) => l,
            Err(e) => return fail(e),
        };
        variable |= matches!(shape, Layout::Variable);
        let mut these = match read_records(input, &shape, page) {
            Ok(r) => r,
            Err(e) => return fail(e),
        };
        if matches!(shape, Layout::Lines) {
            let width = these.iter().map(Vec::len).max().unwrap_or(0).max(input.lrecl.unwrap_or(0));
            for r in &mut these {
                r.resize(width, 0x40);
            }
        }
        if let Some(selection) = &control.selection {
            let mut kept = Vec::with_capacity(these.len());
            for r in these {
                match crate::dfsort::keeps(selection, &r, page) {
                    Ok(true) => kept.push(r),
                    Ok(false) => {}
                    Err(e) => return fail(format!("{} in DD {}: {e}", if selection.include { "INCLUDE" } else { "OMIT" }, input.name)),
                }
            }
            these = kept;
        }
        if let Some(edit) = &control.inrec {
            match these.iter().map(|r| crate::dfsort::reformat(edit, r, matches!(shape, Layout::Variable), page)).collect() {
                Ok(r) => these = r,
                Err(e) => return fail(format!("INREC in DD {}: {e}", input.name)),
            }
        }
        if control.kind == jcl::sort::Kind::Merge {
            match keys.out_of_order(&these) {
                Ok(Some(i)) => return fail(format!("record {} of DD {} is out of order for the MERGE", i + 1, input.name)),
                Ok(None) => {}
                Err(e) => return fail(format!("DD {}: {e}", input.name)),
            }
        }
        records.extend(these);
    }
    let mut sorted = if control.kind == jcl::sort::Kind::Copy {
        records
    } else {
        match keys.sort(records) {
            Ok(r) => r,
            Err(e) => return fail(e.to_string()),
        }
    };
    if control.drop_duplicates && !control.fields.is_empty() {
        let mut kept: Vec<Vec<u8>> = Vec::with_capacity(sorted.len());
        for r in sorted {
            if kept.last().is_some_and(|k| keys.compare(k, &r).ok() == Some(std::cmp::Ordering::Equal)) {
                continue;
            }
            kept.push(r);
        }
        sorted = kept;
    }
    if let Some(edit) = &control.outrec {
        match sorted.iter().map(|r| crate::dfsort::reformat(edit, r, variable, page)).collect() {
            Ok(r) => sorted = r,
            Err(e) => return fail(format!("OUTREC: {e}")),
        }
    }
    let mut outputs: Vec<Vec<Vec<u8>>> = vec![Vec::new(); groups.len()];
    for record in &sorted {
        let mut taken = false;
        for (k, group) in groups.iter().enumerate() {
            if group.save {
                continue;
            }
            let selected = match &group.selection {
                Some(s) => crate::dfsort::keeps(s, record, page),
                None => Ok(true),
            };
            match selected {
                Ok(true) => {
                    taken = true;
                    outputs[k].push(record.clone());
                }
                Ok(false) => {}
                Err(e) => return fail(format!("OUTFIL {}: {e}", group.names.join(","))),
            }
        }
        if !taken {
            for (k, _) in groups.iter().enumerate().filter(|(_, g)| g.save) {
                outputs[k].push(record.clone());
            }
        }
    }
    let mut written = Vec::new();
    for ((group, records), dds) in groups.iter().zip(outputs).zip(&targets) {
        let records = match &group.edit {
            Some(edit) => match records.iter().map(|r| crate::dfsort::reformat(edit, r, variable, page)).collect::<Result<Vec<_>, _>>() {
                Ok(r) => r,
                Err(e) => return fail(format!("OUTFIL {}: {e}", group.names.join(","))),
            },
            None => records,
        };
        for out in dds {
            if let Err(e) = write_records(out, &records, variable, page) {
                return fail(e);
            }
            written.push(format!("ironwork SORT: {} records written to {}", records.len(), out.name));
        }
    }
    write_print(dd("SYSOUT"), &written);
    0
}

struct Frame {
    active: bool,
    parent: bool,
    value: bool,
    abend_aware: bool,
}

pub fn run(req: Request) -> ExitCode {
    let shown = req.jcl.display().to_string();
    let mut job = match parse(&req.jcl, &req.datasets, &req.proclibs, req.user.as_deref()) {
        Ok(j) => j,
        Err((outcome, e)) => {
            eprintln!("ironwork: {e}");
            return exit::job_status(outcome);
        }
    };
    if let Err(e) = override_steps(&mut job, &req) {
        return crate::usage_error(&e);
    }
    if !req.datasets.is_dir() {
        eprintln!("ironwork: --datasets {}: not a directory", req.datasets.display());
        return exit::job_status(Outcome::Usage);
    }
    let refused = refusals(&job, &req);
    if !refused.is_empty() {
        for r in refused {
            eprintln!("ironwork: {shown}: {r}");
        }
        return exit::job_status(Outcome::NotRun);
    }
    let mut replay = match &req.replay {
        Some(file) => match fs::read_to_string(file).map_err(|e| e.to_string()).and_then(|t| exec::sql::Replay::parse(&t, false)) {
            Ok(r) => Some(r),
            Err(e) => {
                eprintln!("ironwork: --sql-replay {}: {e}", file.display());
                return exit::job_status(Outcome::Usage);
            }
        },
        None => None,
    };
    let scratch = match crate::compare::scratch("job") {
        Ok(d) => d,
        Err(e) => {
            eprintln!("ironwork: a scratch directory: {e}");
            return exit::job_status(Outcome::Internal);
        }
    };
    let declared = match &req.declare {
        Some(file) => match fs::read_to_string(file).map_err(|e| e.to_string()).and_then(|t| crate::compare::parse_declared(&t)) {
            Ok(d) => d,
            Err(e) => return crate::usage_error(&format!("--declare {}: {e}", file.display())),
        },
        None => Vec::new(),
    };
    let datasets = match &req.expected {
        Some(_) => {
            let copy = scratch.join("datasets");
            if let Err(e) = copy_tree(&req.datasets, &copy) {
                eprintln!("ironwork: copying --datasets {}: {e}", req.datasets.display());
                let _ = fs::remove_dir_all(&scratch);
                return exit::job_status(Outcome::Usage);
            }
            copy
        }
        None => req.datasets.clone(),
    };
    let inputs: Vec<(String, Value)> = if req.expected.is_some() { files_under(&datasets).into_iter().map(|(n, p)| (n, crate::compare::digest_of(fs::read(p).ok().as_deref()))).collect() } else { Vec::new() };
    let coverage = req.coverage.as_ref().map(|_| RefCell::new(Default::default()));
    let mut runner = Runner { req: &req, datasets, scratch, temporaries: BTreeMap::new(), gdg_start: BTreeMap::new(), passed_new: BTreeSet::new(), files: 0, coverage };
    let roots: Vec<PathBuf> = std::iter::once(req.jcl.parent().map(Path::to_path_buf).unwrap_or_default())
        .chain([req.datasets.clone()])
        .chain(req.libraries.iter().cloned())
        .chain(req.program_dirs.iter().cloned())
        .chain(req.proclibs.iter().cloned())
        .collect();
    let journal = match &req.evidence {
        Some(dir) => match crate::evidence::start(dir, &roots, "job", &shown) {
            Ok(mut j) => {
                crate::evidence::sources(&mut j, &[], &shown, &roots);
                Some(j)
            }
            Err(e) => {
                eprintln!("ironwork: --evidence {}: {e}", dir.display());
                let _ = fs::remove_dir_all(&runner.scratch);
                return exit::job_status(Outcome::Usage);
            }
        },
        None => None,
    };
    let journal = RefCell::new(journal);
    let report = run_job(&job, &mut runner, replay.as_mut().map(|r| r as &mut dyn exec::sql::Database), &journal, &roots);
    if let Some(j) = journal.into_inner() {
        crate::evidence::finish(Some(j), exit::recorded(report.outcome));
    }
    if let (Some(file), Some(c)) = (&req.coverage, &runner.coverage) {
        let (covered, outlines) = &*c.borrow();
        if let Err(e) = fs::write(file, format!("{}\n", exec::evidence::canonical(&covered.report(outlines)))) {
            eprintln!("ironwork: --coverage {}: {e}", file.display());
        }
    }
    for path in std::mem::take(&mut runner.passed_new) {
        let _ = delete_data_set(&path);
    }
    let code = match &req.expected {
        Some(expected) => equivalence(&req, &job, &report, expected, &runner.datasets, &inputs, &declared),
        None => exit::job_status(report.outcome),
    };
    let _ = fs::remove_dir_all(&runner.scratch);
    code
}

/// Runs every step the job's conditions allow. The job ends as the first step that ended without
/// a return code says, or a JCL error that ended it, and otherwise with the highest return code.
/// With a journal, each step's DDs, CALLs, sources and outcome go into it as the step ends.
fn run_job(job: &Job, runner: &mut Runner<'_>, mut database: Option<&mut dyn exec::sql::Database>, journal: &RefCell<Option<exec::evidence::Journal>>, roots: &[PathBuf]) -> Report {
    let (mut ran, mut abended, mut stopped) = (Vec::<Ran>::new(), false, None);
    let mut frames: Vec<Frame> = Vec::new();
    let mut stdout = std::io::stdout().lock();
    let (mut steps, mut gaps, mut programs) = (Vec::new(), Vec::new(), BTreeSet::new());
    let mut log = |name: &str, pgm: &str, what: String| {
        eprintln!("ironwork job {}: {name} PGM={pgm} {what}", job.name);
        if let Some(j) = journal.borrow_mut().as_mut() {
            let _ = j.append("step", fields([("step", name.into()), ("pgm", pgm.into()), ("outcome", what.clone().into())]));
        }
        steps.push(Value::Obj(fields([("step", name.into()), ("pgm", pgm.into()), ("outcome", what.into())])));
    };
    let mut ended = false;
    for item in &job.items {
        match item {
            Item::If { expr, caller, .. } => {
                let parent = frames.last().is_none_or(|f| f.active);
                let value = cond::eval(expr, &ran, caller.as_deref());
                frames.push(Frame { active: parent && value, parent, value, abend_aware: cond::tests_abend(expr) });
            }
            Item::Else { .. } => {
                if let Some(f) = frames.last_mut() {
                    f.active = f.parent && !f.value;
                }
            }
            Item::EndIf { .. } => {
                frames.pop();
            }
            Item::Step(step) => {
                let shown = step.shown();
                let name = shown.as_str();
                if ended {
                    log(name, &step.pgm, "BYPASSED: the job ended".into());
                    continue;
                }
                if !frames.iter().all(|f| f.active) {
                    log(name, &step.pgm, "BYPASSED: its IF branch is not taken".into());
                    continue;
                }
                if let Some(reason) = cond::bypassed_by_cond(&job.cond, &ran, false, None) {
                    log(name, &step.pgm, format!("BYPASSED: the JOB statement's {reason}; the job ends"));
                    ended = true;
                    continue;
                }
                let abend_tested = frames.iter().any(|f| f.abend_aware);
                if let Some(reason) = cond::bypassed_by_cond(&step.cond, &ran, abended && !abend_tested, step.caller.as_deref()) {
                    log(name, &step.pgm, format!("BYPASSED: {reason}"));
                    continue;
                }
                let mut disposals = Vec::new();
                let mut dds = Vec::new();
                let mut jcl_error = None;
                for dd in allocated_dds(step) {
                    match runner.allocate(step, dd, &mut disposals) {
                        Ok(a) => dds.push(a),
                        Err(e) => {
                            jcl_error = Some(e);
                            break;
                        }
                    }
                }
                if let Some(e) = jcl_error {
                    log(name, &step.pgm, format!("JCL ERROR: {e}; the job ends"));
                    runner.dispose(disposals, true);
                    stopped.get_or_insert(Outcome::Abend);
                    ended = true;
                    continue;
                }
                let program = program_of(&step.pgm, &runner.req.program_dirs);
                let source = match &program {
                    Program::Cobol(path) => path.display().to_string(),
                    _ => step.pgm.clone(),
                };
                let run = journal.borrow_mut().take().map(|j| {
                    let mut r = crate::evidence::Run::new(j, roots, &source, runner.req.trace_marker.as_deref());
                    for d in dds.iter().filter(|d| d.dataset) {
                        r.track(&d.name, &d.path);
                    }
                    Rc::new(RefCell::new(r))
                });
                let mut place = None;
                let outcome = match program {
                    Program::Iefbr14 => Ok(0),
                    Program::Iebgener => iebgener(&dds),
                    Program::Idcams => Ok(idcams(runner, &dds)),
                    Program::Sort => Ok(sort_step(&dds)),
                    Program::Cobol(path) => {
                        programs.insert(path.clone());
                        run_cobol(&path, step.parm.as_deref().unwrap_or(""), runner.req, &dds, database.as_deref_mut(), &mut stdout, &mut programs, run.as_ref(), roots, &mut place, runner.coverage.as_ref())
                    }
                    Program::Missing => Err(Failed::abend(AbendCode::ModuleNotFound, format!("program {} is not in the program libraries", step.pgm))),
                };
                for d in dds.iter().filter(|d| d.sysout) {
                    if let Ok(bytes) = fs::read(&d.path) {
                        let _ = stdout.write_all(&bytes);
                    }
                }
                let _ = stdout.flush();
                if let Some(run) = run.and_then(|r| Rc::try_unwrap(r).ok()) {
                    let abend = outcome.as_ref().err().map(|failed| (failed.code.to_string(), place.as_ref().and_then(|(f, _)| f.as_deref()), place.as_ref().map_or(0, |(_, l)| *l)));
                    *journal.borrow_mut() = Some(run.into_inner().end(abend));
                }
                match outcome {
                    Ok(rc) => {
                        let rc = rc.clamp(0, 4095) as u16;
                        log(name, &step.pgm, format!("RC={rc:04}"));
                        runner.dispose(disposals, false);
                        ran.push(Ran { name: step.name.clone(), caller: step.caller.clone(), rc: Some(rc), abend: None });
                    }
                    Err(Failed { code, message, outcome: exits }) => {
                        if code == AbendCode::Ironwork {
                            gaps.push(format!("step {name} reached what ironwork does not model: {message}"));
                        }
                        log(name, &step.pgm, format!("ABEND {code}: {message}"));
                        runner.dispose(disposals, true);
                        abended = true;
                        stopped.get_or_insert(exits);
                        ran.push(Ran { name: step.name.clone(), caller: step.caller.clone(), rc: None, abend: Some(code.to_string()) });
                    }
                }
            }
        }
    }
    let outcome = stopped.unwrap_or_else(|| Outcome::Ended(ran.iter().filter_map(|r| r.rc).max().map_or(0, i64::from)));
    Report { outcome, steps, gaps, programs }
}

/// What a job did: how it ended, a record per step, what it reached that ironwork does not model,
/// and the COBOL programs it ran.
struct Report {
    outcome: Outcome,
    steps: Vec<Value>,
    gaps: Vec<String>,
    programs: BTreeSet<PathBuf>,
}

pub(crate) fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    // nosemgrep: rust.actix.path-traversal.tainted-path.tainted-path -- each name is one component read from the directory, and links are skipped
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        } else if kind.is_file() {
            fs::copy(entry.path(), to.join(entry.file_name()))?;
        }
    }
    Ok(())
}

/// The files under `dir` by data set name, A.B or A.B(M), in order.
fn files_under(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else { return out };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if path.is_dir() {
            for member in fs::read_dir(&path).into_iter().flatten().flatten() {
                if member.path().is_file() {
                    out.push((format!("{name}({})", member.file_name().to_string_lossy()), member.path()));
                }
            }
        } else if path.is_file() {
            out.push((name, path));
        }
    }
    out.sort();
    out
}

pub const JOB_PREDICATE: &str = "https://github.com/Portll/ironwork/blob/main/docs/evidence.md#job-equivalence-v1";

/// Each data set production left, compared with the one the job left: an in-toto statement whose
/// subjects are the JCL and the programs the job ran. Exit 0 equivalent or equivalent as
/// declared, 1 diverged, 3 inconclusive.
fn equivalence(req: &Request, job: &Job, report: &Report, expected: &Path, left: &Path, inputs: &[(String, Value)], declared: &[crate::compare::Declared]) -> ExitCode {
    use crate::compare::{Assessment, assess, digest_of};
    let (mut results, mut undeclared, mut declared_hit) = (Vec::new(), 0usize, 0usize);
    let wanted = files_under(expected);
    if wanted.is_empty() {
        return crate::usage_error(&format!("--expected {}: no data sets to compare", expected.display()));
    }
    if let Some(file) = &req.expected_steps {
        let text = match fs::read_to_string(file) {
            Ok(t) => t,
            Err(e) => return crate::usage_error(&format!("--expected STEPS={}: {e}", file.display())),
        };
        let outcome_of = |step: &str| {
            report.steps.iter().find_map(|v| match v {
                Value::Obj(m) if m.get("step") == Some(&Value::Str(step.to_string())) => match m.get("outcome") {
                    Some(Value::Str(o)) => Some(o.split(':').next().unwrap_or(o).trim().to_string()),
                    _ => None,
                },
                _ => None,
            })
        };
        for (n, line) in text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
            let Some((step, want)) = line.trim().split_once(char::is_whitespace) else { return crate::usage_error(&format!("--expected STEPS line {}: STEP RC=nnnn or STEP ABEND code", n + 1)) };
            let want = want.trim().to_string();
            let got = outcome_of(step);
            let (mut r, assessment) = assess(&format!("STEP {step}"), Some(want.as_bytes()), got.as_deref().map(str::as_bytes), declared);
            match assessment {
                Assessment::Declared => declared_hit += 1,
                Assessment::Undeclared => undeclared += 1,
                Assessment::Same => {}
            }
            r.insert("expected".into(), want.into());
            r.insert("actual".into(), got.map_or(Value::Null, Value::Str));
            results.push(Value::Obj(r));
        }
    }
    for (name, path) in &wanted {
        let want = fs::read(path).ok();
        let (dsn, member) = match name.split_once('(') {
            Some((d, m)) => (d, Some(m.trim_end_matches(')'))),
            None => (name.as_str(), None),
        };
        let got = fs::read(member.map_or_else(|| left.join(dsn), |m| left.join(dsn).join(m))).ok();
        let (r, assessment) = assess(&format!("DATASET {name}"), want.as_deref(), got.as_deref(), declared);
        match assessment {
            Assessment::Declared => declared_hit += 1,
            Assessment::Undeclared => undeclared += 1,
            Assessment::Same => {}
        }
        results.push(Value::Obj(r));
    }
    let verdict = if !report.gaps.is_empty() {
        "inconclusive"
    } else if undeclared > 0 {
        "diverged"
    } else if declared_hit > 0 {
        "equivalent-as-declared"
    } else {
        "equivalent"
    };
    let subject = |name: String, path: &Path| Value::Obj(fields([("name", name.into()), ("digest", Value::Obj(fields([("sha256", digest_of(fs::read(path).ok().as_deref()))])))]));
    let file_name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut subjects = vec![subject(format!("job:{}", file_name(&req.jcl)), &req.jcl)];
    subjects.extend(report.programs.iter().map(|p| subject(format!("program:{}", file_name(p)), p)));
    let predicate = fields([
        ("verdict", verdict.into()),
        ("job", job.name.clone().into()),
        ("inputs", Value::Arr(inputs.iter().map(|(n, d)| Value::Obj(fields([("dataset", n.clone().into()), ("sha256", d.clone())]))).collect())),
        ("sqlRecording", req.replay.as_ref().map_or(Value::Null, |p| digest_of(fs::read(p).ok().as_deref()))),
        ("clock", crate::compare::clock_value(req.clock)),
        ("steps", Value::Arr(report.steps.clone())),
        ("results", Value::Arr(results)),
        ("declared", Value::Arr(declared.iter().map(|d| Value::Obj(fields([("what", d.what.clone().into()), ("reason", d.reason.clone().into())]))).collect())),
        ("inconclusive", Value::Arr(report.gaps.iter().map(|g| Value::Str(g.clone())).collect())),
        ("coverage", Value::Null),
        ("ironwork", env!("CARGO_PKG_VERSION").into()),
        ("limit", crate::compare::LIMIT.into()),
    ]);
    let statement = Value::Obj(fields([("_type", "https://in-toto.io/Statement/v1".into()), ("subject", Value::Arr(subjects)), ("predicateType", JOB_PREDICATE.into()), ("predicate", Value::Obj(predicate))]));
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
    eprintln!("ironwork job {}: {verdict}{}", job.name, if undeclared > 0 { format!(", {undeclared} undeclared divergence(s)") } else { String::new() });
    ExitCode::from(match verdict {
        "equivalent" | "equivalent-as-declared" => 0,
        "diverged" => 1,
        _ => 3,
    })
}
