use std::fmt;
use zarch::ebcdic::CodePage;
use zarch::hfp::Precision;

/// Enterprise COBOL's compiler options, from Table 45 of IBM's Programming Guide, vendored byte for
/// byte from cobolwork's provenance/enterprise-options.tsv (tools/sync-option-table.sh).
const TABLE: &str = include_str!("../data/enterprise-options.tsv");

/// An option in IBM's table: the spellings it answers to, and where it may be given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Documented {
    pub name: &'static str,
    spellings: &'static str,
    /// Whether a CBL or PROCESS statement may give it.
    pub process: bool,
    /// Whether a CBL or PROCESS statement may give it only before a batch compilation's first program.
    pub first_program_only: bool,
    pub installation_default: bool,
    /// The page of its section in the Enterprise COBOL 6.4 Programming Guide.
    pub page: u16,
}

impl Documented {
    pub fn spellings(&self) -> impl Iterator<Item = &'static str> {
        self.spellings.split(' ')
    }
}

/// Every option in IBM's table.
pub fn documented() -> impl Iterator<Item = Documented> {
    TABLE.lines().filter(|l| !l.starts_with('#')).map(|line| {
        let f: Vec<&'static str> = line.split('\t').collect();
        let yes = |i: usize| f[i] == "yes";
        Documented { name: f[0], spellings: f[2], process: yes(3), first_program_only: yes(4), installation_default: yes(5), page: f[6].parse().expect("the table's pages are numbers") }
    })
}

/// The option IBM documents under `spelling`, and whether the spelling turns the option off: a
/// spelling beginning NO that is not the option's own name.
pub fn spelled(spelling: &str) -> Option<(Documented, bool)> {
    let o = documented().find(|o| o.spellings().any(|s| s == spelling))?;
    Some((o, spelling.starts_with("NO") && !o.name.split('/').any(|n| n == spelling)))
}

/// Whether `option`, as a CBL or PROCESS statement writes it, sets the on-off option `name`, and to
/// which: `SSR(ZLEN)` gives Some(true) for SSRANGE, `NOSSR` Some(false), and TRUNC(OPT) None.
pub fn switch(option: &str, name: &str) -> Option<bool> {
    let option = option.trim().to_ascii_uppercase();
    let word = option.split('(').next().unwrap_or("").trim();
    spelled(word).filter(|(o, _)| o.name == name).map(|(_, off)| !off)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Arith {
    #[default]
    Compat,
    Extend,
}

impl Arith {
    pub const fn max_picture_digits(self) -> u32 {
        match self {
            Self::Compat => 18,
            Self::Extend => 31,
        }
    }

    pub const fn intermediate_digits(self) -> u32 {
        match self {
            Self::Compat => 30,
            Self::Extend => 31,
        }
    }

    pub const fn float_intermediate(self) -> Precision {
        match self {
            Self::Compat => Precision::Long,
            Self::Extend => Precision::Extended,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Trunc {
    #[default]
    Std,
    Opt,
    Bin,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Numproc {
    #[default]
    Nopfd,
    Pfd,
}

/// Whether checked mode reports: a store that TRUNC(OPT) leaves to the generated code, when decimal
/// and binary truncation disagree, and a SORT whose outcome FASTSRT changes. `-silent` turns the
/// reports off.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TruncCheck {
    #[default]
    Report,
    Silent,
}

/// How SORT and MERGE compare a zoned or packed key: as DFSORT compares ZD and PD fields, so no
/// bytes are invalid, or (`-strict-sort-keys`) as the program reads the item, so an invalid one is a
/// data exception.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortKeys {
    #[default]
    Dfsort,
    Strict,
}

/// Whether FASTSRT gives DFSORT the I/O of a USING or GIVING print file under ADV, whose data set's
/// records are a byte longer than its FD's: never (`--fastsrt-adv-print=exclude`), or as any other
/// file's (`--fastsrt-adv-print=include`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FastsrtAdvPrint {
    #[default]
    Exclude,
    Include,
}

impl FastsrtAdvPrint {
    pub const fn flag(self) -> &'static str {
        match self {
            Self::Exclude => "--fastsrt-adv-print=exclude",
            Self::Include => "--fastsrt-adv-print=include",
        }
    }
}

/// What a program with no STOP RUN, GOBACK or EXIT PROGRAM that ends with EXEC CICS RETURN or XCTL
/// gets (assumption C124): IBM's warning (`--cics-return-warning=always`), one informational note
/// in place of it (`once`), or nothing (`never`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CicsReturnWarning {
    #[default]
    Once,
    Always,
    Never,
}

