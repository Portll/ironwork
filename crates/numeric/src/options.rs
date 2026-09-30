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
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OptionError {
    BadSuboption { option: String, given: String },
    Removed { option: String, since: &'static str },
    UnsupportedCodePage(u16),
    UnknownFlag(String),
}

impl fmt::Display for OptionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadSuboption { option, given } => write!(f, "{option} does not take ({given})"),
            Self::Removed { option, since } => write!(f, "{option} was removed in Enterprise COBOL {since}"),
            Self::UnsupportedCodePage(ccsid) => write!(f, "CODEPAGE({ccsid}) is not a single-byte EBCDIC page this compiler carries"),
            Self::UnknownFlag(flag) => write!(f, "unknown flag {flag}"),
        }
    }
}

impl std::error::Error for OptionError {}

impl Options {
    /// Applies one IBM compiler option as a CBL or PROCESS card or PARM writes it, e.g.
    /// `TRUNC(OPT)`, `AR(E)`, `CP(1047)`. Returns false for an option this layer does not read.
    pub fn apply(&mut self, option: &str) -> Result<bool, OptionError> {
        let option = option.trim().to_ascii_uppercase();
        let (name, sub) = match option.split_once('(') {
            Some((name, rest)) => (name.trim(), rest.trim_end_matches(')').trim()),
            None => (option.as_str(), ""),
        };
        let bad = || OptionError::BadSuboption { option: name.to_owned(), given: sub.to_owned() };
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
                    "MIG" => return Err(OptionError::Removed { option: "NUMPROC(MIG)".into(), since: "V5" }),
                    _ => return Err(bad()),
                }
            }
            "CODEPAGE" => {
                let ccsid: u16 = sub.parse().map_err(|_| bad())?;
                CodePage::by_ccsid(ccsid).ok_or(OptionError::UnsupportedCodePage(ccsid))?;
                self.codepage = ccsid;
            }
            "FASTSRT" => self.fastsrt = !off,
            "ADV" => self.adv = !off,
            "THREAD" => self.thread = !off,
            "DLL" => self.dll = !off,
            "RENT" => self.rent = !off,
            "DBCS" => self.dbcs = !off,
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
            _ => return Err(OptionError::UnknownFlag(flag.to_owned())),
        }
        Ok(())
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
        assert_eq!((o.thread, o.dll, o.rent, o.dbcs), (false, false, true, true));
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
        for name in ["ARITH", "CODEPAGE", "TRUNC", "NUMPROC", "FASTSRT"] {
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
    fn refusals_name_the_problem() {
        let mut o = Options::default();
        assert!(matches!(o.apply("NUMPROC(MIG)"), Err(OptionError::Removed { .. })));
        assert!(matches!(o.apply("TRUNC(FAST)"), Err(OptionError::BadSuboption { .. })));
        assert_eq!(o.apply("CODEPAGE(930)"), Err(OptionError::UnsupportedCodePage(930)));
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
}
