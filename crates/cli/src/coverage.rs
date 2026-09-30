//! Paragraph coverage: which paragraphs of each program control entered during a run, and how
//! often, from the run unit's Paragraph events.

use std::collections::BTreeMap;

use exec::evidence::{Value, fields};

/// A program's paragraphs as written: name, line and whether it is a section header.
pub struct Outline {
    pub program: String,
    pub paragraphs: Vec<(String, u32, bool)>,
}

impl Outline {
    pub fn of(program: &syntax::ast::Program) -> Self {
        Outline { program: program.id.clone(), paragraphs: program.paragraphs.iter().map(|p| (p.name.clone(), p.pos.line, p.is_section)).collect() }
    }
}

#[derive(Default)]
pub struct Coverage {
    /// Per program, per paragraph index: its name and how often control entered it.
    entered: BTreeMap<String, BTreeMap<usize, (String, u64)>>,
}

impl Coverage {
    pub fn observe(&mut self, event: &exec::unit::Event<'_>) {
        if let exec::unit::Event::Paragraph { program, name, index } = event {
            let entry = self.entered.entry(program.to_string()).or_default().entry(*index).or_insert_with(|| (name.to_string(), 0));
            entry.1 += 1;
        }
    }

    /// Whether control entered paragraph `index` of `program`.
    pub fn reached(&self, program: &str, index: usize) -> bool {
        self.entered.get(program).is_some_and(|p| p.contains_key(&index))
    }

    /// Each outlined program with every paragraph and its count, then the paragraphs reached in
    /// programs no outline covers (programs CALL loaded from a library).
    pub fn report(&self, outlines: &[Outline]) -> Value {
        let programs = outlines
            .iter()
            .map(|o| {
                let counts = self.entered.get(&o.program);
                let paragraphs: Vec<Value> = o
                    .paragraphs
                    .iter()
                    .enumerate()
                    .map(|(i, (name, line, section))| {
                        let n = counts.and_then(|c| c.get(&i)).map_or(0, |(_, n)| *n);
                        Value::Obj(fields([("name", name.clone().into()), ("line", Value::Int(i64::from(*line))), ("section", (*section).into()), ("entered", Value::Int(n as i64))]))
                    })
                    .collect();
                let reached = counts.map_or(0, |c| c.len());
                Value::Obj(fields([("program", o.program.clone().into()), ("paragraphs", Value::Int(o.paragraphs.len() as i64)), ("reached", Value::Int(reached as i64)), ("detail", Value::Arr(paragraphs))]))
            })
            .collect();
        let called = self
            .entered
            .iter()
            .filter(|(p, _)| !outlines.iter().any(|o| &o.program == *p))
            .map(|(p, c)| Value::Obj(fields([("program", p.clone().into()), ("reached", Value::Arr(c.values().map(|(n, _)| Value::Str(n.clone())).collect()))])))
            .collect();
        Value::Obj(fields([("programs", Value::Arr(programs)), ("called", Value::Arr(called))]))
    }
}