impl CicsReturnWarning {
    pub const fn flag(self) -> &'static str {
        match self {
            Self::Once => "--cics-return-warning=once",
            Self::Always => "--cics-return-warning=always",
            Self::Never => "--cics-return-warning=never",
        }
    }
}

/// Whether a program whose compile gave warnings runs (`Proceed`), or (`-warnings-block`, the
/// command line's NOCOMPILE(W)) is refused. The return code is 4 either way.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Warnings {
    #[default]
    Proceed,
    Block,
}

/// The COMPILE option: which messages stop the object code, so that run and cics refuse the
/// program (Programming Guide SC27-8714-03, p. 355).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compile {
    /// COMPILE: object code whatever the messages, unless one is U and the compilation ended.
    Full,
    /// NOCOMPILE(W), NOCOMPILE(E) or NOCOMPILE(S): none from the first message of that severity up.
    Until(Stop),
    /// NOCOMPILE: a syntax check, with no object code at all.
    SyntaxOnly,
}

/// A severity NOCOMPILE names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    W,
    E,
    S,
}

impl Default for Compile {
    fn default() -> Self {
        Self::Until(Stop::S)
    }
}

impl Compile {
    /// The lowest return code (Programming Guide Table 38, p. 282) of a message that stops the object
    /// code: 0 stops it whatever the messages.
    pub const fn stops_at(self) -> u8 {
        match self {
            Self::Full => 16,
            Self::Until(Stop::S) => 12,
            Self::Until(Stop::E) => 8,
            Self::Until(Stop::W) => 4,
            Self::SyntaxOnly => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    pub arith: Arith,
    pub trunc: Trunc,
    pub numproc: Numproc,
    pub codepage: u16,
    pub trunc_check: TruncCheck,
    /// FASTSRT: DFSORT does the I/O of a SORT's USING and GIVING files where IBM's rules allow.
    pub fastsrt: bool,
    pub fastsrt_adv_print: FastsrtAdvPrint,
    pub sort_keys: SortKeys,
    /// ADV: a print file's printer control character is a byte added before each record; under
    /// NOADV it is the record's own first byte.
    pub adv: bool,
    /// THREAD, DLL, RENT and DBCS, which object-oriented programs are compiled with.
    pub thread: bool,
    pub dll: bool,
    pub rent: bool,
    pub dbcs: bool,
    pub warnings: Warnings,
    /// COMPILE or NOCOMPILE as a CBL or PROCESS card gave it; None when none did.
    pub compile: Option<Compile>,
    /// DYNAM: a CALL of a literal loads the program at run time, as a CALL of an identifier does.
    pub dynam: bool,
    /// The Language Environment runtime option DEBUG (`-debug`): USE FOR DEBUGGING procedures run.
    /// NODEBUG, IBM's default, keeps them from running (assumption C63).
    pub debug: bool,
    pub cics_return_warning: CicsReturnWarning,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            arith: Arith::default(),
            trunc: Trunc::default(),
            numproc: Numproc::default(),
            codepage: 1140,
            trunc_check: TruncCheck::default(),
            fastsrt: false,
            fastsrt_adv_print: FastsrtAdvPrint::default(),
            sort_keys: SortKeys::default(),
            adv: true,
            thread: false,
            dll: false,
            rent: true,
            dbcs: true,
            warnings: Warnings::default(),
            compile: None,
            dynam: false,
            debug: false,
            cics_return_warning: CicsReturnWarning::default(),
        }
    }
}

/// What an option could not do. Each but UnsupportedCodePage and UnknownFlag is a message IBM's
/// compiler gives and carries on from (assumptions C120 to C122).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OptionError {
    /// A suboption the option does not have: the option is discarded.
    BadSuboption { option: String, given: String },
    /// A suboption IBM removed, replaced by `instead`.
    Removed { option: String, since: &'static str, instead: &'static str },
    /// An option Enterprise COBOL 6.4 does not have, given a warning or (`warning` false) an
    /// informational message and no effect.
    NoEffect { option: &'static str, why: &'static str, warning: bool },
    UnsupportedCodePage(u16),
    UnknownFlag(String),
}

