//! A job's JCL as the z/OS MVS JCL Reference describes it: the JOB statement, EXEC PGM= steps, DD
//! statements with their data sets, dispositions and in-stream data, and IF/THEN/ELSE/ENDIF. What
//! the reader does not model (procedures, symbolic parameters, INCLUDE, generation data groups,
//! backward references) it refuses by name rather than reading it some other way.

pub mod cond;

use cond::Cond;

#[derive(Debug)]
pub struct Job {
    pub name: String,
    pub cond: Cond,
    pub items: Vec<Item>,
}

#[derive(Debug)]
pub enum Item {
    Step(Step),
    If { expr: cond::Expr, line: usize },
    Else { line: usize },
    EndIf { line: usize },
}

#[derive(Debug)]
pub struct Step {
    pub name: Option<String>,
    pub pgm: String,
    pub parm: Option<String>,
    pub cond: Cond,
    pub dds: Vec<Dd>,
    pub line: usize,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Dataset { dsn: String, member: Option<String> },
    /// `&&NAME`, a data set that lasts until the job ends.
    Temporary { name: String, member: Option<String> },
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

/// A name in the name field, a DD name, a step or program name, a member or a qualifier's shape.
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

struct Statement {
    line: usize,
    name: Option<String>,
    operation: String,
    operands: String,
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

/// `KEY=value` as (Some(KEY), value); a positional operand as (None, operand).
fn keyword(operand: &str) -> (Option<&str>, &str) {
    let key_end = operand.find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '#' | '$' | '@' | '-')));
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

