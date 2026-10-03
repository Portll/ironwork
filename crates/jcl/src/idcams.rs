//! IDCAMS commands from SYSIN, as DFSMS Access Method Services reads them: columns 2 to 72, a
//! hyphen or plus continuing a command, comments between /* and */, and the modal commands IF,
//! SET, DO and END around DELETE, REPRO and DEFINE CLUSTER. Every other command, and every
//! parameter these do not model, is refused by name.

use crate::cond::Op;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Dd(String),
    Dataset(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Organization {
    Indexed,
    Nonindexed,
    Numbered,
    Linear,
}

/// KEYS(length offset): where a key lies in each record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Keys {
    pub length: usize,
    pub offset: usize,
}

/// RECORDSIZE(average maximum).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordSize {
    pub average: usize,
    pub maximum: usize,
}

pub const DEFAULT_KEYS: Keys = Keys { length: 64, offset: 0 };
pub const DEFAULT_RECORD_SIZE: RecordSize = RecordSize { average: 4089, maximum: 4089 };
pub const DEFAULT_AIX_RECORD_SIZE: RecordSize = RecordSize { average: 4086, maximum: 32600 };

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cluster {
    pub name: String,
    pub organization: Organization,
    /// An indexed cluster's prime key.
    pub keys: Option<Keys>,
    pub record_size: RecordSize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlternateIndex {
    pub name: String,
    /// The base cluster it indexes.
    pub relate: String,
    pub keys: Keys,
    pub unique: bool,
    /// Kept current as the base cluster changes.
    pub upgrade: bool,
    pub record_size: RecordSize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
    pub name: String,
    /// The alternate index the path reaches its base cluster through.
    pub entry: String,
    /// Opening the path opens the base cluster's upgrade set too.
    pub update: bool,
}

/// A key as PRINT's FROMKEY and TOKEY give it: characters, or X'hex' bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    Chars(String),
    Bytes(Vec<u8>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintFormat {
    Character,
    Hex,
    Dump,
}

/// Where PRINT starts: the first record, after SKIP(n) records, or at the first key from FROMKEY.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum First {
    Start,
    Skip(usize),
    Key(Key),
}

/// Where PRINT stops: the last record, after COUNT(n) records, or after the last key up to TOKEY.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Last {
    End,
    Count(usize),
    Key(Key),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Print {
    pub from: Target,
    pub format: PrintFormat,
    pub first: First,
    pub last: Last,
    /// OUTFILE's DD, in place of SYSPRINT.
    pub out: Option<String>,
}

/// The catalog entries LISTCAT lists: those named, those whose names begin with a level, or all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entries {
    All,
    Named(Vec<String>),
    Level(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EntryType {
    Cluster,
    Data,
    Index,
    AlternateIndex,
    Path,
    Nonvsam,
    GenerationDataGroup,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listcat {
    pub entries: Entries,
    /// The entry types listed; empty lists every type.
    pub types: Vec<EntryType>,
    /// ALL: each entry's attributes as well as its name.
    pub all: bool,
    pub out: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Each entry is a data set name, or NAME(MEMBER).
    Delete(Vec<String>),
    Repro { from: Target, to: Target },
    DefineCluster(Cluster),
    DefineAlternateIndex(AlternateIndex),
    DefinePath(Path),
    /// BLDINDEX: each alternate index built from its base cluster's records.
    Bldindex { from: Target, to: Vec<Target> },
    Listcat(Listcat),
    Print(Print),
    /// A generation data group's base: how many generations it keeps, whether a generation that
    /// rolls off is scratched, and whether all roll off together when the limit is passed.
    DefineGdg { name: String, limit: u16, scratch: bool, empty: bool },
    /// SET MAXCC= when `max`, SET LASTCC= otherwise.
    Set { max: bool, value: u16 },
    If { max: bool, op: Op, value: u16, then: Vec<Command>, otherwise: Vec<Command> },
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Word(String),
    /// A keyword and the text of its parenthesised value.
    Keyed(String, String),
    Group(String),
    Op(Op),
    End,
}

/// The commands' text: columns 2-72 of each card, comments removed, continued cards joined, and
/// a newline where each command ends.
fn command_text(cards: &[String]) -> Result<String, String> {
    let mut out = String::new();
    let mut in_comment = false;
    for card in cards {
        let text: String = card.chars().skip(1).take(71).collect();
        let mut line = String::new();
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if in_comment {
                if c == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    in_comment = false;
                }
                continue;
            }
            if c == '/' && chars.peek() == Some(&'*') {
                chars.next();
                in_comment = true;
                continue;
            }
            line.push(c);
        }
        let trimmed = line.trim_end();
        if let Some(rest) = trimmed.strip_suffix('-') {
            out.push_str(rest);
            out.push(' ');
        } else if let Some(rest) = trimmed.strip_suffix('+') {
            out.push_str(rest.trim_end());
        } else {
            out.push_str(trimmed);
            out.push('\n');
        }
    }
    if in_comment {
        return Err("a comment is not closed with */".into());
    }
    Ok(out)
}

fn tokens(text: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = text.chars().collect();
    let (mut out, mut i) = (Vec::new(), 0);
    let group = |i: &mut usize| -> Result<String, String> {
        let (start, mut depth) = (*i + 1, 0i32);
        let mut quoted = false;
        while *i < chars.len() {
            match chars[*i] {
                '\'' => quoted = !quoted,
                '(' if !quoted => depth += 1,
                ')' if !quoted => {
                    depth -= 1;
                    if depth == 0 {
                        *i += 1;
                        return Ok(chars[start..*i - 1].iter().collect::<String>().trim().to_string());
                    }
                }
                _ => {}
            }
            *i += 1;
        }
        Err("a parenthesis is not closed".into())
    };
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match c {
            '\n' | ';' => {
                out.push(Token::End);
                i += 1;
            }
            ' ' | ',' | '\t' => i += 1,
            '(' => out.push(Token::Group(group(&mut i)?)),
            '=' => {
                out.push(Token::Op(Op::Eq));
                i += 1;
            }
            '¬' if next == Some('=') => {
                out.push(Token::Op(Op::Ne));
                i += 2;
            }
            '>' | '<' => {
                let op = match (c, next == Some('=')) {
                    ('>', true) => Op::Ge,
                    ('>', false) => Op::Gt,
                    ('<', true) => Op::Le,
                    _ => Op::Lt,
                };
                out.push(Token::Op(op));
                i += if next == Some('=') { 2 } else { 1 };
            }
            _ => {
                let start = i;
                while i < chars.len() && !matches!(chars[i], ' ' | ',' | '\t' | '\n' | ';' | '(' | ')' | '=' | '¬' | '>' | '<') {
                    i += 1;
                }
                if start == i {
                    return Err(format!("{c} has no meaning here"));
                }
                let word: String = chars[start..i].iter().collect::<String>().to_ascii_uppercase();
                let mut j = i;
                while j < chars.len() && chars[j] == ' ' {
                    j += 1;
                }
                if chars.get(j) == Some(&'(') {
                    i = j;
                    out.push(Token::Keyed(word, group(&mut i)?));
                } else {
                    out.push(match word.as_str() {
                        "EQ" => Token::Op(Op::Eq),
                        "NE" => Token::Op(Op::Ne),
                        "GT" => Token::Op(Op::Gt),
                        "GE" => Token::Op(Op::Ge),
                        "LT" => Token::Op(Op::Lt),
                        "LE" => Token::Op(Op::Le),
                        _ => Token::Word(word),
                    });
                }
            }
        }
    }
    out.push(Token::End);
    Ok(out)
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
}

