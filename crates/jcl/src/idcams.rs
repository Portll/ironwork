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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Each entry is a data set name, or NAME(MEMBER).
    Delete(Vec<String>),
    Repro { from: Target, to: Target },
    DefineCluster(String),
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

fn repro(args: &[Token]) -> Result<Command, String> {
    let (mut from, mut to) = (None, None);
    for a in args {
        match a {
            Token::Keyed(k, v) => match k.as_str() {
                "INFILE" | "IFILE" => from = Some(Target::Dd(dd_name(v)?)),
                "INDATASET" | "IDS" => from = Some(Target::Dataset(name_of(v)?)),
                "OUTFILE" | "OFILE" => to = Some(Target::Dd(dd_name(v)?)),
                "OUTDATASET" | "ODS" => to = Some(Target::Dataset(name_of(v)?)),
                k => return Err(format!("REPRO parameter {k} is not supported yet")),
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

fn dd_name(v: &str) -> Result<String, String> {
    let name = v.split_whitespace().next().unwrap_or("");
    if crate::is_name(name) && v.split_whitespace().count() == 1 { Ok(name.to_string()) } else { Err(format!("{v} is not a DD name")) }
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

fn define(args: &[Token]) -> Result<Command, String> {
    let mut name = None;
    for a in args {
        match a {
            Token::Keyed(k, v) if k == "GENERATIONDATAGROUP" || k == "GDG" => return define_gdg(v),
            Token::Keyed(k, v) if k == "CLUSTER" || k == "CL" => {
                let inner = tokens(v)?;
                for t in inner {
                    if let Token::Keyed(k, v) = t
                        && k == "NAME"
                    {
                        name = Some(name_of(&v)?);
                    }
                }
            }
            Token::Keyed(k, _) if matches!(k.as_str(), "DATA" | "INDEX" | "IX" | "CATALOG" | "CAT") => {}
            Token::Keyed(k, _) | Token::Word(k) => return Err(format!("DEFINE {k} is not supported yet; DEFINE CLUSTER and GDG are")),
            other => return Err(format!("DEFINE has {other:?} where a parameter belongs")),
        }
    }
    name.map(Command::DefineCluster).ok_or_else(|| "DEFINE CLUSTER needs NAME".into())
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
            Command::DefineCluster("PROD.KSDS".into()),
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
    fn a_plus_joins_without_a_blank() {
        assert_eq!(parse(&cards("DELETE PROD.LON+\nG.NAME")).unwrap(), [Command::Delete(vec!["PROD.LONG.NAME".into()])]);
    }

    #[test]
    fn what_is_not_modelled_is_refused_by_name() {
        for (text, message) in [
            ("PRINT INFILE(X) CHARACTER", "the IDCAMS command PRINT is not supported yet"),
            ("LISTCAT ALL", "the IDCAMS command LISTCAT is not supported yet"),
            ("DELETE PROD.*", "DELETE of a generic name (PROD.*) is not supported yet"),
            ("REPRO INFILE(A) OUTFILE(B) REPLACE", "REPRO parameter REPLACE is not supported yet"),
            ("REPRO INFILE(A) OUTFILE(B) COUNT(5)", "REPRO parameter COUNT is not supported yet"),
            ("DEFINE ALIAS(NAME(X.Y) RELATE(CAT))", "DEFINE ALIAS is not supported yet; DEFINE CLUSTER and GDG are"),
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
