//! What `--compliance extended` does to the tokens before the parser reads them: each constant
//! entry comes out and every later use of its name stands for its value, literals joined by `&`
//! become one literal, BINARY-SHORT, BINARY-LONG and BINARY-DOUBLE become COMP-5 PICTUREs,
//! FLOAT-SHORT and FLOAT-LONG become COMP-1 and COMP-2, PROGRAM-POINTER becomes PROCEDURE-POINTER,
//! and RETURNING OMITTED leaves a program's
//! PROCEDURE DIVISION header (docs/compliance.md).

use crate::lexer::{Tok, Token};
use crate::messages::{IWX0002, IWX0004, IWX0005, IWX0006, IWX0009, IWX0027, IWX0044, IWX0066, IWX0073, IWX0074, IWX0104};
use crate::{Error, Pos};
use std::collections::HashMap;

pub const CONSTANT: &str = "constant entry (Micro Focus and GnuCOBOL; Enterprise COBOL has no level 78 and no CONSTANT clause)";
pub const CONCATENATION: &str = "literal concatenation with & (Micro Focus and GnuCOBOL; Enterprise COBOL has none)";
pub const BINARY_USAGE: &str = "the COBOL 2002 binary usage (Micro Focus and GnuCOBOL; not Enterprise COBOL's)";
pub const GNUCOBOL_BINARY_USAGE: &str = "GnuCOBOL's binary usage (not Enterprise COBOL's)";
pub const NO_IDENTIFICATION_HEADER: &str = "PROGRAM-ID with no IDENTIFICATION DIVISION header before it (COBOL 2002, Micro Focus and GnuCOBOL; Enterprise COBOL requires the header)";
pub const ASSIGN_ITEM: &str = "ASSIGN to a data item (Micro Focus and GnuCOBOL; Enterprise COBOL's assignment-name is never a data item)";
pub const RETURNING_OMITTED: &str = "PROCEDURE DIVISION RETURNING OMITTED (GnuCOBOL; Enterprise COBOL's RETURNING names an 01 or 77 item of the LINKAGE SECTION)";

/// A binary usage word read as a COMP-5 PICTURE of two, four or eight bytes.
struct BinaryUsage {
    word: &'static str,
    digits: &'static str,
    /// Whether the word fixes its sign; the others take SIGNED (the default) or UNSIGNED after them.
    signed: Option<bool>,
    origin: &'static str,
}

/// The binary usages and the COMP-5 PICTURE each is, as GnuCOBOL 3.2 sizes them.
const BINARY_USAGES: &[BinaryUsage] = &[
    BinaryUsage { word: "BINARY-SHORT", digits: "9(4)", signed: None, origin: BINARY_USAGE },
    BinaryUsage { word: "BINARY-LONG", digits: "9(9)", signed: None, origin: BINARY_USAGE },
    BinaryUsage { word: "BINARY-DOUBLE", digits: "9(18)", signed: None, origin: BINARY_USAGE },
    BinaryUsage { word: "BINARY-LONG-LONG", digits: "9(18)", signed: None, origin: GNUCOBOL_BINARY_USAGE },
    BinaryUsage { word: "BINARY-INT", digits: "9(9)", signed: None, origin: GNUCOBOL_BINARY_USAGE },
    BinaryUsage { word: "SIGNED-SHORT", digits: "9(4)", signed: Some(true), origin: GNUCOBOL_BINARY_USAGE },
    BinaryUsage { word: "UNSIGNED-SHORT", digits: "9(4)", signed: Some(false), origin: GNUCOBOL_BINARY_USAGE },
    BinaryUsage { word: "SIGNED-INT", digits: "9(9)", signed: Some(true), origin: GNUCOBOL_BINARY_USAGE },
    BinaryUsage { word: "UNSIGNED-INT", digits: "9(9)", signed: Some(false), origin: GNUCOBOL_BINARY_USAGE },
    BinaryUsage { word: "SIGNED-LONG", digits: "9(18)", signed: Some(true), origin: GNUCOBOL_BINARY_USAGE },
    BinaryUsage { word: "UNSIGNED-LONG", digits: "9(18)", signed: Some(false), origin: GNUCOBOL_BINARY_USAGE },
];

/// The words between ASSIGN and the name it gives the file.
const ASSIGN_PHRASE: &[&str] = &["TO", "USING", "DYNAMIC", "EXTERNAL", "DISK"];

const FIGURATIVES: &[&str] = &["ZERO", "ZEROS", "ZEROES", "SPACE", "SPACES", "HIGH-VALUE", "HIGH-VALUES", "LOW-VALUE", "LOW-VALUES", "QUOTE", "QUOTES", "NULL", "NULLS"];

