use crate::messages::{IWS0099, IWX0015};
use crate::source::{FreeSpan, Source};
use crate::{Error, Pos};
use numeric::Compliance;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tok {
    /// A COBOL word, uppercased: a name, a reserved word or a level number with letters in it.
    Word(String),
    /// A numeric literal as written: optional sign, digits, optional decimal point.
    Number(String),
    Alnum(String),
    Hex(Vec<u8>),
    National(String),
    /// A DBCS literal's characters, its shift-out and shift-in removed.
    Dbcs(String),
    Pic(String),
    /// An EXEC ... END-EXEC block, as written: SQL, CICS or DLI for a precompiler.
    Exec(String),
    Period,
    LParen,
    RParen,
    Colon,
    Plus,
    Minus,
    Star,
    Slash,
    Power,
    Eq,
    Lt,
    Gt,
    Le,
    Ge,
    /// `&` after a literal or a word, under `--compliance extended`: [`crate::extended`] joins the
    /// literals on either side before the parser sees it.
    Ampersand,
}

/// The warning for `<>`, which the lexer reads as NOT =.
pub const NOT_EQUAL: &str = "<> (Micro Focus and GnuCOBOL; Enterprise COBOL writes NOT =) is read as NOT =";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub tok: Tok,
    pub pos: Pos,
    /// Starts in area A (columns 8 to 11), where division, section and paragraph headers go.
    pub area_a: bool,
    /// A word as the source spells it, where that is not all capitals.
    pub spelled: Option<String>,
    /// A separator comma or semicolon comes between this token and the one before it.
    pub after_comma: bool,
    /// Messages about the token that do not stop the parse.
    pub messages: Vec<Error>,
}

struct Lexer<'a> {
    chars: Vec<char>,
    positions: &'a [Pos],
    at: usize,
    tokens: Vec<Token>,
    /// DECIMAL-POINT IS COMMA is in force: a comma between digits is the decimal point.
    decimal_comma: bool,
    /// The currency symbols CURRENCY SIGN clauses have named so far, whose case a PICTURE keeps.
    currency: Vec<char>,
    /// For each program begun and not yet ended, whether the comma was the decimal point before
    /// it: a contained program has its container's, and a program after it starts afresh.
    outer: Vec<bool>,
    comma_pending: bool,
    /// Messages for the next token emitted.
    pending: Vec<Error>,
    extended: bool,
    /// The lines read in free form, and whether a token from each has carried its warning.
    free: &'a [FreeSpan],
    warned: Vec<bool>,
    /// NSYMBOL(DBCS) is on the cards: N'...' is a DBCS literal.
    n_is_dbcs: bool,
    /// How a zero-length alphanumeric literal is read.
    empty: numeric::EmptyLiteral,
    /// The words with letters outside COBOL's character set already warned of (IWX0094).
    wide_words: Vec<String>,
}

/// The most characters a DBCS literal holds (Language Reference SC27-8713-03, p. 42).
const DBCS_LITERAL_MAX: usize = 28;

/// The most characters a user-defined word has.
pub(crate) const USER_WORD_MAX: usize = 30;

pub fn lex(source: &Source) -> Result<Vec<Token>, Error> {
    lex_under(source, Compliance::Strict)
}

/// Lexes `source` under `compliance`: `--compliance extended` reads `<>` as NOT = and keeps `&`
/// after a literal or a word for [`crate::extended`].
pub fn lex_under(source: &Source, compliance: Compliance) -> Result<Vec<Token>, Error> {
    lex_with(source, compliance, numeric::EmptyLiteral::Space)
}