impl fmt::Display for OptionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadSuboption { option, given } => write!(f, "{option} does not take ({given})"),
            Self::Removed { option, since, instead } => write!(f, "{option} was removed in Enterprise COBOL {since}, so {instead} is in effect"),
            Self::NoEffect { option, why, .. } => write!(f, "{option} {why}"),
            Self::UnsupportedCodePage(ccsid) => write!(f, "CODEPAGE({ccsid}) is not a single-byte EBCDIC page this compiler carries"),
            Self::UnknownFlag(flag) => write!(f, "unknown flag {flag}"),
        }
    }
}

/// The options IBM removed from Enterprise COBOL that a 6.4 compile accepts without effect, by
/// spelling: LIB and SIZE (Migration Guide GC27-8715-03, Table 32, p. 167), FLAGSAA and NOFDUMP
/// (Table 23, p. 112).
fn without_effect(name: &str) -> Option<OptionError> {
    let (option, why, warning) = match name {
        "LIB" => ("LIB", "is no longer needed: COPY members are always read from the libraries", false),
        "SIZE" | "SZ" => ("SIZE", "was removed in Enterprise COBOL V5 and has no effect", false),
        "FLAGSAA" => ("FLAGSAA", "is not an Enterprise COBOL option and has no effect", true),
        "NOFDUMP" => ("NOFDUMP", "is not an Enterprise COBOL option and has no effect", true),
        _ => return None,
    };
    Some(OptionError::NoEffect { option, why, warning })
}

impl std::error::Error for OptionError {}