fn condition_code(text: &str) -> Result<u16, String> {
    match text.parse::<u16>() {
        Ok(n) if n <= 16 && text.bytes().all(|b| b.is_ascii_digit()) => Ok(n),
        _ => Err(format!("{text} is not a condition code from 0 to 16")),
    }
}

fn name_of(value: &str) -> Result<String, String> {
    let v = value.trim().trim_matches('\'').to_string();
    let base = v.split_once('(').map_or(v.as_str(), |(b, _)| b);
    if !crate::is_dsn(base) {
        return Err(format!("{v} is not a data set name"));
    }
    if let Some((_, m)) = v.split_once('(') {
        let m = m.strip_suffix(')').unwrap_or("");
        if !crate::is_name(m) {
            return Err(format!("{v} is not a data set name"));
        }
    }
    Ok(v)
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn skip_ends(&mut self) {
        while self.peek() == Some(&Token::End) {
            self.at += 1;
        }
    }

    fn word(&mut self) -> Option<String> {
        match self.peek() {
            Some(Token::Word(w)) => {
                let w = w.clone();
                self.at += 1;
                Some(w)
            }
            _ => None,
        }
    }

    fn commands(&mut self, until_end: bool) -> Result<Vec<Command>, String> {
        let mut out = Vec::new();
        loop {
            self.skip_ends();
            match self.peek() {
                None => {
                    return if until_end { Err("DO has no END".into()) } else { Ok(out) };
                }
                Some(Token::Word(w)) if w == "END" => {
                    if !until_end {
                        return Err("END without DO".into());
                    }
                    self.at += 1;
                    return Ok(out);
                }
                Some(Token::Word(w)) if w == "ELSE" => {
                    if until_end {
                        return Err("ELSE inside DO with no IF".into());
                    }
                    return Err("ELSE without IF".into());
                }
                _ => out.push(self.command()?),
            }
        }
    }

    /// One command, or a DO group, after THEN or ELSE.
    fn clause(&mut self) -> Result<Vec<Command>, String> {
        self.skip_ends();
        if self.peek() == Some(&Token::Word("DO".into())) {
            self.at += 1;
            return self.commands(true);
        }
        if matches!(self.peek(), Some(Token::Word(w)) if w == "ELSE") || self.peek().is_none() {
            return Ok(Vec::new());
        }
        Ok(vec![self.command()?])
    }

    fn command(&mut self) -> Result<Command, String> {
        let mut args = Vec::new();
        let verb = match self.peek().cloned() {
            Some(Token::Word(w)) => w,
            Some(Token::Keyed(w, group)) => {
                args.push(Token::Group(group));
                w
            }
            other => return Err(format!("a command, found {other:?}")),
        };
        self.at += 1;
        if verb != "IF" {
            while let Some(t) = self.peek() {
                if *t == Token::End {
                    break;
                }
                args.push(t.clone());
                self.at += 1;
            }
        }
        match verb.as_str() {
            "IF" => self.if_command(),
            "SET" => match args.as_slice() {
                [Token::Word(which), Token::Op(Op::Eq), Token::Word(n)] if which == "MAXCC" || which == "LASTCC" => Ok(Command::Set { max: which == "MAXCC", value: condition_code(n)? }),
                _ => Err("SET takes MAXCC=n or LASTCC=n".into()),
            },
            "DELETE" | "DEL" => delete(&args),
            "REPRO" => repro(&args),
            "DEFINE" | "DEF" => define(&args),
            "BLDINDEX" | "BIX" => bldindex(&args),
            "LISTCAT" | "LISTC" => listcat(&args),
            "PRINT" => print(&args),
            "DO" | "END" | "THEN" | "ELSE" => Err(format!("{verb} is out of place")),
            v => Err(format!("the IDCAMS command {v} is not supported yet")),
        }
    }

    fn if_command(&mut self) -> Result<Command, String> {
        let which = self.word().filter(|w| w == "MAXCC" || w == "LASTCC").ok_or("IF takes MAXCC or LASTCC")?;
        let Some(Token::Op(op)) = self.peek().cloned() else { return Err("IF needs a comparison operator".into()) };
        self.at += 1;
        let value = condition_code(&self.word().ok_or("IF needs a condition code")?)?;
        if self.word().as_deref() != Some("THEN") {
            return Err("IF needs THEN".into());
        }
        let then = self.clause()?;
        let mark = self.at;
        self.skip_ends();
        let otherwise = if self.word().as_deref() == Some("ELSE") {
            self.clause()?
        } else {
            self.at = mark;
            Vec::new()
        };
        Ok(Command::If { max: which == "MAXCC", op, value, then, otherwise })
    }
}

