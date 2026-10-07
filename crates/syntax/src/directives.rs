//! The compiler directives `--compliance extended` reads besides the source format: the COBOL 2002
//! conditional compilation directives (>>DEFINE, >>IF, >>ELIF, >>ELSE, >>END-IF), GnuCOBOL's
//! debugging line >>D, and >>TURN, >>LISTING, >>PAGE and a word after >>D that is no directive,
//! read and of no effect.

use crate::messages::{IWC0318, IWX0048, IWX0049, IWX0050};
use crate::{Error, Pos};
use std::collections::HashMap;

/// A directive line other than a source-format one.
pub enum Directive {
    Define { name: String, value: Option<String> },
    If(Vec<String>),
    Elif(Vec<String>),
    Else,
    EndIf,
    /// >>TURN, >>LISTING or >>PAGE, and its warning.
    Ignored(Error),
    /// >>D: the rest of the line, a debugging line's text, and its warning.
    Debugging(Vec<char>, Error),
}

/// The directive on `chars`, None for any other line and for >>SOURCE and >>SET, which the source
/// reader takes.
pub fn read(chars: &[char], pos: Pos) -> Option<Result<Directive, Error>> {
    let start = chars.iter().position(|c| *c != ' ')?;
    let rest = chars[start..].strip_prefix(&['>', '>'])?;
    let pos = Pos { col: start as u32 + 1, ..pos };
    let words = words(&rest.iter().collect::<String>());
    let first = words.first()?.to_ascii_uppercase();
    let shown: String = chars[start..].iter().collect::<String>().trim_end().to_owned();
    let tail = || words[1..].to_vec();
    Some(Ok(match first.as_str() {
        "SOURCE" | "SET" => return None,
        "DEFINE" => match define(&words[1..]) {
            Some((name, value)) => Directive::Define { name, value },
            None => return Some(Err(IWC0318.at(pos, format!("{shown}: >>DEFINE takes a name, then AS and a literal or OFF")))),
        },
        "IF" => Directive::If(tail()),
        "ELIF" => Directive::Elif(tail()),
        "ELSE" if words.len() == 1 => Directive::Else,
        "END-IF" if words.len() == 1 => Directive::EndIf,
        "TURN" | "LISTING" | "PAGE" => Directive::Ignored(IWX0049.at(pos, format!(">>{first} (COBOL 2002 and GnuCOBOL; Enterprise COBOL has no such directive): it is read and has no effect"))),
        // cobc 3.2 ignores a word it does not know after >>D, as >>DMOVE, as an invalid directive.
        d if d.starts_with('D') && d.len() > 1 && d != "DEFINE" && d != "DISPLAY" => Directive::Ignored(IWX0049.at(pos, format!(">>{first} (COBOL 2002 and GnuCOBOL; Enterprise COBOL has no such directive): it is read and has no effect"))),
        "D" => {
            let after = rest.iter().position(|c| *c != ' ').map_or(rest.len(), |k| k + 1);
            let warning = IWX0050.at(pos, ">>D (GnuCOBOL; Enterprise COBOL marks a debugging line with D in column 7): the line is a debugging line, compiled only WITH DEBUGGING MODE");
            Directive::Debugging(rest[after..].to_vec(), warning)
        }
        _ => return None,
    }))
}

/// `name [AS] {literal | OFF}`, and a value of None for OFF; PARAMETER, a value from the
/// compiler's options, which ironwork has none of, also leaves the name undefined.
fn define(words: &[String]) -> Option<(String, Option<String>)> {
    let (name, rest) = words.split_first()?;
    if name.eq_ignore_ascii_case("CONSTANT") {
        return None;
    }
    let rest: Vec<&String> = rest.iter().filter(|w| !w.eq_ignore_ascii_case("AS") && !w.eq_ignore_ascii_case("OVERRIDE")).collect();
    let value = match rest.as_slice() {
        [] => Some(String::new()),
        [w] if w.eq_ignore_ascii_case("OFF") || w.eq_ignore_ascii_case("PARAMETER") => None,
        [w] => Some(unquote(w)),
        _ => return None,
    };
    Some((name.to_ascii_uppercase(), value))
}

