//! A job's JCL as the z/OS MVS JCL Reference describes it: the JOB statement, EXEC steps, DD
//! statements with their data sets, dispositions and in-stream data, IF/THEN/ELSE/ENDIF, and
//! procedures (in-stream and cataloged, with symbolic parameters, SET, JCLLIB, INCLUDE, and EXEC
//! and DD overrides) expanded into the steps they run. What the reader does not model it refuses
//! by name rather than reading it some other way.

pub mod cond;
pub mod idcams;
pub mod sort;
pub mod symnames;

use cond::Cond;
use std::collections::HashMap;

#[derive(Debug)]
pub struct Job {
    pub name: String,
    pub cond: Cond,
    /// The JOBLIB DD's libraries, searched for every step's program that has no STEPLIB.
    pub joblib: Option<Dd>,
    pub items: Vec<Item>,
}

#[derive(Debug)]
pub enum Item {
    Step(Step),
    /// `caller` is the step that called the procedure the IF is in.
    If { expr: cond::Expr, caller: Option<String>, line: usize },
    Else { line: usize },
    EndIf { line: usize },
}

#[derive(Debug)]
pub struct Step {
    pub name: Option<String>,
    /// The job step whose EXEC called the procedure this step is in.
    pub caller: Option<String>,
    pub pgm: String,
    pub parm: Option<String>,
    pub cond: Cond,
    pub dds: Vec<Dd>,
    pub line: usize,
}

impl Step {
    /// The name the job log and COND and IF use: stepname, or stepname.procstepname.
    pub fn shown(&self) -> String {
        match (&self.caller, &self.name) {
            (Some(c), Some(n)) => format!("{c}.{n}"),
            (Some(c), None) => format!("{c}."),
            (None, n) => n.clone().unwrap_or_default(),
        }
    }
}

/// A DD statement and the unnamed DD statements concatenated after it.
#[derive(Debug)]
pub struct Dd {
    pub name: String,
    pub parts: Vec<Part>,
}

#[derive(Debug)]
pub struct Part {
    pub source: Source,
    pub disp: Disp,
    pub line: usize,
    /// The record format and length the DD gives (RECFM, LRECL, alone or in DCB), where it gives them.
    pub recfm: Option<String>,
    pub lrecl: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Dataset { dsn: String, member: Option<String> },
    /// `&&NAME`, a data set that lasts until the job ends. A DD that names no data set and asks
    /// for one (UNIT=, SPACE=) gets a new one, named as no `&&NAME` can be: `SYS.nnnnn`.
    Temporary { name: String, member: Option<String> },
    /// A generation of a generation data group, relative to the newest when the job began: 0 the
    /// newest, -1 the one before, +1 the next.
    Generation { base: String, relative: i32 },
    /// DSN=*.stepname.ddname before it is resolved to the data set it names.
    Refer(String),
    InStream(Vec<String>),
    Dummy,
    Sysout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    New,
    Old,
    Shr,
    Mod,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    Delete,
    Keep,
    Pass,
    Catlg,
    Uncatlg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Disp {
    pub status: Status,
    pub normal: Option<End>,
    pub abnormal: Option<End>,
}

impl Default for Disp {
    fn default() -> Self {
        Disp { status: Status::New, normal: None, abnormal: None }
    }
}

impl Disp {
    /// What happens to the data set when the step ends: its normal disposition, or its abnormal
    /// one after an abend. Unstated, a data set the step created is deleted and one that existed
    /// is kept, and an abnormal end takes the normal disposition, PASS aside.
    pub fn at_end(&self, abended: bool) -> End {
        let default = if self.status == Status::New { End::Delete } else { End::Keep };
        let normal = self.normal.unwrap_or(default);
        if !abended {
            return normal;
        }
        match (self.abnormal, normal) {
            (Some(end), _) => end,
            (None, End::Pass) => default,
            (None, end) => end,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

fn err<T>(line: usize, message: impl Into<String>) -> Result<T, Error> {
    Err(Error { line, message: message.into() })
}

/// A name in the name field, a DD name, a step or program name, a member or a symbol's shape.
pub fn is_name(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty() && b.len() <= 8 && matches!(b[0], b'A'..=b'Z' | b'#' | b'$' | b'@') && b[1..].iter().all(|c| matches!(c, b'A'..=b'Z' | b'0'..=b'9' | b'#' | b'$' | b'@'))
}

/// A data set name: qualifiers of one to eight characters joined by periods, 44 at most.
pub fn is_dsn(s: &str) -> bool {
    s.len() <= 44
        && s.split('.').all(|q| {
            let b = q.as_bytes();
            !b.is_empty() && b.len() <= 8 && matches!(b[0], b'A'..=b'Z' | b'#' | b'$' | b'@') && b[1..].iter().all(|c| matches!(c, b'A'..=b'Z' | b'0'..=b'9' | b'#' | b'$' | b'@' | b'-'))
        })
}

/// A statement as written, continuation lines joined, with the in-stream data after a DD * or
/// DD DATA.
#[derive(Debug, Clone)]
struct Raw {
    line: usize,
    name: Option<String>,
    operation: String,
    operands: String,
    data: Option<Vec<String>>,
}

/// Columns 1-71 of a statement; 72 holds a comment's continuation mark and 73-80 a sequence number.
fn statement_columns(text: &str) -> &str {
    let text = text.trim_end_matches(['\r', '\n']);
    match text.char_indices().nth(71) {
        Some((i, _)) => &text[..i],
        None => text,
    }
}

/// The operand field: up to the first blank outside quotes. An unbalanced quote means a value
/// continued onto the next line.
fn operand_field(text: &str, line: usize) -> Result<&str, Error> {
    let mut quoted = false;
    for (i, c) in text.char_indices() {
        match c {
            '\'' => quoted = !quoted,
            ' ' if !quoted => return Ok(&text[..i]),
            _ => {}
        }
    }
    if quoted {
        return err(line, "a quoted value continued onto the next line is not supported yet");
    }
    Ok(text)
}

fn fields(text: &str) -> (Option<String>, &str) {
    let body = &text[2..];
    if body.starts_with(' ') || body.is_empty() {
        (None, body.trim_start())
    } else {
        let end = body.find(' ').unwrap_or(body.len());
        (Some(body[..end].to_string()), body[end..].trim_start())
    }
}

/// The operands of `text` split at top-level commas.
fn split_operands(text: &str, line: usize) -> Result<Vec<String>, Error> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let (mut out, mut current, mut depth, mut quoted) = (Vec::new(), String::new(), 0i32, false);
    for c in text.chars() {
        match c {
            '\'' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => {
                depth -= 1;
                if depth < 0 {
                    return err(line, format!("an unbalanced ) in {text}"));
                }
            }
            ',' if !quoted && depth == 0 => {
                out.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    if depth != 0 || quoted {
        return err(line, format!("unbalanced parentheses or quotes in {text}"));
    }
    out.push(current);
    Ok(out)
}

/// `KEY=value` as (Some(KEY), value); a positional operand as (None, operand). A key may be
/// qualified by a procedure step, as in PARM.STEP1=.
fn keyword(operand: &str) -> (Option<&str>, &str) {
    let key_end = operand.find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '#' | '$' | '@' | '-' | '.')));
    match key_end {
        Some(i) if i > 0 && operand[i..].starts_with('=') => (Some(&operand[..i]), &operand[i + 1..]),
        _ => (None, operand),
    }
}

fn unquote(value: &str) -> String {
    match value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')) {
        Some(inner) => inner.replace("''", "'"),
        None => value.to_string(),
    }
}

/// `text` with each `&NAME` outside apostrophes replaced by its value; a period after the name
/// ends it and is dropped. `&&` begins a temporary data set name and is kept.
fn substitute(text: &str, symbols: &HashMap<String, String>, line: usize) -> Result<String, Error> {
    let chars: Vec<char> = text.chars().collect();
    let (mut out, mut i, mut quoted) = (String::new(), 0, false);
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' {
            quoted = !quoted;
        }
        if c != '&' || quoted {
            out.push(c);
            i += 1;
            continue;
        }
        if chars.get(i + 1) == Some(&'&') {
            out.push_str("&&");
            i += 2;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || matches!(chars[i], '#' | '$' | '@')) {
                out.push(chars[i]);
                i += 1;
            }
            continue;
        }
        let start = i + 1;
        let mut end = start;
        while end < chars.len() && end - start < 8 && (chars[end].is_ascii_uppercase() || chars[end].is_ascii_digit() || matches!(chars[end], '#' | '$' | '@')) {
            end += 1;
        }
        let name: String = chars[start..end].iter().collect();
        if !is_name(&name) {
            return err(line, format!("& is not followed by a symbolic parameter name in {text}"));
        }
        let Some(value) = symbols.get(&name) else {
            if name == "SYSUID" {
                return err(line, "&SYSUID is the user ID the job runs under: USER= on the JOB statement, or the user that submitted it");
            }
            return err(line, format!("symbolic parameter &{name} has no value"));
        };
        out.push_str(value);
        i = end;
        if chars.get(i) == Some(&'.') {
            i += 1;
        }
    }
    Ok(out)
}

