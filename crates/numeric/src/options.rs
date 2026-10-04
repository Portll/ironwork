use crate::assumptions;
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

/// NUMCHECK: implicit numeric class tests of zoned and packed senders, and size tests of binary
/// senders, each a warning that lets the statement run (MSG) or a terminating message (`abd`)
/// (Programming Guide SC27-8714-03, pp. 388-392). ZONECHECK(MSG|ABD) is NUMCHECK(ZON,MSG|ABD)
/// (p. 427).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Numcheck {
    pub zon: Option<ZonCheck>,
    pub pac: bool,
    pub bin: Option<BinCheck>,
    pub abd: bool,
}

impl Default for Numcheck {
    fn default() -> Self {
        Self { zon: Some(ZonCheck::default()), pac: true, bin: Some(BinCheck { truncbin: true }), abd: false }
    }
}

/// ZON's suboptions: whether a zoned item compared with an alphanumeric operand is checked
/// (ALPHNUM, the default), and whether the redefinitions p. 390 lists are tolerated (LAX).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZonCheck {
    pub alphnum: bool,
    pub lax: bool,
}

impl Default for ZonCheck {
    fn default() -> Self {
        Self { alphnum: true, lax: false }
    }
}

/// BIN's suboption: whether binary senders are checked under TRUNC(BIN) too (TRUNCBIN, the default).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinCheck {
    pub truncbin: bool,
}

/// PARMCHECK: a buffer of `bytes` after the last WORKING-STORAGE item, set to X'AA' before each
/// CALL and checked after it, a change being a warning (MSG) or a terminating message (`abd`)
/// (Programming Guide SC27-8714-03, p. 397).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Parmcheck {
    pub abd: bool,
    pub bytes: u16,
}

/// INITCHECK: a compile-time warning for a WORKING-STORAGE or LOCAL-STORAGE item used before it is
/// set on some path (`Lax`, the default) or on any path (`Strict`) (Programming Guide
/// SC27-8714-03, pp. 373-374).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Initcheck {
    #[default]
    Lax,
    Strict,
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
/// `Compat`, 00 under `Succ` (Programming Guide SC27-8714-03, p. 424). OPEN verifies a data set a
/// run left open for output (assumption C220).
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

/// Whose result a computation gives where ironwork knowingly differs from GnuCOBOL: Enterprise
/// COBOL's as the assumptions register reads it (`Ibm`), or that of GnuCOBOL's
/// `cobc -std=ibm-strict` (`Gnucobol`, `--dialect gnucobol`), so a migration can compare ironwork
/// with a GnuCOBOL build. docs/dialect.md lists each difference.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Dialect {
    #[default]
    Ibm,
    Gnucobol,
}

impl Dialect {
    pub const fn flag(self) -> &'static str {
        match self {
            Self::Ibm => "--dialect=ibm",
            Self::Gnucobol => "--dialect=gnucobol",
        }
    }

    /// The value `--dialect` takes for it.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ibm => "ibm",
            Self::Gnucobol => "gnucobol",
        }
    }

    pub fn named(value: &str) -> Option<Self> {
        [Self::Ibm, Self::Gnucobol].into_iter().find(|d| d.name() == value)
    }
}

/// A chosen assumption `--assume ID=VALUE` switches, by its place in [`SWITCHES`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Switched {
    RoundedExtraPlace,
    DisplayOfNondisplayNumeric,
    DecimalCommaDisplayLiteral,
    AcceptAtEnd,
    EntryCalls,
    ExternalStorage,
    OptimizedZonesCompared,
}

const IBM_OR_GNUCOBOL: &[&str] = &["ibm", "gnucobol"];

/// Each switched assumption's register id and the values `--assume` takes for it: `ibm`, the result
/// the register states, then `gnucobol`, cobc's, which `--dialect gnucobol` gives, then any other
/// alternative the claim names (docs/dialect.md).
pub const SWITCHES: [(&str, &[&str]); 7] = [
    (assumptions::ROUNDED_EXTRA_PLACE, &["ibm", "gnucobol", "off"]),
    (assumptions::DISPLAY_OF_NONDISPLAY_NUMERIC, IBM_OR_GNUCOBOL),
    (assumptions::DECIMAL_COMMA_DISPLAY_LITERAL, IBM_OR_GNUCOBOL),
    (assumptions::ACCEPT_AT_END, IBM_OR_GNUCOBOL),
    (assumptions::ENTRY_CALLS, IBM_OR_GNUCOBOL),
    (assumptions::EXTERNAL_STORAGE, IBM_OR_GNUCOBOL),
    (assumptions::OPTIMIZED_ZONES_COMPARED, IBM_OR_GNUCOBOL),
];

