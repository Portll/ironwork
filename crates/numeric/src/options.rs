use std::fmt;
use zarch::ebcdic::CodePage;
use zarch::hfp::Precision;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    pub arith: Arith,
    pub trunc: Trunc,
    pub numproc: Numproc,
    pub codepage: u16,
    pub trunc_check: TruncCheck,
    /// FASTSRT: DFSORT does the I/O of a SORT's USING and GIVING files where IBM's rules allow.
    pub fastsrt: bool,
    pub sort_keys: SortKeys,
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
            sort_keys: SortKeys::default(),
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
        match name {
            "ARITH" | "AR" => {
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
            "CODEPAGE" | "CP" => {
                let ccsid: u16 = sub.parse().map_err(|_| bad())?;
                CodePage::by_ccsid(ccsid).ok_or(OptionError::UnsupportedCodePage(ccsid))?;
                self.codepage = ccsid;
            }
            "FASTSRT" | "FSRT" => self.fastsrt = true,
            "NOFASTSRT" | "NOFSRT" => self.fastsrt = false,
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Applies a flag of this compiler's own command line.
    pub fn apply_flag(&mut self, flag: &str) -> Result<(), OptionError> {
        match flag {
            "-silent" => self.trunc_check = TruncCheck::Silent,
            "-strict-sort-keys" => self.sort_keys = SortKeys::Strict,
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
        assert_eq!((o.arith, o.trunc, o.numproc, o.codepage, o.fastsrt), (Arith::Compat, Trunc::Std, Numproc::Nopfd, 1140, false));
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
}
