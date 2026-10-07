//! `ironwork fuzz --differential`: runs a batch program on each generated input twice, through
//! `ironwork run` and `ironwork run --vm` under one statement limit, and keeps each input on which
//! the interpreter and the VM differ (docs/codegen-runtime.md B2, docs/lir.md §12.3).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

use super::{Inputs, Others, Request, Rng, Varied};

/// How many differing inputs a fuzz run keeps, one to each way of differing.
const KEPT: usize = 20;
/// The run pairs spent making each kept input smaller.
const BUDGET: u32 = 100;
/// `ironwork run`'s exit statuses for an abend, and for a program lowering refused or a run the VM
/// stopped, whose standard error says which (README, Exit status).
const ABEND: [i32; 2] = [240, 244];
const VM_STOPPED: [i32; 2] = [242, 243];

/// One executor's run: whether it was stopped at the timeout; its exit status, None when a signal
/// ended it; standard output; standard error; and each DD's data set as the run left it, None
/// where there was none.
#[derive(PartialEq, Eq)]
struct Ran {
    timed_out: bool,
    status: Option<i32>,
    out: Vec<u8>,
    err: String,
    files: BTreeMap<String, Option<Vec<u8>>>,
}

impl Ran {
    fn at_limit(&self) -> bool {
        self.timed_out || self.err.lines().any(|l| l.split_once(": ABEND ").is_some_and(|(_, rest)| rest.starts_with("S322:")))
    }

    /// The VM's account of what stopped it: what it does not run yet, or the lowering it refused.
    fn unimplemented(&self) -> Option<String> {
        if !self.status.is_some_and(|s| VM_STOPPED.contains(&s)) {
            return None;
        }
        self.err.lines().find_map(|l| {
            l.split_once("the VM does not run ").and_then(|(_, rest)| rest.strip_suffix(" yet; run it with --interpret")).map(str::to_string).or_else(|| l.split_once(": lowering: ").map(|(_, why)| format!("lowering: {why}")))
        })
    }

    fn ending(&self) -> String {
        let abend = self.err.lines().rev().find_map(|l| l.split_once(": ABEND ").map(|(place, rest)| (place, rest.split(':').next().unwrap_or(rest))));
        match (self.timed_out, self.status, abend) {
            (true, _, _) => "timed out".into(),
            (_, None, _) => "ended by a signal".into(),
            (_, Some(status), Some((place, code))) if ABEND.contains(&status) => format!("{code} at {}", place.rsplit('/').next().unwrap_or(place)),
            (_, Some(status), _) => format!("exit status {status}"),
        }
    }
}

enum Verdict {
    Agree,
    /// One ran to the timeout and the other to the timeout or the statement limit, so nothing is
    /// compared. Two runs ended at the statement limit stopped at the same statement and are compared.
    TimedOut,
    Unimplemented(String),
    /// What differs, the first of each kind.
    Differ(Vec<String>),
}

fn first_line_differing(what: &str, a: &[u8], b: &[u8]) -> Option<String> {
    if a == b {
        return None;
    }
    let (x, y): (Vec<&[u8]>, Vec<&[u8]>) = (a.split(|&c| c == b'\n').collect(), b.split(|&c| c == b'\n').collect());
    let n = x.iter().zip(&y).position(|(p, q)| p != q).unwrap_or(x.len().min(y.len()));
    let shown = |lines: &[&[u8]]| lines.get(n).map_or("(none)".to_string(), |l| format!("{:?}", String::from_utf8_lossy(l)));
    Some(format!("{what} differs at line {}: interpreter {}, VM {}", n + 1, shown(&x), shown(&y)))
}

fn verdict(interpreter: &Ran, vm: &Ran) -> Verdict {
    if (interpreter.timed_out || vm.timed_out) && interpreter.at_limit() && vm.at_limit() {
        return Verdict::TimedOut;
    }
    if interpreter == vm {
        return Verdict::Agree;
    }
    if let Some(what) = vm.unimplemented() {
        return Verdict::Unimplemented(what);
    }
    let mut found = Vec::new();
    if interpreter.ending() != vm.ending() {
        found.push(format!("the ending differs: interpreter {}, VM {}", interpreter.ending(), vm.ending()));
    }
    found.extend(first_line_differing("standard output", &interpreter.out, &vm.out));
    found.extend(first_line_differing("standard error", interpreter.err.as_bytes(), vm.err.as_bytes()));
    for (dd, a) in &interpreter.files {
        let b = vm.files.get(dd).cloned().flatten();
        if *a != b {
            let size = |f: &Option<Vec<u8>>| f.as_ref().map_or("none".to_string(), |f| format!("{} bytes", f.len()));
            found.push(format!("DD {dd} differs: interpreter {}, VM {}", size(a), size(&b)));
        }
    }
    Verdict::Differ(found)
}