/// The tokens with constant entries taken out, their names replaced by their values, `&`
/// concatenations joined, the binary usages rewritten and RETURNING OMITTED taken out of a
/// program's PROCEDURE DIVISION header. `cards` are the CBL and PROCESS options, whose code page
/// reads a hexadecimal literal joined to an alphanumeric one.
pub fn rewrite(tokens: Vec<Token>, cards: &[String], directed: &[crate::directives::Constant]) -> Result<Vec<Token>, Error> {
    let mut options = numeric::Options::default();
    for card in cards {
        options.apply(card).ok();
    }
    let mut r = Rewrite { tokens, at: 0, out: Vec::new(), constants: HashMap::new(), pending: Vec::new(), options, assigned: Vec::new() };
    let mut directed: Vec<&crate::directives::Constant> = directed.iter().collect();
    // `program`: the last ID paragraph was a PROGRAM-ID, not a function's, class's or method's.
    let (mut data, mut program, mut header) = (false, false, false);
    while r.at < r.tokens.len() {
        // A directive's constant holds from the first token of its file after its line.
        let at = r.tokens[r.at].pos;
        directed.retain(|c| {
            if c.pos.file != at.file || c.pos.line >= at.line {
                return true;
            }
            if c.replaces || !r.constants.contains_key(&c.name) {
                r.constants.insert(c.name.clone(), directed_value(&c.literal));
            }
            false
        });
        let division = r.tokens.get(r.at + 1).is_some_and(|t| matches!(&t.tok, Tok::Word(w) if w == "DIVISION"));
        if let Tok::Word(w) = &r.tokens[r.at].tok
            && matches!(w.as_str(), "PROGRAM-ID" | "FUNCTION-ID" | "METHOD-ID" | "CLASS-ID" | "INTERFACE-ID")
        {
            program = w == "PROGRAM-ID";
            r.assigned.clear();
        }
        let data_section = r.tokens.get(r.at + 1).is_some_and(|t| matches!(&t.tok, Tok::Word(w) if w == "SECTION"));
        match &r.tokens[r.at].tok {
            Tok::Word(w) if division => {
                data = w == "DATA";
                header = program && w == "PROCEDURE";
            }
            // A DATA DIVISION section with no division header begins the division (IWX0071).
            Tok::Word(w) if data_section && matches!(w.as_str(), "WORKING-STORAGE" | "LOCAL-STORAGE" | "LINKAGE" | "FILE") => data = true,
            Tok::Period => header = false,
            Tok::Word(w) if header && w == "RETURNING" && r.word_at(1) == Some("OMITTED") && r.tokens.get(r.at + 2).is_some_and(|t| t.tok == Tok::Period) => {
                r.returning_omitted();
                continue;
            }
            Tok::Number(n) if data && r.out.last().is_none_or(|t| t.tok == Tok::Period) && matches!(r.tokens.get(r.at + 1).map(|t| &t.tok), Some(Tok::Word(_))) && (n == "78" || r.word_at(2) == Some("CONSTANT") && matches!(n.as_str(), "01" | "1")) => {
                r.constant()?;
                continue;
            }
            Tok::Word(w) if data && BINARY_USAGES.iter().any(|u| u.word == w) => {
                r.binary_usage()?;
                continue;
            }
            Tok::Word(w) if data && matches!(w.as_str(), "FLOAT-SHORT" | "FLOAT-LONG" | "FLOAT-HEX-7" | "FLOAT-HEX-15") => {
                r.float_usage();
                continue;
            }
            Tok::Word(w) if data && w == "PROGRAM-POINTER" => {
                r.program_pointer();
                continue;
            }
            Tok::Word(w) if (w == "PROGRAM-ID" || w == "FUNCTION-ID") && !ends_with_header(&r.out) => r.identification_header(),
            _ => {}
        }
        let value = r.value()?;
        r.push(value);
        if r.assign_name_last() {
            r.assigned.push(r.out.len() - 1);
        }
    }
    if let Some(last) = r.out.last_mut() {
        last.messages.append(&mut r.pending);
    }
    Ok(r.out)
}

struct Rewrite {
    tokens: Vec<Token>,
    at: usize,
    out: Vec<Token>,
    constants: HashMap<String, Tok>,
    /// Messages of tokens taken out, for the next token kept.
    pending: Vec<Error>,
    /// The cards' options, whose code page reads a hexadecimal literal joined to an alphanumeric one.
    options: numeric::Options,
    /// Where in `out` the program's ASSIGN clauses give a file's name.
    assigned: Vec<usize>,
}

impl Rewrite {
    fn push(&mut self, mut token: Token) {
        token.messages.splice(0..0, self.pending.drain(..));
        self.out.push(token);
    }

    /// Whether the token last kept is the name after ASSIGN [TO] [DYNAMIC | USING | EXTERNAL | DISK].
    fn assign_name_last(&self) -> bool {
        let Some((last, before)) = self.out.split_last() else { return false };
        let phrase = |t: &Token| matches!(&t.tok, Tok::Word(w) if ASSIGN_PHRASE.contains(&w.as_str()));
        matches!(last.tok, Tok::Word(_)) && !phrase(last) && before.iter().rev().find(|t| !phrase(t)).is_some_and(|t| matches!(&t.tok, Tok::Word(w) if w == "ASSIGN"))
    }

