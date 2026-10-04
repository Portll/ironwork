//! The exit status of `run`, `cics` and `job`: how the run ended, as a code of the reserved band
//! or, with `--exit-code`, as a verdict. Both come from one mapping, [`Outcome::codes`].

use exec::abend::AbendCode;
use std::process::ExitCode;
use std::sync::atomic::{AtomicU8, Ordering};

/// How a run, a CICS task or a job ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// It ran to its end: the program's RETURN-CODE, or a job's highest step return code.
    Ended(i64),
    /// An abend the message names: a system or user completion code, a CICS abend code, an I/O
    /// status nothing handled, or SQL or SQLR from the database or its recording. For a job, a
    /// step's abend or a JCL error that ended it.
    Abend,
    /// The compile gave no program to run: its return code reached the refusal level, NOCOMPILE
    /// asked for a syntax check, or the source or module holds only user-defined functions.
    Refused,
    /// Code generation refused a construct.
    NotGenerated,
    /// The VM stopped at a construct it does not run yet.
    Stopped,
    /// The run reached a construct ironwork does not run, which its IRONWORK, EXEC and JAVA abends
    /// name, or the job holds JCL ironwork refuses before any step runs.
    NotRun,
    /// The source, JCL or load module cannot be read, or the reader refuses the module.
    Unreadable,
    /// The command line is wrong, or a file, directory, address or database a flag names cannot
    /// be used.
    Usage,
    /// ironwork itself failed: a panic, or a scratch directory it could not make.
    Internal,
}

impl Outcome {
    /// The exit status in the reserved band, and with `--exit-code`.
    pub fn codes(self) -> (u8, u8) {
        match self {
            Self::Ended(rc) => (u8::try_from(rc).ok().filter(|&c| c <= 238).unwrap_or(239), u8::from(rc != 0)),
            Self::Abend => (240, 3),
            Self::Refused => (241, 4),
            Self::NotGenerated => (242, 4),
            Self::Stopped => (243, 5),
            Self::NotRun => (244, 4),
            Self::Unreadable => (245, 2),
            Self::Usage => (246, 2),
            Self::Internal => (255, 70),
        }
    }

    /// How a run ended, read back from its exit status in the band; None for a code the band does
    /// not give. A RETURN-CODE read back from 239 is not its own value.
    pub fn read(code: u8) -> Option<Self> {
        if code <= 239 {
            return Some(Self::Ended(i64::from(code)));
        }
        STOPS.into_iter().find(|o| o.codes().0 == code)
    }

    /// An abend by its code: ironwork's own codes say the run reached what it does not run.
    pub fn of_abend(code: &AbendCode) -> Self {
        if matches!(code, AbendCode::Ironwork | AbendCode::Exec | AbendCode::Java) { Self::NotRun } else { Self::Abend }
    }
}

/// Every way a run can end other than with a RETURN-CODE.
const STOPS: [Outcome; 8] = [Outcome::Abend, Outcome::Refused, Outcome::NotGenerated, Outcome::Stopped, Outcome::NotRun, Outcome::Unreadable, Outcome::Usage, Outcome::Internal];

/// The exit statuses a command follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Convention {
    /// `check`, `compile`, `compare`, `dump`, `fuzz` and `job --expected`, whose statuses
    /// are their own: from here they take only 2 for anything that stops them and 16 for a panic.
    Own = 0,
    Band = 1,
    Verdict = 2,
}

static CONVENTION: AtomicU8 = AtomicU8::new(Convention::Own as u8);

/// Sets the convention the rest of the process exits by, once the command is known.
pub fn follow(convention: Convention) {
    CONVENTION.store(convention as u8, Ordering::Relaxed);
}

fn convention() -> Convention {
    match CONVENTION.load(Ordering::Relaxed) {
        1 => Convention::Band,
        2 => Convention::Verdict,
        _ => Convention::Own,
    }
}

/// The exit status for `outcome` under `convention`, and whether it gives a RETURN-CODE exactly.
pub fn code(outcome: Outcome, convention: Convention) -> (u8, bool) {
    let (band, verdict) = outcome.codes();
    match (convention, outcome) {
        (Convention::Own, Outcome::Internal) => (16, true),
        (Convention::Own, _) => (2, true),
        (Convention::Band, Outcome::Ended(rc)) => (band, (0..=238).contains(&rc)),
        (Convention::Verdict, Outcome::Ended(rc)) => (verdict, rc == 0),
        (Convention::Band, _) => (band, true),
        (Convention::Verdict, _) => (verdict, true),
    }
}

/// The `exit` an evidence journal's `close` record holds: a RETURN-CODE's own value whatever the
/// exit status gives it, else the band's code, whichever convention the process exits by.
pub fn recorded(outcome: Outcome) -> i64 {
    match outcome {
        Outcome::Ended(rc) => rc,
        other => i64::from(other.codes().0),
    }
}