/// Where a ROUNDED receiver's extra decimal place counts (assumption C101): in every operation of
/// the statement (`ibm`), in its last alone (`gnucobol`), or in none (`off`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtraPlace {
    Every,
    Last,
    Off,
}

/// The values `--assume` gave, by place in [`SWITCHES`]: 0 where none did and the dialect decides,
/// else one more than the value's place in the switch's list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Assumed {
    pub given: [u8; SWITCHES.len()],
}

impl Assumed {
    /// Reads `ID=VALUE` as a switch's place and the value's mark, refusing by name an id the
    /// register lacks, an assumption with no alternative, and a value the switch does not take.
    pub fn parse(spec: &str) -> Result<(usize, u8), String> {
        let Some((id, value)) = spec.split_once('=') else {
            return Err(format!("--assume {spec}: needs ID=VALUE, such as C101=off"));
        };
        let Some(i) = SWITCHES.iter().position(|(s, _)| *s == id) else {
            let switched = listed(&SWITCHES.map(|(s, _)| s), "and");
            return Err(match assumptions::ASSUMPTIONS.iter().find(|a| a.id == id) {
                Some(a) => format!("--assume {spec}: assumption {id} ({}) has no alternative; --assume switches {switched}", a.basis.name()),
                None => format!("--assume {spec}: the register has no assumption {id}; ironwork assumptions lists them"),
            });
        };
        match SWITCHES[i].1.iter().position(|v| *v == value) {
            Some(k) => Ok((i, k as u8 + 1)),
            None => Err(format!("--assume {spec}: {id} takes {}", listed(SWITCHES[i].1, "or"))),
        }
    }
}

/// `a, b and c`, or with `or`.
fn listed(words: &[&str], last: &str) -> String {
    match words {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [rest @ .., end] => format!("{} {last} {end}", rest.join(", ")),
    }
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

/// Whether the compile refuses what Enterprise COBOL refuses (`Strict`), or accepts, each with a
/// warning, the other dialects' extensions docs/compliance.md lists (`Extended`,
/// `--compliance extended`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Compliance {
    #[default]
    Strict,
    Extended,
}