    /// The constant `name` stands for `value` from here on, and where an ASSIGN before it names it,
    /// as cobc reads the names ASSIGN gives once the DATA DIVISION is read.
    fn define(&mut self, name: String, value: Tok) {
        for &k in &self.assigned {
            let token = &mut self.out[k];
            if matches!(&token.tok, Tok::Word(w) if *w == name) {
                token.messages.push(IWX0104.at(token.pos, format!("ASSIGN {name}: a constant defined after the ASSIGN (GnuCOBOL; Enterprise COBOL has no constant entry): the file's name is its value")));
                token.tok = value.clone();
                token.spelled = None;
            }
        }
        self.constants.insert(name, value);
    }

    /// Token `k`, a constant's name replaced by its value and a PICTURE's `(name)` by the number.
    fn substituted(&self, k: usize) -> Token {
        let mut token = self.tokens[k].clone();
        match &token.tok {
            Tok::Word(w) => {
                if let Some(value) = self.constants.get(w) {
                    token.tok = value.clone();
                    token.spelled = None;
                }
            }
            Tok::Pic(p) => {
                if let Some(p) = picture(p, &self.constants) {
                    token.tok = Tok::Pic(p);
                }
            }
            _ => {}
        }
        token
    }

    /// The token at `at`, substituted, and the literals any `&` joins to it.
    fn value(&mut self) -> Result<Token, Error> {
        let mut left = self.substituted(self.at);
        self.at += 1;
        while self.tokens.get(self.at).is_some_and(|t| t.tok == Tok::Ampersand) {
            let amp = self.tokens[self.at].clone();
            if self.at + 1 >= self.tokens.len() {
                return Err(crate::messages::IWS0013.at(amp.pos, "& with no literal after it"));
            }
            let right = self.substituted(self.at + 1);
            self.at += 2;
            left = join(left, amp, right, |bytes| self.options.code_page().decode(bytes))?;
        }
        Ok(left)
    }

    fn word_at(&self, ahead: usize) -> Option<&str> {
        match self.tokens.get(self.at + ahead).map(|t| &t.tok) {
            Some(Tok::Word(w)) => Some(w),
            _ => None,
        }
    }

    /// A constant entry from its level number, `78 name [IS] [GLOBAL] VALUE [IS] value.` or
    /// `01 name CONSTANT [IS] [GLOBAL] [AS] value.`, where the value is a literal, a figurative
    /// constant, a constant already defined, or literals joined by `&`. The entry is taken out of
    /// the tokens.
    fn constant(&mut self) -> Result<(), Error> {
        let level = self.tokens[self.at].clone();
        let named = self.tokens[self.at + 1].clone();
        let Tok::Word(name) = named.tok else { unreachable!("the caller saw a word") };
        let seventy_eight = level.tok == Tok::Number("78".into());
        self.at += if seventy_eight { 2 } else { 3 };
        self.pending.extend(level.messages);
        self.pending.extend(named.messages);
        let refused = |at: Pos, why: &str| crate::messages::IWS0014.at(at, format!("constant {name}: {why}"));
        match (self.word_at(0), self.word_at(1)) {
            (Some("IS"), Some("GLOBAL")) => self.at += 2,
            (Some("GLOBAL"), _) => self.at += 1,
            _ => {}
        }
        let here = self.tokens.get(self.at).map_or(named.pos, |t| t.pos);
        if seventy_eight {
            if self.word_at(0) != Some("VALUE") {
                return Err(refused(here, "a level-78 entry is VALUE and its value, then a period"));
            }
            self.at += 1;
            if self.word_at(0) == Some("IS") {
                self.at += 1;
            }
        } else if self.word_at(0) == Some("AS") {
            self.at += 1;
        }
        if self.at >= self.tokens.len() {
            return Err(refused(here, "VALUE with no value"));
        }
        let end = (self.at..self.tokens.len()).find(|&k| self.tokens[k].tok == Tok::Period);
        if let Some(end) = end
            && end - self.at > 1
            && self.tokens[self.at..end].iter().any(|t| matches!(t.tok, Tok::Plus | Tok::Minus | Tok::Star | Tok::Slash | Tok::LParen))
        {
            let value = arithmetic(&self.tokens[self.at..end], &self.constants).ok_or_else(|| refused(here, "an arithmetic expression of numeric literals and numeric constants defined before, with +, -, *, / and parentheses"))?;
            self.pending.extend(self.tokens[self.at..=end].iter().flat_map(|t| t.messages.iter().cloned()));
            self.at = end + 1;
            self.pending.push(IWX0074.at(level.pos, format!("constant {name} AS an arithmetic expression (COBOL 2002, Micro Focus and GnuCOBOL; Enterprise COBOL has no constant entry): it stands for {value}, the expression's value truncated to an integer, as the standard gives it")));
            self.define(name, Tok::Number(value.to_string()));
            return Ok(());
        }
        let value = self.value()?;
        let literal = match &value.tok {
            Tok::Alnum(_) | Tok::Hex(_) | Tok::National(_) | Tok::Number(_) => true,
            Tok::Word(w) => FIGURATIVES.contains(&w.as_str()),
            _ => false,
        };
        let ended = self.tokens.get(self.at).is_some_and(|t| t.tok == Tok::Period);
        if !literal || !ended {
            let at = if literal { self.tokens.get(self.at).map_or(value.pos, |t| t.pos) } else { value.pos };
            return Err(refused(at, "the value is a literal, a figurative constant, a constant defined before, or literals joined by &; ironwork computes no expression there"));
        }
        self.pending.extend(self.tokens[self.at].messages.iter().cloned());
        self.at += 1;
        self.pending.push(IWX0002.at(level.pos, format!("{CONSTANT}: {name} stands for its value wherever it is used after this entry")));
        self.pending.extend(value.messages);
        self.define(name, value.tok);
        Ok(())
    }

