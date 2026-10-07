//! The Communication feature of the 1985 standard (ANSI X3.23-1985 section XIV): the COMMUNICATION
//! SECTION's CD entries and the ENABLE, DISABLE, RECEIVE, SEND, PURGE and ACCEPT MESSAGE COUNT
//! statements. Enterprise COBOL compiles none of them (Migration Guide, "Communication language
//! items affected"), so each is read through and refused where it stands, and a CD's area is
//! declared so the rest of the program reads as written. IBM's own message for them is not known:
//! no Enterprise COBOL listing of a program that uses them was available.

use super::{Parser, R};
use crate::ast::*;
use crate::lexer::Tok;
use crate::messages::IWS0102;
use crate::Pos;

/// A CD's fields in the order the standard lays out its area: the words that name each in a
/// clause, after an optional SYMBOLIC or MESSAGE, and its PICTURE.
type Fields = &'static [(&'static [&'static str], &'static str)];

const INPUT: Fields = &[
    (&["QUEUE"], "X(12)"),
    (&["SUB-QUEUE-1"], "X(12)"),
    (&["SUB-QUEUE-2"], "X(12)"),
    (&["SUB-QUEUE-3"], "X(12)"),
    (&["DATE"], "9(6)"),
    (&["TIME"], "9(8)"),
    (&["SOURCE"], "X(12)"),
    (&["TEXT", "LENGTH"], "9(4)"),
    (&["END", "KEY"], "X"),
    (&["STATUS", "KEY"], "XX"),
    (&["COUNT"], "9(6)"),
];

const IO: Fields = &[
    (&["DATE"], "9(6)"),
    (&["TIME"], "9(8)"),
    (&["TERMINAL"], "X(12)"),
    (&["TEXT", "LENGTH"], "9(4)"),
    (&["END", "KEY"], "X"),
    (&["STATUS", "KEY"], "XX"),
];

const OUTPUT: Fields = &[(&["DESTINATION", "COUNT"], "9(4)"), (&["TEXT", "LENGTH"], "9(4)"), (&["STATUS", "KEY"], "XX")];

/// An output CD's destination table, each entry an error key and a symbolic destination.
const DESTINATION: Fields = &[(&["ERROR", "KEY"], "X"), (&["DESTINATION"], "X(12)")];

/// Statements of the feature, which Enterprise COBOL does not reserve: they begin a statement only in
/// a program with a CD, and ENABLE and DISABLE wherever INPUT, OUTPUT or I-O follows.
const VERBS: &[&str] = &["ENABLE", "DISABLE", "RECEIVE", "SEND", "PURGE"];