struct Reader<'a> {
    lines: Vec<&'a str>,
    at: usize,
}

impl<'a> Reader<'a> {
    fn peek(&self) -> Option<&'a str> {
        self.lines.get(self.at).copied()
    }

    /// Every statement up to the null statement or the end, with its in-stream data.
    fn all(&mut self) -> Result<Vec<Raw>, Error> {
        let mut out = Vec::new();
        while let Some(mut raw) = self.statement()? {
            if raw.operation == "DD" {
                let operands = split_operands(&raw.operands, raw.line)?;
                let positional = |w: &str| operands.iter().any(|o| o == w);
                let dlm = operands.iter().find_map(|o| o.strip_prefix("DLM=")).map(unquote);
                if positional("*") || positional("DATA") {
                    raw.data = Some(self.in_stream(positional("*"), dlm.as_deref()));
                }
            }
            out.push(raw);
        }
        Ok(out)
    }

    /// The next statement, joining continuation lines; None at the end of the job.
    fn statement(&mut self) -> Result<Option<Raw>, Error> {
        while let Some(raw) = self.peek() {
            let line = self.at + 1;
            let text = statement_columns(raw);
            if text.starts_with("//*") || text.starts_with("/*") {
                self.at += 1;
                continue;
            }
            if !text.starts_with("//") {
                if text.trim().is_empty() {
                    self.at += 1;
                    continue;
                }
                // JCL Reference, SYSIN DD statement: data with no DD before it gets a //SYSIN DD *.
                return Ok(Some(Raw { line, name: Some("SYSIN".into()), operation: "DD".into(), operands: "*".into(), data: None }));
            }
            if text[2..].trim().is_empty() {
                return Ok(None);
            }
            self.at += 1;
            let (name, rest) = fields(text);
            let (operation, rest) = rest.split_once(' ').map_or((rest, ""), |(o, r)| (o, r.trim_start()));
            let operation = operation.to_string();
            if operation == "IF" {
                return self.if_statement(line, name, rest).map(Some);
            }
            let mut operands = operand_field(rest, line)?.to_string();
            let mut last = raw;
            while operands.ends_with(',') {
                let Some(next) = self.peek().map(statement_columns) else { return err(line, "the statement is continued past the end of the job") };
                if !next.starts_with("// ") {
                    return err(self.at + 1, "a continuation line must start with // and a blank");
                }
                let body = next[2..].trim_start();
                if next.len() - body.len() > 15 {
                    return err(self.at + 1, "a continued operand must start in columns 4 to 16");
                }
                operands.push_str(operand_field(body, self.at + 1)?);
                last = self.lines[self.at];
                self.at += 1;
            }
            if last.chars().nth(71).is_some_and(|c| c != ' ') && self.peek().is_some_and(|l| l.starts_with("//")) {
                self.at += 1;
            }
            return Ok(Some(Raw { line, name, operation, operands, data: None }));
        }
        Ok(None)
    }

    fn if_statement(&mut self, line: usize, name: Option<String>, first: &str) -> Result<Raw, Error> {
        let mut expr = String::new();
        let mut text = first.to_string();
        loop {
            let words: Vec<&str> = text.split_whitespace().collect();
            if let Some(then) = words.iter().position(|w| *w == "THEN") {
                expr.push_str(&words[..then].join(" "));
                return Ok(Raw { line, name, operation: "IF".into(), operands: expr, data: None });
            }
            expr.push_str(&words.join(" "));
            expr.push(' ');
            let Some(next) = self.peek().map(statement_columns) else { return err(line, "IF has no THEN") };
            if !next.starts_with("// ") {
                return err(self.at + 1, "IF has no THEN");
            }
            text = next[2..].to_string();
            self.at += 1;
        }
    }

    /// The lines after a DD * or DD DATA up to its delimiter, which is consumed unless it is
    /// the next statement.
    fn in_stream(&mut self, star: bool, dlm: Option<&str>) -> Vec<String> {
        let mut out = Vec::new();
        while let Some(raw) = self.peek() {
            let text = raw.trim_end_matches(['\r', '\n']);
            match dlm {
                Some(d) if text.starts_with(d) => {
                    self.at += 1;
                    break;
                }
                Some(_) => {}
                None if text.starts_with("/*") => {
                    self.at += 1;
                    break;
                }
                None if star && text.starts_with("//") => break,
                None => {}
            }
            let card: String = text.chars().take(80).collect();
            out.push(card.trim_end().to_string());
            self.at += 1;
        }
        out
    }
}