    /// `IDENTIFICATION DIVISION.` before a PROGRAM-ID that has none, which it means.
    fn identification_header(&mut self) {
        let at = self.tokens[self.at].clone();
        let made = |tok: Tok, messages: Vec<Error>| Token { tok, pos: at.pos, area_a: at.area_a, spelled: None, after_comma: false, messages };
        let warning = if at.tok == Tok::Word("FUNCTION-ID".into()) {
            IWX0073.at(at.pos, "FUNCTION-ID with no IDENTIFICATION DIVISION header before it (COBOL 2002 and GnuCOBOL; Enterprise COBOL requires the header): the function reads as though IDENTIFICATION DIVISION. came before it")
        } else {
            IWX0006.at(at.pos, format!("{NO_IDENTIFICATION_HEADER}: the program reads as though IDENTIFICATION DIVISION. came before it"))
        };
        self.push(made(Tok::Word("IDENTIFICATION".into()), vec![warning]));
        self.push(made(Tok::Word("DIVISION".into()), Vec::new()));
        self.push(made(Tok::Period, Vec::new()));
    }

    /// `RETURNING OMITTED` ending a program's PROCEDURE DIVISION header, taken out: GnuCOBOL's
    /// program that returns no item is one with no RETURNING phrase.
    fn returning_omitted(&mut self) {
        let returning = self.tokens[self.at].clone();
        self.pending.push(IWX0009.at(returning.pos, format!("{RETURNING_OMITTED}: the program is read with no RETURNING phrase, and returns its RETURN-CODE to its caller as any program does")));
        self.pending.extend(returning.messages);
        self.pending.extend(self.tokens[self.at + 1].messages.iter().cloned());
        self.at += 2;
    }

    /// `[USAGE [IS]] BINARY-SHORT [SIGNED|UNSIGNED]` and the other binary usages in a data entry, as
    /// `PIC S9(n) COMP-5`, or `PIC 9(n) COMP-5` when unsigned: SIGNED is the default where the word
    /// does not fix the sign.
    /// FLOAT-SHORT or FLOAT-LONG, and z390's FLOAT-HEX-7 or FLOAT-HEX-15, read as COMP-1 or COMP-2,
    /// IBM's hexadecimal floating point.
    fn float_usage(&mut self) {
        let mut token = self.tokens[self.at].clone();
        let Tok::Word(usage) = &token.tok else { unreachable!("the caller saw a word") };
        let ibm = if matches!(usage.as_str(), "FLOAT-SHORT" | "FLOAT-HEX-7") { "COMP-1" } else { "COMP-2" };
        let message = if usage.starts_with("FLOAT-HEX") {
            IWX0066.at(token.pos, format!("{usage} (z390's zCOBOL; Enterprise COBOL writes {ibm}): it is read as {ibm}, the same hexadecimal floating point"))
        } else {
            IWX0027.at(token.pos, format!("{usage} (GnuCOBOL and Micro Focus; Enterprise COBOL writes COMP-1 and COMP-2): it is read as {ibm}, IBM's hexadecimal floating point"))
        };
        token.messages.insert(0, message);
        token.tok = Tok::Word(ibm.into());
        token.spelled = None;
        self.at += 1;
        self.push(token);
    }

    /// PROGRAM-POINTER read as PROCEDURE-POINTER, which IBM sets and CALLs the same way.
    fn program_pointer(&mut self) {
        let mut token = self.tokens[self.at].clone();
        token.messages.insert(0, IWX0044.at(token.pos, "PROGRAM-POINTER (GnuCOBOL and Micro Focus; Enterprise COBOL writes PROCEDURE-POINTER): it is read as PROCEDURE-POINTER, set by SET ... TO ENTRY and called by CALL"));
        token.tok = Tok::Word("PROCEDURE-POINTER".into());
        token.spelled = None;
        self.at += 1;
        self.push(token);
    }