const DELETE_IGNORED: &[&str] = &["PURGE", "PRG", "NOPURGE", "NPRG", "ERASE", "ERAS", "NOERASE", "NERAS", "SCRATCH", "SCR", "NOSCRATCH", "NSCR", "NONVSAM", "NVSAM", "CLUSTER", "CL", "FORCE", "FRC", "NOFORCE", "NFRC", "GENERATIONDATAGROUP", "GDG"];

fn delete(args: &[Token]) -> Result<Command, String> {
    let mut names = Vec::new();
    for a in args {
        match a {
            Token::Word(w) if DELETE_IGNORED.contains(&w.as_str()) => {}
            Token::Word(w) if w.contains('*') || w.contains('%') => return Err(format!("DELETE of a generic name ({w}) is not supported yet")),
            Token::Word(w) => names.push(name_of(w)?),
            Token::Group(g) => {
                for n in g.split([' ', ',']).filter(|n| !n.is_empty()) {
                    names.push(name_of(n)?);
                }
            }
            Token::Keyed(k, v) if crate::is_dsn(k) => names.push(name_of(&format!("{k}({v})"))?),
            Token::Keyed(k, _) => return Err(format!("DELETE parameter {k} is not supported yet")),
            other => return Err(format!("DELETE has {other:?} where a name belongs")),
        }
    }
    if names.is_empty() {
        return Err("DELETE names no entry".into());
    }
    Ok(Command::Delete(names))
}

/// INFILE or INDATASET (true), or OUTFILE or OUTDATASET (false), as REPRO and PRINT name their
/// data sets; None for any other parameter.
fn in_out(k: &str, v: &str) -> Result<Option<(bool, Target)>, String> {
    Ok(Some(match k {
        "INFILE" | "IFILE" => (true, Target::Dd(dd_name(v)?)),
        "INDATASET" | "IDS" => (true, Target::Dataset(name_of(v)?)),
        "OUTFILE" | "OFILE" => (false, Target::Dd(dd_name(v)?)),
        "OUTDATASET" | "ODS" => (false, Target::Dataset(name_of(v)?)),
        _ => return Ok(None),
    }))
}

fn repro(args: &[Token]) -> Result<Command, String> {
    let (mut from, mut to) = (None, None);
    for a in args {
        match a {
            Token::Keyed(k, v) => match in_out(k, v)? {
                Some((true, t)) => from = Some(t),
                Some((false, t)) => to = Some(t),
                None => return Err(format!("REPRO parameter {k} is not supported yet")),
            },
            Token::Word(w) => return Err(format!("REPRO parameter {w} is not supported yet")),
            other => return Err(format!("REPRO has {other:?} where a parameter belongs")),
        }
    }
    match (from, to) {
        (Some(from), Some(to)) => Ok(Command::Repro { from, to }),
        _ => Err("REPRO needs INFILE or INDATASET, and OUTFILE or OUTDATASET".into()),
    }
}

