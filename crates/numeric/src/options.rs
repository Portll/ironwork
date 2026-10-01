use std::fmt;
use zarch::ebcdic::{self, CodePage};
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

/// INVDATA: zoned and packed items may hold invalid digits, sign codes or zone bits. FORCENUMCMP
/// compares zoned items as numbers whatever their zones; CLEANSIGN cleans a sign code on input to
/// a comparison or computation (Programming Guide SC27-8714-03, pp. 376-378). ZONEDATA(NOPFD) and
/// ZONEDATA(MIG) are INVDATA with FORCENUMCMP off and on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Invdata {
    pub forcenumcmp: bool,
    pub cleansign: bool,
}

impl Default for Invdata {
    fn default() -> Self {
        Self { forcenumcmp: false, cleansign: true }
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

/// What the figurative constant QUOTE is: a quotation mark under QUOTE, IBM's default, or an
/// apostrophe under APOST (Programming Guide SC27-8714-03, p. 347).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Quote {
    #[default]
    Quote,
    Apost,
}

impl Quote {
    /// The character in EBCDIC, where every single-byte page carries it at the same place.
    pub const fn byte(self) -> u8 {
        match self {
            Self::Quote => ebcdic::QUOTE,
            Self::Apost => ebcdic::APOSTROPHE,
        }
    }

    /// The character as a UTF-16 unit, for a national item.
    pub const fn unit(self) -> u16 {
        match self {
            Self::Quote => 0x0022,
            Self::Apost => 0x0027,
        }
    }
}

/// CURRENCY(literal): the character, or a hexadecimal literal's byte, which the program's code page
/// turns into one (Programming Guide SC27-8714-03, p. 358).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Currency {
    Char(char),
    Hex(u8),
}

/// Whether N literals, and PICTURE N items with no USAGE clause, are national, IBM's default, or
/// DBCS (Programming Guide SC27-8714-03, pp. 387-388).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Nsymbol {
    #[default]
    National,
    Dbcs,
}

/// How DISPLAY shows a signed binary, packed or overpunched zoned item: as releases before 6 did,
/// with an overpunched digit (`Compat`), or with a separate leading sign (`Sep`) (Programming
/// Guide SC27-8714-03, pp. 362-363).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DispSign {
    #[default]
    Compat,
    Sep,
}

/// Day 1 of the date intrinsic functions' integer dates: 1 January 1601 (`Ansi`), or Language
/// Environment's Lilian 15 October 1582 (Programming Guide SC27-8714-03, p. 375).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IntDate {
    #[default]
    Ansi,
    Lilian,
}

/// Whether a reference must be unique by the standard's rules (`Compat`), or resolves to the one
/// item a complete set of qualifiers names (`Extend`) (Programming Guide SC27-8714-03, p. 400).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Qualify {
    #[default]
    Compat,
    Extend,
}

/// What a READ of a variable-length record checks its length against: the level-01 records under
/// `Standard`, RECORD VARYING under `Compat`; outside them its status is 04, else 00 (Programming
/// Guide SC27-8714-03, pp. 422-424, Table 52). See assumptions C218 and C219.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Vlr {
    #[default]
    Standard,
    Compat,
}

/// The file status of a VSAM OPEN that succeeds once its file's integrity is verified: 97 under
/// `Compat`, 00 under `Succ` (Programming Guide SC27-8714-03, p. 424). No ironwork OPEN verifies
/// a file yet (assumption C220).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VsamOpenFs {
    #[default]
    Compat,
    Succ,
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
    /// INVDATA's suboptions; None for NOINVDATA, IBM's default, which assumes the data is valid.
    pub invdata: Option<Invdata>,
    /// ZWB: a signed zoned item compared with a nonnumeric operand loses its sign first.
    pub zwb: bool,
    pub quote: Quote,
    /// CURRENCY(literal): the currency symbol a program with no CURRENCY SIGN clause uses in place
    /// of $. None under NOCURRENCY, IBM's default.
    pub currency: Option<Currency>,
    pub nsymbol: Nsymbol,
    pub dispsign: DispSign,
    pub intdate: IntDate,
    pub qualify: Qualify,
    /// INITIAL: the program and its nested programs behave as though their PROGRAM-ID paragraphs
    /// said IS INITIAL (Programming Guide SC27-8714-03, p. 374). A compile with THREAD takes
    /// NOINITIAL (p. 344).
    pub initial: bool,
    pub vlr: Vlr,
    pub vsamopenfs: VsamOpenFs,
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
            invdata: None,
            zwb: true,
            quote: Quote::default(),
            currency: None,
            nsymbol: Nsymbol::default(),
            dispsign: DispSign::default(),
            intdate: IntDate::default(),
            qualify: Qualify::default(),
            initial: false,
            vlr: Vlr::default(),
            vsamopenfs: VsamOpenFs::default(),
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

