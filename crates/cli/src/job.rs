//! `ironwork job`: a job's steps run in order against a directory of data sets. Each EXEC PGM=
//! runs a COBOL program from the program libraries, or IEFBR14 or IEBGENER; DD statements become
//! the files a step's DDs stand for, and dispositions create, keep and delete them as the step
//! ends. COND and IF/THEN/ELSE decide which steps run, from the return codes and abends before.

use exec::abend::{AbendCode, Signal};
use jcl::cond::{self, Ran};
use jcl::{Dd, End, Item, Job, Source, Status, Step};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub struct Request {
    pub jcl: PathBuf,
    pub datasets: PathBuf,
    /// Data sets hold UTF-8 lines rather than z/OS records.
    pub text: bool,
    pub libraries: Vec<PathBuf>,
    pub program_dirs: Vec<PathBuf>,
    pub flags: Vec<String>,
    pub clock: exec::unit::Clock,
    pub replay: Option<PathBuf>,
}

/// IBM programs a job can name that ironwork does not run; each is refused before the job starts.
const NOT_SUPPORTED: &[&str] = &[
    "SORT", "ICEMAN", "DFSORT", "SYNCSORT", "ICETOOL", "IDCAMS", "IEBCOPY", "IEBUPDTE", "IEBPTPCH", "IEBCOMPR", "IEBDG", "IEHLIST", "IEHPROGM", "IEHMOVE", "IKJEFT01", "IKJEFT1A", "IKJEFT1B", "IRXJCL", "BPXBATCH", "BPXBATSL", "FTP", "DSNUTILB", "DSNUPROC", "DSNTEP2", "DSNTEP4", "DSNTIAUL", "DSNTIAD", "DFSRRC00", "ADRDSSU", "IEWL", "IEWBLINK", "HEWL", "IGYCRCTL", "ASMA90", "DFHECP1$", "DFHEAP1$", "IEBEDIT", "AMASPZAP",
];

enum Program {
    Iefbr14,
    Iebgener,
    Cobol(PathBuf),
    Missing,
}

fn program_of(pgm: &str, dirs: &[PathBuf]) -> Program {
    match pgm {
        "IEFBR14" => return Program::Iefbr14,
        "IEBGENER" | "ICEGENER" => return Program::Iebgener,
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
        if step.parm.is_some() && !matches!(program, Program::Iefbr14) {
            out.push(at(format!("PARM for PGM={} is not supported yet", step.pgm)));
        }
        for dd in &step.dds {
            for part in &dd.parts {
                if part.disp.status == Status::Mod {
                    out.push(format!("line {}: DISP=MOD is not supported yet", part.line));
                }
            }
            let text_parts = dd.parts.iter().filter(|p| matches!(p.source, Source::InStream(_))).count();
            if dd.parts.len() > 1 && text_parts > 0 && text_parts < dd.parts.len() && !req.text {
                out.push(at(format!("DD {} concatenates in-stream data with data sets of z/OS records", dd.name)));
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
    name: String,
    path: PathBuf,
    text: bool,
    sysout: bool,
}

struct Disposal {
    path: PathBuf,
    disp: jcl::Disp,
    created: bool,
    temporary: bool,
}

struct Runner<'a> {
    req: &'a Request,
    scratch: PathBuf,
    temporaries: BTreeMap<String, PathBuf>,
    /// Data sets this job created that are only passed so far: deleted when the job ends.
    passed_new: BTreeSet<PathBuf>,
    files: usize,
}

fn fresh_name(dir: &Path, n: &mut usize, what: &str) -> PathBuf {
    *n += 1;
    dir.join(format!("{n:04}-{what}"))
}

impl Runner<'_> {
    fn dataset_path(&self, source: &Source) -> Option<PathBuf> {
        match source {
            Source::Dataset { dsn, member } => Some(member.as_ref().map_or_else(|| self.req.datasets.join(dsn), |m| self.req.datasets.join(dsn).join(m))),
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
        let label = format!("{}.{}", step.name.as_deref().unwrap_or("STEP"), dd.name);
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
                source @ (Source::Dataset { .. } | Source::Temporary { .. }) => {
                    let path = self.dataset_path(source).expect("a data set has a path");
                    let (member, shown) = match source {
                        Source::Dataset { dsn, member } => (member.is_some(), member.as_ref().map_or(dsn.clone(), |m| format!("{dsn}({m})"))),
                        Source::Temporary { name, member } => (member.is_some(), format!("&&{name}")),
                        _ => unreachable!(),
                    };
                    let whole = if member { path.parent().map(Path::to_path_buf).unwrap_or_default() } else { path.clone() };
                    let temporary = matches!(source, Source::Temporary { .. });
                    let created = part.disp.status == Status::New;
                    if created {
                        if whole.exists() {
                            return Err(format!("DD {}: {shown} already exists, and DISP=NEW creates it", dd.name));
                        }
                        if member {
                            fs::create_dir_all(&whole).map_err(|e| format!("DD {}: {e}", dd.name))?;
                        } else {
                            fs::write(&path, b"").map_err(|e| format!("DD {}: {e}", dd.name))?;
                        }
                        if let Source::Temporary { name, .. } = source {
                            self.temporaries.insert(name.clone(), whole.clone());
                        }
                    } else if !whole.exists() {
                        return Err(format!("DD {}: {shown} was not found", dd.name));
                    } else if member != whole.is_dir() {
                        return Err(format!("DD {}: {shown} {}", dd.name, if member { "names a member of a data set that has none" } else { "is a partitioned data set; name a member" }));
                    }
                    disposals.push(Disposal { path: whole, disp: part.disp, created, temporary });
                    paths.push(path);
                }
            }
        }
        if paths.len() == 1 {
            return Ok(Allocated { name: dd.name.clone(), path: paths.remove(0), text, sysout });
        }
        let joined = fresh_name(&self.scratch, &mut self.files, &label);
        let mut bytes = Vec::new();
        for p in &paths {
            bytes.extend(fs::read(p).map_err(|e| format!("DD {}: {e}", dd.name))?);
        }
        fs::write(&joined, bytes).map_err(|e| format!("DD {}: {e}", dd.name))?;
        Ok(Allocated { name: dd.name.clone(), path: joined, text, sysout })
    }

    fn dispose(&mut self, disposals: Vec<Disposal>, abended: bool) {
        for d in disposals {
            match d.disp.at_end(abended) {
                End::Delete => {
                    let _ = if d.path.is_dir() { fs::remove_dir_all(&d.path) } else { fs::remove_file(&d.path) };
                    self.passed_new.remove(&d.path);
                }
                End::Pass if d.created && !d.temporary => {
                    self.passed_new.insert(d.path);
                }
                End::Pass => {}
                End::Keep | End::Catlg | End::Uncatlg => {
                    self.passed_new.remove(&d.path);
                }
            }
        }
    }
}

