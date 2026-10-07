//! `ironwork fuzz --interface --differential`: runs a subprogram on each generated argument set
//! through `ironwork run --argument`, on the interpreter and on the VM, and compares the exit status
//! (RETURN-CODE) and ending, standard output, standard error, each argument as the caller sees it
//! after the CALL, and each data set its files are given.

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use super::{Launch, Subject, command, kept_name, limited};
use crate::fuzz::interface::{self, Arguments, NameSlot, Setup};
use crate::fuzz::{Request, Rng};

struct Subprogram<'a> {
    req: &'a Request,
    setup: Setup,
    slots: Vec<(NameSlot, Vec<String>)>,
}

impl Subject for Subprogram<'_> {
    type Input = Arguments;

    /// Arguments shaped by a CALL that passes them, where any does, with a program name a library
    /// holds where a CALL takes one from them.
    fn generate(&self, rng: &mut Rng) -> Arguments {
        let sites = &self.setup.sites;
        let site = (!sites.is_empty()).then(|| &sites[rng.below(sites.len())]);
        let mut generated = interface::arguments(rng, &self.setup.params, site);
        for (slot, names) in &self.slots {
            interface::with_name(&mut generated, &self.setup.params, *slot, &names[rng.below(names.len())]);
        }
        generated
    }

    fn launch(&self, arguments: &Arguments, dir: &Path, vm: bool) -> std::io::Result<Launch> {
        let mut c = command(self.req, "run")?;
        let (mut left, mut named) = (Vec::new(), Vec::new());
        let data_sets = &self.setup.data_sets;
        for (k, dd) in data_sets.empty.iter().chain(&data_sets.new).enumerate() {
            let kept = kept_name(k, dd);
            let path = dir.join(&kept);
            if k < data_sets.empty.len() {
                fs::write(&path, b"")?;
            }
            c.arg("--dd").arg(format!("{dd}={}", path.display()));
            left.push((format!("DD {dd}"), kept, path.clone()));
            named.push((path, dd.clone()));
        }
        let returned = dir.join("returned.arguments");
        for (i, (param, argument)) in self.setup.params.iter().zip(arguments).enumerate() {
            let kept = format!("argument.{}", i + 1);
            c.arg("--argument");
            match argument {
                Some(bytes) => {
                    let path = dir.join(&kept);
                    fs::write(&path, bytes)?;
                    c.arg(&path);
                    named.push((path, format!("argument {}", param.name)));
                }
                None => {
                    c.arg("OMITTED");
                }
            }
            left.push((format!("argument {}", param.name), kept, returned.join(format!("arg{}", i + 1))));
        }
        c.arg("--arguments-out").arg(&returned);
        Ok(Launch { command: limited(c, self.req, vm), left, tasks: None, named })
    }

    fn smaller(&self, arguments: Arguments, holds: &mut dyn FnMut(&Arguments) -> bool) -> Arguments {
        interface::smaller(&self.setup.params, arguments, holds)
    }
}

pub fn run(req: Request) -> ExitCode {
    let (compiled, rest) = match crate::fuzz::compile(&req) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(12);
        }
    };
    if let Some(why) = interface::refusal(&compiled) {
        eprintln!("ironwork fuzz: {why}");
        return ExitCode::from(2);
    }
    let setup = interface::setup(&req, &compiled, rest);
    setup.data_sets.tell(&setup.ungiven);
    let slots = interface::name_slots(&compiled, &setup.params, &setup.candidates);
    super::fuzz(&req, &compiled, &Subprogram { req: &req, setup, slots })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuzz::differential::{Left, Ran, keep};
    use std::path::PathBuf;
    use std::time::Duration;

    #[test]
    fn a_kept_input_holds_each_argument_what_each_executor_returned_and_the_command_that_repeats_it() {
        let dir = std::env::temp_dir().join(format!("iw-differential-interface-keep-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        let program = dir.join("src/SUB.cbl");
        let source = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. SUB.\n       DATA DIVISION.\n       LINKAGE SECTION.\n       01  QTY PIC 9(3).\n       01  NOTE PIC X(2).\n       PROCEDURE DIVISION USING QTY NOTE.\n           ADD 1 TO QTY\n           GOBACK.\n";
        fs::write(&program, source).unwrap();
        let req = Request {
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
        let (compiled, rest) = crate::fuzz::compile(&req).unwrap();
        let subject = Subprogram { req: &req, setup: interface::setup(&req, &compiled, rest), slots: Vec::new() };
        let arguments = vec![Some(b"\xF0\xF4\xF1".to_vec()), None];
        let ran = |qty: &[u8]| Ran {
            timed_out: false,
            status: Some(0),
            out: Vec::new(),
            err: String::new(),
            left: vec![
                Left { what: "argument QTY".into(), file: "argument.1".into(), bytes: Some(qty.to_vec()) },
                Left { what: "argument NOTE".into(), file: "argument.2".into(), bytes: None },
            ],
            tasks: Vec::new(),
        };
        let kept = dir.join("out/divergence-0");
        keep(&kept, &subject, &arguments, &ran(b"\xF0\xF4\xF2"), &ran(b"\xF0\xF4\xF3"), &["argument QTY differs".into()]).unwrap();
        assert_eq!(fs::read(kept.join("input/argument.1")).unwrap(), b"\xF0\xF4\xF1");
        assert!(!kept.join("input/argument.2").exists());
        assert_eq!(fs::read(kept.join("vm/argument.1")).unwrap(), b"\xF0\xF4\xF3");
        let report = fs::read_to_string(kept.join("report.txt")).unwrap();
        let input: PathBuf = crate::fuzz::resolved(&kept.join("input"));
        let expected = format!(
            "run {} --clock 2026-01-01T00:00:00 --argument {} --argument OMITTED --arguments-out {} --statement-limit 500 --vm",
            program.display(),
            input.join("argument.1").display(),
            input.join("returned.arguments").display()
        );
        assert!(report.contains(&expected), "{report}");
        fs::remove_dir_all(&dir).unwrap();
    }
}