/// Runs inputs on either executor, each run in the same directory, made afresh.
struct Runner<'a> {
    req: &'a Request,
    others: &'a Others,
    rdw: Vec<String>,
    work: PathBuf,
}

impl Runner<'_> {
    /// The data sets and SYSIN a run is given, by DD, and the bytes each starts with.
    fn given(&self, inputs: &Inputs) -> Vec<(String, Option<Vec<u8>>)> {
        let mut given: Vec<(String, Option<Vec<u8>>)> = inputs.files.iter().map(|(dd, records)| (dd.clone(), Some(super::data_set(records, self.rdw.contains(dd))))).collect();
        given.extend(self.others.unfed.iter().map(|dd| (dd.clone(), Some(Vec::new()))));
        given.extend(self.others.written.iter().map(|dd| (dd.clone(), None)));
        if let Some(lines) = inputs.lines.get("SYSIN") {
            given.push(("SYSIN".into(), Some(super::sysin_text(lines))));
        }
        given
    }

    /// The command that runs the program on `inputs` with each DD's data set in `dir`.
    fn command(&self, inputs: &Inputs, dir: &Path, given: &[(String, Option<Vec<u8>>)], vm: bool) -> std::io::Result<Command> {
        let mut command = Command::new(std::env::current_exe()?);
        command.arg("run").arg(&self.req.program).arg("--clock").arg(&self.req.clock).args(&self.req.flags);
        // Each run compiles the program afresh, so WHEN-COMPILED is fixed at the clock's time
        // unless the environment already fixes it.
        if std::env::var_os("SOURCE_DATE_EPOCH").is_none()
            && let Some(exec::unit::Clock::Fixed(seconds, _)) = crate::parse_clock(&self.req.clock)
            && seconds >= 0
        {
            command.env("SOURCE_DATE_EPOCH", seconds.to_string());
        }
        for d in &self.req.libraries {
            command.arg("-I").arg(d);
        }
        for d in &self.req.program_dirs {
            command.arg("-L").arg(d);
        }
        for (k, (dd, _)) in given.iter().enumerate() {
            command.arg("--dd").arg(format!("{dd}={}", dir.join(format!("dd{k}")).display()));
        }
        if let Some(parm) = inputs.parms.get("PARM") {
            command.arg("--parm").arg(String::from_utf8_lossy(parm).as_ref());
        }
        command.arg("--statement-limit").arg(self.req.hang_limit.to_string());
        command.arg(if vm { "--vm" } else { "--interpret" });
        Ok(command)
    }

    fn run(&self, inputs: &Inputs, vm: bool) -> std::io::Result<Ran> {
        let dir = self.work.join("run");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir)?;
        let given = self.given(inputs);
        for (k, (_, bytes)) in given.iter().enumerate() {
            if let Some(bytes) = bytes {
                fs::write(dir.join(format!("dd{k}")), bytes)?;
            }
        }
        let (stdout, stderr) = (self.work.join("stdout"), self.work.join("stderr"));
        let mut command = self.command(inputs, &dir, &given, vm)?;
        command.stdin(Stdio::null()).stdout(fs::File::create(&stdout)?).stderr(fs::File::create(&stderr)?);
        let mut child = command.spawn()?;
        let began = Instant::now();
        let (timed_out, status) = loop {
            if let Some(status) = child.try_wait()? {
                break (false, status.code());
            }
            if began.elapsed() > self.req.timeout {
                let _ = child.kill();
                let _ = child.wait();
                break (true, None);
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        let files = given.iter().enumerate().map(|(k, (dd, _))| (dd.clone(), fs::read(dir.join(format!("dd{k}"))).ok())).collect();
        let mut err = without_thread_numbers(&String::from_utf8_lossy(&fs::read(&stderr)?));
        for (k, (dd, _)) in given.iter().enumerate() {
            err = err.replace(&dir.join(format!("dd{k}")).display().to_string(), dd);
        }
        Ok(Ran { timed_out, status, out: fs::read(&stdout)?, err, files })
    }

    fn both(&self, inputs: &Inputs) -> std::io::Result<(Ran, Ran)> {
        Ok((self.run(inputs, false)?, self.run(inputs, true)?))
    }
}

