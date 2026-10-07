//! What an assumption of the register governs, and what a run holds of it: the statement kinds and
//! data usages of the programs it entered, and the options in force. An assumption governs a run
//! when every trigger of one of its conjunctions is among the run's facts, so the ids a run names
//! are those its outcome could have rested on, at program granularity: an over-approximation.

use crate::options::{DispSign, IntDate, Qualify, Quote, Trunc};
use crate::{Compliance, Numproc, Options};

macro_rules! vocabulary {
    ($(#[$doc:meta])* $name:ident { $($variant:ident => $word:literal,)* }) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum $name {
            $($variant,)*
        }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant,)*];

            /// The word a load module and `ironwork assumptions --json` give it.
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $word,)*
                }
            }

            pub fn named(word: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|k| k.name() == word)
            }

            const fn bit(self) -> u64 {
                1 << self as u32
            }
        }
    };
}

vocabulary! {
    /// A statement, or a construct of the PROCEDURE DIVISION, a program holds.
    Statement {
        Move => "move",
        Arithmetic => "arithmetic",
        Exponentiation => "exponentiation",
        Corresponding => "corresponding",
        Display => "display",
        Accept => "accept",
        Call => "call",
        Cancel => "cancel",
        Entry => "entry",
        Stop => "stop-run",
        Alter => "alter",
        Perform => "perform",
        Condition => "condition",
        Initialize => "initialize",
        Inspect => "inspect",
        Set => "set",
        Sort => "sort",
        Merge => "merge",
        TableSort => "table-sort",
        FileIo => "file-io",
        WriteAdvancing => "write-advancing",
        UseProcedure => "use-procedure",
        Debugging => "debugging",
        Function => "intrinsic-function",
        UserFunction => "user-function",
        CopyMember => "copy-member",
        Sql => "exec-sql",
        Cics => "exec-cics",
        Dli => "exec-dli",
        Xml => "xml",
        Json => "json",
        Invoke => "invoke",
        ReportWriter => "report-writer",
        Screen => "screen",
    }
}

vocabulary! {
    /// A usage, or a file organization or data description clause, a program's data holds.
    Usage {
        Zoned => "zoned",
        Packed => "packed",
        Binary => "binary",
        NativeBinary => "native-binary",
        Float => "float",
        National => "national",
        Dbcs => "dbcs",
        NumericEdited => "numeric-edited",
        ObjectReference => "object-reference",
        ProgramPointer => "program-pointer",
        IndexedFile => "indexed-file",
        RelativeFile => "relative-file",
        LinageFile => "linage-file",
        Synchronized => "synchronized",
        OccursDepending => "occurs-depending",
        External => "external",
        Global => "global",
        Alphabet => "alphabet",
        DecimalComma => "decimal-point-comma",
        Upsi => "upsi",
    }
}

vocabulary! {
    /// A compiler option a program was compiled with, or a way the run was made.
    OptionFact {
        TruncOpt => "TRUNC(OPT)",
        NumprocPfd => "NUMPROC(PFD)",
        Ssrange => "SSRANGE",
        Fastsrt => "FASTSRT",
        Thread => "THREAD",
        Invdata => "INVDATA",
        Numcheck => "NUMCHECK",
        Initcheck => "INITCHECK",
        Parmcheck => "PARMCHECK",
        IntdateLilian => "INTDATE(LILIAN)",
        QualifyExtend => "QUALIFY(EXTEND)",
        DispsignSep => "DISPSIGN(SEP)",
        Apost => "APOST",
        Currency => "CURRENCY",
        Cards => "cbl-card",
        MixedCodepage => "mixed-codepage",
        Extended => "compliance-extended",
        CicsTask => "cics-task",
        JobStep => "job-step",
        Parm => "parm",
        StatementLimit => "statement-limit",
    }
}

const _: () = assert!(Statement::ALL.len() <= 64 && Usage::ALL.len() <= 64 && OptionFact::ALL.len() <= 64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    Statement(Statement),
    Usage(Usage),
    Option(OptionFact),
    /// The claim holds for every program: storage layout, code page tables, how source is read.
    Always,
}

impl Trigger {
    /// `statement:sort`, `usage:packed`, `option:TRUNC(OPT)` or `always`.
    pub fn name(self) -> String {
        match self {
            Self::Statement(s) => format!("statement:{}", s.name()),
            Self::Usage(u) => format!("usage:{}", u.name()),
            Self::Option(o) => format!("option:{}", o.name()),
            Self::Always => "always".into(),
        }
    }
}

/// Any of the conjunctions, each met when all of its triggers are.
pub type Governs = &'static [&'static [Trigger]];

/// The statement kinds, usages and options a program holds, or a run's programs together.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Facts {
    statements: u64,
    usages: u64,
    options: u64,
}

impl Facts {
    /// Every statement kind and usage, for a program whose own are not known; no option.
    pub fn every_construct() -> Self {
        let all = |bits: &mut u64, n: usize| *bits = if n == 64 { u64::MAX } else { (1 << n) - 1 };
        let mut facts = Self::default();
        all(&mut facts.statements, Statement::ALL.len());
        all(&mut facts.usages, Usage::ALL.len());
        facts
    }

