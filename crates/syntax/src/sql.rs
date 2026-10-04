//! EXEC SQL statements, typed: what each one sends, what it receives into, and the WHENEVER actions
//! in force where it stands in the listing. The text a backend runs is the statement with each
//! input host variable replaced by `?` and any INTO list removed, in one canonical spelling.

use crate::Pos;
use crate::ast::{Expr, Literal, Operand, Ref};
use std::collections::HashMap;

/// A host variable and the indicator variable that follows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostVar {
    pub var: Ref,
    pub indicator: Option<Ref>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Action {
    #[default]
    Continue,
    GoTo(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Condition {
    SqlError,
    NotFound,
    SqlWarning,
}

/// The WHENEVER actions in force at a statement.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Whenever {
    pub sqlerror: Action,
    pub not_found: Action,
    pub sqlwarning: Action,
}

impl Whenever {
    pub fn set(&mut self, condition: Condition, action: Action) {
        match condition {
            Condition::SqlError => self.sqlerror = action,
            Condition::NotFound => self.not_found = action,
            Condition::SqlWarning => self.sqlwarning = action,
        }
    }
}

/// What DESCRIBE puts in each SQLNAME: USING NAMES, LABELS or ANY.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Names {
    #[default]
    Names,
    Labels,
    Any,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    Insert,
    Update,
    Delete,
}

/// A cursor as its DECLARE gives it: the query OPEN runs and the host variables OPEN sends, or
/// the prepared statement it runs, whose inputs OPEN ... USING sends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub name: String,
    pub text: String,
    pub inputs: Vec<HostVar>,
    pub with_hold: bool,
    /// The statement name of a cursor for a prepared statement; its `text` and `inputs` are empty.
    pub statement: Option<String>,
    /// WITH ROWSET POSITIONING, which a rowset FETCH needs.
    pub rowset: bool,
}

/// FOR n ROWS: absent, so a rowset FETCH asks for as many rows as the cursor's last one did, or
/// one; a constant; or a host variable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rows {
    Implicit,
    Constant(u32),
    Host(Box<HostVar>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Statement {
    /// SELECT ... INTO, VALUES ... INTO, and SET :host-variable = expression.
    Query { text: String, inputs: Vec<HostVar>, into: Vec<HostVar> },
    /// INSERT, UPDATE and DELETE; `current_of` names the cursor of a positioned UPDATE or DELETE.
    Change { kind: ChangeKind, text: String, inputs: Vec<HostVar>, current_of: Option<String> },
    DeclareCursor(Cursor),
    /// A cursor ironwork does not run, such as a scrollable one, and what it is.
    DeclareUnsupported { name: String, what: String },
    /// `declared` is the cursor's DECLARE, which [`Cursors::resolve`] fills; `using` the host
    /// variables OPEN ... USING sends to a prepared statement's parameter markers, or `descriptor`
    /// the SQLDA OPEN ... USING DESCRIPTOR names.
    Open { cursor: String, declared: Option<Cursor>, using: Vec<HostVar>, descriptor: Option<HostVar> },
    Fetch { cursor: String, into: Vec<HostVar> },
    FetchDescriptor { cursor: String, descriptor: HostVar },
    /// FETCH NEXT ROWSET into host-variable arrays; `enabled` is whether the cursor's DECLARE
    /// allows rowsets, which [`Cursors::resolve`] fills.
    FetchRowset { cursor: String, rows: Rows, into: Vec<HostVar>, enabled: bool },
    Close { cursor: String },
    /// INSERT ... FOR n ROWS: `text` and `inputs` are the INSERT of one row, which runs for each.
    InsertRows { text: String, inputs: Vec<HostVar>, rows: Rows, atomic: bool },
    /// CALL of a stored procedure: `text` names it with `?` for each host-variable argument.
    Call { procedure: String, text: String, args: Vec<HostVar> },
    Commit,
    Rollback,
    /// PREPARE: the statement name, the host variable holding the statement string, and the SQLDA
    /// PREPARE ... INTO describes the statement in.
    Prepare { name: String, source: HostVar, into: Option<(HostVar, Names)> },
    ExecuteImmediate { source: HostVar },
    /// EXECUTE of a prepared statement, with the host variables USING sends or the SQLDA USING
    /// DESCRIPTOR names.
    Execute { name: String, inputs: Vec<HostVar>, descriptor: Option<HostVar> },
    /// DESCRIBE [OUTPUT] of a prepared statement into an SQLDA.
    Describe { name: String, descriptor: HostVar, names: Names },
    Whenever { condition: Condition, action: Action },
    /// INCLUDE, DECLARE SECTION, DECLARE TABLE and DECLARE STATEMENT, which declare and do nothing.
    Declaration,
    /// CONNECT and SET CONNECTION, which ironwork does not run: `what` names the statement, and
    /// `target` is the host variable naming the location, where one does.
    Connect { what: String, target: Option<HostVar> },
    /// A statement ironwork does not run, named by what it is.
    Unsupported(String),
    /// A statement the precompiler refuses, with the reason: it does not read as its verb requires,
    /// or it is not Db2 for z/OS.
    Malformed(String),
}

impl Statement {
    /// Every host variable and indicator the statement names.
    pub fn references(&self) -> Vec<&Ref> {
        let vars: Vec<&HostVar> = match self {
            Statement::Query { inputs, into, .. } => into.iter().chain(inputs).collect(),
            Statement::Change { inputs, .. } | Statement::DeclareCursor(Cursor { inputs, .. }) => inputs.iter().collect(),
            Statement::Fetch { into, .. } => into.iter().collect(),
            Statement::FetchRowset { rows, into, .. } => into.iter().chain(rows.host()).collect(),
            Statement::InsertRows { inputs, rows, .. } => inputs.iter().chain(rows.host()).collect(),
            Statement::Call { args, .. } => args.iter().collect(),
            Statement::Open { using, descriptor, .. } | Statement::Execute { inputs: using, descriptor, .. } => using.iter().chain(descriptor).collect(),
            Statement::Prepare { source, into, .. } => std::iter::once(source).chain(into.as_ref().map(|(d, _)| d)).collect(),
            Statement::ExecuteImmediate { source } | Statement::FetchDescriptor { descriptor: source, .. } | Statement::Describe { descriptor: source, .. } => vec![source],
            _ => Vec::new(),
        };
        vars.into_iter().flat_map(|h| std::iter::once(&h.var).chain(&h.indicator)).collect()
    }
}

impl Rows {
    fn host(&self) -> Option<&HostVar> {
        match self {
            Rows::Host(h) => Some(h.as_ref()),
            _ => None,
        }
    }
}

/// The cursors declared so far in a listing. The precompiler reads a cursor's name against the
/// DECLAREs before it, so a statement naming a cursor declared later, or never, is refused.
#[derive(Debug, Default)]
pub struct Cursors(HashMap<String, Result<Cursor, String>>);

impl Cursors {
    /// Records a DECLARE CURSOR, gives OPEN its cursor, and refuses a cursor not yet declared.
    pub fn resolve(&mut self, statement: Statement) -> Statement {
        let named = match &statement {
            Statement::DeclareCursor(c) => {
                self.0.insert(c.name.clone(), Ok(c.clone()));
                return statement;
            }
            Statement::DeclareUnsupported { name, what } => {
                self.0.insert(name.clone(), Err(what.clone()));
                return statement;
            }
            Statement::Open { cursor, .. }
            | Statement::Fetch { cursor, .. }
            | Statement::FetchDescriptor { cursor, .. }
            | Statement::FetchRowset { cursor, .. }
            | Statement::Close { cursor, .. }
            | Statement::Change { current_of: Some(cursor), .. } => cursor.clone(),
            _ => return statement,
        };
        match (self.0.get(&named), statement) {
            (None, _) => Statement::Malformed(format!("cursor {named} is not declared before this statement")),
            (Some(Err(what)), _) => Statement::Unsupported(what.clone()),
            (Some(Ok(c)), Statement::Open { using, descriptor, .. }) if c.statement.is_none() && (!using.is_empty() || descriptor.is_some()) => {
                Statement::Unsupported("OPEN ... USING of a cursor declared for a select-statement".into())
            }
            (Some(Ok(c)), Statement::Open { cursor, using, descriptor, .. }) => Statement::Open { cursor, declared: Some(c.clone()), using, descriptor },
            (Some(Ok(c)), Statement::FetchRowset { cursor, rows, into, .. }) => Statement::FetchRowset { cursor, rows, into, enabled: c.rowset },
            (Some(Ok(_)), statement) => statement,
        }
    }
}

/// An EXEC SQL block's typed statement, its place among the program's EXEC SQL blocks in listing
/// order (from 1), and the WHENEVER actions in force where it stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sql {
    pub statement: Statement,
    pub ordinal: u32,
    pub whenever: Whenever,
}