impl Options {
    /// Applies one IBM compiler option as a CBL or PROCESS card or PARM writes it, e.g.
    /// `TRUNC(OPT)`, `AR(E)`, `CP(1047)`. Returns false for an option this layer does not read. An
    /// error leaves the options as they were, except NUMPROC(MIG), which sets the default NUMPROC.
    pub fn apply(&mut self, option: &str) -> Result<bool, OptionError> {
        let option = option.trim().to_ascii_uppercase();
        let (name, sub) = match option.split_once('(') {
            Some((name, rest)) => (name.trim(), rest.trim_end_matches(')').trim()),
            None => (option.as_str(), ""),
        };
        let bad = || OptionError::BadSuboption { option: name.to_owned(), given: sub.to_owned() };
        if let Some(e) = without_effect(name) {
            return Err(e);
        }
        let Some((documented, off)) = spelled(name) else { return Ok(false) };
        match documented.name {
            "ARITH" => {
                self.arith = match sub {
                    "COMPAT" | "C" => Arith::Compat,
                    "EXTEND" | "E" => Arith::Extend,
                    _ => return Err(bad()),
                }
            }
            "TRUNC" => {
                self.trunc = match sub {
                    "STD" => Trunc::Std,
                    "OPT" => Trunc::Opt,
                    "BIN" => Trunc::Bin,
                    _ => return Err(bad()),
                }
            }
            "NUMPROC" => {
                self.numproc = match sub {
                    "NOPFD" => Numproc::Nopfd,
                    "PFD" => Numproc::Pfd,
                    "MIG" => {
                        self.numproc = Numproc::default();
                        return Err(OptionError::Removed { option: "NUMPROC(MIG)".into(), since: "V5", instead: "NUMPROC(NOPFD)" });
                    }
                    _ => return Err(bad()),
                }
            }
            "CODEPAGE" => {
                let ccsid: u16 = sub.parse().map_err(|_| bad())?;
                CodePage::by_ccsid(ccsid).ok_or(OptionError::UnsupportedCodePage(ccsid))?;
                self.codepage = ccsid;
            }
            "COMPILE" => {
                self.compile = Some(match (off, sub) {
                    (false, "") => Compile::Full,
                    (true, "") => Compile::SyntaxOnly,
                    (true, "W") => Compile::Until(Stop::W),
                    (true, "E") => Compile::Until(Stop::E),
                    (true, "S") => Compile::Until(Stop::S),
                    _ => return Err(bad()),
                })
            }
            "FASTSRT" => self.fastsrt = !off,
            "ADV" => self.adv = !off,
            "THREAD" => self.thread = !off,
            "DLL" => self.dll = !off,
            "RENT" => self.rent = !off,
            "DBCS" => self.dbcs = !off,
            "DYNAM" => self.dynam = !off,
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Applies a flag of this compiler's own command line.
    pub fn apply_flag(&mut self, flag: &str) -> Result<(), OptionError> {
        match flag {
            "-silent" => self.trunc_check = TruncCheck::Silent,
            "-strict-sort-keys" => self.sort_keys = SortKeys::Strict,
            "--fastsrt-adv-print=exclude" => self.fastsrt_adv_print = FastsrtAdvPrint::Exclude,
            "--fastsrt-adv-print=include" => self.fastsrt_adv_print = FastsrtAdvPrint::Include,
            "-warnings-block" => self.warnings = Warnings::Block,
            "-debug" => self.debug = true,
            "--cics-return-warning=once" => self.cics_return_warning = CicsReturnWarning::Once,
            "--cics-return-warning=always" => self.cics_return_warning = CicsReturnWarning::Always,
            "--cics-return-warning=never" => self.cics_return_warning = CicsReturnWarning::Never,
            _ => return Err(OptionError::UnknownFlag(flag.to_owned())),
        }
        Ok(())
    }

    /// The COMPILE option in force: a card's, which outranks the command line as a PROCESS
    /// statement outranks the compiler's invocation (Programming Guide SC27-8714-03, p. 273), else
    /// NOCOMPILE(W) under `-warnings-block`, else IBM's default NOCOMPILE(S).
    pub fn object_code(&self) -> Compile {
        self.compile.unwrap_or(match self.warnings {
            Warnings::Block => Compile::Until(Stop::W),
            Warnings::Proceed => Compile::default(),
        })
    }

    pub fn code_page(&self) -> &'static CodePage {
        CodePage::by_ccsid(self.codepage).expect("codepage validated when applied")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_ibms() {
        let o = Options::default();
        assert_eq!((o.arith, o.trunc, o.numproc, o.codepage, o.fastsrt, o.adv), (Arith::Compat, Trunc::Std, Numproc::Nopfd, 1140, false, true));
        assert_eq!((o.thread, o.dll, o.rent, o.dbcs, o.dynam), (false, false, true, true, false));
    }

    #[test]
    fn dynam_and_its_abbreviations() {
        let mut o = Options::default();
        assert_eq!(o.apply("DYN"), Ok(true));
        assert!(o.dynam);
        assert_eq!(o.apply("NODYNAM"), Ok(true));
        assert!(!o.dynam);
    }

    #[test]
    fn adv_and_noadv_have_no_abbreviations() {
        let mut o = Options::default();
        assert_eq!(o.apply("noadv"), Ok(true));
        assert!(!o.adv);
        assert_eq!(o.apply("ADV"), Ok(true));
        assert!(o.adv);
        assert_eq!(o.apply("NOAD"), Ok(false));
    }

    #[test]
    fn thread_dll_rent_and_dbcs_and_their_negatives() {
        let mut o = Options::default();
        for option in ["thread", "DLL", "NORENT", "NODBCS"] {
            assert_eq!(o.apply(option), Ok(true));
        }
        assert_eq!((o.thread, o.dll, o.rent, o.dbcs), (true, true, false, false));
        for option in ["NOTHREAD", "NODLL", "RENT", "DBCS"] {
            assert_eq!(o.apply(option), Ok(true));
        }
        assert_eq!((o.thread, o.dll, o.rent, o.dbcs), (false, false, true, true));
    }

    #[test]
    fn abbreviations_and_case() {
        let mut o = Options::default();
        assert_eq!(o.apply("ar(e)"), Ok(true));
        assert_eq!(o.apply("CP(1047)"), Ok(true));
        assert_eq!(o.apply("TRUNC(BIN)"), Ok(true));
        assert_eq!(o.apply("fsrt"), Ok(true));
        assert_eq!((o.arith, o.codepage, o.trunc, o.fastsrt), (Arith::Extend, 1047, Trunc::Bin, true));
        assert_eq!(o.apply("NOFASTSRT"), Ok(true));
        assert!(!o.fastsrt);
    }

    #[test]
    fn ibms_table_is_whole_and_no_spelling_names_two_options() {
        let all: Vec<Documented> = documented().collect();
        assert_eq!(all.len(), 85, "Table 45 lists 85 options");
        let mut seen = std::collections::HashMap::new();
        for o in &all {
            for s in o.spellings() {
                assert!(seen.insert(s, o.name).is_none(), "{s} names {} and {}", seen[s], o.name);
            }
        }
        assert!(TABLE.lines().nth(1).is_some_and(|l| l.contains("cobolwork's provenance/enterprise-options.json")));
        let adata = all.iter().find(|o| o.name == "ADATA").unwrap();
        assert!(!adata.process && adata.page == 345);
    }

    #[test]
    fn every_spelling_ibm_documents_for_an_option_read_here_is_read() {
        let suboption = |name| match name {
            "ARITH" => "(E)",
            "CODEPAGE" => "(1047)",
            "TRUNC" => "(OPT)",
            "NUMPROC" => "(PFD)",
            _ => "",
        };
        for name in ["ARITH", "CODEPAGE", "TRUNC", "NUMPROC", "FASTSRT", "COMPILE"] {
            let o = documented().find(|o| o.name == name).unwrap();
            for s in o.spellings() {
                assert_eq!(Options::default().apply(&format!("{s}{}", suboption(name))), Ok(true), "{s}");
            }
        }
        let mut o = Options::default();
        o.apply("FSRT").unwrap();
        o.apply("NOFSRT").unwrap();
        assert!(!o.fastsrt);
        assert_eq!(switch("ssr(zlen)", "SSRANGE"), Some(true));
        assert_eq!(switch("SSRANGE(NOZLEN,MSG)", "SSRANGE"), Some(true));
        assert_eq!(switch("NOSSR", "SSRANGE"), Some(false));
        assert_eq!(switch("TRUNC(OPT)", "SSRANGE"), None);
    }

    /// Run with IRONWORK_COBOLWORK_DIR naming a cobolwork checkout to check the vendored copy.
    #[test]
    fn the_vendored_table_is_cobolworks() {
        let Ok(dir) = std::env::var("IRONWORK_COBOLWORK_DIR") else { return };
        let theirs = std::fs::read_to_string(std::path::Path::new(&dir).join("provenance/enterprise-options.tsv")).expect("cobolwork's table");
        assert!(theirs == TABLE, "crates/numeric/data/enterprise-options.tsv differs from cobolwork's: run tools/sync-option-table.sh");
    }

    #[test]
    fn options_this_layer_does_not_read_pass_through() {
        assert_eq!(Options::default().apply("SSRANGE"), Ok(false));
    }

    #[test]
    fn an_option_that_cannot_be_applied_says_why_and_leaves_the_options_as_they_were() {
        let mut o = Options::default();
        o.apply("NUMPROC(PFD)").unwrap();
        o.apply("TRUNC(BIN)").unwrap();
        assert!(matches!(o.apply("TRUNC(FAST)"), Err(OptionError::BadSuboption { .. })));
        assert_eq!(o.apply("CODEPAGE(930)"), Err(OptionError::UnsupportedCodePage(930)));
        assert!(matches!(o.apply("CP(X)"), Err(OptionError::BadSuboption { .. })));
        for (option, warns) in [("LIB", false), ("SIZE(MAX)", false), ("sz(2097152)", false), ("FLAGSAA", true), ("NOFDUMP", true)] {
            assert!(matches!(o.apply(option), Err(OptionError::NoEffect { warning, .. }) if warning == warns), "{option}");
        }
        assert_eq!((o.apply("NOLIB"), o.apply("FDUMP")), (Ok(false), Ok(false)));
        assert_eq!((o.numproc, o.trunc, o.codepage), (Numproc::Pfd, Trunc::Bin, 1140));
        let removed = o.apply("NUMPROC(MIG)").unwrap_err();
        assert_eq!(removed.to_string(), "NUMPROC(MIG) was removed in Enterprise COBOL V5, so NUMPROC(NOPFD) is in effect");
        assert_eq!(o.numproc, Numproc::Nopfd, "the default NUMPROC, not the one before");
    }

    #[test]
    fn silent_flag_turns_off_trunc_reports() {
        let mut o = Options::default();
        o.apply_flag("-silent").unwrap();
        assert_eq!(o.trunc_check, TruncCheck::Silent);
        o.apply_flag("-strict-sort-keys").unwrap();
        assert_eq!(o.sort_keys, SortKeys::Strict);
        assert!(o.apply_flag("-quiet").is_err());
    }

    #[test]
    fn warnings_proceed_unless_the_flag_blocks_them() {
        let mut o = Options::default();
        assert_eq!(o.warnings, Warnings::Proceed);
        o.apply_flag("-warnings-block").unwrap();
        assert_eq!(o.warnings, Warnings::Block);
        assert!(o.apply_flag("-Werror").is_err());
    }

    #[test]
    fn compile_and_nocompile_with_each_severity_and_their_abbreviations() {
        let given = |option: &str| {
            let mut o = Options::default();
            o.apply(option).map(|_| o.compile)
        };
        assert_eq!(Options::default().object_code(), Compile::Until(Stop::S));
        assert_eq!(given("COMPILE"), Ok(Some(Compile::Full)));
        assert_eq!(given("c"), Ok(Some(Compile::Full)));
        assert_eq!(given("NOCOMPILE"), Ok(Some(Compile::SyntaxOnly)));
        assert_eq!(given("NOC(w)"), Ok(Some(Compile::Until(Stop::W))));
        assert_eq!(given("NOCOMPILE(E)"), Ok(Some(Compile::Until(Stop::E))));
        assert_eq!(given("NOC(S)"), Ok(Some(Compile::Until(Stop::S))));
        for bad in ["NOCOMPILE(U)", "NOC(I)", "COMPILE(S)", "C(E)"] {
            assert!(matches!(given(bad), Err(OptionError::BadSuboption { .. })), "{bad}");
        }
        let codes = [Compile::Full, Compile::Until(Stop::S), Compile::Until(Stop::E), Compile::Until(Stop::W), Compile::SyntaxOnly].map(Compile::stops_at);
        assert_eq!(codes, [16, 12, 8, 4, 0]);
    }

    #[test]
    fn a_card_outranks_warnings_block() {
        let mut o = Options::default();
        o.apply_flag("-warnings-block").unwrap();
        assert_eq!(o.object_code(), Compile::Until(Stop::W));
        o.apply("NOCOMPILE(S)").unwrap();
        assert_eq!(o.object_code(), Compile::Until(Stop::S));
        o.apply("NOCOMPILE(E)").unwrap();
        assert_eq!(o.object_code(), Compile::Until(Stop::E), "the last card wins");
    }

    #[test]
    fn fastsrt_adv_print_excludes_unless_included() {
        let mut o = Options::default();
        assert_eq!(o.fastsrt_adv_print, FastsrtAdvPrint::Exclude);
        o.apply_flag("--fastsrt-adv-print=include").unwrap();
        assert_eq!(o.fastsrt_adv_print, FastsrtAdvPrint::Include);
        o.apply_flag("--fastsrt-adv-print=exclude").unwrap();
        assert_eq!(o.fastsrt_adv_print.flag(), "--fastsrt-adv-print=exclude");
        assert!(o.apply_flag("--fastsrt-adv-print=maybe").is_err());
        assert!(o.apply_flag("--fastsrt-adv-print").is_err());
    }

    #[test]
    fn cics_return_warning_is_once_unless_the_flag_says_always_or_never() {
        let mut o = Options::default();
        assert_eq!(o.cics_return_warning, CicsReturnWarning::Once);
        for mode in [CicsReturnWarning::Always, CicsReturnWarning::Never, CicsReturnWarning::Once] {
            o.apply_flag(mode.flag()).unwrap();
            assert_eq!(o.cics_return_warning, mode);
        }
        assert!(o.apply_flag("--cics-return-warning=sometimes").is_err());
        assert!(o.apply_flag("--cics-return-warning").is_err());
    }
}