/// `text` with the number Rust gives a panicking thread taken out, as it differs from run to run.
fn without_thread_numbers(text: &str) -> String {
    text.split_inclusive('\n')
        .map(|line| {
            let named = line.strip_prefix("thread '").and_then(|rest| rest.split_once("' (")).and_then(|(name, rest)| rest.split_once(") panicked at ").map(|(_, at)| (name, at)));
            named.map_or_else(|| line.to_string(), |(name, at)| format!("thread '{name}' panicked at {at}"))
        })
        .collect()
}

/// How a pair of runs differs, as a key that keeps one input to each way: both endings and what
/// differs, without the detail.
fn signature(interpreter: &Ran, vm: &Ran, found: &[String]) -> String {
    let kinds: Vec<&str> = found.iter().map(|f| f.split(" differs").next().unwrap_or(f)).collect();
    format!("{} / {}: {}", interpreter.ending(), vm.ending(), kinds.join(", "))
}

/// `inputs` with records, lines and PARM text dropped while the runs still differ in the same way.
fn smaller(runner: &Runner, mut inputs: Inputs, key: &str) -> Inputs {
    let mut left = BUDGET;
    let mut holds = |candidate: &Inputs| -> bool {
        if left == 0 {
            return false;
        }
        left -= 1;
        match runner.both(candidate) {
            Ok((a, b)) => matches!(verdict(&a, &b), Verdict::Differ(found) if signature(&a, &b, &found) == key),
            Err(_) => false,
        }
    };
    for dd in inputs.files.keys().cloned().collect::<Vec<_>>() {
        for k in (0..inputs.files[&dd].len()).rev() {
            let mut candidate = inputs.clone();
            candidate.files.get_mut(&dd).expect("fed").remove(k);
            if holds(&candidate) {
                inputs = candidate;
            }
        }
    }
    for name in inputs.lines.keys().cloned().collect::<Vec<_>>() {
        for k in (0..inputs.lines[&name].len()).rev() {
            let mut candidate = inputs.clone();
            candidate.lines.get_mut(&name).expect("given").remove(k);
            if holds(&candidate) {
                inputs = candidate;
            }
        }
    }
    for name in inputs.parms.keys().cloned().collect::<Vec<_>>() {
        let parm = inputs.parms[&name].clone();
        for keep in [0, parm.len() / 2].into_iter().filter(|&k| k < parm.len()) {
            let mut candidate = inputs.clone();
            candidate.parms.insert(name.clone(), parm[..keep].to_vec());
            if holds(&candidate) {
                inputs = candidate;
                break;
            }
        }
    }
    inputs
}

/// The file a kept data set is written to: its DD name where that is a plain DD name, else its
/// place among the run's data sets, as an ASSIGN literal may name a path anywhere. The other
/// files kept beside them have a period in their names, which no DD name has.
fn kept_name(k: usize, dd: &str) -> String {
    if jcl::is_name(dd) { dd.to_string() } else { format!("dd.{k}") }
}