/// CURRENCY's literal: one character between quotation marks or apostrophes, or a hexadecimal
/// literal of one byte (Programming Guide SC27-8714-03, p. 358). A figurative constant, a
/// null-terminated, DBCS or national literal is none of these, so it is refused here.
fn currency_literal(text: &str) -> Option<Currency> {
    fn quoted(t: &str) -> Option<&str> {
        let q = t.chars().next().filter(|q| matches!(q, '\'' | '"'))?;
        (t.len() >= 2 && t.ends_with(q)).then(|| &t[1..t.len() - 1])
    }
    let text = text.trim();
    if let Some(hex) = text.strip_prefix(['X', 'x']).and_then(quoted) {
        return u8::from_str_radix(hex, 16).ok().filter(|_| hex.len() == 2).map(Currency::Hex);
    }
    let mut chars = quoted(text)?.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if can_be_currency(c) => Some(Currency::Char(c)),
        _ => None,
    }
}

/// Whether the CURRENCY option may name `c`: a single-byte character that is no digit, space, one
/// of the letters A B C D E G N P R S U V X Z in either case, or one of * + - / , . ; ( ) " =.
pub fn can_be_currency(c: char) -> bool {
    u32::from(c) < 256 && !c.is_ascii_digit() && !"ABCDEGNPRSUVXZabcdegnprsuvxz *+-/,.;()\"=".contains(c)
}

impl std::error::Error for OptionError {}