    fn binary_usage(&mut self) -> Result<(), Error> {
        let token = self.tokens[self.at].clone();
        let Tok::Word(usage) = &token.tok else { unreachable!("the caller saw a word") };
        let Some(found) = BINARY_USAGES.iter().find(|u| u.word == usage) else { unreachable!("the caller saw a binary usage") };
        let mut messages = Vec::new();
        if self.out.last().is_some_and(|t| t.tok == Tok::Word("IS".into())) && self.out.len() >= 2 && self.out[self.out.len() - 2].tok == Tok::Word("USAGE".into()) {
            messages.extend(self.out.pop().map(|t| t.messages).unwrap_or_default());
        }
        if self.out.last().is_some_and(|t| t.tok == Tok::Word("USAGE".into())) {
            messages.extend(self.out.pop().map(|t| t.messages).unwrap_or_default());
        }
        self.at += 1;
        let signed = match (found.signed, self.tokens.get(self.at).map(|t| &t.tok)) {
            (Some(fixed), _) => fixed,
            (None, Some(Tok::Word(w))) if w == "SIGNED" || w == "UNSIGNED" => {
                self.at += 1;
                w == "SIGNED"
            }
            (None, _) => true,
        };
        let picture = format!("{}{}", if signed { "S" } else { "" }, found.digits);
        let suffix = if found.signed.is_none() && !signed { " UNSIGNED" } else { "" };
        let shown = format!("{}: {usage}{suffix} is read as PIC {picture} COMP-5", found.origin);
        messages.push(IWX0005.at(token.pos, shown));
        messages.extend(token.messages.iter().cloned());
        let made = |tok: Tok, messages: Vec<Error>| Token { tok, pos: token.pos, area_a: false, spelled: None, after_comma: false, messages };
        self.push(made(Tok::Word("PIC".into()), messages));
        self.push(made(Tok::Pic(picture), Vec::new()));
        self.push(made(Tok::Word("COMP-5".into()), Vec::new()));
        Ok(())
    }
}

/// Whether the tokens end with `IDENTIFICATION DIVISION.` or `ID DIVISION.`.
fn ends_with_header(tokens: &[Token]) -> bool {
    matches!(tokens, [.., a, b, c] if matches!(&a.tok, Tok::Word(w) if w == "IDENTIFICATION" || w == "ID") && b.tok == Tok::Word("DIVISION".into()) && c.tok == Tok::Period)
}

/// A constant entry's arithmetic expression, worked exactly and truncated toward zero (ISO 2002
/// 7.3.6.3): numeric literals and numeric constants, + and - of either arity, *, / and parentheses.
fn arithmetic(tokens: &[Token], constants: &HashMap<String, Tok>) -> Option<i128> {
    let mut at = 0;
    let (num, den) = sum(tokens, &mut at, constants)?;
    (at == tokens.len() && den != 0).then(|| num / den)
}

type Ratio = (i128, i128);

fn reduced((num, den): Ratio) -> Option<Ratio> {
    let gcd = |mut a: i128, mut b: i128| {
        while b != 0 {
            (a, b) = (b, a % b);
        }
        a.abs().max(1)
    };
    let g = gcd(num, den);
    let sign = if den < 0 { -1 } else { 1 };
    Some((num / g * sign, (den / g * sign)))
}

fn sum(tokens: &[Token], at: &mut usize, constants: &HashMap<String, Tok>) -> Option<Ratio> {
    let mut left = product(tokens, at, constants)?;
    while let Some(op) = tokens.get(*at).map(|t| &t.tok).filter(|t| matches!(t, Tok::Plus | Tok::Minus)) {
        let minus = *op == Tok::Minus;
        *at += 1;
        let (n, d) = product(tokens, at, constants)?;
        let n = if minus { n.checked_neg()? } else { n };
        left = reduced((left.0.checked_mul(d)?.checked_add(n.checked_mul(left.1)?)?, left.1.checked_mul(d)?))?;
    }
    Some(left)
}

fn product(tokens: &[Token], at: &mut usize, constants: &HashMap<String, Tok>) -> Option<Ratio> {
    let mut left = factor(tokens, at, constants)?;
    while let Some(op) = tokens.get(*at).map(|t| &t.tok).filter(|t| matches!(t, Tok::Star | Tok::Slash)) {
        let divide = *op == Tok::Slash;
        *at += 1;
        let (n, d) = factor(tokens, at, constants)?;
        left = if divide { (left.0.checked_mul(d)?, left.1.checked_mul(n)?) } else { (left.0.checked_mul(n)?, left.1.checked_mul(d)?) };
        if left.1 == 0 {
            return None;
        }
        left = reduced(left)?;
    }
    Some(left)
}

fn factor(tokens: &[Token], at: &mut usize, constants: &HashMap<String, Tok>) -> Option<Ratio> {
    let tok = &tokens.get(*at)?.tok;
    *at += 1;
    match tok {
        Tok::Plus => factor(tokens, at, constants),
        Tok::Minus => factor(tokens, at, constants).and_then(|(n, d)| Some((n.checked_neg()?, d))),
        Tok::LParen => {
            let inner = sum(tokens, at, constants)?;
            (tokens.get(*at)?.tok == Tok::RParen).then(|| *at += 1)?;
            Some(inner)
        }
        Tok::Number(text) => decimal(text),
        Tok::Word(name) => match constants.get(name)? {
            Tok::Number(text) => decimal(text),
            _ => None,
        },
        _ => None,
    }
}