fn read(text: &str) -> Result<Vec<Raw>, Error> {
    Reader { lines: text.lines().collect(), at: 0 }.all()
}

const JOB_IGNORED: &[&str] = &["CLASS", "MSGCLASS", "MSGLEVEL", "NOTIFY", "REGION", "TIME", "PRTY", "USER", "GROUP", "JOBRC", "LINES", "ADDRSPC", "BYTES", "CARDS", "PAGES", "PERFORM", "RD", "SCHENV", "SECLABEL", "SYSAFF", "SYSTEM", "UJOBCORR", "MEMLIMIT", "EMAIL"];
const EXEC_IGNORED: &[&str] = &["REGION", "TIME", "ACCT", "ADDRSPC", "DYNAMNBR", "PERFORM", "RD", "MEMLIMIT", "CCSID", "REGIONX", "TVSMSG", "TVSAMCOM"];
const DD_IGNORED: &[&str] = &[
    "DCB", "SPACE", "UNIT", "VOL", "VOLUME", "LRECL", "RECFM", "BLKSIZE", "DSORG", "LABEL", "RETPD", "EXPDT", "STORCLAS", "MGMTCLAS", "DATACLAS", "AVGREC", "FREE", "HOLD", "OUTLIM", "COPIES", "DEST", "DSNTYPE", "BUFNO", "KEYLEN", "EATTR", "FCB", "UCS", "OUTPUT", "SPIN", "SEGMENT", "BLKSZLIM", "DSID",
];
const NOT_SUPPORTED_STATEMENTS: &[&str] = &["CNTL", "ENDCNTL", "EXPORT", "SCHEDULE", "XMIT", "COMMAND"];

/// Finds a member of a procedure library: `(order, member)` gives the JCLLIB ORDER data sets in
/// force and the member's name, and answers the member's text, or None when no library holds it.
pub type Libraries<'a> = dyn Fn(&[String], &str) -> Result<Option<String>, String> + 'a;

/// Reads one job with no procedure libraries.
pub fn parse(text: &str) -> Result<Job, Error> {
    parse_with(text, &|_, _| Ok(None), None)
}

/// Reads one job, finding cataloged procedures and INCLUDE members through `libraries`. Names
/// must be in upper case, as z/OS requires. `submitter` is the user ID the job was submitted
/// from, &SYSUID's value when the JOB statement gives no USER=.
pub fn parse_with(text: &str, libraries: &Libraries<'_>, submitter: Option<&str>) -> Result<Job, Error> {
    let raws = read(text)?;
    let Some(first) = raws.first() else { return err(1, "no JOB statement") };
    if first.operation != "JOB" {
        return err(first.line, "the first statement is not a JOB statement");
    }
    let name = match &first.name {
        Some(n) if is_name(n) => n.clone(),
        _ => return err(first.line, "the JOB statement needs a job name of one to eight characters"),
    };
    let (cond, user) = job_operands(first)?;
    let symbols: HashMap<String, String> = user.or(submitter.map(str::to_string)).map(|u| ("SYSUID".to_string(), u)).into_iter().collect();
    let mut expander = Expander { libraries, procs: HashMap::new(), order: Vec::new(), symbols, joblib: None, items: Vec::new() };
    expander.statements(&raws[1..], None, &HashMap::new(), 0)?;
    check_nesting(&expander.items)?;
    let mut items = expander.items;
    name_unnamed(&mut items);
    resolve_references(&mut items)?;
    Ok(Job { name, cond, joblib: expander.joblib, items })
}

