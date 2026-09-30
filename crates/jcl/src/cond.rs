//! COND on JOB and EXEC statements and the relational expression of an IF statement, read and
//! decided against the steps that ran before.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Gt,
    Ge,
    Eq,
    Ne,
    Lt,
    Le,
}

impl Op {
    pub fn holds(self, left: u16, right: u16) -> bool {
        match self {
            Op::Gt => left > right,
            Op::Ge => left >= right,
            Op::Eq => left == right,
            Op::Ne => left != right,
            Op::Lt => left < right,
            Op::Le => left <= right,
        }
    }

    fn word(self) -> &'static str {
        match self {
            Op::Gt => "GT",
            Op::Ge => "GE",
            Op::Eq => "EQ",
            Op::Ne => "NE",
            Op::Lt => "LT",
            Op::Le => "LE",
        }
    }
}

/// `stepname` or `stepname.procstepname`. Inside a procedure a bare stepname is one of the
/// procedure's own steps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepRef {
    pub step: String,
    pub procstep: Option<String>,
}

impl StepRef {
    fn parse(text: &str) -> Result<StepRef, String> {
        let (step, procstep) = match text.split_once('.') {
            Some((s, p)) => (s, Some(p)),
            None => (text, None),
        };
        for n in std::iter::once(step).chain(procstep) {
            if !crate::is_name(n) {
                return Err(format!("{n} is not a step name"));
            }
        }
        Ok(StepRef { step: step.to_string(), procstep: procstep.map(str::to_string) })
    }

    fn shown(&self) -> String {
        match &self.procstep {
            Some(p) => format!("{}.{p}", self.step),
            None => self.step.clone(),
        }
    }

    /// The steps this names, latest first. A bare stepname that names no step where it is used
    /// but called a procedure names every step of that call.
    fn find<'r>(&self, ran: &'r [Ran], context: Option<&str>) -> Vec<&'r Ran> {
        let named = |r: &&Ran| match &self.procstep {
            Some(p) => r.caller.as_deref() == Some(&self.step) && r.name.as_deref() == Some(p),
            None => r.caller.as_deref() == context && r.name.as_deref() == Some(&self.step),
        };
        if let Some(r) = ran.iter().rev().find(named) {
            return vec![r];
        }
        if self.procstep.is_none() && context.is_none() {
            return ran.iter().rev().filter(|r| r.caller.as_deref() == Some(&self.step)).collect();
        }
        Vec::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Test {
    pub code: u16,
    pub op: Op,
    pub step: Option<StepRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Plain,
    Even,
    Only,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cond {
    pub tests: Vec<Test>,
    pub mode: Mode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Compare(Value, Op, u16),
    Abend(Option<StepRef>),
    AbendCc(String),
    Run(StepRef),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// RC alone: the highest return code of the steps that ran.
    MaxRc,
    StepRc(StepRef),
}

/// A step that ran, in order: its return code, or its abend code when it abended. `caller` is
/// the job step that called the procedure it is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ran {
    pub name: Option<String>,
    pub caller: Option<String>,
    pub rc: Option<u16>,
    pub abend: Option<String>,
}

fn code(text: &str) -> Result<u16, String> {
    match text.parse::<u16>() {
        Ok(n) if n <= 4095 && !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) => Ok(n),
        _ => Err(format!("{text} is not a return code from 0 to 4095")),
    }
}

fn cond_op(text: &str) -> Result<Op, String> {
    Ok(match text {
        "GT" => Op::Gt,
        "GE" => Op::Ge,
        "EQ" => Op::Eq,
        "NE" => Op::Ne,
        "LT" => Op::Lt,
        "LE" => Op::Le,
        _ => return Err(format!("{text} is not a COND operator (GT, GE, EQ, NE, LT or LE)")),
    })
}

fn cond_test(text: &str) -> Result<Test, String> {
    let parts: Vec<&str> = text.split(',').collect();
    if !(2..=3).contains(&parts.len()) {
        return Err(format!("COND test ({text}) is not (code,operator) or (code,operator,stepname)"));
    }
    let step = parts.get(2).map(|s| StepRef::parse(s)).transpose()?;
    Ok(Test { code: code(parts[0])?, op: cond_op(parts[1])?, step })
}

/// Splits `text` at commas outside parentheses.
fn top_level(text: &str) -> Vec<&str> {
    let (mut out, mut depth, mut start) = (Vec::new(), 0i32, 0);
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&text[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&text[start..]);
    out
}

