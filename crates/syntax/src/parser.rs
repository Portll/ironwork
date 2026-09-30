use crate::ast::*;
use crate::lexer::{Tok, Token};
use crate::{Error, Pos};

mod declaratives;
mod oo;
mod report;
mod sort;

/// Every program in the source, first to last, with nested programs after the one containing them.
pub fn parse(tokens: &[Token], options: Vec<String>) -> Result<Vec<Program>, Error> {
    let mut parser = Parser { tokens, at: 0, exec_declarations: Vec::new(), cics: false, sql: SqlState::default(), mnemonics: Vec::new(), debugging: false };
    let mut programs = Vec::new();
    parser.program(&options, &mut programs)?;
    while parser.peek().is_some() {
        if !parser.at_division(&["IDENTIFICATION", "ID"]) {
            return Err(parser.error("another program, or the end of the source"));
        }
        parser.program(&options, &mut programs)?;
    }
    Ok(programs)
}

/// Words that begin a statement: a list of operands ends at any of them.
const VERBS: &[&str] = &[
    "MOVE", "COMPUTE", "ADD", "SUBTRACT", "MULTIPLY", "DIVIDE", "IF", "PERFORM", "DISPLAY", "INITIALIZE", "GO", "GOBACK", "STOP",
    "CONTINUE", "EXIT", "EVALUATE", "SET", "CALL", "ACCEPT", "STRING", "UNSTRING", "INSPECT", "READ", "WRITE", "OPEN", "CLOSE",
    "REWRITE", "DELETE", "START", "SEARCH", "SORT", "MERGE", "RETURN", "RELEASE", "CANCEL", "EXEC", "NEXT", "INVOKE",
    "INITIATE", "GENERATE", "TERMINATE", "SUPPRESS", "ALTER", "ENTRY",
];

/// Words that end a phrase or a nested block.
const PHRASE_WORDS: &[&str] = &[
    "ELSE", "END-IF", "END-PERFORM", "END-COMPUTE", "END-ADD", "END-SUBTRACT", "END-MULTIPLY", "END-DIVIDE", "END-DISPLAY", "WHEN",
    "TO", "FROM", "BY", "INTO", "GIVING", "REMAINDER", "ROUNDED", "ON", "NOT", "SIZE", "UNTIL", "VARYING", "TIMES", "THRU", "THROUGH",
    "AND", "OR", "THEN", "UPON", "WITH", "IS", "END-EVALUATE", "ALSO", "OTHER", "OF", "IN", "AT", "END", "END-READ", "END-WRITE",
    "BEFORE", "AFTER", "ADVANCING", "INPUT", "OUTPUT", "EXTEND", "I-O", "REVERSED", "USING", "RETURNING", "EXCEPTION", "OVERFLOW",
    "END-CALL", "OMITTED", "CONTENT", "REFERENCE", "VALUE", "UP", "DOWN", "DELIMITED", "DELIMITER", "COUNT", "POINTER", "TALLYING",
    "REPLACING", "CONVERTING", "INITIAL", "FOR", "CHARACTERS", "LEADING", "FIRST", "ALL", "END-STRING", "END-UNSTRING", "END-SEARCH",
    "NEXT", "INVALID", "KEY", "END-REWRITE", "END-DELETE", "END-START", "END-INVOKE", "END-RETURN", "END-OF-PAGE", "EOP",
];

/// The environment-names a WRITE ADVANCING mnemonic-name can stand for (Language Reference,
/// SPECIAL-NAMES, Table 5): channels C01 to C12, CSP, pockets S01 to S05, and AFP-5A.
fn advancing_environment_name(word: &str) -> bool {
    let numbered = |prefix: char, last: u8| word.len() == 3 && word.starts_with(prefix) && word[1..].parse::<u8>().is_ok_and(|n| (1..=last).contains(&n));
    matches!(word, "CSP" | "AFP-5A") || numbered('C', 12) || numbered('S', 5)
}

fn figurative(word: &str) -> Option<Figurative> {
    Some(match word {
        "ZERO" | "ZEROS" | "ZEROES" => Figurative::Zero,
        "SPACE" | "SPACES" => Figurative::Space,
        "HIGH-VALUE" | "HIGH-VALUES" => Figurative::HighValue,
        "LOW-VALUE" | "LOW-VALUES" => Figurative::LowValue,
        "QUOTE" | "QUOTES" => Figurative::Quote,
        "NULL" | "NULLS" => Figurative::Null,
        _ => return None,
    })
}

/// A character that can stand for a currency sign in a PICTURE: a single byte that is no digit, space,
/// PICTURE letter or one of + - , . * / ; ( ) " = ' (Language Reference SC27-8713-03, p. 130).
fn currency_symbol(c: char) -> bool {
    u32::from(c) < 256 && !c.is_ascii_digit() && !"ABCDEGNPRSUVXZabcdegnprsuvxz +-,.*/;()\"='".contains(c)
}

/// A contained program has the alphabets, collating sequence, decimal point, currency signs and
/// debugging mode of the program containing it, whose configuration section is the only one
/// (Language Reference SC27-8713-03, p. 121).
fn share_configuration(outer: &Environment, inner: &mut Environment) {
    inner.debugging_mode |= outer.debugging_mode;
    inner.decimal_point_comma |= outer.decimal_point_comma;
    if inner.currency.is_empty() {
        inner.currency.clone_from(&outer.currency);
    }
    if inner.collating_sequence.is_none() {
        inner.collating_sequence.clone_from(&outer.collating_sequence);
    }
    for (name, alphabet) in &outer.alphabets {
        if !inner.alphabets.iter().any(|(n, _)| n == name) {
            inner.alphabets.push((name.clone(), alphabet.clone()));
        }
    }
}

fn usage_word(word: &str) -> Option<Usage> {
    Some(match word {
        "DISPLAY" => Usage::Display,
        "BINARY" | "COMP" | "COMPUTATIONAL" | "COMP-4" | "COMPUTATIONAL-4" => Usage::Binary,
        "COMP-5" | "COMPUTATIONAL-5" => Usage::NativeBinary,
        "PACKED-DECIMAL" | "COMP-3" | "COMPUTATIONAL-3" => Usage::Packed,
        "COMP-1" | "COMPUTATIONAL-1" => Usage::Float1,
        "COMP-2" | "COMPUTATIONAL-2" => Usage::Float2,
        "NATIONAL" => Usage::National,
        "POINTER" => Usage::Pointer,
        "INDEX" => Usage::Index,
        "FUNCTION-POINTER" | "PROCEDURE-POINTER" => Usage::ProgramPointer,
        _ => return None,
    })
}

struct Parser<'a> {
    tokens: &'a [Token],
    at: usize,
    /// DATA DIVISION EXEC blocks of the program being parsed.
    exec_declarations: Vec<ExecBlock>,
    /// Whether the program being parsed has EXEC CICS, so the translator's additions apply.
    cics: bool,
    /// The WRITE ADVANCING mnemonic-names in scope: the program's own, then those of the programs
    /// containing it, whose configuration section applies to it too.
    mnemonics: Vec<(String, String)>,
    sql: SqlState,
    /// WITH DEBUGGING MODE, from the program's configuration section or its container's.
    debugging: bool,
}

/// The WHENEVER actions in force, which carry on in listing order, and the EXEC SQL blocks the
/// program being parsed has so far.
#[derive(Default)]
struct SqlState {
    whenever: crate::sql::Whenever,
    blocks: u32,
    cursors: crate::sql::Cursors,
}