/// The JOB statement's COND, and the user ID its USER= names.
fn job_operands(st: &Raw) -> Result<(Cond, Option<String>), Error> {
    let (mut cond, mut user) = (Cond::default(), None);
    for op in split_operands(&st.operands, st.line)? {
        match keyword(&op) {
            (None, _) => {}
            (Some("USER"), value) if is_name(value) => user = Some(value.to_string()),
            (Some("COND"), value) if value.contains('&') => return err(st.line, "a symbolic parameter in the JOB statement's COND is not supported yet"),
            (Some("COND"), value) => {
                cond = cond::parse_cond(value).map_err(|m| Error { line: st.line, message: m })?;
                if cond.mode != cond::Mode::Plain || cond.tests.iter().any(|t| t.step.is_some()) {
                    return err(st.line, "COND on the JOB statement takes (code,operator) tests only");
                }
            }
            (Some("RESTART"), _) => return err(st.line, "RESTART is not supported yet"),
            (Some("TYPRUN"), _) => return err(st.line, "TYPRUN is not supported yet"),
            (Some(k), _) if JOB_IGNORED.contains(&k) => {}
            (Some(k), _) => return err(st.line, format!("JOB keyword {k} is not supported yet")),
        }
    }
    Ok((cond, user))
}

/// A procedure's statements between its PROC and PEND, and the defaults its PROC statement gives.
struct Procedure {
    defaults: HashMap<String, String>,
    body: Vec<Raw>,
}

struct Expander<'a, 'l> {
    libraries: &'a Libraries<'l>,
    procs: HashMap<String, Vec<Raw>>,
    order: Vec<String>,
    /// Values from SET statements, in force from the SET onward.
    symbols: HashMap<String, String>,
    joblib: Option<Dd>,
    items: Vec<Item>,
}

/// The EXEC keywords that are not symbolic parameters when they call a procedure.
fn exec_keyword(key: &str) -> bool {
    let base = key.split('.').next().unwrap_or(key);
    matches!(base, "PGM" | "PROC" | "PARM" | "PARMDD" | "COND") || EXEC_IGNORED.contains(&base)
}

fn symbol_assignments(operands: &[String], line: usize) -> Result<Vec<(String, String)>, Error> {
    operands
        .iter()
        .map(|op| match keyword(op) {
            (Some(k), v) if is_name(k) => Ok((k.to_string(), v.to_string())),
            _ => err(line, format!("{op} is not NAME=value")),
        })
        .collect()
}

/// Maps an error inside a member or procedure to the statement that brought it in.
fn inside(what: &str, line: usize) -> impl Fn(Error) -> Error + '_ {
    move |e| Error { line, message: format!("{what} line {}: {}", e.line, e.message) }
}

impl Expander<'_, '_> {
    fn member(&self, name: &str, line: usize) -> Result<Vec<Raw>, Error> {
        match (self.libraries)(&self.order, name) {
            Ok(Some(text)) => read(&text).map_err(inside(name, line)),
            Ok(None) => err(line, format!("no procedure library holds member {name}")),
            Err(e) => err(line, format!("member {name}: {e}")),
        }
    }

    fn procedure(&self, name: &str, line: usize) -> Result<Procedure, Error> {
        let raws = match self.procs.get(name) {
            Some(r) => r.clone(),
            None => self.member(name, line)?,
        };
        let mut body: &[Raw] = &raws;
        let mut defaults = HashMap::new();
        if let Some(first) = body.first()
            && first.operation == "PROC"
        {
            for (k, v) in symbol_assignments(&split_operands(&first.operands, first.line)?, first.line).map_err(inside(name, line))? {
                defaults.insert(k, v);
            }
            body = &body[1..];
        }
        if body.last().is_some_and(|r| r.operation == "PEND") {
            body = &body[..body.len() - 1];
        }
        if let Some(r) = body.iter().find(|r| matches!(r.operation.as_str(), "PROC" | "PEND" | "JOB")) {
            return Err(inside(name, line)(Error { line: r.line, message: format!("a {} statement inside a procedure", r.operation) }));
        }
        Ok(Procedure { defaults, body: body.to_vec() })
    }

