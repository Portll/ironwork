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

    fn absorb(&mut self, other: Coverage) {
        for (program, paragraphs) in other.entered {
            let mine = self.entered.entry(program).or_default();
            for (index, (name, n)) in paragraphs {
                mine.entry(index).or_insert_with(|| (name, 0)).1 += n;
            }
        }
    }
}

/// A job's coverage by the source each COBOL step ran: steps that run one source add up, and the
/// programs of two sources stay apart even where they share a PROGRAM-ID.
#[derive(Default)]
pub struct BySource(Vec<(String, Coverage, Vec<Outline>)>);

impl BySource {
    pub fn add(&mut self, source: String, coverage: Coverage, outlines: Vec<Outline>) {
        match self.0.iter_mut().find(|(s, _, _)| *s == source) {
            Some((_, mine, known)) => {
                mine.absorb(coverage);
                known.extend(outlines.into_iter().filter(|o| !known.iter().any(|k| k.program == o.program)).collect::<Vec<_>>());
            }
            None => self.0.push((source, coverage, outlines)),
        }
    }

    /// As [`Coverage::report`], each program also naming the `source` it is in; the programs CALL
    /// loaded from a library once each.
    pub fn report(&self) -> Value {
        let (mut programs, mut called) = (Vec::new(), Vec::new());
        for (source, coverage, outlines) in &self.0 {
            let Value::Obj(mut report) = coverage.report(outlines) else { continue };
            if let Some(Value::Arr(list)) = report.remove("programs") {
                programs.extend(list.into_iter().map(|p| match p {
                    Value::Obj(mut p) => {
                        p.insert("source".into(), source.clone().into());
                        Value::Obj(p)
                    }
                    p => p,
                }));
            }
            if let Some(Value::Arr(list)) = report.remove("called") {
                called.extend(list.into_iter().filter(|c| !called.contains(c)).collect::<Vec<_>>());
            }
        }
        Value::Obj(fields([("programs", Value::Arr(programs)), ("called", Value::Arr(called))]))
    }
}