type R<T> = Result<T, Error>;

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.at).map(|t| &t.tok)
    }

    fn peek_at(&self, ahead: usize) -> Option<&Tok> {
        self.tokens.get(self.at + ahead).map(|t| &t.tok)
    }

    fn pos(&self) -> Pos {
        self.tokens.get(self.at).or(self.tokens.last()).map(|t| t.pos).unwrap_or_default()
    }

    fn word(&self) -> Option<&str> {
        match self.peek() {
            Some(Tok::Word(w)) => Some(w),
            _ => None,
        }
    }

    fn word_at(&self, ahead: usize) -> Option<&str> {
        match self.peek_at(ahead) {
            Some(Tok::Word(w)) => Some(w),
            _ => None,
        }
    }

    fn is_word(&self, w: &str) -> bool {
        self.word() == Some(w)
    }

    fn accept_word(&mut self, w: &str) -> bool {
        let yes = self.is_word(w);
        if yes {
            self.at += 1;
        }
        yes
    }

    fn accept_any(&mut self, words: &[&str]) -> Option<String> {
        let w = self.word().filter(|w| words.contains(w))?.to_owned();
        self.at += 1;
        Some(w)
    }

    fn expect_word(&mut self, w: &str) -> R<()> {
        if self.accept_word(w) { Ok(()) } else { Err(self.error(format!("expected {w}"))) }
    }

    fn accept(&mut self, tok: &Tok) -> bool {
        let yes = self.peek() == Some(tok);
        if yes {
            self.at += 1;
        }
        yes
    }

    fn expect(&mut self, tok: &Tok, what: &str) -> R<()> {
        if self.accept(tok) { Ok(()) } else { Err(self.error(format!("expected {what}"))) }
    }

    fn error(&self, message: impl Into<String>) -> Error {
        let found = match self.peek() {
            None => "end of source".to_owned(),
            Some(Tok::Word(w)) => w.clone(),
            Some(t) => format!("{t:?}"),
        };
        Error::at(self.pos(), format!("{}, found {found}", message.into()))
    }

    fn name(&mut self, what: &str) -> R<String> {
        match self.peek() {
            Some(Tok::Word(w)) => {
                let w = w.clone();
                self.at += 1;
                Ok(w)
            }
            _ => Err(self.error(format!("expected {what}"))),
        }
    }

    fn at_division(&self, names: &[&str]) -> bool {
        self.word().is_some_and(|w| names.contains(&w)) && self.word_at(1) == Some("DIVISION")
    }

    fn program(&mut self, options: &[String], out: &mut Vec<Program>) -> R<()> {
        let outer = (std::mem::take(&mut self.exec_declarations), std::mem::take(&mut self.cics), std::mem::take(&mut self.sql.blocks), self.mnemonics.clone(), self.debugging);
        let parsed = self.one_program(options, out);
        (self.exec_declarations, self.cics, self.sql.blocks, self.mnemonics, self.debugging) = outer;
        parsed
    }

    fn one_program(&mut self, options: &[String], out: &mut Vec<Program>) -> R<()> {
        if !self.accept_word("IDENTIFICATION") {
            self.expect_word("ID")?;
        }
        self.expect_word("DIVISION")?;
        self.expect(&Tok::Period, "a period")?;
        if self.is_word("CLASS-ID") {
            return self.class_definition(options, out);
        }
        self.expect_word("PROGRAM-ID")?;
        self.accept(&Tok::Period);
        let id = match self.peek() {
            Some(Tok::Alnum(s)) => {
                let s = s.clone();
                self.at += 1;
                s
            }
            _ => self.name("a program name")?,
        };
        let (mut initial, mut recursive) = (false, false);
        while let Some(t) = self.peek() {
            if *t == Tok::Period {
                self.at += 1;
                break;
            }
            initial |= self.is_word("INITIAL");
            recursive |= self.is_word("RECURSIVE");
            self.at += 1;
        }
        self.program_body(id, initial, recursive, options, out, false)
    }

    /// The rest of a program, or of a method after its METHOD-ID paragraph; a method's END METHOD
    /// is left for its class to read.
    fn program_body(&mut self, id: String, initial: bool, recursive: bool, options: &[String], out: &mut Vec<Program>, method: bool) -> R<()> {
        while self.peek().is_some() && !self.at_division(&["ENVIRONMENT", "DATA", "PROCEDURE", "IDENTIFICATION", "ID"]) && !self.at_end_program() {
            self.at += 1;
        }
        let (mut files, mut repository, mut environment) = (Vec::new(), Vec::new(), Environment::default());
        if self.at_division(&["ENVIRONMENT"]) {
            (files, repository) = self.environment(&mut environment)?;
        }
        self.mnemonics.splice(0..0, environment.mnemonics.iter().cloned());
        self.debugging |= environment.debugging_mode;
        let (mut working_storage, mut local_storage, mut linkage) = (Vec::new(), Vec::new(), Vec::new());
        let mut report_writer = crate::report::ReportWriter::default();
        let mut declaratives = Declaratives::default();
        if self.at_division(&["DATA"]) {
            self.at += 2;
            self.expect(&Tok::Period, "a period")?;
            while !self.at_division(&["PROCEDURE", "IDENTIFICATION", "ID"]) && !self.at_end_program() && self.peek().is_some() {
                if self.data_exec()? {
                    continue;
                }
                let section = self.name("a DATA DIVISION section")?;
                self.expect_word("SECTION")?;
                self.expect(&Tok::Period, "a period")?;
                match section.as_str() {
                    "WORKING-STORAGE" => working_storage = self.data_entries()?,
                    "LINKAGE" => linkage = self.data_entries()?,
                    "LOCAL-STORAGE" => local_storage = self.data_entries()?,
                    "FILE" => self.file_section(&mut files)?,
                    "REPORT" => report_writer.reports.extend(self.report_section()?),
                    other => return Err(self.error(format!("the {other} SECTION is not supported yet"))),
                }
            }
        }
        let (mut using, mut returning) = (Vec::new(), None);
        let paragraphs = if self.at_division(&["PROCEDURE"]) {
            self.at += 2;
            if self.accept_word("USING") {
                using = self.parameters()?;
            }
            if self.accept_word("RETURNING") {
                returning = Some(self.name("a RETURNING item")?);
            }
            self.expect(&Tok::Period, "a period after the PROCEDURE DIVISION header")?;
            self.procedure_paragraphs(&mut report_writer, &mut declaratives)?
        } else {
            Vec::new()
        };
        declaratives::debugging_sections_allowed(&declaratives, recursive, method)?;
        if let Some(f) = files.iter().find(|f| f.assign.is_empty()) {
            return Err(Error::at(f.pos, format!("{} has no SELECT ... ASSIGN", f.name)));
        }
        if self.cics {
            self.translator_additions(&mut linkage, &mut using)?;
        }
        let exec_declarations = std::mem::take(&mut self.exec_declarations);
        let (mut nested, mut contained) = (Vec::new(), Vec::new());
        while self.at_division(&["IDENTIFICATION", "ID"]) {
            let first = nested.len();
            self.program(options, &mut nested)?;
            contained.extend(nested.get(first).map(|p: &Program| p.id.clone()));
        }
        let entry = nested.iter().flat_map(|p: &Program| &p.paragraphs).flat_map(|p| &p.statements).find_map(|s| match s {
            Stmt::Entry { pos, .. } => Some(*pos),
            _ => None,
        });
        if let Some(pos) = entry {
            return Err(Error::at(pos, "ENTRY cannot be used in a nested program"));
        }
        oo::share_repository(&repository, &mut nested)?;
        for inner in &mut nested {
            share_configuration(&environment, &mut inner.environment);
        }
        declaratives::contained_programs(&declaratives, &report_writer, &nested)?;
        if !method && self.at_end_program() && self.word_at(1) == Some("PROGRAM") {
            self.at += 2;
            if self.word().is_some() || matches!(self.peek(), Some(Tok::Alnum(_))) {
                self.at += 1;
            }
            self.accept(&Tok::Period);
        }
        out.push(Program {
            id,
            options: options.to_vec(),
            initial,
            recursive,
            working_storage,
            local_storage,
            linkage,
            using,
            returning,
            paragraphs,
            files,
            exec_declarations,
            report_writer,
            declaratives,
            oo: oo::program_oo(repository),
            environment,
            nested: contained,
            ..Program::default()
        });
        out.extend(nested);
        Ok(())
    }

    /// The LINKAGE items of a PROCEDURE DIVISION or ENTRY USING list, each BY REFERENCE or BY VALUE.
    fn parameters(&mut self) -> R<Vec<Param>> {
        let (mut using, mut by_value) = (Vec::new(), false);
        loop {
            if self.accept_word("BY") {
                by_value = self.accept_any(&["REFERENCE", "VALUE"]).as_deref() == Some("VALUE");
                continue;
            }
            if !self.starts_ref() {
                return Ok(using);
            }
            using.push(Param { by_value, name: self.name("a LINKAGE item")? });
        }
    }

    fn at_end_program(&self) -> bool {
        self.is_word("END") && matches!(self.word_at(1), Some("PROGRAM" | "METHOD"))
    }

    /// The ENVIRONMENT DIVISION: SELECT entries of FILE-CONTROL and the REPOSITORY's classes;
    /// everything else is skipped, except what would change the meaning of the rest of the program.
    fn environment(&mut self, clauses: &mut Environment) -> R<(Vec<FileDecl>, Vec<ClassEntry>)> {
        let (mut files, mut repository) = (Vec::new(), Vec::new());
        while self.peek().is_some() && !self.at_division(&["DATA", "PROCEDURE"]) {
            if self.accept_word("DECIMAL-POINT") {
                self.accept_word("IS");
                self.expect_word("COMMA")?;
                clauses.decimal_point_comma = true;
                continue;
            }
            if self.is_word("CURRENCY") {
                let pos = self.pos();
                self.at += 1;
                let sign = self.currency_sign()?;
                if clauses.currency.iter().any(|c| c.symbol == sign.symbol) {
                    return Err(Error::at(pos, format!("a second CURRENCY SIGN clause for the currency symbol {:?}", sign.symbol)));
                }
                clauses.currency.push(sign);
                continue;
            }
            if self.environment_clause(clauses)? {
                continue;
            }
            if self.is_word("DEBUGGING") && self.word_at(1) == Some("MODE") {
                self.at += 2;
                clauses.debugging_mode = true;
                continue;
            }
            if let Some(environment) = self.word().filter(|w| advancing_environment_name(w)).map(str::to_owned) {
                let at_name = if self.word_at(1) == Some("IS") { 2 } else { 1 };
                if let Some(name) = self.word_at(at_name).map(str::to_owned) {
                    self.at += at_name + 1;
                    clauses.mnemonics.push((name, environment));
                    continue;
                }
            }
            if self.accept_word("SELECT") {
                files.push(self.select()?);
                continue;
            }
            if self.accept_word("REPOSITORY") {
                repository = self.repository()?;
                continue;
            }
            self.at += 1;
        }
        Ok((files, repository))
    }

    /// CURRENCY [SIGN] [IS] literal-6 [[WITH] PICTURE SYMBOL literal-7], after CURRENCY, checked as
    /// the Language Reference lists (SC27-8713-03, pp. 129-130).
    fn currency_sign(&mut self) -> R<CurrencySign> {
        self.accept_word("SIGN");
        self.accept_word("IS");
        let pos = self.pos();
        let value = match self.literal()? {
            Literal::Alnum(v) if !v.is_empty() => v,
            Literal::Hex(_) => return Err(Error::at(pos, "a hexadecimal CURRENCY SIGN literal is not supported yet")),
            _ => return Err(Error::at(pos, "CURRENCY SIGN needs a nonempty alphanumeric literal")),
        };
        let with = self.accept_word("WITH");
        if !(self.is_word("PICTURE") && self.word_at(1) == Some("SYMBOL")) {
            if with {
                return Err(self.error("expected PICTURE SYMBOL after WITH"));
            }
            let mut chars = value.chars();
            return match (chars.next(), chars.next()) {
                (Some(symbol), None) if currency_symbol(symbol) => Ok(CurrencySign { value, symbol }),
                _ => Err(Error::at(pos, format!("CURRENCY SIGN {value:?} is not one character that can be a PICTURE currency symbol"))),
            };
        }
        self.at += 2;
        if value.chars().any(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | '.' | ',')) {
            return Err(Error::at(pos, format!("CURRENCY SIGN {value:?} contains a digit, +, -, . or ,")));
        }
        let pos = self.pos();
        let symbol = match self.literal()? {
            Literal::Alnum(s) => s,
            _ => String::new(),
        };
        let mut chars = symbol.chars();
        match (chars.next(), chars.next()) {
            (Some(symbol), None) if currency_symbol(symbol) => Ok(CurrencySign { value, symbol }),
            _ => Err(Error::at(pos, format!("PICTURE SYMBOL {symbol:?} is not one character that can be a PICTURE currency symbol"))),
        }
    }

    fn select(&mut self) -> R<FileDecl> {
        let pos = self.pos();
        let optional = self.accept_word("OPTIONAL");
        let name = self.name("a file name")?;
        let mut f = FileDecl {
            name,
            assign: String::new(),
            organization: Organization::Sequential,
            access: Access::Sequential,
            record_key: None,
            alternate_keys: Vec::new(),
            relative_key: None,
            optional,
            status: None,
            recording: None,
            record_min: None,
            record_max: None,
            records: Vec::new(),
            reports: Vec::new(),
            linage: None,
            sort: false,
            pos,
        };
        while !self.accept(&Tok::Period) {
            let clause = self.name("a SELECT clause or a period")?;
            match clause.as_str() {
                "ASSIGN" => {
                    self.accept_word("TO");
                    let target = match self.peek().cloned() {
                        Some(Tok::Word(w)) | Some(Tok::Alnum(w)) => w,
                        _ => return Err(self.error("a DD name after ASSIGN")),
                    };
                    self.at += 1;
                    let target = target.to_ascii_uppercase();
                    f.assign = target.rsplit('-').next().filter(|_| target.contains("-S-") || target.starts_with("S-") || target.starts_with("AS-")).unwrap_or(&target).to_owned();
                }
                "RECORD" if !self.is_word("SEQUENTIAL") => {
                    self.accept_word("KEY");
                    self.accept_word("IS");
                    f.record_key = Some(self.reference()?);
                }
                "RELATIVE" if self.is_word("KEY") => {
                    self.at += 1;
                    self.accept_word("IS");
                    f.relative_key = Some(self.reference()?);
                }
                "ALTERNATE" => {
                    self.accept_word("RECORD");
                    self.expect_word("KEY")?;
                    self.accept_word("IS");
                    let key = self.reference()?;
                    let duplicates = self.accept_word("WITH") | self.is_word("DUPLICATES");
                    if duplicates {
                        self.expect_word("DUPLICATES")?;
                    }
                    f.alternate_keys.push((key, duplicates));
                }
                "ORGANIZATION" | "LINE" | "RECORD" | "SEQUENTIAL" | "INDEXED" | "RELATIVE" => {
                    if clause == "ORGANIZATION" {
                        self.accept_word("IS");
                    }
                    let first = if clause == "ORGANIZATION" { self.name("an organization")? } else { clause.clone() };
                    f.organization = match first.as_str() {
                        "LINE" => {
                            self.expect_word("SEQUENTIAL")?;
                            Organization::LineSequential
                        }
                        "RECORD" => {
                            self.expect_word("SEQUENTIAL")?;
                            Organization::Sequential
                        }
                        "SEQUENTIAL" => Organization::Sequential,
                        "INDEXED" => Organization::Indexed,
                        "RELATIVE" => Organization::Relative,
                        other => return Err(self.error(format!("ORGANIZATION {other} is not supported yet"))),
                    };
                }
                "ACCESS" => {
                    self.accept_word("MODE");
                    self.accept_word("IS");
                    f.access = match self.name("an access mode")?.as_str() {
                        "SEQUENTIAL" => Access::Sequential,
                        "RANDOM" => Access::Random,
                        "DYNAMIC" => Access::Dynamic,
                        other => return Err(self.error(format!("ACCESS MODE {other} is not an access mode"))),
                    };
                }
                "FILE" | "STATUS" => {
                    if clause == "FILE" {
                        self.expect_word("STATUS")?;
                    }
                    self.accept_word("IS");
                    f.status = Some(self.reference()?);
                    if self.starts_ref() && !self.word().is_some_and(|w| SELECT_CLAUSES.contains(&w)) {
                        self.reference()?;
                    }
                }
                "RESERVE" | "PADDING" => {
                    while self.peek().is_some() && self.peek() != Some(&Tok::Period) && !self.word().is_some_and(|w| SELECT_CLAUSES.contains(&w)) {
                        self.at += 1;
                    }
                }
                other => return Err(self.error(format!("{other} is not a SELECT clause ironwork for COBOL supports yet"))),
            }
        }
        Ok(f)
    }

    /// FD entries: the clauses that shape records are kept, the rest skipped; then each file's
    /// record descriptions.
    fn file_section(&mut self, files: &mut [FileDecl]) -> R<()> {
        while self.is_word("FD") || self.is_word("SD") {
            let indicator = self.name("FD or SD")?;
            let pos = self.pos();
            let name = self.name("a file name")?;
            let Some(index) = files.iter().position(|f| f.name == name) else {
                return Err(Error::at(pos, format!("{indicator} {name} has no SELECT")));
            };
            files[index].sort = indicator == "SD";
            while !self.accept(&Tok::Period) {
                match self.name("an FD clause or a period")?.as_str() {
                    "RECORDING" => {
                        self.accept_word("MODE");
                        self.accept_word("IS");
                        let mode = self.name("F, V, U or S")?;
                        files[index].recording = mode.chars().next();
                    }
                    "RECORD" => {
                        self.accept_word("CONTAINS");
                        self.accept_word("IS");
                        if self.accept_word("VARYING") {
                            files[index].recording.get_or_insert('V');
                            self.accept_word("IN");
                            self.accept_word("SIZE");
                            self.accept_word("FROM");
                        }
                        let number = |p: &mut Self| -> R<Option<u32>> {
                            match p.peek() {
                                Some(Tok::Number(n)) => {
                                    let v = n.parse().map_err(|_| p.error("a record length"))?;
                                    p.at += 1;
                                    Ok(Some(v))
                                }
                                _ => Ok(None),
                            }
                        };
                        let first = number(self)?;
                        let second = if self.accept_word("TO") { number(self)? } else { None };
                        if first.is_some() {
                            files[index].record_min = first;
                            files[index].record_max = second.or(first);
                        }
                        self.accept_word("CHARACTERS");
                        if self.accept_word("DEPENDING") {
                            self.accept_word("ON");
                            self.reference()?;
                        }
                    }
                    "REPORT" | "REPORTS" if files[index].sort => return Err(Error::at(pos, format!("SD {name}: a sort or merge file takes no REPORT clause"))),
                    "REPORT" | "REPORTS" => {
                        let names = self.report_names()?;
                        files[index].reports.extend(names);
                    }
                    "LINAGE" => {
                        let linage = self.linage()?;
                        if files[index].linage.is_some() {
                            return Err(Error::at(pos, format!("{indicator} {name}: LINAGE is given twice")));
                        }
                        files[index].linage = (indicator == "FD").then_some(linage);
                    }
                    _ => {
                        while self.peek().is_some() && self.peek() != Some(&Tok::Period) && !self.word().is_some_and(|w| FD_WORDS.contains(&w)) {
                            self.at += 1;
                        }
                    }
                }
            }
            files[index].records = self.data_entries()?;
        }
        Ok(())
    }

    /// The LINAGE clause after its keyword; FOOTING, TOP and BOTTOM are taken in any order.
    fn linage(&mut self) -> R<Linage> {
        self.accept_word("IS");
        let lines = self.linage_value("LINAGE")?;
        self.accept_word("LINES");
        let mut linage = Linage { lines, footing: None, top: None, bottom: None };
        loop {
            let start = self.at;
            self.accept_word("WITH");
            self.accept_word("LINES");
            self.accept_word("AT");
            let Some(phrase) = self.accept_any(&["FOOTING", "TOP", "BOTTOM"]) else {
                self.at = start;
                return Ok(linage);
            };
            if phrase == "FOOTING" {
                self.accept_word("AT");
            }
            let value = Some(self.linage_value(&phrase)?);
            let slot = match phrase.as_str() {
                "FOOTING" => &mut linage.footing,
                "TOP" => &mut linage.top,
                _ => &mut linage.bottom,
            };
            if slot.is_some() {
                return Err(self.error(format!("LINAGE: {phrase} is given twice")));
            }
            *slot = value;
        }
    }

    /// An unsigned integer, or a data-name that may be qualified.
    fn linage_value(&mut self, phrase: &str) -> R<LinageValue> {
        match self.peek() {
            Some(Tok::Number(n)) if n.bytes().all(|b| b.is_ascii_digit()) => {
                let n = n.clone();
                self.at += 1;
                Ok(LinageValue::Integer(n))
            }
            Some(Tok::Number(n)) => Err(self.error(format!("LINAGE: {phrase} {n} is not an unsigned integer"))),
            Some(Tok::Word(_)) if self.starts_ref() => {
                let r = self.reference()?;
                if !r.subscripts.is_empty() || r.refmod.is_some() {
                    return Err(Error::at(r.pos, format!("LINAGE: {phrase} {} takes no subscript or reference modification", r.name)));
                }
                Ok(LinageValue::Data(r))
            }
            _ => Err(self.error(format!("an integer or a data-name after {phrase}"))),
        }
    }

    /// An EXEC block among DATA DIVISION entries, kept as a declaration with its period.
    fn data_exec(&mut self) -> R<bool> {
        let Some(Tok::Exec(text)) = self.peek().cloned() else { return Ok(false) };
        let block = self.exec_block(&text, self.pos());
        self.at += 1;
        self.accept(&Tok::Period);
        self.exec_declarations.push(block);
        Ok(true)
    }

    /// What the CICS translator adds: the EXEC interface block, a one-byte DFHCOMMAREA when the
    /// program declares none, and both at the head of PROCEDURE DIVISION USING.
    fn translator_additions(&mut self, linkage: &mut Vec<DataEntry>, using: &mut Vec<Param>) -> R<()> {
        let defined = |l: &[DataEntry], n: &str| l.iter().any(|e| e.level == 1 && e.name.as_deref() == Some(n));
        let mut added = Vec::new();
        if !defined(linkage, "DFHEIBLK") {
            added.extend(system_entries("DFHEIBLK")?);
        }
        if !defined(linkage, "DFHCOMMAREA") {
            added.extend(system_text_entries("       01  DFHCOMMAREA PIC X(1).\n")?);
        }
        linkage.splice(0..0, added);
        for (i, name) in ["DFHEIBLK", "DFHCOMMAREA"].into_iter().enumerate() {
            if using.get(i).map(|p| p.name.as_str()) != Some(name) {
                using.insert(i, Param { by_value: false, name: name.into() });
            }
        }
        Ok(())
    }

    fn exec_block(&mut self, text: &str, pos: Pos) -> ExecBlock {
        let (kind_word, body) = text.split_once(' ').unwrap_or((text, ""));
        let kind = match kind_word.to_ascii_uppercase().as_str() {
            "SQL" => ExecKind::Sql,
            "CICS" => ExecKind::Cics,
            "DLI" => ExecKind::Dli,
            _ => ExecKind::Other,
        };
        let words: Vec<String> = body.split_whitespace().map(|w| w.to_ascii_uppercase()).collect();
        let word = |i: usize| words.get(i).map(String::as_str).unwrap_or("");
        let mut block = ExecBlock { kind, command: word(0).to_owned(), options: Vec::new(), host_variables: Vec::new(), sql: None, text: text.to_owned(), pos };
        match kind {
            ExecKind::Sql => {
                block.command = match (word(0), word(1), word(2)) {
                    ("DECLARE", _, "CURSOR") => "DECLARE CURSOR".into(),
                    ("DECLARE", _, "TABLE") => "DECLARE TABLE".into(),
                    ("DECLARE", _, "STATEMENT") => "DECLARE STATEMENT".into(),
                    ("BEGIN" | "END", "DECLARE", "SECTION") => format!("{} DECLARE SECTION", word(0)),
                    (first, _, _) => first.into(),
                };
                let statement = self.sql.cursors.resolve(crate::sql::parse(body, pos));
                if let crate::sql::Statement::Whenever { condition, action } = &statement {
                    self.sql.whenever.set(*condition, action.clone());
                }
                block.host_variables = match &statement {
                    crate::sql::Statement::Unsupported(_)
                    | crate::sql::Statement::Malformed(_)
                    | crate::sql::Statement::Declaration
                    | crate::sql::Statement::DeclareUnsupported { .. } => host_variables(body, pos),
                    typed => typed.references().into_iter().cloned().collect(),
                };
                self.sql.blocks += 1;
                block.sql = Some(crate::sql::Sql { statement, ordinal: self.sql.blocks, whenever: self.sql.whenever.clone() });
            }
            ExecKind::Cics => {
                self.cics = true;
                block.options = cics_options(body);
                if let Some((first, None)) = block.options.first().cloned() {
                    block.command = first.clone();
                    block.options.remove(0);
                    if let Some((second, None)) = block.options.first().cloned()
                        && crate::system::cics_two_word(&first, &second)
                    {
                        block.command = format!("{first} {second}");
                        block.options.remove(0);
                    }
                }
                let labels = block.command.starts_with("HANDLE");
                for (_, arg) in &mut block.options {
                    if let Some(ExecArg::Text(t)) = arg
                        && !labels
                        && let Some(op) = operand_of(t, pos)
                    {
                        *arg = Some(ExecArg::Operand(op));
                    }
                }
            }
            _ => {}
        }
        block
    }

    fn data_entries(&mut self) -> R<Vec<DataEntry>> {
        let mut entries = Vec::new();
        loop {
            if self.data_exec()? {
                continue;
            }
            let Some((level, pos)) = self.level_number()? else { break };
            entries.push(self.data_entry(level, pos)?);
        }
        Ok(entries)
    }

    fn level_number(&mut self) -> R<Option<(u8, Pos)>> {
        let Some(Tok::Number(level)) = self.peek() else { return Ok(None) };
        let pos = self.pos();
        let level = level.parse().map_err(|_| self.error("a level number"))?;
        self.at += 1;
        Ok(Some((level, pos)))
    }

    /// The character-string after PICTURE.
    fn picture(&mut self) -> R<String> {
        self.accept_word("IS");
        match self.peek() {
            Some(Tok::Pic(p)) => {
                let p = p.clone();
                self.at += 1;
                Ok(p)
            }
            _ => Err(self.error("a PICTURE character-string")),
        }
    }

    /// The SIGN clause, after its first word: SIGN, LEADING or TRAILING.
    fn sign_clause(&mut self, first: &str) -> R<SignClause> {
        let side = if first == "SIGN" {
            self.accept_word("IS");
            self.name("LEADING or TRAILING")?
        } else {
            first.to_owned()
        };
        let position = match side.as_str() {
            "LEADING" => SignPosition::Leading,
            "TRAILING" => SignPosition::Trailing,
            _ => return Err(self.error("LEADING or TRAILING")),
        };
        let separate = self.accept_word("SEPARATE");
        if separate {
            self.accept_word("CHARACTER");
        }
        Ok(SignClause { position, separate })
    }

    /// WHEN ZERO, after BLANK.
    fn blank_when_zero(&mut self) -> R<()> {
        self.accept_word("WHEN");
        if self.accept_any(&["ZERO", "ZEROS", "ZEROES"]).is_none() {
            return Err(self.error("ZERO after BLANK WHEN"));
        }
        Ok(())
    }

    fn data_entry(&mut self, level: u8, pos: Pos) -> R<DataEntry> {
        let mut e = DataEntry {
            level,
            name: None,
            picture: None,
            usage: None,
            value: None,
            redefines: None,
            occurs: None,
            depending_on: None,
            sign: None,
            justified: false,
            sync: false,
            blank_when_zero: false,
            indexed_by: Vec::new(),
            keys: Vec::new(),
            condition_values: Vec::new(),
            false_value: None,
            renames: None,
            object_class: None,
            pos,
        };
        if let Some(w) = self.word()
            && !is_clause_word(w)
        {
            if w != "FILLER" {
                e.name = Some(w.to_owned());
            }
            self.at += 1;
        }
        while !self.accept(&Tok::Period) {
            let clause = self.name("a data description clause or a period")?;
            match clause.as_str() {
                "PIC" | "PICTURE" => e.picture = Some(self.picture()?),
                "USAGE" => {
                    self.accept_word("IS");
                    let w = self.name("a usage")?;
                    if w == "OBJECT" {
                        self.object_reference(&mut e)?;
                    } else {
                        e.usage = Some(usage_word(&w).ok_or_else(|| Error::at(pos, format!("USAGE {w} is not supported yet")))?);
                    }
                }
                "OBJECT" => self.object_reference(&mut e)?,
                "VALUE" | "VALUES" => {
                    self.accept_word("IS");
                    self.accept_word("ARE");
                    if level == 88 {
                        while self.peek().is_some() && self.peek() != Some(&Tok::Period) && !self.is_word("WHEN") && !self.is_word("FALSE") {
                            let low = self.literal()?;
                            let high = if self.accept_any(&["THRU", "THROUGH"]).is_some() { Some(self.literal()?) } else { None };
                            e.condition_values.push((low, high));
                        }
                        if self.accept_word("WHEN") {
                            self.accept_word("SET");
                            self.accept_word("TO");
                            self.expect_word("FALSE")?;
                            self.accept_word("IS");
                            e.false_value = Some(self.literal()?);
                        } else if self.accept_word("FALSE") {
                            self.accept_word("IS");
                            e.false_value = Some(self.literal()?);
                        }
                    } else {
                        e.value = Some(self.literal()?);
                    }
                }
                "REDEFINES" => e.redefines = Some(self.name("the item redefined")?),
                "RENAMES" => {
                    let first = self.reference()?;
                    let last = if self.accept_any(&["THRU", "THROUGH"]).is_some() { Some(self.reference()?) } else { None };
                    e.renames = Some((first, last));
                }
                "OCCURS" => {
                    let count = |p: &mut Self| -> R<u32> {
                        let n = match p.peek() {
                            Some(Tok::Number(n)) => n.parse().map_err(|_| p.error("an OCCURS count"))?,
                            _ => return Err(p.error("an OCCURS count")),
                        };
                        p.at += 1;
                        Ok(n)
                    };
                    let mut most = count(self)?;
                    if self.accept_word("TO") {
                        most = count(self)?;
                    }
                    e.occurs = Some(most);
                    self.accept_word("TIMES");
                    if self.accept_word("DEPENDING") {
                        self.accept_word("ON");
                        e.depending_on = Some(self.reference()?);
                    }
                    loop {
                        if let Some(order) = self.accept_any(&["ASCENDING", "DESCENDING"]) {
                            self.accept_word("KEY");
                            self.accept_word("IS");
                            while self.word().is_some_and(|w| !is_clause_word(w) && !matches!(w, "INDEXED" | "ASCENDING" | "DESCENDING")) {
                                e.keys.push((order == "ASCENDING", self.reference()?));
                            }
                        } else if self.accept_word("INDEXED") {
                            self.accept_word("BY");
                            while self.word().is_some_and(|w| !is_clause_word(w) && !matches!(w, "ASCENDING" | "DESCENDING")) {
                                e.indexed_by.push(self.name("an index name")?);
                            }
                        } else {
                            break;
                        }
                    }
                }
                "SIGN" | "LEADING" | "TRAILING" => e.sign = Some(self.sign_clause(&clause)?),
                "JUSTIFIED" | "JUST" => {
                    self.accept_word("RIGHT");
                    e.justified = true;
                }
                "SYNC" | "SYNCHRONIZED" => {
                    self.accept_any(&["LEFT", "RIGHT"]);
                    e.sync = true;
                }
                "BLANK" => {
                    self.blank_when_zero()?;
                    e.blank_when_zero = true;
                }
                "GLOBAL" | "EXTERNAL" => {}
                other => match usage_word(other) {
                    Some(u) => e.usage = Some(u),
                    None => return Err(Error::at(self.tokens[self.at - 1].pos, format!("{other} is not a data description clause ironwork for COBOL supports yet"))),
                },
            }
        }
        Ok(e)
    }

    fn literal(&mut self) -> R<Literal> {
        let lit = match self.peek().cloned() {
            Some(Tok::Alnum(s)) => Literal::Alnum(s),
            Some(Tok::Hex(b)) => Literal::Hex(b),
            Some(Tok::National(s)) => Literal::National(s),
            Some(Tok::Number(n)) => Literal::Number(n),
            Some(Tok::Word(w)) if w == "ALL" => {
                self.at += 1;
                return Ok(Literal::All(Box::new(self.literal()?)));
            }
            Some(Tok::Word(w)) => Literal::Figurative(figurative(&w).ok_or_else(|| self.error("a literal"))?),
            _ => return Err(self.error("a literal")),
        };
        self.at += 1;
        Ok(lit)
    }

    fn paragraph_header(&self) -> bool {
        self.tokens.get(self.at).is_some_and(|t| t.area_a && matches!(t.tok, Tok::Word(_))) && self.peek_at(1) == Some(&Tok::Period)
    }

    fn section_header(&self) -> bool {
        // EXIT SECTION is a statement; EXIT, reserved, names no section.
        self.tokens.get(self.at).is_some_and(|t| matches!(t.tok, Tok::Word(_))) && self.word_at(0) != Some("EXIT") && self.word_at(1) == Some("SECTION")
    }

    fn paragraphs(&mut self) -> R<Vec<Paragraph>> {
        let mut paragraphs = Vec::new();
        while self.peek().is_some() && !self.at_end_program() && !self.at_division(&["IDENTIFICATION", "ID"]) {
            if self.is_word("DECLARATIVES") {
                return Err(self.error("DECLARATIVES must begin the PROCEDURE DIVISION"));
            }
            self.procedure_item(&mut paragraphs)?;
        }
        Ok(paragraphs)
    }

    /// A section or paragraph header, a separator period, or statements, added to `paragraphs`; true for a section header.
    fn procedure_item(&mut self, paragraphs: &mut Vec<Paragraph>) -> R<bool> {
        if self.section_header() {
            let pos = self.pos();
            let name = self.name("a section name")?;
            self.at += 1;
            let mut priority = 0;
            if let Some(Tok::Number(n)) = self.peek() {
                priority = n.trim_start_matches('+').parse().ok().filter(|p| *p <= 99).ok_or_else(|| self.error("a priority-number from 0 to 99"))?;
                self.at += 1;
            }
            self.expect(&Tok::Period, "a period after the section header")?;
            paragraphs.push(Paragraph { section: Some(name.clone()), name, statements: Vec::new(), is_section: true, priority, pos });
            return Ok(true);
        }
        if self.paragraph_header() {
            let pos = self.pos();
            let name = self.name("a paragraph name")?;
            self.at += 1;
            let (section, priority) = paragraphs.last().map_or((None, 0), |p| (p.section.clone(), p.priority));
            paragraphs.push(Paragraph { name, statements: Vec::new(), section, is_section: false, priority, pos });
            return Ok(false);
        }
        if self.accept(&Tok::Period) {
            if let Some(p) = paragraphs.last_mut()
                && p.statements.last().is_some_and(|s| *s != Stmt::SentenceEnd)
            {
                p.statements.push(Stmt::SentenceEnd);
            }
            return Ok(false);
        }
        let block = self.block(&[])?;
        if block.is_empty() {
            return Err(self.error("a statement"));
        }
        if paragraphs.is_empty() {
            paragraphs.push(Paragraph { name: String::new(), statements: Vec::new(), section: None, is_section: false, priority: 0, pos: self.pos() });
        }
        paragraphs.last_mut().unwrap().statements.extend(block);
        Ok(false)
    }

    /// Statements up to a period, a paragraph header, or one of `stops`, none of them consumed.
    fn block(&mut self, stops: &[&str]) -> R<Vec<Stmt>> {
        let mut out = Vec::new();
        while let Some(tok) = self.peek() {
            if *tok == Tok::Period || self.paragraph_header() || self.section_header() || self.word().is_some_and(|w| stops.contains(&w)) {
                break;
            }
            if let Some(Tok::Exec(text)) = self.peek().cloned() {
                let block = self.exec_block(&text, self.pos());
                self.at += 1;
                out.push(Stmt::Exec(Box::new(block)));
                continue;
            }
            if !self.word().is_some_and(|w| VERBS.contains(&w)) {
                break;
            }
            out.push(self.statement()?);
        }
        Ok(out)
    }

    fn statement(&mut self) -> R<Stmt> {
        let pos = self.pos();
        let verb = self.name("a statement")?;
        Ok(match verb.as_str() {
            "MOVE" => {
                if self.accept_any(&["CORRESPONDING", "CORR"]).is_some() {
                    return Err(Error::at(pos, "MOVE CORRESPONDING is not supported yet"));
                }
                let from = self.operand()?;
                self.expect_word("TO")?;
                let to = self.refs()?;
                Stmt::Move { from, to, pos }
            }
            "COMPUTE" => {
                let targets = self.targets()?;
                if !self.accept(&Tok::Eq) {
                    self.expect_word("EQUAL")?;
                }
                let expr = self.expr()?;
                let size_error = self.size_error()?;
                self.accept_word("END-COMPUTE");
                Stmt::Compute { targets, expr, size_error, pos }
            }
            "ADD" | "SUBTRACT" | "MULTIPLY" | "DIVIDE" => Stmt::Arith(Box::new(self.arith(&verb, pos)?)),
            "IF" => {
                let cond = self.cond()?;
                self.accept_word("THEN");
                let then = self.block(&["ELSE", "END-IF"])?;
                let otherwise = if self.accept_word("ELSE") { self.block(&["END-IF"])? } else { Vec::new() };
                self.accept_word("END-IF");
                Stmt::If { cond, then, otherwise, pos }
            }
            "PERFORM" => self.perform(pos)?,
            "DISPLAY" => {
                let mut items = Vec::new();
                let no_advancing_ahead = |p: &Self| p.is_word("NO") && p.word_at(1) == Some("ADVANCING");
                while self.starts_operand() && !no_advancing_ahead(self) {
                    items.push(self.operand()?);
                }
                if self.accept_word("UPON") {
                    self.name("a mnemonic name")?;
                }
                let no_advancing = self.accept_word("WITH") | no_advancing_ahead(self);
                if no_advancing {
                    self.expect_word("NO")?;
                    self.expect_word("ADVANCING")?;
                    if self.is_word("UPON") {
                        return Err(self.error("the end of DISPLAY: Enterprise COBOL takes UPON before WITH NO ADVANCING"));
                    }
                }
                self.accept_word("END-DISPLAY");
                Stmt::Display { items, no_advancing, pos }
            }
            "INITIALIZE" => Stmt::Initialize { targets: self.refs()?, pos },
            "CALL" => Stmt::Call(Box::new(self.call(pos)?)),
            "INVOKE" => Stmt::Invoke(Box::new(self.invoke(pos)?)),
            "CANCEL" => {
                let mut targets = Vec::new();
                while self.starts_operand() {
                    targets.push(self.operand()?);
                }
                if targets.is_empty() {
                    return Err(self.error("a program to CANCEL"));
                }
                Stmt::Cancel { targets, pos }
            }
            "SET" => Stmt::Set { set: self.set()?, pos },
            "STRING" => Stmt::String(Box::new(self.string(pos)?)),
            "UNSTRING" => Stmt::Unstring(Box::new(self.unstring(pos)?)),
            "INSPECT" => Stmt::Inspect(Box::new(self.inspect(pos)?)),
            "SEARCH" => Stmt::Search(Box::new(self.search(pos)?)),
            "SORT" | "MERGE" | "RELEASE" | "RETURN" => Stmt::Sorting(Box::new(self.sorting(&verb, pos)?)),
            "NEXT" => {
                self.expect_word("SENTENCE")?;
                Stmt::NextSentence
            }
            "ACCEPT" => {
                let target = self.reference()?;
                if self.is_word("FROM") && self.word_at(1) == Some("ENVIRONMENT") {
                    return Err(Error::at(pos, "ACCEPT ... FROM ENVIRONMENT is GnuCOBOL's, not Enterprise COBOL's"));
                }
                let from = if self.accept_word("FROM") {
                    match self.name("SYSIN, DATE, DAY, DAY-OF-WEEK or TIME")?.as_str() {
                        "DATE" => AcceptFrom::Date { four_digit_year: self.accept_word("YYYYMMDD") },
                        "DAY" => AcceptFrom::Day { four_digit_year: self.accept_word("YYYYDDD") },
                        "DAY-OF-WEEK" => AcceptFrom::DayOfWeek,
                        "TIME" => AcceptFrom::Time,
                        _ => AcceptFrom::Sysin,
                    }
                } else {
                    AcceptFrom::Sysin
                };
                self.accept_word("END-ACCEPT");
                Stmt::Accept { target, from, pos }
            }
            "OPEN" => {
                let mut files = Vec::new();
                while let Some(mode) = self.accept_any(&["INPUT", "OUTPUT", "EXTEND", "I-O"]) {
                    let mode = match mode.as_str() {
                        "INPUT" => OpenMode::Input,
                        "OUTPUT" => OpenMode::Output,
                        "EXTEND" => OpenMode::Extend,
                        _ => OpenMode::InputOutput,
                    };
                    while self.starts_ref() {
                        files.push((mode, self.name("a file name")?));
                        self.accept_any(&["REVERSED"]);
                        if self.accept_word("WITH") {
                            self.expect_word("NO")?;
                            self.expect_word("REWIND")?;
                        }
                    }
                }
                if files.is_empty() {
                    return Err(self.error("INPUT, OUTPUT, EXTEND or I-O and a file"));
                }
                Stmt::Open { files, pos }
            }
            "CLOSE" => {
                let mut files = Vec::new();
                while self.starts_ref() {
                    files.push(self.name("a file name")?);
                    if self.accept_word("WITH") {
                        self.accept_any(&["LOCK", "NO"]);
                        self.accept_word("REWIND");
                    }
                }
                Stmt::Close { files, pos }
            }
            "READ" => {
                let file = self.name("a file name")?;
                let previous = self.accept_word("PREVIOUS");
                let next = previous || self.accept_word("NEXT");
                self.accept_word("RECORD");
                let into = if self.accept_word("INTO") { Some(self.reference()?) } else { None };
                let key = if self.accept_word("KEY") {
                    self.accept_word("IS");
                    Some(self.reference()?)
                } else {
                    None
                };
                let [at_end, invalid] = self.on_phrases(&["AT", "END", "INVALID"], &["END-READ"], |p| {
                    if p.accept_word("INVALID") {
                        p.accept_word("KEY");
                        return Ok(1);
                    }
                    p.accept_word("AT");
                    p.expect_word("END").map(|()| 0)
                })?;
                self.accept_word("END-READ");
                Stmt::Read(Box::new(ReadStmt { file, next, previous, into, key, at_end, invalid, pos }))
            }
            "REWRITE" => {
                let record = self.reference()?;
                let from = if self.accept_word("FROM") { Some(self.operand()?) } else { None };
                let invalid = self.invalid_key("END-REWRITE")?;
                Stmt::Rewrite { record, from, invalid, pos }
            }
            "DELETE" => {
                let file = self.name("a file name")?;
                self.accept_word("RECORD");
                let invalid = self.invalid_key("END-DELETE")?;
                Stmt::Delete { file, invalid, pos }
            }
            "START" => {
                let file = self.name("a file name")?;
                let key = if self.accept_word("KEY") {
                    self.accept_word("IS");
                    let op = if self.accept_word("NOT") {
                        match self.relop()? {
                            Some(RelOp::Lt) => RelOp::Ge,
                            _ => return Err(self.error("NOT < in START KEY")),
                        }
                    } else {
                        self.relop()?.ok_or_else(|| self.error("a relation after START KEY"))?
                    };
                    Some((op, self.reference()?))
                } else {
                    None
                };
                let invalid = self.invalid_key("END-START")?;
                Stmt::Start { file, key, invalid, pos }
            }
            "WRITE" => {
                let record = self.reference()?;
                let from = if self.accept_word("FROM") { Some(self.operand()?) } else { None };
                let mut advancing = None;
                if let Some(side) = self.accept_any(&["BEFORE", "AFTER"]) {
                    let before = side == "BEFORE";
                    self.accept_word("ADVANCING");
                    let mnemonic = self.word().and_then(|w| self.mnemonics.iter().find(|(name, _)| name == w)).cloned();
                    advancing = Some(if self.accept_word("PAGE") {
                        Advancing::Page { before }
                    } else if let Some((name, environment)) = mnemonic {
                        self.at += 1;
                        Advancing::Mnemonic { before, name, environment }
                    } else {
                        let count = self.expr()?;
                        self.accept_any(&["LINE", "LINES"]);
                        Advancing::Lines { before, count }
                    });
                }
                let eop_ahead = |p: &Self, i: usize| {
                    let at = usize::from(p.word_at(i) == Some("AT"));
                    matches!(p.word_at(i + at), Some("END-OF-PAGE" | "EOP"))
                };
                let [end_of_page, invalid] = self.phrases_opening(|p, i| eop_ahead(p, i) || p.word_at(i) == Some("INVALID"), &["END-WRITE"], |p| {
                    if p.accept_word("INVALID") {
                        p.accept_word("KEY");
                        return Ok(1);
                    }
                    p.accept_word("AT");
                    p.at += 1;
                    Ok(0)
                })?;
                self.accept_word("END-WRITE");
                Stmt::Write { record, from, advancing, invalid, end_of_page, pos }
            }
            "GO" => {
                self.accept_word("TO");
                if self.peek() == Some(&Tok::Period) {
                    return Ok(Stmt::GoTo { target: None, pos });
                }
                let target = self.proc_name()?;
                if !self.starts_ref() && !self.is_word("DEPENDING") {
                    return Ok(Stmt::GoTo { target: Some(target), pos });
                }
                let mut targets = vec![target];
                while self.starts_ref() && !self.is_word("DEPENDING") {
                    targets.push(self.proc_name()?);
                }
                self.expect_word("DEPENDING")?;
                self.accept_word("ON");
                Stmt::GoToDepending { targets, on: self.reference()?, pos }
            }
            "ALTER" => {
                let mut pairs = Vec::new();
                loop {
                    let paragraph = self.proc_name()?;
                    self.expect_word("TO")?;
                    if self.accept_word("PROCEED") {
                        self.expect_word("TO")?;
                    }
                    pairs.push((paragraph, self.proc_name()?));
                    if !self.starts_ref() {
                        break;
                    }
                }
                Stmt::Alter { pairs, pos }
            }
            "ENTRY" => {
                let Some(Tok::Alnum(name)) = self.peek().cloned() else {
                    return Err(self.error("an alphanumeric literal naming the entry point"));
                };
                self.at += 1;
                let using = if self.accept_word("USING") { self.parameters()? } else { Vec::new() };
                Stmt::Entry { name: name.to_ascii_uppercase(), using, pos }
            }
            "EVALUATE" => self.evaluate(pos)?,
            "INITIATE" | "GENERATE" | "TERMINATE" | "SUPPRESS" => Stmt::Report(Box::new(self.report_statement(&verb, pos)?)),
            "GOBACK" => Stmt::Goback { pos },
            "STOP" => {
                self.expect_word("RUN")?;
                Stmt::StopRun { pos }
            }
            "CONTINUE" => Stmt::Continue,
            "EXIT" => match self.accept_any(&["PROGRAM", "PARAGRAPH", "SECTION", "PERFORM", "METHOD"]).as_deref() {
                Some("PROGRAM") => Stmt::ExitProgram { pos },
                Some("METHOD") => Stmt::ExitMethod { pos },
                Some("PARAGRAPH") => Stmt::Exit { kind: ExitKind::Paragraph, pos },
                Some("SECTION") => Stmt::Exit { kind: ExitKind::Section, pos },
                Some(_) if self.accept_word("CYCLE") => Stmt::Exit { kind: ExitKind::PerformCycle, pos },
                Some(_) => Stmt::Exit { kind: ExitKind::Perform, pos },
                None => Stmt::Exit { kind: ExitKind::Plain, pos },
            },
            other => return Err(Error::at(pos, format!("{other} is not a statement ironwork for COBOL supports yet"))),
        })
    }

    fn size_error(&mut self) -> R<Option<SizeError>> {
        let [h] = self.on_phrases(&["ON", "SIZE"], &["END-COMPUTE", "END-ADD", "END-SUBTRACT", "END-MULTIPLY", "END-DIVIDE"], |p| {
            p.accept_word("ON");
            p.expect_word("SIZE")?;
            p.expect_word("ERROR").map(|()| 0)
        })?;
        Ok((h.on.is_some() || h.not_on.is_some()).then(|| SizeError { on: h.on.unwrap_or_default(), not_on: h.not_on.unwrap_or_default() }))
    }

    fn targets(&mut self) -> R<Vec<Target>> {
        let mut out = Vec::new();
        while self.starts_ref() {
            let r = self.reference()?;
            out.push(Target { r, rounded: self.accept_word("ROUNDED") });
        }
        if out.is_empty() {
            return Err(self.error("a receiving item"));
        }
        Ok(out)
    }

    fn refs(&mut self) -> R<Vec<Ref>> {
        let mut out = Vec::new();
        while self.starts_ref() {
            out.push(self.reference()?);
        }
        if out.is_empty() {
            return Err(self.error("a data name"));
        }
        Ok(out)
    }

    fn operands_until(&mut self, stops: &[&str]) -> R<Vec<Expr>> {
        let mut out = Vec::new();
        while self.starts_operand() && !self.word().is_some_and(|w| stops.contains(&w)) {
            out.push(Expr::Operand(self.operand()?));
        }
        if out.is_empty() {
            return Err(self.error("an operand"));
        }
        Ok(out)
    }

    fn arith(&mut self, verb: &str, pos: Pos) -> R<Arith> {
        let sum = |mut es: Vec<Expr>| {
            let first = es.remove(0);
            es.into_iter().fold(first, |acc, e| Expr::Bin(Box::new(acc), BinOp::Add, Box::new(e)))
        };
        let of = |t: &Target| Expr::Operand(Operand::Ref(t.r.clone()));
        let bin = |a: Expr, op: BinOp, b: Expr| Expr::Bin(Box::new(a), op, Box::new(b));
        let mut remainder = None;
        let (verb, computations) = match verb {
            "ADD" => {
                let addends = sum(self.operands_until(&["TO", "GIVING"])?);
                if self.accept_word("TO") {
                    if self.words_ahead_include("GIVING") {
                        let to = sum(self.operands_until(&["GIVING"])?);
                        self.expect_word("GIVING")?;
                        let targets = self.targets()?;
                        let total = bin(addends, BinOp::Add, to);
                        (ArithVerb::Add, targets.into_iter().map(|t| (t, total.clone())).collect())
                    } else {
                        let targets = self.targets()?;
                        (ArithVerb::Add, targets.into_iter().map(|t| (t.clone(), bin(of(&t), BinOp::Add, addends.clone()))).collect())
                    }
                } else {
                    self.expect_word("GIVING")?;
                    let targets = self.targets()?;
                    (ArithVerb::Add, targets.into_iter().map(|t| (t, addends.clone())).collect())
                }
            }
            "SUBTRACT" => {
                let subtrahend = sum(self.operands_until(&["FROM"])?);
                self.expect_word("FROM")?;
                if self.words_ahead_include("GIVING") {
                    let minuend = sum(self.operands_until(&["GIVING"])?);
                    self.expect_word("GIVING")?;
                    let targets = self.targets()?;
                    let diff = bin(minuend, BinOp::Sub, subtrahend);
                    (ArithVerb::Subtract, targets.into_iter().map(|t| (t, diff.clone())).collect())
                } else {
                    let targets = self.targets()?;
                    (ArithVerb::Subtract, targets.into_iter().map(|t| (t.clone(), bin(of(&t), BinOp::Sub, subtrahend.clone()))).collect())
                }
            }
            "MULTIPLY" => {
                let a = Expr::Operand(self.operand()?);
                self.expect_word("BY")?;
                if self.words_ahead_include("GIVING") {
                    let b = Expr::Operand(self.operand()?);
                    self.expect_word("GIVING")?;
                    let targets = self.targets()?;
                    (ArithVerb::Multiply, targets.into_iter().map(|t| (t, bin(a.clone(), BinOp::Mul, b.clone()))).collect())
                } else {
                    let targets = self.targets()?;
                    (ArithVerb::Multiply, targets.into_iter().map(|t| (t.clone(), bin(a.clone(), BinOp::Mul, of(&t)))).collect())
                }
            }
            _ => {
                let first = Expr::Operand(self.operand()?);
                let into = match self.accept_any(&["INTO", "BY"]).as_deref() {
                    Some("INTO") => true,
                    Some(_) => false,
                    None => return Err(self.error("INTO or BY")),
                };
                if !into || self.words_ahead_include("GIVING") {
                    let second = Expr::Operand(self.operand()?);
                    let (dividend, divisor) = if into { (second, first) } else { (first, second) };
                    self.expect_word("GIVING")?;
                    let targets = self.targets()?;
                    if self.accept_word("REMAINDER") {
                        let r = self.reference()?;
                        remainder = Some((Target { r, rounded: false }, dividend.clone(), divisor.clone()));
                    }
                    (ArithVerb::Divide, targets.into_iter().map(|t| (t, bin(dividend.clone(), BinOp::Div, divisor.clone()))).collect())
                } else {
                    let targets = self.targets()?;
                    (ArithVerb::Divide, targets.into_iter().map(|t| (t.clone(), bin(of(&t), BinOp::Div, first.clone()))).collect())
                }
            }
        };
        let size_error = self.size_error()?;
        self.accept_any(&["END-ADD", "END-SUBTRACT", "END-MULTIPLY", "END-DIVIDE"]);
        Ok(Arith { verb, computations, remainder, size_error, pos })
    }

    /// Whether `word` appears before the statement ends.
    fn words_ahead_include(&self, word: &str) -> bool {
        self.tokens[self.at..]
            .iter()
            .take_while(|t| t.tok != Tok::Period && !matches!(&t.tok, Tok::Word(w) if VERBS.contains(&w.as_str()) && w != word))
            .any(|t| matches!(&t.tok, Tok::Word(w) if w == word))
    }

    fn perform(&mut self, pos: Pos) -> R<Stmt> {
        let named = self.word().is_some_and(|w| !VERBS.contains(&w) && !PHRASE_WORDS.contains(&w))
            && self.word_at(1) != Some("TIMES")
            && !matches!(self.peek(), Some(Tok::Number(_)));
        if named {
            let from = self.proc_name()?;
            let thru = if self.accept_any(&["THRU", "THROUGH"]).is_some() { Some(self.proc_name()?) } else { None };
            let repeat = self.repeat()?;
            return Ok(Stmt::PerformProc { from, thru, repeat, pos });
        }
        let repeat = self.repeat()?;
        if matches!(&repeat, Loop::Varying { after, .. } if !after.is_empty()) {
            return Err(Error::at(pos, "an inline PERFORM cannot have AFTER phrases: Enterprise COBOL takes them only when PERFORM names a procedure"));
        }
        let body = self.block(&["END-PERFORM"])?;
        self.expect_word("END-PERFORM")?;
        Ok(Stmt::PerformInline { body, repeat, pos })
    }

    /// ON and NOT ON phrases, each opening with one of `starts`; `head` reads its words and picks the handlers it fills.
    fn on_phrases<const N: usize>(&mut self, starts: &[&str], ends: &[&str], head: impl Fn(&mut Self) -> R<usize>) -> R<[Handlers; N]> {
        self.phrases_opening(|p, i| p.word_at(i).is_some_and(|w| starts.contains(&w)), ends, head)
    }

    /// ON and NOT ON phrases, each where `opens` finds one `i` words ahead.
    fn phrases_opening<const N: usize>(&mut self, opens: impl Fn(&Self, usize) -> bool, ends: &[&str], head: impl Fn(&mut Self) -> R<usize>) -> R<[Handlers; N]> {
        let stops = [&["NOT"][..], ends].concat();
        let mut handlers = std::array::from_fn(|_| Handlers::default());
        loop {
            let negated = self.is_word("NOT") && opens(self, 1);
            if !negated && !opens(self, 0) {
                return Ok(handlers);
            }
            self.at += usize::from(negated);
            let h = &mut handlers[head(self)?];
            let body = self.block(&stops)?;
            if negated { h.not_on = Some(body) } else { h.on = Some(body) }
        }
    }

    /// INVALID KEY and NOT INVALID KEY phrases, then the scope terminator.
    fn invalid_key(&mut self, end: &str) -> R<Handlers> {
        let [h] = self.on_phrases(&["INVALID"], &[end], |p| {
            p.at += 1;
            p.accept_word("KEY");
            Ok(0)
        })?;
        self.accept_word(end);
        Ok(h)
    }

    fn call(&mut self, pos: Pos) -> R<Call> {
        let target = self.operand()?;
        let mut using = Vec::new();
        if self.accept_word("USING") {
            let mut mode = ArgMode::Reference;
            loop {
                if self.accept_word("BY") {
                    mode = match self.accept_any(&["REFERENCE", "CONTENT", "VALUE"]).as_deref() {
                        Some("CONTENT") => ArgMode::Content,
                        Some("VALUE") => ArgMode::Value,
                        Some(_) => ArgMode::Reference,
                        None => return Err(self.error("REFERENCE, CONTENT or VALUE after BY")),
                    };
                } else if self.accept_word("OMITTED") {
                    using.push(Arg { mode, value: None });
                } else if self.starts_operand() {
                    using.push(Arg { mode, value: Some(self.operand()?) });
                } else {
                    break;
                }
            }
        }
        let returning = if self.accept_word("RETURNING") { Some(self.reference()?) } else { None };
        let [exception] = self.on_phrases(&["ON", "EXCEPTION", "OVERFLOW"], &["END-CALL"], |p| {
            p.accept_word("ON");
            p.accept_any(&["EXCEPTION", "OVERFLOW"]).map(|_| 0).ok_or_else(|| p.error("EXCEPTION or OVERFLOW"))
        })?;
        self.accept_word("END-CALL");
        Ok(Call { target, using, returning, on_exception: exception.on, not_on_exception: exception.not_on, pos })
    }

    /// ON OVERFLOW and NOT ON OVERFLOW phrases, in either order.
    fn overflow(&mut self, end: &str) -> R<Handlers> {
        let [h] = self.on_phrases(&["ON", "OVERFLOW"], &[end], |p| {
            p.accept_word("ON");
            p.expect_word("OVERFLOW").map(|()| 0)
        })?;
        self.accept_word(end);
        Ok(h)
    }

    fn string(&mut self, pos: Pos) -> R<StringStmt> {
        let mut sources = Vec::new();
        while !self.is_word("INTO") {
            let mut group = Vec::new();
            while self.starts_operand() && !self.is_word("DELIMITED") {
                group.push(self.operand()?);
            }
            if group.is_empty() {
                return Err(self.error("a sending item"));
            }
            let delimiter = if self.accept_word("DELIMITED") {
                self.accept_word("BY");
                if self.accept_word("SIZE") { Delimiter::Size } else { Delimiter::By(self.operand()?) }
            } else {
                Delimiter::Size
            };
            sources.extend(group.into_iter().map(|op| (op, delimiter.clone())));
        }
        self.expect_word("INTO")?;
        let into = self.reference()?;
        let pointer = if self.accept_word("WITH") || self.is_word("POINTER") {
            self.expect_word("POINTER")?;
            Some(self.reference()?)
        } else {
            None
        };
        let Handlers { on: on_overflow, not_on: not_on_overflow } = self.overflow("END-STRING")?;
        Ok(StringStmt { sources, into, pointer, on_overflow, not_on_overflow, pos })
    }

    fn unstring(&mut self, pos: Pos) -> R<Unstring> {
        let source = self.reference()?;
        let mut delimiters = Vec::new();
        if self.accept_word("DELIMITED") {
            self.accept_word("BY");
            loop {
                let all = self.accept_word("ALL");
                delimiters.push((all, self.operand()?));
                if !self.accept_word("OR") {
                    break;
                }
            }
        }
        self.expect_word("INTO")?;
        let mut into = Vec::new();
        while self.starts_ref() {
            let target = self.reference()?;
            let delimiter_in = if self.accept_word("DELIMITER") {
                self.accept_word("IN");
                Some(self.reference()?)
            } else {
                None
            };
            let count_in = if self.accept_word("COUNT") {
                self.accept_word("IN");
                Some(self.reference()?)
            } else {
                None
            };
            into.push(UnstringInto { target, delimiter_in, count_in });
        }
        if into.is_empty() {
            return Err(self.error("a receiving item after INTO"));
        }
        let pointer = if self.accept_word("WITH") || self.is_word("POINTER") {
            self.expect_word("POINTER")?;
            Some(self.reference()?)
        } else {
            None
        };
        let tallying = if self.accept_word("TALLYING") {
            self.accept_word("IN");
            Some(self.reference()?)
        } else {
            None
        };
        let Handlers { on: on_overflow, not_on: not_on_overflow } = self.overflow("END-UNSTRING")?;
        Ok(Unstring { source, delimiters, into, pointer, tallying, on_overflow, not_on_overflow, pos })
    }

    fn bounds(&mut self) -> R<Vec<Bound>> {
        let mut bounds = Vec::new();
        while let Some(side) = self.accept_any(&["BEFORE", "AFTER"]) {
            self.accept_word("INITIAL");
            bounds.push(Bound { after: side == "AFTER", value: self.operand()? });
        }
        Ok(bounds)
    }

    fn inspect(&mut self, pos: Pos) -> R<Inspect> {
        let target = self.reference()?;
        let (mut tallying, mut replacing, mut converting) = (Vec::new(), Vec::new(), None);
        if self.accept_word("TALLYING") {
            while self.starts_ref() && self.word_at(1) == Some("FOR") || self.starts_ref() && !self.is_word("REPLACING") && self.tally_counter_ahead() {
                let counter = self.reference()?;
                self.expect_word("FOR")?;
                loop {
                    if self.accept_word("CHARACTERS") {
                        tallying.push(InspectPhrase { mode: InspectMode::Characters, pattern: None, by: None, counter: Some(counter.clone()), bounds: self.bounds()? });
                    } else if let Some(mode) = self.accept_any(&["ALL", "LEADING"]) {
                        let mode = if mode == "ALL" { InspectMode::All } else { InspectMode::Leading };
                        loop {
                            let pattern = self.operand()?;
                            tallying.push(InspectPhrase { mode, pattern: Some(pattern), by: None, counter: Some(counter.clone()), bounds: self.bounds()? });
                            if !self.starts_operand() || self.word_at(1) == Some("FOR") || self.is_word("ALL") || self.is_word("LEADING") {
                                break;
                            }
                        }
                    } else {
                        break;
                    }
                }
            }
        }
        if self.accept_word("REPLACING") {
            loop {
                if self.accept_word("CHARACTERS") {
                    self.expect_word("BY")?;
                    let by = self.operand()?;
                    replacing.push(InspectPhrase { mode: InspectMode::Characters, pattern: None, by: Some(by), counter: None, bounds: self.bounds()? });
                } else if let Some(mode) = self.accept_any(&["ALL", "LEADING", "FIRST"]) {
                    let mode = match mode.as_str() {
                        "ALL" => InspectMode::All,
                        "LEADING" => InspectMode::Leading,
                        _ => InspectMode::First,
                    };
                    loop {
                        let pattern = self.operand()?;
                        self.expect_word("BY")?;
                        let by = self.operand()?;
                        replacing.push(InspectPhrase { mode, pattern: Some(pattern), by: Some(by), counter: None, bounds: self.bounds()? });
                        if !self.starts_operand() || self.is_word("ALL") || self.is_word("LEADING") || self.is_word("FIRST") {
                            break;
                        }
                    }
                } else {
                    break;
                }
            }
        }
        if self.accept_word("CONVERTING") {
            let from = self.operand()?;
            self.expect_word("TO")?;
            let to = self.operand()?;
            converting = Some((from, to, self.bounds()?));
        }
        if tallying.is_empty() && replacing.is_empty() && converting.is_none() {
            return Err(self.error("TALLYING, REPLACING or CONVERTING"));
        }
        Ok(Inspect { target, tallying, replacing, converting, pos })
    }

    /// Whether a counter and FOR follow, past any subscripts on the counter.
    fn tally_counter_ahead(&self) -> bool {
        let mut i = self.at + 1;
        if self.tokens.get(i).map(|t| &t.tok) == Some(&Tok::LParen) {
            let mut depth = 0;
            while let Some(t) = self.tokens.get(i) {
                match t.tok {
                    Tok::LParen => depth += 1,
                    Tok::RParen => {
                        depth -= 1;
                        if depth == 0 {
                            i += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
        }
        matches!(self.tokens.get(i).map(|t| &t.tok), Some(Tok::Word(w)) if w == "FOR")
    }

    fn search(&mut self, pos: Pos) -> R<Search> {
        let all = self.accept_word("ALL");
        let table = self.reference()?;
        let varying = if self.accept_word("VARYING") { Some(self.reference()?) } else { None };
        let at_end = if self.accept_word("AT") || self.is_word("END") {
            self.expect_word("END")?;
            Some(self.block(&["WHEN"])?)
        } else {
            None
        };
        let mut whens = Vec::new();
        while self.accept_word("WHEN") {
            let cond = self.cond()?;
            let body = self.block(&["WHEN", "END-SEARCH"])?;
            whens.push((cond, body));
        }
        if whens.is_empty() {
            return Err(self.error("WHEN"));
        }
        self.accept_word("END-SEARCH");
        Ok(Search { table, all, varying, at_end, whens, pos })
    }

    fn set(&mut self) -> R<SetStmt> {
        if self.is_word("ENVIRONMENT") {
            return Err(Error::at(self.pos(), "SET ENVIRONMENT is GnuCOBOL's, not Enterprise COBOL's"));
        }
        if self.is_word("ADDRESS") && self.word_at(1) == Some("OF") {
            let mut targets = Vec::new();
            while self.is_word("ADDRESS") && self.word_at(1) == Some("OF") {
                self.at += 2;
                targets.push(self.reference()?);
            }
            self.expect_word("TO")?;
            return Ok(SetStmt::AddressOf { targets, value: self.operand()? });
        }
        let targets = self.refs()?;
        if self.accept_word("TO") {
            if self.accept_word("TRUE") {
                return Ok(SetStmt::ConditionTrue(targets));
            }
            if self.accept_word("FALSE") {
                return Ok(SetStmt::ConditionFalse(targets));
            }
            return Ok(SetStmt::To { targets, value: self.operand()? });
        }
        match self.accept_any(&["UP", "DOWN"]).as_deref() {
            Some(direction) => {
                self.expect_word("BY")?;
                Ok(SetStmt::UpDown { targets, down: direction == "DOWN", by: self.expr()? })
            }
            None => Err(self.error("TO, UP BY or DOWN BY")),
        }
    }

    fn proc_name(&mut self) -> R<ProcName> {
        let name = self.name("a procedure name")?;
        let section = if self.accept_any(&["OF", "IN"]).is_some() { Some(self.name("a section name")?) } else { None };
        Ok(ProcName { name, section })
    }

    fn evaluate(&mut self, pos: Pos) -> R<Stmt> {
        let mut subjects = vec![self.subject()?];
        while self.accept_word("ALSO") {
            subjects.push(self.subject()?);
        }
        let (mut whens, mut other) = (Vec::new(), Vec::new());
        while self.is_word("WHEN") {
            if self.word_at(1) == Some("OTHER") {
                self.at += 2;
                other = self.block(&["END-EVALUATE"])?;
                break;
            }
            let mut alternatives = Vec::new();
            while self.is_word("WHEN") && self.word_at(1) != Some("OTHER") {
                self.at += 1;
                let mut objects = Vec::new();
                for (k, subject) in subjects.iter().enumerate() {
                    if k > 0 {
                        self.expect_word("ALSO")?;
                    }
                    objects.push(self.object(subject)?);
                }
                alternatives.push(objects);
            }
            let body = self.block(&["WHEN", "END-EVALUATE"])?;
            whens.push(When { alternatives, body });
        }
        self.accept_word("END-EVALUATE");
        Ok(Stmt::Evaluate { subjects, whens, other, pos })
    }

    fn subject(&mut self) -> R<Subject> {
        if let Some(b) = self.accept_any(&["TRUE", "FALSE"]) {
            return Ok(Subject::Bool(b == "TRUE"));
        }
        let save = self.at;
        self.expr()?;
        let conditional = self.relop_ahead(0) || self.is_word("IS") || self.is_word("NOT")
            || self.word().is_some_and(|w| matches!(w, "NUMERIC" | "ALPHABETIC" | "POSITIVE" | "NEGATIVE" | "ZERO"));
        self.at = save;
        Ok(if conditional { Subject::Cond(self.cond()?) } else { Subject::Expr(self.expr()?) })
    }

    fn object(&mut self, subject: &Subject) -> R<Object> {
        if self.accept_word("ANY") {
            return Ok(Object::Any);
        }
        if let Some(b) = self.accept_any(&["TRUE", "FALSE"]) {
            return Ok(Object::Bool(b == "TRUE"));
        }
        if !matches!(subject, Subject::Expr(_)) {
            return Ok(Object::Cond(self.cond()?));
        }
        let not = self.accept_word("NOT");
        let from = self.expr()?;
        let thru = if self.accept_any(&["THRU", "THROUGH"]).is_some() { Some(self.expr()?) } else { None };
        Ok(Object::Value { not, from, thru })
    }

    fn repeat(&mut self) -> R<Loop> {
        let mut test_after = false;
        if self.accept_word("WITH") || self.is_word("TEST") {
            self.expect_word("TEST")?;
            test_after = self.accept_any(&["BEFORE", "AFTER"]).as_deref() == Some("AFTER");
        }
        if self.accept_word("UNTIL") {
            return Ok(Loop::Until { cond: self.cond()?, test_after });
        }
        if self.accept_word("VARYING") {
            let varying = Box::new(self.varying()?);
            let mut after = Vec::new();
            while self.is_word("AFTER") {
                if after.len() == 6 {
                    return Err(self.error("the end of the PERFORM: Enterprise COBOL takes at most six AFTER phrases"));
                }
                self.at += 1;
                after.push(self.varying()?);
            }
            return Ok(Loop::Varying { varying, after, test_after });
        }
        if self.starts_operand() && self.word_at(1) == Some("TIMES") || matches!(self.peek(), Some(Tok::Number(_))) {
            let count = self.expr()?;
            self.expect_word("TIMES")?;
            return Ok(Loop::Times(count));
        }
        Ok(Loop::Once)
    }

    /// One VARYING or AFTER phrase, after its keyword.
    fn varying(&mut self) -> R<Varying> {
        let var = self.reference()?;
        self.expect_word("FROM")?;
        let from = self.expr()?;
        self.expect_word("BY")?;
        let by = self.expr()?;
        self.expect_word("UNTIL")?;
        Ok(Varying { var, from, by, until: self.cond()? })
    }

    fn starts_ref(&self) -> bool {
        self.word().is_some_and(|w| !VERBS.contains(&w) && !PHRASE_WORDS.contains(&w) && figurative(w).is_none() && w != "FUNCTION")
            && !self.paragraph_header()
    }

    fn starts_operand(&self) -> bool {
        match self.peek() {
            Some(Tok::Alnum(_) | Tok::Hex(_) | Tok::National(_) | Tok::Number(_)) => true,
            Some(Tok::Word(w)) => {
                (figurative(w).is_some() || matches!(w.as_str(), "ALL" | "FUNCTION" | "LENGTH" | "ADDRESS" | "DFHRESP" | "DFHVALUE") || self.starts_ref()) && !self.paragraph_header()
            }
            _ => false,
        }
    }

    fn operand(&mut self) -> R<Operand> {
        let pos = self.pos();
        match self.peek() {
            Some(Tok::Word(w)) if w == "FUNCTION" => {
                self.at += 1;
                let name = self.name("a function name")?;
                let (mut args, mut modifier, mut all_subscripts) = (Vec::new(), None, Vec::new());
                if self.peek() == Some(&Tok::LParen) && !self.refmod_ahead() {
                    self.at += 1;
                    while !self.accept(&Tok::RParen) {
                        if let Some(m) = self.accept_any(&["LEADING", "TRAILING"]) {
                            modifier = Some(m);
                            continue;
                        }
                        if self.all_subscript_ahead() {
                            let (table, all) = self.table_with_all()?;
                            all_subscripts.push((args.len(), all));
                            args.push(Expr::Operand(Operand::Ref(table)));
                            continue;
                        }
                        args.push(self.expr()?);
                    }
                }
                let refmod = self.refmod()?;
                Ok(Operand::Function(FunctionCall { name, args, modifier, refmod, all_subscripts, pos }))
            }
            Some(Tok::Word(w)) if w == "LENGTH" && self.word_at(1) == Some("OF") => {
                self.at += 2;
                Ok(Operand::LengthOf(self.reference()?))
            }
            Some(Tok::Word(w)) if w == "DFHRESP" && self.peek_at(1) == Some(&Tok::LParen) => {
                self.at += 2;
                let condition = self.name("a CICS condition")?;
                self.expect(&Tok::RParen, "')'")?;
                let code = crate::system::resp_code(&condition).ok_or_else(|| Error::at(pos, format!("DFHRESP({condition}): not a CICS condition ironwork for COBOL knows")))?;
                Ok(Operand::Literal(Literal::Number(code.to_string())))
            }
            Some(Tok::Word(w)) if w == "DFHVALUE" && self.peek_at(1) == Some(&Tok::LParen) => {
                self.at += 2;
                let name = self.name("a CVDA value")?;
                self.expect(&Tok::RParen, "')'")?;
                let value = rt::cics_tables::cvda(&name).ok_or_else(|| Error::at(pos, format!("DFHVALUE({name}): not a CVDA ironwork for COBOL knows")))?;
                Ok(Operand::Literal(Literal::Number(value.to_string())))
            }
            Some(Tok::Word(w)) if w == "ADDRESS" && self.word_at(1) == Some("OF") => {
                self.at += 2;
                Ok(Operand::AddressOf(self.reference()?))
            }
            Some(Tok::Word(w)) if figurative(w).is_some() || w == "ALL" => Ok(Operand::Literal(self.literal()?)),
            Some(Tok::Word(_)) => Ok(Operand::Ref(self.reference()?)),
            Some(Tok::Alnum(_) | Tok::Hex(_) | Tok::National(_) | Tok::Number(_)) => Ok(Operand::Literal(self.literal()?)),
            _ => Err(self.error("an operand")),
        }
    }

    /// Whether the parenthesis at the cursor opens a reference modification `(start:length)`.
    fn refmod_ahead(&self) -> bool {
        let mut depth = 0;
        for t in &self.tokens[self.at..] {
            match t.tok {
                Tok::LParen => depth += 1,
                Tok::RParen => {
                    depth -= 1;
                    if depth == 0 {
                        return false;
                    }
                }
                Tok::Colon if depth == 1 => return true,
                Tok::Period => return false,
                _ => {}
            }
        }
        false
    }

    fn refmod(&mut self) -> R<Option<RefMod>> {
        if self.peek() != Some(&Tok::LParen) || !self.refmod_ahead() {
            return Ok(None);
        }
        self.at += 1;
        let start = Box::new(self.expr()?);
        self.expect(&Tok::Colon, "':'")?;
        let length = if self.peek() == Some(&Tok::RParen) { None } else { Some(Box::new(self.expr()?)) };
        self.expect(&Tok::RParen, "')'")?;
        Ok(Some(RefMod { start, length }))
    }

    /// Whether a reference whose subscripts include the word ALL starts at the cursor.
    fn all_subscript_ahead(&self) -> bool {
        if !self.starts_ref() {
            return false;
        }
        let mut at = self.at + 1;
        while matches!(self.tokens.get(at).map(|t| &t.tok), Some(Tok::Word(w)) if w == "OF" || w == "IN") {
            at += 2;
        }
        if self.tokens.get(at).map(|t| &t.tok) != Some(&Tok::LParen) {
            return false;
        }
        let mut depth = 0;
        for t in &self.tokens[at..] {
            match &t.tok {
                Tok::LParen => depth += 1,
                Tok::RParen if depth == 1 => return false,
                Tok::RParen => depth -= 1,
                Tok::Word(w) if depth == 1 && w == "ALL" => return true,
                Tok::Period => return false,
                _ => {}
            }
        }
        false
    }

    /// A table reference with ALL subscripts (Language Reference SC27-8713-03, pp. 501-502): the
    /// reference, holding 1 for each ALL, and the positions of the ALLs.
    fn table_with_all(&mut self) -> R<(Ref, Vec<usize>)> {
        let pos = self.pos();
        let name = self.name("a data name")?;
        let mut qualifiers = Vec::new();
        while self.accept_any(&["OF", "IN"]).is_some() {
            qualifiers.push(self.name("a qualifier")?);
        }
        self.expect(&Tok::LParen, "'('")?;
        let (mut subscripts, mut all) = (Vec::new(), Vec::new());
        while !self.accept(&Tok::RParen) {
            if self.accept_any(&["ALL"]).is_some() {
                all.push(subscripts.len());
                subscripts.push(Expr::Operand(Operand::Literal(Literal::Number("1".into()))));
            } else {
                subscripts.push(self.expr()?);
            }
        }
        let refmod = self.refmod()?;
        Ok((Ref { name, qualifiers, subscripts, refmod, pos }, all))
    }

    fn reference(&mut self) -> R<Ref> {
        let pos = self.pos();
        let name = self.name("a data name")?;
        let mut qualifiers = Vec::new();
        while self.accept_any(&["OF", "IN"]).is_some() {
            qualifiers.push(self.name("a qualifier")?);
        }
        let mut subscripts = Vec::new();
        if self.peek() == Some(&Tok::LParen) && !self.refmod_ahead() {
            self.at += 1;
            while !self.accept(&Tok::RParen) {
                subscripts.push(self.expr()?);
            }
        }
        let refmod = self.refmod()?;
        Ok(Ref { name, qualifiers, subscripts, refmod, pos })
    }

    fn expr(&mut self) -> R<Expr> {
        let mut left = self.term()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Plus) => BinOp::Add,
                Some(Tok::Minus) => BinOp::Sub,
                _ => return Ok(left),
            };
            self.at += 1;
            left = Expr::Bin(Box::new(left), op, Box::new(self.term()?));
        }
    }

    fn term(&mut self) -> R<Expr> {
        let mut left = self.power()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Star) => BinOp::Mul,
                Some(Tok::Slash) => BinOp::Div,
                _ => return Ok(left),
            };
            self.at += 1;
            left = Expr::Bin(Box::new(left), op, Box::new(self.power()?));
        }
    }

    fn power(&mut self) -> R<Expr> {
        let mut left = self.unary()?;
        while self.accept(&Tok::Power) {
            left = Expr::Bin(Box::new(left), BinOp::Pow, Box::new(self.unary()?));
        }
        Ok(left)
    }

    fn unary(&mut self) -> R<Expr> {
        if self.accept(&Tok::Minus) {
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        self.accept(&Tok::Plus);
        if self.accept(&Tok::LParen) {
            let e = self.expr()?;
            self.expect(&Tok::RParen, "')'")?;
            return Ok(e);
        }
        Ok(Expr::Operand(self.operand()?))
    }

    fn cond(&mut self) -> R<Cond> {
        let mut last = None;
        self.or_cond(&mut last)
    }

    fn or_cond(&mut self, last: &mut Option<(Expr, RelOp)>) -> R<Cond> {
        let mut left = self.and_cond(last)?;
        while self.accept_word("OR") {
            left = Cond::Or(Box::new(left), Box::new(self.and_cond(last)?));
        }
        Ok(left)
    }

    fn and_cond(&mut self, last: &mut Option<(Expr, RelOp)>) -> R<Cond> {
        let mut left = self.not_cond(last)?;
        while self.accept_word("AND") {
            left = Cond::And(Box::new(left), Box::new(self.not_cond(last)?));
        }
        Ok(left)
    }

    fn not_cond(&mut self, last: &mut Option<(Expr, RelOp)>) -> R<Cond> {
        if self.is_word("NOT") && !self.relop_ahead(1) {
            self.at += 1;
            return Ok(Cond::Not(Box::new(self.not_cond(last)?)));
        }
        self.primary_cond(last)
    }

    fn relop_ahead(&self, ahead: usize) -> bool {
        matches!(self.peek_at(ahead), Some(Tok::Eq | Tok::Lt | Tok::Gt | Tok::Le | Tok::Ge))
            || matches!(self.word_at(ahead), Some("EQUAL" | "GREATER" | "LESS"))
    }

    fn primary_cond(&mut self, last: &mut Option<(Expr, RelOp)>) -> R<Cond> {
        if self.peek() == Some(&Tok::LParen) {
            let save = self.at;
            self.at += 1;
            let mut inner_last = None;
            if let Ok(c) = self.or_cond(&mut inner_last)
                && self.accept(&Tok::RParen)
                && !matches!(self.peek(), Some(Tok::Plus | Tok::Minus | Tok::Star | Tok::Slash | Tok::Power))
                && !self.relop_ahead(0)
                && !self.is_word("IS")
            {
                return Ok(c);
            }
            self.at = save;
        }
        let left = self.expr()?;
        self.accept_word("IS");
        let negated = self.is_word("NOT") && {
            self.at += 1;
            true
        };
        let wrap = |c: Cond| if negated { Cond::Not(Box::new(c)) } else { c };
        if let Some(op) = self.relop()? {
            let right = self.expr()?;
            *last = Some((left.clone(), op));
            return Ok(wrap(Cond::Rel(left, op, right)));
        }
        if let Some(class) = self.accept_any(&["NUMERIC", "ALPHABETIC", "POSITIVE", "NEGATIVE", "ZERO"]) {
            let class = match class.as_str() {
                "NUMERIC" => Class::Numeric,
                "ALPHABETIC" => Class::Alphabetic,
                "POSITIVE" => Class::Positive,
                "NEGATIVE" => Class::Negative,
                _ => Class::Zero,
            };
            return Ok(wrap(Cond::Class(left, class)));
        }
        if negated {
            return Err(self.error("a relational operator or class after NOT"));
        }
        match (left, last.clone()) {
            (Expr::Operand(Operand::Ref(name)), Some((subject, op))) if self.abbreviation_context() => Ok(Cond::NameOrRel { subject, op, name }),
            (right, Some((subject, op))) if !matches!(&right, Expr::Operand(Operand::Ref(_))) => Ok(Cond::Rel(subject, op, right)),
            (Expr::Operand(Operand::Ref(r)), _) => Ok(Cond::Name(r)),
            (_, _) => Err(self.error("a relational operator")),
        }
    }

    /// After AND or OR, an operand with no operator of its own continues an abbreviated relation.
    fn abbreviation_context(&self) -> bool {
        self.at > 1 && matches!(self.tokens.get(self.at.saturating_sub(2)).map(|t| &t.tok), Some(Tok::Word(w)) if w == "OR" || w == "AND")
    }

    fn relop(&mut self) -> R<Option<RelOp>> {
        if self.peek() == Some(&Tok::Lt) && self.peek_at(1) == Some(&Tok::Gt) {
            return Err(Error::at(self.pos(), "<> is not an Enterprise COBOL relational operator: it writes NOT ="));
        }
        let op = match self.peek() {
            Some(Tok::Eq) => RelOp::Eq,
            Some(Tok::Lt) => RelOp::Lt,
            Some(Tok::Gt) => RelOp::Gt,
            Some(Tok::Le) => RelOp::Le,
            Some(Tok::Ge) => RelOp::Ge,
            Some(Tok::Word(w)) if w == "EQUAL" => {
                self.at += 1;
                self.accept_word("TO");
                return Ok(Some(RelOp::Eq));
            }
            Some(Tok::Word(w)) if w == "GREATER" || w == "LESS" => {
                let greater = w == "GREATER";
                self.at += 1;
                self.accept_word("THAN");
                let or_equal = self.accept_word("OR");
                if or_equal {
                    self.expect_word("EQUAL")?;
                    self.accept_word("TO");
                }
                return Ok(Some(match (greater, or_equal) {
                    (true, false) => RelOp::Gt,
                    (true, true) => RelOp::Ge,
                    (false, false) => RelOp::Lt,
                    (false, true) => RelOp::Le,
                }));
            }
            _ => return Ok(None),
        };
        self.at += 1;
        Ok(Some(op))
    }
}