impl Options {
    /// Applies one IBM compiler option as a CBL or PROCESS card or PARM writes it, e.g.
    /// `TRUNC(OPT)`, `AR(E)`, `CP(1047)`. Returns false for an option this layer does not read. An
    /// error leaves the options as they were, except NUMPROC(MIG), which sets the default NUMPROC.
    pub fn apply(&mut self, option: &str) -> Result<bool, OptionError> {
        let given = option.trim();
        let option = given.to_ascii_uppercase();
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
            "INVDATA" if off => self.invdata = None,
            "INVDATA" => {
                let mut invdata = Invdata::default();
                for part in sub.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                    match part {
                        "FORCENUMCMP" | "FNC" => invdata.forcenumcmp = true,
                        "NOFORCENUMCMP" | "NOFNC" => invdata.forcenumcmp = false,
                        "CLEANSIGN" | "CS" => invdata.cleansign = true,
                        "NOCLEANSIGN" | "NOCS" => invdata.cleansign = false,
                        _ => return Err(bad()),
                    }
                }
                self.invdata = Some(invdata);
            }
            "ZONEDATA" => {
                self.invdata = match sub {
                    "PFD" => None,
                    "NOPFD" => Some(Invdata::default()),
                    "MIG" => Some(Invdata { forcenumcmp: true, cleansign: true }),
                    _ => return Err(bad()),
                }
            }
            "ZWB" => self.zwb = !off,
            "FASTSRT" => self.fastsrt = !off,
            "ADV" => self.adv = !off,
            "THREAD" => self.thread = !off,
            "DLL" => self.dll = !off,
            "RENT" => self.rent = !off,
            "DBCS" => self.dbcs = !off,
            "DYNAM" => self.dynam = !off,
            "APOST/QUOTE" if sub.is_empty() => self.quote = if name == "APOST" { Quote::Apost } else { Quote::Quote },
            "CURRENCY" => {
                self.currency = match (off, sub) {
                    (true, "") => None,
                    (false, _) => Some(currency_literal(given.split_once('(').map_or("", |(_, rest)| rest.trim_end().trim_end_matches(')'))).ok_or_else(bad)?),
                    _ => return Err(bad()),
                }
            }
            "NSYMBOL" => {
                self.nsymbol = match sub {
                    "NATIONAL" | "NAT" => Nsymbol::National,
                    "DBCS" => Nsymbol::Dbcs,
                    _ => return Err(bad()),
                }
            }
            "DISPSIGN" => {
                self.dispsign = match sub {
                    "COMPAT" | "C" => DispSign::Compat,
                    "SEP" | "S" => DispSign::Sep,
                    _ => return Err(bad()),
                }
            }
            "INTDATE" => {
                self.intdate = match sub {
                    "ANSI" => IntDate::Ansi,
                    "LILIAN" => IntDate::Lilian,
                    _ => return Err(bad()),
                }
            }
            "QUALIFY" => {
                self.qualify = match sub {
                    "COMPAT" | "C" => Qualify::Compat,
                    "EXTEND" | "E" => Qualify::Extend,
                    _ => return Err(bad()),
                }
            }
            "INITIAL" if sub.is_empty() => self.initial = !off,
            "VLR" => {
                self.vlr = match sub {
                    "STANDARD" | "S" => Vlr::Standard,
                    "COMPAT" | "C" => Vlr::Compat,
                    _ => return Err(bad()),
                }
            }
            "VSAMOPENFS" => {
                self.vsamopenfs = match sub {
                    "COMPAT" | "C" => VsamOpenFs::Compat,
                    "SUCC" | "S" => VsamOpenFs::Succ,
                    _ => return Err(bad()),
                }
            }
            "APOST/QUOTE" | "INITIAL" => return Err(bad()),
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