    /// Expands `raws` into items. `caller` is the job step whose EXEC called the procedure being
    /// expanded, and `local` the symbolic parameters in force inside it.
    fn statements(&mut self, raws: &[Raw], caller: Option<&str>, local: &HashMap<String, String>, depth: usize) -> Result<(), Error> {
        if depth > 15 {
            return err(raws.first().map_or(0, |r| r.line), "procedures and INCLUDE members nest more than 15 deep");
        }
        let mut i = 0;
        while i < raws.len() {
            let raw = &raws[i];
            i += 1;
            let mut symbols = self.symbols.clone();
            symbols.extend(local.iter().map(|(k, v)| (k.clone(), v.clone())));
            let operands = if matches!(raw.operation.as_str(), "PROC" | "IF") { raw.operands.clone() } else { substitute(&raw.operands, &symbols, raw.line)? };
            match raw.operation.as_str() {
                "PROC" if caller.is_none() && depth == 0 => {
                    let Some(name) = raw.name.clone().filter(|n| is_name(n)) else { return err(raw.line, "an in-stream PROC needs a name") };
                    let end = raws[i..].iter().position(|r| r.operation == "PEND").ok_or_else(|| Error { line: raw.line, message: format!("PROC {name} has no PEND") })?;
                    let mut body = vec![raw.clone()];
                    body.extend(raws[i..i + end].iter().cloned());
                    self.procs.insert(name, body);
                    i += end + 1;
                }
                "PROC" | "PEND" => return err(raw.line, format!("{} is out of place", raw.operation)),
                "SET" => {
                    for (k, v) in symbol_assignments(&split_operands(&operands, raw.line)?, raw.line)? {
                        self.symbols.insert(k, v);
                    }
                }
                "JCLLIB" => {
                    let ops = split_operands(&operands, raw.line)?;
                    let order = match ops.as_slice() {
                        [o] => o.strip_prefix("ORDER=").ok_or_else(|| Error { line: raw.line, message: "JCLLIB takes ORDER=".into() })?,
                        _ => return err(raw.line, "JCLLIB takes ORDER="),
                    };
                    let inner = order.strip_prefix('(').and_then(|o| o.strip_suffix(')')).unwrap_or(order);
                    self.order = inner.split(',').map(unquote).collect();
                    if let Some(bad) = self.order.iter().find(|d| !is_dsn(d)) {
                        return err(raw.line, format!("{bad} is not a data set name"));
                    }
                }
                "INCLUDE" => {
                    let Some(member) = operands.strip_prefix("MEMBER=").filter(|m| is_name(m)) else { return err(raw.line, "INCLUDE takes MEMBER=name") };
                    let body = self.member(member, raw.line)?;
                    if let Some(r) = body.iter().find(|r| matches!(r.operation.as_str(), "JOB" | "PROC" | "PEND" | "JCLLIB" | "INCLUDE")) {
                        return Err(inside(member, raw.line)(Error { line: r.line, message: format!("a {} statement in an INCLUDE member is not supported", r.operation) }));
                    }
                    self.statements(&body, caller, local, depth + 1).map_err(inside(member, raw.line))?;
                }
                "EXEC" => {
                    let overrides_end = raws[i..].iter().position(|r| r.operation != "DD").map_or(raws.len(), |p| i + p);
                    match exec_program(raw, &operands, caller)? {
                        Some(step) => self.items.push(Item::Step(step)),
                        None => {
                            self.call(raw, &operands, &raws[i..overrides_end], caller, &symbols, depth)?;
                            i = overrides_end;
                        }
                    }
                }
                "DD" => {
                    let before_steps = caller.is_none() && depth == 0 && !self.items.iter().any(|it| matches!(it, Item::Step(_)));
                    if before_steps && raw.name.as_deref() == Some("JOBLIB") && self.joblib.is_none() {
                        self.joblib = Some(Dd { name: "JOBLIB".into(), parts: vec![dd_part(raw, &operands)?] });
                        continue;
                    }
                    if before_steps
                        && raw.name.is_none()
                        && let Some(lib) = &mut self.joblib
                        && raws[i - 2].operation == "DD"
                    {
                        lib.parts.push(dd_part(raw, &operands)?);
                        continue;
                    }
                    let Some(Item::Step(step)) = self.items.last_mut() else { return err(raw.line, "a DD statement that follows no EXEC statement") };
                    let part = dd_part(raw, &operands)?;
                    add_dd(step, raw, part)?;
                }
                "IF" => {
                    let expr = cond::parse_expr(&operands).map_err(|m| Error { line: raw.line, message: m })?;
                    self.items.push(Item::If { expr, caller: caller.map(str::to_string), line: raw.line });
                }
                "ELSE" => self.items.push(Item::Else { line: raw.line }),
                "ENDIF" => self.items.push(Item::EndIf { line: raw.line }),
                "JOB" => return err(raw.line, "a second JOB statement; give one job a file"),
                "OUTPUT" => {}
                op if NOT_SUPPORTED_STATEMENTS.contains(&op) => return err(raw.line, format!("{op} statements are not supported yet")),
                op => return err(raw.line, format!("{op} is not a JCL statement")),
            }
        }
        Ok(())
    }