/// SQL host variables: `:NAME`, `:GROUP.NAME` and indicator variables, outside quoted strings.
fn host_variables(sql: &str, pos: Pos) -> Vec<Ref> {
    let chars: Vec<char> = sql.chars().collect();
    let (mut out, mut i, mut quote) = (Vec::new(), 0, None);
    while i < chars.len() {
        let c = chars[i];
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '\'' || c == '"' => quote = Some(c),
            None if c == ':' && chars.get(i + 1).is_some_and(|n| n.is_ascii_alphanumeric()) => {
                let start = i + 1;
                let mut end = start;
                while end < chars.len() && (chars[end].is_ascii_alphanumeric() || matches!(chars[end], '-' | '_' | '.')) {
                    end += 1;
                }
                let path: String = chars[start..end].iter().collect::<String>().to_ascii_uppercase();
                let mut parts: Vec<String> = path.trim_end_matches('.').split('.').map(str::to_owned).collect();
                let name = parts.pop().unwrap_or_default();
                parts.reverse();
                out.push(Ref { name, qualifiers: parts, subscripts: Vec::new(), refmod: None, pos });
                i = end;
                continue;
            }
            None => {}
        }
        i += 1;
    }
    out
}

/// CICS command words and options: `NAME` or `NAME(argument)`, the argument kept as written.
fn cics_options(body: &str) -> Vec<(String, Option<ExecArg>)> {
    let chars: Vec<char> = body.chars().collect();
    let (mut out, mut i) = (Vec::new(), 0);
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '(' {
            i += 1;
        }
        let name: String = chars[start..i].iter().collect::<String>().to_ascii_uppercase();
        let mut j = i;
        while j < chars.len() && chars[j].is_whitespace() {
            j += 1;
        }
        if j < chars.len() && chars[j] == '(' {
            let (mut depth, mut quote, mut k) = (0, None, j);
            while k < chars.len() {
                match (quote, chars[k]) {
                    (Some(q), c) if c == q => quote = None,
                    (Some(_), _) => {}
                    (None, '\'' | '"') => quote = Some(chars[k]),
                    (None, '(') => depth += 1,
                    (None, ')') => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                k += 1;
            }
            let arg: String = chars[j + 1..k.min(chars.len())].iter().collect();
            out.push((name, Some(ExecArg::Text(arg.trim().to_owned()))));
            i = (k + 1).min(chars.len());
        } else {
            out.push((name, None));
        }
    }
    out
}