    /// The CURRENCY option's character, a hexadecimal literal's read in the program's code page;
    /// Err with that character when the page gives one the option may not name.
    pub fn currency_symbol(&self) -> Option<Result<char, char>> {
        self.currency.map(|c| match c {
            Currency::Char(c) => Ok(c),
            Currency::Hex(b) => {
                let c = self.code_page().decode_byte(b);
                if can_be_currency(c) { Ok(c) } else { Err(c) }
            }
        })
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
    fn invdata_zonedata_and_zwb() {
        let mut o = Options::default();
        assert_eq!((o.invdata, o.zwb), (None, true));
        assert_eq!(o.apply("INVDATA"), Ok(true));
        assert_eq!(o.invdata, Some(Invdata { forcenumcmp: false, cleansign: true }));
        assert_eq!(o.apply("INVD(FNC,NOCS)"), Ok(true));
        assert_eq!(o.invdata, Some(Invdata { forcenumcmp: true, cleansign: false }));
        assert_eq!(o.apply("NOINVDATA"), Ok(true));
        assert_eq!(o.invdata, None);
        assert_eq!(o.apply("ZD(MIG)"), Ok(true));
        assert_eq!(o.invdata, Some(Invdata { forcenumcmp: true, cleansign: true }));
        assert_eq!(o.apply("ZONEDATA(NOPFD)"), Ok(true));
        assert_eq!(o.invdata, Some(Invdata::default()));
        assert_eq!(o.apply("ZONEDATA(PFD)"), Ok(true));
        assert_eq!(o.invdata, None);
        assert!(o.apply("INVDATA(SOMETIMES)").is_err());
        assert_eq!(o.apply("NOZWB"), Ok(true));
        assert!(!o.zwb);
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
            "ZONEDATA" => "(MIG)",
            _ => "",
        };
        for name in ["ARITH", "CODEPAGE", "TRUNC", "NUMPROC", "FASTSRT", "COMPILE", "INVDATA", "ZONEDATA", "ZWB"] {
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
    fn the_options_of_roadmap_2_10_to_2_12_default_to_ibms_and_take_each_documented_suboption() {
        let o = Options::default();
        assert_eq!((o.quote, o.currency, o.nsymbol, o.dispsign, o.intdate), (Quote::Quote, None, Nsymbol::National, DispSign::Compat, IntDate::Ansi));
        assert_eq!((o.qualify, o.initial, o.vlr, o.vsamopenfs), (Qualify::Compat, false, Vlr::Standard, VsamOpenFs::Compat));
        let given = |cards: &[&str]| {
            let mut o = Options::default();
            cards.iter().for_each(|c| assert_eq!(o.apply(c), Ok(true), "{c}"));
            o
        };
        assert_eq!(given(&["APOST"]).quote, Quote::Apost);
        assert_eq!(given(&["apost", "Q"]).quote, Quote::Quote);
        assert_eq!(given(&["NS(DBCS)"]).nsymbol, Nsymbol::Dbcs);
        assert_eq!(given(&["NSYMBOL(DBCS)", "NS(NAT)"]).nsymbol, Nsymbol::National);
        assert_eq!(given(&["DS(S)"]).dispsign, DispSign::Sep);
        assert_eq!(given(&["DISPSIGN(SEP)", "DISPSIGN(COMPAT)"]).dispsign, DispSign::Compat);
        assert_eq!(given(&["INTDATE(LILIAN)"]).intdate, IntDate::Lilian);
        assert_eq!(given(&["QUA(E)"]).qualify, Qualify::Extend);
        assert_eq!(given(&["QUALIFY(EXTEND)", "QUA(C)"]).qualify, Qualify::Compat);
        assert!(given(&["INITIAL"]).initial);
        assert!(!given(&["INITIAL", "NOINITIAL"]).initial);
        assert_eq!(given(&["VLR(C)"]).vlr, Vlr::Compat);
        assert_eq!(given(&["VLR(COMPAT)", "VLR(STANDARD)"]).vlr, Vlr::Standard);
        assert_eq!(given(&["VS(S)"]).vsamopenfs, VsamOpenFs::Succ);
        assert_eq!(given(&["VSAMOPENFS(SUCC)", "VSAMOPENFS(COMPAT)"]).vsamopenfs, VsamOpenFs::Compat);
        for bad in ["APOST(X)", "NSYMBOL(N)", "DS(X)", "INTDATE(JULIAN)", "QUA(X)", "INITIAL(Y)", "VLR(X)", "VS(X)", "INTDATE"] {
            assert!(matches!(Options::default().apply(bad), Err(OptionError::BadSuboption { .. })), "{bad}");
        }
    }

    #[test]
    fn currency_takes_one_character_it_may_name_in_either_delimiter_or_a_hexadecimal_byte() {
        let currency = |card: &str| {
            let mut o = Options::default();
            o.apply(card).map(|_| o.currency)
        };
        assert_eq!(currency("CURRENCY('£')"), Ok(Some(Currency::Char('£'))));
        assert_eq!(currency("curr(\"f\")"), Ok(Some(Currency::Char('f'))), "the literal keeps its case");
        assert_eq!(currency("CURRENCY(X'5B')"), Ok(Some(Currency::Hex(0x5B))));
        assert_eq!(currency("NOCURR"), Ok(None));
        for bad in ["CURRENCY('E')", "CURRENCY('e')", "CURRENCY('1')", "CURRENCY(' ')", "CURRENCY('*')", "CURRENCY('EUR')", "CURRENCY(SPACE)", "CURRENCY(N'£')", "CURRENCY(Z'£')", "CURRENCY(X'5B5B')", "CURRENCY", "NOCURRENCY('£')"] {
            assert!(matches!(currency(bad), Err(OptionError::BadSuboption { .. })), "{bad}");
        }
        let mut o = Options::default();
        o.apply("CURRENCY(X'4A')").unwrap();
        assert_eq!(o.currency_symbol(), Some(Ok('¢')), "X'4A' is the cent sign in CCSID 1140");
        o.apply("CURRENCY(X'F1')").unwrap();
        assert_eq!(o.currency_symbol(), Some(Err('1')));
        assert_eq!(Options::default().currency_symbol(), None);
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