    /// EXEC of a procedure: its steps with the call's symbolic parameters, then the call's PARM
    /// and COND and the DD overrides and additions after it.
    fn call(&mut self, exec: &Raw, operands: &str, overrides: &[Raw], caller: Option<&str>, outer: &HashMap<String, String>, depth: usize) -> Result<(), Error> {
        let ops = split_operands(operands, exec.line)?;
        let name = match keyword(&ops[0]) {
            (Some("PROC"), n) | (None, n) => n.to_string(),
            _ => return err(exec.line, "EXEC starts with PGM=, PROC= or a procedure name"),
        };
        if !is_name(&name) {
            return err(exec.line, format!("{name} is not a procedure name"));
        }
        let procedure = self.procedure(&name, exec.line)?;
        let mut local = procedure.defaults.clone();
        let mut exec_parms: Vec<(Option<String>, String)> = Vec::new();
        let mut exec_conds: Vec<(Option<String>, Cond)> = Vec::new();
        for op in &ops[1..] {
            let (Some(key), value) = keyword(op) else { return err(exec.line, format!("an EXEC operand {op} this reader does not know")) };
            let (base, step) = match key.split_once('.') {
                Some((b, s)) => (b, Some(s.to_string())),
                None => (key, None),
            };
            match base {
                "PARM" => exec_parms.push((step, parm_value(value))),
                "COND" => exec_conds.push((step, cond::parse_cond(value).map_err(|m| Error { line: exec.line, message: m })?)),
                "PARMDD" => return err(exec.line, "PARMDD is not supported yet"),
                k if EXEC_IGNORED.contains(&k) => {}
                _ if !exec_keyword(key) && is_name(key) => {
                    let used = local.contains_key(key) || procedure.body.iter().any(|r| r.operands.contains(&format!("&{key}")));
                    if !used {
                        return err(exec.line, format!("procedure {name} does not use symbolic parameter {key}"));
                    }
                    local.insert(key.to_string(), value.to_string());
                }
                _ => return err(exec.line, format!("EXEC keyword {key} is not supported yet")),
            }
        }
        let start = self.items.len();
        let caller_name = caller.map(str::to_string).or_else(|| exec.name.clone());
        self.statements(&procedure.body, caller_name.as_deref(), &local, depth + 1).map_err(inside(&name, exec.line))?;
        let steps: Vec<usize> = (start..self.items.len()).filter(|&k| matches!(self.items[k], Item::Step(_))).collect();
        let Some(&first) = steps.first() else { return err(exec.line, format!("procedure {name} has no steps")) };
        let find = |items: &[Item], s: &str| steps.iter().copied().find(|&k| matches!(&items[k], Item::Step(st) if st.name.as_deref() == Some(s)));
        for (step, parm) in exec_parms {
            match step {
                Some(s) => match find(&self.items, &s) {
                    Some(k) => as_step(&mut self.items[k]).parm = Some(parm),
                    None => return err(exec.line, format!("PARM.{s}: procedure {name} has no step {s}")),
                },
                None => {
                    for &k in &steps {
                        as_step(&mut self.items[k]).parm = None;
                    }
                    as_step(&mut self.items[first]).parm = Some(parm);
                }
            }
        }
        for (step, cond) in exec_conds {
            match step {
                Some(s) => match find(&self.items, &s) {
                    Some(k) => as_step(&mut self.items[k]).cond = cond,
                    None => return err(exec.line, format!("COND.{s}: procedure {name} has no step {s}")),
                },
                None => {
                    for &k in &steps {
                        as_step(&mut self.items[k]).cond = cond.clone();
                    }
                }
            }
        }
        let mut target: Option<(usize, String, usize)> = None;
        for raw in overrides {
            let operands = substitute(&raw.operands, outer, raw.line)?;
            let (k, dd, index) = match &raw.name {
                Some(n) => {
                    let (step, dd) = match n.split_once('.') {
                        Some((s, d)) => (find(&self.items, s).ok_or_else(|| Error { line: raw.line, message: format!("procedure {name} has no step {s}") })?, d.to_string()),
                        None => (first, n.clone()),
                    };
                    if !is_name(&dd) {
                        return err(raw.line, format!("{dd} is not a DD name"));
                    }
                    (step, dd, 0)
                }
                None => match &target {
                    Some((k, dd, index)) => (*k, dd.clone(), index + 1),
                    None => return err(raw.line, "an unnamed DD statement with nothing to concatenate to"),
                },
            };
            target = Some((k, dd.clone(), index));
            override_dd(as_step(&mut self.items[k]), &dd, index, raw, &operands)?;
        }
        Ok(())
    }
}

fn as_step(item: &mut Item) -> &mut Step {
    match item {
        Item::Step(s) => s,
        _ => unreachable!("only steps are looked up"),
    }
}

fn parm_value(v: &str) -> String {
    let v = v.strip_prefix('(').and_then(|v| v.strip_suffix(')')).unwrap_or(v);
    unquote(v)
}

/// The step an EXEC PGM= runs, or None for an EXEC that calls a procedure.
fn exec_program(raw: &Raw, operands: &str, caller: Option<&str>) -> Result<Option<Step>, Error> {
    if let Some(n) = &raw.name
        && !is_name(n)
    {
        return err(raw.line, format!("{n} is not a step name"));
    }
    let ops = split_operands(operands, raw.line)?;
    match ops.first().map(|o| keyword(o)) {
        None => return err(raw.line, "EXEC needs PGM= or a procedure"),
        Some((Some("PGM"), _)) => {}
        Some(_) => return Ok(None),
    }
    let (mut pgm, mut parm, mut cond) = (None, None, Cond::default());
    for op in &ops {
        match keyword(op) {
            (Some("PGM"), v) if v.starts_with("*.") => return err(raw.line, "PGM=*.stepname.ddname is not supported yet"),
            (Some("PGM"), v) if is_name(v) => pgm = Some(v.to_string()),
            (Some("PGM"), v) => return err(raw.line, format!("{v} is not a program name")),
            (Some("PARM"), v) => parm = Some(parm_value(v)),
            (Some("PARMDD"), _) => return err(raw.line, "PARMDD is not supported yet"),
            (Some("COND"), v) => cond = cond::parse_cond(v).map_err(|m| Error { line: raw.line, message: m })?,
            (Some(k), _) if EXEC_IGNORED.contains(&k) => {}
            (Some(k), _) => return err(raw.line, format!("EXEC keyword {k} is not supported yet")),
            (None, v) => return err(raw.line, format!("an EXEC operand {v} this reader does not know")),
        }
    }
    let pgm = pgm.expect("the first operand is PGM=");
    Ok(Some(Step { name: raw.name.clone(), caller: caller.map(str::to_string), pgm, parm, cond, dds: Vec::new(), line: raw.line }))
}

fn dataset(value: &str, line: usize) -> Result<Source, Error> {
    let value = unquote(value);
    if let Some(path) = value.strip_prefix("*.") {
        if !path.split('.').all(is_name) || path.split('.').count() > 3 {
            return err(line, format!("*.{path} is not *.ddname, *.stepname.ddname or *.stepname.procstepname.ddname"));
        }
        return Ok(Source::Refer(path.to_string()));
    }
    let (base, member) = match value.split_once('(') {
        Some((b, rest)) => {
            let Some(m) = rest.strip_suffix(')') else { return err(line, format!("{value} is not a data set name")) };
            if m.starts_with(['+', '-']) || (!m.is_empty() && m.bytes().all(|c| c.is_ascii_digit())) {
                let relative: i32 = m.trim_start_matches('+').parse().map_err(|_| Error { line, message: format!("{m} is not a relative generation") })?;
                if !(-255..=255).contains(&relative) || b.starts_with("&&") || !is_dsn(b) || b.len() > 35 {
                    return err(line, format!("{value} is not a generation of a generation data group"));
                }
                return Ok(Source::Generation { base: b.to_string(), relative });
            }
            if !is_name(m) {
                return err(line, format!("{m} is not a member name"));
            }
            (b, Some(m.to_string()))
        }
        None => (value.as_str(), None),
    };
    if base == "NULLFILE" && member.is_none() {
        return Ok(Source::Dummy);
    }
    if let Some(temp) = base.strip_prefix("&&") {
        if !is_name(temp) {
            return err(line, format!("&&{temp} is not a temporary data set name"));
        }
        return Ok(Source::Temporary { name: temp.to_string(), member });
    }
    if !is_dsn(base) {
        return err(line, format!("{base} is not a data set name"));
    }
    Ok(Source::Dataset { dsn: base.to_string(), member })
}