/// The value of COND=: EVEN, ONLY, one test, or a list of up to eight tests that may end with
/// EVEN or ONLY.
pub fn parse_cond(text: &str) -> Result<Cond, String> {
    match text {
        "EVEN" => return Ok(Cond { tests: Vec::new(), mode: Mode::Even }),
        "ONLY" => return Ok(Cond { tests: Vec::new(), mode: Mode::Only }),
        _ => {}
    }
    let Some(inner) = text.strip_prefix('(').and_then(|t| t.strip_suffix(')')) else { return Err(format!("COND={text} is not in parentheses")) };
    if !inner.starts_with('(') {
        return Ok(Cond { tests: vec![cond_test(inner)?], mode: Mode::Plain });
    }
    let items = top_level(inner);
    let mut cond = Cond::default();
    for (i, item) in items.iter().enumerate() {
        match *item {
            "EVEN" | "ONLY" if i + 1 == items.len() => cond.mode = if *item == "EVEN" { Mode::Even } else { Mode::Only },
            "EVEN" | "ONLY" => return Err(format!("{item} must come last in COND")),
            t => match t.strip_prefix('(').and_then(|t| t.strip_suffix(')')) {
                Some(t) => cond.tests.push(cond_test(t)?),
                None => return Err(format!("{t} in COND is not a test in parentheses")),
            },
        }
    }
    if cond.tests.len() > 8 {
        return Err("COND takes at most eight tests".into());
    }
    Ok(cond)
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Open,
    Close,
    And,
    Or,
    Not,
    Op(Op),
    Word(String),
}

fn word_char(c: char) -> bool {
    c.is_ascii_uppercase() || c.is_ascii_digit() || matches!(c, '#' | '$' | '@' | '.')
}

/// Keywords that take `=value` as part of themselves rather than as a comparison.
fn takes_value(word: &str) -> bool {
    word == "ABEND" || word == "ABENDCC" || word.ends_with(".ABEND") || word.ends_with(".RUN")
}

