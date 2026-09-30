use crate::source::Source;
use crate::{Error, Pos};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tok {
    /// A COBOL word, uppercased: a name, a reserved word or a level number with letters in it.
    Word(String),
    /// A numeric literal as written: optional sign, digits, optional decimal point.
    Number(String),
    Alnum(String),
    Hex(Vec<u8>),
    National(String),
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
}

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
}

pub fn lex(source: &Source) -> Result<Vec<Token>, Error> {
    let mut lx = Lexer { chars: source.text.chars().collect(), positions: &source.positions, at: 0, tokens: Vec::new(), decimal_comma: false, currency: Vec::new(), outer: Vec::new(), comma_pending: false };
    while lx.at < lx.chars.len() {
        lx.next_token()?;
    }
    Ok(lx.tokens)
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

impl Lexer<'_> {
    fn peek(&self, ahead: usize) -> Option<char> {
        self.chars.get(self.at + ahead).copied()
    }

    fn pos(&self) -> Pos {
        self.positions.get(self.at).copied().unwrap_or_default()
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
        let after_comma = std::mem::take(&mut self.comma_pending);
        self.tokens.push(Token { tok, pos, area_a: (8..=11).contains(&pos.col), spelled, after_comma });
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
                let text = self.quoted(pos)?;
                self.emit(Tok::Alnum(text), pos);
            }
            'X' | 'x' if quote_next => {
                self.at += 1;
                let text = self.quoted(pos)?;
                let bytes = unhex(&text).ok_or_else(|| Error::at(pos, format!("X'{text}' is not an even number of hex digits")))?;
                self.emit(Tok::Hex(bytes), pos);
            }
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
            '.' if self.separator_follows(1) => {
                self.at += 1;
                self.emit(Tok::Period, pos);
            }
            '+' | '-' if next.is_some_and(|n| n.is_ascii_digit() || (n == point && self.peek(2).is_some_and(|d| d.is_ascii_digit()))) => {
                self.at += 1;
                let digits = self.number_or_word(pos)?;
                match digits {
                    Tok::Number(n) => self.emit(Tok::Number(format!("{c}{n}")), pos),
                    _ => return Err(Error::at(pos, "a sign must be followed by a number")),
                }
            }
            _ if c.is_ascii_alphanumeric() || c == '.' => {
                let tok = self.number_or_word(pos)?;
                match tok {
                    Tok::Word(w) if w == "EXEC" || w == "EXECUTE" => {
                        let tok = self.exec_block(pos)?;
                        self.emit(tok, pos);
                    }
                    tok => self.emit(tok, pos),
                }
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
                        return Err(Error::at(pos, "literal concatenation with & is not Enterprise COBOL's"));
                    }
                    _ => return Err(Error::at(pos, format!("unexpected character {c:?}"))),
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
        Err(Error::at(pos, "EXEC with no END-EXEC"))
    }

    /// Reads a quoted literal starting at the opening quote; a doubled quote stands for one.
    fn quoted(&mut self, pos: Pos) -> Result<String, Error> {
        let quote = self.chars[self.at];
        self.at += 1;
        let mut text = String::new();
        loop {
            match self.peek(0) {
                None | Some('\n') => return Err(Error::at(pos, "an unterminated literal")),
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

    fn number_or_word(&mut self, pos: Pos) -> Result<Tok, Error> {
        let start = self.at;
        while self.peek(0).is_some_and(is_word_char) {
            self.at += 1;
        }
        let run: String = self.chars[start..self.at].iter().collect();
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
            return Err(Error::at(pos, "unexpected '.'"));
        }
        Ok(if all_digits { Tok::Number(run) } else { Tok::Word(run.to_ascii_uppercase()) })
    }

    fn picture(&mut self, pos: Pos) -> Result<(), Error> {
        let start = self.at;
        while self.peek(0).is_some_and(|c| c != ' ' && c != '\n') {
            self.at += 1;
        }
        let mut end = self.at;
        while end > start && matches!(self.chars[end - 1], '.' | ',' | ';') {
            end -= 1;
        }
        let text: String = self.chars[start..end].iter().collect();
        if text.is_empty() {
            return Err(Error::at(pos, "PICTURE with no character-string"));
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
}