fn disp(value: &str, line: usize) -> Result<Disp, Error> {
    let inner = value.strip_prefix('(').and_then(|v| v.strip_suffix(')')).unwrap_or(value);
    let parts: Vec<&str> = inner.split(',').collect();
    if parts.len() > 3 {
        return err(line, format!("DISP={value} has more than three subparameters"));
    }
    let status = match parts[0] {
        "" | "NEW" => Status::New,
        "OLD" => Status::Old,
        "SHR" => Status::Shr,
        "MOD" => Status::Mod,
        s => return err(line, format!("{s} is not a DISP status")),
    };
    let end = |s: &str, abnormal: bool| -> Result<Option<End>, Error> {
        Ok(Some(match s {
            "" => return Ok(None),
            "DELETE" => End::Delete,
            "KEEP" => End::Keep,
            "PASS" if !abnormal => End::Pass,
            "CATLG" => End::Catlg,
            "UNCATLG" => End::Uncatlg,
            s => return err(line, format!("{s} is not a DISP {} disposition", if abnormal { "abnormal" } else { "normal" })),
        }))
    };
    let normal = parts.get(1).map_or(Ok(None), |s| end(s, false))?;
    let abnormal = parts.get(2).map_or(Ok(None), |s| end(s, true))?;
    Ok(Disp { status, normal, abnormal })
}

/// RECFM and LRECL from a DD's operands, alone or inside DCB=(...).
fn record_format(operands: &[String], line: usize) -> Result<(Option<String>, Option<usize>), Error> {
    let (mut recfm, mut lrecl) = (None, None);
    let mut take = |key: &str, value: &str| -> Result<(), Error> {
        match key {
            "RECFM" => recfm = Some(value.to_string()),
            "LRECL" => match value.parse::<usize>() {
                Ok(n) if n > 0 => lrecl = Some(n),
                _ if value == "X" => {}
                _ => return err(line, format!("LRECL={value} is not a record length")),
            },
            _ => {}
        }
        Ok(())
    };
    for op in operands {
        match keyword(op) {
            (Some("DCB"), v) if v.starts_with('(') => {
                for sub in split_operands(v.trim_start_matches('(').trim_end_matches(')'), line)? {
                    if let (Some(k), sv) = keyword(&sub) {
                        take(k, sv)?;
                    }
                }
            }
            (Some(k @ ("RECFM" | "LRECL")), v) => take(k, v)?,
            _ => {}
        }
    }
    Ok((recfm, lrecl))
}

/// What a DD statement says: its data (a data set, in-stream data, DUMMY or SYSOUT) and its
/// DISP, each None where the statement does not say.
fn dd_fields(raw: &Raw, operands: &str) -> Result<(Option<Source>, Option<Disp>), Error> {
    let (mut source, mut disposition) = (None, None);
    for op in split_operands(operands, raw.line)? {
        match keyword(&op) {
            (None, "*" | "DATA") => source = Some(Source::InStream(raw.data.clone().unwrap_or_default())),
            (None, "DUMMY") => source = Some(Source::Dummy),
            (None, v) => return err(raw.line, format!("a DD operand {v} this reader does not know")),
            (Some("DSN" | "DSNAME"), v) => {
                if source != Some(Source::Dummy) {
                    source = Some(dataset(v, raw.line)?);
                }
            }
            (Some("DISP"), v) => disposition = Some(disp(v, raw.line)?),
            (Some("SYSOUT"), _) => source = Some(Source::Sysout),
            (Some("DLM"), v) => {
                if unquote(v).chars().count() != 2 {
                    return err(raw.line, "DLM takes two characters");
                }
            }
            (Some(k), _) if DD_IGNORED.contains(&k) => {}
            (Some(k), _) => return err(raw.line, format!("DD keyword {k} is not supported yet")),
        }
    }
    if disposition.is_some() && source.as_ref().is_some_and(|s| !matches!(s, Source::Dataset { .. } | Source::Temporary { .. } | Source::Generation { .. } | Source::Refer(_))) {
        return err(raw.line, "DISP applies to a data set");
    }
    Ok((source, disposition))
}

fn dd_part(raw: &Raw, operands: &str) -> Result<Part, Error> {
    let (source, disp) = dd_fields(raw, operands)?;
    let unnamed = (!operands.is_empty()).then(|| Source::Temporary { name: String::new(), member: None });
    let Some(source) = source.or(unnamed) else { return err(raw.line, "the DD statement names no data set, in-stream data, DUMMY or SYSOUT") };
    let (recfm, lrecl) = record_format(&split_operands(operands, raw.line)?, raw.line)?;
    Ok(Part { source, disp: disp.unwrap_or_default(), line: raw.line, recfm, lrecl })
}

