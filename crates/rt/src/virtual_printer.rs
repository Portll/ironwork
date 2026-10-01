//! The virtual printer. In a run given DD PRINTER, a CALL of SYSTEM or C$SYSTEM whose command prints
//! files with lp or lpr appends those files to that DD, and nothing runs on the host. Each file the
//! command names is a DD, as an ASSIGN literal names one, so the run still reaches only the files the
//! operator gave it.

use crate::files::{Dd, Dds};
use crate::unit::Event;
use crate::vocab::OpenMode;
use std::fs::OpenOptions;
use std::io::Write;

pub const DD: &str = "PRINTER";
pub const ROUTINES: &[&str] = &["SYSTEM", "C$SYSTEM"];

/// The files an lp or lpr command prints, in order.
#[derive(Debug, PartialEq, Eq)]
pub struct Job {
    pub files: Vec<String>,
}

/// The option letters CUPS documents for a command: those that take a value, attached or as the
/// next word, and those that stand alone.
struct Options {
    valued: &'static str,
    flags: &'static str,
}

const LP: Options = Options { valued: "dhnoqtHPU", flags: "Ecms" };
const LPR: Options = Options { valued: "#CHJPTUo", flags: "Ehlmpqr" };

/// A word the shell passes as it stands: any other character has it expand, quote, redirect or
/// chain commands, and a leading `#` starts a comment.
fn plain(word: &str) -> bool {
    !word.starts_with('#') && word.chars().all(|c| c.is_ascii_alphanumeric() || "-_.,/=:+@%#".contains(c))
}

