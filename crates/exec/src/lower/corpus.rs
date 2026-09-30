//! The COBOL programs of exec's test suite, read from the test sources. The tests build their
//! programs with Rust (string literals, `line`, `format!`, `concat!`, helpers, closures, loops), so
//! this evaluates that subset of Rust over each `#[test]` function and records every string that
//! reaches a call or an array and holds a program. What it cannot evaluate is opaque and records
//! nothing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// A program's source text, and the test that first built it.
pub(super) struct Source {
    pub text: String,
    pub origin: String,
}

pub(super) struct Corpus {
    pub sources: Vec<Source>,
    /// Every `#[test]` function read, and those whose programs could not be evaluated.
    pub tests: usize,
    pub silent: Vec<String>,
}

/// Every test source under `src`, apart from `skip`, with testing.rs's helpers visible to all.
pub(super) fn read(src: &Path, skip: &Path) -> Corpus {
    let mut paths = Vec::new();
    walk(src, skip, &mut paths);
    paths.sort();
    let mut files: Vec<File> = Vec::new();
    for path in &paths {
        let Ok(text) = std::fs::read_to_string(path) else { continue };
        let is_helpers = path.ends_with("testing.rs");
        if !is_helpers && !text.contains("#[test]") {
            continue;
        }
        let name = path.strip_prefix(src).unwrap_or(path).display().to_string();
        let mut file = File { name, fns: HashMap::new(), consts: HashMap::new(), tests: Vec::new(), parent: None, children: HashMap::new(), helpers: is_helpers };
        let toks = lex(&text);
        let mut p = Parser { toks: &toks, at: 0 };
        p.items(&mut file, toks.len());
        files.push(file);
    }
    let names: Vec<String> = files.iter().map(|f| f.name.replace('\\', "/")).collect();
    for (i, name) in names.iter().enumerate() {
        if let Some(rest) = name.strip_suffix(".rs")
            && let Some((dir, stem)) = rest.rsplit_once('/')
            && let Some(parent) = names.iter().position(|n| *n == format!("{dir}.rs"))
        {
            files[i].parent = Some(parent);
            files[parent].children.insert(stem.to_owned(), i);
        }
    }
    let helpers = files.iter().position(|f| f.helpers);
    let mut corpus = Corpus { sources: Vec::new(), tests: 0, silent: Vec::new() };
    let mut seen = std::collections::HashSet::new();
    for (i, file) in files.iter().enumerate() {
        // Tests first; then helpers that take nothing, which a test may hand to a thread.
        let mut helpers_alone: Vec<&Rc<FnDef>> = file.fns.values().filter(|f| f.params.is_empty() && !file.tests.iter().any(|t| Rc::ptr_eq(t, f))).collect();
        helpers_alone.sort_by(|a, b| a.name.cmp(&b.name));
        let entries = file.tests.iter().map(|t| (t, true)).chain(helpers_alone.into_iter().map(|f| (f, false)));
        for (test, is_test) in entries {
            let mut e = Eval { files: &files, helpers, scope: i, env: vec![HashMap::new()], found: Vec::new(), steps: 0, depth: 0, returning: None };
            e.eval(&test.body);
            let origin = format!("{}::{}", file.name, test.name);
            if is_test {
                corpus.tests += 1;
                if e.found.is_empty() {
                    corpus.silent.push(origin.clone());
                }
            }
            for text in e.found {
                if seen.insert(text.clone()) {
                    corpus.sources.push(Source { text, origin: origin.clone() });
                }
            }
        }
    }
    corpus
}