/// Lexes as [`lex_under`] does, a zero-length alphanumeric literal read as `empty` says.
pub fn lex_with(source: &Source, compliance: Compliance, empty: numeric::EmptyLiteral) -> Result<Vec<Token>, Error> {
    let mut options = numeric::Options::default();
    for card in &source.options {
        options.apply(card).ok();
    }
    let n_is_dbcs = options.nsymbol == numeric::Nsymbol::Dbcs;
    let mut lx = Lexer {
        chars: source.text.chars().collect(),
        positions: &source.positions,
        at: 0,
        tokens: Vec::new(),
        decimal_comma: false,
        currency: Vec::new(),
        outer: Vec::new(),
        comma_pending: false,
        pending: source.notes.clone(),
        extended: compliance == Compliance::Extended,
        free: &source.free,
        warned: vec![false; source.free.len()],
        n_is_dbcs,
        empty,
        wide_words: Vec::new(),
    };
    while lx.at < lx.chars.len() {
        lx.next_token()?;
    }
    Ok(lx.tokens)
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

/// A single-byte character outside IBM's basic COBOL character set (Language Reference
/// SC27-8713-03, Table 1, pp. 3-6), which IBM accepts with an error as part of the text around it
/// (assumption C123). `$` and `&`, which ironwork reads only where COBOL puts them, are left out.
/// The lowercase letter an EBCDIC byte is, the same in every code page ironwork carries.
fn ebcdic_lowercase(byte: u8) -> Option<char> {
    let (start, first) = match byte {
        0x81..=0x89 => (0x81, b'a'),
        0x91..=0x99 => (0x91, b'j'),
        0xA2..=0xA9 => (0xA2, b's'),
        _ => return None,
    };
    Some(char::from(first + (byte - start)))
}

fn non_cobol(c: char) -> bool {
    u32::from(c) <= 0xFF && !c.is_ascii_alphanumeric() && !" \n+-*/=$,;.\"'()><:_&".contains(c)
}

/// A letter beyond ASCII, such as ñ, é, a kanji or kana, which `--compliance extended` reads in a
/// user-defined word (IWX0094).
fn wide_letter(c: char) -> bool {
    !c.is_ascii() && c.is_alphabetic()
}

impl Lexer<'_> {
    fn peek(&self, ahead: usize) -> Option<char> {
        self.chars.get(self.at + ahead).copied()
    }

    fn pos(&self) -> Pos {
        self.positions.get(self.at).copied().unwrap_or_default()
    }

    /// Whether the last division header read is PROCEDURE DIVISION's.
    fn in_procedure_division(&self) -> bool {
        self.tokens.windows(2).rev().find_map(|w| match (&w[0].tok, &w[1].tok) {
            (Tok::Word(division), Tok::Word(word)) if word == "DIVISION" => Some(division == "PROCEDURE"),
            _ => None,
        }) == Some(true)
    }

    fn separator_follows(&self, ahead: usize) -> bool {
        self.peek(ahead).is_none_or(|c| c == ' ' || c == '\n')
    }

    fn emit(&mut self, tok: Tok, pos: Pos) {
        let before = |back: usize| match self.tokens.len().checked_sub(back).map(|i| &self.tokens[i].tok) {
            Some(Tok::Word(p)) => p.as_str(),
            _ => "",
        };
        match &tok {
            Tok::Word(w) => match w.as_str() {
                "PROGRAM-ID" => self.outer.push(self.decimal_comma),
                "PROGRAM" if before(1) == "END" => self.decimal_comma = self.outer.pop().unwrap_or(false),
                "COMMA" if before(1) == "DECIMAL-POINT" || before(1) == "IS" && before(2) == "DECIMAL-POINT" => self.decimal_comma = true,
                _ => {}
            },
            Tok::Alnum(a) => {
                let mut chars = a.chars();
                let names_symbol = before(1) == "SYMBOL" || (1..=3).any(|back| before(back) == "CURRENCY" && (1..back).all(|b| matches!(before(b), "SIGN" | "IS")));
                if let (Some(c), None, true) = (chars.next(), chars.next(), names_symbol) {
                    self.currency.push(c);
                }
            }
            Tok::Hex(bytes) => {
                let names_symbol = (1..=3).any(|back| before(back) == "CURRENCY" && (1..back).all(|b| matches!(before(b), "SIGN" | "IS")));
                if let ([byte], true) = (bytes.as_slice(), names_symbol)
                    && let Some(c) = ebcdic_lowercase(*byte)
                {
                    self.currency.push(c);
                }
            }
            _ => {}
        }
        let spelled = match &tok {
            Tok::Word(w) => self
                .at
                .checked_sub(w.chars().count())
                .map(|start| &self.chars[start..self.at])
                .filter(|raw| raw.iter().any(char::is_ascii_lowercase))
                .map(|raw| raw.iter().collect::<String>())
                .filter(|raw| raw.eq_ignore_ascii_case(w)),
            _ => None,
        };
        let (tok, spelled) = match tok {
            Tok::Word(w) if w.chars().count() > USER_WORD_MAX => self.long_word(w, spelled, pos),
            tok => (tok, spelled),
        };
        let after_comma = std::mem::take(&mut self.comma_pending);
        let span = self.free.iter().position(|s| s.holds(pos));
        let area_a = match span {
            Some(k) => {
                if !std::mem::replace(&mut self.warned[k], true)
                    && let Some(warning) = &self.free[k].warning
                {
                    self.pending.insert(0, warning.clone());
                }
                self.tokens.last().is_none_or(|t| t.tok == Tok::Period) && !matches!(&tok, Tok::Word(w) if rt::reserved_words::is_reserved(w))
            }
            None => (8..=11).contains(&pos.col),
        };
        self.tokens.push(Token { tok, pos, area_a, spelled, after_comma, messages: std::mem::take(&mut self.pending) });
    }

    /// A word longer than a user-defined word can be (Language Reference SC27-8713-03, p. 13).
    /// Enterprise COBOL reads its first 30 characters, with an error (IGYDS0023-E); Micro Focus
    /// and GnuCOBOL read it whole.
    fn long_word(&mut self, word: String, spelled: Option<String>, pos: Pos) -> (Tok, Option<String>) {
        if self.extended {
            self.pending.push(IWX0015.at(pos, format!("a user-defined word of more than 30 characters (Micro Focus and GnuCOBOL; Enterprise COBOL reads its first 30): {word} is read whole")));
            return (Tok::Word(word), spelled);
        }
        let first: String = word.chars().take(USER_WORD_MAX).collect();
        let count = word.chars().count();
        self.pending.push(IWS0099.at(pos, format!("{word}: a user-defined word has at most 30 characters, and this one has {count}; it is read as its first 30, {first}")));
        (Tok::Word(first), spelled.map(|s| s.chars().take(USER_WORD_MAX).collect()))
    }

    /// The character that is a numeric literal's decimal point.
    fn point(&self) -> char {
        if self.decimal_comma { ',' } else { '.' }
    }

    fn expecting_picture(&self) -> bool {
        let words: Vec<&str> =
            self.tokens.iter().rev().take(2).map(|t| if let Tok::Word(w) = &t.tok { w.as_str() } else { "" }).collect();
        matches!(words.as_slice(), ["PIC" | "PICTURE", ..] | ["IS", "PIC" | "PICTURE"])
    }

    fn next_token(&mut self) -> Result<(), Error> {
        let c = self.chars[self.at];
        let pos = self.pos();
        let point = self.point();
        let leading_point = c == ',' && point == ',' && self.peek(1).is_some_and(|d| d.is_ascii_digit()) && !self.at.checked_sub(1).is_some_and(|i| is_word_char(self.chars[i]));
        if leading_point && self.expecting_picture() {
            return self.picture(pos);
        }
        if leading_point {
            let tok = self.number_or_word(pos)?;
            self.emit(tok, pos);
            return Ok(());
        }
        if c == ' ' || c == '\n' || c == ',' || c == ';' {
            self.comma_pending |= c == ',' || c == ';';
            self.at += 1;
            return Ok(());
        }
        if self.expecting_picture() {
            return self.picture(pos);
        }
        let next = self.peek(1);
        let quote_next = matches!(next, Some('\'' | '"'));
        match c {
            '\'' | '"' => {
                let mut text = self.quoted(pos)?;
                if text.is_empty() {
                    let (read, how) = match self.empty {
                        numeric::EmptyLiteral::Space => (" ", "a space is assumed, as cobc assumes it"),
                        numeric::EmptyLiteral::Empty => ("", "it is read as no characters (--empty-literal empty)"),
                    };
                    let shown = format!("{c}{c}");
                    self.pending.push(if self.extended {
                        crate::messages::IWX0063.at(pos, format!("{shown} (GnuCOBOL and Micro Focus; Enterprise COBOL's literals hold at least one character): {how}"))
                    } else {
                        crate::messages::IWS0106.at(pos, format!("{shown}: Enterprise COBOL's alphanumeric literals hold at least one character; {how}"))
                    });
                    text = read.to_owned();
                }
                self.emit(Tok::Alnum(text), pos);
            }
            'X' | 'x' if quote_next => {
                self.at += 1;
                let text = self.quoted(pos)?;
                let bytes = unhex(&text).ok_or_else(|| crate::messages::IWS0017.at(pos, format!("X'{text}' is not an even number of hex digits")))?;
                self.emit(Tok::Hex(bytes), pos);
            }
            'N' | 'n' if matches!(next, Some('X' | 'x')) && matches!(self.peek(2), Some('\'' | '"')) => {
                self.at += 2;
                let text = self.quoted(pos)?;
                let units = unhex(&text).filter(|b| !b.is_empty() && b.len().is_multiple_of(2) && b.len() <= 160);
                let units = units.map(|b| b.chunks(2).map(|u| u16::from_be_bytes([u[0], u[1]])).collect::<Vec<_>>());
                let national = units.and_then(|u| String::from_utf16(&u).ok());
                let national = national.ok_or_else(|| crate::messages::IWS0018.at(pos, format!("NX'{text}': a national hexadecimal literal is 4 to 320 hex digits, four to each UTF-16 code unit")))?;
                self.emit(Tok::National(national), pos);
            }
            'N' | 'n' if quote_next && self.n_is_dbcs => self.dbcs_literal(pos)?,
            'G' | 'g' if quote_next => self.dbcs_literal(pos)?,
            'N' | 'n' if quote_next => {
                self.at += 1;
                let text = self.quoted(pos)?;
                self.emit(Tok::National(text), pos);
            }
            'Z' | 'z' if quote_next => {
                self.at += 1;
                let text = self.quoted(pos)?;
                self.emit(Tok::Alnum(format!("{text}\0")), pos);
            }
            '.' if self.extended && next.is_some_and(|n| n.is_ascii_alphabetic()) && matches!(self.tokens.last().map(|t| &t.tok), Some(Tok::Word(w)) if matches!(w.as_str(), "PROGRAM-ID" | "FUNCTION-ID" | "CLASS-ID" | "METHOD-ID")) => {
                let paragraph = match self.tokens.last().map(|t| &t.tok) {
                    Some(Tok::Word(w)) => w.clone(),
                    _ => String::new(),
                };
                self.pending.push(crate::messages::IWX0100.at(pos, format!("a period with no space after it ends {paragraph} (GnuCOBOL; Enterprise COBOL follows a separator period with a space): it is read as a separator period")));
                self.at += 1;
                self.emit(Tok::Period, pos);
            }
            '.' if self.separator_follows(1) => {
                self.at += 1;
                // A period alone in the PROCEDURE DIVISION is an empty sentence, which strict reads too.
                if self.extended && self.tokens.last().is_some_and(|t| t.tok == Tok::Period) && !self.in_procedure_division() {
                    self.pending.push(crate::messages::IWX0041.at(pos, "periods after a period (GnuCOBOL and Micro Focus; Enterprise COBOL ends a sentence with one): the periods after the first are ignored"));
                } else {
                    self.emit(Tok::Period, pos);
                }
            }
            '.' if self.extended && next == Some('.') && self.separator_follows(self.periods()) => {
                self.pending.push(crate::messages::IWX0041.at(pos, "periods after a period (GnuCOBOL and Micro Focus; Enterprise COBOL ends a sentence with one): the periods after the first are ignored"));
                self.at += self.periods();
                self.emit(Tok::Period, pos);
            }
            '+' | '-' if next.is_some_and(|n| n.is_ascii_digit() || (n == point && self.peek(2).is_some_and(|d| d.is_ascii_digit()))) => {
                self.at += 1;
                let digits = self.number_or_word(pos)?;
                match digits {
                    Tok::Number(n) => self.emit(Tok::Number(format!("{c}{n}")), pos),
                    _ => return Err(crate::messages::IWS0019.at(pos, "a sign must be followed by a number")),
                }
            }
            _ if c.is_ascii_alphanumeric() || c == '.' || non_cobol(c) || (self.extended && wide_letter(c)) => {
                let tok = self.number_or_word(pos)?;
                match tok {
                    Tok::Word(w) if w == "EXEC" || w == "EXECUTE" => {
                        let tok = self.exec_block(pos)?;
                        self.emit(tok, pos);
                    }
                    tok => self.emit(tok, pos),
                }
            }
            '<' if self.extended && next == Some('>') => {
                self.pending.push(crate::messages::IWX0003.at(pos, NOT_EQUAL));
                self.at += 2;
                self.emit(Tok::Word("NOT".into()), pos);
                let at = self.positions.get(self.at - 1).copied().unwrap_or(pos);
                self.emit(Tok::Eq, at);
            }
            '&' if self.extended && matches!(self.tokens.last().map(|t| &t.tok), Some(Tok::Alnum(_) | Tok::Hex(_) | Tok::National(_) | Tok::Word(_))) => {
                self.at += 1;
                self.emit(Tok::Ampersand, pos);
            }
            _ => {
                let (tok, len) = match (c, next) {
                    ('*', Some('*')) => (Tok::Power, 2),
                    ('<', Some('=')) => (Tok::Le, 2),
                    ('>', Some('=')) => (Tok::Ge, 2),
                    ('*', _) => (Tok::Star, 1),
                    ('/', _) => (Tok::Slash, 1),
                    ('+', _) => (Tok::Plus, 1),
                    ('-', _) => (Tok::Minus, 1),
                    ('=', _) => (Tok::Eq, 1),
                    ('<', _) => (Tok::Lt, 1),
                    ('>', _) => (Tok::Gt, 1),
                    ('(', _) => (Tok::LParen, 1),
                    (')', _) => (Tok::RParen, 1),
                    (':', _) => (Tok::Colon, 1),
                    ('&', _) if matches!(self.tokens.last().map(|t| &t.tok), Some(Tok::Alnum(_) | Tok::Hex(_) | Tok::National(_))) => {
                        return Err(crate::messages::IWS0020.at(pos, "literal concatenation with & is not Enterprise COBOL's"));
                    }
                    _ => return Err(crate::messages::IWS0021.at(pos, format!("unexpected character {c:?}"))),
                };
                self.at += len;
                self.emit(tok, pos);
            }
        }
        Ok(())
    }

    /// The text of an EXEC block up to END-EXEC, which is consumed; quotes inside are skipped whole.
    fn exec_block(&mut self, pos: Pos) -> Result<Tok, Error> {
        let start = self.at;
        let mut quote: Option<char> = None;
        while let Some(c) = self.peek(0) {
            match quote {
                Some(q) if c == q => quote = None,
                Some(_) => {}
                None if c == '\'' || c == '"' => quote = Some(c),
                None if c.eq_ignore_ascii_case(&'E') => {
                    let ahead: String = self.chars[self.at..].iter().take(8).collect();
                    let boundary = self.chars.get(self.at + 8).is_none_or(|d| !is_word_char(*d)) && (self.at == 0 || !is_word_char(self.chars[self.at - 1]));
                    if ahead.eq_ignore_ascii_case("END-EXEC") && boundary {
                        let text: String = self.chars[start..self.at].iter().collect();
                        self.at += 8;
                        return Ok(Tok::Exec(text.split_whitespace().collect::<Vec<_>>().join(" ")));
                    }
                }
                None => {}
            }
            self.at += 1;
        }
        Err(crate::messages::IWS0022.at(pos, "EXEC with no END-EXEC"))
    }

    /// Reads a quoted literal starting at the opening quote; a doubled quote stands for one.
    /// G'...', or N'...' under NSYMBOL(DBCS): the characters, the shift-out after the opening
    /// delimiter and the shift-in before the closing one, which a source in Unicode drops, removed
    /// where present.
    fn dbcs_literal(&mut self, pos: Pos) -> Result<(), Error> {
        self.at += 1;
        let text = self.quoted(pos)?;
        let text = text.strip_prefix('\u{E}').unwrap_or(&text);
        let text = text.strip_suffix('\u{F}').unwrap_or(text).to_owned();
        let count = text.chars().count();
        if count == 0 || count > DBCS_LITERAL_MAX {
            return Err(crate::messages::IWS0023.at(pos, format!("a DBCS literal holds 1 to {DBCS_LITERAL_MAX} characters, not {count}")));
        }
        self.emit(Tok::Dbcs(text), pos);
        Ok(())
    }

    fn quoted(&mut self, pos: Pos) -> Result<String, Error> {
        let quote = self.chars[self.at];
        self.at += 1;
        let mut text = String::new();
        loop {
            match self.peek(0) {
                None | Some('\n') => return Err(crate::messages::IWS0024.at(pos, "an unterminated literal")),
                Some(c) if c == quote && self.peek(1) == Some(quote) => {
                    text.push(quote);
                    self.at += 2;
                }
                Some(c) if c == quote => {
                    self.at += 1;
                    return Ok(text);
                }
                Some(c) => {
                    text.push(c);
                    self.at += 1;
                }
            }
        }
    }

    /// How many periods follow one another from here.
    fn periods(&self) -> usize {
        (0..).take_while(|&k| self.peek(k) == Some('.')).count()
    }

    fn number_or_word(&mut self, pos: Pos) -> Result<Tok, Error> {
        let start = self.at;
        while let Some(c) = self.peek(0).filter(|&c| is_word_char(c) || non_cobol(c) || (self.extended && wide_letter(c))) {
            if non_cobol(c) && !(self.extended && wide_letter(c)) {
                self.pending.push(crate::messages::IWS0025.at(self.pos(), format!("non-COBOL character {c:?}: the character was accepted")).graded(crate::Severity::Error));
            }
            self.at += 1;
        }
        let run: String = self.chars[start..self.at].iter().collect();
        if self.extended && run.chars().any(wide_letter) && !self.wide_words.contains(&run) {
            self.pending.push(crate::messages::IWX0094.at(pos, format!("the word {run} has letters outside COBOL's character set (GnuCOBOL reads a user-defined word's letters as UTF-8; Enterprise COBOL writes such a word in DBCS characters): it is read as a user-defined word")));
            self.wide_words.push(run.clone());
        }
        let all_digits = run.chars().all(|c| c.is_ascii_digit());
        if all_digits && self.peek(0) == Some(self.point()) && self.peek(1).is_some_and(|c| c.is_ascii_digit()) {
            self.at += 1;
            let frac_start = self.at;
            while self.peek(0).is_some_and(|c| c.is_ascii_digit()) {
                self.at += 1;
            }
            let frac: String = self.chars[frac_start..self.at].iter().collect();
            return Ok(Tok::Number(format!("{run}.{frac}")));
        }
        if run.is_empty() {
            return Err(crate::messages::IWS0026.at(pos, "unexpected '.'"));
        }
        Ok(if all_digits { Tok::Number(run) } else { Tok::Word(run.to_ascii_uppercase()) })
    }

    /// Only the period, comma or semicolon just before the space is a separator (assumption C195).
    fn picture(&mut self, pos: Pos) -> Result<(), Error> {
        let start = self.at;
        while self.peek(0).is_some_and(|c| c != ' ' && c != '\n') {
            self.at += 1;
        }
        let mut end = self.at;
        if end > start && matches!(self.chars[end - 1], '.' | ',' | ';') {
            end -= 1;
        }
        let text: String = self.chars[start..end].iter().collect();
        if text.is_empty() {
            return Err(crate::messages::IWS0027.at(pos, "PICTURE with no character-string"));
        }
        self.at = end;
        let text: String = text.chars().map(|c| if self.currency.contains(&c) { c } else { c.to_ascii_uppercase() }).collect();
        let tok = if matches!(text.as_str(), "IS" | "SYMBOL") && self.tokens.last().is_some_and(|t| matches!(&t.tok, Tok::Word(w) if w == "PIC" || w == "PICTURE")) {
            Tok::Word(text)
        } else {
            Tok::Pic(text)
        };
        self.emit(tok, pos);
        Ok(())
    }
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source;

    fn toks(text: &str) -> Vec<Tok> {
        lex(&source::read(text).unwrap()).unwrap().into_iter().map(|t| t.tok).collect()
    }

    fn w(s: &str) -> Tok {
        Tok::Word(s.into())
    }

    #[test]
    fn numbers_words_and_the_separator_period() {
        assert_eq!(toks("           05 A-1 VALUE 0.1."), [Tok::Number("05".into()), w("A-1"), w("VALUE"), Tok::Number("0.1".into()), Tok::Period]);
        assert_eq!(toks("           VALUE -12345."), [w("VALUE"), Tok::Number("-12345".into()), Tok::Period]);
        assert_eq!(toks("       100-MAIN."), [w("100-MAIN"), Tok::Period]);
    }

    #[test]
    fn operators_need_spaces_and_signed_literals_do_not() {
        assert_eq!(toks("           A - 1 ** 2"), [w("A"), Tok::Minus, Tok::Number("1".into()), Tok::Power, Tok::Number("2".into())]);
        assert_eq!(toks("           >= <= ("), [Tok::Ge, Tok::Le, Tok::LParen]);
    }

    #[test]
    fn literals() {
        assert_eq!(toks("           'IT''S' X'F1C1' N'AB'"), [Tok::Alnum("IT'S".into()), Tok::Hex(vec![0xF1, 0xC1]), Tok::National("AB".into())]);
        assert_eq!(toks("           NX'00410042' nx\"265ED83DDE00\""), [Tok::National("AB".into()), Tok::National("\u{265E}\u{1F600}".into())]);
        assert_eq!(toks("           G'\u{E}ＡＢ\u{F}' g\"日本\""), [Tok::Dbcs("ＡＢ".into()), Tok::Dbcs("日本".into())]);
        assert_eq!(toks("       CBL NSYMBOL(DBCS)\n           N'ＡＢ' NX'0041'"), [Tok::Dbcs("ＡＢ".into()), Tok::National("A".into())]);
        let long = format!("           G'{}'", "Ａ".repeat(29));
        assert!(lex(&source::read(&long).unwrap()).unwrap_err().message.contains("1 to 28 characters, not 29"));
        for bad in ["NX'GH'", "NX'1'", "NX'004'", "NX'D83D'"] {
            let e = lex(&source::read(&format!("           {bad}")).unwrap()).unwrap_err();
            assert!(e.message.contains("a national hexadecimal literal is 4 to 320 hex digits"), "{bad}: {}", e.message);
        }
    }

    #[test]
    fn a_null_terminated_literal_ends_with_x00() {
        assert_eq!(toks("           Z'ABC' z\"(I)V\""), [Tok::Alnum("ABC\0".into()), Tok::Alnum("(I)V\0".into())]);
    }

    #[test]
    fn a_picture_is_one_token_and_keeps_its_own_periods() {
        assert_eq!(toks("           PIC S9(3)V99 COMP-3."), [w("PIC"), Tok::Pic("S9(3)V99".into()), w("COMP-3"), Tok::Period]);
        assert_eq!(toks("           PICTURE IS ZZ,ZZ9.99."), [w("PICTURE"), w("IS"), Tok::Pic("ZZ,ZZ9.99".into()), Tok::Period]);
    }

    #[test]
    fn only_the_last_period_or_comma_before_the_space_is_a_separator() {
        assert_eq!(toks("           PIC 9,9,9,."), [w("PIC"), Tok::Pic("9,9,9,".into()), Tok::Period]);
        assert_eq!(toks("           PIC 999999999999.."), [w("PIC"), Tok::Pic("999999999999.".into()), Tok::Period]);
        assert_eq!(toks("           PIC 99, VALUE 1."), [w("PIC"), Tok::Pic("99".into()), w("VALUE"), Tok::Number("1".into()), Tok::Period]);
        assert_eq!(toks("           PIC 9.9,; VALUE 1."), [w("PIC"), Tok::Pic("9.9,".into()), w("VALUE"), Tok::Number("1".into()), Tok::Period]);
        assert_eq!(toks("           PIC 999., VALUE 1."), [w("PIC"), Tok::Pic("999.".into()), w("VALUE"), Tok::Number("1".into()), Tok::Period]);
        let comma = "           DECIMAL-POINT IS COMMA.\n           PIC 9.9.9,. PIC 999,,\n";
        let pics: Vec<Tok> = toks(comma).into_iter().filter(|t| matches!(t, Tok::Pic(_))).collect();
        assert_eq!(pics, [Tok::Pic("9.9.9,".into()), Tok::Pic("999,".into())]);
    }

    #[test]
    fn commas_between_operands_are_separators() {
        assert_eq!(toks("           F(A, 1)"), [w("F"), Tok::LParen, w("A"), Tok::Number("1".into()), Tok::RParen]);
        assert_eq!(toks("           T(I,J)"), [w("T"), Tok::LParen, w("I"), w("J"), Tok::RParen]);
    }

    #[test]
    fn an_exec_block_is_one_token() {
        assert_eq!(
            toks("           EXEC SQL SELECT A.B INTO :X FROM T WHERE C = 'END-EXEC'\n               END-EXEC."),
            [Tok::Exec("SQL SELECT A.B INTO :X FROM T WHERE C = 'END-EXEC'".into()), Tok::Period]
        );
    }

    #[test]
    fn a_comment_entry_holds_any_character() {
        let program = concat!(
            "       IDENTIFICATION DIVISION.\n",
            "       PROGRAM-ID. CE3.\n",
            "       AUTHOR. Smith & Jones @ ACME #1, Café O'Grady.\n",
            "       INSTALLATION. \"HQ\n",
            "           ~ ^ ` { } | \\ ?\n",
            "       DATE-WRITTEN. 01/01/99.\n",
            "       PROCEDURE DIVISION.\n",
            "           GOBACK.\n",
        );
        let words = [w("IDENTIFICATION"), w("DIVISION"), Tok::Period, w("PROGRAM-ID"), Tok::Period, w("CE3"), Tok::Period];
        let paragraphs = [w("AUTHOR"), Tok::Period, w("INSTALLATION"), Tok::Period, w("DATE-WRITTEN"), Tok::Period];
        let procedure = [w("PROCEDURE"), w("DIVISION"), Tok::Period, w("GOBACK"), Tok::Period];
        assert_eq!(toks(program), [&words[..], &paragraphs[..], &procedure[..]].concat());
    }

    #[test]
    fn a_literal_continued_between_the_quotes_of_a_doubled_quote() {
        let head = "           \"A+0B-1C*2D";
        let text = format!("{head}{}\"\n      -    \"\"9K(L)M>N<O\".\n", "=".repeat(72 - head.len() - 1));
        assert_eq!(toks(&text), [Tok::Alnum(format!("A+0B-1C*2D{}\"9K(L)M>N<O", "=".repeat(72 - head.len() - 1))), Tok::Period]);
    }

    #[test]
    fn a_continuation_that_opens_a_quote_after_a_closed_literal_is_a_second_literal() {
        assert_eq!(toks("           VALUE 'ABC'\n      -    'DEF'."), [w("VALUE"), Tok::Alnum("ABC".into()), Tok::Alnum("DEF".into()), Tok::Period]);
    }

    #[test]
    fn an_ampersand_after_a_literal_is_named_as_concatenation() {
        let error = |text: &str| lex(&source::read(text).unwrap()).unwrap_err().message;
        assert!(error("           'A' & 'B'").contains("literal concatenation with & is not Enterprise COBOL's"));
        assert!(error("           NOTIFY=&SYSUID").contains("unexpected character '&'"));
    }

    #[test]
    fn a_non_cobol_character_is_accepted_into_its_word_with_an_error() {
        let lexed = lex(&source::read("           MOVE WS#1 TO %\u{1b} 'A@B'.").unwrap()).unwrap();
        assert_eq!(lexed.iter().map(|t| t.tok.clone()).collect::<Vec<_>>(), [w("MOVE"), w("WS#1"), w("TO"), w("%\u{1b}"), Tok::Alnum("A@B".into()), Tok::Period]);
        let messages: Vec<(usize, u32, &str, crate::Severity)> =
            lexed.iter().enumerate().flat_map(|(i, t)| t.messages.iter().map(move |m| (i, m.pos.col, m.message.as_str(), m.severity))).collect();
        assert_eq!(
            messages,
            [
                (1, 19, "non-COBOL character '#': the character was accepted", crate::Severity::Error),
                (3, 25, "non-COBOL character '%': the character was accepted", crate::Severity::Error),
                (3, 26, "non-COBOL character '\\u{1b}': the character was accepted", crate::Severity::Error),
            ]
        );
        let error = |text: &str| lex(&source::read(text).unwrap()).unwrap_err().message;
        assert_eq!(error("           MOVE $X"), "unexpected character '$'");
        assert_eq!(error("           MOVE \u{3042}"), "unexpected character '\u{3042}'");
    }

    #[test]
    fn under_extended_not_equal_is_not_and_equals_and_an_ampersand_is_kept() {
        let lexed = lex_under(&source::read("           IF A <> 'B' & X'C1' MOVE C & D").unwrap(), Compliance::Extended).unwrap();
        let toks: Vec<Tok> = lexed.iter().map(|t| t.tok.clone()).collect();
        assert_eq!(toks, [w("IF"), w("A"), w("NOT"), Tok::Eq, Tok::Alnum("B".into()), Tok::Ampersand, Tok::Hex(vec![0xC1]), w("MOVE"), w("C"), Tok::Ampersand, w("D")]);
        assert_eq!(lexed[2].messages.iter().map(|m| (m.pos.col, m.message.as_str(), m.severity)).collect::<Vec<_>>(), [(17, NOT_EQUAL, crate::Severity::Warning)]);
        assert_eq!(lexed[3].pos.col, 18);
        let strict = |text: &str| lex(&source::read(text).unwrap()).map(|t| t.into_iter().map(|t| t.tok).collect::<Vec<_>>());
        assert_eq!(strict("           A <> B"), Ok(vec![w("A"), Tok::Lt, Tok::Gt, w("B")]));
        assert!(lex_under(&source::read("           NOTIFY=&SYSUID").unwrap(), Compliance::Extended).unwrap_err().message.contains("unexpected character '&'"));
    }

    #[test]
    fn in_free_form_area_a_is_a_word_after_a_period_that_is_not_reserved() {
        let text = "IDENTIFICATION DIVISION.\nPROCEDURE DIVISION.\nMAIN-P.\n    MOVE A TO\n  B.\n    GOBACK.\n0100.\n";
        let free = source::read_under(text, 0, false, Compliance::Extended).unwrap();
        let lexed = lex_under(&free, Compliance::Extended).unwrap();
        let marked: Vec<Tok> = lexed.iter().filter(|t| t.area_a).map(|t| t.tok.clone()).collect();
        assert_eq!(marked, [w("MAIN-P"), Tok::Number("0100".into())]);
        assert_eq!(lexed[0].messages[0].id, Some("IWX0001"), "{:?}", lexed[0].messages);
        assert!(lexed[1..].iter().all(|t| t.messages.is_empty()));
    }

    #[test]
    fn area_a_is_marked() {
        let t = lex(&source::read("       PARA.\n           MOVE").unwrap()).unwrap();
        assert!(t[0].area_a);
        assert!(!t[2].area_a);
    }

    fn numbers(text: &str) -> Vec<String> {
        toks(text).into_iter().filter_map(|t| if let Tok::Number(n) = t { Some(n) } else { None }).collect()
    }

    #[test]
    fn under_decimal_point_is_comma_a_comma_between_digits_is_the_point() {
        let text = "           DECIMAL-POINT IS COMMA.\n           1,5 -,25 +3,0 ,75 T(1, 2) A,B 1.\n";
        assert_eq!(numbers(text), ["1.5", "-.25", "+3.0", ".75", "1", "2", "1"]);
        assert!(toks(text).contains(&w("B")));
        assert!(toks("           DECIMAL-POINT IS COMMA.\n           PIC ,99.").contains(&Tok::Pic(",99".into())));
        assert!(lex(&source::read("           DECIMAL-POINT COMMA.\n           MOVE 1.5").unwrap()).is_err());
    }

    #[test]
    fn a_contained_program_keeps_the_decimal_comma_and_the_next_program_does_not() {
        let text = concat!(
            "       PROGRAM-ID. A.\n           DECIMAL-POINT IS COMMA.\n           1,5\n",
            "       PROGRAM-ID. B.\n           2,5\n       END PROGRAM B.\n           3,5\n       END PROGRAM A.\n",
            "       PROGRAM-ID. C.\n           4,5\n",
        );
        assert_eq!(numbers(text), ["1.5", "2.5", "3.5", "4", "5"]);
    }

    #[test]
    fn a_zero_length_literal_is_a_space_unless_the_flag_says_empty_and_strict_names_it_at_severity_e() {
        let source = source::read("           MOVE '' TO X \"\" 'A'").unwrap();
        let literals = |lexed: &[Token]| lexed.iter().filter_map(|t| if let Tok::Alnum(a) = &t.tok { Some(a.clone()) } else { None }).collect::<Vec<_>>();
        let ids = |lexed: &[Token]| lexed.iter().flat_map(|t| t.messages.iter().map(|m| (m.id, m.severity))).collect::<Vec<_>>();
        let strict = lex_with(&source, Compliance::Strict, numeric::EmptyLiteral::Space).unwrap();
        assert_eq!(literals(&strict), [" ", " ", "A"]);
        assert_eq!(ids(&strict), [(Some("IWS0106"), crate::Severity::Error); 2]);
        let extended = lex_with(&source, Compliance::Extended, numeric::EmptyLiteral::Empty).unwrap();
        assert_eq!(literals(&extended), ["", "", "A"]);
        assert_eq!(ids(&extended), [(Some("IWX0063"), crate::Severity::Warning); 2]);
        assert!(extended[1].messages[0].message.ends_with("it is read as no characters (--empty-literal empty)"));
    }
}