fn add_dd(step: &mut Step, raw: &Raw, part: Part) -> Result<(), Error> {
    match &raw.name {
        Some(n) if n.contains('.') => err(raw.line, format!("DD {n} overrides a procedure step, but the EXEC before it runs a program")),
        Some(n) if !is_name(n) => err(raw.line, format!("{n} is not a DD name")),
        Some(n) => {
            if step.dds.iter().any(|d| d.name == *n) {
                return err(raw.line, format!("DD {n} appears twice in the step"));
            }
            step.dds.push(Dd { name: n.clone(), parts: vec![part] });
            Ok(())
        }
        None => match step.dds.last_mut() {
            Some(dd) => {
                dd.parts.push(part);
                Ok(())
            }
            None => err(raw.line, "an unnamed DD statement with nothing to concatenate to"),
        },
    }
}

/// A DD override changes what it states of the procedure's DD, part `index` of a concatenation,
/// and keeps the rest; a DD the step does not have is added to it.
fn override_dd(step: &mut Step, name: &str, index: usize, raw: &Raw, operands: &str) -> Result<(), Error> {
    let (source, disposition) = dd_fields(raw, operands)?;
    match step.dds.iter_mut().find(|d| d.name == name) {
        Some(dd) if index < dd.parts.len() => {
            let part = &mut dd.parts[index];
            if let Some(s) = source {
                if !matches!(s, Source::Dataset { .. } | Source::Temporary { .. } | Source::Generation { .. } | Source::Refer(_)) {
                    part.disp = Disp::default();
                }
                part.source = s;
            }
            if let Some(d) = disposition {
                part.disp = d;
            }
            let (recfm, lrecl) = record_format(&split_operands(operands, raw.line)?, raw.line)?;
            part.recfm = recfm.or(part.recfm.take());
            part.lrecl = lrecl.or(part.lrecl);
            part.line = raw.line;
            Ok(())
        }
        Some(dd) => {
            let part = dd_part(raw, operands)?;
            dd.parts.push(part);
            Ok(())
        }
        None if index == 0 => {
            let part = dd_part(raw, operands)?;
            step.dds.push(Dd { name: name.to_string(), parts: vec![part] });
            Ok(())
        }
        None => err(raw.line, "an unnamed DD statement with nothing to concatenate to"),
    }
}

/// Each DD that asked for a new data set without naming one gets a temporary name of its own.
fn name_unnamed(items: &mut [Item]) {
    let mut n = 0;
    for item in items {
        let Item::Step(step) = item else { continue };
        for part in step.dds.iter_mut().flat_map(|d| d.parts.iter_mut()) {
            if let Source::Temporary { name, .. } = &mut part.source
                && name.is_empty()
            {
                n += 1;
                *name = format!("SYS.{n:05}");
            }
        }
    }
}

/// Each DSN=*.… becomes the data set it names: *.ddname an earlier DD of the same step,
/// *.stepname.ddname a DD of an earlier step (inside a procedure, one of the procedure's steps
/// first), *.stepname.procstepname.ddname a DD of a step in the procedure a job step called.
fn resolve_references(items: &mut [Item]) -> Result<(), Error> {
    for k in 0..items.len() {
        let Item::Step(step) = &items[k] else { continue };
        let caller = step.caller.clone();
        let mut resolved = Vec::new();
        for (d, dd) in step.dds.iter().enumerate() {
            for (p, part) in dd.parts.iter().enumerate() {
                let Source::Refer(path) = &part.source else { continue };
                let names: Vec<&str> = path.split('.').collect();
                let found = match names.as_slice() {
                    [ddname] => step.dds[..d].iter().find(|x| x.name == *ddname).map(|x| &x.parts[0].source),
                    [s, ddname] => {
                        let earlier = |caller_of: Option<&str>| items[..k].iter().rev().find_map(|i| match i {
                            Item::Step(st) if st.name.as_deref() == Some(*s) && st.caller.as_deref() == caller_of => st.dds.iter().find(|x| x.name == *ddname).map(|x| &x.parts[0].source),
                            _ => None,
                        });
                        earlier(caller.as_deref()).or_else(|| earlier(None))
                    }
                    [s, ps, ddname] => items[..k].iter().rev().find_map(|i| match i {
                        Item::Step(st) if st.caller.as_deref() == Some(*s) && st.name.as_deref() == Some(*ps) => st.dds.iter().find(|x| x.name == *ddname).map(|x| &x.parts[0].source),
                        _ => None,
                    }),
                    _ => None,
                };
                match found {
                    Some(src @ (Source::Dataset { .. } | Source::Temporary { .. } | Source::Generation { .. })) => resolved.push((d, p, src.clone())),
                    Some(_) => return err(part.line, format!("*.{path} names a DD that is no data set")),
                    None => return err(part.line, format!("*.{path} names no earlier DD")),
                }
            }
        }
        if let Item::Step(step) = &mut items[k] {
            for (d, p, src) in resolved {
                step.dds[d].parts[p].source = src;
            }
        }
    }
    Ok(())
}

/// IF and ENDIF pair up, ELSE has an IF, and nesting stops at 15 levels.
fn check_nesting(items: &[Item]) -> Result<(), Error> {
    let mut open: Vec<(usize, bool)> = Vec::new();
    for item in items {
        match item {
            Item::If { line, .. } => {
                open.push((*line, false));
                if open.len() > 15 {
                    return err(*line, "IF statements nest more than 15 deep");
                }
            }
            Item::Else { line } => match open.last_mut() {
                Some((_, seen)) if !*seen => *seen = true,
                Some(_) => return err(*line, "a second ELSE for one IF"),
                None => return err(*line, "ELSE without IF"),
            },
            Item::EndIf { line } => {
                if open.pop().is_none() {
                    return err(*line, "ENDIF without IF");
                }
            }
            Item::Step(_) => {}
        }
    }
    match open.last() {
        Some((line, _)) => err(*line, "IF without ENDIF"),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests;