impl Compliance {
    pub const fn flag(self) -> &'static str {
        match self {
            Self::Strict => "--compliance=strict",
            Self::Extended => "--compliance=extended",
        }
    }

    /// The value `--compliance` takes for it.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::Extended => "extended",
        }
    }

    pub fn named(value: &str) -> Option<Self> {
        [Self::Strict, Self::Extended].into_iter().find(|c| c.name() == value)
    }

    /// The level the last `--compliance=` flag among `flags` gives, strict without one.
    pub fn of(flags: &[String]) -> Self {
        flags.iter().rev().find_map(|f| f.strip_prefix("--compliance=").and_then(Self::named)).unwrap_or_default()
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
    pub numcheck: Option<Numcheck>,
    pub parmcheck: Option<Parmcheck>,
    pub initcheck: Option<Initcheck>,
    /// OPTIMIZE's level, 0 to 2.
    pub optimize: u8,
    pub compliance: Compliance,
    pub dialect: Dialect,
    pub assumed: Assumed,
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
            numcheck: None,
            parmcheck: None,
            initcheck: None,
            optimize: 0,
            compliance: Compliance::default(),
            dialect: Dialect::default(),
            assumed: Assumed::default(),
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
            Self::UnsupportedCodePage(ccsid) => write!(f, "CODEPAGE({ccsid}) is not an EBCDIC page this compiler carries"),
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

/// The text between an option's first opening parenthesis and its last closing one.
fn inner(option: &str) -> &str {
    option.split_once('(').map_or("", |(_, rest)| rest.trim_end().strip_suffix(')').unwrap_or(rest)).trim()
}

/// `text` split at the commas outside parentheses.
fn top_level(text: &str) -> Vec<&str> {
    let (mut parts, mut depth, mut start) = (Vec::new(), 0i32, 0);
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(text[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(text[start..].trim());
    parts.into_iter().filter(|p| !p.is_empty()).collect()
}

/// NUMCHECK's suboptions with IBM's defaults: none gives every data type and MSG; data types left
/// out are on when none is given and off when any is; every data type off is NONUMCHECK, None
/// (Programming Guide SC27-8714-03, p. 389). None inside Some is a suboption IBM does not have.
fn numcheck(text: &str) -> Option<Option<Numcheck>> {
    let (mut zon, mut pac, mut bin, mut abd, mut typed) = (None, None, None, false, false);
    for part in top_level(text) {
        let (word, args) = match part.split_once('(') {
            Some((w, _)) => (w.trim(), top_level(inner(part))),
            None => (part, Vec::new()),
        };
        match word {
            "ZON" => {
                let mut z = ZonCheck::default();
                for a in args {
                    match a {
                        "ALPHNUM" => z.alphnum = true,
                        "NOALPHNUM" => z.alphnum = false,
                        "LAX" | "LAXREDEF" => z.lax = true,
                        "STRICT" | "STRICTREDEF" => z.lax = false,
                        _ => return None,
                    }
                }
                zon = Some(Some(z));
            }
            "NOZON" if args.is_empty() => zon = Some(None),
            "PAC" if args.is_empty() => pac = Some(true),
            "NOPAC" if args.is_empty() => pac = Some(false),
            "BIN" => {
                bin = Some(Some(BinCheck {
                    truncbin: match args[..] {
                        [] | ["TRUNCBIN"] => true,
                        ["NOTRUNCBIN"] => false,
                        _ => return None,
                    },
                }))
            }
            "NOBIN" if args.is_empty() => bin = Some(None),
            "MSG" if args.is_empty() => abd = false,
            "ABD" if args.is_empty() => abd = true,
            _ => return None,
        }
        typed |= matches!(word, "ZON" | "NOZON" | "PAC" | "NOPAC" | "BIN" | "NOBIN");
    }
    let all = Numcheck::default();
    let n = if typed {
        Numcheck { zon: zon.flatten(), pac: pac.unwrap_or(false), bin: bin.flatten(), abd }
    } else {
        Numcheck { abd, ..all }
    };
    Some((n.zon.is_some() || n.pac || n.bin.is_some()).then_some(n))
}

/// PARMCHECK's MSG or ABD and buffer size, 100 bytes and MSG when left out (Programming Guide
/// SC27-8714-03, p. 397).
fn parmcheck(sub: &str) -> Option<Parmcheck> {
    let mut p = Parmcheck { abd: false, bytes: 100 };
    let parts: Vec<&str> = sub.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
    if parts.len() > 2 {
        return None;
    }
    for (i, part) in parts.iter().enumerate() {
        match *part {
            "MSG" if i == 0 => p.abd = false,
            "ABD" if i == 0 => p.abd = true,
            n => p.bytes = n.parse().ok().filter(|b| (1..=9999).contains(b))?,
        }
    }
    Some(p)
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
            "NUMCHECK" if off => self.numcheck = None,
            "NUMCHECK" => self.numcheck = numcheck(inner(&option)).ok_or_else(bad)?,
            "ZONECHECK" if off => self.numcheck = self.numcheck.map(|n| Numcheck { zon: None, ..n }).filter(|n| n.pac || n.bin.is_some()),
            "ZONECHECK" => {
                let abd = match sub {
                    "MSG" => false,
                    "ABD" => true,
                    _ => return Err(bad()),
                };
                self.numcheck = Some(Numcheck { zon: Some(ZonCheck::default()), pac: false, bin: None, abd });
            }
            "PARMCHECK" if off => self.parmcheck = None,
            "PARMCHECK" => self.parmcheck = Some(parmcheck(sub).ok_or_else(bad)?),
            "INITCHECK" if off => self.initcheck = None,
            "INITCHECK" => {
                self.initcheck = Some(match sub {
                    "" | "LAX" => Initcheck::Lax,
                    "STRICT" => Initcheck::Strict,
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
            // NOOPTIMIZE, OPTIMIZE, OPTIMIZE(STD) and OPTIMIZE(FULL) are tolerated as Table 51
            // maps them; FULL's STGOPT changes nothing ironwork runs.
            "OPTIMIZE" => {
                self.optimize = match (off, sub) {
                    (true, "") | (false, "0") => 0,
                    (false, "1") => 1,
                    (false, "2" | "" | "STD" | "FULL") => 2,
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
            // The compiler invocation's OPTIMIZE, which a card's outranks.
            "--optimize=0" => self.optimize = 0,
            "--optimize=1" => self.optimize = 1,
            "--optimize=2" => self.optimize = 2,
            f if f.starts_with("--compliance=") => match Compliance::named(&f["--compliance=".len()..]) {
                Some(c) => self.compliance = c,
                None => return Err(OptionError::UnknownFlag(flag.to_owned())),
            },
            f if f.starts_with("--dialect=") => match Dialect::named(&f["--dialect=".len()..]) {
                Some(d) => self.dialect = d,
                None => return Err(OptionError::UnknownFlag(flag.to_owned())),
            },
            f if f.starts_with("--assume=") => match Assumed::parse(&f["--assume=".len()..]) {
                Ok((i, mark)) => self.assumed.given[i] = mark,
                Err(_) => return Err(OptionError::UnknownFlag(flag.to_owned())),
            },
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

    /// Whether an unsigned zoned integer compared with zero, or with an unsigned zoned integer of
    /// its own length, is compared by its bytes, zones included: under INVDATA(NOFORCENUMCMP)
    /// (assumption C223), and under NOINVDATA at OPTIMIZE(1) or OPTIMIZE(2) (C262).
    pub fn zones_compared(&self) -> bool {
        match self.invdata {
            Some(i) => !i.forcenumcmp,
            None => self.optimize > 0,
        }
    }

    /// Whether an unsigned zoned integer compared with zero is compared by its bytes: where zones
    /// are compared, but under NOINVDATA and C262 gnucobol as a number at every OPTIMIZE
    /// level, as cobc compares it (C262).
    pub fn zones_compared_with_zero(&self) -> bool {
        self.zones_compared() && !self.cobc_zoned_compare()
    }

    /// Whether an unsigned zoned integer compared with one of its own length is compared by its
    /// bytes: where zones are compared, and under NOINVDATA and C262 gnucobol at every
    /// OPTIMIZE level, as cobc compares two such items with memcmp (C262).
    pub fn zones_compared_between_items(&self) -> bool {
        self.zones_compared() || self.cobc_zoned_compare()
    }

    fn cobc_zoned_compare(&self) -> bool {
        self.invdata.is_none() && self.dialect_of(Switched::OptimizedZonesCompared) == Dialect::Gnucobol
    }

    /// The place in its switch's list of the value a switched assumption takes: `--assume`'s, else
    /// the dialect's.
    fn switch_value(&self, i: usize) -> usize {
        match (self.assumed.given[i], self.dialect) {
            (0, Dialect::Ibm) => 0,
            (0, Dialect::Gnucobol) => 1,
            (mark, _) => usize::from(mark) - 1,
        }
    }

    /// Whose result a switched assumption gives. C101's `off` is [`Options::extra_place`]'s alone.
    pub fn dialect_of(&self, switched: Switched) -> Dialect {
        if self.switch_value(switched as usize) == 1 { Dialect::Gnucobol } else { Dialect::Ibm }
    }

    pub fn extra_place(&self) -> ExtraPlace {
        match self.switch_value(Switched::RoundedExtraPlace as usize) {
            0 => ExtraPlace::Every,
            1 => ExtraPlace::Last,
            _ => ExtraPlace::Off,
        }
    }

    /// The `--assume` flags that give these options' choices, for a compile that repeats them.
    pub fn assume_flags(&self) -> impl Iterator<Item = String> {
        SWITCHES.iter().zip(self.assumed.given).filter(|&(_, mark)| mark > 0).map(|((id, values), mark)| format!("--assume={id}={}", values[usize::from(mark) - 1]))
    }

    /// Each switched assumption whose value in force is not `ibm`, with that value.
    pub fn alternatives_in_force(&self) -> impl Iterator<Item = (&'static str, &'static str)> {
        SWITCHES.iter().enumerate().map(|(i, (id, values))| (*id, values[self.switch_value(i)])).filter(|&(_, value)| value != "ibm")
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
    fn optimize_levels_and_the_removed_spellings_table_51_maps() {
        let mut o = Options::default();
        assert_eq!(o.optimize, 0);
        for (option, level) in [("OPT(1)", 1), ("OPTIMIZE(0)", 0), ("opt(2)", 2), ("NOOPTIMIZE", 0), ("OPTIMIZE", 2), ("NOOPTIMIZE", 0), ("OPTIMIZE(STD)", 2), ("OPT(0)", 0), ("OPTIMIZE(FULL)", 2)] {
            assert_eq!(o.apply(option), Ok(true), "{option}");
            assert_eq!(o.optimize, level, "{option}");
        }
        assert!(o.apply("OPT(3)").is_err());
        assert!(o.apply("NOOPTIMIZE(2)").is_err());
        assert_eq!(o.optimize, 2);
    }

    #[test]
    fn zones_are_compared_under_invdata_noforcenumcmp_or_noinvdata_optimized() {
        let compared = |options: &[&str]| {
            let mut o = Options::default();
            options.iter().for_each(|x| assert_eq!(o.apply(x), Ok(true)));
            o.zones_compared()
        };
        assert!(!compared(&[]));
        assert!(compared(&["OPT(1)"]));
        assert!(compared(&["OPT(2)"]));
        assert!(compared(&["INVDATA"]));
        assert!(!compared(&["INVDATA(FNC)", "OPT(2)"]));
        assert!(!compared(&["ZONEDATA(MIG)", "OPT(2)"]));
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
        for name in ["ARITH", "CODEPAGE", "TRUNC", "NUMPROC", "FASTSRT", "COMPILE", "INVDATA", "ZONEDATA", "ZWB", "OPTIMIZE"] {
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
        assert_eq!(o.apply("CODEPAGE(290)"), Err(OptionError::UnsupportedCodePage(290)));
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
    fn codepage_takes_the_mixed_pages_dbcs_programs_compile_with() {
        let mut o = Options::default();
        for ccsid in [930, 939, 1390, 1399, 5026, 5035, 933, 1364, 935, 1388, 937] {
            o.apply(&format!("CODEPAGE({ccsid})")).unwrap();
            assert!(o.code_page().dbcs_ccsid().is_some(), "{ccsid}");
        }
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
    fn numcheck_takes_ibms_suboption_defaults_and_zonecheck_is_its_zoned_check() {
        let numcheck = |card: &str| {
            let mut o = Options::default();
            o.apply(card).map(|_| o.numcheck)
        };
        let all = Numcheck::default();
        assert_eq!(all, Numcheck { zon: Some(ZonCheck { alphnum: true, lax: false }), pac: true, bin: Some(BinCheck { truncbin: true }), abd: false });
        assert_eq!(numcheck("NUMCHECK"), Ok(Some(all)));
        assert_eq!(numcheck("NC(ABD)"), Ok(Some(Numcheck { abd: true, ..all })));
        assert_eq!(numcheck("NUMCHECK(BIN)"), Ok(Some(Numcheck { zon: None, pac: false, bin: Some(BinCheck { truncbin: true }), abd: false })));
        assert_eq!(
            numcheck("NUMCHECK(ZON(NOALPHNUM,LAX),NOPAC,BIN(NOTRUNCBIN),ABD)"),
            Ok(Some(Numcheck { zon: Some(ZonCheck { alphnum: false, lax: true }), pac: false, bin: Some(BinCheck { truncbin: false }), abd: true }))
        );
        assert_eq!(numcheck("NUMCHECK(ZON(LAXREDEF))"), Ok(Some(Numcheck { zon: Some(ZonCheck { alphnum: true, lax: true }), pac: false, bin: None, abd: false })));
        assert_eq!(numcheck("NUMCHECK(NOZON,NOPAC,NOBIN)"), Ok(None));
        assert_eq!(numcheck("NONC"), Ok(None));
        for bad in ["NUMCHECK(ZON(X))", "NUMCHECK(PAC(X))", "NUMCHECK(BIN(X))", "NUMCHECK(X)", "ZONECHECK", "ZC(X)"] {
            assert!(matches!(numcheck(bad), Err(OptionError::BadSuboption { .. })), "{bad}");
        }
        assert_eq!(numcheck("ZC(ABD)"), Ok(Some(Numcheck { zon: Some(ZonCheck::default()), pac: false, bin: None, abd: true })));
        let mut o = Options::default();
        o.apply("NUMCHECK").unwrap();
        o.apply("NOZONECHECK").unwrap();
        assert_eq!(o.numcheck, Some(Numcheck { zon: None, ..all }));
        o.apply("ZONECHECK(MSG)").unwrap();
        o.apply("NOZC").unwrap();
        assert_eq!(o.numcheck, None);
    }

    #[test]
    fn parmcheck_and_initcheck_take_their_suboptions_and_ibms_defaults() {
        let parmcheck = |card: &str| {
            let mut o = Options::default();
            o.apply(card).map(|_| o.parmcheck)
        };
        assert_eq!(parmcheck("PARMCHECK"), Ok(Some(Parmcheck { abd: false, bytes: 100 })));
        assert_eq!(parmcheck("PC(ABD)"), Ok(Some(Parmcheck { abd: true, bytes: 100 })));
        assert_eq!(parmcheck("PC(5000)"), Ok(Some(Parmcheck { abd: false, bytes: 5000 })));
        assert_eq!(parmcheck("PARMCHECK(ABD,1)"), Ok(Some(Parmcheck { abd: true, bytes: 1 })));
        assert_eq!(parmcheck("NOPC"), Ok(None));
        for bad in ["PC(0)", "PC(10000)", "PC(5000,ABD)", "PC(MSG,1,2)", "PC(X)"] {
            assert!(matches!(parmcheck(bad), Err(OptionError::BadSuboption { .. })), "{bad}");
        }
        let initcheck = |card: &str| {
            let mut o = Options::default();
            o.apply(card).map(|_| o.initcheck)
        };
        assert_eq!(initcheck("INITCHECK"), Ok(Some(Initcheck::Lax)));
        assert_eq!(initcheck("IC(STRICT)"), Ok(Some(Initcheck::Strict)));
        assert_eq!(initcheck("NOIC"), Ok(None));
        assert!(matches!(initcheck("IC(X)"), Err(OptionError::BadSuboption { .. })));
        let o = Options::default();
        assert_eq!((o.numcheck, o.parmcheck, o.initcheck), (None, None, None));
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

    #[test]
    fn compliance_is_strict_unless_the_flag_says_extended() {
        let mut o = Options::default();
        assert_eq!(o.compliance, Compliance::Strict);
        for level in [Compliance::Extended, Compliance::Strict] {
            o.apply_flag(level.flag()).unwrap();
            assert_eq!(o.compliance, level);
            assert_eq!(Compliance::named(level.name()), Some(level));
        }
        for bad in ["--compliance=EXTENDED", "--compliance=", "--compliance", "--compliance=mf"] {
            assert!(o.apply_flag(bad).is_err(), "{bad}");
        }
        let flags = |given: &[&str]| Compliance::of(&given.iter().map(|f| f.to_string()).collect::<Vec<_>>());
        assert_eq!(flags(&[]), Compliance::Strict);
        assert_eq!(flags(&["-silent", "--compliance=extended"]), Compliance::Extended);
        assert_eq!(flags(&["--compliance=extended", "--compliance=strict"]), Compliance::Strict);
    }

    #[test]
    fn the_dialect_is_ibm_unless_the_flag_says_gnucobol() {
        let mut o = Options::default();
        assert_eq!(o.dialect, Dialect::Ibm);
        for dialect in [Dialect::Gnucobol, Dialect::Ibm] {
            o.apply_flag(dialect.flag()).unwrap();
            assert_eq!(o.dialect, dialect);
            assert_eq!(Dialect::named(dialect.name()), Some(dialect));
        }
        for bad in ["--dialect=GNUCOBOL", "--dialect=", "--dialect", "--dialect=mf"] {
            assert!(o.apply_flag(bad).is_err(), "{bad}");
        }
        assert_eq!(o.dialect, Dialect::Ibm);
    }

    #[test]
    fn each_switch_names_its_assumption_in_the_order_switched_lists_them() {
        use assumptions::*;
        let ids = [ROUNDED_EXTRA_PLACE, DISPLAY_OF_NONDISPLAY_NUMERIC, DECIMAL_COMMA_DISPLAY_LITERAL, ACCEPT_AT_END, ENTRY_CALLS, EXTERNAL_STORAGE, OPTIMIZED_ZONES_COMPARED];
        let switched = [
            Switched::RoundedExtraPlace,
            Switched::DisplayOfNondisplayNumeric,
            Switched::DecimalCommaDisplayLiteral,
            Switched::AcceptAtEnd,
            Switched::EntryCalls,
            Switched::ExternalStorage,
            Switched::OptimizedZonesCompared,
        ];
        for (s, id) in switched.into_iter().zip(ids) {
            let (switch, values) = SWITCHES[s as usize];
            assert_eq!(switch, id);
            assert_eq!(values[..2], ["ibm", "gnucobol"], "{id}");
            assert_eq!(get(id).basis, Basis::Chosen, "{id}");
        }
    }

    #[test]
    fn assume_switches_one_assumption_and_wins_over_the_dialect_whatever_the_order() {
        let mut o = Options::default();
        assert_eq!((o.extra_place(), o.dialect_of(Switched::AcceptAtEnd)), (ExtraPlace::Every, Dialect::Ibm));
        o.apply_flag("--assume=C15=gnucobol").unwrap();
        assert_eq!((o.extra_place(), o.dialect_of(Switched::AcceptAtEnd)), (ExtraPlace::Every, Dialect::Gnucobol));
        o.apply_flag("--dialect=gnucobol").unwrap();
        o.apply_flag("--assume=C101=off").unwrap();
        assert_eq!((o.extra_place(), o.dialect_of(Switched::EntryCalls)), (ExtraPlace::Off, Dialect::Gnucobol));
        o.apply_flag("--assume=C101=ibm").unwrap();
        assert_eq!(o.extra_place(), ExtraPlace::Every);
        assert_eq!(o.assume_flags().collect::<Vec<_>>(), ["--assume=C101=ibm", "--assume=C15=gnucobol"]);
        assert_eq!(o.alternatives_in_force().map(|(id, _)| id).collect::<Vec<_>>(), ["C14", "C95", "C15", "C51", "C180", "C262"]);
        o.apply_flag("--dialect=ibm").unwrap();
        assert_eq!(o.alternatives_in_force().collect::<Vec<_>>(), [("C15", "gnucobol")]);
        assert!(o.apply_flag("--assume=C101=on").is_err());
        assert_eq!(o.extra_place(), ExtraPlace::Every);
    }

    #[test]
    fn assume_refuses_by_name_what_it_cannot_switch() {
        assert_eq!(Assumed::parse("C101=off"), Ok((0, 3)));
        assert_eq!(Assumed::parse("C262=ibm"), Ok((6, 1)));
        for (spec, said) in [
            ("C101", "--assume C101: needs ID=VALUE, such as C101=off"),
            ("C1=gnucobol", "--assume C1=gnucobol: assumption C1 (documented) has no alternative; --assume switches C101, C14, C95, C15, C51, C180 and C262"),
            ("C16=gnucobol", "--assume C16=gnucobol: assumption C16 (chosen) has no alternative; --assume switches C101, C14, C95, C15, C51, C180 and C262"),
            ("C9999=off", "--assume C9999=off: the register has no assumption C9999; ironwork assumptions lists them"),
            ("C14=off", "--assume C14=off: C14 takes ibm or gnucobol"),
            ("C101=GNUCOBOL", "--assume C101=GNUCOBOL: C101 takes ibm, gnucobol or off"),
        ] {
            assert_eq!(Assumed::parse(spec), Err(said.to_owned()), "{spec}");
        }
    }
}