fn walk(dir: &Path, skip: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.starts_with(skip) {
            continue;
        }
        if path.is_dir() {
            walk(&path, skip, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn is_program(text: &str) -> bool {
    (text.contains("PROGRAM-ID") || text.contains("CLASS-ID")) && text.contains("PROCEDURE DIVISION")
}

// The lexer.

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Ident(String),
    Str(String),
    Char(char),
    Int(i128),
    Float,
    Life,
    P(&'static str),
}

const PUNCTS: [&str; 45] = [
    "..=", "...", "::", "->", "=>", "==", "!=", "<=", ">=", "&&", "||", "+=", "-=", "*=", "/=", "%=", "^=", "|=", "&=", "..", "+", "-", "*", "/", "%", "=", "<",
    ">", "!", "&", "|", "^", "~", "@", ".", ",", ";", ":", "#", "$", "?", "(", ")", "[", "]",
];

fn lex(src: &str) -> Vec<Tok> {
    let s: Vec<char> = src.chars().collect();
    let at = |i: usize| s.get(i).copied().unwrap_or('\0');
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '/' && at(i + 1) == '/' {
            while i < s.len() && s[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && at(i + 1) == '*' {
            let mut depth = 0;
            while i < s.len() {
                if s[i] == '/' && at(i + 1) == '*' {
                    depth += 1;
                    i += 2;
                } else if s[i] == '*' && at(i + 1) == '/' {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
        } else if c == '"' {
            let (text, next) = string(&s, i + 1);
            out.push(Tok::Str(text));
            i = next;
        } else if (c == 'r' && matches!(at(i + 1), '"' | '#')) || (c == 'b' && at(i + 1) == 'r' && matches!(at(i + 2), '"' | '#')) {
            let mut j = if c == 'b' { i + 2 } else { i + 1 };
            let mut hashes = 0;
            while at(j) == '#' {
                hashes += 1;
                j += 1;
            }
            if at(j) != '"' {
                out.push(Tok::Ident(c.to_string()));
                i += 1;
                continue;
            }
            let start = j + 1;
            let mut k = start;
            while k < s.len() && !(s[k] == '"' && (0..hashes).all(|h| at(k + 1 + h) == '#')) {
                k += 1;
            }
            out.push(Tok::Str(s[start..k.min(s.len())].iter().collect()));
            i = k + 1 + hashes;
        } else if c == 'b' && at(i + 1) == '"' {
            let (text, next) = string(&s, i + 2);
            out.push(Tok::Str(text));
            i = next;
        } else if c == 'b' && at(i + 1) == '\'' {
            let (ch, next) = char_lit(&s, i + 2);
            out.push(Tok::Int(ch as i128));
            i = next;
        } else if c == '\'' {
            if at(i + 1) == '\\' || at(i + 2) == '\'' {
                let (ch, next) = char_lit(&s, i + 1);
                out.push(Tok::Char(ch));
                i = next;
            } else {
                i += 1;
                while i < s.len() && (s[i].is_alphanumeric() || s[i] == '_') {
                    i += 1;
                }
                out.push(Tok::Life);
            }
        } else if c.is_ascii_digit() {
            let start = i;
            let hex = c == '0' && matches!(at(i + 1), 'x' | 'X');
            if hex {
                i += 2;
            }
            while i < s.len() && (s[i].is_ascii_hexdigit() && hex || s[i].is_ascii_digit() || s[i] == '_') {
                i += 1;
            }
            let digits: String = s[if hex { start + 2 } else { start }..i].iter().filter(|&&d| d != '_').collect();
            let mut float = false;
            if !hex && at(i) == '.' && at(i + 1).is_ascii_digit() {
                float = true;
                i += 1;
                while i < s.len() && (s[i].is_ascii_digit() || s[i] == '_') {
                    i += 1;
                }
            }
            while i < s.len() && (s[i].is_ascii_alphanumeric() || s[i] == '_') {
                i += 1;
            }
            let value = if hex { i128::from_str_radix(&digits, 16).ok() } else { digits.parse().ok() };
            out.push(match (float, value) {
                (false, Some(v)) => Tok::Int(v),
                _ => Tok::Float,
            });
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < s.len() && (s[i].is_alphanumeric() || s[i] == '_') {
                i += 1;
            }
            out.push(Tok::Ident(s[start..i].iter().collect()));
        } else if let Some(p) = PUNCTS.iter().chain(&["{", "}"]).find(|p| p.chars().enumerate().all(|(k, pc)| at(i + k) == pc)) {
            out.push(Tok::P(p));
            i += p.len();
        } else {
            i += 1;
        }
    }
    out
}

fn escape(s: &[char], i: usize) -> (Option<char>, usize) {
    let at = |k: usize| s.get(k).copied().unwrap_or('\0');
    match at(i) {
        'n' => (Some('\n'), i + 1),
        'r' => (Some('\r'), i + 1),
        't' => (Some('\t'), i + 1),
        '0' => (Some('\0'), i + 1),
        'x' => {
            let hex: String = [at(i + 1), at(i + 2)].iter().collect();
            (u8::from_str_radix(&hex, 16).ok().map(char::from), i + 3)
        }
        'u' => {
            let end = (i..s.len()).find(|&k| s[k] == '}').unwrap_or(s.len());
            let hex: String = s[(i + 2).min(end)..end].iter().collect();
            (u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32), end + 1)
        }
        '\n' => {
            let mut k = i + 1;
            while k < s.len() && s[k].is_whitespace() {
                k += 1;
            }
            (None, k)
        }
        other => (Some(other), i + 1),
    }
}

fn string(s: &[char], mut i: usize) -> (String, usize) {
    let mut out = String::new();
    while i < s.len() && s[i] != '"' {
        if s[i] == '\\' {
            let (c, next) = escape(s, i + 1);
            out.extend(c);
            i = next;
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
    (out, i + 1)
}

fn char_lit(s: &[char], i: usize) -> (char, usize) {
    let (c, next) = if s.get(i) == Some(&'\\') { escape(s, i + 1) } else { (s.get(i).copied(), i + 1) };
    (c.unwrap_or('\0'), next + 1)
}

// The parser: items, and the expressions and statements of function bodies.

#[derive(Debug)]
enum E {
    Lit(V),
    Path(Vec<String>),
    Macro(String, Vec<E>),
    Call(Box<E>, Vec<E>),
    Method(Box<E>, String, String, Vec<E>),
    Field(Box<E>, String),
    Index(Box<E>, Box<E>),
    Array(Vec<E>),
    Repeat(Box<E>, Box<E>),
    Tuple(Vec<E>),
    Unary(&'static str, Box<E>),
    Bin(&'static str, Box<E>, Box<E>),
    Assign(&'static str, Box<E>, Box<E>),
    Range(Box<E>, Box<E>, bool),
    Closure(Rc<Lambda>),
    Block(Vec<S>),
    If(Box<E>, Box<E>, Option<Box<E>>),
    For(Pat, Box<E>, Box<E>),
    Scrutinee(Box<E>),
    Return(Option<Box<E>>),
    Opaque,
}

#[derive(Debug)]
enum S {
    Let(Pat, String, Option<E>),
    Expr(E),
    Tail(E),
}

#[derive(Clone, Debug)]
enum Pat {
    Name(String),
    Tuple(Vec<Pat>),
    Other,
}

#[derive(Debug)]
struct Lambda {
    params: Vec<Pat>,
    body: E,
}

struct FnDef {
    name: String,
    params: Vec<Pat>,
    ret: String,
    body: E,
}

struct File {
    name: String,
    fns: HashMap<String, Rc<FnDef>>,
    consts: HashMap<String, Rc<E>>,
    tests: Vec<Rc<FnDef>>,
    parent: Option<usize>,
    children: HashMap<String, usize>,
    helpers: bool,
}

struct Parser<'t> {
    toks: &'t [Tok],
    at: usize,
}

const BINARY: [(&str, u8); 18] = [
    ("||", 3),
    ("&&", 4),
    ("==", 5),
    ("!=", 5),
    ("<", 5),
    (">", 5),
    ("<=", 5),
    (">=", 5),
    ("|", 6),
    ("^", 7),
    ("&", 8),
    ("+", 9),
    ("-", 9),
    ("*", 10),
    ("/", 10),
    ("%", 10),
    ("..", 2),
    ("..=", 2),
];

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at)
    }

    fn peek_at(&self, k: usize) -> Option<&Tok> {
        self.toks.get(self.at + k)
    }

    fn is(&self, p: &str) -> bool {
        matches!(self.peek(), Some(Tok::P(q)) if *q == p)
    }

    fn is_word(&self, w: &str) -> bool {
        matches!(self.peek(), Some(Tok::Ident(i)) if i == w)
    }

    fn eat(&mut self, p: &str) -> bool {
        let hit = self.is(p);
        if hit {
            self.at += 1;
        }
        hit
    }

    fn eat_word(&mut self, w: &str) -> bool {
        let hit = self.is_word(w);
        if hit {
            self.at += 1;
        }
        hit
    }

    fn ident(&mut self) -> Option<String> {
        match self.peek() {
            Some(Tok::Ident(i)) => {
                let i = i.clone();
                self.at += 1;
                Some(i)
            }
            _ => None,
        }
    }

    /// Past the group that opens here, `(`, `[`, `{` or `<`, to just after its close.
    fn skip_group(&mut self) {
        let (open, close) = match self.peek() {
            Some(Tok::P("(")) => ("(", ")"),
            Some(Tok::P("[")) => ("[", "]"),
            Some(Tok::P("{")) => ("{", "}"),
            Some(Tok::P("<")) => ("<", ">"),
            _ => {
                self.at += 1;
                return;
            }
        };
        let mut depth = 0;
        while let Some(t) = self.peek() {
            match t {
                Tok::P(p) if *p == open => depth += 1,
                Tok::P(p) if *p == close => depth -= 1,
                Tok::P("->") if open == "<" => {}
                _ => {}
            }
            self.at += 1;
            if depth == 0 {
                return;
            }
        }
    }

    /// Past a type, returning its tokens as text.
    fn skip_type(&mut self) -> String {
        let start = self.at;
        loop {
            match self.peek() {
                Some(Tok::P("&" | "&&" | "*")) | Some(Tok::Life) => self.at += 1,
                Some(Tok::Ident(w)) if matches!(w.as_str(), "mut" | "dyn" | "impl" | "const") => self.at += 1,
                _ => break,
            }
        }
        match self.peek() {
            Some(Tok::P("(" | "[")) => self.skip_group(),
            Some(Tok::Ident(w)) if w == "fn" => {
                self.at += 1;
                self.skip_group();
                if self.eat("->") {
                    self.skip_type();
                }
            }
            Some(Tok::Ident(_)) => loop {
                self.at += 1;
                if self.is("<") {
                    self.skip_group();
                }
                if self.is("::") && matches!(self.peek_at(1), Some(Tok::Ident(_))) {
                    self.at += 1;
                    continue;
                }
                if self.is("+") && matches!(self.peek_at(1), Some(Tok::Ident(_)) | Some(Tok::Life)) {
                    self.at += 1;
                    if matches!(self.peek(), Some(Tok::Life)) {
                        self.at += 1;
                        break;
                    }
                    continue;
                }
                break;
            },
            _ => {}
        }
        self.toks[start..self.at].iter().map(|t| format!("{t:?}")).collect()
    }

    fn items(&mut self, file: &mut File, end: usize) {
        let mut test = false;
        while self.at < end {
            if self.eat("#") {
                self.eat("!");
                if self.is("[") {
                    let start = self.at;
                    self.skip_group();
                    test |= self.toks[start..self.at] == [Tok::P("["), Tok::Ident("test".into()), Tok::P("]")];
                }
                continue;
            }
            if self.eat_word("pub") {
                if self.is("(") {
                    self.skip_group();
                }
                continue;
            }
            match self.peek() {
                Some(Tok::Ident(w)) if w == "fn" => {
                    if let Some(f) = self.function() {
                        let f = Rc::new(f);
                        if test {
                            file.tests.push(f.clone());
                        }
                        file.fns.insert(f.name.clone(), f);
                    }
                    test = false;
                }
                Some(Tok::Ident(w)) if w == "const" || w == "static" => {
                    self.at += 1;
                    self.eat_word("mut");
                    let Some(name) = self.ident() else { continue };
                    if self.eat(":") {
                        self.skip_type();
                    }
                    if self.eat("=") {
                        let e = self.expr(false).unwrap_or(E::Opaque);
                        file.consts.insert(name, Rc::new(e));
                    }
                    self.skip_to(";");
                    test = false;
                }
                Some(Tok::Ident(w)) if w == "mod" => {
                    self.at += 2;
                    if self.is("{") {
                        let close = self.matching(self.at);
                        self.at += 1;
                        self.items(file, close);
                        self.at = close + 1;
                    } else {
                        self.eat(";");
                    }
                    test = false;
                }
                Some(Tok::Ident(_)) => {
                    self.skip_item();
                    test = false;
                }
                _ => self.at += 1,
            }
        }
    }

    /// The index of the token that closes the group opening at `open`.
    fn matching(&self, open: usize) -> usize {
        let mut p = Parser { toks: self.toks, at: open };
        p.skip_group();
        p.at.saturating_sub(1)
    }

    fn skip_to(&mut self, stop: &str) {
        while let Some(t) = self.peek() {
            match t {
                Tok::P(p) if *p == stop => {
                    self.at += 1;
                    return;
                }
                Tok::P("(" | "[" | "{") => self.skip_group(),
                _ => self.at += 1,
            }
        }
    }

    /// Past a `use`, `impl`, `struct`, `enum`, `trait`, `type` or macro item.
    fn skip_item(&mut self) {
        while let Some(t) = self.peek() {
            match t {
                Tok::P(";") => {
                    self.at += 1;
                    return;
                }
                Tok::P("{") => {
                    self.skip_group();
                    return;
                }
                Tok::P("(" | "[") => self.skip_group(),
                _ => self.at += 1,
            }
        }
    }

    fn function(&mut self) -> Option<FnDef> {
        self.at += 1;
        let name = self.ident()?;
        if self.is("<") {
            self.skip_group();
        }
        if !self.is("(") {
            return None;
        }
        let close = self.matching(self.at);
        self.at += 1;
        let mut params = Vec::new();
        while self.at < close {
            let pat = self.pattern();
            if self.eat(":") {
                self.skip_type();
            }
            params.push(pat);
            if !self.eat(",") && self.at < close {
                self.at = close;
            }
        }
        self.at = close + 1;
        let ret = if self.eat("->") { self.skip_type() } else { String::new() };
        while self.peek().is_some() && !self.is("{") && !self.is(";") {
            self.at += 1;
        }
        if !self.is("{") {
            self.eat(";");
            return None;
        }
        let body = self.block();
        Some(FnDef { name, params, ret, body })
    }

    fn pattern(&mut self) -> Pat {
        while self.eat("&") || self.eat("&&") || self.eat_word("mut") || self.eat_word("ref") {}
        match self.peek() {
            Some(Tok::P("(")) => {
                self.at += 1;
                let mut parts = Vec::new();
                while !self.is(")") && self.peek().is_some() {
                    parts.push(self.pattern());
                    if !self.eat(",") {
                        break;
                    }
                }
                self.eat(")");
                Pat::Tuple(parts)
            }
            Some(Tok::Ident(w)) if w == "_" => {
                self.at += 1;
                Pat::Other
            }
            Some(Tok::Ident(w)) if w.chars().next().is_some_and(|c| c.is_lowercase() || c == '_') && !matches!(self.peek_at(1), Some(Tok::P("(" | "::" | "{"))) => {
                Pat::Name(self.ident().unwrap_or_default())
            }
            _ => {
                while let Some(t) = self.peek() {
                    match t {
                        Tok::P("," | ")" | ":" | "=" | "|" | "]") => break,
                        Tok::Ident(w) if w == "in" => break,
                        Tok::P("(" | "[" | "{") => self.skip_group(),
                        _ => self.at += 1,
                    }
                }
                Pat::Other
            }
        }
    }

    fn block(&mut self) -> E {
        if !self.is("{") {
            return E::Opaque;
        }
        let close = self.matching(self.at);
        self.at += 1;
        let mut stmts = Vec::new();
        while self.at < close {
            if self.eat(";") {
                continue;
            }
            if self.eat("#") {
                self.skip_group();
                continue;
            }
            if self.is_word("let") {
                self.at += 1;
                let pat = self.pattern();
                let ty = if self.eat(":") { self.skip_type() } else { String::new() };
                let init = if self.eat("=") { self.expr(false) } else { None };
                if self.is_word("else") {
                    self.at += 1;
                    self.skip_group();
                }
                if !self.eat(";") {
                    self.recover(close);
                }
                stmts.push(S::Let(pat, ty, Some(init.unwrap_or(E::Opaque))));
                continue;
            }
            if matches!(self.peek(), Some(Tok::Ident(w)) if matches!(w.as_str(), "fn" | "use" | "struct" | "enum" | "impl" | "const" | "static" | "type" | "trait" | "mod")) {
                self.skip_item();
                continue;
            }
            let start = self.at;
            match self.expr(false) {
                Some(e) if self.at <= close => {
                    if self.eat(";") {
                        stmts.push(S::Expr(e));
                    } else if self.at == close {
                        stmts.push(S::Tail(e));
                    } else if matches!(e, E::Block(_) | E::If(..) | E::For(..) | E::Scrutinee(_) | E::Opaque) {
                        stmts.push(S::Expr(e));
                    } else {
                        self.recover(close);
                    }
                }
                _ => {
                    self.at = start;
                    self.recover(close);
                }
            }
        }
        self.at = close + 1;
        E::Block(stmts)
    }

    /// Past the rest of a statement that did not parse.
    fn recover(&mut self, close: usize) {
        while self.at < close {
            match self.peek() {
                Some(Tok::P(";")) => {
                    self.at += 1;
                    return;
                }
                Some(Tok::P("(" | "[" | "{")) => self.skip_group(),
                _ => self.at += 1,
            }
        }
    }

    fn expr(&mut self, no_struct: bool) -> Option<E> {
        let lhs = self.binary(2, no_struct)?;
        for op in ["=", "+=", "-="] {
            if self.eat(op) {
                let rhs = self.expr(no_struct)?;
                return Some(E::Assign(op, Box::new(lhs), Box::new(rhs)));
            }
        }
        for op in ["*=", "/=", "%=", "^=", "|=", "&="] {
            if self.eat(op) {
                self.expr(no_struct)?;
                return Some(E::Opaque);
            }
        }
        Some(lhs)
    }

    fn binary(&mut self, min: u8, no_struct: bool) -> Option<E> {
        let mut lhs = self.unary(no_struct)?;
        while let Some(Tok::P(p)) = self.peek() {
            let Some(&(op, prec)) = BINARY.iter().find(|(o, _)| o == p) else { break };
            if prec < min {
                break;
            }
            self.at += 1;
            if prec == 2 {
                let rhs = self.binary(3, no_struct)?;
                lhs = E::Range(Box::new(lhs), Box::new(rhs), op == "..=");
                continue;
            }
            let rhs = self.binary(prec + 1, no_struct)?;
            lhs = E::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Some(lhs)
    }

    fn unary(&mut self, no_struct: bool) -> Option<E> {
        for op in ["&", "&&", "*", "!", "-"] {
            if self.eat(op) {
                self.eat_word("mut");
                let inner = self.unary(no_struct)?;
                return Some(E::Unary(if op == "&&" { "&" } else { op }, Box::new(inner)));
            }
        }
        let mut e = self.primary(no_struct)?;
        loop {
            if self.eat("?") {
                continue;
            }
            if self.is_word("as") {
                self.at += 1;
                self.skip_type();
                continue;
            }
            if self.is("(") {
                e = E::Call(Box::new(e), self.args("(", ")")?);
                continue;
            }
            if self.is("[") {
                self.at += 1;
                let index = self.expr(false)?;
                if !self.eat("]") {
                    return None;
                }
                e = E::Index(Box::new(e), Box::new(index));
                continue;
            }
            if self.eat(".") {
                match self.peek().cloned() {
                    Some(Tok::Ident(name)) => {
                        self.at += 1;
                        let mut turbofish = String::new();
                        if self.is("::") && matches!(self.peek_at(1), Some(Tok::P("<"))) {
                            self.at += 1;
                            turbofish = self.skip_type_group();
                        }
                        if self.is("(") {
                            e = E::Method(Box::new(e), name, turbofish, self.args("(", ")")?);
                        } else {
                            e = E::Field(Box::new(e), name);
                        }
                    }
                    Some(Tok::Int(n)) => {
                        self.at += 1;
                        e = E::Field(Box::new(e), n.to_string());
                    }
                    Some(Tok::Float) => {
                        self.at += 1;
                        e = E::Opaque;
                    }
                    _ => return None,
                }
                continue;
            }
            break;
        }
        Some(e)
    }

    fn skip_type_group(&mut self) -> String {
        let start = self.at;
        self.skip_group();
        self.toks[start..self.at].iter().map(|t| format!("{t:?}")).collect()
    }

    /// Comma-separated expressions up to `close`; one that does not parse is opaque.
    fn args(&mut self, open: &str, close: &str) -> Option<Vec<E>> {
        if !self.eat(open) {
            return None;
        }
        let end = self.matching(self.at - 1);
        let mut out = Vec::new();
        while self.at < end {
            let start = self.at;
            match self.expr(false) {
                Some(e) if self.at <= end && (self.is(",") || self.at == end) => out.push(e),
                _ => {
                    self.at = start;
                    while self.at < end && !self.is(",") {
                        if matches!(self.peek(), Some(Tok::P("(" | "[" | "{"))) {
                            self.skip_group();
                        } else {
                            self.at += 1;
                        }
                    }
                    out.push(E::Opaque);
                }
            }
            if !self.eat(",") {
                self.at = end;
            }
        }
        self.at = end;
        self.eat(close);
        Some(out)
    }

    fn primary(&mut self, no_struct: bool) -> Option<E> {
        let tok = self.peek()?.clone();
        match tok {
            Tok::Str(s) => {
                self.at += 1;
                Some(E::Lit(V::Str(s)))
            }
            Tok::Int(n) => {
                self.at += 1;
                Some(E::Lit(V::Int(n)))
            }
            Tok::Char(c) => {
                self.at += 1;
                Some(E::Lit(V::Char(c)))
            }
            Tok::Float => {
                self.at += 1;
                Some(E::Opaque)
            }
            Tok::Life => {
                self.at += 1;
                self.eat(":");
                self.primary(no_struct)
            }
            Tok::P("(") => {
                let mut parts = self.args("(", ")")?;
                Some(if parts.len() == 1 { parts.remove(0) } else { E::Tuple(parts) })
            }
            Tok::P("[") => {
                let close = self.matching(self.at);
                self.at += 1;
                if self.at == close {
                    self.at += 1;
                    return Some(E::Array(Vec::new()));
                }
                let first = self.expr(false)?;
                if self.eat(";") {
                    let n = self.expr(false)?;
                    self.at = close + 1;
                    return Some(E::Repeat(Box::new(first), Box::new(n)));
                }
                let mut items = vec![first];
                while self.eat(",") && self.at < close {
                    let start = self.at;
                    match self.expr(false) {
                        Some(e) => items.push(e),
                        None => {
                            self.at = start;
                            break;
                        }
                    }
                }
                self.at = close + 1;
                Some(E::Array(items))
            }
            Tok::P("{") => Some(self.block()),
            Tok::P("|") | Tok::P("||") => {
                let mut params = Vec::new();
                if self.eat("|") {
                    while !self.is("|") && self.peek().is_some() {
                        params.push(self.pattern());
                        if self.eat(":") {
                            self.skip_type();
                        }
                        if !self.eat(",") {
                            break;
                        }
                    }
                    if !self.eat("|") {
                        return None;
                    }
                } else {
                    self.at += 1;
                }
                let body = if self.eat("->") {
                    self.skip_type();
                    self.block()
                } else {
                    self.expr(no_struct)?
                };
                Some(E::Closure(Rc::new(Lambda { params, body })))
            }
            Tok::P("..") => {
                self.at += 1;
                Some(E::Opaque)
            }
            Tok::P("<") => {
                self.skip_group();
                while self.eat("::") {
                    self.ident();
                }
                Some(E::Opaque)
            }
            Tok::Ident(w) => self.word(&w, no_struct),
            _ => None,
        }
    }

    fn word(&mut self, w: &str, no_struct: bool) -> Option<E> {
        match w {
            "true" | "false" => {
                self.at += 1;
                Some(E::Lit(V::Bool(w == "true")))
            }
            "move" => {
                self.at += 1;
                self.primary(no_struct)
            }
            "unsafe" => {
                self.at += 1;
                Some(self.block())
            }
            "if" => {
                self.at += 1;
                if self.eat_word("let") {
                    self.pattern();
                    self.eat("=");
                    let scrutinee = self.expr(true)?;
                    self.block();
                    if self.eat_word("else") {
                        if self.is_word("if") { self.primary(no_struct)?; } else { self.block(); }
                    }
                    return Some(E::Scrutinee(Box::new(scrutinee)));
                }
                let cond = self.expr(true)?;
                let then = self.block();
                let otherwise = if self.eat_word("else") { Some(Box::new(if self.is_word("if") { self.primary(no_struct)? } else { self.block() })) } else { None };
                Some(E::If(Box::new(cond), Box::new(then), otherwise))
            }
            "for" => {
                self.at += 1;
                let pat = self.pattern();
                if !self.eat_word("in") {
                    return None;
                }
                let iter = self.expr(true)?;
                let body = self.block();
                Some(E::For(pat, Box::new(iter), Box::new(body)))
            }
            "match" => {
                self.at += 1;
                let scrutinee = self.expr(true)?;
                self.skip_group();
                Some(E::Scrutinee(Box::new(scrutinee)))
            }
            "while" | "loop" => {
                self.at += 1;
                while self.peek().is_some() && !self.is("{") {
                    if matches!(self.peek(), Some(Tok::P("(" | "["))) {
                        self.skip_group();
                    } else {
                        self.at += 1;
                    }
                }
                self.skip_group();
                Some(E::Opaque)
            }
            "return" => {
                self.at += 1;
                if self.is(";") || self.is("}") {
                    return Some(E::Return(None));
                }
                Some(E::Return(Some(Box::new(self.expr(false)?))))
            }
            "break" | "continue" => {
                self.at += 1;
                if matches!(self.peek(), Some(Tok::Life)) {
                    self.at += 1;
                }
                Some(E::Opaque)
            }
            _ => {
                let mut path = Vec::new();
                loop {
                    path.push(self.ident()?);
                    if self.is("::") {
                        self.at += 1;
                        if self.is("<") {
                            self.skip_group();
                            if !self.eat("::") {
                                break;
                            }
                        }
                        continue;
                    }
                    break;
                }
                if self.is("!") && !matches!(self.peek_at(1), Some(Tok::P("="))) {
                    self.at += 1;
                    let name = path.last().cloned().unwrap_or_default();
                    let open = match self.peek() {
                        Some(Tok::P(p)) if matches!(*p, "(" | "[" | "{") => *p,
                        _ => return None,
                    };
                    let close = match open {
                        "(" => ")",
                        "[" => "]",
                        _ => "}",
                    };
                    if name == "vec" {
                        let start = self.at;
                        if let Some(E::Repeat(a, b)) = self.primary(false) {
                            return Some(E::Repeat(a, b));
                        }
                        self.at = start;
                    }
                    return Some(E::Macro(name, self.args(open, close)?));
                }
                if self.is("{") && !no_struct && path.last().is_some_and(|s| s.starts_with(|c: char| c.is_uppercase())) {
                    self.skip_group();
                    return Some(E::Opaque);
                }
                Some(E::Path(path))
            }
        }
    }
}

// The evaluator.

#[derive(Clone, Debug)]
enum V {
    Str(String),
    Int(i128),
    Bool(bool),
    Char(char),
    List(Vec<V>),
    Tuple(Vec<V>),
    Closure(Rc<Lambda>, Rc<Vec<(String, V)>>),
    Fn(String),
    Unit,
    Opaque,
}

const STEPS: usize = 2_000_000;
const DEPTH: usize = 64;
const ITERATIONS: usize = 300;

struct Eval<'f> {
    files: &'f [File],
    helpers: Option<usize>,
    scope: usize,
    env: Vec<HashMap<String, V>>,
    found: Vec<String>,
    steps: usize,
    depth: usize,
    returning: Option<V>,
}

fn text(v: &V) -> Option<String> {
    match v {
        V::Str(s) => Some(s.clone()),
        V::Int(n) => Some(n.to_string()),
        V::Char(c) => Some(c.to_string()),
        V::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// A `collect` or a `let` of type `ty`: strings concatenated when it is a `String`.
fn coerce(v: V, ty: &str) -> V {
    let string = ty.contains("\"String\"") && !ty.contains("\"Vec\"") && !ty.contains("P(\"[\")") && !ty.contains("P(\"(\")");
    match v {
        V::List(items) if string => items.iter().map(text).collect::<Option<String>>().map_or(V::Opaque, V::Str),
        other => other,
    }
}

fn truthy(v: &V) -> Option<bool> {
    match v {
        V::Bool(b) => Some(*b),
        _ => None,
    }
}

impl Eval<'_> {
    fn record(&mut self, v: &V) {
        match v {
            V::Str(s) if is_program(s) => self.found.push(s.clone()),
            V::List(items) | V::Tuple(items) => {
                for item in items {
                    if let V::Str(s) = item
                        && is_program(s)
                    {
                        self.found.push(s.clone());
                    }
                }
            }
            _ => {}
        }
    }

    fn lookup(&self, name: &str) -> Option<V> {
        self.env.iter().rev().find_map(|frame| frame.get(name).cloned())
    }

    fn set(&mut self, name: &str, v: V) {
        for frame in self.env.iter_mut().rev() {
            if let Some(slot) = frame.get_mut(name) {
                *slot = v;
                return;
            }
        }
    }

    fn bind(&mut self, pat: &Pat, v: V) {
        match (pat, v) {
            (Pat::Name(n), v) => {
                if let Some(frame) = self.env.last_mut() {
                    frame.insert(n.clone(), v);
                }
            }
            (Pat::Tuple(ps), V::Tuple(vs) | V::List(vs)) => {
                for (p, v) in ps.iter().zip(vs) {
                    self.bind(p, v);
                }
            }
            (Pat::Tuple(ps), _) => ps.iter().for_each(|p| self.bind(p, V::Opaque)),
            (Pat::Other, _) => {}
        }
    }

    /// The files a path is looked up in: the current one, its parents, a named child module, and
    /// the shared helpers.
    fn scopes(&self, path: &[String]) -> Vec<usize> {
        let mut out = Vec::new();
        if let [module, _] = path
            && let Some(&child) = self.files[self.scope].children.get(module)
        {
            out.push(child);
        }
        let mut at = Some(self.scope);
        while let Some(f) = at {
            out.push(f);
            at = self.files[f].parent;
        }
        out.extend(self.helpers);
        out
    }

    fn function(&self, path: &[String]) -> Option<(usize, Rc<FnDef>)> {
        let name = path.last()?;
        self.scopes(path).into_iter().find_map(|f| self.files[f].fns.get(name).map(|d| (f, d.clone())))
    }

    fn eval(&mut self, e: &E) -> V {
        self.steps += 1;
        if self.steps > STEPS || self.returning.is_some() {
            return V::Opaque;
        }
        match e {
            E::Lit(v) => v.clone(),
            E::Path(path) => self.path(path),
            E::Macro(name, args) => self.mac(name, args),
            E::Call(f, args) => {
                let values: Vec<V> = args.iter().map(|a| self.eval(a)).collect();
                values.iter().for_each(|v| self.record(v));
                match &**f {
                    E::Path(path) => self.call_path(path, values),
                    other => {
                        let callee = self.eval(other);
                        self.call(&callee, values)
                    }
                }
            }
            E::Method(recv, name, turbofish, args) => self.method(recv, name, turbofish, args),
            E::Field(recv, name) => match (self.eval(recv), name.parse::<usize>()) {
                (V::Tuple(vs), Ok(k)) => vs.get(k).cloned().unwrap_or(V::Opaque),
                _ => V::Opaque,
            },
            E::Index(recv, index) => match (self.eval(recv), self.eval(index)) {
                (V::List(vs), V::Int(k)) => usize::try_from(k).ok().and_then(|k| vs.get(k).cloned()).unwrap_or(V::Opaque),
                _ => V::Opaque,
            },
            E::Array(items) => {
                let values: Vec<V> = items.iter().map(|a| self.eval(a)).collect();
                values.iter().for_each(|v| self.record(v));
                V::List(values)
            }
            E::Repeat(item, n) => match (self.eval(item), self.eval(n)) {
                (v, V::Int(n)) if (0..=1000).contains(&n) => V::List(vec![v; n as usize]),
                _ => V::Opaque,
            },
            E::Tuple(items) => V::Tuple(items.iter().map(|a| self.eval(a)).collect()),
            E::Unary(op, inner) => {
                let v = self.eval(inner);
                match (*op, v) {
                    ("!", V::Bool(b)) => V::Bool(!b),
                    ("-", V::Int(n)) => V::Int(-n),
                    ("&" | "*", v) => v,
                    _ => V::Opaque,
                }
            }
            E::Bin(op, a, b) => self.binary(op, a, b),
            E::Assign(op, lhs, rhs) => {
                let v = self.eval(rhs);
                if let E::Path(path) = &**lhs
                    && let [name] = path.as_slice()
                {
                    let next = match (*op, self.lookup(name), v) {
                        ("=", _, v) => v,
                        ("+=", Some(V::Str(s)), v) => text(&v).map_or(V::Opaque, |t| V::Str(s + &t)),
                        ("+=", Some(V::Int(n)), V::Int(m)) => V::Int(n + m),
                        ("-=", Some(V::Int(n)), V::Int(m)) => V::Int(n - m),
                        _ => V::Opaque,
                    };
                    self.set(name, next);
                }
                V::Unit
            }
            E::Range(a, b, inclusive) => match (self.eval(a), self.eval(b)) {
                (V::Int(a), V::Int(b)) => {
                    let end = if *inclusive { b + 1 } else { b };
                    V::List((a..end).take(ITERATIONS).map(V::Int).collect())
                }
                _ => V::Opaque,
            },
            E::Closure(lambda) => {
                let captured: Vec<(String, V)> = self.env.iter().flat_map(|f| f.iter().map(|(k, v)| (k.clone(), v.clone()))).collect();
                V::Closure(lambda.clone(), Rc::new(captured))
            }
            E::Block(stmts) => {
                self.env.push(HashMap::new());
                let mut result = V::Unit;
                for s in stmts {
                    if self.returning.is_some() {
                        break;
                    }
                    match s {
                        S::Let(pat, ty, init) => {
                            let v = init.as_ref().map_or(V::Opaque, |e| self.eval(e));
                            let v = coerce(v, ty);
                            self.bind(pat, v);
                        }
                        S::Expr(e) => {
                            self.eval(e);
                        }
                        S::Tail(e) => result = self.eval(e),
                    }
                }
                self.env.pop();
                result
            }
            E::If(cond, then, otherwise) => match truthy(&self.eval(cond)) {
                Some(true) => self.eval(then),
                Some(false) => otherwise.as_ref().map_or(V::Unit, |o| self.eval(o)),
                None => {
                    self.eval(then);
                    if let Some(o) = otherwise {
                        self.eval(o);
                    }
                    V::Opaque
                }
            },
            E::For(pat, iter, body) => {
                if let V::List(items) = self.eval(iter) {
                    for item in items.into_iter().take(ITERATIONS) {
                        if self.returning.is_some() || self.steps > STEPS {
                            break;
                        }
                        self.env.push(HashMap::new());
                        self.bind(pat, item);
                        self.eval(body);
                        self.env.pop();
                    }
                }
                V::Unit
            }
            E::Scrutinee(e) => {
                self.eval(e);
                V::Opaque
            }
            E::Return(value) => {
                let v = value.as_ref().map_or(V::Unit, |e| self.eval(e));
                self.returning = Some(v);
                V::Opaque
            }
            E::Opaque => V::Opaque,
        }
    }

    fn path(&mut self, path: &[String]) -> V {
        if let [name] = path
            && let Some(v) = self.lookup(name)
        {
            return v;
        }
        let name = path.last().cloned().unwrap_or_default();
        if let Some(f) = self.scopes(path).into_iter().find(|&f| self.files[f].consts.contains_key(&name)) {
            let e = self.files[f].consts[&name].clone();
            let scope = std::mem::replace(&mut self.scope, f);
            let v = self.eval(&e);
            self.scope = scope;
            return v;
        }
        if self.function(path).is_some() {
            return V::Fn(path.join("::"));
        }
        V::Opaque
    }

    fn call_path(&mut self, path: &[String], args: Vec<V>) -> V {
        if let [name] = path
            && let Some(v) = self.lookup(name)
        {
            return self.call(&v, args);
        }
        let joined = path.join("::");
        let first = args.first().cloned();
        match joined.as_str() {
            "String::new" => return V::Str(String::new()),
            "Vec::new" | "Vec::with_capacity" => return V::List(Vec::new()),
            "String::from" | "Some" | "Ok" | "Box::new" | "Rc::new" => return first.unwrap_or(V::Opaque),
            "std::iter::once" | "iter::once" => return V::List(vec![first.unwrap_or(V::Opaque)]),
            _ => {}
        }
        match self.function(path) {
            Some((file, def)) => self.run(file, &def, args),
            None => V::Opaque,
        }
    }

    fn run(&mut self, file: usize, def: &FnDef, args: Vec<V>) -> V {
        if self.depth >= DEPTH {
            return V::Opaque;
        }
        self.depth += 1;
        let scope = std::mem::replace(&mut self.scope, file);
        let env = std::mem::replace(&mut self.env, vec![HashMap::new()]);
        for (p, v) in def.params.iter().zip(args) {
            self.bind(p, v);
        }
        let mut v = self.eval(&def.body);
        if let Some(r) = self.returning.take() {
            v = r;
        }
        self.env = env;
        self.scope = scope;
        self.depth -= 1;
        coerce(v, &def.ret)
    }

    fn call(&mut self, callee: &V, args: Vec<V>) -> V {
        match callee {
            V::Closure(lambda, captured) => {
                if self.depth >= DEPTH {
                    return V::Opaque;
                }
                self.depth += 1;
                self.env.push(captured.iter().cloned().collect());
                self.env.push(HashMap::new());
                for (p, v) in lambda.params.iter().zip(args) {
                    self.bind(p, v);
                }
                let mut v = self.eval(&lambda.body);
                if let Some(r) = self.returning.take() {
                    v = r;
                }
                self.env.pop();
                self.env.pop();
                self.depth -= 1;
                v
            }
            V::Fn(path) => {
                let path: Vec<String> = path.split("::").map(str::to_owned).collect();
                self.call_path(&path, args)
            }
            _ => V::Opaque,
        }
    }

    fn binary(&mut self, op: &str, a: &E, b: &E) -> V {
        let x = self.eval(a);
        if op == "&&" || op == "||" {
            return match (truthy(&x), op) {
                (Some(false), "&&") => V::Bool(false),
                (Some(true), "||") => V::Bool(true),
                (Some(_), _) => match truthy(&self.eval(b)) {
                    Some(y) => V::Bool(y),
                    None => V::Opaque,
                },
                (None, _) => V::Opaque,
            };
        }
        let y = self.eval(b);
        match (op, &x, &y) {
            ("+", V::Str(s), _) => text(&y).map_or(V::Opaque, |t| V::Str(format!("{s}{t}"))),
            ("+", V::Int(m), V::Int(n)) => V::Int(m + n),
            ("-", V::Int(m), V::Int(n)) => V::Int(m - n),
            ("*", V::Int(m), V::Int(n)) => V::Int(m * n),
            ("/", V::Int(m), V::Int(n)) if *n != 0 => V::Int(m / n),
            ("%", V::Int(m), V::Int(n)) if *n != 0 => V::Int(m % n),
            ("==" | "!=", _, _) => match (text(&x), text(&y)) {
                (Some(p), Some(q)) => V::Bool((p == q) == (op == "==")),
                _ => V::Opaque,
            },
            ("<", V::Int(m), V::Int(n)) => V::Bool(m < n),
            (">", V::Int(m), V::Int(n)) => V::Bool(m > n),
            ("<=", V::Int(m), V::Int(n)) => V::Bool(m <= n),
            (">=", V::Int(m), V::Int(n)) => V::Bool(m >= n),
            _ => V::Opaque,
        }
    }

    fn mac(&mut self, name: &str, args: &[E]) -> V {
        match name {
            "format" => self.format(args),
            "concat" => args.iter().map(|a| text(&self.eval(a))).collect::<Option<String>>().map_or(V::Opaque, V::Str),
            "vec" => {
                let values: Vec<V> = args.iter().map(|a| self.eval(a)).collect();
                values.iter().for_each(|v| self.record(v));
                V::List(values)
            }
            "write" | "writeln" => {
                args.iter().for_each(|a| {
                    self.eval(a);
                });
                V::Opaque
            }
            _ => {
                let values: Vec<V> = args.iter().filter(|a| !matches!(a, E::Assign(..))).map(|a| self.eval(a)).collect();
                values.iter().for_each(|v| self.record(v));
                V::Opaque
            }
        }
    }

    /// `format!`: `{}`, `{name}`, `{0}` and `{:?}`, with a width and alignment.
    fn format(&mut self, args: &[E]) -> V {
        let Some(V::Str(fmt)) = args.first().map(|a| self.eval(a)) else { return V::Opaque };
        let mut positional = Vec::new();
        let mut named = HashMap::new();
        for a in &args[1..] {
            match a {
                E::Assign("=", lhs, rhs) => {
                    if let E::Path(p) = &**lhs {
                        let v = self.eval(rhs);
                        named.insert(p.join("::"), v);
                    }
                }
                other => {
                    let v = self.eval(other);
                    positional.push(v);
                }
            }
        }
        let chars: Vec<char> = fmt.chars().collect();
        let mut out = String::new();
        let mut next = 0;
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if (c == '{' || c == '}') && chars.get(i + 1) == Some(&c) {
                out.push(c);
                i += 2;
                continue;
            }
            if c != '{' {
                out.push(c);
                i += 1;
                continue;
            }
            let Some(end) = (i..chars.len()).find(|&k| chars[k] == '}') else { return V::Opaque };
            let inside: String = chars[i + 1..end].iter().collect();
            i = end + 1;
            let (arg, spec) = inside.split_once(':').unwrap_or((&inside, ""));
            let v = if arg.is_empty() {
                next += 1;
                positional.get(next - 1).cloned()
            } else if let Ok(k) = arg.parse::<usize>() {
                positional.get(k).cloned()
            } else {
                named.get(arg).cloned().or_else(|| self.lookup(arg)).or_else(|| Some(self.path(&[arg.to_owned()])))
            };
            let number = matches!(v, Some(V::Int(_)));
            let shown = match (v, spec.contains('?')) {
                (Some(V::Str(s)), true) => format!("{s:?}"),
                (Some(V::Char(c)), true) => format!("{c:?}"),
                (Some(v), _) => match text(&v) {
                    Some(t) => t,
                    None => return V::Opaque,
                },
                (None, _) => return V::Opaque,
            };
            let spec = spec.trim_end_matches('?');
            let digits: String = spec.chars().filter(char::is_ascii_digit).collect();
            let width: usize = digits.trim_start_matches('0').parse().unwrap_or(0);
            let pad = width.saturating_sub(shown.chars().count());
            let zero = spec.starts_with('0') && !digits.is_empty();
            if spec.contains('<') {
                out.push_str(&shown);
                out.extend(std::iter::repeat_n(' ', pad));
            } else if spec.contains('^') {
                out.extend(std::iter::repeat_n(' ', pad / 2));
                out.push_str(&shown);
                out.extend(std::iter::repeat_n(' ', pad - pad / 2));
            } else if spec.contains('>') || zero || number {
                out.extend(std::iter::repeat_n(if zero { '0' } else { ' ' }, pad));
                out.push_str(&shown);
            } else {
                out.push_str(&shown);
                out.extend(std::iter::repeat_n(' ', pad));
            }
        }
        V::Str(out)
    }

    fn method(&mut self, recv: &E, name: &str, turbofish: &str, args: &[E]) -> V {
        if matches!(name, "push_str" | "push" | "extend" | "insert" | "clear" | "truncate")
            && let E::Path(path) = recv
            && let [var] = path.as_slice()
        {
            let arg = args.first().map(|a| self.eval(a)).unwrap_or(V::Opaque);
            let current = self.lookup(var);
            let next = match (name, current, arg) {
                ("push_str" | "push", Some(V::Str(s)), a) => text(&a).map_or(V::Opaque, |t| V::Str(s + &t)),
                ("push", Some(V::List(mut items)), a) => {
                    items.push(a);
                    V::List(items)
                }
                ("extend", Some(V::List(mut items)), V::List(more)) => {
                    items.extend(more);
                    V::List(items)
                }
                ("extend", Some(V::Str(s)), V::List(more)) => more.iter().map(text).collect::<Option<String>>().map_or(V::Opaque, |t| V::Str(s + &t)),
                ("clear", Some(V::Str(_)), _) => V::Str(String::new()),
                ("clear", Some(V::List(_)), _) => V::List(Vec::new()),
                _ => V::Opaque,
            };
            self.set(var, next);
            return V::Unit;
        }
        let this = self.eval(recv);
        let args: Vec<V> = args.iter().map(|a| self.eval(a)).collect();
        args.iter().for_each(|v| self.record(v));
        let arg = |k: usize| args.get(k).cloned().unwrap_or(V::Opaque);
        match (this, name) {
            (v, "to_owned" | "to_string" | "into" | "clone" | "as_str" | "as_ref" | "borrow" | "unwrap" | "expect" | "unwrap_or_default" | "iter" | "into_iter"
            | "copied" | "cloned" | "to_vec" | "as_slice" | "by_ref") => v,
            (V::Str(s), "to_uppercase" | "to_ascii_uppercase") => V::Str(s.to_uppercase()),
            (V::Str(s), "to_lowercase" | "to_ascii_lowercase") => V::Str(s.to_lowercase()),
            (V::Str(s), "trim") => V::Str(s.trim().to_owned()),
            (V::Str(s), "trim_end") => V::Str(s.trim_end().to_owned()),
            (V::Str(s), "trim_start") => V::Str(s.trim_start().to_owned()),
            (V::Str(s), "len") => V::Int(s.len() as i128),
            (V::Str(s), "is_empty") => V::Bool(s.is_empty()),
            (V::Str(s), "replace") => match (text(&arg(0)), text(&arg(1))) {
                (Some(a), Some(b)) => V::Str(s.replace(&a, &b)),
                _ => V::Opaque,
            },
            (V::Str(s), "replacen") => match (text(&arg(0)), text(&arg(1)), arg(2)) {
                (Some(a), Some(b), V::Int(n)) => V::Str(s.replacen(&a, &b, n as usize)),
                _ => V::Opaque,
            },
            (V::Str(s), "repeat") => match arg(0) {
                V::Int(n) if (0..=10_000).contains(&n) => V::Str(s.repeat(n as usize)),
                _ => V::Opaque,
            },
            (V::Str(s), "contains" | "starts_with" | "ends_with") => match text(&arg(0)) {
                Some(t) => V::Bool(match name {
                    "contains" => s.contains(&t),
                    "starts_with" => s.starts_with(&t),
                    _ => s.ends_with(&t),
                }),
                None => V::Opaque,
            },
            (V::Str(s), "split_whitespace") => V::List(s.split_whitespace().map(|w| V::Str(w.to_owned())).collect()),
            (V::Str(s), "lines") => V::List(s.lines().map(|w| V::Str(w.to_owned())).collect()),
            (V::Str(s), "chars") => V::List(s.chars().map(V::Char).collect()),
            (V::Str(s), "split") => match text(&arg(0)) {
                Some(t) => V::List(s.split(t.as_str()).map(|w| V::Str(w.to_owned())).collect()),
                None => V::Opaque,
            },
            (V::List(items), "concat") => {
                if items.iter().all(|i| matches!(i, V::List(_))) {
                    V::List(items.into_iter().flat_map(|i| if let V::List(v) = i { v } else { Vec::new() }).collect())
                } else {
                    items.iter().map(text).collect::<Option<String>>().map_or(V::Opaque, V::Str)
                }
            }
            (V::List(items), "join") => match text(&arg(0)) {
                Some(sep) => items.iter().map(text).collect::<Option<Vec<String>>>().map_or(V::Opaque, |parts| V::Str(parts.join(&sep))),
                None => V::Opaque,
            },
            (V::List(items), "map" | "flat_map" | "filter" | "filter_map" | "for_each") => {
                let f = arg(0);
                let mut out = Vec::new();
                for item in items.into_iter().take(ITERATIONS * 10) {
                    let r = self.call(&f, vec![item.clone()]);
                    match name {
                        "map" | "filter_map" => out.push(r),
                        "flat_map" => match r {
                            V::List(v) => out.extend(v),
                            other => out.push(other),
                        },
                        "filter" if truthy(&r) != Some(false) => out.push(item),
                        _ => {}
                    }
                }
                V::List(out)
            }
            (V::List(mut items), "chain") => match arg(0) {
                V::List(more) => {
                    items.extend(more);
                    V::List(items)
                }
                _ => V::Opaque,
            },
            (V::List(items), "enumerate") => V::List(items.into_iter().enumerate().map(|(k, v)| V::Tuple(vec![V::Int(k as i128), v])).collect()),
            (V::List(items), "zip") => match arg(0) {
                V::List(more) => V::List(items.into_iter().zip(more).map(|(a, b)| V::Tuple(vec![a, b])).collect()),
                _ => V::Opaque,
            },
            (V::List(mut items), "rev") => {
                items.reverse();
                V::List(items)
            }
            (V::List(items), "take" | "skip") => match arg(0) {
                V::Int(n) if n >= 0 => V::List(if name == "take" { items.into_iter().take(n as usize).collect() } else { items.into_iter().skip(n as usize).collect() }),
                _ => V::Opaque,
            },
            (V::List(items), "collect") => coerce(V::List(items), turbofish),
            (V::List(items), "len" | "count") => V::Int(items.len() as i128),
            (V::List(items), "is_empty") => V::Bool(items.is_empty()),
            (V::List(items), "first" | "next") => items.first().cloned().unwrap_or(V::Opaque),
            (V::List(items), "last") => items.last().cloned().unwrap_or(V::Opaque),
            (V::List(items), "get") => match arg(0) {
                V::Int(k) => usize::try_from(k).ok().and_then(|k| items.get(k).cloned()).unwrap_or(V::Opaque),
                _ => V::Opaque,
            },
            _ => V::Opaque,
        }
    }
}
