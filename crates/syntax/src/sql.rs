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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    Insert,
    Update,
    Delete,
}

/// A cursor as its DECLARE gives it: the query OPEN runs and the host variables OPEN sends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub name: String,
    pub text: String,
    pub inputs: Vec<HostVar>,
    pub with_hold: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Statement {
    /// SELECT ... INTO, VALUES ... INTO, and SET :host-variable = expression.
    Query { text: String, inputs: Vec<HostVar>, into: Vec<HostVar> },
    /// INSERT, UPDATE and DELETE; `current_of` names the cursor of a positioned UPDATE or DELETE.
    Change { kind: ChangeKind, text: String, inputs: Vec<HostVar>, current_of: Option<String> },
    DeclareCursor(Cursor),
    /// A cursor ironwork does not run, such as one for a prepared statement, and what it is.
    DeclareUnsupported { name: String, what: String },
    /// `declared` is the cursor's DECLARE, which [`Cursors::resolve`] fills.
    Open { cursor: String, declared: Option<Cursor> },
    Fetch { cursor: String, into: Vec<HostVar> },
    Close { cursor: String },
    Commit,
    Rollback,
    Whenever { condition: Condition, action: Action },
    /// INCLUDE, DECLARE SECTION, DECLARE TABLE and DECLARE STATEMENT, which declare and do nothing.
    Declaration,
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
            _ => Vec::new(),
        };
        vars.into_iter().flat_map(|h| std::iter::once(&h.var).chain(&h.indicator)).collect()
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
            Statement::Open { cursor, .. } | Statement::Fetch { cursor, .. } | Statement::Close { cursor, .. } | Statement::Change { current_of: Some(cursor), .. } => cursor.clone(),
            _ => return statement,
        };
        match (self.0.get(&named), statement) {
            (None, _) => Statement::Malformed(format!("cursor {named} is not declared before this statement")),
            (Some(Err(what)), _) => Statement::Unsupported(what.clone()),
            (Some(Ok(c)), Statement::Open { cursor, .. }) => Statement::Open { cursor, declared: Some(c.clone()) },
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
        "INSERT" => change(ChangeKind::Insert, toks, pos),
        "UPDATE" => change(ChangeKind::Update, toks, pos),
        "DELETE" => change(ChangeKind::Delete, toks, pos),
        "DECLARE" => declare(toks, pos),
        "OPEN" => match (word(toks, 1), toks.len()) {
            ("", _) => Statement::Malformed("OPEN names no cursor".into()),
            (cursor, 2) => Statement::Open { cursor: cursor.into(), declared: None },
            _ => Statement::Unsupported("OPEN with USING".into()),
        },
        "FETCH" => fetch(toks, pos),
        "CLOSE" => match (word(toks, 1), toks.len()) {
            ("", _) => Statement::Malformed("CLOSE names no cursor".into()),
            (cursor, 2) => Statement::Close { cursor: cursor.into() },
            _ => Statement::Malformed("CLOSE takes only a cursor name".into()),
        },
        "COMMIT" if toks.len() == 1 || (toks.len() == 2 && word(toks, 1) == "WORK") => Statement::Commit,
        "ROLLBACK" if toks.len() == 1 || (toks.len() == 2 && word(toks, 1) == "WORK") => Statement::Rollback,
        "ROLLBACK" => Statement::Unsupported("ROLLBACK TO SAVEPOINT".into()),
        "WHENEVER" => whenever(toks),
        "INCLUDE" => Statement::Declaration,
        "BEGIN" | "END" if word(toks, 1) == "DECLARE" => Statement::Declaration,
        "DISCONNECT" => Statement::Malformed("DISCONNECT is not a Db2 for z/OS statement; Db2 ends a connection with RELEASE and a commit".into()),
        "" => Statement::Malformed("the block holds no statement".into()),
        other => Statement::Unsupported(other.into()),
    }
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
    let query = &toks[for_at + 1..];
    if !matches!(word(query, 0), "SELECT" | "WITH" | "VALUES") && !matches!(query.first(), Some(Tok::Punct('('))) {
        return unsupported("a cursor for a prepared statement");
    }
    let (text, inputs) = render(query, pos);
    Statement::DeclareCursor(Cursor { name: name.into(), text, inputs, with_hold })
}

fn fetch(toks: &[Tok], pos: Pos) -> Statement {
    let mut i = 1;
    if word(toks, i) == "NEXT" {
        i += 1;
    }
    if matches!(word(toks, i), "PRIOR" | "FIRST" | "LAST" | "ABSOLUTE" | "RELATIVE" | "BEFORE" | "AFTER" | "CURRENT" | "SENSITIVE" | "INSENSITIVE") {
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
    match word(toks, i) {
        "" if i == toks.len() => Statement::Fetch { cursor: cursor.into(), into: Vec::new() },
        "INTO" => match host_list(&toks[i + 1..], pos) {
            Ok(into) => Statement::Fetch { cursor: cursor.into(), into },
            Err(why) => Statement::Malformed(why),
        },
        "FOR" => Statement::Unsupported("a multi-row FETCH".into()),
        "USING" => Statement::Unsupported("FETCH USING DESCRIPTOR".into()),
        _ => Statement::Malformed("FETCH takes a cursor and INTO".into()),
    }
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
        let Statement::DeclareCursor(Cursor { name, text, inputs, with_hold }) = st("DECLARE C1 CURSOR WITH HOLD FOR SELECT NAME FROM EMP WHERE DEPT = :WS-DEPT FOR UPDATE OF SAL") else { panic!() };
        assert_eq!((name.as_str(), with_hold), ("C1", true));
        assert_eq!(text, "SELECT NAME FROM EMP WHERE DEPT = ? FOR UPDATE OF SAL");
        assert_eq!(names(&inputs), [("WS-DEPT", None)]);
        assert_eq!(st("OPEN C1"), Statement::Open { cursor: "C1".into(), declared: None });
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
    fn what_is_refused_and_why() {
        assert_eq!(st("CONNECT TO DB1"), Statement::Unsupported("CONNECT".into()));
        assert!(matches!(st("DISCONNECT ALL"), Statement::Malformed(why) if why.starts_with("DISCONNECT is not a Db2 for z/OS statement")));
        assert_eq!(st("PREPARE S1 FROM :STMT"), Statement::Unsupported("PREPARE".into()));
        assert_eq!(st("FETCH PRIOR FROM C1 INTO :A"), Statement::Unsupported("a scrollable FETCH".into()));
        let prepared = Statement::DeclareUnsupported { name: "C2".into(), what: "a cursor for a prepared statement".into() };
        assert_eq!(st("DECLARE C2 CURSOR FOR S1"), prepared);
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
        assert_eq!(cursors.resolve(st("OPEN C1")), Statement::Open { cursor: "C1".into(), declared: Some(c1) });
        assert!(matches!(cursors.resolve(st("FETCH C1 INTO :A")), Statement::Fetch { .. }));
        assert!(matches!(cursors.resolve(st("DELETE FROM T WHERE CURRENT OF C1")), Statement::Change { .. }));
        assert!(matches!(cursors.resolve(st("UPDATE T SET A = 1 WHERE CURRENT OF C9")), Statement::Malformed(_)));
        cursors.resolve(st("DECLARE C2 CURSOR FOR S1"));
        assert_eq!(cursors.resolve(st("OPEN C2")), Statement::Unsupported("a cursor for a prepared statement".into()));
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