/// The words of a directive, a quoted literal one word with its quotes.
fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for c in text.chars() {
        match quote {
            Some(q) if c == q => {
                current.push(c);
                quote = None;
            }
            Some(_) => current.push(c),
            None if c == '\'' || c == '"' => {
                current.push(c);
                quote = Some(c);
            }
            None if c == ' ' => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            None if matches!(c, '=' | '<' | '>') => {
                if !current.is_empty() && !current.ends_with(['<', '>']) {
                    out.push(std::mem::take(&mut current));
                }
                current.push(c);
            }
            None => {
                if current.ends_with(['=', '<', '>']) {
                    out.push(std::mem::take(&mut current));
                }
                current.push(c);
            }
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn unquote(word: &str) -> String {
    word.trim_matches(|c| c == '\'' || c == '"').to_owned()
}

/// Which lines are compiled: the names >>DEFINE has defined and the >>IF levels open.
#[derive(Default)]
pub struct Conditions {
    defined: HashMap<String, String>,
    open: Vec<Branch>,
}

struct Branch {
    /// The lines around the >>IF are compiled.
    outer: bool,
    /// A branch of this >>IF was taken.
    taken: bool,
    active: bool,
    pos: Pos,
}

impl Conditions {
    /// The lines here are compiled.
    pub fn active(&self) -> bool {
        self.open.last().is_none_or(|b| b.active)
    }

    /// Takes a conditional directive, with its warning in `notes` where the lines around it are
    /// compiled.
    pub fn apply(&mut self, directive: Directive, pos: Pos, notes: &mut Vec<Error>) -> Result<(), Error> {
        let note = |what: &str| IWX0048.at(pos, format!("{what} (COBOL 2002 and GnuCOBOL; Enterprise COBOL 6.3 has it too): {}", match what {
            ">>DEFINE" => "the name is defined for the >>IF directives after it",
            _ => "the lines it chooses are compiled and the others are not",
        }));
        match directive {
            Directive::Define { name, value } => {
                if self.active() {
                    notes.push(note(">>DEFINE"));
                    match value {
                        Some(v) => self.defined.insert(name, v),
                        None => self.defined.remove(&name),
                    };
                }
            }
            Directive::If(condition) => {
                let outer = self.active();
                if outer {
                    notes.push(note(">>IF"));
                }
                let taken = outer && self.holds(&condition, pos)?;
                self.open.push(Branch { outer, taken, active: taken, pos });
            }
            Directive::Elif(condition) => {
                let holds = self.holds(&condition, pos)?;
                let branch = self.open.last_mut().ok_or_else(|| IWC0318.at(pos, ">>ELIF with no >>IF before it"))?;
                branch.active = branch.outer && !branch.taken && holds;
                branch.taken |= branch.active;
            }
            Directive::Else => {
                let branch = self.open.last_mut().ok_or_else(|| IWC0318.at(pos, ">>ELSE with no >>IF before it"))?;
                branch.active = branch.outer && !branch.taken;
                branch.taken = true;
            }
            Directive::EndIf => {
                self.open.pop().ok_or_else(|| IWC0318.at(pos, ">>END-IF with no >>IF before it"))?;
            }
            Directive::Ignored(_) | Directive::Debugging(..) => {}
        }
        Ok(())
    }

    /// An >>IF left open at the end of the source.
    pub fn finish(&self) -> Result<(), Error> {
        match self.open.last() {
            Some(b) => Err(IWC0318.at(b.pos, ">>IF with no >>END-IF after it")),
            None => Ok(()),
        }
    }

    /// Whether `condition` holds: terms joined by AND and OR, left to right, each `[NOT] name
    /// [IS] [NOT] DEFINED` or a comparison of a defined name with a literal. A comparison naming
    /// an undefined name is false.
    fn holds(&self, condition: &[String], pos: Pos) -> Result<bool, Error> {
        let bad = || IWC0318.at(pos, format!(">>IF {}: a condition of DEFINED tests and comparisons ironwork does not read", condition.join(" ")));
        let upper: Vec<String> = condition.iter().map(|w| if w.starts_with(['\'', '"']) { w.clone() } else { w.to_ascii_uppercase() }).collect();
        let mut result: Option<bool> = None;
        let mut join = "";
        let mut i = 0;
        while i < upper.len() {
            let mut negated = false;
            while upper.get(i).is_some_and(|w| w == "NOT") {
                negated = !negated;
                i += 1;
            }
            let name = upper.get(i).ok_or_else(bad)?;
            i += 1;
            if upper.get(i).is_some_and(|w| w == "IS") {
                i += 1;
            }
            while upper.get(i).is_some_and(|w| w == "NOT") {
                negated = !negated;
                i += 1;
            }
            let term = if upper.get(i).is_some_and(|w| w == "DEFINED") {
                i += 1;
                self.defined.contains_key(name)
            } else {
                let (op, len) = relation(&upper[i..]).ok_or_else(bad)?;
                i += len;
                let value = upper.get(i).ok_or_else(bad)?;
                i += 1;
                let literal = if value.starts_with(['\'', '"']) { unquote(&condition[i - 1]) } else { self.defined.get(value).cloned().unwrap_or_else(|| value.clone()) };
                self.defined.get(name).is_some_and(|defined| compare(defined, &literal, op))
            } != negated;
            result = Some(match (result, join) {
                (None, _) => term,
                (Some(r), "AND") => r && term,
                (Some(r), _) => r || term,
            });
            join = match upper.get(i).map(String::as_str) {
                Some(w @ ("AND" | "OR")) => {
                    i += 1;
                    if w == "AND" { "AND" } else { "OR" }
                }
                Some(_) => return Err(bad()),
                None => "",
            };
        }
        result.ok_or_else(bad)
    }
}

#[derive(Clone, Copy)]
enum Op {
    Eq,
    Lt,
    Gt,
    Le,
    Ge,
}

/// A relational operator at the start of `words`, and how many words it takes.
fn relation(words: &[String]) -> Option<(Op, usize)> {
    let w = |k: usize| words.get(k).map(String::as_str);
    Some(match (w(0)?, w(1), w(2), w(3)) {
        ("=", ..) => (Op::Eq, 1),
        ("<", ..) => (Op::Lt, 1),
        (">", ..) => (Op::Gt, 1),
        ("<=", ..) => (Op::Le, 1),
        (">=", ..) => (Op::Ge, 1),
        ("EQUAL", Some("TO"), ..) => (Op::Eq, 2),
        ("EQUAL", ..) => (Op::Eq, 1),
        ("GREATER", Some("THAN"), Some("OR"), Some("EQUAL")) => (Op::Ge, 4 + usize::from(w(4) == Some("TO"))),
        ("LESS", Some("THAN"), Some("OR"), Some("EQUAL")) => (Op::Le, 4 + usize::from(w(4) == Some("TO"))),
        ("GREATER", Some("THAN"), ..) => (Op::Gt, 2),
        ("LESS", Some("THAN"), ..) => (Op::Lt, 2),
        ("GREATER", ..) => (Op::Gt, 1),
        ("LESS", ..) => (Op::Lt, 1),
        _ => return None,
    })
}

/// Two values compared as numbers where both are, else as text.
fn compare(a: &str, b: &str, op: Op) -> bool {
    let order = match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) => x.partial_cmp(&y),
        _ => Some(a.cmp(b)),
    };
    let Some(order) = order else { return false };
    match op {
        Op::Eq => order.is_eq(),
        Op::Lt => order.is_lt(),
        Op::Gt => order.is_gt(),
        Op::Le => order.is_le(),
        Op::Ge => order.is_ge(),
    }
}