/// The sort BLDINDEX uses and its work files change how it runs, not the index it builds.
const BLDINDEX_IGNORED: &[&str] = &["INTERNALSORT", "ISORT", "EXTERNALSORT", "ESORT", "WORKFILES", "WFILE", "SORTCALL", "NOSORTCALL", "SORTDEVICETYPE", "SDVT", "SORTFILENUMBER", "SFN", "SORTMESSAGEDD", "SMDD", "SORTMESSAGELEVEL", "SML", "CATALOG", "CAT"];

fn bldindex(args: &[Token]) -> Result<Command, String> {
    let (mut from, mut to) = (None, Vec::new());
    for a in args {
        match a {
            Token::Keyed(k, v) => match k.as_str() {
                "INFILE" | "IFILE" => from = Some(Target::Dd(dd_name(v)?)),
                "INDATASET" | "IDS" => from = Some(Target::Dataset(name_of(v)?)),
                "OUTFILE" | "OFILE" => to.extend(v.split([' ', ',']).filter(|n| !n.is_empty()).map(|n| dd_name(n).map(Target::Dd)).collect::<Result<Vec<_>, _>>()?),
                "OUTDATASET" | "ODS" => to.extend(v.split([' ', ',']).filter(|n| !n.is_empty()).map(|n| name_of(n).map(Target::Dataset)).collect::<Result<Vec<_>, _>>()?),
                k if BLDINDEX_IGNORED.contains(&k) => {}
                k => return Err(format!("BLDINDEX parameter {k} is not supported yet")),
            },
            Token::Word(w) if BLDINDEX_IGNORED.contains(&w.as_str()) => {}
            Token::Word(w) => return Err(format!("BLDINDEX parameter {w} is not supported yet")),
            other => return Err(format!("BLDINDEX has {other:?} where a parameter belongs")),
        }
    }
    match from {
        Some(from) if !to.is_empty() => Ok(Command::Bldindex { from, to }),
        _ => Err("BLDINDEX needs INFILE or INDATASET, and OUTFILE or OUTDATASET".into()),
    }
}

fn dd_name(v: &str) -> Result<String, String> {
    let name = v.split_whitespace().next().unwrap_or("");
    if crate::is_name(name) && v.split_whitespace().count() == 1 { Ok(name.to_string()) } else { Err(format!("{v} is not a DD name")) }
}

/// The whole numbers of a parenthesised value, as KEYS(8 0) or RECORDSIZE(80,80) give them.
fn numbers<const N: usize>(keyword: &str, v: &str) -> Result<[usize; N], String> {
    let parts: Vec<&str> = v.split([' ', ',']).filter(|p| !p.is_empty()).collect();
    let parsed: Option<Vec<usize>> = parts.iter().map(|p| p.parse().ok().filter(|_| p.bytes().all(|b| b.is_ascii_digit()))).collect();
    parsed.and_then(|n| n.try_into().ok()).ok_or_else(|| format!("{keyword}({v}) needs {N} whole numbers"))
}

fn keys(v: &str) -> Result<Keys, String> {
    let [length, offset] = numbers("KEYS", v)?;
    if !(1..=255).contains(&length) {
        return Err(format!("KEYS({v}) has a length that is not from 1 to 255"));
    }
    Ok(Keys { length, offset })
}

fn record_size(v: &str) -> Result<RecordSize, String> {
    let [average, maximum] = numbers("RECORDSIZE", v)?;
    if average == 0 || average > maximum {
        return Err(format!("RECORDSIZE({v}) needs an average from 1 to the maximum"));
    }
    Ok(RecordSize { average, maximum })
}

fn key_value(v: &str) -> Result<Key, String> {
    let v = v.trim();
    if let Some(hex) = v.strip_prefix("X'").or_else(|| v.strip_prefix("x'")).and_then(|h| h.strip_suffix('\'')) {
        let digits: Vec<u8> = hex.bytes().collect();
        if digits.is_empty() || !digits.len().is_multiple_of(2) || !digits.iter().all(u8::is_ascii_hexdigit) {
            return Err(format!("{v} is not an even number of hexadecimal digits"));
        }
        let pairs = digits.chunks(2).map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap_or("00"), 16).unwrap_or(0));
        return Ok(Key::Bytes(pairs.collect()));
    }
    let text = match v.strip_prefix('\'').and_then(|q| q.strip_suffix('\'')) {
        Some(quoted) => quoted.replace("''", "'"),
        None => v.to_string(),
    };
    if text.is_empty() || text.chars().count() > 255 {
        return Err(format!("{v} is not a key of 1 to 255 characters"));
    }
    Ok(Key::Chars(text))
}