pub use rt::sql::fingerprint;

/// The statement in `body`, the text between EXEC SQL and END-EXEC.
pub fn parse(body: &str, pos: Pos) -> Statement {
    match lex(body) {
        Ok(toks) => statement(&toks, pos),
        Err(why) => Statement::Malformed(why),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Tok {
    Word(String),
    Quoted(String),
    Number(String),
    Punct(char),
    Host { path: Vec<String>, subscripts: Vec<String> },
}

fn lex(sql: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = sql.chars().collect();
    let (mut out, mut i) = (Vec::new(), 0);
    let text = |from: usize, to: usize| chars[from..to].iter().collect::<String>();
    while let Some(&c) = chars.get(i) {
        if c.is_whitespace() {
            i += 1;
        } else if c == '\'' || c == '"' {
            let start = i;
            i += 1;
            loop {
                match chars.get(i) {
                    None => return Err("a quoted string is not closed".into()),
                    Some(&q) if q == c && chars.get(i + 1) == Some(&c) => i += 2,
                    Some(&q) if q == c => {
                        i += 1;
                        break;
                    }
                    Some(_) => i += 1,
                }
            }
            out.push(Tok::Quoted(text(start, i)));
        } else if c == ':' && chars.get(i + 1).is_some_and(char::is_ascii_alphanumeric) {
            i += 1;
            let mut path = Vec::new();
            loop {
                let start = i;
                while chars.get(i).is_some_and(|&n| n.is_ascii_alphanumeric() || n == '-' || n == '_') {
                    i += 1;
                }
                path.push(text(start, i).to_ascii_uppercase());
                if chars.get(i) == Some(&'.') && chars.get(i + 1).is_some_and(char::is_ascii_alphanumeric) {
                    i += 1;
                } else {
                    break;
                }
            }
            let mut subscripts = Vec::new();
            if chars.get(i) == Some(&'(') {
                let close = chars[i..].iter().position(|&n| n == ')').ok_or("a subscript is not closed")? + i;
                subscripts = text(i + 1, close).split(',').map(|s| s.trim().to_ascii_uppercase()).collect();
                if subscripts.iter().any(String::is_empty) {
                    return Err("a subscript is empty".into());
                }
                i = close + 1;
            }
            out.push(Tok::Host { path, subscripts });
        } else if c.is_ascii_alphabetic() || matches!(c, '_' | '#' | '@' | '$') {
            // Cursor and statement names in COBOL programs take hyphens, as in PROGRAMS-CSR.
            let start = i;
            while chars.get(i).is_some_and(|&n| {
                n.is_ascii_alphanumeric() || matches!(n, '_' | '#' | '@' | '$') || (n == '-' && chars.get(i + 1).is_some_and(char::is_ascii_alphanumeric))
            }) {
                i += 1;
            }
            out.push(Tok::Word(text(start, i).to_ascii_uppercase()));
        } else if c.is_ascii_digit() {
            let start = i;
            while chars.get(i).is_some_and(|&n| n.is_ascii_digit() || n == '.') {
                i += 1;
            }
            out.push(Tok::Number(text(start, i)));
        } else {
            out.push(Tok::Punct(c));
            i += 1;
        }
    }
    Ok(out)
}

fn word(toks: &[Tok], i: usize) -> &str {
    match toks.get(i) {
        Some(Tok::Word(w)) => w,
        _ => "",
    }
}

fn reference(path: &[String], subscripts: &[String], pos: Pos) -> Ref {
    let mut qualifiers = path.to_vec();
    let name = qualifiers.pop().unwrap_or_default();
    qualifiers.reverse();
    let subscripts = subscripts
        .iter()
        .map(|s| {
            Expr::Operand(if s.chars().all(|c| c.is_ascii_digit()) {
                Operand::Literal(Literal::Number(s.clone()))
            } else {
                Operand::Ref(Ref { name: s.clone(), qualifiers: Vec::new(), subscripts: Vec::new(), refmod: None, pos })
            })
        })
        .collect();
    Ref { name, qualifiers, subscripts, refmod: None, pos }
}

/// The host variable starting at `toks[i]`, with the indicator after it, and where it ends.
fn host_var(toks: &[Tok], i: usize, pos: Pos) -> Option<(HostVar, usize)> {
    let Some(Tok::Host { path, subscripts }) = toks.get(i) else { return None };
    let var = reference(path, subscripts, pos);
    let at = if word(toks, i + 1) == "INDICATOR" { i + 2 } else { i + 1 };
    match toks.get(at) {
        Some(Tok::Host { path, subscripts }) => Some((HostVar { var, indicator: Some(reference(path, subscripts, pos)) }, at + 1)),
        _ => Some((HostVar { var, indicator: None }, i + 1)),
    }
}

/// The canonical text of `toks`, each host variable replaced by `?`, and those host variables.
fn render(toks: &[Tok], pos: Pos) -> (String, Vec<HostVar>) {
    let (mut pieces, mut vars, mut i) = (Vec::<String>::new(), Vec::new(), 0);
    while i < toks.len() {
        if let Some((var, next)) = host_var(toks, i, pos) {
            vars.push(var);
            pieces.push("?".into());
            i = next;
            continue;
        }
        pieces.push(match &toks[i] {
            Tok::Word(s) | Tok::Quoted(s) | Tok::Number(s) => s.clone(),
            Tok::Punct(c) => c.to_string(),
            Tok::Host { .. } => unreachable!("host_var takes every host token"),
        });
        i += 1;
    }
    let mut text = String::new();
    for (n, piece) in pieces.iter().enumerate() {
        let operator = n > 0 && matches!((pieces[n - 1].as_str(), piece.as_str()), ("<" | ">" | "!" | "¬" | "^", "=") | ("<", ">") | ("|", "|"));
        let glued = n == 0 || operator || matches!(piece.as_str(), ")" | "," | ".") || matches!(pieces[n - 1].as_str(), "(" | ".");
        if !glued {
            text.push(' ');
        }
        text.push_str(piece);
    }
    (text, vars)
}

/// A comma-separated list of host variables, as INTO and FETCH ... INTO take.
fn host_list(toks: &[Tok], pos: Pos) -> Result<Vec<HostVar>, String> {
    let (mut out, mut i) = (Vec::new(), 0);
    while i < toks.len() {
        // Only host variables stand in an INTO list, so a name written without its colon is one
        // (assumption S7).
        let (var, next) = match toks.get(i) {
            Some(Tok::Word(name)) => (HostVar { var: reference(std::slice::from_ref(name), &[], pos), indicator: None }, i + 1),
            _ => host_var(toks, i, pos).ok_or("INTO lists something other than host variables")?,
        };
        out.push(var);
        i = next;
        match toks.get(i) {
            None => {}
            Some(Tok::Punct(',')) => i += 1,
            Some(_) => return Err("INTO lists something other than host variables".into()),
        }
    }
    if out.is_empty() {
        return Err("INTO names no host variable".into());
    }
    Ok(out)
}

/// Where `target` first stands outside parentheses, from `from`.
fn top_level(toks: &[Tok], from: usize, target: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (i, t) in toks.iter().enumerate().skip(from) {
        match t {
            Tok::Punct('(') => depth += 1,
            Tok::Punct(')') => depth -= 1,
            Tok::Word(w) if depth == 0 && w == target => return Some(i),
            _ => {}
        }
    }
    None
}

fn statement(toks: &[Tok], pos: Pos) -> Statement {
    let verb = word(toks, 0);
    match verb {
        "SELECT" | "VALUES" => into_query(toks, pos),
        "SET" if matches!(toks.get(1), Some(Tok::Host { .. })) => set_host(toks, pos),
        "SET" if word(toks, 1) == "CONNECTION" => connect("SET CONNECTION", toks, 2, pos),
        "CONNECT" if word(toks, 1) == "TO" => connect("CONNECT", toks, 2, pos),
        "CONNECT" => Statement::Connect { what: "CONNECT".into(), target: None },
        "INSERT" => insert(toks, pos),
        "UPDATE" => change(ChangeKind::Update, toks, pos),
        "DELETE" => change(ChangeKind::Delete, toks, pos),
        "DECLARE" => declare(toks, pos),
        "OPEN" => open(toks, pos),
        "FETCH" => fetch(toks, pos),
        "CLOSE" => match (word(toks, 1), toks.len()) {
            ("", _) => Statement::Malformed("CLOSE names no cursor".into()),
            (cursor, 2) => Statement::Close { cursor: cursor.into() },
            _ => Statement::Malformed("CLOSE takes only a cursor name".into()),
        },
        "COMMIT" if toks.len() == 1 || (toks.len() == 2 && word(toks, 1) == "WORK") => Statement::Commit,
        "ROLLBACK" if toks.len() == 1 || (toks.len() == 2 && word(toks, 1) == "WORK") => Statement::Rollback,
        "ROLLBACK" => Statement::Unsupported("ROLLBACK TO SAVEPOINT".into()),
        "PREPARE" => prepare(toks, pos),
        "EXECUTE" if word(toks, 1) == "IMMEDIATE" => match source(toks, 2, pos) {
            Ok(source) => Statement::ExecuteImmediate { source },
            Err(why) => Statement::Malformed(format!("EXECUTE IMMEDIATE {why}")),
        },
        "EXECUTE" => execute(toks, pos),
        "DESCRIBE" => describe(toks, pos),
        "CALL" => call(toks, pos),
        "WHENEVER" => whenever(toks),
        "INCLUDE" => Statement::Declaration,
        "BEGIN" | "END" if word(toks, 1) == "DECLARE" => Statement::Declaration,
        "DISCONNECT" => Statement::Malformed("DISCONNECT is not a Db2 for z/OS statement; Db2 ends a connection with RELEASE and a commit".into()),
        "" => Statement::Malformed("the block holds no statement".into()),
        other => Statement::Unsupported(other.into()),
    }
}

/// CONNECT TO or SET CONNECTION, its location at `toks[at]`: a host variable, or a name.
fn connect(what: &str, toks: &[Tok], at: usize, pos: Pos) -> Statement {
    Statement::Connect { what: what.into(), target: host_var(toks, at, pos).map(|(var, _)| var) }
}

fn into_query(toks: &[Tok], pos: Pos) -> Statement {
    let Some(into) = top_level(toks, 1, "INTO") else {
        return Statement::Malformed(format!("{} has no INTO", word(toks, 0)));
    };
    let end = top_level(toks, into + 1, "FROM").unwrap_or(toks.len());
    let into_vars = match host_list(&toks[into + 1..end], pos) {
        Ok(v) => v,
        Err(why) => return Statement::Malformed(why),
    };
    let rest: Vec<Tok> = toks[..into].iter().chain(&toks[end..]).cloned().collect();
    let (text, inputs) = render(&rest, pos);
    Statement::Query { text, inputs, into: into_vars }
}

fn set_host(toks: &[Tok], pos: Pos) -> Statement {
    let Some((target, next)) = host_var(toks, 1, pos) else { unreachable!("statement checks the host variable") };
    if toks.get(next) != Some(&Tok::Punct('=')) || next + 1 >= toks.len() {
        return Statement::Malformed("SET :host-variable takes = and an expression".into());
    }
    let expression: Vec<Tok> = std::iter::once(Tok::Word("VALUES".into())).chain(toks[next + 1..].iter().cloned()).collect();
    let (text, inputs) = render(&expression, pos);
    Statement::Query { text, inputs, into: vec![target] }
}

fn change(kind: ChangeKind, toks: &[Tok], pos: Pos) -> Statement {
    let n = toks.len();
    if n >= 5 && (word(toks, n - 5), word(toks, n - 4), word(toks, n - 2), word(toks, n - 1)) == ("FOR", "ROW", "OF", "ROWSET") {
        return Statement::Unsupported("a positioned UPDATE or DELETE FOR ROW n OF ROWSET".into());
    }
    let current_of = (n >= 4 && word(toks, n - 4) == "WHERE" && word(toks, n - 3) == "CURRENT" && word(toks, n - 2) == "OF" && !word(toks, n - 1).is_empty())
        .then(|| word(toks, n - 1).to_owned());
    let (text, inputs) = render(toks, pos);
    Statement::Change { kind, text, inputs, current_of }
}

fn declare(toks: &[Tok], pos: Pos) -> Statement {
    let name = word(toks, 1);
    match word(toks, 2) {
        "TABLE" | "STATEMENT" => return Statement::Declaration,
        _ if name.is_empty() => return Statement::Malformed("DECLARE names nothing".into()),
        _ => {}
    }
    let Some(cursor) = (2..toks.len()).find(|&i| word(toks, i) == "CURSOR") else {
        return Statement::Unsupported(format!("DECLARE {}", word(toks, 2)));
    };
    let unsupported = |what: &str| Statement::DeclareUnsupported { name: name.into(), what: what.into() };
    if (2..cursor).any(|i| word(toks, i) == "SCROLL" && word(toks, i - 1) != "NO") {
        return unsupported("a scrollable cursor");
    }
    let Some(for_at) = top_level(toks, cursor + 1, "FOR") else {
        return Statement::Malformed("DECLARE CURSOR has no FOR".into());
    };
    let with_hold = (cursor + 1..for_at).any(|i| word(toks, i) == "HOLD" && word(toks, i - 1) == "WITH");
    let rowset = (cursor + 1..for_at).any(|i| word(toks, i) == "ROWSET" && word(toks, i - 1) == "WITH");
    let query = &toks[for_at + 1..];
    if !matches!(word(query, 0), "SELECT" | "WITH" | "VALUES") && !matches!(query.first(), Some(Tok::Punct('('))) {
        return match query {
            [Tok::Word(statement)] => Statement::DeclareCursor(Cursor { name: name.into(), text: String::new(), inputs: Vec::new(), with_hold, statement: Some(statement.clone()), rowset }),
            _ => Statement::Malformed("DECLARE CURSOR ... FOR takes a select-statement or a statement name".into()),
        };
    }
    let (text, inputs) = render(query, pos);
    Statement::DeclareCursor(Cursor { name: name.into(), text, inputs, with_hold, statement: None, rowset })
}

/// A comma-separated list of host variables after USING.
fn using_list(toks: &[Tok], pos: Pos) -> Result<Vec<HostVar>, String> {
    let (mut out, mut i) = (Vec::new(), 0);
    while i < toks.len() {
        let (var, next) = host_var(toks, i, pos).ok_or("USING lists something other than host variables")?;
        out.push(var);
        i = next;
        match toks.get(i) {
            None => {}
            Some(Tok::Punct(',')) if i + 1 < toks.len() => i += 1,
            Some(_) => return Err("USING lists something other than host variables".into()),
        }
    }
    if out.is_empty() {
        return Err("USING names no host variable".into());
    }
    Ok(out)
}

/// OPEN, and the host variables OPEN ... USING sends.
fn open(toks: &[Tok], pos: Pos) -> Statement {
    let cursor = word(toks, 1);
    if cursor.is_empty() {
        return Statement::Malformed("OPEN names no cursor".into());
    }
    match (toks.len(), word(toks, 2), word(toks, 3)) {
        (2, _, _) => Statement::Open { cursor: cursor.into(), declared: None, using: Vec::new(), descriptor: None },
        (_, "USING", "DESCRIPTOR") => match descriptor(toks, 4, pos) {
            Ok((d, next)) if next == toks.len() => Statement::Open { cursor: cursor.into(), declared: None, using: Vec::new(), descriptor: Some(d) },
            Ok(_) => Statement::Malformed("OPEN ... USING DESCRIPTOR takes only the descriptor".into()),
            Err(why) => Statement::Malformed(why),
        },
        (_, "USING", _) => match using_list(&toks[3..], pos) {
            Ok(using) => Statement::Open { cursor: cursor.into(), declared: None, using, descriptor: None },
            Err(why) => Statement::Malformed(why),
        },
        _ => Statement::Malformed("OPEN takes a cursor name and USING".into()),
    }
}

/// The statement string's host variable at `toks[at]`, the statement's last token. Db2 takes no
/// indicator with it, and a string expression only in PL/I (Db2 13 SQL, PREPARE).
fn source(toks: &[Tok], at: usize, pos: Pos) -> Result<HostVar, String> {
    match (toks.get(at), host_var(toks, at, pos)) {
        (Some(Tok::Quoted(_)), _) => Err("takes a host variable; a string expression is PL/I's".into()),
        (_, Some((HostVar { indicator: Some(_), .. }, _))) => Err("takes no indicator variable with the statement string".into()),
        (_, Some((var, next))) if next == toks.len() => Ok(var),
        _ => Err("takes the host variable holding the statement string".into()),
    }
}

fn prepare(toks: &[Tok], pos: Pos) -> Statement {
    let name = word(toks, 1);
    if name.is_empty() {
        return Statement::Malformed("PREPARE names no statement".into());
    }
    let (into, from) = match word(toks, 2) {
        "INTO" => match descriptor(toks, 3, pos).and_then(|(d, next)| names(toks, next).map(|(n, next)| (d, n, next))) {
            Ok((_, None, _)) => return Statement::Unsupported("PREPARE ... INTO ... USING BOTH".into()),
            Ok((d, Some(n), next)) => (Some((d, n)), next),
            Err(why) => return Statement::Malformed(why),
        },
        _ => (None, 2),
    };
    match word(toks, from) {
        "ATTRIBUTES" => Statement::Unsupported("PREPARE ... ATTRIBUTES".into()),
        "FROM" => match source(toks, from + 1, pos) {
            Ok(source) => Statement::Prepare { name: name.into(), source, into },
            Err(why) => Statement::Malformed(format!("PREPARE ... FROM {why}")),
        },
        _ => Statement::Malformed("PREPARE takes a statement name and FROM".into()),
    }
}

/// The SQLDA host variable at `toks[at]`, which takes no indicator, and where it ends.
fn descriptor(toks: &[Tok], at: usize, pos: Pos) -> Result<(HostVar, usize), String> {
    match host_var(toks, at, pos) {
        Some((HostVar { indicator: Some(_), .. }, _)) => Err("a descriptor takes no indicator variable".into()),
        Some((d, next)) => Ok((d, next)),
        None => Err("DESCRIPTOR and INTO name the SQLDA as a host variable".into()),
    }
}

/// An optional USING NAMES, LABELS, ANY or BOTH from `toks[at]`: None for BOTH, which ironwork does
/// not run, and where it ends.
fn names(toks: &[Tok], at: usize) -> Result<(Option<Names>, usize), String> {
    if word(toks, at) != "USING" {
        return Ok((Some(Names::Names), at));
    }
    let names = match word(toks, at + 1) {
        "NAMES" => Some(Names::Names),
        "LABELS" => Some(Names::Labels),
        "ANY" => Some(Names::Any),
        "BOTH" => None,
        _ => return Err("USING takes NAMES, LABELS, ANY or BOTH".into()),
    };
    Ok((names, at + 2))
}

/// DESCRIBE [OUTPUT] statement-name INTO descriptor [USING ...].
fn describe(toks: &[Tok], pos: Pos) -> Statement {
    let at = match word(toks, 1) {
        "OUTPUT" => 2,
        "INPUT" => return Statement::Unsupported("DESCRIBE INPUT".into()),
        "CURSOR" | "PROCEDURE" | "TABLE" => return Statement::Unsupported(format!("DESCRIBE {}", word(toks, 1))),
        _ => 1,
    };
    let name = word(toks, at);
    if name.is_empty() || word(toks, at + 1) != "INTO" {
        return Statement::Malformed("DESCRIBE takes a statement name and INTO".into());
    }
    match descriptor(toks, at + 2, pos).and_then(|(d, next)| names(toks, next).map(|(n, next)| (d, n, next))) {
        Ok((_, None, _)) => Statement::Unsupported("DESCRIBE ... USING BOTH".into()),
        Ok((descriptor, Some(names), next)) if next == toks.len() => Statement::Describe { name: name.into(), descriptor, names },
        Ok(_) => Statement::Malformed("DESCRIBE ends with INTO and USING".into()),
        Err(why) => Statement::Malformed(why),
    }
}

fn execute(toks: &[Tok], pos: Pos) -> Statement {
    let name = word(toks, 1);
    if name.is_empty() {
        return Statement::Malformed("EXECUTE names no statement".into());
    }
    match (toks.len(), word(toks, 2), word(toks, 3)) {
        (2, _, _) => Statement::Execute { name: name.into(), inputs: Vec::new(), descriptor: None },
        (_, "USING", "DESCRIPTOR") if top_level(toks, 4, "FOR").is_none() => match descriptor(toks, 4, pos) {
            Ok((d, next)) if next == toks.len() => Statement::Execute { name: name.into(), inputs: Vec::new(), descriptor: Some(d) },
            Ok(_) => Statement::Malformed("EXECUTE ... USING DESCRIPTOR takes only the descriptor".into()),
            Err(why) => Statement::Malformed(why),
        },
        (_, "USING", _) if top_level(toks, 3, "FOR").is_none() => match using_list(&toks[3..], pos) {
            Ok(inputs) => Statement::Execute { name: name.into(), inputs, descriptor: None },
            Err(why) => Statement::Malformed(why),
        },
        (_, "USING" | "FOR", _) => Statement::Unsupported("a multi-row EXECUTE".into()),
        _ => Statement::Malformed("EXECUTE takes a statement name and USING".into()),
    }
}

/// A FETCH: row-positioned NEXT, or NEXT ROWSET, the one rowset orientation a cursor that does
/// not scroll takes (Db2 13 SQL, FETCH).
fn fetch(toks: &[Tok], pos: Pos) -> Statement {
    let rowset = word(toks, 1) == "NEXT" && word(toks, 2) == "ROWSET";
    let mut i = if rowset { 3 } else { 1 };
    if !rowset && word(toks, i) == "NEXT" {
        i += 1;
    }
    if matches!(word(toks, i), "PRIOR" | "FIRST" | "LAST" | "ABSOLUTE" | "RELATIVE" | "BEFORE" | "AFTER" | "CURRENT" | "SENSITIVE" | "INSENSITIVE" | "ROWSET") {
        return Statement::Unsupported("a scrollable FETCH".into());
    }
    if word(toks, i) == "FROM" {
        i += 1;
    }
    let cursor = word(toks, i);
    if cursor.is_empty() {
        return Statement::Malformed("FETCH names no cursor".into());
    }
    i += 1;
    if rowset {
        return fetch_rowset(cursor, &toks[i..], pos);
    }
    match word(toks, i) {
        "" if i == toks.len() => Statement::Fetch { cursor: cursor.into(), into: Vec::new() },
        "INTO" => match host_list(&toks[i + 1..], pos) {
            Ok(into) => Statement::Fetch { cursor: cursor.into(), into },
            Err(why) => Statement::Malformed(why),
        },
        "FOR" => Statement::Malformed("FOR n ROWS takes the rowset orientation NEXT ROWSET".into()),
        "USING" if word(toks, i + 1) == "DESCRIPTOR" => match descriptor(toks, i + 2, pos) {
            Ok((descriptor, next)) if next == toks.len() => Statement::FetchDescriptor { cursor: cursor.into(), descriptor },
            Ok(_) => Statement::Malformed("FETCH ... USING DESCRIPTOR takes only the descriptor".into()),
            Err(why) => Statement::Malformed(why),
        },
        _ => Statement::Malformed("FETCH takes a cursor and INTO".into()),
    }
}

/// FETCH NEXT ROWSET's clauses after the cursor: FOR n ROWS, then INTO.
fn fetch_rowset(cursor: &str, toks: &[Tok], pos: Pos) -> Statement {
    let (rows, at) = match word(toks, 0) {
        "FOR" => match rows_clause(toks, pos) {
            Ok(clause) => clause,
            Err(why) => return Statement::Malformed(why),
        },
        _ => (Rows::Implicit, 0),
    };
    let rest = &toks[at..];
    let into = match (word(rest, 0), word(rest, 1)) {
        _ if rest.is_empty() => Vec::new(),
        ("INTO" | "USING", "DESCRIPTOR") => return Statement::Unsupported("FETCH ... INTO DESCRIPTOR".into()),
        ("INTO", _) => match host_list(&rest[1..], pos) {
            Ok(into) => into,
            Err(why) => return Statement::Malformed(why),
        },
        _ => return Statement::Malformed("FETCH NEXT ROWSET takes a cursor, FOR n ROWS and INTO".into()),
    };
    Statement::FetchRowset { cursor: cursor.into(), rows, into, enabled: false }
}

/// `FOR n ROWS` at `toks[0]`, and how many tokens it takes. A host variable for n takes no
/// indicator (Db2 13 SQL, FETCH and INSERT).
fn rows_clause(toks: &[Tok], pos: Pos) -> Result<(Rows, usize), String> {
    let (rows, next) = match toks.get(1) {
        Some(Tok::Number(n)) => (Rows::Constant(n.parse().map_err(|_| format!("FOR {n} ROWS: the number of rows is not an integer"))?), 2),
        Some(Tok::Host { .. }) => match host_var(toks, 1, pos) {
            Some((HostVar { indicator: Some(_), .. }, _)) => return Err("FOR n ROWS takes no indicator variable".into()),
            Some((var, next)) => (Rows::Host(Box::new(var)), next),
            None => unreachable!("a host token is a host variable"),
        },
        _ => return Err("FOR takes a number of rows or a host variable, then ROWS".into()),
    };
    match word(toks, next) {
        "ROWS" => Ok((rows, next + 1)),
        _ => Err("FOR n takes ROWS".into()),
    }
}

/// INSERT, or INSERT ... FOR n ROWS with ATOMIC or NOT ATOMIC CONTINUE ON SQLEXCEPTION, which
/// IBM writes after VALUES and also before it; without them it is the INSERT of one row.
fn insert(toks: &[Tok], pos: Pos) -> Statement {
    let (mut rows, mut atomic, mut kept) = (None, None, Vec::new());
    let (mut depth, mut i) = (0i32, 0);
    while i < toks.len() {
        match &toks[i] {
            Tok::Punct('(') => depth += 1,
            Tok::Punct(')') => depth -= 1,
            _ => {}
        }
        if depth == 0 && word(toks, i) == "FOR" && matches!(toks.get(i + 1), Some(Tok::Number(_) | Tok::Host { .. })) {
            match rows_clause(&toks[i..], pos) {
                Ok((n, taken)) => {
                    rows = Some(n);
                    i += taken;
                    continue;
                }
                Err(why) => return Statement::Malformed(why),
            }
        }
        if depth == 0 && word(toks, i) == "ATOMIC" {
            atomic = Some(true);
            i += 1;
            continue;
        }
        if depth == 0 && word(toks, i) == "NOT" && word(toks, i + 1) == "ATOMIC" {
            if (word(toks, i + 2), word(toks, i + 3), word(toks, i + 4)) != ("CONTINUE", "ON", "SQLEXCEPTION") {
                return Statement::Malformed("NOT ATOMIC takes CONTINUE ON SQLEXCEPTION".into());
            }
            atomic = Some(false);
            i += 5;
            continue;
        }
        kept.push(toks[i].clone());
        i += 1;
    }
    match rows {
        None if atomic.is_some() => Statement::Malformed("ATOMIC and NOT ATOMIC take FOR n ROWS".into()),
        None => change(ChangeKind::Insert, toks, pos),
        Some(rows) => {
            let (text, inputs) = render(&kept, pos);
            Statement::InsertRows { text, inputs, rows, atomic: atomic.unwrap_or(true) }
        }
    }
}

/// CALL of a procedure by its name, with its arguments in parentheses or none.
fn call(toks: &[Tok], pos: Pos) -> Statement {
    if matches!(toks.get(1), Some(Tok::Host { .. })) {
        return Statement::Unsupported("CALL of a procedure a host variable names".into());
    }
    let (mut name, mut i) = (Vec::new(), 1);
    loop {
        match toks.get(i) {
            Some(Tok::Word(part) | Tok::Quoted(part)) => name.push(part.clone()),
            _ => return Statement::Malformed("CALL names no procedure".into()),
        }
        i += 1;
        if toks.get(i) != Some(&Tok::Punct('.')) {
            break;
        }
        i += 1;
    }
    let closes_at_end = || {
        let mut depth = 0i32;
        for (k, t) in toks[i..].iter().enumerate() {
            depth += match t {
                Tok::Punct('(') => 1,
                Tok::Punct(')') => -1,
                _ => 0,
            };
            if depth == 0 {
                return i + k == toks.len() - 1;
            }
        }
        false
    };
    match (toks.get(i), word(toks, i), word(toks, i + 1)) {
        (None, _, _) => {}
        (_, "USING", "DESCRIPTOR") => return Statement::Unsupported("CALL ... USING DESCRIPTOR".into()),
        (Some(Tok::Punct('(')), _, _) if closes_at_end() => {}
        _ => return Statement::Malformed("CALL takes a procedure name and its arguments in parentheses".into()),
    }
    let (text, args) = render(toks, pos);
    Statement::Call { procedure: name.join("."), text, args }
}

fn whenever(toks: &[Tok]) -> Statement {
    let (condition, at) = match (word(toks, 1), word(toks, 2)) {
        ("SQLERROR", _) => (Condition::SqlError, 2),
        ("SQLWARNING", _) => (Condition::SqlWarning, 2),
        ("NOT", "FOUND") => (Condition::NotFound, 3),
        _ => return Statement::Malformed("WHENEVER takes SQLERROR, SQLWARNING or NOT FOUND".into()),
    };
    let label_at = match (word(toks, at), word(toks, at + 1)) {
        ("CONTINUE", _) if toks.len() == at + 1 => return Statement::Whenever { condition, action: Action::Continue },
        ("GO", "TO") => at + 2,
        ("GOTO", _) => at + 1,
        _ => return Statement::Malformed("WHENEVER takes CONTINUE or GO TO a label".into()),
    };
    let label: String = toks[label_at..]
        .iter()
        .map(|t| match t {
            Tok::Word(s) | Tok::Number(s) => s.clone(),
            Tok::Host { path, .. } => path.join("."),
            Tok::Punct(c) => c.to_string(),
            Tok::Quoted(s) => s.clone(),
        })
        .collect();
    if label.is_empty() {
        return Statement::Malformed("WHENEVER ... GO TO names no label".into());
    }
    Statement::Whenever { condition, action: Action::GoTo(label) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(sql: &str) -> Statement {
        parse(sql, Pos::default())
    }

    fn names(vars: &[HostVar]) -> Vec<(&str, Option<&str>)> {
        vars.iter().map(|h| (h.var.name.as_str(), h.indicator.as_ref().map(|r| r.name.as_str()))).collect()
    }

    #[test]
    fn select_into_splits_outputs_from_inputs_and_drops_into() {
        let Statement::Query { text, inputs, into } = st("SELECT NAME, PHONE INTO :WS-NAME, :WS-PHONE:WS-PHONE-IND FROM CUST WHERE ID = :WS-ID") else { panic!() };
        assert_eq!(text, "SELECT NAME, PHONE FROM CUST WHERE ID = ?");
        assert_eq!(names(&into), [("WS-NAME", None), ("WS-PHONE", Some("WS-PHONE-IND"))]);
        assert_eq!(names(&inputs), [("WS-ID", None)]);
    }

    #[test]
    fn indicator_keyword_and_spaced_indicator() {
        let Statement::Query { into, .. } = st("select a, b into :x indicator :xi, :y :yi from t") else { panic!() };
        assert_eq!(names(&into), [("X", Some("XI")), ("Y", Some("YI"))]);
    }

    #[test]
    fn qualified_and_subscripted_host_variables() {
        let Statement::Query { into, inputs, .. } = st("SELECT A INTO :CUST.NAME FROM T WHERE K = :TAB(3) AND J = :TAB(IDX)") else { panic!() };
        assert_eq!((into[0].var.name.as_str(), into[0].var.qualifiers.as_slice()), ("NAME", ["CUST".to_owned()].as_slice()));
        assert_eq!(inputs[0].var.subscripts, [Expr::Operand(Operand::Literal(Literal::Number("3".into())))]);
        assert!(matches!(&inputs[1].var.subscripts[0], Expr::Operand(Operand::Ref(r)) if r.name == "IDX"));
    }

    #[test]
    fn a_colon_inside_a_string_is_text() {
        let Statement::Query { text, inputs, .. } = st("SELECT A INTO :X FROM T WHERE B = ':NOT-A-HOST'") else { panic!() };
        assert_eq!(text, "SELECT A FROM T WHERE B = ':NOT-A-HOST'");
        assert!(inputs.is_empty());
    }

    #[test]
    fn changes_and_positioned_changes() {
        let Statement::Change { kind, text, inputs, current_of } = st("INSERT INTO T (A, B) VALUES (:A, :B:BI)") else { panic!() };
        assert_eq!((kind, text.as_str(), current_of), (ChangeKind::Insert, "INSERT INTO T (A, B) VALUES (?, ?)", None));
        assert_eq!(names(&inputs), [("A", None), ("B", Some("BI"))]);
        let Statement::Change { current_of, .. } = st("UPDATE EMP SET SAL = :NEW-SAL WHERE CURRENT OF C1") else { panic!() };
        assert_eq!(current_of.as_deref(), Some("C1"));
        assert!(matches!(st("DELETE FROM T WHERE K = :K"), Statement::Change { kind: ChangeKind::Delete, current_of: None, .. }));
    }

    #[test]
    fn cursors() {
        let Statement::DeclareCursor(Cursor { name, text, inputs, with_hold, statement, rowset }) = st("DECLARE C1 CURSOR WITH HOLD FOR SELECT NAME FROM EMP WHERE DEPT = :WS-DEPT FOR UPDATE OF SAL") else { panic!() };
        assert_eq!((name.as_str(), with_hold, statement, rowset), ("C1", true, None, false));
        assert_eq!(text, "SELECT NAME FROM EMP WHERE DEPT = ? FOR UPDATE OF SAL");
        assert_eq!(names(&inputs), [("WS-DEPT", None)]);
        assert_eq!(st("OPEN C1"), Statement::Open { cursor: "C1".into(), declared: None, using: Vec::new(), descriptor: None });
        assert_eq!(st("CLOSE C1"), Statement::Close { cursor: "C1".into() });
        let Statement::Fetch { cursor, into } = st("FETCH NEXT FROM C1 INTO :A, :B:BI") else { panic!() };
        assert_eq!((cursor.as_str(), names(&into).len()), ("C1", 2));
        assert_eq!(st("FETCH C1"), Statement::Fetch { cursor: "C1".into(), into: Vec::new() });
    }

    #[test]
    fn an_into_list_may_omit_colons() {
        let Statement::Fetch { into, .. } = st("FETCH LON-NHM-ENT-PROJ-WC INTO CSR-ENTITY, CSR-PROJ-ID") else { panic!() };
        assert_eq!(names(&into), [("CSR-ENTITY", None), ("CSR-PROJ-ID", None)]);
    }

    #[test]
    fn hyphenated_cursor_names() {
        assert_eq!(st("CLOSE PROGRAMS-CSR"), Statement::Close { cursor: "PROGRAMS-CSR".into() });
        assert!(matches!(st("FETCH FROM PROGRAMS-CSR INTO :A"), Statement::Fetch { cursor, .. } if cursor == "PROGRAMS-CSR"));
        assert!(matches!(st("DECLARE PROGRAMS-CSR CURSOR FOR SELECT A FROM T"), Statement::DeclareCursor(c) if c.name == "PROGRAMS-CSR"));
        assert!(matches!(st("DELETE FROM T WHERE CURRENT OF PROGRAMS-CSR"), Statement::Change { current_of: Some(c), .. } if c == "PROGRAMS-CSR"));
    }

    #[test]
    fn units_of_work_and_declarations() {
        assert_eq!(st("COMMIT"), Statement::Commit);
        assert_eq!(st("COMMIT WORK"), Statement::Commit);
        assert_eq!(st("ROLLBACK WORK"), Statement::Rollback);
        assert_eq!(st("ROLLBACK TO SAVEPOINT SP1"), Statement::Unsupported("ROLLBACK TO SAVEPOINT".into()));
        assert_eq!(st("INCLUDE SQLCA"), Statement::Declaration);
        assert_eq!(st("BEGIN DECLARE SECTION"), Statement::Declaration);
        assert_eq!(st("DECLARE EMP TABLE (ID CHAR(6) NOT NULL)"), Statement::Declaration);
    }

    #[test]
    fn whenever_actions() {
        assert_eq!(st("WHENEVER SQLERROR GO TO 9999-ERROR"), Statement::Whenever { condition: Condition::SqlError, action: Action::GoTo("9999-ERROR".into()) });
        assert_eq!(st("WHENEVER NOT FOUND CONTINUE"), Statement::Whenever { condition: Condition::NotFound, action: Action::Continue });
        assert_eq!(st("WHENEVER SQLWARNING GOTO :WARN-PARA"), Statement::Whenever { condition: Condition::SqlWarning, action: Action::GoTo("WARN-PARA".into()) });
    }

    #[test]
    fn set_and_values_assign_through_a_query() {
        let Statement::Query { text, into, .. } = st("SET :WS-TS = CURRENT TIMESTAMP") else { panic!() };
        assert_eq!((text.as_str(), into[0].var.name.as_str()), ("VALUES CURRENT TIMESTAMP", "WS-TS"));
        let Statement::Query { text, .. } = st("VALUES (:A + 1) INTO :B") else { panic!() };
        assert_eq!(text, "VALUES (? + 1)");
    }

    #[test]
    fn connect_and_set_connection_keep_the_host_variable_naming_the_location() {
        let target = |sql: &str| match st(sql) {
            Statement::Connect { what, target } => (what, target.map(|t| t.var.name)),
            other => panic!("{other:?}"),
        };
        assert_eq!(target("CONNECT TO :LOC USER :ID USING :PW"), ("CONNECT".into(), Some("LOC".into())));
        assert_eq!(target("SET CONNECTION :WS-LOC"), ("SET CONNECTION".into(), Some("WS-LOC".into())));
        assert_eq!(target("CONNECT RESET"), ("CONNECT".into(), None));
        assert_eq!(target("CONNECT USER :ID USING :PW"), ("CONNECT".into(), None));
        assert_eq!(st("SET CURRENT SQLID = 'X'"), Statement::Unsupported("SET".into()));
    }

    #[test]
    fn what_is_refused_and_why() {
        assert_eq!(st("CONNECT TO DB1"), Statement::Connect { what: "CONNECT".into(), target: None });
        assert!(matches!(st("DISCONNECT ALL"), Statement::Malformed(why) if why.starts_with("DISCONNECT is not a Db2 for z/OS statement")));
        assert_eq!(st("DESCRIBE INPUT S1 INTO :SQLDA"), Statement::Unsupported("DESCRIBE INPUT".into()));
        assert_eq!(st("FETCH PRIOR FROM C1 INTO :A"), Statement::Unsupported("a scrollable FETCH".into()));
        assert_eq!(st("DECLARE C2 SCROLL CURSOR FOR S1"), Statement::DeclareUnsupported { name: "C2".into(), what: "a scrollable cursor".into() });
        assert!(matches!(st("FETCH INTO :A"), Statement::Malformed(_)));
        assert!(matches!(st("SELECT A INTO FROM T"), Statement::Malformed(_)));
        assert!(matches!(st("SELECT A FROM T"), Statement::Malformed(_)));
        assert!(matches!(st("SELECT A INTO :X FROM T WHERE B = 'OPEN"), Statement::Malformed(_)));
    }

    #[test]
    fn two_character_operators_stay_whole() {
        let Statement::Query { text, .. } = st("SELECT A INTO :X FROM T WHERE B<=:Y AND C <> 1 AND D >= 2 AND E||F = 'A < = B'") else { panic!() };
        assert_eq!(text, "SELECT A FROM T WHERE B <= ? AND C <> 1 AND D >= 2 AND E || F = 'A < = B'");
    }

    #[test]
    fn the_fingerprint_ignores_spelling() {
        let (Statement::Query { text: a, .. }, Statement::Query { text: b, .. }) = (st("select a into :x from t\n   where k = :k"), st("SELECT A INTO :Y FROM T WHERE K = :J")) else { panic!() };
        assert_eq!(fingerprint(&a), fingerprint(&b));
        assert_eq!(fingerprint(""), 0x811c_9dc5);
    }

    #[test]
    fn the_parser_stamps_whenever_in_listing_order_and_numbers_blocks() {
        let program = crate::parse(concat!(
            "       IDENTIFICATION DIVISION.\n",
            "       PROGRAM-ID. T.\n",
            "       DATA DIVISION.\n",
            "       WORKING-STORAGE SECTION.\n",
            "       01 A PIC X(8).\n",
            "       PROCEDURE DIVISION.\n",
            "           PERFORM LATER.\n",
            "           EXEC SQL SELECT X INTO :A FROM T END-EXEC.\n",
            "           GOBACK.\n",
            "       LATER.\n",
            "           EXEC SQL WHENEVER NOT FOUND GO TO DONE END-EXEC.\n",
            "           EXEC SQL SELECT Y INTO :A FROM T END-EXEC.\n",
            "       DONE.\n",
            "           EXIT.\n",
        ))
        .expect("parses");
        let blocks: Vec<&Sql> = sql_blocks(&program);
        assert_eq!(blocks.iter().map(|s| s.ordinal).collect::<Vec<_>>(), [1, 2, 3]);
        assert_eq!(blocks[0].whenever.not_found, Action::Continue);
        assert_eq!(blocks[2].whenever.not_found, Action::GoTo("DONE".into()));
    }

    #[test]
    fn a_cursor_is_read_against_the_declares_before_it() {
        let mut cursors = Cursors::default();
        assert_eq!(cursors.resolve(st("FETCH C1 INTO :A")), Statement::Malformed("cursor C1 is not declared before this statement".into()));
        let Statement::DeclareCursor(c1) = cursors.resolve(st("DECLARE C1 CURSOR WITH HOLD FOR SELECT A FROM T WHERE K = :K")) else { panic!() };
        assert_eq!(cursors.resolve(st("OPEN C1")), Statement::Open { cursor: "C1".into(), declared: Some(c1), using: Vec::new(), descriptor: None });
        assert!(matches!(cursors.resolve(st("FETCH C1 INTO :A")), Statement::Fetch { .. }));
        assert!(matches!(cursors.resolve(st("DELETE FROM T WHERE CURRENT OF C1")), Statement::Change { .. }));
        assert!(matches!(cursors.resolve(st("UPDATE T SET A = 1 WHERE CURRENT OF C9")), Statement::Malformed(_)));
        let Statement::DeclareCursor(c2) = cursors.resolve(st("DECLARE C2 CURSOR FOR S1")) else { panic!() };
        let Statement::Open { declared, using, .. } = cursors.resolve(st("OPEN C2 USING :A, :B")) else { panic!() };
        assert_eq!((declared, names(&using)), (Some(c2), vec![("A", None), ("B", None)]));
        assert_eq!(cursors.resolve(st("OPEN C1 USING :A")), Statement::Unsupported("OPEN ... USING of a cursor declared for a select-statement".into()));
    }

    #[test]
    fn prepare_and_execute_immediate_take_the_statement_string_from_a_host_variable() {
        let Statement::Prepare { name, source, into } = st("PREPARE PRELT98_SQL FROM :SQLSEL-SQL") else { panic!() };
        assert_eq!((name.as_str(), source.var.name.as_str(), source.indicator, into), ("PRELT98_SQL", "SQLSEL-SQL", None, None));
        let Statement::ExecuteImmediate { source } = st("EXECUTE IMMEDIATE :WS-DYN-SQL") else { panic!() };
        assert_eq!(source.var.name, "WS-DYN-SQL");
        assert!(matches!(st("PREPARE S1 FROM 'SELECT 1'"), Statement::Malformed(why) if why.contains("PL/I")));
        assert!(matches!(st("EXECUTE IMMEDIATE :S :S-IND"), Statement::Malformed(why) if why.contains("no indicator")));
        assert!(matches!(st("PREPARE S1 FROM :A :B"), Statement::Malformed(_)));
        assert!(matches!(st("PREPARE FROM :A"), Statement::Malformed(_)));
        let Statement::Prepare { into: Some((descriptor, names)), .. } = st("PREPARE S1 INTO :SQLDA USING ANY FROM :A") else { panic!() };
        assert_eq!((descriptor.var.name.as_str(), names), ("SQLDA", Names::Any));
        assert_eq!(st("PREPARE S1 INTO :SQLDA USING BOTH FROM :A"), Statement::Unsupported("PREPARE ... INTO ... USING BOTH".into()));
        assert_eq!(st("PREPARE S1 ATTRIBUTES :ATTR FROM :A"), Statement::Unsupported("PREPARE ... ATTRIBUTES".into()));
    }

    #[test]
    fn execute_sends_its_using_list_to_the_parameter_markers() {
        assert_eq!(st("EXECUTE MMPREPSTMT"), Statement::Execute { name: "MMPREPSTMT".into(), inputs: Vec::new(), descriptor: None });
        let Statement::Execute { name, inputs, .. } = st("EXECUTE INS_STMT USING :EMP-NO, :EMP-NAME:EMP-NAME-IND") else { panic!() };
        assert_eq!((name.as_str(), names(&inputs)), ("INS_STMT", vec![("EMP-NO", None), ("EMP-NAME", Some("EMP-NAME-IND"))]));
        assert!(matches!(st("EXECUTE S1 USING DESCRIPTOR :SQLDA"), Statement::Execute { descriptor: Some(d), .. } if d.var.name == "SQLDA"));
        assert_eq!(st("EXECUTE S1 USING :ARR FOR 10 ROWS"), Statement::Unsupported("a multi-row EXECUTE".into()));
        assert!(matches!(st("EXECUTE S1 USING :A,"), Statement::Malformed(_)));
        assert!(matches!(st("EXECUTE S1 USING A"), Statement::Malformed(_)));
    }

    #[test]
    fn describe_and_fetch_using_descriptor_name_the_sqlda() {
        assert_eq!(st("DESCRIBE OUTPUT S1 INTO :SDSC"), Statement::Describe { name: "S1".into(), descriptor: HostVar { var: reference(&["SDSC".into()], &[], Pos::default()), indicator: None }, names: Names::Names });
        assert!(matches!(st("DESCRIBE S1 INTO :D USING LABELS"), Statement::Describe { names: Names::Labels, .. }));
        assert_eq!(st("DESCRIBE S1 INTO :D USING BOTH"), Statement::Unsupported("DESCRIBE ... USING BOTH".into()));
        assert_eq!(st("DESCRIBE CURSOR C1 INTO :D"), Statement::Unsupported("DESCRIBE CURSOR".into()));
        assert!(matches!(st("DESCRIBE S1 :D"), Statement::Malformed(_)));
        assert!(matches!(st("DESCRIBE S1 INTO :D :I"), Statement::Malformed(_)));
        let Statement::FetchDescriptor { cursor, descriptor } = st("FETCH DT USING DESCRIPTOR :SQLDA") else { panic!() };
        assert_eq!((cursor.as_str(), descriptor.var.name.as_str()), ("DT", "SQLDA"));
        let mut cursors = Cursors::default();
        cursors.resolve(st("DECLARE C1 CURSOR FOR SELECT A FROM T"));
        assert_eq!(cursors.resolve(st("OPEN C1 USING DESCRIPTOR :D")), Statement::Unsupported("OPEN ... USING of a cursor declared for a select-statement".into()));
        assert!(matches!(cursors.resolve(st("FETCH C9 USING DESCRIPTOR :D")), Statement::Malformed(_)));
    }

    #[test]
    fn a_cursor_for_a_prepared_statement_names_it() {
        let Statement::DeclareCursor(c) = st("DECLARE DT CURSOR WITH HOLD FOR DYN-STMT") else { panic!() };
        assert_eq!((c.name.as_str(), c.statement.as_deref(), c.with_hold, c.text.as_str()), ("DT", Some("DYN-STMT"), true, ""));
        assert!(matches!(st("DECLARE C1 CURSOR FOR S1 S2"), Statement::Malformed(_)));
        assert!(matches!(st("OPEN C1 USING DESCRIPTOR :SQLDA"), Statement::Open { descriptor: Some(d), .. } if d.var.name == "SQLDA"));
        assert!(matches!(st("OPEN C1 FOR"), Statement::Malformed(_)));
    }

    #[test]
    fn a_rowset_fetch_takes_next_rowset_its_rows_and_arrays_from_a_rowset_cursor() {
        let mut cursors = Cursors::default();
        cursors.resolve(st("DECLARE C1 CURSOR WITH HOLD WITH ROWSET POSITIONING FOR SELECT A, B FROM T"));
        cursors.resolve(st("DECLARE C2 CURSOR FOR SELECT A FROM T"));
        let Statement::FetchRowset { cursor, rows, into, enabled } = cursors.resolve(st("FETCH NEXT ROWSET FROM C1 FOR 5 ROWS INTO :COL1 :COL1IND, :COL2")) else { panic!() };
        assert_eq!((cursor.as_str(), rows, names(&into), enabled), ("C1", Rows::Constant(5), vec![("COL1", Some("COL1IND")), ("COL2", None)], true));
        let Statement::FetchRowset { rows: Rows::Host(n), enabled, .. } = cursors.resolve(st("FETCH NEXT ROWSET C2 FOR :N ROWS INTO :A")) else { panic!() };
        assert_eq!((n.var.name.as_str(), enabled), ("N", false));
        assert!(matches!(st("FETCH NEXT ROWSET FROM C1"), Statement::FetchRowset { rows: Rows::Implicit, into, .. } if into.is_empty()));
        assert!(matches!(st("FETCH NEXT ROWSET FROM C1 FOR :N :NI ROWS INTO :A"), Statement::Malformed(why) if why.contains("indicator")));
        assert!(matches!(st("FETCH NEXT FROM BINCSR FOR 50 ROWS INTO :WS-BIN-TABLE"), Statement::Malformed(why) if why.contains("NEXT ROWSET")));
        assert_eq!(st("FETCH NEXT ROWSET FROM C1 FOR 5 ROWS INTO DESCRIPTOR :D"), Statement::Unsupported("FETCH ... INTO DESCRIPTOR".into()));
        for scrolled in ["FETCH PRIOR ROWSET FROM C1 FOR 5 ROWS INTO :A", "FETCH ROWSET STARTING AT ABSOLUTE 3 FROM C1 FOR 2 ROWS INTO :A", "FETCH FIRST ROWSET FROM C1 INTO :A"] {
            assert_eq!(st(scrolled), Statement::Unsupported("a scrollable FETCH".into()), "{scrolled}");
        }
        assert!(matches!(Cursors::default().resolve(st("FETCH NEXT ROWSET FROM C9 INTO :A")), Statement::Malformed(_)));
    }

    #[test]
    fn a_multiple_row_insert_keeps_the_insert_of_one_row_and_its_atomicity() {
        let Statement::InsertRows { text, inputs, rows, atomic } = st("INSERT INTO DSN8D10.ACT (ACTNO, ACTKWD, ACTDESC) VALUES (:HVA1, :HVA2 :HVA2-IND, 'X') FOR :NUM-ROWS ROWS") else { panic!() };
        assert_eq!(text, "INSERT INTO DSN8D10.ACT (ACTNO, ACTKWD, ACTDESC) VALUES (?, ?, 'X')");
        assert_eq!((names(&inputs), atomic), (vec![("HVA1", None), ("HVA2", Some("HVA2-IND"))], true));
        assert!(matches!(rows, Rows::Host(h) if h.var.name == "NUM-ROWS" && h.indicator.is_none()));
        let Statement::InsertRows { text, rows, atomic, .. } = st("INSERT INTO T1 FOR 5 ROWS VALUES (:HVA) NOT ATOMIC CONTINUE ON SQLEXCEPTION") else { panic!() };
        assert_eq!((text.as_str(), rows, atomic), ("INSERT INTO T1 VALUES (?)", Rows::Constant(5), false));
        assert!(matches!(st("INSERT INTO T VALUES (:A) FOR 3 ROWS ATOMIC"), Statement::InsertRows { atomic: true, .. }));
        assert!(matches!(st("INSERT INTO T VALUES (:A) FOR 3 ROWS NOT ATOMIC"), Statement::Malformed(_)));
        assert!(matches!(st("INSERT INTO T VALUES (:A) ATOMIC"), Statement::Malformed(_)));
        assert!(matches!(st("INSERT INTO T VALUES (:A)"), Statement::Change { kind: ChangeKind::Insert, .. }));
        let row_of_rowset = Statement::Unsupported("a positioned UPDATE or DELETE FOR ROW n OF ROWSET".into());
        assert_eq!(st("UPDATE T SET A = 1 WHERE CURRENT OF C1 FOR ROW :N OF ROWSET"), row_of_rowset);
        assert_eq!(st("DELETE FROM T WHERE CURRENT OF C1 FOR ROW 5 OF ROWSET"), row_of_rowset);
    }

    #[test]
    fn call_names_its_procedure_and_sends_its_arguments() {
        let Statement::Call { procedure, text, args } = st("CALL PDAPROD.PDASP2 (:PDASP2-USERID, :PDASP2-STATUS :STATUS-IND, 'Y', NULL)") else { panic!() };
        assert_eq!((procedure.as_str(), text.as_str()), ("PDAPROD.PDASP2", "CALL PDAPROD.PDASP2 (?, ?, 'Y', NULL)"));
        assert_eq!(names(&args), [("PDASP2-USERID", None), ("PDASP2-STATUS", Some("STATUS-IND"))]);
        assert!(matches!(st("CALL PCTPROC"), Statement::Call { args, .. } if args.is_empty()));
        assert!(matches!(st("CALL P()"), Statement::Call { args, .. } if args.is_empty()));
        assert_eq!(st("CALL :PROC-NAME (:A)"), Statement::Unsupported("CALL of a procedure a host variable names".into()));
        assert_eq!(st("CALL P USING DESCRIPTOR :D"), Statement::Unsupported("CALL ... USING DESCRIPTOR".into()));
        for broken in ["CALL", "CALL P (:A", "CALL P (:A) (:B)", "CALL P :A"] {
            assert!(matches!(st(broken), Statement::Malformed(_)), "{broken}");
        }
    }

    fn sql_blocks(program: &crate::ast::Program) -> Vec<&Sql> {
        program
            .paragraphs
            .iter()
            .flat_map(|p| &p.statements)
            .filter_map(|s| match s {
                crate::ast::Stmt::Exec(block) => block.sql.as_ref(),
                _ => None,
            })
            .collect()
    }
}