/// An argument as the COBOL operand it names, through ironwork's own lexer and parser.
fn operand_of(text: &str, pos: Pos) -> Option<Operand> {
    let source = crate::source::Source { text: text.to_owned(), positions: vec![pos; text.chars().count()], options: Vec::new(), debugging: None };
    let tokens = crate::lexer::lex(&source).ok()?;
    let mut p = Parser { tokens: &tokens, at: 0, exec_declarations: Vec::new(), cics: false, sql: SqlState::default(), mnemonics: Vec::new(), debugging: false };
    let op = p.operand().ok()?;
    (p.at == tokens.len()).then_some(op)
}

/// DATA DIVISION entries from the text of a system member.
fn system_entries(member: &str) -> R<Vec<DataEntry>> {
    system_text_entries(&crate::system::member(member).unwrap_or_default())
}

fn system_text_entries(text: &str) -> R<Vec<DataEntry>> {
    let source = crate::source::read(text)?;
    let tokens = crate::lexer::lex(&source)?;
    Parser { tokens: &tokens, at: 0, exec_declarations: Vec::new(), cics: false, sql: SqlState::default(), mnemonics: Vec::new(), debugging: false }.data_entries()
}

/// Words that begin a SELECT clause, and so end the one before.
const SELECT_CLAUSES: &[&str] = &[
    "ASSIGN", "ORGANIZATION", "ACCESS", "FILE", "STATUS", "RECORD", "ALTERNATE", "RELATIVE", "LINE", "SEQUENTIAL", "INDEXED", "RESERVE",
    "PADDING", "LOCK", "SHARING",
];

