//! `ironwork fuzz --job`: runs a job many times through `ironwork job` on generated data sets,
//! in-stream data and step PARMs, and keeps each abend one of its COBOL steps gives, as `ironwork
//! fuzz` does for one program (docs/evidence.md §5).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use jcl::{Item, Source, Status};

use super::{Feed, Inputs, Outcome, Varied};

pub struct Request {
    /// The job's JCL is `fuzz.program`.
    pub fuzz: super::Request,
    /// Data sets every run starts with, copied into the run's own data set directory.
    pub datasets: Option<PathBuf>,
    pub proclibs: Vec<PathBuf>,
    pub user: Option<String>,
}

/// A data set's file under the job's data set directory: A.B, or A.B/M for member M.
fn path_of(dsn: &str, member: Option<&str>) -> PathBuf {
    match member {
        Some(m) => Path::new(dsn).join(m),
        None => PathBuf::from(dsn),
    }
}

/// What a job's runs vary and give: the data sets its COBOL steps read, fed by data set name with
/// each one's file, the data sets it reads that are neither fed nor among the given ones, which get
/// an empty file, each in-stream DD a COBOL step reads (STEP.DD), and each step whose program
/// takes a PARM.
struct Plan {
    varied: Varied,
    files: BTreeMap<String, PathBuf>,
    empty: BTreeSet<PathBuf>,
}

/// A data set a step creates (NEW, or MOD that may) is the job's own output from then on, never an
/// input; one it reads first is fed from the first COBOL step that reads it.
fn plan(job: &jcl::Job, req: &Request) -> Plan {
    let (mut created, mut files, mut empty) = (BTreeSet::new(), BTreeMap::new(), BTreeSet::new());
    let (mut feeds, mut lines, mut parms) = (Vec::new(), Vec::new(), Vec::new());
    for item in &job.items {
        let Item::Step(step) = item else { continue };
        let compiled = match crate::job::program_of(&step.pgm, &req.fuzz.program_dirs) {
            crate::job::Program::Cobol(path) => super::compile(&super::Request { program: path, ..req.fuzz.clone() }).ok(),
            _ => None,
        };
        let (own, sysin) = compiled.as_ref().map(|(c, rest)| super::inputs_of(c, rest)).map_or((Vec::new(), false), |(f, s, _)| (f, s));
        let reads = |dd: &str| own.iter().find(|f| f.dd == dd);
        for dd in &step.dds {
            let [part] = dd.parts.as_slice() else { continue };
            match &part.source {
                Source::Dataset { dsn, member } => {
                    let name = member.as_ref().map_or_else(|| dsn.clone(), |m| format!("{dsn}({m})"));
                    if matches!(part.disp.status, Status::New | Status::Mod) {
                        created.insert(name);
                        continue;
                    }
                    if created.contains(&name) || files.contains_key(&name) {
                        continue;
                    }
                    let path = path_of(dsn, member.as_deref());
                    match reads(&dd.name) {
                        Some(f) => {
                            feeds.push(Feed { dd: name.clone(), ..f.clone() });
                            empty.remove(&path);
                            files.insert(name, path);
                        }
                        None if !req.datasets.as_ref().is_some_and(|d| d.join(&path).exists()) => {
                            empty.insert(path);
                        }
                        None => {}
                    }
                }
                Source::InStream(_) if (dd.name == "SYSIN" && sysin) || reads(&dd.name).is_some() => lines.push(format!("{}.{}", step.shown(), dd.name)),
                _ => {}
            }
        }
        if compiled.as_ref().is_some_and(|(c, _)| super::takes_parm(c)) {
            parms.push(step.shown());
        }
    }
    Plan { varied: Varied { feeds, lines, parms }, files, empty }
}

/// One run of the job as its own process, in a data set directory of its own.
struct Runner<'a> {
    req: &'a Request,
    plan: &'a Plan,
    work: PathBuf,
    count: u64,
}