/// `"$NAME"` or `"${NAME}"`: one word holding the variable's value, which the shell neither splits
/// nor runs, so a program can pass an option's value through the environment.
fn quoted_variable(word: &str) -> bool {
    let Some(inner) = word.strip_prefix("\"$").and_then(|w| w.strip_suffix('"')) else { return false };
    let name = inner.strip_prefix('{').and_then(|n| n.strip_suffix('}')).unwrap_or(inner);
    name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

impl Job {
    /// The job `command` asks for when it is lp or lpr, with options CUPS documents, naming at
    /// least one file; `None` for any other command, including one that reads standard input.
    pub fn parse(command: &str) -> Option<Job> {
        let command = command.split('\0').next().unwrap_or_default();
        let mut words = command.split(' ').filter(|w| !w.is_empty());
        let program = words.next().filter(|w| plain(w))?;
        let options = match program.rsplit('/').next()? {
            "lp" => LP,
            "lpr" => LPR,
            _ => return None,
        };
        let mut files = Vec::new();
        let mut in_options = true;
        while let Some(word) = words.next() {
            if word == "-" {
                return None;
            }
            if in_options && word == "--" {
                in_options = false;
                continue;
            }
            let Some(letters) = word.strip_prefix('-').filter(|_| in_options) else {
                if !plain(word) {
                    return None;
                }
                files.push(word.to_owned());
                continue;
            };
            for (at, letter) in letters.char_indices() {
                if options.valued.contains(letter) {
                    let value = match &letters[at + 1..] {
                        "" => words.next()?,
                        attached => attached,
                    };
                    if !plain(value) && !quoted_variable(value) {
                        return None;
                    }
                    break;
                }
                if !options.flags.contains(letter) {
                    return None;
                }
            }
        }
        (!files.is_empty()).then_some(Job { files })
    }
}

/// Prints `job` on `printer`: each file's DD, read whole, appended to it in order. Every file
/// needs a DD before anything prints, as lp prints nothing when it cannot read one. `notify`
/// hears each DD opened and closed.
pub fn print(dds: &Dds, printer: &Dd, job: &Job, notify: &mut dyn FnMut(Event<'_>)) -> Result<(), String> {
    let named = job
        .files
        .iter()
        .map(|file| {
            let name = file.to_ascii_uppercase();
            dds.get(&name).map(|dd| (name.clone(), dd)).ok_or_else(|| format!("{file}: no DD {name}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut bytes = Vec::new();
    for (name, dd) in &named {
        notify(Event::Open { dd: name, mode: OpenMode::Input, path: &dd.path });
        let read = std::fs::read(&dd.path);
        notify(Event::Close { dd: name, path: &dd.path });
        bytes.extend(read.map_err(|e| format!("DD {name} {}: {e}", dd.path.display()))?);
    }
    notify(Event::Open { dd: DD, mode: OpenMode::Extend, path: &printer.path });
    let written = OpenOptions::new().create(true).append(true).open(&printer.path).and_then(|mut f| f.write_all(&bytes));
    notify(Event::Close { dd: DD, path: &printer.path });
    written.map_err(|e| format!("DD {DD} {}: {e}", printer.path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(command: &str) -> Option<Vec<String>> {
        Job::parse(command).map(|j| j.files)
    }

    #[test]
    fn lp_and_lpr_name_the_files_they_print() {
        let padded = format!("lp -d HP_LaserJet -o cpi=10 -o lpi=6 report.txt{}", " ".repeat(40));
        assert_eq!(files(&padded), Some(vec!["report.txt".into()]));
        assert_eq!(files("lpr -P office -#2 -h a.lst b.lst"), Some(vec!["a.lst".into(), "b.lst".into()]));
        assert_eq!(files("/usr/bin/lp -dqueue -n 3 -- -odd.txt\0garbage"), Some(vec!["-odd.txt".into()]));
        assert_eq!(files("lp -cs report.txt"), Some(vec!["report.txt".into()]));
        assert_eq!(files("lp -d \"$CH7ASG02_PRINTER\" -o cpi=10 -o lpi=6 report.txt"), Some(vec!["report.txt".into()]));
        assert_eq!(files("lpr -P\"${QUEUE}\" report.txt"), Some(vec!["report.txt".into()]));
    }

    #[test]
    fn a_command_the_shell_would_do_more_with_is_not_a_print() {
        for command in [
            "lp -d x;rm -rf ~ report.txt",
            "lp report.txt | mail me",
            "lp -d $(id) report.txt",
            "lp \"report.txt\"",
            "lp report.txt\nrm report.txt",
            "lp report.txt #note",
            "lp -d",
            "lp -d ;x report.txt",
            "$(id)/lp report.txt",
            "lp -d \"$(id)\" report.txt",
            "lp -d \"$Q;id\" report.txt",
            "lp -d \"$1\" report.txt",
            "lp -d '$Q' report.txt",
            "lp -d $Q report.txt",
            "lp \"$REPORT\"",
            "lp -c\"$Q\" report.txt",
        ] {
            assert_eq!(files(command), None, "{command}");
        }
    }

    #[test]
    fn anything_else_is_not_a_print() {
        for command in ["lpstat -p", "lp", "lp -d office", "lp -", "lp -i 12 -n 2", "lp -z report.txt", "lpr -d office report.txt", "cat report.txt", "", "  "] {
            assert_eq!(files(command), None, "{command:?}");
        }
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("iw-vprinter-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_job_appends_each_file_and_reaches_only_given_dds() {
        let dir = scratch("print");
        std::fs::write(dir.join("report"), b"PAYROLL\n").unwrap();
        let printer = dir.join("printer.prn");
        let dds = Dds::new(&[format!("REPORT.TXT={}", dir.join("report").display()), format!("PRINTER={}", printer.display())], false).unwrap();
        let dd = dds.get(DD).unwrap();
        let mut heard = Vec::new();
        let job = Job::parse("lp -d office report.txt").unwrap();
        print(&dds, &dd, &job, &mut |e| heard.push(matches!(e, Event::Open { .. }))).unwrap();
        print(&dds, &dd, &job, &mut |_| {}).unwrap();
        assert_eq!(std::fs::read(&printer).unwrap(), b"PAYROLL\nPAYROLL\n");
        assert_eq!(heard, [true, false, true, false]);
        let refused = print(&dds, &dd, &Job::parse("lp report.txt /etc/passwd").unwrap(), &mut |_| {});
        assert_eq!(refused, Err("/etc/passwd: no DD /ETC/PASSWD".to_string()));
        assert_eq!(std::fs::read(&printer).unwrap(), b"PAYROLL\nPAYROLL\n", "nothing prints when one file has no DD");
    }
}
