//! DFSORT control statements from SYSIN: SORT and MERGE with FIELDS, SUM FIELDS=NONE, OPTION,
//! RECORD, INCLUDE and OMIT, INREC and OUTREC, and OUTFIL, as DFSORT Application Programming
//! Guide's control-statement chapter writes them, with IFTHEN clauses, edited and converted numeric
//! items, and the symbols a SYMNAMES data set defines. A statement, operand or item these do not
//! model (a SUM of fields, FINDREP, PARSE, arithmetic, dates, exits) is refused by name.

use crate::symnames::Symbols;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Ch,
    Ac,
    Zd,
    Clo,
    Csl,
    Cst,
    Pd,
    Bi,
    Fi,
}

/// One control field: `length` bytes from `position` (1 is the record's first byte, the RDW's
/// first for a variable-length record).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    pub position: usize,
    pub length: usize,
    pub format: Format,
    pub ascending: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Sort,
    Merge,
    Copy,
}

/// RECORD TYPE=F|V and the first LENGTH value: the input's record format when its DD gives none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Record {
    pub variable: bool,
    pub length: Option<usize>,
}

/// A field a condition compares: `length` bytes from `position`, 1 the first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Area {
    pub position: usize,
    pub length: usize,
    pub format: Format,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Constant {
    /// C'...': padded with blanks or truncated on the right to the field's length.
    Chars(String),
    /// X'...': padded with X'00' or truncated on the right.
    Hex(Vec<u8>),
    /// n, +n or -n: padded with zeros or truncated on the left.
    Decimal(i128),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operand {
    Field(Area),
    Constant(Constant),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Condition {
    Compare { left: Area, relation: Relation, right: Operand },
    And(Box<Condition>, Box<Condition>),
    Or(Box<Condition>, Box<Condition>),
    /// COND=ALL or COND=NONE.
    Always(bool),
}

/// INCLUDE keeps the records its condition holds for; OMIT drops them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub include: bool,
    pub condition: Condition,
}

/// The formats a numeric item reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberFormat {
    Bi,
    Fi,
    Pd,
    /// Packed decimal with its first digit and sign ignored.
    Pd0,
    Zd,
    /// CSF or FS: digits with an optional leading floating sign.
    Fs,
    /// Unsigned free form: every digit in the field, right to left.
    Uff,
    /// Signed free form: as UFF, negative when the field holds - or ).
    Sff,
}

/// What a numeric item reads: a field, or a signed decimal constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Numeric {
    Field { position: usize, length: usize, format: NumberFormat },
    Constant(i128),
}

/// An edit mask: one of M0-M26, or a pattern of insignificant and significant digit characters,
/// sign characters and anything else printed as it stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mask {
    Predefined(u8),
    Pattern { text: String, insignificant: char, significant: char, sign: char },
}

/// The formats TO converts a number to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToFormat {
    Bi,
    Fi,
    Pd,
    Pdc,
    Pdf,
    Zd,
    Zdf,
    Zdc,
    Fs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    /// Edited to characters. `signs` are lp, ln, tp, tn; None leaves the mask's own.
    Edit { mask: Mask, signs: Option<[Option<char>; 4]> },
    To(ToFormat),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    /// `length` bytes from `position`; with no length, the rest of a variable-length record.
    Field { position: usize, length: Option<usize> },
    Blanks(usize),
    Zeros(usize),
    Chars(String),
    Hex(Vec<u8>),
    /// A number edited or converted; `length` overrides the length the output implies.
    Number { value: Numeric, output: Output, length: Option<usize> },
}

/// A reformatting item, placed at `column` (1 the first) or after the item before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub column: Option<usize>,
    pub piece: Piece,
}

