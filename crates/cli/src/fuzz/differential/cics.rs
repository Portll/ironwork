//! `ironwork fuzz --cics --differential`: runs each generated COMMAREA and operator's typing as a
//! task through `ironwork cics`, on the interpreter and on the VM, and compares the exit status and
//! ending, standard output with the screens the tasks sent, standard error, the task record of each
//! task of the pseudo-conversation (RETURN's TRANSID and COMMAREA, its TS and TD queues), each
//! --file data set and each --td queue's file.

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use super::{Launch, Subject, command, limited};
use crate::fuzz::Rng;
use crate::fuzz_cics::{self, Inputs, Plan, Request};

struct Task<'a> {
    req: &'a Request,
    plan: Plan,
}

/// The file a CICS file's or TD queue's data set has in a run's directory and a kept divergence:
/// `kind.NAME`, or `kind.k` by its place where the name is not a plain one.
fn kept_name(kind: &str, k: usize, name: &str) -> String {
    if jcl::is_name(name) { format!("{kind}.{name}") } else { format!("{kind}.{k}") }
}

impl Subject for Task<'_> {
    type Input = Inputs;

    fn generate(&self, rng: &mut Rng) -> Inputs {
        fuzz_cics::generate(rng, self.plan.shape.as_ref(), &self.plan.terminal)
    }

    fn launch(&self, inputs: &Inputs, dir: &Path, vm: bool) -> std::io::Result<Launch> {
        let mut c = command(&self.req.fuzz, "cics")?;
        for (name, value) in &self.plan.passed {
            c.arg(name).arg(value);
        }
        let (mut left, mut named) = (Vec::new(), Vec::new());
        for (k, file) in self.plan.files.iter().enumerate() {
            let kept = kept_name("file", k, &file.name);
            let path = dir.join(&kept);
            if file.data_set.exists() {
                fs::copy(&file.data_set, &path)?;
            }
            c.arg("--file").arg(format!("{}={},{}", file.name, path.display(), file.spec));
            left.push((format!("CICS file {}", file.name), kept, path.clone()));
            named.push((path, file.name.clone()));
        }
        for (k, queue) in self.plan.queues.iter().enumerate() {
            let kept = kept_name("td", k, queue);
            let path = dir.join(&kept);
            c.arg("--td").arg(format!("{queue}={}", path.display()));
            left.push((format!("TD queue {queue}"), kept, path.clone()));
            named.push((path, queue.clone()));
        }
        if let Some(bytes) = &inputs.commarea {
            let path = dir.join("commarea.bin");
            fs::write(&path, bytes)?;
            c.arg("--commarea").arg(&path);
            named.push((path, "DFHCOMMAREA".into()));
        }
        if let Some(turns) = &inputs.turns {
            let path = dir.join("screens.txt");
            fs::write(&path, fuzz_cics::script(turns))?;
            c.arg("--screens").arg(&path);
            named.push((path, "the screen script".into()));
        }
        let tasks = dir.join("tasks.jsonl");
        c.arg("--task-out").arg(&tasks);
        Ok(Launch { command: limited(c, &self.req.fuzz, vm), left, tasks: Some(tasks), named })
    }

    fn smaller(&self, inputs: Inputs, holds: &mut dyn FnMut(&Inputs) -> bool) -> Inputs {
        fuzz_cics::smaller(self.plan.shape.as_ref(), inputs, holds)
    }
}

pub fn run(req: Request) -> ExitCode {
    let plan = match fuzz_cics::plan(&req) {
        Ok(p) => p,
        Err(code) => return code,
    };
    if let Some(t) = &plan.inferred {
        eprintln!("ironwork fuzz: each task runs as transaction {t}, the one RETURN TRANSID names");
    }
    if !plan.terminal.missing.is_empty() {
        eprintln!("ironwork fuzz: no library holds mapset {}, so the fields of its maps are typed as text of no map", plan.terminal.missing.iter().cloned().collect::<Vec<_>>().join(", "));
    }
    let task = Task { req: &req, plan };
    super::fuzz(&req.fuzz, &task.plan.compiled, &task)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuzz::differential::{Ran, keep};
    use std::time::Duration;

    #[test]
    fn a_kept_input_holds_the_commarea_and_screens_what_each_task_left_and_the_command_that_repeats_it() {
        let dir = std::env::temp_dir().join(format!("iw-differential-cics-keep-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        let program = dir.join("src/TASK.cbl");
        let source = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. TASK.\n       DATA DIVISION.\n       LINKAGE SECTION.\n       01  DFHCOMMAREA.\n           05 CA-QTY PIC 9(3).\n       PROCEDURE DIVISION.\n           EXEC CICS SEND TEXT FROM(CA-QTY) END-EXEC\n           EXEC CICS RETURN TRANSID('TSK1')\n                COMMAREA(DFHCOMMAREA) END-EXEC.\n";
        fs::write(&program, source).unwrap();
        let fuzz = crate::fuzz::Request {
            program: program.clone(),
            out: dir.join("out"),
            root: dir.clone(),
            runs: 1,
            seed: 1,
            timeout: Duration::from_secs(1),
            hang_limit: 500,
            libraries: Vec::new(),
            program_dirs: Vec::new(),
            flags: Vec::new(),
            clock: "2026-01-01T00:00:00".into(),
        };
        let req = Request { fuzz, options: vec![("--td".into(), "LOGQ=ignored".into())] };
        let plan = fuzz_cics::plan(&req).unwrap_or_else(|_| panic!("the task plans"));
        assert_eq!(plan.inferred.as_deref(), Some("TSK1"));
        let task = Task { req: &req, plan };
        let inputs = fuzz_cics::generate(&mut Rng(3), task.plan.shape.as_ref(), &task.plan.terminal);
        let inputs = Inputs { commarea: Some(b"\xF0\xF4\xF2".to_vec()), ..inputs };
        let ran = |record: &str| Ran { timed_out: false, status: Some(0), out: Vec::new(), err: String::new(), left: Vec::new(), tasks: record.as_bytes().to_vec() };
        let kept = dir.join("out/divergence-0");
        keep(&kept, &task, &inputs, &ran("{\"task\":1}\n"), &ran("{\"task\":2}\n"), &["the task record differs".into()]).unwrap();
        assert_eq!(fs::read(kept.join("input/commarea.bin")).unwrap(), b"\xF0\xF4\xF2");
        assert_eq!(fs::read_to_string(kept.join("vm/tasks.jsonl")).unwrap(), "{\"task\":2}\n");
        let report = fs::read_to_string(kept.join("report.txt")).unwrap();
        let input = crate::fuzz::resolved(&kept.join("input"));
        let at = |name: &str| input.join(name).display().to_string();
        let expected = format!(
            "cics {} --clock 2026-01-01T00:00:00 --transid TSK1 --td LOGQ={} --commarea {}",
            program.display(),
            at("td.LOGQ"),
            at("commarea.bin")
        );
        assert!(report.contains(&expected), "{report}");
        assert!(report.contains(&format!("--task-out {} --statement-limit 500 --vm", at("tasks.jsonl"))), "{report}");
        fs::remove_dir_all(&dir).unwrap();
    }
}