/// A numeric literal as written, as a fraction over a power of ten.
fn decimal(text: &str) -> Option<Ratio> {
    let (whole, fraction) = text.split_once(['.', ',']).unwrap_or((text, ""));
    let digits: String = format!("{whole}{fraction}").chars().filter(|c| *c != '+').collect();
    reduced((digits.parse().ok()?, 10i128.checked_pow(fraction.len() as u32)?))
}

/// A directive constant's literal as the token it stands for: a quoted literal's characters, its
/// doubled quotes single, or a number as written.
fn directed_value(literal: &str) -> Tok {
    match literal.chars().next() {
        Some(q @ ('"' | '\'')) => Tok::Alnum(literal[1..literal.len().saturating_sub(1).max(1)].replace(&format!("{q}{q}"), &q.to_string())),
        _ => Tok::Number(literal.to_owned()),
    }
}

/// `(name)` in a PICTURE, where `name` is a constant whose value is an unsigned integer, with the
/// integer in its place.
fn picture(text: &str, constants: &HashMap<String, Tok>) -> Option<String> {
    let (mut out, mut rest, mut changed) = (String::new(), text, false);
    while let Some(open) = rest.find('(') {
        let Some(close) = rest[open..].find(')').map(|c| open + c) else { break };
        match constants.get(&rest[open + 1..close]) {
            Some(Tok::Number(n)) if n.bytes().all(|b| b.is_ascii_digit()) => {
                out.push_str(&rest[..=open]);
                out.push_str(n);
                out.push(')');
                changed = true;
            }
            _ => out.push_str(&rest[..=close]),
        }
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    changed.then_some(out)
}

/// `left & right` as one literal, at `left`'s place. Alphanumeric and hexadecimal literals join
/// into an alphanumeric one, a hexadecimal one's bytes read in the program's code page, two
/// hexadecimal literals into a hexadecimal one, and national literals into a national one.
fn join(left: Token, amp: Token, right: Token, decode: impl Fn(&[u8]) -> String) -> Result<Token, Error> {
    let tok = match (&left.tok, &right.tok) {
        (Tok::Alnum(a), Tok::Alnum(b)) => Tok::Alnum(format!("{a}{b}")),
        (Tok::Hex(a), Tok::Hex(b)) => Tok::Hex([a.as_slice(), b].concat()),
        (Tok::Alnum(a), Tok::Hex(b)) => Tok::Alnum(format!("{a}{}", decode(b))),
        (Tok::Hex(a), Tok::Alnum(b)) => Tok::Alnum(format!("{}{b}", decode(a))),
        (Tok::National(a), Tok::National(b)) => Tok::National(format!("{a}{b}")),
        _ => return Err(crate::messages::IWS0016.at(amp.pos, "& joins two alphanumeric or hexadecimal literals, or two national literals, either of which may be a level-78 constant standing for one")),
    };
    let mut messages = left.messages;
    messages.push(IWX0004.at(amp.pos, format!("{CONCATENATION}: the literals on either side are one literal")));
    messages.extend(amp.messages);
    messages.extend(right.messages);
    Ok(Token { tok, pos: left.pos, area_a: left.area_a, spelled: None, after_comma: left.after_comma, messages })
}

#[cfg(test)]
mod tests {
    use crate::ast::{Literal, Usage};
    use crate::copy::Libraries;
    use numeric::Compliance;

    fn source(data: &str, procedure: &str) -> String {
        format!("       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n{data}       PROCEDURE DIVISION.\n{procedure}           GOBACK.\n")
    }

    fn extended(text: &str) -> Result<crate::ast::Program, crate::Error> {
        crate::parse_with(text, &Libraries::default().with_compliance(Compliance::Extended))
    }

    #[test]
    fn a_constant_stands_for_its_value_in_pictures_values_occurs_and_statements() {
        let data = concat!(
            "       78  MAX-LEN VALUE 3.\n",
            "       78  GREETING IS GLOBAL VALUE IS 'AB' & X'C1'.\n",
            "       01  QUOTED CONSTANT AS 'Q'.\n",
            "       01  NOTHING CONSTANT GLOBAL SPACE.\n",
            "       01  X PIC X(MAX-LEN) VALUE GREETING.\n",
            "       01  G.\n",
            "           05 T OCCURS MAX-LEN TIMES PIC X VALUE NOTHING.\n",
        );
        let p = extended(&source(data, "           MOVE QUOTED TO X.\n")).unwrap();
        let names: Vec<&str> = p.working_storage.iter().filter_map(|e| e.name.as_deref()).collect();
        assert_eq!(names, ["X", "G", "T"]);
        let x = &p.working_storage[0];
        assert_eq!((x.picture.as_deref(), x.value.clone()), (Some("X(3)"), Some(Literal::Alnum("ABA".into()))));
        assert_eq!(p.working_storage[2].occurs, Some(3));
        let warnings: Vec<(u32, String)> = p.messages.iter().map(|m| (m.pos.line, m.message.clone())).collect();
        assert_eq!(warnings.len(), 5, "{warnings:?}");
        assert!(warnings[..2].iter().all(|(_, m)| m.starts_with(super::CONSTANT)) && warnings[0].0 == 5 && warnings[0].1.ends_with("MAX-LEN stands for its value wherever it is used after this entry"));
        assert!(warnings[2].1.starts_with(super::CONCATENATION) && warnings[2].0 == 6);
        assert!(warnings[3..].iter().all(|(_, m)| m.starts_with(super::CONSTANT)));
        assert!(format!("{:?}", p.paragraphs[0].statements[0]).contains("Alnum(\"Q\")"));
    }

    #[test]
    fn a_constant_whose_value_is_missing_or_an_expression_of_other_operands_is_refused_by_name() {
        let refused = |data: &str| extended(&source(data, "")).unwrap_err().message;
        assert_eq!(refused("       78  N VALUE 1 + B.\n"), "constant N: an arithmetic expression of numeric literals and numeric constants defined before, with +, -, *, / and parentheses");
        assert_eq!(refused("       78  N PIC 9 VALUE 1.\n"), "constant N: a level-78 entry is VALUE and its value, then a period");
        assert!(refused("       01  N PIC X VALUE 'A' & B.\n").starts_with("& joins two alphanumeric or hexadecimal literals"));
        assert!(refused("       01  N PIC X VALUE 'A' & N'B'.\n").starts_with("& joins two alphanumeric or hexadecimal literals"));
    }

    #[test]
    fn directive_constants_expressions_ascii_repository_programs_and_header_forms_are_read() {
        let text = concat!(
            "       IDENTIFICATION DIVISION.\n",
            "       PROGRAM-ID. T.\n",
            "       ENVIRONMENT DIVISION.\n",
            "       CONFIGURATION SECTION.\n",
            "       SPECIAL-NAMES.\n",
            "           ALPHABET A-SEQ IS ASCII.\n",
            "       REPOSITORY.\n",
            "           PROGRAM SUB AS 'subprog'.\n",
            "       >>DEFINE CONSTANT LIM AS 3\n",
            "      $SET CONSTANT GREET \"HI\"\n",
            "       >>DEFINE CONSTANT LIM AS 4\n",
            "       LINKAGE SECTION.\n",
            "       01 A PIC X(LIM).\n",
            "       01 B PIC X.\n",
            "       78 N VALUE 7 * (2 + 1) / 2.\n",
            "       PROCEDURE DIVISION USING A\n",
            "           OPTIONAL\n",
            "       B.\n",
            "           MOVE GREET TO A\n",
            "           CALL SUB\n",
            "           DISPLAY N.\n",
        );
        let p = extended(text).unwrap();
        let mut ids: Vec<&str> = p.messages.iter().filter_map(|m| m.id).filter(|id| id.starts_with("IWX006") || id.starts_with("IWX007")).collect();
        ids.sort_unstable();
        assert_eq!(ids, ["IWX0067", "IWX0067", "IWX0067", "IWX0068", "IWX0069", "IWX0070", "IWX0071", "IWX0072", "IWX0074"]);
        assert_eq!(p.linkage[0].picture.as_deref(), Some("X(3)"), "a constant defined again without OVERRIDE keeps its first value");
        assert_eq!(p.using.iter().map(|u| u.name.as_str()).collect::<Vec<_>>(), ["A", "B"]);
        let statements = format!("{:?}", p.paragraphs[0].statements);
        assert!(statements.contains("Alnum(\"HI\")") && statements.contains("Alnum(\"subprog\")") && statements.contains("Number(\"10\")"), "{statements}");
        assert!(crate::parse(text).is_err());
        let function = "       FUNCTION-ID. TWICE.\n       DATA DIVISION.\n       LINKAGE SECTION.\n       01 R PIC 9.\n       PROCEDURE DIVISION RETURNING R.\n           MOVE 2 TO R\n           GOBACK.\n       END FUNCTION TWICE.\n";
        let f = crate::parse_all_with(function, &Libraries::default().with_compliance(Compliance::Extended)).unwrap();
        assert!(f[0].messages.iter().any(|m| m.id == Some("IWX0073")), "{:?}", f[0].messages);
    }

    #[test]
    fn the_binary_usages_are_comp_5_pictures_and_binary_char_is_left_to_the_parser() {
        let data = "       01  A USAGE IS BINARY-LONG.\n       01  B BINARY-SHORT UNSIGNED VALUE 7.\n       01  C BINARY-DOUBLE SIGNED.\n";
        let p = extended(&source(data, "")).unwrap();
        let read: Vec<(Option<&str>, Option<Usage>)> = p.working_storage.iter().map(|e| (e.picture.as_deref(), e.usage)).collect();
        assert_eq!(read, [(Some("S9(9)"), Some(Usage::NativeBinary)), (Some("9(4)"), Some(Usage::NativeBinary)), (Some("S9(18)"), Some(Usage::NativeBinary))]);
        assert_eq!(p.working_storage[1].value, Some(Literal::Number("7".into())));
        let shown: Vec<String> = p.messages.iter().map(|m| m.message.clone()).collect();
        assert_eq!(shown[1], format!("{}: BINARY-SHORT UNSIGNED is read as PIC 9(4) COMP-5", super::BINARY_USAGE));
        let data = "       01  F UNSIGNED-INT.\n       01  G SIGNED-SHORT.\n       01  H BINARY-LONG-LONG UNSIGNED.\n       01  I UNSIGNED-LONG.\n       01  J BINARY-INT.\n       01  K BINARY-INT UNSIGNED.\n";
        let p = extended(&source(data, "")).unwrap();
        let read: Vec<(Option<&str>, Option<Usage>)> = p.working_storage.iter().map(|e| (e.picture.as_deref(), e.usage)).collect();
        let binary = |p: &'static str| (Some(p), Some(Usage::NativeBinary));
        assert_eq!(read, [binary("9(9)"), binary("S9(4)"), binary("9(18)"), binary("9(18)"), binary("S9(9)"), binary("9(9)")]);
        assert_eq!(p.messages[0].message, format!("{}: UNSIGNED-INT is read as PIC 9(9) COMP-5", super::GNUCOBOL_BINARY_USAGE));
        let p = extended(&source("       01  D BINARY-CHAR UNSIGNED.\n       01  E USAGE BINARY-CHAR SIGNED.\n", "")).unwrap();
        let read: Vec<(Option<&str>, Option<Usage>)> = p.working_storage.iter().map(|e| (e.picture.as_deref(), e.usage)).collect();
        assert_eq!(read, [(None, Some(Usage::BinaryChar { signed: false })), (None, Some(Usage::BinaryChar { signed: true }))]);
    }

    #[test]
    fn a_name_and_a_period_after_a_period_in_area_b_is_a_paragraph_header() {
        let procedure = "           MAIN-PARA.\n           PERFORM 100-STEP\n           MOVE A TO B C.\n           100-STEP.\n           DISPLAY A.\n";
        let text = source("       01 A PIC X.\n       01 B PIC X.\n       01 C PIC X.\n", procedure);
        let p = extended(&text).unwrap();
        let names: Vec<&str> = p.paragraphs.iter().map(|q| q.name.as_str()).collect();
        assert_eq!(names, ["MAIN-PARA", "100-STEP"]);
        assert!(matches!(&p.paragraphs[0].statements[1], crate::ast::Stmt::Move { to, .. } if to.len() == 2));
        let warned: Vec<(u32, Option<&str>)> = p.messages.iter().filter(|m| m.id == Some("IWX0061")).map(|m| (m.pos.line, m.id)).collect();
        assert_eq!(warned, [(9, Some("IWX0061")), (12, Some("IWX0061"))]);
        assert!(crate::parse_with(&text, &Libraries::default()).is_err());
    }

    #[test]
    fn returning_omitted_leaves_a_programs_header_and_nothing_else() {
        let header = "       PROCEDURE DIVISION USING A RETURNING OMITTED.\n";
        let text = format!("       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       DATA DIVISION.\n       LINKAGE SECTION.\n       01 A PIC X.\n{header}           CALL 'S' RETURNING OMITTED.\n           GOBACK.\n");
        let p = extended(&text).unwrap();
        assert_eq!((p.using.len(), p.returning.as_deref()), (1, None));
        let shown: Vec<(u32, u32, &str)> = p.messages.iter().map(|m| (m.pos.line, m.pos.col, m.message.as_str())).collect();
        let warning = format!("{}: the program is read with no RETURNING phrase, and returns its RETURN-CODE to its caller as any program does", super::RETURNING_OMITTED);
        assert_eq!(shown, [(6, header.find("RETURNING").unwrap() as u32 + 1, warning.as_str())]);
        assert!(format!("{:?}", p.paragraphs[0].statements[0]).contains("\"OMITTED\""), "a CALL's RETURNING OMITTED is not the header's");
        assert_eq!(crate::parse(&text).unwrap().returning.as_deref(), Some("OMITTED"));
        let function = "       IDENTIFICATION DIVISION.\n       FUNCTION-ID. F.\n       PROCEDURE DIVISION RETURNING OMITTED.\n           GOBACK.\n       END FUNCTION F.\n";
        let f = extended(function).unwrap();
        assert_eq!((f.returning.as_deref(), f.messages.len()), (Some("OMITTED"), 0), "a function returns an item");
    }

    #[test]
    fn strict_reads_none_of_it() {
        let strict = |data: &str| crate::parse(&source(data, ""));
        assert_eq!(strict("       01  N PIC X(2) VALUE 'A' & 'B'.\n").unwrap_err().message, "literal concatenation with & is not Enterprise COBOL's");
        assert_eq!(strict("       78  N VALUE 1.\n").unwrap().working_storage[0].level, 78);
        assert!(strict("       01  A BINARY-LONG.\n").unwrap_err().message.contains("BINARY-LONG is not a data description clause"));
    }
}