/// Runs a COBOL program with the step's DDs: its return code, or the abend's code and message.
fn run_cobol(path: &Path, req: &Request, dds: &[Allocated], database: Option<&mut (dyn exec::sql::Database + '_)>, out: &mut dyn Write) -> Result<i16, (AbendCode, String)> {
    let ironwork = |m: String| (AbendCode::Ironwork, m);
    let text = fs::read(path).map(|b| syntax::copy::decode(&b)).map_err(|e| ironwork(format!("{}: {e}", path.display())))?;
    let own = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let libraries = syntax::copy::Libraries::new(std::iter::once(own.clone()).chain(req.libraries.iter().cloned()).collect()).with_program(path);
    let mut programs = syntax::parse_all_with(&text, &libraries).map_err(|e| ironwork(e.place(&path.display().to_string()).to_string()))?;
    let first = programs.remove(0);
    let library = exec::unit::Library { programs, dirs: std::iter::once(own).chain(req.program_dirs.iter().cloned()).collect(), copy: libraries, flags: req.flags.clone() };
    let compiled = exec::compile(first, &req.flags).map_err(|errors| ironwork(syntax::most_severe(&errors).map(|e| e.place(&path.display().to_string()).to_string()).unwrap_or_default()))?;
    let specs: Vec<String> = dds.iter().map(|d| format!("{}={}{}", d.name, d.path.display(), if d.text { ":text" } else { "" })).collect();
    let dds = exec::files::Dds::new(&specs, false).map_err(ironwork)?;
    let sysin: Box<dyn std::io::BufRead> = match dds.get("SYSIN").and_then(|d| fs::File::open(d.path).ok()) {
        Some(f) => Box::new(std::io::BufReader::new(f)),
        None => Box::new(std::io::empty()),
    };
    let mut err = std::io::stderr();
    match compiled.execute_with(library, dds, Some(sysin), req.clock, database, out, &mut err) {
        Ok((_, rc)) => Ok(rc),
        Err(exec::Abend { code: AbendCode::Signal(Signal::ClosedOutput), .. }) => Ok(0),
        Err(a) => Err((a.code, a.message)),
    }
}

/// IEBGENER with no control statements: SYSUT1 copied to SYSUT2 as it stands. Without either
/// DD it ends with return code 12.
fn iebgener(dds: &[Allocated]) -> Result<i16, (AbendCode, String)> {
    let dd = |n: &str| dds.iter().find(|d| d.name == n);
    let (Some(from), Some(to)) = (dd("SYSUT1"), dd("SYSUT2")) else { return Ok(12) };
    let bytes = fs::read(&from.path).map_err(|e| (AbendCode::Ironwork, format!("SYSUT1: {e}")))?;
    fs::write(&to.path, bytes).map_err(|e| (AbendCode::Ironwork, format!("SYSUT2: {e}")))?;
    Ok(0)
}

struct Frame {
    active: bool,
    parent: bool,
    value: bool,
    abend_aware: bool,
}

fn scratch_dir() -> std::io::Result<PathBuf> {
    let nonce = exec::digest::hex(&exec::digest::sha256(format!("{:?}{}", std::time::SystemTime::now(), std::process::id()).as_bytes()))[..12].to_string();
    let dir = std::env::temp_dir().join(format!("ironwork-job-{nonce}"));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn run(req: Request) -> ExitCode {
    let shown = req.jcl.display().to_string();
    let text = match fs::read_to_string(&req.jcl) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("ironwork: {shown}: {e}");
            return ExitCode::from(2);
        }
    };
    let job = match jcl::parse(&text) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("ironwork: {shown}:{}: {}", e.line, e.message);
            return ExitCode::from(2);
        }
    };
    if !req.datasets.is_dir() {
        eprintln!("ironwork: --datasets {}: not a directory", req.datasets.display());
        return ExitCode::from(2);
    }
    let refused = refusals(&job, &req);
    if !refused.is_empty() {
        for r in refused {
            eprintln!("ironwork: {shown}: {r}");
        }
        return ExitCode::from(2);
    }
    let mut replay = match &req.replay {
        Some(file) => match fs::read_to_string(file).map_err(|e| e.to_string()).and_then(|t| exec::sql::Replay::parse(&t, false)) {
            Ok(r) => Some(r),
            Err(e) => {
                eprintln!("ironwork: --sql-replay {}: {e}", file.display());
                return ExitCode::from(2);
            }
        },
        None => None,
    };
    let scratch = match scratch_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("ironwork: a scratch directory: {e}");
            return ExitCode::from(2);
        }
    };
    let mut runner = Runner { req: &req, scratch, temporaries: BTreeMap::new(), passed_new: BTreeSet::new(), files: 0 };
    let status = run_job(&job, &mut runner, replay.as_mut().map(|r| r as &mut dyn exec::sql::Database));
    for path in std::mem::take(&mut runner.passed_new) {
        let _ = if path.is_dir() { fs::remove_dir_all(&path) } else { fs::remove_file(&path) };
    }
    let _ = fs::remove_dir_all(&runner.scratch);
    ExitCode::from(status)
}

