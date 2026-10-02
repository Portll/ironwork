//! DFSORT control statements from SYSIN: SORT and MERGE with FIELDS, SUM FIELDS=NONE, OPTION,
//! RECORD, INCLUDE and OMIT, INREC and OUTREC, and OUTFIL, as DFSORT Application Programming
//! Guide's control-statement chapter writes them. A statement, operand or item these do not model
//! (a SUM of fields, IFTHEN, field conversions and editing, exits) is refused by name.

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    /// `length` bytes from `position`; with no length, the rest of a variable-length record.
    Field { position: usize, length: Option<usize> },
    Blanks(usize),
    Zeros(usize),
    Chars(String),
    Hex(Vec<u8>),
}

/// A reformatting item, placed at `column` (1 the first) or after the item before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub column: Option<usize>,
    pub piece: Piece,
}

/// BUILD (or FIELDS) makes a new record of its items; OVERLAY writes them over the record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    Build(Vec<Item>),
    Overlay(Vec<Item>),
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

fn number(text: &str, what: &str) -> Result<usize, String> {
    match text.parse::<usize>() {
        Ok(n) if n > 0 => Ok(n),
        _ => Err(format!("{text} is not a {what}")),
    }
}

fn is_number(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

/// FIELDS=(p,l,f,o,...), (p,l,o,...) with FORMAT=f, or COPY.
fn fields(value: &str, format: Option<Format>) -> Result<Option<Vec<Field>>, String> {
    if value == "COPY" {
        return Ok(None);
    }
    let inner = parenthesized(value).ok_or_else(|| format!("FIELDS={value} is not in parentheses"))?;
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    let width = if format.is_some() { 3 } else { 4 };
    if parts.is_empty() || !parts.len().is_multiple_of(width) {
        return Err(format!("FIELDS=({inner}) is not position, length{}, order for each field", if format.is_some() { "" } else { ", format" }));
    }
    let mut out = Vec::new();
    for group in parts.chunks(width) {
        let format = match format {
            Some(f) => f,
            None => format_of(group[2])?,
        };
        let ascending = match group[width - 1] {
            "A" => true,
            "D" => false,
            "E" => return Err("an E order (an exit's own) is not supported yet".into()),
            o => return Err(format!("{o} is not A or D")),
        };
        out.push(Field { position: number(group[0], "position")?, length: number(group[1], "length")?, format, ascending });
    }
    Ok(Some(out))
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

/// A comparison's left or right field: position, length and, unless FORMAT= gives it, format.
fn area(t: &[String], at: &mut usize, format: Option<Format>) -> Result<Area, String> {
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

fn comparison(t: &[String], at: &mut usize, format: Option<Format>) -> Result<Condition, String> {
    let left = area(t, at, format)?;
    let op = t.get(*at).ok_or("a comparison has no relation")?;
    let relation = relation(op).ok_or_else(|| format!("{op} is not EQ, NE, GT, GE, LT or LE"))?;
    *at += 1;
    let right_starts_field = t.get(*at).is_some_and(|w| is_number(w)) && t.get(*at + 1).is_some_and(|w| is_number(w));
    let right = if right_starts_field {
        Operand::Field(area(t, at, format)?)
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

fn factor(t: &[String], at: &mut usize, format: Option<Format>) -> Result<Condition, String> {
    let token = t.get(*at).ok_or("a condition ends early")?;
    if let Some(inner) = parenthesized(token) {
        *at += 1;
        return expression_of(inner, format);
    }
    comparison(t, at, format)
}

/// Comparisons joined by AND, which DFSORT evaluates before OR.
fn term(t: &[String], at: &mut usize, format: Option<Format>) -> Result<Condition, String> {
    let mut left = factor(t, at, format)?;
    while t.get(*at).is_some_and(|w| w == "AND" || w == "&") {
        *at += 1;
        left = Condition::And(Box::new(left), Box::new(factor(t, at, format)?));
    }
    Ok(left)
}

fn expression_of(text: &str, format: Option<Format>) -> Result<Condition, String> {
    let t = tokens(text);
    let mut at = 0;
    let mut left = term(&t, &mut at, format)?;
    while t.get(at).is_some_and(|w| w == "OR" || w == "|") {
        at += 1;
        left = Condition::Or(Box::new(left), Box::new(term(&t, &mut at, format)?));
    }
    match t.get(at) {
        Some(extra) => Err(format!("{extra} in a condition is not AND or OR")),
        None => Ok(left),
    }
}

/// COND=(...), ALL or NONE.
fn condition(value: &str, format: Option<Format>) -> Result<Condition, String> {
    match value {
        "ALL" | "(ALL)" => return Ok(Condition::Always(true)),
        "NONE" | "(NONE)" => return Ok(Condition::Always(false)),
        _ => {}
    }
    let inner = parenthesized(value).ok_or_else(|| format!("COND={value} is not in parentheses"))?;
    expression_of(inner, format)
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

/// Whether `token` begins a reformatting item: a column, a position, a literal, or one of the
/// items these do not model (dates, times, sequence numbers, parsed fields, a new line).
fn begins_item(token: &str) -> bool {
    token.split_once(':').is_some_and(|(c, _)| is_number(c))
        || is_number(token)
        || literal(token).is_ok_and(|l| l.is_some())
        || ["DATE", "TIME", "SEQNUM", "%", "/"].iter().any(|p| token.starts_with(p))
}

/// The items of BUILD=(...), FIELDS=(...) or OVERLAY=(...).
fn items(value: &str, what: &str) -> Result<Vec<Item>, String> {
    let inner = parenthesized(value).ok_or_else(|| format!("{what}={value} is not in parentheses"))?;
    let t = tokens(inner);
    let mut out = Vec::new();
    let mut i = 0;
    while i < t.len() {
        let (column, token) = match t[i].split_once(':') {
            Some((c, rest)) if is_number(c) => (Some(number(c, "column")?), rest),
            _ => (None, t[i].as_str()),
        };
        i += 1;
        let piece = if is_number(token) {
            let position = number(token, "position")?;
            let length = match t.get(i) {
                Some(l) if is_number(l) => {
                    i += 1;
                    Some(number(l, "length")?)
                }
                _ => None,
            };
            if let Some(next) = t.get(i).filter(|n| !begins_item(n)) {
                return Err(format!("{what} field conversion and editing ({next}) is not supported yet"));
            }
            Piece::Field { position, length }
        } else {
            match literal(token)? {
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

/// The reformatting an INREC, OUTREC or OUTFIL statement names, from its operands.
fn edit(ops: &[String], verb: &str) -> Result<Option<Edit>, String> {
    let mut found = None;
    for o in ops {
        let (key, value) = o.split_once('=').unwrap_or((o.as_str(), ""));
        let made = match (key, verb) {
            ("FIELDS" | "BUILD", _) | ("OUTREC", "OUTFIL") => Edit::Build(items(value, key)?),
            ("OVERLAY", _) => Edit::Overlay(items(value, key)?),
            _ => continue,
        };
        if found.replace(made).is_some() {
            return Err(format!("{verb} has more than one of BUILD, FIELDS, OUTREC and OVERLAY"));
        }
    }
    Ok(found)
}

fn outfil(ops: &[String]) -> Result<Outfil, String> {
    let mut group = Outfil { names: Vec::new(), selection: None, save: false, edit: edit(ops, "OUTFIL")? };
    for o in ops {
        let (key, value) = o.split_once('=').unwrap_or((o.as_str(), ""));
        match key {
            "FNAMES" => group.names.extend(parenthesized(value).unwrap_or(value).split(',').map(str::to_string)),
            "FILES" => group.names.extend(parenthesized(value).unwrap_or(value).split(',').map(|n| format!("SORTOF{n}"))),
            "INCLUDE" | "OMIT" => {
                if group.selection.is_some() || group.save {
                    return Err("OUTFIL takes one of INCLUDE, OMIT and SAVE".into());
                }
                group.selection = Some(Selection { include: key == "INCLUDE", condition: condition(value, None)? });
            }
            "SAVE" if value.is_empty() => {
                if group.selection.is_some() {
                    return Err("OUTFIL takes one of INCLUDE, OMIT and SAVE".into());
                }
                group.save = true;
            }
            "FIELDS" | "BUILD" | "OUTREC" | "OVERLAY" => {}
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

/// The control statements in SYSIN's cards.
pub fn parse(cards: &[String]) -> Result<Control, String> {
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
                match fields(&value, format)? {
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
                control.selection = Some(Selection { include: verb == "INCLUDE", condition: condition(&value, format)? });
            }
            "INREC" | "OUTREC" => {
                if let Some(other) = ops.iter().map(|o| o.split('=').next().unwrap_or(o)).find(|k| !matches!(*k, "FIELDS" | "BUILD" | "OVERLAY")) {
                    return Err(format!("the {verb} parameter {other} is not supported yet"));
                }
                let Some(made) = edit(&ops, verb)? else { return Err(format!("{verb} needs FIELDS=, BUILD= or OVERLAY=")) };
                let slot = if verb == "INREC" { &mut control.inrec } else { &mut control.outrec };
                if slot.replace(made).is_some() {
                    return Err(format!("more than one {verb} statement"));
                }
            }
            "OUTFIL" => {
                let mut group = outfil(&ops)?;
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
    fn what_is_not_modelled_is_refused_by_name() {
        for (text, message) in [
            ("  SORT FIELDS=(1,8,CH,A)\n  OUTFIL FNAMES=OUT1,HEADER1=('X')", "the OUTFIL parameter HEADER1 is not supported yet"),
            ("  SORT FIELDS=(1,8,CH,A)\n  INREC IFTHEN=(WHEN=(1,1,CH,EQ,C'A'),OVERLAY=(2:C'B'))", "the INREC parameter IFTHEN is not supported yet"),
            ("  SORT FIELDS=(1,8,CH,A)\n  OUTREC FIELDS=(1,5,ZD,TO=PD,LENGTH=3)", "FIELDS field conversion and editing (ZD) is not supported yet"),
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