/// Words that begin an FD clause, and so end the clause or the list of report names before them.
const FD_WORDS: &[&str] = &[
    "RECORDING", "RECORD", "BLOCK", "LABEL", "DATA", "VALUE", "CODE-SET", "LINAGE", "REPORT", "REPORTS", "IS", "EXTERNAL", "GLOBAL", "STYLE",
];

fn is_clause_word(w: &str) -> bool {
    matches!(
        w,
        "PIC" | "PICTURE" | "USAGE" | "VALUE" | "VALUES" | "REDEFINES" | "OCCURS" | "SIGN" | "LEADING" | "TRAILING" | "JUSTIFIED"
            | "JUST" | "SYNC" | "SYNCHRONIZED" | "GLOBAL" | "EXTERNAL" | "BLANK"
    ) || usage_word(w).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(body: &str) -> Program {
        let text = format!(
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n{body}"
        );
        crate::parse(&text).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn dfhresp_and_dfhvalue_fold_to_the_numbers_in_ibms_tables() {
        let body = |operand: &str| format!("       01  A PIC S9(8) COMP.\n       PROCEDURE DIVISION.\n           IF A = {operand}\n               GOBACK\n           END-IF.\n");
        let folds_to = |operand: &str, n: i32| {
            let p = program(&body(operand));
            assert!(format!("{:?}", p.paragraphs[0].statements[0]).contains(&format!("Number(\"{n}\")")), "{operand}");
        };
        folds_to("DFHRESP(NOTFINISHED)", 113);
        folds_to("DFHRESP(DSIDERR)", 12);
        folds_to("DFHRESP(FILENOTFOUND)", 12);
        assert_eq!(rt::cics_tables::cvda("ENABLED"), Some(23));
        folds_to("DFHVALUE(ENABLED)", 23);
        for (operand, message) in [("DFHRESP(NOSUCH)", "DFHRESP(NOSUCH): not a CICS condition"), ("DFHVALUE(NOSUCH)", "DFHVALUE(NOSUCH): not a CVDA")] {
            let err = crate::parse(&format!("       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n{}", body(operand))).unwrap_err();
            assert!(err.to_string().contains(message), "{err}");
        }
    }

    #[test]
    fn an_exec_cics_program_compares_with_dfhresp_notfinished() {
        program("       01  WS-RESP PIC S9(8) COMP.\n       PROCEDURE DIVISION.\n           EXEC CICS RETURN END-EXEC\n           IF WS-RESP = DFHRESP(NOTFINISHED)\n               GOBACK\n           END-IF.\n");
    }

    #[test]
    fn data_entries_with_clauses_in_any_order() {
        let p = program(
            "       01  G.\n           05 A PIC S9(3)V99 COMP-3 VALUE -1.5.\n           05 B REDEFINES A PIC X(3).\n           05 FILLER PIC X(2) VALUE SPACES.\n           05 T PIC 9 OCCURS 3 TIMES.\n       PROCEDURE DIVISION.\n           GOBACK.\n",
        );
        let ws = &p.working_storage;
        assert_eq!(ws.len(), 5);
        assert_eq!((ws[1].picture.as_deref(), ws[1].usage, &ws[1].value), (Some("S9(3)V99"), Some(Usage::Packed), &Some(Literal::Number("-1.5".into()))));
        assert_eq!(ws[2].redefines.as_deref(), Some("A"));
        assert_eq!((ws[3].name.as_deref(), &ws[3].value), (None, &Some(Literal::Figurative(Figurative::Space))));
        assert_eq!(ws[4].occurs, Some(3));
    }

    #[test]
    fn paragraphs_and_nested_if() {
        let p = program(
            "       01  A PIC X.\n       PROCEDURE DIVISION.\n       MAIN-LINE.\n           PERFORM P2\n           GOBACK.\n       P2.\n           IF A < 'B'\n               MOVE 'L' TO A\n           ELSE\n               IF A = 'B'\n                   MOVE 'E' TO A\n               ELSE\n                   MOVE 'G' TO A\n               END-IF\n           END-IF.\n",
        );
        assert_eq!(p.paragraphs.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["MAIN-LINE", "P2"]);
        let Stmt::If { otherwise, .. } = &p.paragraphs[1].statements[0] else { panic!() };
        assert!(matches!(otherwise[0], Stmt::If { .. }));
    }

    #[test]
    fn a_program_names_the_programs_it_directly_contains() {
        let program = |id: &str, inner: &str| format!("       IDENTIFICATION DIVISION.\n       PROGRAM-ID. {id}.\n       PROCEDURE DIVISION.\n           GOBACK.\n{inner}       END PROGRAM {id}.\n");
        let text = program("OUTER", &[program("A", &program("A1", "")), program("B", "")].concat());
        let all = crate::parse_all_with(&text, &Default::default()).unwrap_or_else(|e| panic!("{e}"));
        let contained: Vec<(&str, &[String])> = all.iter().map(|p| (p.id.as_str(), p.nested.as_slice())).collect();
        assert_eq!(contained, [("OUTER", &["A".to_owned(), "B".to_owned()][..]), ("A", &["A1".to_owned()][..]), ("A1", &[][..]), ("B", &[][..])]);
    }

    #[test]
    fn inline_perform_varying_with_a_compound_condition() {
        let p = program(
            "       01  I PIC 9(4) COMP.\n       01  J PIC 9(4) COMP.\n       PROCEDURE DIVISION.\n           PERFORM VARYING J FROM 0 BY 1\n                   UNTIL J > 31 OR I + J > 64\n               CONTINUE\n           END-PERFORM.\n",
        );
        let Stmt::PerformInline { repeat: Loop::Varying { varying, .. }, .. } = &p.paragraphs[0].statements[0] else { panic!() };
        assert!(matches!(varying.until, Cond::Or(..)));
    }

    #[test]
    fn functions_reference_modification_and_length_of() {
        let p = program(
            "       01  X PIC X(9).\n       01  N PIC 9(4) COMP.\n       PROCEDURE DIVISION.\n           COMPUTE N = FUNCTION ORD(X(N + 1:1)) - 1\n           MOVE FUNCTION NATIONAL-OF(X, 1047) TO X\n           MOVE LENGTH OF X TO N.\n",
        );
        let Stmt::Compute { expr: Expr::Bin(left, BinOp::Sub, _), .. } = &p.paragraphs[0].statements[0] else { panic!() };
        let Expr::Operand(Operand::Function(f)) = left.as_ref() else { panic!() };
        let Expr::Operand(Operand::Ref(r)) = &f.args[0] else { panic!() };
        assert!(r.refmod.is_some());
        assert!(matches!(&p.paragraphs[0].statements[2], Stmt::Move { from: Operand::LengthOf(_), .. }));
    }

    #[test]
    fn divide_giving_remainder_and_add_to() {
        let p = program(
            "       01  N PIC 9(4) COMP.\n       01  H PIC 9(4) COMP.\n       01  L PIC 9(4) COMP.\n       PROCEDURE DIVISION.\n           DIVIDE N BY 16 GIVING H REMAINDER L\n           ADD 1 TO H ROUNDED.\n",
        );
        let Stmt::Arith(a) = &p.paragraphs[0].statements[0] else { panic!() };
        assert_eq!(a.verb, ArithVerb::Divide);
        assert!(a.remainder.is_some());
        let Stmt::Arith(add) = &p.paragraphs[0].statements[1] else { panic!() };
        assert!(add.computations[0].0.rounded);
    }

    #[test]
    fn abbreviated_combined_relation() {
        let p = program("       01  A PIC 9.\n       PROCEDURE DIVISION.\n           IF A = 1 OR 2 CONTINUE END-IF.\n");
        let Stmt::If { cond: Cond::Or(_, right), .. } = &p.paragraphs[0].statements[0] else { panic!() };
        assert!(matches!(right.as_ref(), Cond::Rel(_, RelOp::Eq, _)));
    }

    #[test]
    fn indexed_select_clauses_and_keyed_statements() {
        let text = [
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       ENVIRONMENT DIVISION.\n       INPUT-OUTPUT SECTION.\n       FILE-CONTROL.\n",
            "           SELECT K ASSIGN TO KDD ORGANIZATION IS INDEXED\n",
            "               ACCESS MODE IS DYNAMIC RECORD KEY IS K-ID\n",
            "               ALTERNATE KEY K-ALT WITH DUPLICATES.\n",
            "           SELECT N ASSIGN TO NDD STATUS N-FS N-VS ORGANIZATION\n",
            "               INDEXED FILE STATUS IS N-FS RECORD N-ID.\n",
            "       DATA DIVISION.\n       FILE SECTION.\n       FD  K.\n       01  K-REC.\n           05 K-ID PIC XX.\n           05 K-ALT PIC XX.\n",
            "       FD  N.\n       01  N-ID PIC XX.\n       WORKING-STORAGE SECTION.\n       01  N-FS PIC XX.\n       01  N-VS PIC X(6).\n",
            "       PROCEDURE DIVISION.\n",
            "           READ K NEXT RECORD AT END CONTINUE\n",
            "               NOT AT END CONTINUE END-READ\n",
            "           READ K KEY IS K-ALT INVALID KEY CONTINUE END-READ\n",
            "           START K KEY IS NOT LESS THAN K-ID\n",
            "               INVALID KEY CONTINUE NOT INVALID KEY CONTINUE\n",
            "           END-START\n",
            "           REWRITE K-REC INVALID KEY CONTINUE END-REWRITE\n",
            "           DELETE K RECORD END-DELETE.\n",
        ]
        .concat();
        let p = crate::parse(&text).unwrap_or_else(|e| panic!("{e}"));
        let f = &p.files[0];
        assert_eq!((f.organization, f.access), (Organization::Indexed, Access::Dynamic));
        assert_eq!(f.record_key.as_ref().map(|r| r.name.as_str()), Some("K-ID"));
        assert_eq!(f.alternate_keys.iter().map(|(r, d)| (r.name.as_str(), *d)).collect::<Vec<_>>(), [("K-ALT", true)]);
        let n = &p.files[1];
        assert_eq!((n.organization, n.record_key.as_ref().map(|r| r.name.as_str())), (Organization::Indexed, Some("N-ID")));
        let s = &p.paragraphs[0].statements;
        let Stmt::Read(r) = &s[0] else { panic!() };
        assert!(r.next && !r.previous && r.at_end.on.is_some() && r.at_end.not_on.is_some());
        let Stmt::Read(r) = &s[1] else { panic!() };
        assert!(r.key.is_some() && r.invalid.on.is_some() && !r.next);
        let Stmt::Start { key: Some((RelOp::Ge, _)), invalid, .. } = &s[2] else { panic!() };
        assert!(invalid.on.is_some() && invalid.not_on.is_some());
        assert!(matches!(&s[3], Stmt::Rewrite { invalid, .. } if invalid.on.is_some()));
        assert!(matches!(&s[4], Stmt::Delete { .. }));
    }

    #[test]
    fn entry_alter_the_go_to_forms_and_section_priorities() {
        let p = program(
            "       01  D PIC 9.\n       LINKAGE SECTION.\n       01  L PIC X.\n       PROCEDURE DIVISION.\n       S SECTION 50.\n       P1.\n           ENTRY 'Alt' USING BY VALUE L.\n           ALTER P2 TO PROCEED TO P1 P3 TO P1\n           GO TO P1 P2 DEPENDING ON D.\n       P2.\n           GO TO.\n       P3.\n           GO TO P1.\n       T SECTION.\n",
        );
        let priorities: Vec<(&str, u8)> = p.paragraphs.iter().map(|q| (q.name.as_str(), q.priority)).collect();
        assert_eq!(priorities, [("S", 50), ("P1", 50), ("P2", 50), ("P3", 50), ("T", 0)]);
        let s = &p.paragraphs[1].statements;
        assert!(matches!(&s[0], Stmt::Entry { name, using, .. } if name == "ALT" && using == &[Param { by_value: true, name: "L".into() }]));
        assert!(matches!(&s[2], Stmt::Alter { pairs, .. } if pairs.len() == 2 && pairs[1].0.name == "P3"));
        assert!(matches!(&s[3], Stmt::GoToDepending { targets, on, .. } if targets.len() == 2 && on.name == "D"));
        assert!(matches!(&p.paragraphs[2].statements[0], Stmt::GoTo { target: None, .. }));
        assert!(matches!(&p.paragraphs[3].statements[0], Stmt::GoTo { target: Some(t), .. } if t.name == "P1"));
    }

    #[test]
    fn unsupported_statements_are_named() {
        let text = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       PROCEDURE DIVISION.\n           MOVE CORRESPONDING A TO B.\n";
        assert!(crate::parse(text).unwrap_err().message.contains("MOVE CORRESPONDING"));
    }

    #[test]
    fn advancing_mnemonic_names_reach_contained_programs_and_linage_is_noted() {
        let text = [
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. OUTER.\n       ENVIRONMENT DIVISION.\n",
            "       CONFIGURATION SECTION.\n       SPECIAL-NAMES.\n           C01 IS TOP-OF-PAGE CSP NO-SPACE\n",
            "           AFP-5A IS PAGE-MODE UPSI-0 IS SWITCH-0 ON STATUS IS SW-ON.\n",
            "       INPUT-OUTPUT SECTION.\n       FILE-CONTROL.\n           SELECT P ASSIGN TO PDD.\n",
            "       DATA DIVISION.\n       FILE SECTION.\n       FD  P LINAGE IS 60.\n       01  P-REC PIC X.\n",
            "       PROCEDURE DIVISION.\n           WRITE P-REC AFTER TOP-OF-PAGE\n",
            "           WRITE P-REC BEFORE ADVANCING NO-SPACE\n           WRITE P-REC AFTER ADVANCING PAGE-COUNT LINES.\n",
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. INNER.\n       PROCEDURE DIVISION.\n",
            "           WRITE P-REC AFTER ADVANCING PAGE-MODE.\n       END PROGRAM INNER.\n       END PROGRAM OUTER.\n",
        ]
        .concat();
        let programs = crate::parse_all_with(&text, &crate::copy::Libraries::default()).unwrap_or_else(|e| panic!("{e}"));
        let (outer, inner) = (&programs[0], &programs[1]);
        let pairs = |p: &Program| p.environment.mnemonics.iter().map(|(n, e)| format!("{n}={e}")).collect::<Vec<_>>();
        assert_eq!(pairs(outer), ["TOP-OF-PAGE=C01", "NO-SPACE=CSP", "PAGE-MODE=AFP-5A"]);
        assert_eq!(outer.files[0].linage.as_ref().map(|l| &l.lines), Some(&LinageValue::Integer("60".into())));
        let advancing = |p: &Program, i: usize| match &p.paragraphs[0].statements[i] {
            Stmt::Write { advancing: Some(a), .. } => a.clone(),
            other => panic!("{other:?}"),
        };
        assert!(matches!(advancing(outer, 0), Advancing::Mnemonic { before: false, environment, .. } if environment == "C01"));
        assert!(matches!(advancing(outer, 1), Advancing::Mnemonic { before: true, environment, .. } if environment == "CSP"));
        assert!(matches!(advancing(outer, 2), Advancing::Lines { before: false, .. }));
        assert!(matches!(advancing(inner, 0), Advancing::Mnemonic { environment, .. } if environment == "AFP-5A"));
    }

    #[test]
    fn exit_section_opening_a_paragraph_is_a_statement_not_a_section() {
        let p = program("       PROCEDURE DIVISION.\n       MAIN-LINE SECTION.\n       SKIPPED.\n           EXIT SECTION.\n       NEVER.\n           GOBACK.\n");
        let names: Vec<&str> = p.paragraphs.iter().map(|q| q.name.as_str()).collect();
        assert_eq!(names, ["MAIN-LINE", "SKIPPED", "NEVER"]);
        assert!(matches!(p.paragraphs[1].statements[..], [Stmt::Exit { kind: ExitKind::Section, .. }, Stmt::SentenceEnd]), "{:?}", p.paragraphs[1].statements);
    }

    fn linage_program(fds: &str, procedure: &str) -> String {
        [
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       ENVIRONMENT DIVISION.\n       INPUT-OUTPUT SECTION.\n       FILE-CONTROL.\n",
            "           SELECT P ASSIGN TO PDD.\n           SELECT S ASSIGN TO SDD.\n           SELECT Q ASSIGN TO QDD.\n",
            "       DATA DIVISION.\n       FILE SECTION.\n",
            fds,
            "       WORKING-STORAGE SECTION.\n       01  SIZES.\n           05 BODY PIC 99.\n       01  T-M PIC 9.\n       PROCEDURE DIVISION.\n",
            procedure,
        ]
        .concat()
    }

    #[test]
    fn linage_phrases_come_in_any_order_and_an_sds_is_dropped() {
        let fds = [
            "       FD  P LINAGE IS BODY OF SIZES LINES LINES AT BOTTOM 6\n           WITH FOOTING AT 45 TOP T-M.\n       01  P-REC PIC X.\n",
            "       SD  S LINAGE 10.\n       01  S-REC PIC X.\n",
            "       FD  Q LABEL RECORDS STANDARD LINAGE 5 RECORDING MODE F.\n       01  Q-REC PIC X.\n",
        ]
        .concat();
        let p = crate::parse(&linage_program(&fds, "           GOBACK.\n")).unwrap_or_else(|e| panic!("{e}"));
        let l = p.files[0].linage.as_ref().unwrap();
        assert!(matches!(&l.lines, LinageValue::Data(r) if r.name == "BODY" && r.qualifiers == ["SIZES"]));
        assert_eq!((&l.footing, &l.bottom), (&Some(LinageValue::Integer("45".into())), &Some(LinageValue::Integer("6".into()))));
        assert!(matches!(&l.top, Some(LinageValue::Data(r)) if r.name == "T-M"));
        assert_eq!(p.files[1].linage, None);
        assert_eq!((p.files[2].linage.as_ref().map(|l| &l.lines), p.files[2].recording), (Some(&LinageValue::Integer("5".into())), Some('F')));
        for (fd, expected) in [("LINAGE 5 TOP 1 TOP 2", "TOP is given twice"), ("LINAGE 5.5", "not an unsigned integer"), ("LINAGE 5 FOOTING", "after FOOTING")] {
            let text = linage_program(&format!("       FD  P {fd}.\n       01  P-REC PIC X.\n"), "           GOBACK.\n");
            let e = crate::parse(&text).unwrap_err();
            assert!(e.message.contains(expected), "{fd}: {}", e.message);
        }
    }

    #[test]
    fn end_of_page_phrases_leave_a_read_its_not_at_end() {
        let procedure = [
            "           READ Q AT END WRITE P-REC\n",
            "               NOT AT END WRITE P-REC AT EOP CONTINUE END-WRITE\n           END-READ\n",
            "           WRITE P-REC BEFORE ADVANCING 2 LINES END-OF-PAGE CONTINUE\n",
            "               NOT AT END-OF-PAGE CONTINUE\n           END-WRITE\n",
            "           WRITE P-REC INVALID KEY CONTINUE NOT EOP CONTINUE.\n",
        ]
        .concat();
        let fds = "       FD  P LINAGE 5.\n       01  P-REC PIC X.\n       FD  Q.\n       01  Q-REC PIC X.\n";
        let p = crate::parse(&linage_program(fds, &procedure)).unwrap_or_else(|e| panic!("{e}"));
        let s = &p.paragraphs[0].statements;
        let Stmt::Read(r) = &s[0] else { panic!("{:?}", s[0]) };
        let (Some(on), Some(not_on)) = (&r.at_end.on, &r.at_end.not_on) else { panic!("{r:?}") };
        assert!(matches!(&on[0], Stmt::Write { end_of_page, .. } if *end_of_page == Handlers::default()));
        assert!(matches!(&not_on[0], Stmt::Write { end_of_page, .. } if end_of_page.on.is_some() && end_of_page.not_on.is_none()));
        assert!(matches!(&s[1], Stmt::Write { advancing: Some(Advancing::Lines { before: true, .. }), end_of_page, .. } if end_of_page.on.is_some() && end_of_page.not_on.is_some()));
        assert!(matches!(&s[2], Stmt::Write { invalid, end_of_page, .. } if invalid.on.is_some() && end_of_page.not_on.is_some()));
    }

    #[test]
    fn decimal_point_is_comma_and_currency_signs_reach_contained_programs() {
        let text = [
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. OUTER.\n       ENVIRONMENT DIVISION.\n",
            "       CONFIGURATION SECTION.\n       SPECIAL-NAMES.\n           CURRENCY SIGN IS 'W'\n",
            "           CURRENCY 'EUR ' WITH PICTURE SYMBOL 'y'\n           DECIMAL-POINT IS COMMA.\n",
            "       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  A PIC 9V9 VALUE 1,5.\n",
            "       PROCEDURE DIVISION.\n           GOBACK.\n",
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. INNER.\n       PROCEDURE DIVISION.\n           GOBACK.\n",
            "       END PROGRAM INNER.\n       END PROGRAM OUTER.\n",
        ]
        .concat();
        let programs = crate::parse_all_with(&text, &crate::copy::Libraries::default()).unwrap_or_else(|e| panic!("{e}"));
        assert!(programs.iter().all(|p| p.environment.decimal_point_comma));
        assert_eq!(programs[0].working_storage[0].value, Some(Literal::Number("1.5".into())));
        let signs = [CurrencySign { value: "W".into(), symbol: 'W' }, CurrencySign { value: "EUR ".into(), symbol: 'y' }];
        assert!(programs.iter().all(|p| p.environment.currency == signs));
        for (clause, why) in [
            ("'E'", "one character"),
            ("'EUR'", "one character"),
            ("'E9' PICTURE SYMBOL 'Y'", "a digit"),
            ("'EUR' PICTURE SYMBOL 'Z'", "PICTURE SYMBOL"),
            ("X'9F' PICTURE SYMBOL 'Y'", "hexadecimal"),
            ("'EUR' WITH 'Y'", "PICTURE SYMBOL after WITH"),
            ("'W'\n           CURRENCY 'WON' PICTURE SYMBOL 'W'", "a second CURRENCY SIGN"),
        ] {
            let bad = text.replace("CURRENCY SIGN IS 'W'", &format!("CURRENCY SIGN IS {clause}"));
            let message = crate::parse(&bad).unwrap_err().message;
            assert!(message.contains(why), "{clause}: {message}");
        }
    }

    #[test]
    fn a_picture_keeps_the_case_of_a_currency_symbol_and_picture_symbol_is_no_picture_string() {
        let text = concat!(
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n",
            "       SPECIAL-NAMES.\n           CURRENCY SIGN 'CHF ' WITH PICTURE SYMBOL 'f'.\n       DATA DIVISION.\n",
            "       WORKING-STORAGE SECTION.\n       01  A PIC fff9v99.\n       01  B PIC zz9.\n",
        );
        let p = crate::parse(text).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(p.environment.currency, [CurrencySign { value: "CHF ".into(), symbol: 'f' }]);
        assert_eq!((p.working_storage[0].picture.as_deref(), p.working_storage[1].picture.as_deref()), (Some("fff9V99"), Some("ZZ9")));
    }

    #[test]
    fn renames_and_condition_names_with_a_false_value() {
        let p = program(concat!(
            "       01  R.\n           05 A PIC X.\n           05 B PIC X.\n              88 B-ON VALUE 'Y' FALSE 'N'.\n",
            "              88 B-OFF VALUES 'N' 'X' WHEN SET TO FALSE IS SPACE.\n",
            "       66  AB RENAMES A THRU B.\n       66  BB RENAMES B OF R.\n",
            "       PROCEDURE DIVISION.\n           SET B-ON B-OFF TO FALSE.\n",
        ));
        let ws = &p.working_storage;
        assert_eq!((ws[3].false_value.as_ref(), ws[3].condition_values.len()), (Some(&Literal::Alnum("N".into())), 1));
        assert_eq!((ws[4].false_value.as_ref(), ws[4].condition_values.len()), (Some(&Literal::Figurative(Figurative::Space)), 2));
        let (first, last) = ws[5].renames.as_ref().unwrap();
        assert_eq!((ws[5].level, first.name.as_str(), last.as_ref().map(|r| r.name.as_str())), (66, "A", Some("B")));
        assert_eq!(ws[6].renames.as_ref().unwrap().0.qualifiers, ["R"]);
        assert!(matches!(&p.paragraphs[0].statements[0], Stmt::Set { set: SetStmt::ConditionFalse(t), .. } if t.len() == 2));
    }
}
