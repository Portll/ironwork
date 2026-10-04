//! Paragraph and statement coverage: which paragraphs of each program control entered during a run,
//! and which statements started, and how often, from the run unit's Paragraph and Statement events.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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

    /// A load module's program: its paragraphs from the LIR, each line from the debug table.
    pub fn of_lir(program: &exec::lir::Program) -> Self {
        let sym = |id: u32| program.symbols.get(id as usize).cloned().unwrap_or_default();
        let line = |at: u32| program.debug.positions.get(at as usize).map_or(0, |pos| pos.line);
        Outline { program: sym(program.id), paragraphs: program.paragraphs.iter().map(|p| (sym(p.name), line(p.at), p.is_section)).collect() }
    }
}

/// The programs of a source a run outlines: each but a function prototype, which holds no code, so
/// that the function it declares is reported as called.
pub fn source_outlines(programs: &[syntax::ast::Program]) -> Vec<Outline> {
    programs.iter().filter(|p| !p.is_prototype()).map(Outline::of).collect()
}

/// The programs a run that begins with `module` outlines: those compiled from the source its
/// program 0 was, as a run of that source outlines them.
pub fn module_outlines(module: &exec::module::LoadedModule) -> Vec<Outline> {
    let first = module.files.first().and_then(|f| f.first());
    module.programs.iter().zip(&module.files).filter(|(_, files)| files.first() == first).map(|(p, _)| Outline::of_lir(p)).collect()
}

#[derive(Default)]
pub struct Coverage {
    /// Per program, per paragraph index: its name and how often control entered it.
    entered: BTreeMap<String, BTreeMap<usize, (String, u64)>>,
    /// Per file as the Statement event names it, per line: how often a statement there started.
    started: BTreeMap<String, BTreeMap<u32, u64>>,
    /// The run's first program and the directories it reads, which name each statement's file as
    /// the journal does.
    files: Option<(String, Vec<PathBuf>)>,
}

impl Coverage {
    /// Coverage whose statements are named by their path from the root that supplied them, the
    /// first program's own source standing for the empty name its events give it.
    pub fn naming(program: &str, roots: &[PathBuf]) -> Self {
        Coverage { files: Some((program.to_string(), roots.to_vec())), ..Default::default() }
    }

    pub fn observe(&mut self, event: &exec::unit::Event<'_>) {
        match event {
            exec::unit::Event::Paragraph { program, name, index } => {
                let entry = self.entered.entry(program.to_string()).or_default().entry(*index).or_insert_with(|| (name.to_string(), 0));
                entry.1 += 1;
            }
            exec::unit::Event::Statement { file, line } => {
                // Looked up by the borrowed name, so a statement start allocates only for a new file.
                if !self.started.contains_key(*file) {
                    self.started.insert(file.to_string(), BTreeMap::new());
                }
                if let Some(lines) = self.started.get_mut(*file) {
                    *lines.entry(*line).or_default() += 1;
                }
            }
            _ => {}
        }
    }

    /// Each statement line that started, by file, with how often.
    fn statements(&self) -> BTreeMap<(String, i64), i64> {
        let mut out = BTreeMap::new();
        for (file, lines) in &self.started {
            let name = match &self.files {
                Some((program, roots)) => crate::evidence::relative(Path::new(if file.is_empty() { program } else { file }), roots),
                None => file.clone(),
            };
            for (&line, &n) in lines {
                *out.entry((name.clone(), i64::from(line))).or_default() += n as i64;
            }
        }
        out
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
        Value::Obj(fields([("programs", Value::Arr(programs)), ("called", Value::Arr(called)), ("statements", statement_list(self.statements()))]))
    }

    fn absorb(&mut self, other: Coverage) {
        for (program, paragraphs) in other.entered {
            let mine = self.entered.entry(program).or_default();
            for (index, (name, n)) in paragraphs {
                mine.entry(index).or_insert_with(|| (name, 0)).1 += n;
            }
        }
        for (file, lines) in other.started {
            let mine = self.started.entry(file).or_default();
            for (line, n) in lines {
                *mine.entry(line).or_default() += n;
            }
        }
    }
}

/// Statements as a report lists them: by file and line, each with how often it started.
fn statement_list(started: BTreeMap<(String, i64), i64>) -> Value {
    Value::Arr(started.into_iter().map(|((file, line), n)| Value::Obj(fields([("file", file.into()), ("line", Value::Int(line)), ("started", Value::Int(n))]))).collect())
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
        let (mut programs, mut called, mut started) = (Vec::new(), Vec::new(), BTreeMap::new());
        for (source, coverage, outlines) in &self.0 {
            for (at, n) in coverage.statements() {
                *started.entry(at).or_default() += n;
            }
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
        Value::Obj(fields([("programs", Value::Arr(programs)), ("called", Value::Arr(called)), ("statements", statement_list(started))]))
    }
}