fn tokens(text: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = text.chars().collect();
    let (mut out, mut i) = (Vec::new(), 0);
    while i < chars.len() {
        let next = chars.get(i + 1).copied();
        let (token, width) = match chars[i] {
            ' ' => {
                i += 1;
                continue;
            }
            '(' => (Token::Open, 1),
            ')' => (Token::Close, 1),
            '&' => (Token::And, 1),
            '|' => (Token::Or, 1),
            '¬' if next == Some('=') => (Token::Op(Op::Ne), 2),
            '¬' if next == Some('>') => (Token::Op(Op::Le), 2),
            '¬' if next == Some('<') => (Token::Op(Op::Ge), 2),
            '¬' => (Token::Not, 1),
            '>' if next == Some('=') => (Token::Op(Op::Ge), 2),
            '<' if next == Some('=') => (Token::Op(Op::Le), 2),
            '>' => (Token::Op(Op::Gt), 1),
            '<' => (Token::Op(Op::Lt), 1),
            '=' => (Token::Op(Op::Eq), 1),
            c if word_char(c) => {
                let start = i;
                while i < chars.len() && word_char(chars[i]) {
                    i += 1;
                }
                let mut word: String = chars[start..i].iter().collect();
                if chars.get(i) == Some(&'=') && takes_value(&word) {
                    let value_start = i + 1;
                    let mut end = value_start;
                    while end < chars.len() && word_char(chars[end]) {
                        end += 1;
                    }
                    word.push('=');
                    word.extend(&chars[value_start..end]);
                    i = end;
                }
                out.push(match word.as_str() {
                    "AND" => Token::And,
                    "OR" => Token::Or,
                    "NOT" => Token::Not,
                    "GT" => Token::Op(Op::Gt),
                    "GE" | "NL" => Token::Op(Op::Ge),
                    "LT" => Token::Op(Op::Lt),
                    "LE" | "NG" => Token::Op(Op::Le),
                    "EQ" => Token::Op(Op::Eq),
                    "NE" => Token::Op(Op::Ne),
                    _ => Token::Word(word),
                });
                continue;
            }
            c => return Err(format!("{c} has no meaning in an IF expression")),
        };
        out.push(token);
        i += width;
    }
    Ok(out)
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
    depth: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn or(&mut self) -> Result<Expr, String> {
        let mut left = self.and()?;
        while self.peek() == Some(&Token::Or) {
            self.at += 1;
            left = Expr::Or(Box::new(left), Box::new(self.and()?));
        }
        Ok(left)
    }

    fn and(&mut self) -> Result<Expr, String> {
        let mut left = self.unary()?;
        while self.peek() == Some(&Token::And) {
            self.at += 1;
            left = Expr::And(Box::new(left), Box::new(self.unary()?));
        }
        Ok(left)
    }

    /// NOT binds tighter than a comparison, so NOT RC = 0 negates the comparison it begins.
    fn unary(&mut self) -> Result<Expr, String> {
        if self.peek() == Some(&Token::Not) {
            self.at += 1;
            return Ok(Expr::Not(Box::new(self.unary()?)));
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<Expr, String> {
        match self.tokens.get(self.at).cloned() {
            Some(Token::Open) => {
                self.depth += 1;
                if self.depth > 15 {
                    return Err("parentheses in an IF expression nest more than 15 deep".into());
                }
                self.at += 1;
                let inner = self.or()?;
                if self.peek() != Some(&Token::Close) {
                    return Err("an IF expression is missing a )".into());
                }
                self.at += 1;
                self.depth -= 1;
                Ok(inner)
            }
            Some(Token::Word(w)) => {
                self.at += 1;
                self.word(&w)
            }
            Some(t) => Err(format!("an IF expression has {t:?} where a keyword belongs")),
            None => Err("an IF expression ends early".into()),
        }
    }

    fn comparison(&mut self, value: Value) -> Result<Expr, String> {
        let Some(Token::Op(op)) = self.peek().cloned() else { return Err("RC needs a comparison operator and a number".into()) };
        self.at += 1;
        match self.tokens.get(self.at).cloned() {
            Some(Token::Word(n)) => {
                self.at += 1;
                Ok(Expr::Compare(value, op, code(&n)?))
            }
            _ => Err(format!("{} needs a number after it", op.word())),
        }
    }

    fn word(&mut self, w: &str) -> Result<Expr, String> {
        let truth = |base: Expr, value: Option<&str>| match value {
            None | Some("TRUE") => Ok(base),
            Some("FALSE") => Ok(Expr::Not(Box::new(base))),
            Some(v) => Err(format!("{v} is not TRUE or FALSE")),
        };
        let (key, value) = match w.split_once('=') {
            Some((k, v)) => (k, Some(v)),
            None => (w, None),
        };
        let (target, keyword) = key.rsplit_once('.').map_or((None, key), |(t, k)| (Some(t), k));
        let step = || StepRef::parse(target.unwrap_or(""));
        match (target, keyword) {
            (None, "RC") => self.comparison(Value::MaxRc),
            (None, "ABEND") => truth(Expr::Abend(None), value),
            (None, "ABENDCC") => {
                let v = value.unwrap_or("");
                let valid = match v.as_bytes() {
                    [b'S', rest @ ..] => rest.len() == 3 && rest.iter().all(u8::is_ascii_hexdigit),
                    [b'U', rest @ ..] => rest.len() == 4 && rest.iter().all(u8::is_ascii_digit),
                    _ => false,
                };
                if valid { Ok(Expr::AbendCc(v.to_string())) } else { Err(format!("ABENDCC={v} is not Sxxx or Unnnn")) }
            }
            (Some(_), "RC") => self.comparison(Value::StepRc(step()?)),
            (Some(_), "ABEND") => truth(Expr::Abend(Some(step()?)), value),
            (Some(_), "RUN") => truth(Expr::Run(step()?), value),
            (Some(_), "ABENDCC") => Err("stepname.ABENDCC is not supported yet".into()),
            _ => Err(format!("{w} is not RC, ABEND, ABENDCC or a stepname.RC, .ABEND or .RUN")),
        }
    }
}

/// The relational expression between IF and THEN. NOT binds first, then the comparisons, then
/// AND, then OR.
pub fn parse_expr(text: &str) -> Result<Expr, String> {
    let mut p = Parser { tokens: tokens(text)?, at: 0, depth: 0 };
    let expr = p.or()?;
    if p.at != p.tokens.len() {
        return Err(format!("an IF expression has {:?} after its end", p.tokens[p.at]));
    }
    Ok(expr)
}

/// Why a step is bypassed, or None when it runs. After an abend a step runs only under EVEN or
/// ONLY; ONLY runs only after one. A test naming a step that did not run, or abended, is not
/// made; a test naming none is made against every step that ran.
/// `context` is the job step whose procedure the step is in.
pub fn bypassed_by_cond(cond: &Cond, ran: &[Ran], abended: bool, context: Option<&str>) -> Option<String> {
    match (cond.mode, abended) {
        (Mode::Plain, true) => return Some("an earlier step abended".into()),
        (Mode::Only, false) => return Some("COND=ONLY and no earlier step abended".into()),
        _ => {}
    }
    for t in &cond.tests {
        let hit = match &t.step {
            Some(s) => s.find(ran, context).iter().filter_map(|r| r.rc).any(|rc| t.op.holds(t.code, rc)),
            None => ran.iter().filter_map(|r| r.rc).any(|rc| t.op.holds(t.code, rc)),
        };
        if hit {
            let step = t.step.as_ref().map_or(String::new(), |s| format!(",{}", s.shown()));
            return Some(format!("COND=({},{}{step}) is true", t.code, t.op.word()));
        }
    }
    None
}

/// A stepname.RC that names a call of a procedure is the highest return code of its steps.
pub fn eval(expr: &Expr, ran: &[Ran], context: Option<&str>) -> bool {
    match expr {
        Expr::Not(e) => !eval(e, ran, context),
        Expr::And(a, b) => eval(a, ran, context) && eval(b, ran, context),
        Expr::Or(a, b) => eval(a, ran, context) || eval(b, ran, context),
        Expr::Compare(Value::MaxRc, op, n) => op.holds(ran.iter().filter_map(|r| r.rc).max().unwrap_or(0), *n),
        Expr::Compare(Value::StepRc(s), op, n) => {
            let found = s.find(ran, context);
            !found.is_empty() && found.iter().all(|r| r.rc.is_some()) && op.holds(found.iter().filter_map(|r| r.rc).max().unwrap_or(0), *n)
        }
        Expr::Abend(None) => ran.iter().any(|r| r.abend.is_some()),
        Expr::Abend(Some(s)) => s.find(ran, context).iter().any(|r| r.abend.is_some()),
        Expr::AbendCc(c) => ran.iter().any(|r| r.abend.as_deref() == Some(c)),
        Expr::Run(s) => !s.find(ran, context).is_empty(),
    }
}

/// Whether the expression asks about abends or whether a step ran: the steps it governs may
/// run after an abend.
pub fn tests_abend(expr: &Expr) -> bool {
    match expr {
        Expr::Not(e) => tests_abend(e),
        Expr::And(a, b) | Expr::Or(a, b) => tests_abend(a) || tests_abend(b),
        Expr::Compare(..) => false,
        Expr::Abend(_) | Expr::AbendCc(_) | Expr::Run(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ran(name: &str, rc: Option<u16>, abend: Option<&str>) -> Ran {
        Ran { name: Some(name.into()), caller: None, rc, abend: abend.map(Into::into) }
    }

    #[test]
    fn cond_forms() {
        assert_eq!(parse_cond("EVEN").unwrap().mode, Mode::Even);
        assert_eq!(parse_cond("(4,LT)").unwrap().tests, [Test { code: 4, op: Op::Lt, step: None }]);
        assert_eq!(parse_cond("(4,LT,STEP1.PSTEP)").unwrap().tests[0].step, Some(StepRef { step: "STEP1".into(), procstep: Some("PSTEP".into()) }));
        let c = parse_cond("((4,LT),(8,GT,STEP1),EVEN)").unwrap();
        assert_eq!((c.tests.len(), c.mode), (2, Mode::Even));
        assert_eq!(parse_cond("((0,NE,S1),ONLY)").unwrap().mode, Mode::Only);
        for bad in ["(4,LT),EVEN", "(4096,LT)", "(4,XX)", "((4,LT),EVEN,(8,GT))", "(4)", "(4,LT,S1.P1.X)", "4,LT", &format!("({})", ["(0,EQ)"; 9].join(","))] {
            assert!(parse_cond(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn cond_decides_from_the_steps_that_ran() {
        let history = [ran("S1", Some(0), None), ran("S2", Some(8), None)];
        let c = |t: &str| parse_cond(t).unwrap();
        assert!(bypassed_by_cond(&c("(4,LT)"), &history, false, None).is_some());
        assert!(bypassed_by_cond(&c("(4,LT,S1)"), &history, false, None).is_none());
        assert!(bypassed_by_cond(&c("(0,EQ,NOSTEP)"), &history, false, None).is_none());
        assert!(bypassed_by_cond(&Cond::default(), &history, true, None).is_some());
        assert!(bypassed_by_cond(&c("EVEN"), &history, true, None).is_none());
        assert!(bypassed_by_cond(&c("ONLY"), &history, false, None).is_some());
        let abended = [ran("S1", None, Some("S0C7"))];
        assert!(bypassed_by_cond(&c("((0,LE,S1),EVEN)"), &abended, true, None).is_none());
    }

    #[test]
    fn if_precedence_and_spellings() {
        let e = parse_expr("RC = 0 | RC = 4 & ABEND").unwrap();
        assert!(matches!(e, Expr::Or(_, _)));
        assert_eq!(parse_expr("¬ABEND").unwrap(), Expr::Not(Box::new(Expr::Abend(None))));
        assert_eq!(parse_expr("ABEND=FALSE").unwrap(), parse_expr("NOT ABEND").unwrap());
        assert_eq!(parse_expr("S1.RUN=TRUE").unwrap(), Expr::Run(StepRef { step: "S1".into(), procstep: None }));
        assert_eq!(parse_expr("ABENDCC=S0C7").unwrap(), Expr::AbendCc("S0C7".into()));
        for (text, op) in [("RC GT 4", Op::Gt), ("RC>4", Op::Gt), ("RC NG 4", Op::Le), ("RC ¬> 4", Op::Le), ("RC NL 4", Op::Ge), ("RC ¬< 4", Op::Ge), ("RC ¬= 4", Op::Ne), ("RC<=4", Op::Le)] {
            assert_eq!(parse_expr(text).unwrap(), Expr::Compare(Value::MaxRc, op, 4), "{text}");
        }
        assert_eq!(parse_expr("NOT RC = 0").unwrap(), Expr::Not(Box::new(Expr::Compare(Value::MaxRc, Op::Eq, 0))));
        for bad in ["RC", "RC = 4096", "4 = RC", "S1.ABENDCC=S0C7", "ABENDCC=S0CX", "(RC = 0", "RC = 0)", "S1.ABEND=MAYBE"] {
            assert!(parse_expr(bad).is_err(), "{bad}");
        }
        assert!(parse_expr(&format!("{}RC = 0{}", "(".repeat(15), ")".repeat(15))).is_ok());
        assert!(parse_expr(&format!("{}RC = 0{}", "(".repeat(16), ")".repeat(16))).unwrap_err().contains("15 deep"));
    }

    #[test]
    fn if_decides_from_the_steps_that_ran() {
        let history = [ran("S1", Some(4), None), ran("S2", None, Some("S0C7"))];
        let t = |text: &str| eval(&parse_expr(text).unwrap(), &history, None);
        assert!(t("RC = 4"));
        assert!(!t("S2.RC = 0") && !t("S2.RC ¬= 0"));
        assert!(t("S2.ABEND & ABENDCC=S0C7 & S1.RUN & ¬S3.RUN"));
        assert!(!t("S1.ABEND"));
        assert!(eval(&parse_expr("RC = 0").unwrap(), &[], None));
        assert!(tests_abend(&parse_expr("RC = 0 | ¬S1.RUN").unwrap()) && !tests_abend(&parse_expr("RC = 0").unwrap()));
    }

    #[test]
    fn procedure_steps_are_named_through_their_caller() {
        let history = [
            Ran { name: Some("P1".into()), caller: Some("CALL".into()), rc: Some(4), abend: None },
            Ran { name: Some("P2".into()), caller: Some("CALL".into()), rc: Some(8), abend: None },
            ran("P1", Some(0), None),
        ];
        let t = |text: &str, context| eval(&parse_expr(text).unwrap(), &history, context);
        assert!(t("CALL.P1.RC = 4", None) && t("P1.RC = 0", None));
        assert!(t("P1.RC = 4", Some("CALL")) && t("CALL.RC = 8", None) && t("CALL.RUN", None));
        assert!(!t("P2.RC = 8", None));
        let c = |text: &str, context| bypassed_by_cond(&parse_cond(text).unwrap(), &history, false, context).is_some();
        assert!(c("(4,EQ,P1)", Some("CALL")) && !c("(4,EQ,P1)", None) && c("(8,EQ,CALL.P2)", None));
    }
}