fn print(args: &[Token]) -> Result<Command, String> {
    let (mut from, mut format, mut first, mut last, mut out) = (None, PrintFormat::Dump, First::Start, Last::End, None);
    for a in args {
        match a {
            Token::Keyed(k, v) => match (in_out(k, v)?, k.as_str()) {
                (Some((true, t)), _) => from = Some(t),
                (Some((false, Target::Dd(dd))), _) => out = Some(dd),
                (Some((false, Target::Dataset(_))), _) => return Err("PRINT writes to OUTFILE, not OUTDATASET".into()),
                (None, "SKIP") => first = First::Skip(numbers::<1>("SKIP", v)?[0]),
                (None, "FROMKEY" | "FKEY") => first = First::Key(key_value(v)?),
                (None, "COUNT") => last = Last::Count(numbers::<1>("COUNT", v)?[0]),
                (None, "TOKEY" | "TKEY") => last = Last::Key(key_value(v)?),
                (None, k) => return Err(format!("PRINT parameter {k} is not supported yet")),
            },
            Token::Word(w) => {
                format = match w.as_str() {
                    "CHARACTER" | "CHAR" => PrintFormat::Character,
                    "HEX" => PrintFormat::Hex,
                    "DUMP" => PrintFormat::Dump,
                    w => return Err(format!("PRINT parameter {w} is not supported yet")),
                }
            }
            other => return Err(format!("PRINT has {other:?} where a parameter belongs")),
        }
    }
    let from = from.ok_or("PRINT needs INFILE or INDATASET")?;
    Ok(Command::Print(Print { from, format, first, last, out }))
}

/// A LISTCAT ENTRIES name: a data set name, a qualifier of which may be an asterisk.
fn listed_name(v: &str) -> Result<String, String> {
    let v = v.trim().trim_matches('\'');
    let stand_in = v.split('.').map(|q| if q == "*" { "A" } else { q }).collect::<Vec<_>>().join(".");
    if crate::is_dsn(&stand_in) { Ok(v.to_string()) } else { Err(format!("{v} is not a data set name or a generic name")) }
}

fn listcat(args: &[Token]) -> Result<Command, String> {
    let (mut entries, mut types, mut all, mut out) = (Entries::All, Vec::new(), false, None);
    for a in args {
        match a {
            Token::Keyed(k, v) => match k.as_str() {
                "ENTRIES" | "ENTRY" | "ENT" => entries = Entries::Named(v.split([' ', ',']).filter(|n| !n.is_empty()).map(listed_name).collect::<Result<_, _>>()?),
                "LEVEL" | "LVL" => entries = Entries::Level(listed_name(v)?),
                "OUTFILE" | "OFILE" => out = Some(dd_name(v)?),
                "CATALOG" | "CAT" => {}
                k => return Err(format!("LISTCAT parameter {k} is not supported yet")),
            },
            Token::Word(w) => match w.as_str() {
                "ALL" => all = true,
                "NAME" | "NAMES" => all = false,
                "CLUSTER" | "CL" => types.push(EntryType::Cluster),
                "DATA" => types.push(EntryType::Data),
                "INDEX" | "IX" => types.push(EntryType::Index),
                "ALTERNATEINDEX" | "AIX" => types.push(EntryType::AlternateIndex),
                "PATH" => types.push(EntryType::Path),
                "NONVSAM" | "NVSAM" => types.push(EntryType::Nonvsam),
                "GENERATIONDATAGROUP" | "GDG" => types.push(EntryType::GenerationDataGroup),
                w => return Err(format!("LISTCAT parameter {w} is not supported yet")),
            },
            other => return Err(format!("LISTCAT has {other:?} where a parameter belongs")),
        }
    }
    if matches!(&entries, Entries::Named(n) if n.is_empty()) {
        return Err("LISTCAT ENTRIES names no entry".into());
    }
    types.sort_unstable();
    types.dedup();
    Ok(Command::Listcat(Listcat { entries, types, all, out }))
}