/// What WHEN=GROUP puts in each record of a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pushed {
    /// A field of the group's first record.
    Field { position: usize, length: usize },
    /// The group's number, from 1, as n zoned digits.
    Id(usize),
    /// The record's number in its group, from 1, as n zoned digits.
    Seq(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Push {
    pub column: Option<usize>,
    pub value: Pushed,
}

/// How WHEN=GROUP finds its groups: a record that starts one, a key that changes, a record that
/// ends one, or a count of records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub begin: Option<Condition>,
    pub key: Option<(usize, usize)>,
    pub end: Option<Condition>,
    pub records: Option<usize>,
    pub push: Vec<Push>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum When {
    Init,
    Group(Group),
    Condition(Condition),
    /// After one of the WHEN=(cond) clauses since the last WHEN=ANY held.
    Any,
    /// When no WHEN=(cond) clause held.
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clause {
    pub when: When,
    pub edit: Option<Edit>,
    /// HIT=NEXT: the clauses after this one are tried even when it holds.
    pub hit_next: bool,
}

/// BUILD (or FIELDS) makes a new record of its items; OVERLAY writes them over the record; IFTHEN
/// applies its clauses in DFSORT's order, and `length` is IFOUTLEN.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    Build(Vec<Item>),
    Overlay(Vec<Item>),
    IfThen { clauses: Vec<Clause>, length: Option<usize> },
}

/// An OUTFIL group: the DDs it writes, the records it selects, and how it reformats them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outfil {
    pub names: Vec<String>,
    pub selection: Option<Selection>,
    /// The records no other group selects.
    pub save: bool,
    pub edit: Option<Edit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Control {
    pub kind: Kind,
    pub fields: Vec<Field>,
    /// SUM FIELDS=NONE: of records with equal keys, the first is kept.
    pub drop_duplicates: bool,
    pub record: Option<Record>,
    pub selection: Option<Selection>,
    pub inrec: Option<Edit>,
    pub outrec: Option<Edit>,
    pub outfil: Vec<Outfil>,
}

const OPTION_IGNORED: &[&str] = &["EQUALS", "NOEQUALS", "DYNALLOC", "FILSZ", "SIZE", "MAINSIZE", "AVGRLEN", "MSGPRT", "LIST", "NOLIST", "LISTX", "NOLISTX", "STOPAFT"];
const REFUSED: &[&str] = &["MODS", "ALTSEQ", "JOINKEYS", "JOIN", "REFORMAT", "DEBUG", "ALTER"];
const ARITHMETIC: &[&str] = &["ADD", "SUB", "MUL", "DIV", "MOD", "MIN", "MAX"];

/// Each statement's text: its verb and operand field, comment cards (`*` in column 1) dropped, and
/// a card whose operands end in a comma continued on the next. An operand field ends at the first
/// blank outside apostrophes; what follows is a comment.
fn statements(cards: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut continuing = false;
    for card in cards {
        if card.starts_with('*') {
            continue;
        }
        let text: String = card.chars().take(71).collect();
        let body = text.trim();
        if body.is_empty() {
            continue;
        }
        if continuing {
            if let Some(last) = out.last_mut() {
                last.push_str(operand_field(body));
            }
        } else {
            let (verb, rest) = body.split_once(' ').unwrap_or((body, ""));
            out.push(format!("{verb} {}", operand_field(rest.trim_start())).trim_end().to_string());
        }
        continuing = out.last().is_some_and(|s| s.ends_with(','));
    }
    out
}

fn operand_field(text: &str) -> &str {
    let mut quoted = false;
    for (i, c) in text.char_indices() {
        match c {
            '\'' => quoted = !quoted,
            ' ' if !quoted => return &text[..i],
            _ => {}
        }
    }
    text
}

/// `text` split at the commas outside parentheses and apostrophes.
fn tokens(text: &str) -> Vec<String> {
    let (mut out, mut cur, mut depth, mut quoted) = (Vec::new(), String::new(), 0i32, false);
    for c in text.chars() {
        match c {
            '\'' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => depth -= 1,
            ',' if depth == 0 && !quoted => {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn parenthesized(value: &str) -> Option<&str> {
    value.strip_prefix('(').and_then(|v| v.strip_suffix(')'))
}

fn format_of(word: &str) -> Result<Format, String> {
    Ok(match word {
        "CH" => Format::Ch,
        "AC" => Format::Ac,
        "ZD" => Format::Zd,
        "CLO" | "OL" => Format::Clo,
        "CSL" | "LS" => Format::Csl,
        "CST" | "TS" => Format::Cst,
        "PD" => Format::Pd,
        "BI" => Format::Bi,
        "FI" => Format::Fi,
        f => return Err(format!("the field format {f} is not supported yet")),
    })
}

fn number_format(word: &str) -> Option<NumberFormat> {
    Some(match word {
        "BI" => NumberFormat::Bi,
        "FI" => NumberFormat::Fi,
        "PD" => NumberFormat::Pd,
        "PD0" => NumberFormat::Pd0,
        "ZD" => NumberFormat::Zd,
        "CSF" | "FS" => NumberFormat::Fs,
        "UFF" => NumberFormat::Uff,
        "SFF" => NumberFormat::Sff,
        _ => return None,
    })
}

fn to_format(word: &str) -> Option<ToFormat> {
    Some(match word {
        "BI" => ToFormat::Bi,
        "FI" => ToFormat::Fi,
        "PD" => ToFormat::Pd,
        "PDC" => ToFormat::Pdc,
        "PDF" => ToFormat::Pdf,
        "ZD" => ToFormat::Zd,
        "ZDF" => ToFormat::Zdf,
        "ZDC" => ToFormat::Zdc,
        "CSF" | "FS" => ToFormat::Fs,
        _ => return None,
    })
}

/// The longest field each number format reads, in bytes (Table 7 of OUTFIL OUTREC).
fn longest_number(format: NumberFormat) -> std::ops::RangeInclusive<usize> {
    match format {
        NumberFormat::Bi | NumberFormat::Fi => 1..=8,
        NumberFormat::Pd => 1..=16,
        NumberFormat::Pd0 => 2..=8,
        NumberFormat::Zd => 1..=31,
        NumberFormat::Fs => 1..=32,
        NumberFormat::Uff | NumberFormat::Sff => 1..=44,
    }
}

/// Formats DFSORT edits and converts that these items do not read: dates, times, floating point.
fn unmodelled_number(word: &str) -> bool {
    matches!(word, "FL" | "DC1" | "DC2" | "DC3" | "DE1" | "DE2" | "DE3" | "DT1" | "DT2" | "DT3" | "TC1" | "TC2" | "TC3" | "TC4" | "TE1" | "TE2" | "TE3" | "TE4" | "TM1" | "TM2" | "TM3" | "TM4")
        || ((word.starts_with("Y2") || word.starts_with("Y4")) && (3..=4).contains(&word.len()))
}

fn number(text: &str, what: &str) -> Result<usize, String> {
    match text.parse::<usize>() {
        Ok(n) if n > 0 => Ok(n),
        _ => Err(format!("{text} is not a {what}")),
    }
}

fn is_number(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

fn relation(word: &str) -> Option<Relation> {
    Some(match word {
        "EQ" => Relation::Eq,
        "NE" => Relation::Ne,
        "GT" => Relation::Gt,
        "GE" => Relation::Ge,
        "LT" => Relation::Lt,
        "LE" => Relation::Le,
        _ => return None,
    })
}

/// The text between the apostrophes of `prefix'...'`, a doubled apostrophe standing for one.
fn quoted(token: &str, prefix: &str) -> Option<String> {
    let inner = token.strip_prefix(prefix)?.strip_prefix('\'')?.strip_suffix('\'')?;
    Some(inner.replace("''", "'"))
}

fn hex(text: &str) -> Result<Vec<u8>, String> {
    if !text.len().is_multiple_of(2) || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("X'{text}' is not pairs of hexadecimal digits"));
    }
    Ok((0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("checked hexadecimal")).collect())
}

/// +n or -n with 1 to 31 digits.
fn signed_decimal(token: &str) -> Option<i128> {
    let digits = token.strip_prefix(['+', '-'])?;
    if digits.is_empty() || digits.len() > 31 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: i128 = digits.parse().ok()?;
    Some(if token.starts_with('-') { -n } else { n })
}

fn constant(token: &str) -> Result<Constant, String> {
    if let Some(text) = quoted(token, "C") {
        return Ok(Constant::Chars(text));
    }
    if let Some(text) = quoted(token, "X") {
        return Ok(Constant::Hex(hex(&text)?));
    }
    let digits = token.strip_prefix(['+', '-']).unwrap_or(token);
    if is_number(digits) && digits.len() <= 31 {
        let n: i128 = digits.parse().map_err(|_| format!("{token} is not a decimal constant"))?;
        return Ok(Constant::Decimal(if token.starts_with('-') { -n } else { n }));
    }
    Err(format!("the constant {token} is not supported yet; C'...', X'...' and decimal numbers are"))
}

/// The longest field each format compares, in bytes.
fn longest(format: Format) -> usize {
    match format {
        Format::Zd => 31,
        Format::Pd => 16,
        Format::Fi => 8,
        _ => 256,
    }
}

fn name(format: Format) -> &'static str {
    match format {
        Format::Ch => "CH",
        Format::Ac => "AC",
        Format::Zd => "ZD",
        Format::Clo => "CLO",
        Format::Csl => "CSL",
        Format::Cst => "CST",
        Format::Pd => "PD",
        Format::Bi => "BI",
        Format::Fi => "FI",
    }
}

/// nX, nZ, nC'...' or nX'...', with n 1 when it is left out.
fn literal(token: &str) -> Result<Option<Piece>, String> {
    let digits = token.bytes().take_while(u8::is_ascii_digit).count();
    let times = if digits == 0 { 1 } else { number(&token[..digits], "repetition")? };
    let rest = &token[digits..];
    Ok(Some(match rest {
        "X" => Piece::Blanks(times),
        "Z" => Piece::Zeros(times),
        _ => match (quoted(rest, "C"), quoted(rest, "X")) {
            (Some(text), _) => Piece::Chars(text.repeat(times)),
            (_, Some(text)) => Piece::Hex(hex(&text)?.repeat(times)),
            _ => return Ok(None),
        },
    }))
}

/// Whether `token` begins a reformatting item: a column, a position, a literal, a decimal
/// constant, a parenthesized field, or one of the items these do not model (dates, times,
/// sequence numbers, parsed fields, a new line).
fn begins_item(token: &str) -> bool {
    token.split_once(':').is_some_and(|(c, _)| is_number(c))
        || is_number(token)
        || signed_decimal(token).is_some()
        || (token.starts_with('(') && token.ends_with(')'))
        || literal(token).is_ok_and(|l| l.is_some())
        || ["DATE", "TIME", "SEQNUM", "%", "/"].iter().any(|p| token.starts_with(p))
}

/// Whether `token` edits or converts the number before it. A bare format after a symbol is the
/// symbol's format, as DFSORT reads it, which is why TO= is written there.
fn edits(token: &str) -> bool {
    mask_number(token).is_some()
        || ["EDIT=", "SIGNS=", "LENGTH=", "TO="].iter().any(|p| token.starts_with(p))
        || (token.len() > 5 && token.starts_with("ED") && token.as_bytes()[4] == b'=')
        || (token.len() > 6 && token.starts_with("SIGN") && token.as_bytes()[5] == b'=')
}

/// n of Mn, M0 to M26.
fn mask_number(token: &str) -> Option<u8> {
    let n = token.strip_prefix('M')?;
    if n.is_empty() || n.len() > 2 || !n.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    n.parse::<u8>().ok().filter(|&n| n <= 26)
}

/// A pattern as EDIT=(...) or EDIT=('...') writes it.
fn pattern_text(value: &str) -> Result<String, String> {
    let inner = parenthesized(value).ok_or_else(|| format!("the edit pattern {value} is not in parentheses"))?;
    let text = match inner.strip_prefix('\'').and_then(|i| i.strip_suffix('\'')) {
        Some(q) => q.replace("''", "'"),
        None => inner.to_string(),
    };
    if text.chars().count() > 44 {
        return Err(format!("the edit pattern {text} is longer than 44 characters"));
    }
    Ok(text)
}

/// SIGNS=(lp,ln,tp,tn): each a character, quoted where it is a comma, a blank or a parenthesis.
fn signs(value: &str) -> Result<[Option<char>; 4], String> {
    let inner = parenthesized(value).ok_or_else(|| format!("SIGNS={value} is not in parentheses"))?;
    let parts = tokens(inner);
    if parts.len() > 4 {
        return Err(format!("SIGNS=({inner}) has more than four signs"));
    }
    let mut out = [None; 4];
    for (slot, part) in out.iter_mut().zip(&parts) {
        let text = match part.strip_prefix('\'').and_then(|p| p.strip_suffix('\'')) {
            Some(q) => q.replace("''", "'"),
            None => part.clone(),
        };
        let mut chars = text.chars();
        *slot = match (chars.next(), chars.next()) {
            (None, _) => None,
            (Some(c), None) => Some(c),
            _ => return Err(format!("the sign {part} is not one character")),
        };
    }
    Ok(out)
}

/// Reads control statements, substituting the SYMNAMES symbols where a field, a constant or a
/// column is taken.
struct Reader<'s> {
    symbols: &'s Symbols,
}

impl Reader<'_> {
    /// FIELDS=(p,l,f,o,...), (p,l,o,...) with FORMAT=f, or COPY.
    fn fields(&self, value: &str, format: Option<Format>) -> Result<Option<Vec<Field>>, String> {
        if value == "COPY" {
            return Ok(None);
        }
        let inner = parenthesized(value).ok_or_else(|| format!("FIELDS={value} is not in parentheses"))?;
        let mut parts = Vec::new();
        for p in inner.split(',').map(str::trim) {
            match self.symbols.in_sort_fields(p, format.is_some())? {
                Some(expanded) => parts.extend(expanded),
                None => parts.push(p.to_string()),
            }
        }
        let width = if format.is_some() { 3 } else { 4 };
        if parts.is_empty() || !parts.len().is_multiple_of(width) {
            return Err(format!("FIELDS=({inner}) is not position, length{}, order for each field", if format.is_some() { "" } else { ", format" }));
        }
        let mut out = Vec::new();
        for group in parts.chunks(width) {
            let format = match format {
                Some(f) => f,
                None => format_of(&group[2])?,
            };
            let ascending = match group[width - 1].as_str() {
                "A" => true,
                "D" => false,
                "E" => return Err("an E order (an exit's own) is not supported yet".into()),
                o => return Err(format!("{o} is not A or D")),
            };
            out.push(Field { position: number(&group[0], "position")?, length: number(&group[1], "length")?, format, ascending });
        }
        Ok(Some(out))
    }

    /// The tokens of a condition with each symbol replaced by its field or constant.
    fn expand_condition(&self, raw: Vec<String>, format_given: bool) -> Result<Vec<String>, String> {
        let mut out = Vec::new();
        for t in raw {
            let keyword = relation(&t).is_some() || matches!(t.as_str(), "AND" | "&" | "OR" | "|");
            match if keyword { None } else { self.symbols.in_condition(&t, format_given)? } {
                Some(expanded) => out.extend(expanded),
                None => out.push(t),
            }
        }
        Ok(out)
    }

    /// A comparison's left or right field: position, length and, unless FORMAT= gives it, format.
    fn area(&self, t: &[String], at: &mut usize, format: Option<Format>) -> Result<Area, String> {
        let get = |i: usize| t.get(i).map(String::as_str).ok_or("a comparison ends early".to_string());
        let position = number(get(*at)?, "position")?;
        let length = number(get(*at + 1)?, "length")?;
        *at += 2;
        let format = match t.get(*at) {
            Some(w) if relation(w).is_none() && !matches!(w.as_str(), "AND" | "&" | "OR" | "|") => {
                *at += 1;
                format_of(w)?
            }
            _ => format.ok_or_else(|| format!("the field {position},{length} has no format, and no FORMAT= gives one"))?,
        };
        if !matches!(format, Format::Ch | Format::Bi | Format::Fi | Format::Zd | Format::Pd) {
            return Err(format!("the field format {} in a condition is not supported yet; CH, BI, FI, ZD and PD are", name(format)));
        }
        if length > longest(format) {
            return Err(format!("a {} field of {length} bytes is longer than DFSORT compares", name(format)));
        }
        Ok(Area { position, length, format })
    }

    fn comparison(&self, t: &[String], at: &mut usize, format: Option<Format>) -> Result<Condition, String> {
        let left = self.area(t, at, format)?;
        let op = t.get(*at).ok_or("a comparison has no relation")?;
        let relation = relation(op).ok_or_else(|| format!("{op} is not EQ, NE, GT, GE, LT or LE"))?;
        *at += 1;
        let right_starts_field = t.get(*at).is_some_and(|w| is_number(w)) && t.get(*at + 1).is_some_and(|w| is_number(w));
        let right = if right_starts_field {
            Operand::Field(self.area(t, at, format)?)
        } else {
            let token = t.get(*at).ok_or("a comparison has nothing after its relation")?;
            *at += 1;
            Operand::Constant(constant(token)?)
        };
        let allowed = match (&right, left.format) {
            (Operand::Constant(Constant::Chars(_) | Constant::Hex(_)), Format::Ch | Format::Bi) => true,
            (Operand::Constant(Constant::Decimal(n)), Format::Bi) => *n >= 0,
            (Operand::Constant(Constant::Decimal(_)), Format::Fi | Format::Zd | Format::Pd) => true,
            (Operand::Field(r), Format::Ch | Format::Bi) => matches!(r.format, Format::Ch | Format::Bi) && r.length == left.length,
            (Operand::Field(r), Format::Fi) => r.format == Format::Fi && r.length == left.length,
            (Operand::Field(r), Format::Zd | Format::Pd) => matches!(r.format, Format::Zd | Format::Pd),
            _ => false,
        };
        if !allowed {
            return Err(format!("comparing {},{},{} with {} is not supported yet", left.position, left.length, name(left.format), match &right {
                Operand::Field(r) => format!("{},{},{}", r.position, r.length, name(r.format)),
                Operand::Constant(Constant::Chars(_)) => "a character string".into(),
                Operand::Constant(Constant::Hex(_)) => "a hexadecimal string".into(),
                Operand::Constant(Constant::Decimal(n)) => format!("the decimal number {n}"),
            }));
        }
        Ok(Condition::Compare { left, relation, right })
    }

    fn factor(&self, t: &[String], at: &mut usize, format: Option<Format>) -> Result<Condition, String> {
        let token = t.get(*at).ok_or("a condition ends early")?;
        if let Some(inner) = parenthesized(token) {
            *at += 1;
            return self.expression_of(inner, format);
        }
        self.comparison(t, at, format)
    }

    /// Comparisons joined by AND, which DFSORT evaluates before OR.
    fn term(&self, t: &[String], at: &mut usize, format: Option<Format>) -> Result<Condition, String> {
        let mut left = self.factor(t, at, format)?;
        while t.get(*at).is_some_and(|w| w == "AND" || w == "&") {
            *at += 1;
            left = Condition::And(Box::new(left), Box::new(self.factor(t, at, format)?));
        }
        Ok(left)
    }

    fn expression_of(&self, text: &str, format: Option<Format>) -> Result<Condition, String> {
        let t = self.expand_condition(tokens(text), format.is_some())?;
        let mut at = 0;
        let mut left = self.term(&t, &mut at, format)?;
        while t.get(at).is_some_and(|w| w == "OR" || w == "|") {
            at += 1;
            left = Condition::Or(Box::new(left), Box::new(self.term(&t, &mut at, format)?));
        }
        match t.get(at) {
            Some(extra) => Err(format!("{extra} in a condition is not AND or OR")),
            None => Ok(left),
        }
    }

    /// COND=(...), ALL or NONE.
    fn condition(&self, value: &str, format: Option<Format>) -> Result<Condition, String> {
        match value {
            "ALL" | "(ALL)" => return Ok(Condition::Always(true)),
            "NONE" | "(NONE)" => return Ok(Condition::Always(false)),
            _ => {}
        }
        let inner = parenthesized(value).ok_or_else(|| format!("COND={value} is not in parentheses"))?;
        self.expression_of(inner, format)
    }

    /// The tokens of a list of items with each symbol replaced: c: for a column, p,m,f before an
    /// edit or conversion, p,m otherwise, p for a position, and a constant as itself.
    fn expand_items(&self, raw: Vec<String>) -> Result<Vec<String>, String> {
        let mut out = Vec::new();
        for (i, t) in raw.iter().enumerate() {
            let (column, rest) = match t.split_once(':') {
                Some((c, rest)) if !c.contains('\'') => match self.symbols.column(c) {
                    Some(at) => (Some(at.to_string()), rest.to_string()),
                    None => (Some(c.to_string()), rest.to_string()),
                },
                _ => (None, t.clone()),
            };
            let edited = raw.get(i + 1).is_some_and(|n| edits(n));
            let mut parts = match self.symbols.in_items(&rest, edited)? {
                Some(expanded) => expanded,
                None => vec![rest],
            };
            if let Some(c) = column {
                parts[0] = format!("{c}:{}", parts[0]);
            }
            out.extend(parts);
        }
        Ok(out)
    }

    /// The edit or conversion after a number, from `t[*i]` on: Mn, EDIT=, EDxy=, SIGNS=, SIGNz=,
    /// LENGTH=, TO= or a bare output format.
    fn number_output(&self, t: &[String], i: &mut usize, what: &str) -> Result<(Output, Option<usize>), String> {
        let (mut mask, mut signed, mut sign_char, mut to, mut length) = (None, None, 'S', None, None);
        while let Some(token) = t.get(*i) {
            let (key, value) = token.split_once('=').unwrap_or((token.as_str(), ""));
            if let Some(n) = mask_number(token) {
                mask = Some(Mask::Predefined(n));
            } else if let (Some(f), true) = (to_format(token), value.is_empty() && to.is_none() && length.is_none()) {
                to = Some(f);
            } else if key == "TO" {
                to = Some(to_format(parenthesized(value).unwrap_or(value)).ok_or_else(|| format!("TO={value} is not BI, FI, PD, PDC, PDF, ZD, ZDF, ZDC, CSF or FS"))?);
            } else if key == "LENGTH" {
                let n = number(value, "length")?;
                if n > 44 {
                    return Err(format!("LENGTH={n} is longer than 44"));
                }
                length = Some(n);
            } else if key == "EDIT" {
                mask = Some(Mask::Pattern { text: pattern_text(value)?, insignificant: 'I', significant: 'T', sign: 'S' });
            } else if key.len() == 4 && key.starts_with("ED") {
                let (x, y) = (key.as_bytes()[2] as char, key.as_bytes()[3] as char);
                if x == y {
                    return Err(format!("{key}: the two digit characters must differ"));
                }
                mask = Some(Mask::Pattern { text: pattern_text(value)?, insignificant: x, significant: y, sign: 'S' });
            } else if key == "SIGNS" {
                signed = Some(signs(value)?);
            } else if key.len() == 5 && key.starts_with("SIGN") {
                sign_char = key.as_bytes()[4] as char;
                signed = Some(signs(value)?);
            } else if ARITHMETIC.contains(&key) {
                return Err(format!("arithmetic in {what} ({key}) is not supported yet"));
            } else {
                break;
            }
            *i += 1;
        }
        if let Some(Mask::Pattern { sign, .. }) = &mut mask {
            *sign = sign_char;
        }
        match (to, mask) {
            (Some(_), Some(_)) => Err(format!("a {what} number is either edited or converted, not both")),
            (Some(f), None) if signed.is_none() => Ok((Output::To(f), length)),
            (Some(_), None) => Err(format!("SIGNS goes with an edit mask, not TO, in {what}")),
            (None, mask) => Ok((Output::Edit { mask: mask.unwrap_or(Mask::Predefined(0)), signs: signed }, length)),
        }
    }

    /// The items of BUILD=(...), FIELDS=(...) or OVERLAY=(...).
    fn items(&self, value: &str, what: &str) -> Result<Vec<Item>, String> {
        let inner = parenthesized(value).ok_or_else(|| format!("{what}={value} is not in parentheses"))?;
        let t = self.expand_items(tokens(inner))?;
        let mut out = Vec::new();
        let mut i = 0;
        while i < t.len() {
            let (column, token) = match t[i].split_once(':') {
                Some((c, rest)) if is_number(c) => (Some(number(c, "column")?), rest.to_string()),
                Some((c, _)) if !c.contains('\'') => return Err(format!("{c} is not a column or a symbol for one")),
                _ => (None, t[i].clone()),
            };
            i += 1;
            let field = match parenthesized(&token) {
                Some(inner) if inner.split(',').count() == 3 => {
                    let p: Vec<&str> = inner.split(',').collect();
                    if number_format(p[2]).is_none() && !unmodelled_number(p[2]) {
                        return Err(format!("the {what} item {token} is not supported yet"));
                    }
                    Some((number(p[0], "position")?, Some(number(p[1], "length")?), Some(p[2].to_string())))
                }
                _ if is_number(&token) => {
                    let position = number(&token, "position")?;
                    let length = match t.get(i) {
                        Some(l) if is_number(l) => {
                            i += 1;
                            Some(number(l, "length")?)
                        }
                        _ => None,
                    };
                    let format = match t.get(i) {
                        Some(f) if length.is_some() && (number_format(f).is_some() || unmodelled_number(f)) => {
                            i += 1;
                            Some(f.clone())
                        }
                        _ => None,
                    };
                    Some((position, length, format))
                }
                _ => None,
            };
            let piece = if let Some((position, length, format)) = field {
                match (format, length) {
                    (Some(f), Some(length)) => {
                        if unmodelled_number(&f) {
                            return Err(format!("{what} editing of {f} fields is not supported yet"));
                        }
                        let format = number_format(&f).expect("a checked number format");
                        if !longest_number(format).contains(&length) {
                            return Err(format!("a {f} field of {length} bytes is not one {what} edits"));
                        }
                        let (output, out_length) = self.number_output(&t, &mut i, what)?;
                        Piece::Number { value: Numeric::Field { position, length, format }, output, length: out_length }
                    }
                    _ => {
                        if let Some(next) = t.get(i).filter(|n| !begins_item(n)) {
                            if ARITHMETIC.contains(&next.as_str()) {
                                return Err(format!("arithmetic in {what} ({next}) is not supported yet"));
                            }
                            return Err(format!("{what} field conversion and editing ({next}) is not supported yet"));
                        }
                        Piece::Field { position, length }
                    }
                }
            } else if let Some(n) = signed_decimal(&token) {
                let (output, out_length) = self.number_output(&t, &mut i, what)?;
                Piece::Number { value: Numeric::Constant(n), output, length: out_length }
            } else {
                match literal(&token)? {
                    Some(p) => p,
                    None => return Err(format!("the {what} item {token} is not supported yet")),
                }
            };
            out.push(Item { column, piece });
        }
        if out.is_empty() {
            return Err(format!("{what}=() has no items"));
        }
        Ok(out)
    }

    /// PUSH=(c:p,m, c:ID=n, c:SEQ=n, ...).
    fn push(&self, value: &str) -> Result<Vec<Push>, String> {
        let inner = parenthesized(value).ok_or_else(|| format!("PUSH={value} is not in parentheses"))?;
        let t = self.expand_items(tokens(inner))?;
        let mut out = Vec::new();
        let mut i = 0;
        while i < t.len() {
            let (column, token) = match t[i].split_once(':') {
                Some((c, rest)) if is_number(c) => (Some(number(c, "column")?), rest.to_string()),
                _ => (None, t[i].clone()),
            };
            i += 1;
            let digits = |v: &str, what: &str| -> Result<usize, String> {
                let n = number(v, what)?;
                if n > 15 {
                    return Err(format!("{what}={n} is longer than 15 digits"));
                }
                Ok(n)
            };
            let value = if let Some(n) = token.strip_prefix("ID=") {
                Pushed::Id(digits(n, "ID")?)
            } else if let Some(n) = token.strip_prefix("SEQ=") {
                Pushed::Seq(digits(n, "SEQ")?)
            } else if is_number(&token) && t.get(i).is_some_and(|l| is_number(l)) {
                let length = number(&t[i], "length")?;
                i += 1;
                Pushed::Field { position: number(&token, "position")?, length }
            } else {
                return Err(format!("the PUSH item {token} is not supported yet; p,m, ID=n and SEQ=n are"));
            };
            out.push(Push { column, value });
        }
        if out.is_empty() {
            return Err("PUSH=() has no items".into());
        }
        Ok(out)
    }

    /// One IFTHEN=(...) clause.
    fn clause(&self, value: &str, verb: &str) -> Result<Clause, String> {
        let inner = parenthesized(value).ok_or_else(|| format!("IFTHEN={value} is not in parentheses"))?;
        let ops = tokens(inner);
        let when = ops.first().and_then(|w| w.strip_prefix("WHEN=")).ok_or_else(|| format!("IFTHEN=({inner}) does not begin with WHEN="))?;
        let mut group = Group { begin: None, key: None, end: None, records: None, push: Vec::new() };
        let mut clause = Clause {
            when: match when {
                "INIT" => When::Init,
                "NONE" => When::None,
                "ANY" => When::Any,
                "GROUP" => When::Group(group.clone()),
                cond => When::Condition(self.condition(cond, None)?),
            },
            edit: None,
            hit_next: false,
        };
        for o in &ops[1..] {
            let (key, value) = o.split_once('=').unwrap_or((o.as_str(), ""));
            let grouping = matches!(clause.when, When::Group(_));
            let edit = match key {
                "BUILD" if !grouping => Edit::Build(self.items(value, "BUILD")?),
                "OVERLAY" if !grouping => Edit::Overlay(self.items(value, "OVERLAY")?),
                "HIT" if value == "NEXT" && matches!(clause.when, When::Condition(_) | When::Any) => {
                    clause.hit_next = true;
                    continue;
                }
                "BEGIN" if grouping => {
                    group.begin = Some(self.condition(value, None)?);
                    continue;
                }
                "END" if grouping => {
                    group.end = Some(self.condition(value, None)?);
                    continue;
                }
                "KEYBEGIN" if grouping => {
                    let p = self.expand_items(tokens(parenthesized(value).unwrap_or(value)))?;
                    let [position, length] = p.as_slice() else { return Err(format!("KEYBEGIN={value} is not (p,m)")) };
                    group.key = Some((number(position, "position")?, number(length, "length")?));
                    continue;
                }
                "RECORDS" if grouping => {
                    group.records = Some(number(value, "count of records")?);
                    continue;
                }
                "PUSH" if grouping => {
                    group.push = self.push(value)?;
                    continue;
                }
                "PARSE" | "FINDREP" => return Err(format!("{verb} IFTHEN {key} is not supported yet")),
                k => return Err(format!("{k} is not an operand of this {verb} IFTHEN clause")),
            };
            if clause.edit.replace(edit).is_some() {
                return Err(format!("an IFTHEN clause takes one of BUILD and OVERLAY, in {verb}"));
            }
        }
        if let When::Group(_) = clause.when {
            if group.push.is_empty() {
                return Err(format!("{verb} IFTHEN WHEN=GROUP needs PUSH="));
            }
            if group.begin.is_none() && group.key.is_none() && group.end.is_none() && group.records.is_none() {
                return Err(format!("{verb} IFTHEN WHEN=GROUP needs BEGIN, KEYBEGIN, END or RECORDS"));
            }
            clause.when = When::Group(group);
        }
        Ok(clause)
    }

    /// The reformatting an INREC, OUTREC or OUTFIL statement names, from its operands.
    fn edit(&self, ops: &[String], verb: &str) -> Result<Option<Edit>, String> {
        let mut found = None;
        let mut clauses = Vec::new();
        let mut outlen = None;
        for o in ops {
            let (key, value) = o.split_once('=').unwrap_or((o.as_str(), ""));
            let made = match (key, verb) {
                ("FIELDS" | "BUILD", _) | ("OUTREC", "OUTFIL") => Edit::Build(self.items(value, key)?),
                ("OVERLAY", _) => Edit::Overlay(self.items(value, key)?),
                ("IFTHEN", _) => {
                    let clause = self.clause(value, verb)?;
                    if matches!(clause.when, When::Any) {
                        let since = clauses.iter().rev().take_while(|c: &&Clause| !matches!(c.when, When::Any));
                        if !since.into_iter().any(|c| matches!(c.when, When::Condition(_))) {
                            return Err(format!("{verb} IFTHEN WHEN=ANY needs a WHEN=(cond) clause before it"));
                        }
                    }
                    clauses.push(clause);
                    continue;
                }
                ("IFOUTLEN", _) => {
                    outlen = Some(number(value, "IFOUTLEN length")?);
                    continue;
                }
                _ => continue,
            };
            if found.replace(made).is_some() {
                return Err(format!("{verb} has more than one of BUILD, FIELDS, OUTREC and OVERLAY"));
            }
        }
        match (found, clauses.is_empty(), outlen) {
            (Some(_), false, _) => Err(format!("{verb} takes IFTHEN clauses or BUILD, FIELDS and OVERLAY, not both")),
            (_, true, Some(_)) => Err(format!("{verb} IFOUTLEN goes with IFTHEN clauses")),
            (Some(e), true, None) => Ok(Some(e)),
            (None, false, length) => Ok(Some(Edit::IfThen { clauses, length })),
            (None, true, None) => Ok(None),
        }
    }

    fn outfil(&self, ops: &[String]) -> Result<Outfil, String> {
        let mut group = Outfil { names: Vec::new(), selection: None, save: false, edit: self.edit(ops, "OUTFIL")? };
        for o in ops {
            let (key, value) = o.split_once('=').unwrap_or((o.as_str(), ""));
            match key {
                "FNAMES" => group.names.extend(parenthesized(value).unwrap_or(value).split(',').map(str::to_string)),
                "FILES" => group.names.extend(parenthesized(value).unwrap_or(value).split(',').map(|n| format!("SORTOF{n}"))),
                "INCLUDE" | "OMIT" => {
                    if group.selection.is_some() || group.save {
                        return Err("OUTFIL takes one of INCLUDE, OMIT and SAVE".into());
                    }
                    group.selection = Some(Selection { include: key == "INCLUDE", condition: self.condition(value, None)? });
                }
                "SAVE" if value.is_empty() => {
                    if group.selection.is_some() {
                        return Err("OUTFIL takes one of INCLUDE, OMIT and SAVE".into());
                    }
                    group.save = true;
                }
                "FIELDS" | "BUILD" | "OUTREC" | "OVERLAY" | "IFTHEN" | "IFOUTLEN" => {}
                k => return Err(format!("the OUTFIL parameter {k} is not supported yet")),
            }
        }
        if group.names.is_empty() {
            group.names.push("SORTOUT".into());
        }
        if let Some(bad) = group.names.iter().find(|n| n.is_empty() || n.len() > 8) {
            return Err(format!("{bad} is not a ddname"));
        }
        Ok(group)
    }
}

/// The control statements in SYSIN's cards, with no symbols.
pub fn parse(cards: &[String]) -> Result<Control, String> {
    parse_with(cards, &Symbols::default())
}

/// The control statements in SYSIN's cards, with the symbols SYMNAMES defined.
pub fn parse_with(cards: &[String], symbols: &Symbols) -> Result<Control, String> {
    let reader = Reader { symbols };
    let mut control = Control { kind: Kind::Copy, fields: Vec::new(), drop_duplicates: false, record: None, selection: None, inrec: None, outrec: None, outfil: Vec::new() };
    let mut verb_seen = false;
    for statement in statements(cards) {
        let (verb, operands) = statement.split_once(' ').unwrap_or((statement.as_str(), ""));
        let ops = tokens(operands);
        let keyword = |name: &str| ops.iter().find_map(|o| o.strip_prefix(&format!("{name}=")).map(str::to_string));
        match verb {
            "SORT" | "MERGE" => {
                if verb_seen {
                    return Err("more than one SORT or MERGE statement".into());
                }
                verb_seen = true;
                let format = keyword("FORMAT").map(|f| format_of(&f)).transpose()?;
                let value = keyword("FIELDS").ok_or_else(|| format!("{verb} needs FIELDS="))?;
                if let Some(other) = ops.iter().find(|o| !o.starts_with("FIELDS=") && !o.starts_with("FORMAT=") && !matches!(o.as_str(), "EQUALS" | "NOEQUALS") && !o.starts_with("FILSZ=") && !o.starts_with("SIZE=")) {
                    return Err(format!("{verb} operand {other} is not supported yet"));
                }
                match reader.fields(&value, format)? {
                    None => control.kind = Kind::Copy,
                    Some(f) => {
                        control.kind = if verb == "SORT" { Kind::Sort } else { Kind::Merge };
                        control.fields = f;
                    }
                }
            }
            "SUM" => match keyword("FIELDS").as_deref() {
                Some("NONE") | Some("(NONE)") => control.drop_duplicates = true,
                _ => return Err("SUM of fields is not supported yet; SUM FIELDS=NONE is".into()),
            },
            "OPTION" => {
                for o in &ops {
                    let name = o.split(['=', '(']).next().unwrap_or("");
                    match name {
                        "COPY" => {
                            verb_seen = true;
                            control.kind = Kind::Copy;
                        }
                        n if OPTION_IGNORED.contains(&n) => {}
                        n => return Err(format!("OPTION {n} is not supported yet")),
                    }
                }
            }
            "RECORD" => {
                let variable = match keyword("TYPE").as_deref() {
                    Some("F") | None => false,
                    Some("V" | "VB") => true,
                    Some(t) => return Err(format!("RECORD TYPE={t} is not supported yet")),
                };
                let length = keyword("LENGTH").and_then(|l| l.trim_start_matches('(').split([',', ')']).next().map(str::to_string)).filter(|l| !l.is_empty()).map(|l| number(&l, "record length")).transpose()?;
                control.record = Some(Record { variable, length });
            }
            "INCLUDE" | "OMIT" => {
                if control.selection.is_some() {
                    return Err("more than one INCLUDE or OMIT statement; INCLUDE and OMIT are mutually exclusive".into());
                }
                if let Some(other) = ops.iter().find(|o| !o.starts_with("COND=") && !o.starts_with("FORMAT=")) {
                    return Err(format!("{verb} operand {other} is not supported yet"));
                }
                let format = keyword("FORMAT").map(|f| format_of(&f)).transpose()?;
                let value = keyword("COND").ok_or_else(|| format!("{verb} needs COND="))?;
                control.selection = Some(Selection { include: verb == "INCLUDE", condition: reader.condition(&value, format)? });
            }
            "INREC" | "OUTREC" => {
                if let Some(other) = ops.iter().map(|o| o.split('=').next().unwrap_or(o)).find(|k| !matches!(*k, "FIELDS" | "BUILD" | "OVERLAY" | "IFTHEN" | "IFOUTLEN")) {
                    return Err(format!("the {verb} parameter {other} is not supported yet"));
                }
                let Some(made) = reader.edit(&ops, verb)? else { return Err(format!("{verb} needs FIELDS=, BUILD=, OVERLAY= or IFTHEN=")) };
                let slot = if verb == "INREC" { &mut control.inrec } else { &mut control.outrec };
                if slot.replace(made).is_some() {
                    return Err(format!("more than one {verb} statement"));
                }
            }
            "OUTFIL" => {
                let mut group = reader.outfil(&ops)?;
                group.names.retain(|n| !control.outfil.iter().any(|g| g.names.contains(n)));
                control.outfil.push(group);
            }
            "END" => break,
            v if REFUSED.contains(&v) => return Err(format!("the DFSORT {v} statement is not supported yet")),
            v => return Err(format!("{v} is not a DFSORT control statement")),
        }
    }
    if !verb_seen {
        return Err("no SORT, MERGE or OPTION COPY statement".into());
    }
    Ok(control)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cards(text: &str) -> Vec<String> {
        text.lines().map(|l| if l.starts_with('*') { l.to_string() } else { format!(" {l}") }).collect()
    }

    #[test]
    fn sort_fields_with_and_without_format() {
        let c = parse(&cards("  SORT FIELDS=(1,8,CH,A,10,5,PD,D)\n  SUM FIELDS=NONE")).unwrap();
        assert_eq!(c.kind, Kind::Sort);
        assert_eq!(c.fields, [Field { position: 1, length: 8, format: Format::Ch, ascending: true }, Field { position: 10, length: 5, format: Format::Pd, ascending: false }]);
        assert!(c.drop_duplicates);
        let c = parse(&cards("  SORT FIELDS=(1,4,A,9,2,D),FORMAT=ZD,EQUALS")).unwrap();
        assert_eq!(c.fields.iter().map(|f| (f.format, f.ascending)).collect::<Vec<_>>(), [(Format::Zd, true), (Format::Zd, false)]);
        assert_eq!(parse(&cards("  MERGE FIELDS=(5,3,BI,A)")).unwrap().kind, Kind::Merge);
        assert_eq!(parse(&cards("  SORT FIELDS=COPY")).unwrap().kind, Kind::Copy);
        assert_eq!(parse(&cards("  OPTION COPY")).unwrap().kind, Kind::Copy);
    }

    #[test]
    fn a_continued_statement_and_comments() {
        let c = parse(&cards("* the keys\n  SORT FIELDS=(1,8,CH,A,\n               20,4,FI,D)    THE REST IS A COMMENT\n  RECORD TYPE=V,LENGTH=(84)")).unwrap();
        assert_eq!(c.fields.len(), 2);
        assert_eq!(c.record, Some(Record { variable: true, length: Some(84) }));
    }

    #[test]
    fn conditions_join_with_and_before_or() {
        let c = parse(&cards("  OPTION COPY\n  INCLUDE COND=(1,10,CH,EQ,C'STOCK HOLM',\n     AND,21,8,ZD,GT,+50000,OR,31,4,CH,NE,X'C8C5D9D9')")).unwrap();
        let area = |position, length, format| Area { position, length, format };
        let compare = |left, relation, right| Box::new(Condition::Compare { left, relation, right: Operand::Constant(right) });
        assert_eq!(
            c.selection,
            Some(Selection {
                include: true,
                condition: Condition::Or(
                    Box::new(Condition::And(compare(area(1, 10, Format::Ch), Relation::Eq, Constant::Chars("STOCK HOLM".into())), compare(area(21, 8, Format::Zd), Relation::Gt, Constant::Decimal(50000)))),
                    compare(area(31, 4, Format::Ch), Relation::Ne, Constant::Hex(vec![0xC8, 0xC5, 0xD9, 0xD9])),
                ),
            })
        );
        let c = parse(&cards("  OPTION COPY\n  OMIT COND=((1,2,EQ,5,2),OR,(9,1,LT,-3)),FORMAT=PD")).unwrap();
        let s = c.selection.unwrap();
        assert!(!s.include);
        assert!(matches!(s.condition, Condition::Or(a, b) if matches!(*a, Condition::Compare { right: Operand::Field(Area { position: 5, length: 2, format: Format::Pd }), .. }) && matches!(*b, Condition::Compare { right: Operand::Constant(Constant::Decimal(-3)), .. })));
        assert_eq!(parse(&cards("  OPTION COPY\n  INCLUDE COND=NONE")).unwrap().selection.unwrap().condition, Condition::Always(false));
    }

    #[test]
    fn reformatting_items_and_outfil_groups() {
        let c = parse(&cards("  SORT FIELDS=COPY\n  INREC FIELDS=(1:263,16,17:1,262,3X,2C'AB',X'00FF',Z)\n  OUTREC OVERLAY=(5:C'*')\n  OUTFIL FNAMES=(A,B),INCLUDE=(1,1,CH,EQ,C'X'),BUILD=(1,10)\n  OUTFIL FNAMES=(B,C),SAVE")).unwrap();
        let item = |column, piece| Item { column, piece };
        assert_eq!(
            c.inrec,
            Some(Edit::Build(vec![
                item(Some(1), Piece::Field { position: 263, length: Some(16) }),
                item(Some(17), Piece::Field { position: 1, length: Some(262) }),
                item(None, Piece::Blanks(3)),
                item(None, Piece::Chars("ABAB".into())),
                item(None, Piece::Hex(vec![0, 0xFF])),
                item(None, Piece::Zeros(1)),
            ]))
        );
        assert_eq!(c.outrec, Some(Edit::Overlay(vec![item(Some(5), Piece::Chars("*".into()))])));
        assert_eq!(c.outfil.iter().map(|g| (g.names.clone(), g.save, g.selection.is_some(), g.edit.is_some())).collect::<Vec<_>>(), [(vec!["A".to_string(), "B".to_string()], false, true, true), (vec!["C".to_string()], true, false, false)], "B belongs to the first group that names it");
        assert_eq!(parse(&cards("  OPTION COPY\n  OUTFIL INCLUDE=(1,1,CH,EQ,C'X')")).unwrap().outfil[0].names, ["SORTOUT"]);
    }

    #[test]
    fn numbers_are_edited_by_mask_or_pattern_and_converted_by_format() {
        let c = parse(&cards("  OPTION COPY\n  OUTREC BUILD=(21,5,ZD,TO=PD,X,(8,4,ZD),FI,LENGTH=2,5:1,5,ZD,M19,\n   46,5,ZD,EDIT=(**II,IIT.TTXS),SIGNS=(,,+,-),+5000,EDIT=(T,TTT),\n   1,4,ZD,LENGTH=3,3,4,PD,EDAB=('A / B'),SIGNX=(+,'-'))")).unwrap();
        let Some(Edit::Build(items)) = c.outrec else { panic!("a BUILD") };
        let field = |position, length, format| Numeric::Field { position, length, format };
        let edit = |mask, signs| Output::Edit { mask, signs };
        let pattern = |text: &str, i, t, s| Mask::Pattern { text: text.into(), insignificant: i, significant: t, sign: s };
        assert_eq!(
            items.into_iter().map(|i| (i.column, i.piece)).collect::<Vec<_>>(),
            [
                (None, Piece::Number { value: field(21, 5, NumberFormat::Zd), output: Output::To(ToFormat::Pd), length: None }),
                (None, Piece::Blanks(1)),
                (None, Piece::Number { value: field(8, 4, NumberFormat::Zd), output: Output::To(ToFormat::Fi), length: Some(2) }),
                (Some(5), Piece::Number { value: field(1, 5, NumberFormat::Zd), output: edit(Mask::Predefined(19), None), length: None }),
                (None, Piece::Number { value: field(46, 5, NumberFormat::Zd), output: edit(pattern("**II,IIT.TTXS", 'I', 'T', 'S'), Some([None, None, Some('+'), Some('-')])), length: None }),
                (None, Piece::Number { value: Numeric::Constant(5000), output: edit(pattern("T,TTT", 'I', 'T', 'S'), None), length: None }),
                (None, Piece::Number { value: field(1, 4, NumberFormat::Zd), output: edit(Mask::Predefined(0), None), length: Some(3) }),
                (None, Piece::Number { value: field(3, 4, NumberFormat::Pd), output: edit(pattern("A / B", 'A', 'B', 'X'), Some([Some('+'), Some('-'), None, None])), length: None }),
            ]
        );
    }

    #[test]
    fn ifthen_clauses_in_their_kinds() {
        let c = parse(&cards(concat!(
            "  OPTION COPY\n",
            "  INREC IFOUTLEN=50,IFTHEN=(WHEN=INIT,BUILD=(1,20,21:C'Department')),\n",
            "   IFTHEN=(WHEN=(5,2,CH,EQ,C'D1'),OVERLAY=(31:8,3),HIT=NEXT),\n",
            "   IFTHEN=(WHEN=ANY,OVERLAY=(40:C'X')),\n",
            "   IFTHEN=(WHEN=NONE,OVERLAY=(31:C'***'))\n",
            "  OUTFIL IFTHEN=(WHEN=GROUP,BEGIN=(1,5,CH,EQ,C'DATE:'),RECORDS=3,\n",
            "   PUSH=(15:ID=3,31:21,8,SEQ=2)),\n",
            "   IFTHEN=(WHEN=GROUP,KEYBEGIN=(11,5),PUSH=(81:21,8))",
        )))
        .unwrap();
        let Some(Edit::IfThen { clauses, length }) = c.inrec else { panic!("IFTHEN") };
        assert_eq!(length, Some(50));
        assert_eq!(clauses.iter().map(|c| (std::mem::discriminant(&c.when), c.hit_next, c.edit.is_some())).collect::<Vec<_>>(), [
            (std::mem::discriminant(&When::Init), false, true),
            (std::mem::discriminant(&When::Condition(Condition::Always(true))), true, true),
            (std::mem::discriminant(&When::Any), false, true),
            (std::mem::discriminant(&When::None), false, true),
        ]);
        let Some(Edit::IfThen { clauses, .. }) = &c.outfil[0].edit else { panic!("OUTFIL IFTHEN") };
        let When::Group(g) = &clauses[0].when else { panic!("GROUP") };
        assert_eq!((g.records, g.key, g.end.is_none()), (Some(3), None, true));
        assert_eq!(g.push, [Push { column: Some(15), value: Pushed::Id(3) }, Push { column: Some(31), value: Pushed::Field { position: 21, length: 8 } }, Push { column: None, value: Pushed::Seq(2) }]);
        let When::Group(g) = &clauses[1].when else { panic!("GROUP") };
        assert_eq!(g.key, Some((11, 5)));
    }

    #[test]
    fn symbols_stand_in_as_their_place_takes_them() {
        let symbols = Symbols::read(&["First_Field,12,2,BI", "Second_Field,18,6,CH", "Third_Field,28,5,PD", "Fourth_Field,36,3", "Fifth_Field,52,4,PD", "Max,200000", "Outcol2,16"].map(String::from)).unwrap();
        let c = parse_with(&cards("  OMIT COND=(Fifth_Field,GT,Max)\n  SORT FIELDS=(First_Field,A,Fourth_Field,A),FORMAT=CH\n  OUTFIL OUTREC=(First_Field:First_Field,\n   Outcol2:Third_Field,M11,Fourth_Field)"), &symbols).unwrap();
        assert_eq!(c.selection.unwrap().condition, Condition::Compare { left: Area { position: 52, length: 4, format: Format::Pd }, relation: Relation::Gt, right: Operand::Constant(Constant::Decimal(200000)) });
        assert_eq!(c.fields.iter().map(|f| (f.position, f.length, f.format)).collect::<Vec<_>>(), [(12, 2, Format::Ch), (36, 3, Format::Ch)]);
        let Some(Edit::Build(items)) = &c.outfil[0].edit else { panic!("an OUTREC") };
        assert_eq!(items, &[
            Item { column: Some(12), piece: Piece::Field { position: 12, length: Some(2) } },
            Item { column: Some(16), piece: Piece::Number { value: Numeric::Field { position: 28, length: 5, format: NumberFormat::Pd }, output: Output::Edit { mask: Mask::Predefined(11), signs: None }, length: None } },
            Item { column: None, piece: Piece::Field { position: 36, length: Some(3) } },
        ], "IBM's own example: OUTFIL BUILD=(12:12,2,16:28,5,PD,M11,36,3)");
    }

    #[test]
    fn what_is_not_modelled_is_refused_by_name() {
        for (text, message) in [
            ("  SORT FIELDS=(1,8,CH,A)\n  OUTFIL FNAMES=OUT1,HEADER1=('X')", "the OUTFIL parameter HEADER1 is not supported yet"),
            ("  SORT FIELDS=(1,8,CH,A)\n  INREC FINDREP=(IN=C'A',OUT=C'B')", "the INREC parameter FINDREP is not supported yet"),
            ("  SORT FIELDS=(1,8,CH,A)\n  INREC IFTHEN=(WHEN=INIT,PARSE=(%01=(FIXLEN=3)))", "INREC IFTHEN PARSE is not supported yet"),
            ("  SORT FIELDS=(1,8,CH,A)\n  INREC IFTHEN=(WHEN=ANY,OVERLAY=(2:C'B'))", "INREC IFTHEN WHEN=ANY needs a WHEN=(cond) clause before it"),
            ("  SORT FIELDS=(1,8,CH,A)\n  INREC IFTHEN=(WHEN=GROUP,RECORDS=2)", "INREC IFTHEN WHEN=GROUP needs PUSH="),
            ("  SORT FIELDS=(1,8,CH,A)\n  INREC BUILD=(1,3),IFTHEN=(WHEN=INIT,BUILD=(1,2))", "INREC takes IFTHEN clauses or BUILD, FIELDS and OVERLAY, not both"),
            ("  SORT FIELDS=(1,8,CH,A)\n  INREC IFTHEN=(WHEN=INIT,BUILD=(1,2),HIT=NEXT)", "HIT is not an operand of this INREC IFTHEN clause"),
            ("  SORT FIELDS=(1,8,CH,A)\n  OUTREC FIELDS=(1,5,ZD,ADD,+1)", "arithmetic in FIELDS (ADD) is not supported yet"),
            ("  SORT FIELDS=(1,8,CH,A)\n  OUTREC BUILD=(21,3,Y2U)", "BUILD editing of Y2U fields is not supported yet"),
            ("  SORT FIELDS=(1,8,CH,A)\n  OUTREC BUILD=(1,5,ZD,TO=PD,M4)", "a BUILD number is either edited or converted, not both"),
            ("  SORT FIELDS=(1,8,CH,A)\n  OUTREC BUILD=(1,40,ZD)", "a ZD field of 40 bytes is not one BUILD edits"),
            ("  SORT FIELDS=(1,8,CH,A)\n  OUTREC FIELDS=(1,42,DATE1)", "the FIELDS item DATE1 is not supported yet"),
            ("  SORT FIELDS=(1,8,CH,A)\n  INCLUDE COND=(TRAN-ID,EQ,C'A')", "TRAN-ID is not a position"),
            ("  SORT FIELDS=(1,8,CH,A)\n  INCLUDE COND=(1,4,ZD,EQ,C'A')", "comparing 1,4,ZD with a character string is not supported yet"),
            ("  SORT FIELDS=(1,8,CH,A)\n  INCLUDE COND=(1,4,SS,EQ,C'A')", "the field format SS is not supported yet"),
            ("  SORT FIELDS=(1,8,CH,A)\n  INCLUDE COND=(1,4,CH,EQ,C'A')\n  OMIT COND=(1,4,CH,EQ,C'B')", "more than one INCLUDE or OMIT statement; INCLUDE and OMIT are mutually exclusive"),
            ("  SORT FIELDS=(1,8,CH,A)\n  SUM FIELDS=(10,5,PD)", "SUM of fields is not supported yet; SUM FIELDS=NONE is"),
            ("  SORT FIELDS=(1,8,FL,A)", "the field format FL is not supported yet"),
            ("  SORT FIELDS=(1,8,CH)", "FIELDS=(1,8,CH) is not position, length, format, order for each field"),
            ("  OPTION VLSHRT\n  SORT FIELDS=COPY", "OPTION VLSHRT is not supported yet"),
            ("  SORT FIELDS=COPY\n  MODS E15=(X,100)", "the DFSORT MODS statement is not supported yet"),
            ("  SUM FIELDS=NONE", "no SORT, MERGE or OPTION COPY statement"),
        ] {
            assert_eq!(parse(&cards(text)).unwrap_err(), message, "{text}");
        }
    }
}