impl Runner<'_> {
    fn run(&mut self, inputs: &Inputs, evidence: Option<(&Path, &Path)>) -> std::io::Result<Outcome> {
        self.count += 1;
        let dir = self.work.join(format!("run-{}", self.count));
        let datasets = dir.join("datasets");
        fs::create_dir_all(&datasets)?;
        if let Some(base) = &self.req.datasets {
            crate::job::copy_tree(base, &datasets)?;
        }
        let write = |path: &Path, bytes: &[u8]| -> std::io::Result<()> {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, bytes)
        };
        for path in &self.plan.empty {
            write(&datasets.join(path), b"")?;
        }
        let mut given: Vec<(&str, PathBuf)> = Vec::new();
        for (name, records) in &inputs.files {
            let path = datasets.join(&self.plan.files[name]);
            let rdw = self.plan.varied.feeds.iter().any(|f| f.dd == *name && f.variable.is_some());
            write(&path, &super::data_set(records, rdw))?;
            given.push((name.as_str(), path));
        }
        let fuzz = &self.req.fuzz;
        let mut command = Command::new(std::env::current_exe()?);
        command.arg("job").arg(&fuzz.program).arg("--datasets").arg(&datasets).arg("--clock").arg(&fuzz.clock);
        command.args(&fuzz.flags);
        for (flag, dirs) in [("-I", &fuzz.libraries), ("-L", &fuzz.program_dirs), ("--proclib", &self.req.proclibs)] {
            for d in dirs {
                command.arg(flag).arg(d);
            }
        }
        if let Some(user) = &self.req.user {
            command.arg("--user").arg(user);
        }
        for (k, (key, lines)) in inputs.lines.iter().enumerate() {
            let path = dir.join(format!("instream{k}"));
            fs::write(&path, super::sysin_text(lines))?;
            command.arg("--instream").arg(format!("{key}={}", path.display()));
        }
        for (step, text) in &inputs.parms {
            command.arg("--step-parm").arg(format!("{step}={}", String::from_utf8_lossy(text)));
        }
        if let Some(limit) = inputs.limit {
            command.arg("--statement-limit").arg(limit.to_string());
        }
        if let Some((journal, coverage)) = evidence {
            command.arg("--evidence").arg(journal).arg("--coverage").arg(coverage);
            if let Some(marker) = &inputs.marker {
                command.arg("--trace-marker").arg(marker);
            }
        }
        let roots = roots(self.req, &datasets);
        let timeout = if inputs.limit.is_some() { fuzz.timeout * super::HANG_PATIENCE } else { fuzz.timeout };
        let Some((code, text)) = super::wait_for(command, &dir, timeout, &given)? else { return Ok(Outcome::Timeout) };
        Ok(match super::waited(super::ended(code, &text, |l| super::abend_line(l, &roots)), &text) {
            Outcome::Clean => unplaced(&text).unwrap_or(Outcome::Clean),
            outcome => outcome,
        })
    }
}

/// A step that abended with no COBOL statement to place it at, or a JCL error, from the job log:
/// what the job's surroundings did, never a finding.
fn unplaced(text: &str) -> Option<Outcome> {
    text.lines().find(|l| l.starts_with("ironwork job ") && (l.contains(" ABEND ") || l.contains(" JCL ERROR: "))).map(|l| Outcome::Refused(l.to_string()))
}

/// The directories a job run reads, in the order its journal's `input` records number them: the
/// JCL's, the data sets', then each `-I`, `-L` and procedure library.
fn roots(req: &Request, datasets: &Path) -> Vec<PathBuf> {
    let own = req.fuzz.program.parent().map(Path::to_path_buf).unwrap_or_default();
    std::iter::once(own).chain([datasets.to_path_buf()]).chain(req.fuzz.libraries.iter().cloned()).chain(req.fuzz.program_dirs.iter().cloned()).chain(req.proclibs.iter().cloned()).collect()
}

pub fn run(req: Request) -> ExitCode {
    let fail = |message: String| {
        eprintln!("ironwork fuzz: {message}");
        ExitCode::from(2)
    };
    let jcl = &req.fuzz.program;
    let job = match crate::job::parse(jcl, &req.datasets.clone().unwrap_or_default(), &req.proclibs, req.user.as_deref()) {
        Ok(j) => j,
        Err(e) => return fail(e),
    };
    let Some(file) = super::from_root(jcl, &req.fuzz.root) else {
        return fail(format!("{} is not under --root {}", jcl.display(), req.fuzz.root.display()));
    };
    let plan = plan(&job, &req);
    let varied = &plan.varied;
    if varied.feeds.is_empty() && varied.lines.is_empty() && varied.parms.is_empty() {
        return fail(format!("job {} has no COBOL step that reads a data set fuzz can build, in-stream data or a PARM, so there is nothing to vary", job.name));
    }
    let read: Vec<PathBuf> = roots(&req, Path::new("")).into_iter().enumerate().filter(|&(k, _)| k != 1).map(|(_, r)| r).collect();
    let work = match super::prepare(&req.fuzz.out, &read) {
        Ok(w) => w,
        Err(e) => return fail(e),
    };
    let mut runner = Runner { req: &req, plan: &plan, work: work.clone(), count: 0 };
    let found = super::drive(&req.fuzz.out, req.fuzz.runs, req.fuzz.seed, req.fuzz.hang_limit, varied, &mut |inputs, evidence| runner.run(inputs, evidence));
    let _ = fs::remove_dir_all(&work);
    let found = match found {
        Ok(f) => f,
        Err(e) => return fail(e),
    };
    // Each run's data sets are its own, under .work and gone when fuzz ends; the manifest names
    // .work for them.
    let header = super::Header { seed: req.fuzz.seed, clock: &req.fuzz.clock, file: &file, id: &job.name, root: &req.fuzz.root, roots: &roots(&req, &work), entry: "job" };
    let kept = found.runs.len();
    if let Err(e) = super::write_manifest(&req.fuzz.out, &header, found.inputs, &found.tally, found.runs) {
        return fail(format!("-o {}: {e}", req.fuzz.out.display()));
    }
    if !plan.empty.is_empty() {
        eprintln!("ironwork fuzz: not varied, given empty: {}", plan.empty.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", "));
    }
    found.tally.report();
    if let Some((code, file, line)) = found.baseline {
        eprintln!("ironwork fuzz: the job ends with {code} at {file}:{line} on empty input; that abend is not kept");
    }
    println!("{}", found.tally.summary(kept, &req.fuzz.out));
    ExitCode::SUCCESS
}