/// Writes a kept input to `dir`: each data set the run is given under `input/`, the PARM as
/// `input/parm.txt`, what each executor wrote and left under `interpreter/` and `vm/`, and
/// `report.txt`.
fn keep(dir: &Path, runner: &Runner, inputs: &Inputs, interpreter: &Ran, vm: &Ran, found: &[String]) -> std::io::Result<()> {
    let given = runner.given(inputs);
    fs::create_dir_all(dir.join("input"))?;
    for (k, (dd, bytes)) in given.iter().enumerate() {
        if let Some(bytes) = bytes {
            fs::write(dir.join("input").join(kept_name(k, dd)), bytes)?;
        }
    }
    if let Some(parm) = inputs.parms.get("PARM") {
        fs::write(dir.join("input").join("parm.txt"), parm)?;
    }
    for (name, ran) in [("interpreter", interpreter), ("vm", vm)] {
        let to = dir.join(name);
        fs::create_dir_all(&to)?;
        fs::write(to.join("stdout.txt"), &ran.out)?;
        fs::write(to.join("stderr.txt"), &ran.err)?;
        for (k, (dd, _)) in given.iter().enumerate() {
            if let Some(Some(bytes)) = ran.files.get(dd) {
                fs::write(to.join(kept_name(k, dd)), bytes)?;
            }
        }
    }
    let mut report = format!("interpreter: {}\nvm: {}\n", interpreter.ending(), vm.ending());
    for f in found {
        report.push_str(f);
        report.push('\n');
    }
    let input = super::resolved(&dir.join("input"));
    let command = runner.command(inputs, &input, &given, true)?;
    let mut args: Vec<String> = command.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
    for (k, (dd, _)) in given.iter().enumerate() {
        let at = format!("{dd}={}", input.join(format!("dd{k}")).display());
        if let Some(arg) = args.iter_mut().find(|a| **a == at) {
            *arg = format!("{dd}={}", input.join(kept_name(k, dd)).display());
        }
    }
    let envs: Vec<String> = command.get_envs().filter_map(|(k, v)| v.map(|v| format!("{}={} ", k.to_string_lossy(), v.to_string_lossy()))).collect();
    report.push_str(&format!("\nrun it again where fuzz ran, with --interpret and with --vm, on a fresh copy of input/ each time:\n  {}ironwork {}\n", envs.concat(), args.join(" ")));
    fs::write(dir.join("report.txt"), report)
}

pub fn run(req: Request) -> ExitCode {
    let fail = |message: String| {
        eprintln!("ironwork fuzz: {message}");
        ExitCode::from(2)
    };
    let (compiled, rest) = match super::compile(&req) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(12);
        }
    };
    if let Err(e) = exec::vm::lowered(&compiled) {
        eprintln!("{}", syntax::Error::from(e).place(&req.program.display().to_string()));
        return ExitCode::from(12);
    }
    let parm = super::takes_parm(&compiled);
    if !compiled.program.using.is_empty() && !parm {
        return fail(format!("{} takes PROCEDURE DIVISION USING parameters that are not a PARM's halfword length and text: fuzz runs a main program", compiled.program.id));
    }
    let (feeds, sysin, others) = super::inputs_of(&compiled, &rest);
    if feeds.is_empty() && !sysin && !parm {
        return fail(format!("{} reads no sequential, indexed or relative file on a DD of its own, no SYSIN and no PARM, so there is nothing to vary", compiled.program.id));
    }
    if req.out.exists() && fs::read_dir(&req.out).map(|mut d| d.next().is_some()).unwrap_or(true) {
        return fail(format!("-o {} is not empty: each fuzz run gets a directory of its own", req.out.display()));
    }
    let work = req.out.join(".work");
    if let Err(e) = fs::create_dir_all(&work) {
        return fail(format!("-o {}: {e}", req.out.display()));
    }
    let rdw = feeds.iter().filter(|f| f.variable.is_some()).map(|f| f.dd.clone()).collect();
    let runner = Runner { req: &req, others: &others, rdw, work: work.clone() };
    let varied = Varied { feeds, lines: sysin.then(|| "SYSIN".to_string()).into_iter().collect(), parms: parm.then(|| "PARM".to_string()).into_iter().collect() };
    let found = drive(&req, &runner, &varied);
    let _ = fs::remove_dir_all(&work);
    match found {
        Ok(differ) if differ => ExitCode::from(1),
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => fail(format!("a run could not start: {e}")),
    }
}