    pub fn insert(&mut self, t: Trigger) {
        match t {
            Trigger::Statement(s) => self.statements |= s.bit(),
            Trigger::Usage(u) => self.usages |= u.bit(),
            Trigger::Option(o) => self.options |= o.bit(),
            Trigger::Always => {}
        }
    }

    pub fn statement(&mut self, s: Statement) {
        self.insert(Trigger::Statement(s));
    }

    pub fn usage(&mut self, u: Usage) {
        self.insert(Trigger::Usage(u));
    }

    pub fn option(&mut self, o: OptionFact) {
        self.insert(Trigger::Option(o));
    }

    pub fn has(&self, t: Trigger) -> bool {
        match t {
            Trigger::Statement(s) => self.statements & s.bit() != 0,
            Trigger::Usage(u) => self.usages & u.bit() != 0,
            Trigger::Option(o) => self.options & o.bit() != 0,
            Trigger::Always => true,
        }
    }

    pub fn union(&mut self, other: Self) {
        self.statements |= other.statements;
        self.usages |= other.usages;
        self.options |= other.options;
    }

    /// Whether any conjunction of `governs` is met.
    pub fn meet(&self, governs: Governs) -> bool {
        governs.iter().any(|all| all.iter().all(|&t| self.has(t)))
    }

    pub fn statements(&self) -> impl Iterator<Item = Statement> + '_ {
        Statement::ALL.iter().copied().filter(|s| self.statements & s.bit() != 0)
    }

    pub fn usages(&self) -> impl Iterator<Item = Usage> + '_ {
        Usage::ALL.iter().copied().filter(|u| self.usages & u.bit() != 0)
    }

    pub fn options(&self) -> impl Iterator<Item = OptionFact> + '_ {
        OptionFact::ALL.iter().copied().filter(|o| self.options & o.bit() != 0)
    }

    /// The options a program compiled with `options` holds, SSRANGE and its CBL or PROCESS cards
    /// being kept beside them.
    pub fn of_options(options: &Options, ssrange: bool, cards: bool) -> Self {
        let mut facts = Self::default();
        let held = [
            (options.trunc == Trunc::Opt, OptionFact::TruncOpt),
            (options.numproc == Numproc::Pfd, OptionFact::NumprocPfd),
            (ssrange, OptionFact::Ssrange),
            (options.fastsrt, OptionFact::Fastsrt),
            (options.thread, OptionFact::Thread),
            (options.invdata.is_some(), OptionFact::Invdata),
            (options.numcheck.is_some(), OptionFact::Numcheck),
            (options.initcheck.is_some(), OptionFact::Initcheck),
            (options.parmcheck.is_some(), OptionFact::Parmcheck),
            (options.intdate == IntDate::Lilian, OptionFact::IntdateLilian),
            (options.qualify == Qualify::Extend, OptionFact::QualifyExtend),
            (options.dispsign == DispSign::Sep, OptionFact::DispsignSep),
            (options.quote == Quote::Apost, OptionFact::Apost),
            (options.currency.is_some(), OptionFact::Currency),
            (cards, OptionFact::Cards),
            (zarch::ebcdic::CodePage::by_ccsid(options.codepage).is_some_and(|p| p.dbcs().is_some()), OptionFact::MixedCodepage),
            (options.compliance == Compliance::Extended, OptionFact::Extended),
        ];
        for (on, fact) in held {
            if on {
                facts.option(fact);
            }
        }
        facts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_word_names_one_fact_and_reads_back() {
        for s in Statement::ALL {
            assert_eq!(Statement::named(s.name()), Some(*s));
        }
        for u in Usage::ALL {
            assert_eq!(Usage::named(u.name()), Some(*u));
        }
        for o in OptionFact::ALL {
            assert_eq!(OptionFact::named(o.name()), Some(*o));
        }
        assert_eq!(Statement::named("packed"), None);
    }

    #[test]
    fn a_conjunction_needs_every_trigger_and_any_one_suffices() {
        let mut facts = Facts::default();
        facts.usage(Usage::Binary);
        let governs: Governs = &[&[Trigger::Option(OptionFact::TruncOpt), Trigger::Usage(Usage::Binary)], &[Trigger::Statement(Statement::Sort)]];
        assert!(!facts.meet(governs));
        facts.option(OptionFact::TruncOpt);
        assert!(facts.meet(governs));
        assert!(Facts::default().meet(&[&[Trigger::Always]]));
        assert!(!Facts::default().meet(&[]));
    }

    #[test]
    fn every_construct_holds_each_statement_and_usage_and_no_option() {
        let every = Facts::every_construct();
        assert_eq!(every.statements().count(), Statement::ALL.len());
        assert_eq!(every.usages().count(), Usage::ALL.len());
        assert_eq!(every.options().count(), 0);
    }

    #[test]
    fn options_are_read_from_what_the_compile_was_given() {
        let mut o = Options::default();
        assert_eq!(Facts::of_options(&o, false, false).options().count(), 0);
        o.trunc = Trunc::Opt;
        o.codepage = 930;
        let facts = Facts::of_options(&o, true, true);
        assert_eq!(facts.options().collect::<Vec<_>>(), [OptionFact::TruncOpt, OptionFact::Ssrange, OptionFact::Cards, OptionFact::MixedCodepage]);
    }
}