impl Parser<'_> {
    pub(super) fn is_verb(&self, w: &str) -> bool {
        super::VERBS.contains(&w) || !self.cds.is_empty() && VERBS.contains(&w)
    }

    pub(super) fn at_enable_or_disable(&self) -> bool {
        (self.is_word("ENABLE") || self.is_word("DISABLE")) && self.word_at(1).is_some_and(|w| ["INPUT", "OUTPUT", "I-O"].contains(&w))
    }

    fn refuse_communication(&mut self, pos: Pos, item: &str) {
        self.messages.push(IWS0102.at(pos, format!("the Communication feature ({item}) is not part of Enterprise COBOL, which does not compile it")));
    }

    /// The CD entries after COMMUNICATION SECTION, whose header is at `header`, as the data
    /// entries of their areas.
    pub(super) fn communication_section(&mut self, header: Pos) -> R<Vec<DataEntry>> {
        self.refuse_communication(header, "COMMUNICATION SECTION");
        let mut entries = Vec::new();
        while self.is_word("CD") {
            entries.extend(self.cd_entry()?);
        }
        Ok(entries)
    }

    fn cd_entry(&mut self) -> R<Vec<DataEntry>> {
        let pos = self.pos();
        self.at += 1;
        let name = self.name("a CD name")?;
        self.refuse_communication(pos, &format!("CD {name}"));
        self.cds.push(name.clone());
        self.accept_word("FOR");
        self.accept_word("INITIAL");
        let fields = match self.accept_any(&["INPUT", "OUTPUT", "I-O"]).as_deref() {
            Some("INPUT") => INPUT,
            Some("OUTPUT") => OUTPUT,
            Some(_) => IO,
            None => return Err(self.error("INPUT, OUTPUT or I-O after the CD name")),
        };
        let table_fields: Fields = if std::ptr::eq(fields, OUTPUT) { DESTINATION } else { &[] };
        let all: Vec<_> = fields.iter().chain(table_fields).collect();
        let mut named: Vec<Option<String>> = vec![None; all.len()];
        let (mut positional, mut occurs, mut indexes) = (Vec::new(), None, Vec::new());
        while self.peek().is_some_and(|t| *t != Tok::Period) {
            self.accept_any(&["SYMBOLIC", "MESSAGE"]);
            if self.is_word("DESTINATION") && self.word_at(1) == Some("TABLE") {
                self.at += 2;
                self.expect_word("OCCURS")?;
                occurs = Some(self.integer("the number of destinations")?);
                self.accept_word("TIMES");
                if self.accept_word("INDEXED") {
                    self.accept_word("BY");
                    while self.peek().is_some_and(|t| *t != Tok::Period) && !self.at_cd_clause() {
                        indexes.push(self.name("an index name")?);
                    }
                }
                continue;
            }
            let matched = all.iter().enumerate().filter(|(_, (words, _))| words.iter().enumerate().all(|(k, w)| self.word_at(k) == Some(*w))).max_by_key(|(_, (words, _))| words.len());
            if let Some((k, (words, _))) = matched {
                self.at += words.len();
                self.accept_word("IS");
                named[k] = Some(self.name("a data-name")?);
            } else {
                positional.push(self.name("a CD clause or a data-name")?);
            }
        }
        self.expect(&Tok::Period, "a period after the CD entry")?;
        let record = if matches!(self.peek(), Some(Tok::Number(_))) { self.data_entries()? } else { Vec::new() };
        // A record after the CD describes the same area; the names it declares are its own.
        let field = |k: usize| {
            named[k].clone().or_else(|| positional.get(k).cloned()).filter(|n| n != "FILLER" && !record.iter().any(|e| e.name.as_deref() == Some(n))).unwrap_or_else(|| "FILLER".into())
        };
        if !record.is_empty() && (0..all.len()).all(|k| field(k) == "FILLER") {
            return Ok(record);
        }
        let mut text = format!("       01  {name}.\n");
        for (k, (_, picture)) in fields.iter().enumerate() {
            text.push_str(&format!("           05  {} PIC {picture}.\n", field(k)));
        }
        let level = if let Some(n) = occurs {
            text.push_str(&format!("           05  FILLER OCCURS {n} TIMES"));
            if !indexes.is_empty() {
                text.push_str(&format!("\n               INDEXED BY {}", indexes.join(" ")));
            }
            text.push_str(".\n");
            "10"
        } else {
            "05"
        };
        for (k, (_, picture)) in table_fields.iter().enumerate() {
            text.push_str(&format!("               {level}  {} PIC {picture}.\n", field(fields.len() + k)));
        }
        let mut area = super::system_text_entries(&text)?;
        for e in &mut area {
            e.pos = pos;
        }
        area.extend(record);
        Ok(area)
    }

    fn at_cd_clause(&self) -> bool {
        ["SYMBOLIC", "QUEUE", "SUB-QUEUE-1", "SUB-QUEUE-2", "SUB-QUEUE-3", "MESSAGE", "SOURCE", "TERMINAL", "TEXT", "END", "STATUS", "ERROR", "DESTINATION"].iter().any(|w| self.is_word(w))
    }

    fn integer(&mut self, what: &str) -> R<u32> {
        match self.peek() {
            Some(Tok::Number(n)) => {
                let n = n.parse().map_err(|_| self.error(what))?;
                self.at += 1;
                Ok(n)
            }
            _ => Err(self.error(what)),
        }
    }

    /// ENABLE, DISABLE, RECEIVE, SEND or PURGE, `verb` already read, as a statement that does
    /// nothing once refused.
    pub(super) fn communication_statement(&mut self, verb: &str, pos: Pos) -> R<Stmt> {
        match verb {
            "ENABLE" | "DISABLE" => {
                match self.accept_any(&["INPUT", "OUTPUT", "I-O"]).as_deref() {
                    Some("INPUT") => {
                        self.accept_word("TERMINAL");
                    }
                    Some("I-O") => self.expect_word("TERMINAL")?,
                    Some(_) => {}
                    None => return Err(self.error("INPUT, OUTPUT or I-O TERMINAL")),
                }
                self.name("a CD name")?;
                let with = self.accept_word("WITH");
                if self.accept_word("KEY") {
                    self.operand()?;
                } else if with {
                    return Err(self.error("KEY"));
                }
            }
            "RECEIVE" => {
                self.name("a CD name")?;
                if self.accept_any(&["MESSAGE", "SEGMENT"]).is_none() {
                    return Err(self.error("MESSAGE or SEGMENT"));
                }
                self.expect_word("INTO")?;
                self.reference()?;
                if self.accept_word("NO") {
                    self.expect_word("DATA")?;
                    self.block(&["WITH", "END-RECEIVE"])?;
                }
                if self.is_word("WITH") && self.word_at(1) == Some("DATA") {
                    self.at += 2;
                    self.block(&["END-RECEIVE"])?;
                }
                self.accept_word("END-RECEIVE");
            }
            "SEND" => {
                self.name("a CD name")?;
                if self.accept_word("FROM") {
                    self.reference()?;
                }
                if self.accept_word("WITH") && self.accept_any(&["ESI", "EMI", "EGI"]).is_none() {
                    self.reference()?;
                }
                if self.accept_any(&["BEFORE", "AFTER"]).is_some() {
                    self.accept_word("ADVANCING");
                    if !self.accept_word("PAGE") {
                        self.operand()?;
                        self.accept_any(&["LINE", "LINES"]);
                    }
                }
                if self.accept_word("REPLACING") {
                    self.accept_word("LINE");
                }
            }
            _ => {
                self.name("a CD name")?;
            }
        }
        self.refuse_communication(pos, &format!("{verb} statement"));
        Ok(Stmt::Continue { pos })
    }

    /// ACCEPT cd-name [MESSAGE] COUNT, its target already read, when that is what follows.
    pub(super) fn accept_message_count(&mut self, pos: Pos) -> Option<Stmt> {
        let words = if self.is_word("COUNT") { 1 } else if self.is_word("MESSAGE") && self.word_at(1) == Some("COUNT") { 2 } else { return None };
        self.at += words;
        self.refuse_communication(pos, "ACCEPT MESSAGE COUNT");
        Some(Stmt::Continue { pos })
    }

    /// A CD named in USE FOR DEBUGGING ON, refused and passed over: whether one was.
    pub(super) fn debugging_on_cd(&mut self) -> bool {
        let Some(name) = self.word_at(0).map(str::to_string).filter(|w| self.cds.contains(w)) else { return false };
        let pos = self.pos();
        self.at += 1;
        self.refuse_communication(pos, &format!("USE FOR DEBUGGING ON CD {name}"));
        true
    }
}