/// Runs every step the job's conditions allow; the exit status is the highest return code, or
/// 16 when a step abended or the job ended on a JCL error.
fn run_job(job: &Job, runner: &mut Runner<'_>, mut database: Option<&mut dyn exec::sql::Database>) -> u8 {
    let (mut ran, mut abended, mut failed) = (Vec::<Ran>::new(), false, false);
    let mut frames: Vec<Frame> = Vec::new();
    let mut stdout = std::io::stdout().lock();
    let log = |name: &str, pgm: &str, what: String| eprintln!("ironwork job {}: {name} PGM={pgm} {what}", job.name);
    let mut ended = false;
    for item in &job.items {
        match item {
            Item::If { expr, .. } => {
                let parent = frames.last().is_none_or(|f| f.active);
                let value = cond::eval(expr, &ran);
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
                let name = step.name.as_deref().unwrap_or("");
                if ended {
                    log(name, &step.pgm, "BYPASSED: the job ended".into());
                    continue;
                }
                if !frames.iter().all(|f| f.active) {
                    log(name, &step.pgm, "BYPASSED: its IF branch is not taken".into());
                    continue;
                }
                if let Some(reason) = cond::bypassed_by_cond(&job.cond, &ran, false) {
                    log(name, &step.pgm, format!("BYPASSED: the JOB statement's {reason}; the job ends"));
                    ended = true;
                    continue;
                }
                let abend_tested = frames.iter().any(|f| f.abend_aware);
                if let Some(reason) = cond::bypassed_by_cond(&step.cond, &ran, abended && !abend_tested) {
                    log(name, &step.pgm, format!("BYPASSED: {reason}"));
                    continue;
                }
                let mut disposals = Vec::new();
                let mut dds = Vec::new();
                let mut jcl_error = None;
                for dd in &step.dds {
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
                    failed = true;
                    ended = true;
                    continue;
                }
                let outcome = match program_of(&step.pgm, &runner.req.program_dirs) {
                    Program::Iefbr14 => Ok(0),
                    Program::Iebgener => iebgener(&dds),
                    Program::Cobol(path) => run_cobol(&path, runner.req, &dds, database.as_deref_mut(), &mut stdout),
                    Program::Missing => Err((AbendCode::ModuleNotFound, format!("program {} is not in the program libraries", step.pgm))),
                };
                for d in dds.iter().filter(|d| d.sysout) {
                    if let Ok(bytes) = fs::read(&d.path) {
                        let _ = stdout.write_all(&bytes);
                    }
                }
                let _ = stdout.flush();
                match outcome {
                    Ok(rc) => {
                        let rc = rc.clamp(0, 4095) as u16;
                        log(name, &step.pgm, format!("RC={rc:04}"));
                        runner.dispose(disposals, false);
                        ran.push(Ran { name: step.name.clone(), rc: Some(rc), abend: None });
                    }
                    Err((code, message)) => {
                        log(name, &step.pgm, format!("ABEND {code}: {message}"));
                        runner.dispose(disposals, true);
                        abended = true;
                        ran.push(Ran { name: step.name.clone(), rc: None, abend: Some(code.to_string()) });
                    }
                }
            }
        }
    }
    if abended || failed {
        return 16;
    }
    ran.iter().filter_map(|r| r.rc).max().map_or(0, |rc| rc.min(255) as u8)
}