/// The process's exit status for a run's or task's outcome. A RETURN-CODE the status does not
/// give exactly is said on standard error.
pub fn status(outcome: Outcome) -> ExitCode {
    said(outcome, "RETURN-CODE")
}

/// As [`status`], for a job, whose return code is its steps' highest.
pub fn job_status(outcome: Outcome) -> ExitCode {
    said(outcome, "the highest step return code")
}

fn said(outcome: Outcome, what: &str) -> ExitCode {
    let (code, exact) = code(outcome, convention());
    if let (Outcome::Ended(rc), false) = (outcome, exact) {
        eprintln!("ironwork: {what} {rc} exits {code}");
    }
    ExitCode::from(code)
}

#[cfg(test)]
mod tests {
    use super::{Convention, Outcome, STOPS, code};

    const NOT_RUN: [&str; 3] = ["IRONWORK", "EXEC", "JAVA"];

    #[test]
    fn a_return_code_passes_through_up_to_238_and_any_other_exits_239() {
        for rc in [0, 1, 4, 16, 238] {
            assert_eq!(code(Outcome::Ended(rc), Convention::Band), (rc as u8, true), "{rc}");
        }
        for rc in [239, 240, 255, 256, 4095, -1, i64::from(i16::MIN)] {
            assert_eq!(code(Outcome::Ended(rc), Convention::Band), (239, false), "{rc}");
        }
    }

    #[test]
    fn with_exit_code_a_return_code_is_0_or_1() {
        assert_eq!(code(Outcome::Ended(0), Convention::Verdict), (0, true));
        for rc in [1, 4, 239, 1000, -1] {
            assert_eq!(code(Outcome::Ended(rc), Convention::Verdict), (1, false), "{rc}");
        }
    }

    #[test]
    fn every_way_ironwork_ends_a_run_has_a_code_of_its_own_above_239() {
        let band: Vec<u8> = STOPS.iter().map(|&o| code(o, Convention::Band).0).collect();
        assert_eq!(band, [240, 241, 242, 243, 244, 245, 246, 255]);
        let verdict: Vec<u8> = STOPS.iter().map(|&o| code(o, Convention::Verdict).0).collect();
        assert_eq!(verdict, [3, 4, 4, 5, 4, 2, 2, 70]);
        for o in STOPS {
            assert_eq!(Outcome::read(o.codes().0), Some(o));
        }
        assert_eq!(Outcome::read(239), Some(Outcome::Ended(239)));
        assert_eq!(Outcome::read(250), None);
    }

    #[test]
    fn the_commands_with_codes_of_their_own_keep_2_and_16() {
        assert_eq!(code(Outcome::Usage, Convention::Own), (2, true));
        assert_eq!(code(Outcome::Unreadable, Convention::Own), (2, true));
        assert_eq!(code(Outcome::Internal, Convention::Own), (16, true));
    }

    #[test]
    fn ironwork_s_own_abend_codes_say_what_it_does_not_run() {
        use exec::abend::AbendCode;
        for text in NOT_RUN {
            assert_eq!(Outcome::of_abend(&AbendCode::from(text)), Outcome::NotRun, "{text}");
        }
        for text in ["S0C7", "S806", "S322", "U4038", "IO-35", "ASRA", "AEI0", "SQL", "SQLR"] {
            assert_eq!(Outcome::of_abend(&AbendCode::from(text)), Outcome::Abend, "{text}");
        }
    }

    /// docs/run-endings.tsv, which cobolwork vendors to read how a run ended: each status a run
    /// ends with other than its RETURN-CODE, and the abend codes that mean ironwork did not run it.
    #[test]
    fn docs_run_endings_is_this_table() {
        let name = |o: Outcome| {
            let debug = format!("{o:?}");
            debug.chars().enumerate().fold(String::new(), |mut s, (i, c)| {
                if c.is_ascii_uppercase() && i > 0 {
                    s.push('-');
                }
                s.push(c.to_ascii_lowercase());
                s
            })
        };
        let mut table = String::from("# Generated from crates/cli/src/exit.rs by its tests; do not edit.\n# kind\tid\toutcome\n");
        for o in STOPS {
            table.push_str(&format!("status\t{}\t{}\n", o.codes().0, name(o)));
        }
        for code in NOT_RUN {
            table.push_str(&format!("abend\t{code}\t{}\n", name(Outcome::NotRun)));
        }
        // Relative to this file, since the TLS build compiles it from a manifest in tls/.
        let committed = include_str!("../../../docs/run-endings.tsv").replace('\r', "");
        assert!(committed == table, "docs/run-endings.tsv is not this table; write it as:\n{table}");
    }
}