fn has_symbol(text: &str) -> bool {
    let mut quoted = false;
    let b = text.as_bytes();
    for (i, &c) in b.iter().enumerate() {
        match c {
            b'\'' => quoted = !quoted,
            b'&' if !quoted => {
                let temp = b.get(i + 1) == Some(&b'&') || (i > 0 && b[i - 1] == b'&');
                if !temp {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

struct Reader<'a> {
    lines: Vec<&'a str>,
    at: usize,
}

impl<'a> Reader<'a> {
    fn peek(&self) -> Option<&'a str> {
        self.lines.get(self.at).copied()
    }

    /// The next statement, joining continuation lines; None at the end of the job.
    fn statement(&mut self) -> Result<Option<Statement>, Error> {
        while let Some(raw) = self.peek() {
            let line = self.at + 1;
            let text = statement_columns(raw);
            if text.starts_with("//*") {
                self.at += 1;
                continue;
            }
            if text.starts_with("/*") {
                self.at += 1;
                continue;
            }
            if !text.starts_with("//") {
                if text.trim().is_empty() {
                    self.at += 1;
                    continue;
                }
                return err(line, "data lines outside a DD * or DD DATA statement");
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
            return Ok(Some(Statement { line, name, operation, operands }));
        }
        Ok(None)
    }

    fn if_statement(&mut self, line: usize, name: Option<String>, first: &str) -> Result<Statement, Error> {
        let mut expr = String::new();
        let mut text = first.to_string();
        loop {
            let words: Vec<&str> = text.split_whitespace().collect();
            if let Some(then) = words.iter().position(|w| *w == "THEN") {
                expr.push_str(&words[..then].join(" "));
                return Ok(Statement { line, name, operation: "IF".into(), operands: expr });
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

const JOB_IGNORED: &[&str] = &["CLASS", "MSGCLASS", "MSGLEVEL", "NOTIFY", "REGION", "TIME", "PRTY", "USER", "GROUP", "JOBRC", "LINES", "ADDRSPC", "BYTES", "CARDS", "PAGES", "PERFORM", "RD", "SCHENV", "SECLABEL", "SYSAFF", "SYSTEM", "UJOBCORR", "MEMLIMIT", "EMAIL"];
const EXEC_IGNORED: &[&str] = &["REGION", "TIME", "ACCT", "ADDRSPC", "DYNAMNBR", "PERFORM", "RD", "MEMLIMIT", "CCSID", "REGIONX", "TVSMSG", "TVSAMCOM"];
const DD_IGNORED: &[&str] = &[
    "DCB", "SPACE", "UNIT", "VOL", "VOLUME", "LRECL", "RECFM", "BLKSIZE", "DSORG", "LABEL", "RETPD", "EXPDT", "STORCLAS", "MGMTCLAS", "DATACLAS", "AVGREC", "FREE", "HOLD", "OUTLIM", "COPIES", "DEST", "DSNTYPE", "BUFNO", "KEYLEN", "EATTR", "FCB", "UCS", "OUTPUT", "SPIN", "SEGMENT", "BLKSZLIM", "DSID",
];
const NOT_SUPPORTED_STATEMENTS: &[&str] = &["PROC", "PEND", "SET", "INCLUDE", "CNTL", "ENDCNTL", "EXPORT", "SCHEDULE", "XMIT", "COMMAND"];

/// Reads one job. Every value is kept as written, and names must be in upper case as z/OS
/// requires.
pub fn parse(text: &str) -> Result<Job, Error> {
    let mut reader = Reader { lines: text.lines().collect(), at: 0 };
    let Some(first) = reader.statement()? else { return err(1, "no JOB statement") };
    if first.operation != "JOB" {
        return err(first.line, "the first statement is not a JOB statement");
    }
    let name = match &first.name {
        Some(n) if is_name(n) => n.clone(),
        _ => return err(first.line, "the JOB statement needs a job name of one to eight characters"),
    };
    let job_cond = job_operands(&first)?;
    if has_symbol(&first.operands) {
        return err(first.line, "symbolic parameters are not supported yet");
    }
    let mut items = Vec::new();
    while let Some(st) = reader.statement()? {
        if has_symbol(&st.operands) && st.operation != "IF" {
            return err(st.line, "symbolic parameters are not supported yet");
        }
        match st.operation.as_str() {
            "EXEC" => items.push(Item::Step(exec_statement(&st)?)),
            "DD" => {
                let Some(Item::Step(step)) = items.last_mut() else { return err(st.line, "a DD statement that follows no EXEC statement") };
                dd_statement(&st, step, &mut reader)?;
            }
            "IF" => items.push(Item::If { expr: cond::parse_expr(&st.operands).map_err(|m| Error { line: st.line, message: m })?, line: st.line }),
            "ELSE" => items.push(Item::Else { line: st.line }),
            "ENDIF" => items.push(Item::EndIf { line: st.line }),
            "JOB" => return err(st.line, "a second JOB statement; give one job a file"),
            "JCLLIB" | "OUTPUT" => {}
            op if NOT_SUPPORTED_STATEMENTS.contains(&op) => return err(st.line, format!("{op} statements are not supported yet")),
            op => return err(st.line, format!("{op} is not a JCL statement")),
        }
    }
    check_nesting(&items)?;
    Ok(Job { name, cond: job_cond, items })
}

fn job_operands(st: &Statement) -> Result<Cond, Error> {
    let mut cond = Cond::default();
    if st.operands.is_empty() {
        return Ok(cond);
    }
    for op in split_operands(&st.operands, st.line)? {
        match keyword(&op) {
            (None, _) => {}
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
    Ok(cond)
}

fn exec_statement(st: &Statement) -> Result<Step, Error> {
    if let Some(n) = &st.name
        && !is_name(n)
    {
        return err(st.line, format!("{n} is not a step name"));
    }
    let (mut pgm, mut parm, mut cond) = (None, None, Cond::default());
    for (i, op) in split_operands(&st.operands, st.line)?.iter().enumerate() {
        match keyword(op) {
            (Some("PGM"), v) if v.starts_with("*.") => return err(st.line, "PGM=*.stepname.ddname is not supported yet"),
            (Some("PGM"), v) if is_name(v) => pgm = Some(v.to_string()),
            (Some("PGM"), v) => return err(st.line, format!("{v} is not a program name")),
            (Some("PROC"), _) => return err(st.line, "EXEC of a procedure is not supported yet"),
            (None, _) if i == 0 => return err(st.line, "EXEC of a procedure is not supported yet"),
            (Some("PARM"), v) => {
                let v = v.strip_prefix('(').and_then(|v| v.strip_suffix(')')).unwrap_or(v);
                parm = Some(unquote(v));
            }
            (Some("PARMDD"), _) => return err(st.line, "PARMDD is not supported yet"),
            (Some("COND"), v) => cond = cond::parse_cond(v).map_err(|m| Error { line: st.line, message: m })?,
            (Some(k), _) if EXEC_IGNORED.contains(&k) => {}
            (Some(k), _) if k.contains('.') => return err(st.line, "EXEC keywords qualified by a procedure step are not supported yet"),
            (Some(k), _) => return err(st.line, format!("EXEC keyword {k} is not supported yet")),
            (None, v) => return err(st.line, format!("an EXEC operand {v} this reader does not know")),
        }
    }
    let Some(pgm) = pgm else { return err(st.line, "EXEC needs PGM=") };
    Ok(Step { name: st.name.clone(), pgm, parm, cond, dds: Vec::new(), line: st.line })
}

fn dataset(value: &str, line: usize) -> Result<Source, Error> {
    let value = unquote(value);
    if value.starts_with("*.") {
        return err(line, "a backward reference (DSN=*.stepname.ddname) is not supported yet");
    }
    let (base, member) = match value.split_once('(') {
        Some((b, rest)) => {
            let Some(m) = rest.strip_suffix(')') else { return err(line, format!("{value} is not a data set name")) };
            if m.starts_with(['+', '-']) || m.bytes().all(|c| c.is_ascii_digit()) {
                return err(line, "a generation data group reference is not supported yet");
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

fn dd_statement(st: &Statement, step: &mut Step, reader: &mut Reader<'_>) -> Result<(), Error> {
    let (mut source, mut disposition, mut in_stream, mut dlm) = (None, None, None, None);
    for op in split_operands(&st.operands, st.line)? {
        match keyword(&op) {
            (None, "*") => in_stream = Some(true),
            (None, "DATA") => in_stream = Some(false),
            (None, "DUMMY") => source = Some(Source::Dummy),
            (None, v) => return err(st.line, format!("a DD operand {v} this reader does not know")),
            (Some("DSN" | "DSNAME"), v) => {
                if source != Some(Source::Dummy) {
                    source = Some(dataset(v, st.line)?);
                }
            }
            (Some("DISP"), v) => disposition = Some(disp(v, st.line)?),
            (Some("SYSOUT"), _) => source = Some(Source::Sysout),
            (Some("DLM"), v) => {
                let d = unquote(v);
                if d.chars().count() != 2 {
                    return err(st.line, "DLM takes two characters");
                }
                dlm = Some(d);
            }
            (Some(k), _) if DD_IGNORED.contains(&k) => {}
            (Some(k), _) => return err(st.line, format!("DD keyword {k} is not supported yet")),
        }
    }
    if let Some(star) = in_stream {
        source = Some(Source::InStream(reader.in_stream(star, dlm.as_deref())));
    }
    let Some(source) = source else { return err(st.line, "the DD statement names no data set, in-stream data, DUMMY or SYSOUT") };
    if disposition.is_some() && !matches!(source, Source::Dataset { .. } | Source::Temporary { .. }) {
        return err(st.line, "DISP applies to a data set");
    }
    let part = Part { source, disp: disposition.unwrap_or_default(), line: st.line };
    match &st.name {
        Some(n) if n.contains('.') => err(st.line, "a DD override for a procedure step is not supported yet"),
        Some(n) if !is_name(n) => err(st.line, format!("{n} is not a DD name")),
        Some(n) => {
            if step.dds.iter().any(|d| d.name == *n) {
                return err(st.line, format!("DD {n} appears twice in the step"));
            }
            step.dds.push(Dd { name: n.clone(), parts: vec![part] });
            Ok(())
        }
        None => match step.dds.last_mut() {
            Some(dd) => {
                dd.parts.push(part);
                Ok(())
            }
            None => err(st.line, "an unnamed DD statement with nothing to concatenate to"),
        },
    }
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