/// Runs `req.runs` generated inputs on both executors and keeps the differing ones; whether any
/// differed.
fn drive(req: &Request, runner: &Runner, varied: &Varied) -> std::io::Result<bool> {
    let mut rng = Rng(req.seed.max(1));
    let (mut agree, mut at_limit, mut timed_out, mut differ) = (0, 0, 0, 0);
    let mut unimplemented: BTreeMap<String, usize> = BTreeMap::new();
    let mut kept: Vec<(String, Inputs)> = Vec::new();
    for _ in 0..req.runs {
        let inputs = super::generate(&mut rng, varied);
        let (interpreter, vm) = runner.both(&inputs)?;
        match verdict(&interpreter, &vm) {
            Verdict::Agree => {
                agree += 1;
                at_limit += usize::from(interpreter.at_limit());
            }
            Verdict::TimedOut => timed_out += 1,
            Verdict::Unimplemented(what) => *unimplemented.entry(what).or_default() += 1,
            Verdict::Differ(found) => {
                differ += 1;
                let key = signature(&interpreter, &vm, &found);
                if kept.len() < KEPT && !kept.iter().any(|(k, _)| *k == key) {
                    kept.push((key, inputs));
                }
            }
        }
    }
    for (n, (key, inputs)) in kept.iter().enumerate() {
        let small = smaller(runner, inputs.clone(), key);
        let (interpreter, vm) = runner.both(&small)?;
        let Verdict::Differ(found) = verdict(&interpreter, &vm) else {
            eprintln!("ironwork fuzz: {key}: not kept, as its input ran alike when run again");
            continue;
        };
        let dir = req.out.join(format!("divergence-{n}"));
        keep(&dir, runner, &small, &interpreter, &vm, &found)?;
        eprintln!("ironwork fuzz: {}: {}", dir.display(), found.first().map_or(key.as_str(), String::as_str));
    }
    for (what, n) in &unimplemented {
        eprintln!("ironwork fuzz: {n} runs reached what the VM does not run yet: {what}");
    }
    let stopped: usize = unimplemented.values().sum();
    println!(
        "ironwork fuzz --differential: {} runs, {agree} agree ({at_limit} at the statement limit), {timed_out} timed out, {stopped} stopped by the VM, {differ} differ ({} kept): {}",
        req.runs,
        kept.len(),
        req.out.display()
    );
    Ok(differ > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ran(status: i32, out: &str, err: &str) -> Ran {
        Ran { timed_out: false, status: Some(status), out: out.as_bytes().to_vec(), err: err.into(), files: BTreeMap::new() }
    }

    #[test]
    fn runs_that_differ_say_where_and_are_grouped_by_their_endings_and_what_differs() {
        let interpreter = ran(0, "TOTAL 1\nEND\n", "");
        let vm = ran(0, "TOTAL 2\nEND\n", "");
        let Verdict::Differ(found) = verdict(&interpreter, &vm) else { panic!("they differ") };
        assert_eq!(found, ["standard output differs at line 1: interpreter \"TOTAL 1\", VM \"TOTAL 2\""]);
        assert_eq!(signature(&interpreter, &vm, &found), "exit status 0 / exit status 0: standard output");

        let abend = Ran { files: [("OUTFILE".to_string(), Some(b"AB".to_vec()))].into(), ..ran(240, "", "P.cbl:7:12: ABEND S0C7: data exception\n") };
        let written = Ran { files: [("OUTFILE".to_string(), Some(b"ABC".to_vec()))].into(), ..ran(0, "", "") };
        let Verdict::Differ(found) = verdict(&written, &abend) else { panic!("they differ") };
        assert_eq!(found[0], "the ending differs: interpreter exit status 0, VM S0C7 at P.cbl:7:12");
        assert_eq!(Ran { status: Some(240), ..ran(0, "", "P.cbl:7:12: ABEND S0C7: data exception\n") }.ending(), "S0C7 at P.cbl:7:12");
        assert_eq!(found.last().map(String::as_str), Some("DD OUTFILE differs: interpreter 3 bytes, VM 2 bytes"));
    }

    #[test]
    fn a_timeout_is_not_compared_the_statement_limit_is_and_the_vm_stopping_is_counted_not_failed() {
        let limited = ran(240, "A\n", "P.cbl:9:12: ABEND S322: the run reached its statement limit\n");
        let timed_out = Ran { timed_out: true, status: None, ..ran(0, "", "") };
        assert!(matches!(verdict(&limited, &timed_out), Verdict::TimedOut));
        assert!(matches!(verdict(&limited, &limited), Verdict::Agree));
        let elsewhere = ran(240, "A\n", "P.cbl:12:12: ABEND S322: the run reached its statement limit\n");
        let Verdict::Differ(found) = verdict(&limited, &elsewhere) else { panic!("they stopped at different statements") };
        assert_eq!(found[0], "the ending differs: interpreter S322 at P.cbl:9:12, VM S322 at P.cbl:12:12");
        let stopped = ran(243, "", "ironwork: P.cbl: the VM does not run FUNCTION UUID4, which gives another value on every run yet; run it with --interpret\n");
        assert!(matches!(verdict(&ran(0, "X\n", ""), &stopped), Verdict::Unimplemented(what) if what == "FUNCTION UUID4, which gives another value on every run"));
        let refused = ran(242, "", "P.cbl:4:12: lowering: INITIALIZE with FILLER is not lowered yet\n");
        assert!(matches!(verdict(&ran(0, "", ""), &refused), Verdict::Unimplemented(what) if what == "lowering: INITIALIZE with FILLER is not lowered yet"));
        assert!(matches!(verdict(&ran(4, "X\n", ""), &ran(4, "X\n", "")), Verdict::Agree));
    }

    #[test]
    fn a_panic_is_compared_without_its_thread_number() {
        let text = "A\nthread '<unnamed>' (18566553) panicked at crates/rt/src/unit.rs:217:17:\nrange end\n";
        assert_eq!(without_thread_numbers(text), "A\nthread '<unnamed>' panicked at crates/rt/src/unit.rs:217:17:\nrange end\n");
    }

    #[test]
    fn a_kept_input_holds_each_data_set_by_its_dd_or_place_what_each_executor_wrote_and_the_command_that_repeats_it() {
        let dir = std::env::temp_dir().join(format!("iw-differential-keep-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let req = Request {
            program: PathBuf::from("/src/P.cbl"),
            out: dir.clone(),
            root: PathBuf::from("/src"),
            runs: 1,
            seed: 1,
            timeout: Duration::from_secs(1),
            hang_limit: 500,
            libraries: Vec::new(),
            program_dirs: Vec::new(),
            flags: Vec::new(),
            clock: "2026-01-01T00:00:00".into(),
        };
        let others = Others { written: vec!["OUTFILE".into(), "../ESCAPE".into()], ..Default::default() };
        let runner = Runner { req: &req, others: &others, rdw: Vec::new(), work: dir.join(".work") };
        let inputs = Inputs { files: [("INFILE".to_string(), vec![b"AB".to_vec(), b"CD".to_vec()])].into(), lines: [("SYSIN".to_string(), vec![b"X".to_vec()])].into(), ..Default::default() };
        let interpreter = ran(0, "TOTAL 1\n", "");
        let vm = Ran { files: [("../ESCAPE".to_string(), Some(b"OUT".to_vec()))].into(), ..ran(0, "TOTAL 2\n", "") };
        keep(&dir.join("divergence-0"), &runner, &inputs, &interpreter, &vm, &["standard output differs".into()]).unwrap();
        let kept = dir.join("divergence-0");
        assert_eq!(fs::read(kept.join("input/INFILE")).unwrap(), b"ABCD");
        assert_eq!(fs::read(kept.join("input/SYSIN")).unwrap(), b"X\n");
        assert_eq!(fs::read(kept.join("vm/stdout.txt")).unwrap(), b"TOTAL 2\n");
        assert_eq!(fs::read(kept.join("vm/dd.2")).unwrap(), b"OUT");
        assert!(!dir.join("ESCAPE").exists() && !kept.join("ESCAPE").exists());
        let report = fs::read_to_string(kept.join("report.txt")).unwrap();
        let input = super::super::resolved(&kept.join("input"));
        let at = |name: &str| input.join(name).display().to_string();
        let expected = format!(
            "run /src/P.cbl --clock 2026-01-01T00:00:00 --dd INFILE={} --dd OUTFILE={} --dd ../ESCAPE={} --dd SYSIN={} --statement-limit 500 --vm",
            at("INFILE"),
            at("OUTFILE"),
            at("dd.2"),
            at("SYSIN")
        );
        assert!(report.contains(&expected), "{report}");
        assert!(report.starts_with("interpreter: exit status 0\nvm: exit status 0\nstandard output differs\n"), "{report}");
        fs::remove_dir_all(&dir).unwrap();
    }
}