fn define_gdg(group: &str) -> Result<Command, String> {
    let (mut name, mut limit, mut scratch, mut empty) = (None, None, false, false);
    for t in tokens(group)? {
        match t {
            Token::Keyed(k, v) if k == "NAME" => name = Some(name_of(&v)?),
            Token::Keyed(k, v) if k == "LIMIT" || k == "LIM" => match v.trim().parse::<u16>() {
                Ok(n) if (1..=255).contains(&n) => limit = Some(n),
                _ => return Err(format!("LIMIT({v}) is not from 1 to 255")),
            },
            Token::Word(w) if w == "SCRATCH" || w == "SCR" => scratch = true,
            Token::Word(w) if w == "NOSCRATCH" || w == "NSCR" => scratch = false,
            Token::Word(w) if w == "EMPTY" || w == "EMP" => empty = true,
            Token::Word(w) if w == "NOEMPTY" || w == "NEMP" => empty = false,
            Token::End => {}
            Token::Keyed(k, _) | Token::Word(k) => return Err(format!("DEFINE GDG parameter {k} is not supported yet")),
            other => return Err(format!("DEFINE GDG has {other:?} where a parameter belongs")),
        }
    }
    match (name, limit) {
        (Some(name), Some(limit)) if !name.contains('(') && name.len() <= 35 => Ok(Command::DefineGdg { name, limit, scratch, empty }),
        (Some(name), Some(_)) => Err(format!("{name} is not a generation data group name of 35 characters or fewer")),
        _ => Err("DEFINE GDG needs NAME and LIMIT".into()),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Object {
    Cluster,
    AlternateIndex,
    Path,
}

/// The NAME in an object's parameters.
fn object_name(object: &[Token]) -> Result<Option<String>, String> {
    object.iter().find_map(|t| if let Token::Keyed(k, v) = t && k == "NAME" { Some(name_of(v)) } else { None }).transpose()
}

/// KEYS and RECORDSIZE, from the object's parameters or its DATA component's.
fn sizes(object: &[Token], data: &[Token]) -> Result<(Option<Keys>, Option<RecordSize>), String> {
    let (mut k, mut r) = (None, None);
    for t in object.iter().chain(data) {
        match t {
            Token::Keyed(w, v) if w == "KEYS" => k = Some(keys(v)?),
            Token::Keyed(w, v) if w == "RECORDSIZE" || w == "RECSZ" => r = Some(record_size(v)?),
            _ => {}
        }
    }
    Ok((k, r))
}

fn define_cluster(object: &[Token], data: &[Token]) -> Result<Cluster, String> {
    let name = object_name(object)?.ok_or("DEFINE CLUSTER needs NAME")?;
    let mut organization = Organization::Indexed;
    for t in object.iter().chain(data) {
        if let Token::Keyed(k, _) | Token::Word(k) = t
            && (k == "DATABASE" || k == "ZFS")
        {
            return Err(format!("DEFINE CLUSTER {k} is not supported yet"));
        }
        if let Token::Word(w) = t {
            organization = match w.as_str() {
                "INDEXED" | "IXD" => Organization::Indexed,
                "NONINDEXED" | "NIXD" => Organization::Nonindexed,
                "NUMBERED" | "NUMD" => Organization::Numbered,
                "LINEAR" | "LIN" => Organization::Linear,
                _ => organization,
            };
        }
    }
    let (k, r) = sizes(object, data)?;
    let keys = (organization == Organization::Indexed).then(|| k.unwrap_or(DEFAULT_KEYS));
    let record_size = r.unwrap_or(DEFAULT_RECORD_SIZE);
    fits(&name, keys, record_size)?;
    Ok(Cluster { name, organization, keys, record_size })
}

fn fits(name: &str, keys: Option<Keys>, size: RecordSize) -> Result<(), String> {
    match keys {
        Some(k) if k.offset + k.length > size.maximum => Err(format!("{name}: KEYS({} {}) does not fit in a record of {} bytes", k.length, k.offset, size.maximum)),
        _ => Ok(()),
    }
}

fn define_alternate_index(object: &[Token], data: &[Token]) -> Result<AlternateIndex, String> {
    let name = object_name(object)?.ok_or("DEFINE ALTERNATEINDEX needs NAME")?;
    let (mut relate, mut unique, mut upgrade) = (None, false, true);
    for t in object {
        match t {
            Token::Keyed(k, v) if k == "RELATE" || k == "REL" => relate = Some(name_of(v)?),
            Token::Keyed(k, _) if k == "ALTKEYS" || k == "ALTKEYSU" => return Err(format!("DEFINE ALTERNATEINDEX {k} is not supported yet")),
            Token::Word(w) if w == "UNIQUEKEY" || w == "UNQK" => unique = true,
            Token::Word(w) if w == "NONUNIQUEKEY" || w == "NUNQK" => unique = false,
            Token::Word(w) if w == "UPGRADE" || w == "UPG" => upgrade = true,
            Token::Word(w) if w == "NOUPGRADE" || w == "NUPG" => upgrade = false,
            _ => {}
        }
    }
    let relate = relate.ok_or("DEFINE ALTERNATEINDEX needs RELATE")?;
    let (k, r) = sizes(object, data)?;
    let (keys, record_size) = (k.unwrap_or(DEFAULT_KEYS), r.unwrap_or(DEFAULT_AIX_RECORD_SIZE));
    Ok(AlternateIndex { name, relate, keys, unique, upgrade, record_size })
}

fn define_path(object: &[Token]) -> Result<Path, String> {
    let name = object_name(object)?.ok_or("DEFINE PATH needs NAME")?;
    let (mut entry, mut update) = (None, true);
    for t in object {
        match t {
            Token::Keyed(k, v) if k == "PATHENTRY" || k == "PENT" => entry = Some(name_of(v)?),
            Token::Word(w) if w == "UPDATE" || w == "UPD" => update = true,
            Token::Word(w) if w == "NOUPDATE" || w == "NUPD" => update = false,
            Token::Keyed(k, _) | Token::Word(k) if k == "RECATALOG" || k == "RCTLG" => return Err("DEFINE PATH RECATALOG is not supported yet".into()),
            _ => {}
        }
    }
    let entry = entry.ok_or("DEFINE PATH needs PATHENTRY")?;
    Ok(Path { name, entry, update })
}

fn define(args: &[Token]) -> Result<Command, String> {
    let (mut object, mut data) = (None, Vec::new());
    for a in args {
        match a {
            Token::Keyed(k, v) if k == "GENERATIONDATAGROUP" || k == "GDG" => return define_gdg(v),
            Token::Keyed(k, v) => {
                let kind = match k.as_str() {
                    "CLUSTER" | "CL" => Object::Cluster,
                    "ALTERNATEINDEX" | "AIX" => Object::AlternateIndex,
                    "PATH" => Object::Path,
                    "DATA" => {
                        data = tokens(v)?;
                        continue;
                    }
                    "INDEX" | "IX" | "CATALOG" | "CAT" => continue,
                    k => return Err(format!("DEFINE {k} is not supported yet; DEFINE CLUSTER, ALTERNATEINDEX, PATH and GDG are")),
                };
                object = Some((kind, tokens(v)?));
            }
            Token::Word(k) => return Err(format!("DEFINE {k} is not supported yet; DEFINE CLUSTER, ALTERNATEINDEX, PATH and GDG are")),
            other => return Err(format!("DEFINE has {other:?} where a parameter belongs")),
        }
    }
    match object {
        Some((Object::Cluster, o)) => Ok(Command::DefineCluster(define_cluster(&o, &data)?)),
        Some((Object::AlternateIndex, o)) => Ok(Command::DefineAlternateIndex(define_alternate_index(&o, &data)?)),
        Some((Object::Path, o)) => Ok(Command::DefinePath(define_path(&o)?)),
        None => Err("DEFINE needs CLUSTER, ALTERNATEINDEX, PATH or GDG".into()),
    }
}

/// The commands in SYSIN's cards.
pub fn parse(cards: &[String]) -> Result<Vec<Command>, String> {
    let mut p = Parser { tokens: tokens(&command_text(cards)?)?, at: 0 };
    p.commands(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cards(text: &str) -> Vec<String> {
        text.lines().map(|l| format!(" {l}")).collect()
    }

    #[test]
    fn commands_continue_and_comments_are_dropped() {
        let c = parse(&cards("/* clean up first */\nDELETE (PROD.A PROD.B) -\n   PURGE\nSET MAXCC = 0\nREPRO INFILE(IN) -\n  OUTDATASET(PROD.COPY)\nDEF CLUSTER (NAME(PROD.KSDS) INDEXED -\n KEYS(8 0)) DATA(NAME(PROD.KSDS.DATA))\nDELETE PROD.LIB(MEM1)")).unwrap();
        assert_eq!(c, [
            Command::Delete(vec!["PROD.A".into(), "PROD.B".into()]),
            Command::Set { max: true, value: 0 },
            Command::Repro { from: Target::Dd("IN".into()), to: Target::Dataset("PROD.COPY".into()) },
            Command::DefineCluster(Cluster { name: "PROD.KSDS".into(), organization: Organization::Indexed, keys: Some(Keys { length: 8, offset: 0 }), record_size: DEFAULT_RECORD_SIZE }),
            Command::Delete(vec!["PROD.LIB(MEM1)".into()]),
        ]);
        assert_eq!(parse(&cards("DEFINE GDG (NAME(PROD.DAILY) LIMIT(3) SCRATCH EMPTY)")).unwrap(), [Command::DefineGdg { name: "PROD.DAILY".into(), limit: 3, scratch: true, empty: true }]);
    }

    #[test]
    fn if_then_else_and_do_groups() {
        let c = parse(&cards("IF LASTCC > 4 THEN -\n  SET MAXCC = 16\nELSE DO\n  DELETE X.Y\n  SET LASTCC=0\nEND\nIF MAXCC EQ 0 THEN SET MAXCC=4")).unwrap();
        assert_eq!(c.len(), 2);
        assert!(matches!(&c[0], Command::If { max: false, op: Op::Gt, value: 4, then, otherwise } if then.len() == 1 && otherwise.len() == 2));
        assert!(matches!(&c[1], Command::If { max: true, op: Op::Eq, value: 0, then, otherwise } if then.len() == 1 && otherwise.is_empty()));
    }

    #[test]
    fn alternate_indexes_paths_and_their_build() {
        let c = parse(&cards(concat!(
            "DEFINE CLUSTER(NAME(PAY.EMP) NUMBERED RECORDSIZE(40 40))\n",
            "DEFINE AIX(NAME(PAY.EMP.AIX) RELATE(PAY.KSDS) -\n",
            "  KEYS(10 8) UNIQUEKEY NOUPGRADE)\n",
            "DEFINE ALTERNATEINDEX(NAME(PAY.EMP.AIX2) RELATE(PAY.KSDS)) -\n",
            "  DATA(KEYS(6 2) RECSZ(30 300))\n",
            "DEFINE PATH(NAME(PAY.EMP.PATH) PATHENTRY(PAY.EMP.AIX) NOUPDATE)\n",
            "BLDINDEX INDATASET(PAY.KSDS) OUTFILE(AIXDD AIX2DD) INTERNALSORT\n",
        )))
        .unwrap();
        assert_eq!(c, [
            Command::DefineCluster(Cluster { name: "PAY.EMP".into(), organization: Organization::Numbered, keys: None, record_size: RecordSize { average: 40, maximum: 40 } }),
            Command::DefineAlternateIndex(AlternateIndex { name: "PAY.EMP.AIX".into(), relate: "PAY.KSDS".into(), keys: Keys { length: 10, offset: 8 }, unique: true, upgrade: false, record_size: DEFAULT_AIX_RECORD_SIZE }),
            Command::DefineAlternateIndex(AlternateIndex { name: "PAY.EMP.AIX2".into(), relate: "PAY.KSDS".into(), keys: Keys { length: 6, offset: 2 }, unique: false, upgrade: true, record_size: RecordSize { average: 30, maximum: 300 } }),
            Command::DefinePath(Path { name: "PAY.EMP.PATH".into(), entry: "PAY.EMP.AIX".into(), update: false }),
            Command::Bldindex { from: Target::Dataset("PAY.KSDS".into()), to: vec![Target::Dd("AIXDD".into()), Target::Dd("AIX2DD".into())] },
        ]);
    }

    #[test]
    fn print_and_listcat_take_their_ranges_formats_and_selections() {
        let c = parse(&cards("PRINT INDATASET(PAY.KSDS) CHAR FROMKEY('A''B') -\n  TOKEY(X'C1FF') OUTFILE(LIST)\nPRINT INFILE(IN) SKIP(2) COUNT(3)\nLISTCAT ENTRIES(PAY.KSDS PAY.*) ALL\nLISTCAT LEVEL(PAY) GDG CLUSTER NONVSAM\nLISTC")).unwrap();
        assert_eq!(c, [
            Command::Print(Print { from: Target::Dataset("PAY.KSDS".into()), format: PrintFormat::Character, first: First::Key(Key::Chars("A'B".into())), last: Last::Key(Key::Bytes(vec![0xC1, 0xFF])), out: Some("LIST".into()) }),
            Command::Print(Print { from: Target::Dd("IN".into()), format: PrintFormat::Dump, first: First::Skip(2), last: Last::Count(3), out: None }),
            Command::Listcat(Listcat { entries: Entries::Named(vec!["PAY.KSDS".into(), "PAY.*".into()]), types: vec![], all: true, out: None }),
            Command::Listcat(Listcat { entries: Entries::Level("PAY".into()), types: vec![EntryType::Cluster, EntryType::Nonvsam, EntryType::GenerationDataGroup], all: false, out: None }),
            Command::Listcat(Listcat { entries: Entries::All, types: vec![], all: false, out: None }),
        ]);
    }

    #[test]
    fn a_plus_joins_without_a_blank() {
        assert_eq!(parse(&cards("DELETE PROD.LON+\nG.NAME")).unwrap(), [Command::Delete(vec!["PROD.LONG.NAME".into()])]);
    }

    #[test]
    fn what_is_not_modelled_is_refused_by_name() {
        for (text, message) in [
            ("VERIFY FILE(X)", "the IDCAMS command VERIFY is not supported yet"),
            ("PRINT INFILE(X) FROMADDRESS(0)", "PRINT parameter FROMADDRESS is not supported yet"),
            ("PRINT INFILE(X) OUTDATASET(Y.Z)", "PRINT writes to OUTFILE, not OUTDATASET"),
            ("PRINT INFILE(X) FROMKEY(X'C1C')", "X'C1C' is not an even number of hexadecimal digits"),
            ("PRINT CHARACTER", "PRINT needs INFILE or INDATASET"),
            ("LISTCAT ALL HISTORY", "LISTCAT parameter HISTORY is not supported yet"),
            ("LISTCAT USERCATALOG", "LISTCAT parameter USERCATALOG is not supported yet"),
            ("BLDINDEX INDATASET(A.B)", "BLDINDEX needs INFILE or INDATASET, and OUTFILE or OUTDATASET"),
            ("DEFINE AIX(NAME(A.AIX) KEYS(4 0))", "DEFINE ALTERNATEINDEX needs RELATE"),
            ("DEFINE PATH(NAME(A.PATH))", "DEFINE PATH needs PATHENTRY"),
            ("DEFINE CLUSTER(NAME(A.B) KEYS(8 76) RECORDSIZE(80 80))", "A.B: KEYS(8 76) does not fit in a record of 80 bytes"),
            ("DEFINE CLUSTER(NAME(A.B) KEYS(0 0))", "KEYS(0 0) has a length that is not from 1 to 255"),
            ("DEFINE CLUSTER(NAME(A.B) RECORDSIZE(90 80))", "RECORDSIZE(90 80) needs an average from 1 to the maximum"),
            ("DELETE PROD.*", "DELETE of a generic name (PROD.*) is not supported yet"),
            ("REPRO INFILE(A) OUTFILE(B) REPLACE", "REPRO parameter REPLACE is not supported yet"),
            ("REPRO INFILE(A) OUTFILE(B) COUNT(5)", "REPRO parameter COUNT is not supported yet"),
            ("DEFINE ALIAS(NAME(X.Y) RELATE(CAT))", "DEFINE ALIAS is not supported yet; DEFINE CLUSTER, ALTERNATEINDEX, PATH and GDG are"),
            ("DEFINE GDG(NAME(X.Y) LIMIT(300))", "LIMIT(300) is not from 1 to 255"),
            ("DELETE ../ETC", "../ETC is not a data set name"),
            ("SET MAXCC=17", "17 is not a condition code from 0 to 16"),
            ("DO\nDELETE A.B", "DO is out of place"),
            ("/* never closed", "a comment is not closed with */"),
        ] {
            assert_eq!(parse(&cards(text)).unwrap_err(), message, "{text}");
        }
    }
}
