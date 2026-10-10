//! The interpreter: one activation of one program, over the run unit's memory in EBCDIC. A
//! reference can reach anywhere in that memory, as a program compiled without SSRANGE can on
//! z/OS, but never outside it.

pub use rt::abend::{Abend, Ending};
use crate::abend::{AbendCode, Signal};
use crate::layout::{Kind, Layout, Resolved};
use rt::storage::{Loc, Val};
pub(crate) use rt::storage::literal_fixed;
use crate::unit::{ADDRESS_BASE, Event, LoadError, OS_COMMAND_ROUTINES, RETURN_CODE, RunUnit};
use crate::Compiled;
use compile::arith::{decimal_exponent, divided_exponent, function_dmax};
use compile::statements::{flatten_and, key_term, numval_currency, upon_console};
use compile::values::value_kind;
use numeric::precision::{Dmax, Fixed, Places};
use numeric::{LeServices, Options, ProgramScope, Switched};
use rt::fixed::{align, places_of};
use rt::arith;
use rt::callee::{self, Bindings, By, Callee};
use rt::display::utf16_text;
use rt::host::Host;
use rt::lir::{ByteClass, CallArg, ConvertTable, Converting, SignTest, StringSource, TrimSide};
use rt::loc;
use rt::store;
use rt::text::UnstringField;
use std::cmp::Ordering;
use std::collections::HashMap;
use syntax::Pos;
use syntax::ast::*;
use zarch::ebcdic::{self, CodePage};
use zarch::hfp::{Hfp, Precision};

mod cics;
pub(crate) mod cics_bind;
mod declaratives;
mod facts;
mod file_io;
mod function;
mod intrinsic;
use intrinsic::Within;
mod json;
mod le_services;
mod oo;
mod parmcheck;
mod perform;
mod report;
mod scope;
mod sort;
pub(crate) mod sql;
mod xml;

type R<T> = Result<T, Abend>;

enum Flow {
    Next,
    End(Ending),
    GoTo(usize),
    ExitParagraph,
    ExitSection,
    ExitPerform,
    ExitPerformCycle,
    NextSentence,
    /// Returning, at this paragraph and statement, after a PERFORM whose range control left.
    Resume(usize, usize),
    /// Returning to the active PERFORM with this frame number.
    Return(u64),
}

pub struct Machine<'p, 'u, 'w> {
    compiled: &'p Compiled,
    program: &'p Program,
    layout: &'p Layout,
    options: Options,
    ssrange: bool,
    page: &'static CodePage,
    collating: &'p crate::collating::Sequence,
    when_compiled: rt::lir::CompileTime,
    resolved: HashMap<(String, Vec<String>), Resolved>,
    /// This program's place in the run unit, and where its storage starts.
    me: usize,
    base: usize,
    /// Where each LINKAGE record is, once an argument or SET ADDRESS OF has given it an address.
    linkage: Vec<Option<usize>>,
    /// Where this activation's LOCAL-STORAGE starts.
    local_base: usize,
    /// The first program of the run unit, where EXIT PROGRAM does nothing.
    main: bool,
    /// The logical level's HANDLE CONDITION, IGNORE CONDITION and HANDLE ABEND, which the
    /// activation running holds (C234).
    cics_handlers: cics::Handlers,
    /// This activation's number in the CICS task, which owns the HANDLE labels it sets.
    serial: u64,
    /// The run unit's first program, which the run unit holds no handle to, for a CALL, LINK or
    /// XCTL of it from this activation (C148, C127).
    first: Option<&'p Compiled>,
    report_writer: &'p crate::report::Writer,
    /// Each file's printer control character, when it is a print file.
    carriage: &'p [Option<crate::printer::Carriage>],
    /// The method this activation runs, if it is one: its class and SELF.
    oo: oo::Frame,
    /// The SORT or MERGE whose input or output procedure is running.
    sort: Option<sort::Active>,
    /// The priority-number of the segment the running paragraph is in.
    segment: u8,
    declaratives: &'p crate::declaratives::Table,
    uses: declaratives::State,
    returns: perform::Returns,
    /// XML-TEXT and the other XML registers of the event being processed.
    xml: xml::Registers,
    /// The programs containing this one, innermost first, as they are running.
    containers: Vec<scope::Frame<'p>>,
    /// The user-defined functions the program may invoke.
    functions: &'p [compile::function::Udf],
    unit: &'u mut RunUnit<'w>,
}

enum Step {
    Again,
    Leave,
    Out(Flow),
}

impl<'p, 'u, 'w> Machine<'p, 'u, 'w> {
    /// An activation of loaded program `me`. Its storage is initialized on its first activation,
    /// after a CANCEL, and on every activation of an INITIAL program.
    pub fn activation(compiled: &'p Compiled, me: usize, unit: &'u mut RunUnit<'w>, main: bool) -> R<Self> {
        Self::activation_within(compiled, me, unit, main, Vec::new())
    }

    /// An activation of a contained program, called with the programs containing it running.
    fn activation_within(compiled: &'p Compiled, me: usize, unit: &'u mut RunUnit<'w>, main: bool, containers: Vec<scope::Frame<'p>>) -> R<Self> {
        let (base, fresh) = unit.activate(me, compiled.program.initial);
        let mut m = Self::over(compiled, me, base, unit, main);
        m.containers = containers;
        m.bind_shared()?;
        m.initial_values(fresh)
    }

    fn initial_values(mut self, fresh: bool) -> R<Self> {
        let (local, size) = (self.layout.local_size as usize, self.layout.size as usize);
        if local > 0 {
            self.local_base = self.unit.push_temporary(&vec![0; local]);
            self.initialize_values(true)?;
            self.unit.mark_input(self.local_base, local, false);
        }
        if fresh {
            self.unit.mem[self.base..self.base + size].fill(0);
            self.initialize_values(false)?;
            self.unit.mark_input(self.base, size, false);
            self.unit.initialized(self.me);
        }
        Ok(self)
    }

    /// Program `me` over its storage at `base`, with nothing bound or initialized.
    fn over(compiled: &'p Compiled, me: usize, base: usize, unit: &'u mut RunUnit<'w>, main: bool) -> Self {
        let serial = unit.cics.as_mut().map_or(0, crate::cics::Task::next_activation);
        let first = unit.programs[me].compiled.is_none().then_some(compiled);
        Self {
            compiled,
            program: &compiled.program,
            layout: &compiled.layout,
            options: compiled.options,
            ssrange: compiled.ssrange,
            page: compiled.options.code_page(),
            collating: &compiled.collating,
            when_compiled: compiled.when_compiled,
            resolved: HashMap::new(),
            me,
            base,
            linkage: vec![None; compiled.layout.linkage_roots.len()],
            local_base: 0,
            main,
            cics_handlers: cics::Handlers::default(),
            serial,
            first,
            report_writer: &compiled.report_writer,
            carriage: &compiled.carriage,
            oo: oo::Frame::default(),
            sort: None,
            segment: 0,
            declaratives: &compiled.declaratives,
            uses: declaratives::State::default(),
            returns: perform::Returns::new(compiled.program.paragraphs.len()),
            xml: xml::Registers::default(),
            containers: Vec::new(),
            functions: &compiled.functions,
            unit,
        }
    }

    /// Applies VALUE clauses: to WORKING-STORAGE and file records, or to LOCAL-STORAGE.
    fn initialize_values(&mut self, local: bool) -> R<()> {
        let base = if local { self.local_base } else { self.base };
        compile::values::initialize(self.compiled, self.unit, base, local)
    }

    pub fn run_procedure(&mut self) -> R<Ending> {
        self.run_from(None)
    }

    /// Control reaching a paragraph of segment `priority`: an independent segment entered from
    /// another is in its initial state, so its altered GO TOs are as written (assumption C52).
    fn enter_segment(&mut self, priority: u8) {
        if priority == self.segment {
            return;
        }
        self.segment = priority;
        if priority >= 50 {
            let program = self.program;
            for (i, target) in self.unit.programs[self.me].altered.iter_mut().enumerate() {
                if program.paragraphs[i].priority == priority {
                    *target = None;
                }
            }
        }
    }

    /// A paragraph's statements: NEXT SENTENCE resumes after the next separator period.
    fn run_sentences(&mut self, stmts: &'p [Stmt]) -> R<Flow> {
        let mut i = 0;
        while i < stmts.len() {
            match self.exec(&stmts[i])? {
                Flow::Next => i += 1,
                Flow::NextSentence => i = stmts[i..].iter().position(|s| *s == Stmt::SentenceEnd).map_or(stmts.len(), |j| i + j + 1),
                other => return Ok(other),
            }
        }
        Ok(Flow::Next)
    }

    fn run_block(&mut self, stmts: &'p [Stmt]) -> R<Flow> {
        for s in stmts {
            match self.exec(s)? {
                Flow::Next => {}
                other => return Ok(other),
            }
        }
        Ok(Flow::Next)
    }

    fn procedure(&self, p: &ProcName, pos: Pos) -> R<(usize, usize)> {
        crate::procedure(self.program, p).map_err(|m| Abend::ironwork(m, pos))
    }

    /// One statement. An EXCEPTION/ERROR procedure it ran may have sent control elsewhere.
    fn exec(&mut self, s: &'p Stmt) -> R<Flow> {
        if !self.declaratives.triggers.is_empty()
            && let Some(pos) = declaratives::statement_pos(s)
        {
            self.uses.line = pos;
        }
        if self.unit.limited()
            && let Some(pos) = declaratives::statement_pos(s)
        {
            self.unit.start_statement(self.me, pos)?;
        }
        if (self.unit.statements.is_some() || self.unit.taint.is_some())
            && let Some(pos) = declaratives::statement_pos(s)
        {
            self.unit.statement_starts();
            if self.unit.traces(pos.line) {
                let file = self.event_file(pos);
                self.unit.notify(Event::Statement { file: &file, line: pos.line });
            }
        }
        match self.statement(s) {
            Err(Abend { code: AbendCode::Signal(Signal::DeclarativeExit), .. }) => Ok(self.declarative_exit()),
            Err(Abend { code: AbendCode::Signal(Signal::StopRun), .. }) => Ok(Flow::End(Ending::StopRun)),
            flow => flow,
        }
    }

    fn statement(&mut self, s: &'p Stmt) -> R<Flow> {
        match s {
            Stmt::Move { from, to, pos } => {
                for r in to {
                    let dest = self.locate_written(|m| m.locate_receiving(r))?;
                    let (val, src) = self.move_source(from, dest, *pos)?;
                    self.assign(dest, val, src, *pos)?;
                }
            }
            Stmt::Compute { targets, expr, size_error, pos } => {
                let computations: Vec<(Target, Expr)> = targets.iter().map(|t| (t.clone(), expr.clone())).collect();
                return self.arithmetic(&computations, None, size_error.as_ref(), false, *pos);
            }
            Stmt::Arith(a) => return self.arithmetic(&a.computations, a.remainder.as_ref(), a.size_error.as_ref(), true, a.pos),
            Stmt::Corresponding(c) => return Err(Abend::ironwork("CORRESPONDING reached the interpreter unexpanded", c.pos)),
            Stmt::If { cond, then, otherwise, pos } => {
                let branch = if self.condition(cond, *pos)? { then } else { otherwise };
                return self.run_block(branch);
            }
            Stmt::Evaluate { subjects, whens, other, pos } => {
                for w in whens {
                    for alternative in &w.alternatives {
                        if self.alternative_matches(subjects, alternative, *pos)? {
                            return self.run_block(&w.body);
                        }
                    }
                }
                return self.run_block(other);
            }
            Stmt::PerformProc { from, thru, repeat, pos } => {
                let (start, first_end) = self.procedure(from, *pos)?;
                let end = match thru {
                    Some(t) => self.procedure(t, *pos)?.1,
                    None => first_end,
                };
                let statement = if matches!(repeat, Loop::Once) { self.after(s) } else { None };
                return self.repeat(repeat, *pos, &mut |m: &mut Self| {
                    m.uses.line = *pos;
                    m.perform_range(start, end, None, statement)
                });
            }
            Stmt::PerformInline { body, repeat, pos } => return self.repeat(repeat, *pos, &mut |m: &mut Self| m.run_block(body)),
            Stmt::Display { items, upon: Some(upon), pos, .. } if matches!(upon.device.as_str(), "ENVIRONMENT-NAME" | "ENVIRONMENT-VALUE") => {
                let text = self.display_text(items, false, *pos)?;
                if upon.device == "ENVIRONMENT-VALUE" { self.unit.environment.set(&text) } else { self.unit.environment.name(&text) }
            }
            Stmt::Display { items, upon: Some(upon), pos, .. } if upon.device == "ARGUMENT-NUMBER" => {
                let n = match items.as_slice() {
                    [item] => self.integer(&Expr::Operand(item.clone()), *pos)?,
                    _ => return Err(Abend::ironwork("DISPLAY UPON ARGUMENT-NUMBER shows one item", *pos)),
                };
                self.unit.arguments.position(n);
            }
            Stmt::Display { items, screen: Some(screen), pos, .. } => {
                let text = self.display_text(items, false, *pos)?;
                let at = self.screen_at(screen, *pos)?;
                rt::crt::display(self.unit, at, &text, clearing(screen));
            }
            Stmt::Display { items, upon: Some(upon), no_advancing, pos, .. } if upon.device == "SYSERR" => {
                let text = self.display_text(items, false, *pos)?;
                if self.unit.observed() {
                    self.sink("log", *pos, &text);
                }
                rt::display::write(&mut *self.unit.err, &text, *no_advancing, *pos)?;
            }
            Stmt::Display { items, upon, no_advancing, pos, .. } => self.display(items, upon_console(upon.as_ref()), *no_advancing, *pos)?,
            Stmt::Open { files, pos } => {
                for (mode, name) in files {
                    self.open_file(*mode, name, *pos)?;
                }
            }
            Stmt::Close { files, pos } => {
                for (name, closing) in files {
                    self.close_file_with(name, *closing, *pos)?;
                }
            }
            Stmt::Read(r) => return self.read_stmt(r),
            Stmt::Write { record, from, advancing, invalid, end_of_page, pos } => return self.write_stmt(record, from.as_ref(), advancing.as_ref(), invalid, end_of_page, *pos),
            Stmt::Rewrite { record, from, invalid, pos } => return self.rewrite_stmt(record, from.as_ref(), invalid, *pos),
            Stmt::Delete { file, invalid, pos } => return self.delete_stmt(file, invalid, *pos),
            Stmt::DeleteFile { files, pos } => self.delete_files(files, *pos)?,
            Stmt::Start { file, key, invalid, pos } => return self.start_stmt(file, key.as_ref(), invalid, *pos),
            Stmt::Initialize { targets, with, pos } => {
                let with = with.as_deref().unwrap_or(&NO_PHRASES);
                for r in targets {
                    let loc = self.locate_written(|m| m.locate(r))?;
                    let item = (loc.item != usize::MAX).then_some(loc.item);
                    let category = match (item, &r.refmod) {
                        (_, Some(_)) => self.layout.refmod_category(item, loc.kind),
                        (Some(item), None) => {
                            self.initialize(item, loc.offset, with, *pos)?;
                            continue;
                        }
                        (None, None) => DataCategory::Numeric,
                    };
                    match with.initial_value(Some(category), false) {
                        Some(InitialValue::Replacing(by)) => {
                            let (val, src) = self.operand_with_loc(by, *pos)?;
                            self.assign(loc, val, src, *pos)?;
                        }
                        Some(_) if r.refmod.is_some() => self.assign(loc, Val::Fig(Figurative::Space), None, *pos)?,
                        Some(_) => self.unit.write(loc.offset, &vec![0; loc.len]),
                        None => {}
                    }
                }
            }
            Stmt::GoTo { target: Some(target), pos } => return Ok(Flow::GoTo(self.procedure(target, *pos)?.0)),
            Stmt::GoTo { target: None, .. } | Stmt::Entry { .. } => {}
            Stmt::GoToDepending { targets, on, pos } => {
                let n = self.integer(&Expr::Operand(Operand::Ref(on.clone())), *pos)?;
                if let Some(target) = usize::try_from(n).ok().and_then(|n| targets.get(n.wrapping_sub(1))) {
                    return Ok(Flow::GoTo(self.procedure(target, *pos)?.0));
                }
            }
            Stmt::Alter { pairs, pos } => {
                for (paragraph, target) in pairs {
                    let (at, to) = (self.procedure(paragraph, *pos)?.0, self.procedure(target, *pos)?.0);
                    let paragraphs = self.program.paragraphs.len();
                    let altered = &mut self.unit.programs[self.me].altered;
                    altered.resize(paragraphs, None);
                    altered[at] = Some(to);
                }
                if let Some(flow) = self.debug_alter(pairs, *pos)? {
                    return Ok(flow);
                }
            }
            Stmt::Goback { .. } => return Ok(Flow::End(Ending::Goback)),
            Stmt::ExitProgram { .. } if self.main => {}
            Stmt::ExitProgram { .. } => return Ok(Flow::End(Ending::Goback)),
            Stmt::Call(c) => return self.call(c),
            Stmt::Cancel { targets, pos } => {
                for t in targets {
                    let name = self.program_name(t, *pos)?;
                    callee::cancel(self.unit, &name, *pos)?;
                }
            }
            Stmt::Set { set, pos } => self.set(set, *pos)?,
            Stmt::Accept { target, exception, screen: Some(screen), pos, .. } => {
                let inputs = match screen.screen {
                    Some(_) => screen
                        .inputs
                        .iter()
                        .map(|i| {
                            let at = Some((i.line as usize, i.column as usize));
                            Ok(rt::crt::Input { target: self.locate(&i.target)?, field: self.locate(&i.field)?, at, update: i.update, secure: i.secure })
                        })
                        .collect::<R<Vec<_>>>()?,
                    None => {
                        let loc = self.locate(target)?;
                        let at = self.screen_at(screen, *pos)?;
                        vec![rt::crt::Input { target: loc, field: loc, at, update: screen.update, secure: screen.secure }]
                    }
                };
                let raised = rt::crt::accept(&self.facts(), self.unit, &inputs, *pos)?;
                return self.overflow_branch(raised, &exception.on, &exception.not_on);
            }
            Stmt::Accept { target, from: from @ (AcceptFrom::ArgumentValue | AcceptFrom::EnvironmentValue), exception, pos, .. } => {
                let raised = self.accept(target, *from, *pos)?;
                return self.overflow_branch(raised, &exception.on, &exception.not_on);
            }
            Stmt::Accept { target, from, pos, .. } => {
                self.accept(target, *from, *pos)?;
            }
            Stmt::String(st) => return self.string_stmt(st),
            Stmt::Unstring(u) => return self.unstring(u),
            Stmt::Inspect(i) => self.inspect(i)?,
            Stmt::Search(se) => return self.search(se),
            Stmt::Sorting(s) => return self.sorting(s),
            Stmt::NextSentence => return Ok(Flow::NextSentence),
            Stmt::Exec(block) if block.declarative() => {}
            Stmt::Exec(block) if block.kind == ExecKind::Cics => return self.cics(block),
            Stmt::Exec(block) if block.kind == ExecKind::Sql => return self.sql(block),
            Stmt::Report(r) => return self.report_statement(r),
            Stmt::Exec(block) => {
                let kind = match block.kind {
                    ExecKind::Sql => "SQL",
                    ExecKind::Cics => "CICS",
                    ExecKind::Dli => "DLI",
                    ExecKind::Other => "",
                };
                return Err(rt::refusal::IWR0060.ending(AbendCode::Exec, format_args!("EXEC {kind} {} was reached: ironwork for COBOL checks EXEC statements but does not run them yet", block.command), block.pos));
            }
            Stmt::Invoke(i) => return self.invoke(i),
            Stmt::JsonGenerate(g) => return self.json_generate(g),
            Stmt::XmlParse(x) => return self.xml_parse(x),
            Stmt::XmlGenerate(x) => return self.xml_generate(x),
            Stmt::JsonParse(j) => return self.json_parse(j),
            Stmt::ExitMethod { .. } => return Ok(Flow::End(Ending::Goback)),
            Stmt::SentenceEnd => {}
            Stmt::StopRun { .. } => return Ok(Flow::End(Ending::StopRun)),
            Stmt::Exit { kind: ExitKind::Paragraph, .. } => return Ok(Flow::ExitParagraph),
            Stmt::Exit { kind: ExitKind::Section, .. } => return Ok(Flow::ExitSection),
            Stmt::Exit { kind: ExitKind::Perform, .. } => return Ok(Flow::ExitPerform),
            Stmt::Exit { kind: ExitKind::PerformCycle, .. } => return Ok(Flow::ExitPerformCycle),
            Stmt::Continue { .. } | Stmt::Exit { kind: ExitKind::Plain, .. } => {}
            Stmt::Hole { construct, why, pos } => {
                return Err(rt::refusal::IWR0078.abend(format_args!("{construct} was reached: under --compliance relaxed it compiled as a hole, since {why}"), *pos));
            }
        }
        Ok(Flow::Next)
    }

    fn alternative_matches(&mut self, subjects: &[Subject], objects: &[Object], pos: Pos) -> R<bool> {
        for (subject, object) in subjects.iter().zip(objects) {
            let hit = match (subject, object) {
                (_, Object::Any) => true,
                (Subject::Bool(b), Object::Bool(o)) => b == o,
                (Subject::Bool(b), Object::Cond(c)) => self.condition(c, pos)? == *b,
                (Subject::Cond(c), Object::Bool(o)) => self.condition(c, pos)? == *o,
                (Subject::Cond(c), Object::Cond(d)) => self.condition(c, pos)? == self.condition(d, pos)?,
                (Subject::Expr(e), Object::Value { not, from, thru }) => {
                    let inside = match thru {
                        None => self.compare(e, from, pos)? == Ordering::Equal,
                        Some(t) => self.compare(e, from, pos)? != Ordering::Less && self.compare(e, t, pos)? != Ordering::Greater,
                    };
                    inside != *not
                }
                _ => return Err(Abend::ironwork("a WHEN object of a different kind from its subject", pos)),
            };
            if !hit {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Runs a PERFORM's body as its phrase says. EXIT PERFORM leaves the loop; EXIT PERFORM CYCLE
    /// ends one iteration.
    fn repeat(&mut self, repeat: &'p Loop, pos: Pos, body: &mut dyn FnMut(&mut Self) -> R<Flow>) -> R<Flow> {
        self.unit.enter(pos)?;
        let flow = self.repeat_nested(repeat, pos, body);
        self.unit.depth -= 1;
        flow
    }

    fn repeat_nested(&mut self, repeat: &'p Loop, pos: Pos, body: &mut dyn FnMut(&mut Self) -> R<Flow>) -> R<Flow> {
        let mut run = |m: &mut Self| -> R<Step> {
            Ok(match body(m)? {
                Flow::Next | Flow::ExitPerformCycle => Step::Again,
                Flow::ExitPerform => Step::Leave,
                other => Step::Out(other),
            })
        };
        match repeat {
            Loop::Once => match run(self)? {
                Step::Out(f) => return Ok(f),
                Step::Again | Step::Leave => {}
            },
            Loop::Times(count) => {
                for _ in 0..self.integer(count, pos)?.max(0) {
                    match run(self)? {
                        Step::Again => {}
                        Step::Leave => break,
                        Step::Out(f) => return Ok(f),
                    }
                }
            }
            Loop::Forever => loop {
                match run(self)? {
                    Step::Again => {}
                    Step::Leave => break,
                    Step::Out(f) => return Ok(f),
                }
            },
            Loop::Until { cond, test_after } => loop {
                if !test_after && self.condition(cond, pos)? {
                    break;
                }
                match run(self)? {
                    Step::Again => {}
                    Step::Leave => break,
                    Step::Out(f) => return Ok(f),
                }
                if *test_after && self.condition(cond, pos)? {
                    break;
                }
            },
            Loop::Varying { varying, after, test_after } => {
                let levels: Vec<&'p Varying> = std::iter::once(&**varying).chain(after).collect();
                let count = if *test_after { 1 } else { levels.len() };
                for v in &levels[..count] {
                    self.vary_from(v, pos)?;
                }
                if let Step::Out(f) = self.vary(&levels, *test_after, pos, &mut run)? {
                    return Ok(f);
                }
            }
        }
        Ok(Flow::Next)
    }

    /// The loop of `levels[0]`, each pass running the loops of the levels inside it, in the order of
    /// the Language Reference's figures for TEST BEFORE and TEST AFTER (SC27-8713-03, pp. 425-428):
    /// an outer variable is augmented before the one inside it is set to its FROM value again.
    fn vary(&mut self, levels: &[&'p Varying], test_after: bool, pos: Pos, run: &mut dyn FnMut(&mut Self) -> R<Step>) -> R<Step> {
        let Some((level, inner)) = levels.split_first() else { return run(self) };
        loop {
            if !test_after && self.condition(&level.until, pos)? {
                return Ok(Step::Again);
            }
            if test_after && let Some(next) = inner.first() {
                self.vary_from(next, pos)?;
            }
            match self.vary(inner, test_after, pos, run)? {
                Step::Again => {}
                other => return Ok(other),
            }
            if test_after && self.condition(&level.until, pos)? {
                return Ok(Step::Again);
            }
            self.vary_by(level, pos)?;
            if !test_after && let Some(next) = inner.first() {
                self.vary_from(next, pos)?;
            }
        }
    }

    fn vary_from(&mut self, v: &Varying, pos: Pos) -> R<()> {
        let var = self.locate_written(|m| m.locate(&v.var))?;
        let start = self.expr_value(&v.from, pos)?;
        self.assign(var, start, None, pos)
    }

    /// The step adds BY to the variable; a COMP-1 or COMP-2 variable makes it floating point, as
    /// an ADD to it is (Programming Guide SC27-8714-03, p. 800).
    fn vary_by(&mut self, v: &Varying, pos: Pos) -> R<()> {
        let var = self.locate(&v.var)?;
        let step = Expr::Bin(Box::new(Expr::Operand(Operand::Ref(v.var.clone()))), BinOp::Add, Box::new(v.by.clone()));
        if let Kind::Float(_) = var.kind {
            return self.arithmetic(&[(Target { r: v.var.clone(), rounded: false }, step)], None, None, false, pos).map(|_| ());
        }
        let dmax = var.kind.digits_scale().map_or(0, |(_, s)| s).max(self.dmax(&step)?);
        let next = self.eval_fixed(&step, dmax, pos)?;
        store::store_fixed(&self.facts(), self.unit, var, &next, false, pos)
    }

    fn resolve(&mut self, r: &Ref) -> R<Resolved> {
        let key = (r.name.clone(), r.qualifiers.clone());
        if let Some(&hit) = self.resolved.get(&key) {
            return Ok(hit);
        }
        let found = self.layout.resolve(&r.name, &r.qualifiers, r.pos).map_err(|e| Abend::ironwork(e.message, r.pos))?;
        self.resolved.insert(key, found);
        Ok(found)
    }

    fn locate(&mut self, r: &Ref) -> R<Loc> {
        self.locate_as(r, false)
    }

    /// A receiver the statement only writes, located as `locate` would: under taint its old bytes
    /// are not read (`Taint::writing`).
    fn locate_written(&mut self, locate: impl FnOnce(&mut Self) -> R<Loc>) -> R<Loc> {
        let was = self.unit.writing(true);
        let loc = locate(self);
        self.unit.writing(was);
        loc
    }

    /// The receiving item of MOVE, ACCEPT, STRING, UNSTRING, READ and RETURN INTO, and WRITE,
    /// REWRITE and RELEASE FROM: a group holding the object of its own OCCURS DEPENDING ON is its
    /// maximum length (Language Reference SC27-8713-03, pp. 205-206).
    fn locate_receiving(&mut self, r: &Ref) -> R<Loc> {
        self.locate_as(r, true)
    }

    fn locate_as(&mut self, r: &Ref, receiving: bool) -> R<Loc> {
        if let Some(loc) = self.oo_register(r)?.or(self.xml_register(r)?) {
            self.unit.taint_read(loc);
            return Ok(loc);
        }
        if r.name == "RETURN-CODE" && r.qualifiers.is_empty() && !self.layout.items.iter().any(|i| i.name.as_deref() == Some("RETURN-CODE")) {
            let (offset, len, kind) = rt::unit::return_code_place(&self.options);
            let loc = Loc { offset: RETURN_CODE + offset, len, kind, item: usize::MAX };
            self.unit.taint_read(loc);
            return Ok(loc);
        }
        let Resolved::Item(index) = self.resolve(r)? else {
            return Err(Abend::ironwork(format!("{} is a condition-name, not a data item", r.name), r.pos));
        };
        self.locate_item(index, r, receiving)
    }

    /// Item `index` with `r`'s subscripts and reference modification, `r` naming it in messages:
    /// how a condition-name reaches its conditional variable, which may be FILLER or share its
    /// name with other items.
    fn locate_item(&mut self, index: usize, r: &Ref, receiving: bool) -> R<Loc> {
        let layout = self.layout;
        let item = &layout.items[index];
        if r.subscripts.len() != item.dims.len() {
            return Err(Abend::ironwork(format!("{} takes {} subscripts, not {}", r.name, item.dims.len(), r.subscripts.len()), r.pos));
        }
        let base = match item.linkage {
            Some(l) => loc::linkage_base(self.linkage[l as usize], &r.name, r.pos)?,
            None if item.local => self.local_base,
            None => self.base,
        };
        let mut offset = (base + item.offset as usize) as i64;
        for &t in &item.moved_by {
            offset -= self.unused(t, r.pos)?;
        }
        let mut composed = 0;
        for (&(stride, _), sub) in item.dims.iter().zip(&r.subscripts) {
            let s = self.integer(sub, r.pos)?;
            composed += loc::subscript(s, stride);
        }
        if self.ssrange
            && let Some((displacement, extent)) = layout.table_range(index)
        {
            loc::table_reference(i64::from(displacement) + composed, i64::from(item.size), i64::from(extent), &r.name, r.pos)?;
        }
        offset += composed;
        let (mut len, mut kind) = (item.size as i64, item.kind);
        let unbounded = item.odo.iter().any(|&t| self.layout.items[t].unbounded);
        if !item.odo.is_empty() && !(receiving && r.refmod.is_none() && !item.followed && !unbounded && self.objects_within(&item.odo, index)?) {
            for &t in &item.odo {
                len -= self.unused(t, r.pos)?;
            }
        }
        if let Some(rm) = &r.refmod {
            let start = self.integer(&rm.start, r.pos)?;
            let length = match &rm.length {
                Some(l) => Some(self.integer(l, r.pos)?),
                None => None,
            };
            // A national or DBCS item's character positions are two bytes, and a part of it keeps its
            // category (Language Reference SC27-8713-03, p. 75).
            let unit = if matches!(kind, Kind::National | Kind::Dbcs { .. }) { 2 } else { 1 };
            let (from, length) = loc::refmod(len / unit, start, length, self.ssrange, &r.name, r.pos)?;
            offset += from * unit;
            len = length * unit;
            kind = match kind {
                Kind::National => Kind::National,
                Kind::Dbcs { .. } => Kind::Dbcs { justified: false, edit: None },
                _ => Kind::Alnum { justified: false },
            };
        }
        let (offset, len) = loc::within(offset, len, self.unit.mem.len(), &r.name, r.pos)?;
        let loc = Loc { offset, len, kind, item: index };
        self.unit.taint_read(loc);
        Ok(loc)
    }

    /// Whether the objects of these tables' OCCURS DEPENDING ON all lie within item `group`.
    fn objects_within(&mut self, tables: &[usize], group: usize) -> R<bool> {
        for &t in tables {
            if !self.object_within(t, group)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// The bytes of OCCURS DEPENDING ON table `t` past its current count of occurrences.
    fn unused(&mut self, t: usize, pos: Pos) -> R<i64> {
        let table = &self.layout.items[t];
        let (max, element) = (table.occurs, table.size);
        let current = self.occurrences(t, pos)?;
        Ok(loc::unused(max, current, element))
    }

    /// The bytes the OCCURS DEPENDING ON tables ahead of item `member` in its record, and not ahead
    /// of the group `holder` it is in, move it back from where the group's layout puts it.
    fn moved_within(&mut self, member: usize, holder: usize, pos: Pos) -> R<usize> {
        let layout = self.layout;
        let mut moved = 0;
        for &t in layout.items[member].moved_by.iter().filter(|t| !layout.items[holder].moved_by.contains(t)) {
            moved += self.unused(t, pos)? as usize;
        }
        Ok(moved)
    }

    /// Whether the object of table `t`'s OCCURS DEPENDING ON lies within item `group`.
    fn object_within(&mut self, t: usize, group: usize) -> R<bool> {
        let layout = self.layout;
        let Some(object) = &layout.items[t].depending_on else { return Ok(false) };
        let Resolved::Item(mut at) = self.resolve(object)? else { return Ok(false) };
        loop {
            if at == group {
                return Ok(true);
            }
            match layout.items[at].parent {
                Some(p) => at = p,
                None => return Ok(false),
            }
        }
    }

    /// The current count of an OCCURS DEPENDING ON table, kept within its declared maximum so that
    /// a bad count never reaches past the table's storage.
    fn occurrences(&mut self, table: usize, pos: Pos) -> R<u32> {
        let layout = self.layout;
        let item = &layout.items[table];
        let Some(object) = &item.depending_on else { return Ok(item.occurs) };
        let count = self.integer(&Expr::Operand(Operand::Ref(object.clone())), pos)?;
        loc::occurrences(count, item.occurs, self.ssrange, &object.name, pos)
    }

    fn integer(&mut self, e: &Expr, pos: Pos) -> R<i64> {
        let dmax = self.dmax(e)?;
        let v = self.eval_fixed(e, dmax, pos)?;
        let whole = align(&v, 0, false).and_then(|m| m.to_u128()).and_then(|m| i64::try_from(m).ok());
        let whole = whole.ok_or_else(|| Abend::ironwork("an integer operand beyond 64 bits", pos))?;
        Ok(if v.negative { -whole } else { whole })
    }

    fn literal_value(&self, lit: &Literal, pos: Pos) -> R<Val> {
        compile::values::literal_value(self.page, lit, self.options.arith.float_intermediate(), pos)
    }

    /// What a DECIMAL-POINT IS COMMA program shows for a decimal point.
    fn decimal_point(&self) -> char {
        if self.program.environment.decimal_point_comma { ',' } else { '.' }
    }

    fn default_currency(&self) -> String {
        numval_currency(&self.program.environment.currency)
    }

    fn operand_with_loc(&mut self, op: &Operand, pos: Pos) -> R<(Val, Option<Loc>)> {
        if let Operand::Ref(r) = op {
            let loc = self.locate(r)?;
            self.numcheck(loc, false, r.pos)?;
            return Ok((store::read(&self.facts(), &self.unit.mem, loc, r.pos)?, Some(loc)));
        }
        Ok((self.operand(op, pos)?, None))
    }

    /// NUMCHECK's test of a sending item, under the option (`rt::store::numcheck`).
    fn numcheck(&mut self, loc: Loc, as_integer: bool, pos: Pos) -> R<()> {
        if self.options.numcheck.is_none() {
            return Ok(());
        }
        let facts = self.facts();
        rt::store::numcheck(&facts, self.unit, loc, as_integer, &self.program.id, pos)
    }

    /// A MOVE's sender, tested as `rt::store::move_check` says.
    fn move_source(&mut self, from: &Operand, dest: Loc, pos: Pos) -> R<(Val, Option<Loc>)> {
        let Operand::Ref(r) = from else { return self.operand_with_loc(from, pos) };
        let loc = self.locate(r)?;
        let check = store::move_check(&self.options, loc.kind, dest.kind);
        store::numcheck_sender(&self.facts(), self.unit, loc, check, &self.program.id, r.pos)?;
        Ok((store::move_sender(&self.facts(), &self.unit.mem, loc, dest, r.pos)?, Some(loc)))
    }

    fn operand(&mut self, op: &Operand, pos: Pos) -> R<Val> {
        match op {
            Operand::Ref(r) => {
                let loc = self.locate(r)?;
                self.numcheck(loc, false, r.pos)?;
                store::read(&self.facts(), &self.unit.mem, loc, r.pos)
            }
            Operand::Literal(lit) => self.literal_value(lit, pos),
            Operand::LengthOf(r) => {
                // A LINKAGE item's length is the compile's, less what its OCCURS DEPENDING ON
                // tables do not hold, read without its address (Language Reference, LENGTH OF).
                let layout = self.layout;
                let linkage = match layout.resolve(&r.name, &r.qualifiers, r.pos) {
                    Ok(Resolved::Item(i)) if r.refmod.is_none() && layout.items[i].linkage.is_some() => Some(i),
                    _ => None,
                };
                let len = match linkage {
                    Some(i) => {
                        let mut len = i64::from(layout.items[i].size);
                        for &t in &layout.items[i].odo {
                            len -= self.unused(t, r.pos)?;
                        }
                        len as usize
                    }
                    None => self.locate(&layout.length_of_ref(r))?.len,
                };
                Ok(Val::Num(Fixed::new(len as i128, Places::new(9, 0))))
            }
            Operand::Function(f) => self.function(f, Within::Own),
            Operand::AddressOf(r) => Ok(Val::Address(self.address_of(r)?)),
        }
    }

    fn set_integer(&mut self, r: &Ref, value: i64, pos: Pos) -> R<()> {
        let dest = self.locate(r)?;
        store::set_integer(&self.facts(), self.unit, dest, value, pos)
    }

    fn overflow_branch(&mut self, overflow: bool, on: &'p Option<Vec<Stmt>>, not_on: &'p Option<Vec<Stmt>>) -> R<Flow> {
        match (overflow, on, not_on) {
            (true, Some(body), _) | (false, _, Some(body)) => self.run_block(body),
            _ => Ok(Flow::Next),
        }
    }

    fn string_stmt(&mut self, st: &'p StringStmt) -> R<Flow> {
        let sources: Vec<_> = st.sources.iter().map(|(op, d)| StringSource { chars: facts::chars(op), delimiter: match d {
            Delimiter::Size => None,
            Delimiter::By(d) => Some(facts::chars(d)),
        } }).collect();
        let overflow = rt::text::string(self, &st.into, st.pointer.as_ref(), &sources, st.pos)?;
        self.overflow_branch(overflow, &st.on_overflow, &st.not_on_overflow)
    }

    fn unstring(&mut self, u: &'p Unstring) -> R<Flow> {
        let delimiters: Vec<_> = u.delimiters.iter().map(|(all, d)| (*all, facts::chars(d))).collect();
        let into: Vec<_> = u.into.iter().map(|i| UnstringField { target: &i.target, delimiter: i.delimiter_in.as_ref(), count: i.count_in.as_ref() }).collect();
        let overflow = rt::text::unstring(self, &facts::chars(&u.source), u.pointer.as_ref(), &delimiters, &into, u.tallying.as_ref(), u.pos)?;
        self.overflow_branch(overflow, &u.on_overflow, &u.not_on_overflow)
    }

    fn inspect(&mut self, i: &Inspect) -> R<()> {
        let tallying: Vec<_> = i.tallying.iter().map(facts::inspect_phrase).collect();
        let Operand::Ref(target) = &i.target else {
            return rt::text::tally(self, &&i.target, &tallying, i.pos);
        };
        let replacing: Vec<_> = i.replacing.iter().map(facts::inspect_phrase).collect();
        let converting = i.converting.as_ref().map(|(from, to, bounds)| Converting { table: ConvertTable::Operands { from: facts::chars(from), to: facts::chars(to) }, bounds: facts::bounds(bounds) });
        rt::text::inspect(self, target, &tallying, &replacing, converting.as_ref(), i.pos)
    }

    fn search(&mut self, se: &'p Search) -> R<Flow> {
        let pos = se.pos;
        let Resolved::Item(t) = self.resolve(&se.table)? else {
            return Err(Abend::ironwork(format!("SEARCH {}: not a table", se.table.name), pos));
        };
        let layout = self.layout;
        let table = &layout.items[t];
        let count = self.occurrences(t, pos)? as i64;
        let Some(index) = table.search_index(se.varying.as_ref(), pos) else {
            return Err(Abend::ironwork(format!("SEARCH {}: the table has no INDEXED BY", se.table.name), pos));
        };
        let index_expr = Expr::Operand(Operand::Ref(index.clone()));
        if !se.all {
            loop {
                let i = self.integer(&index_expr, pos)?;
                if i < 1 || i > count {
                    return match &se.at_end {
                        Some(body) => self.run_block(body),
                        None => Ok(Flow::Next),
                    };
                }
                for (cond, body) in &se.whens {
                    if self.condition(cond, pos)? {
                        return self.run_block(body);
                    }
                }
                self.set_integer(&index, i + 1, pos)?;
                if let Some(v) = &se.varying
                    && v.name != index.name
                {
                    let current = self.integer(&Expr::Operand(Operand::Ref(v.clone())), pos)?;
                    self.set_integer(v, current + 1, pos)?;
                }
            }
        }
        let (cond, body) = &se.whens[0];
        let mut terms = Vec::new();
        flatten_and(cond, &mut terms);
        let (mut low, mut high) = (1i64, count);
        while low <= high {
            let mid = (low + high) / 2;
            self.set_integer(&index, mid, pos)?;
            let mut order = Ordering::Equal;
            for (ascending, key) in &table.keys {
                let Some((subject, value)) = key_term(&terms, &key.name) else { continue };
                let o = self.compare(subject, value, pos)?;
                order = if *ascending { o } else { o.reverse() };
                if order != Ordering::Equal {
                    break;
                }
            }
            match order {
                Ordering::Less => low = mid + 1,
                Ordering::Greater => high = mid - 1,
                Ordering::Equal => {
                    return if self.condition(cond, pos)? {
                        self.run_block(body)
                    } else {
                        match &se.at_end {
                            Some(b) => self.run_block(b),
                            None => Ok(Flow::Next),
                        }
                    };
                }
            }
        }
        match &se.at_end {
            Some(b) => self.run_block(b),
            None => Ok(Flow::Next),
        }
    }

    /// ADDRESS OF: the item's address, or NULL for a LINKAGE record with none yet.
    fn address_of(&mut self, r: &Ref) -> R<u32> {
        if let Ok(Resolved::Item(i)) = self.resolve(r)
            && let Some(l) = self.layout.items[i].linkage
            && self.linkage[l as usize].is_none()
        {
            return Ok(0);
        }
        let loc = self.locate(r)?;
        Ok(ADDRESS_BASE + loc.offset as u32)
    }

    fn program_name(&mut self, op: &Operand, pos: Pos) -> R<String> {
        Ok(match self.operand(op, pos)? {
            Val::Bytes(b) => self.page.decode(&b).trim().to_ascii_uppercase(),
            _ => return Err(Abend::ironwork("a program name must be alphanumeric", pos)),
        })
    }

    /// Tells the observer, for the input trace, the operand of an operation an input could steer.
    pub(crate) fn sink(&mut self, kind: &'static str, pos: Pos, operand: &str) {
        let file = self.event_file(pos);
        let input = self.unit.input_at_sink();
        self.unit.notify(Event::Sink { kind, file: &file, line: pos.line, operand, input });
    }

    /// The file an event names for `pos`: a library program's own source by its path, a COPY
    /// member by the program's file table, and the first program's own source as empty.
    fn event_file(&self, pos: Pos) -> String {
        match (pos.file, &self.unit.programs[self.me].source) {
            (0, Some(path)) => path.to_str().unwrap_or_default().to_owned(),
            (i, _) => self.program.sources.get(i as usize).cloned().unwrap_or_default(),
        }
    }

    fn arguments_text(&mut self, c: &Call) -> R<String> {
        callee::arguments_text(self, &call_args(&c.using), c.pos)
    }

    fn call(&mut self, c: &'p Call) -> R<Flow> {
        let pos = c.pos;
        let (name, dynamic) = match self.entry_pointer(&c.target)? {
            Some(entry) => (entry.name, entry.dynamic),
            None => {
                if let Some(flow) = self.call_through_pointer(c)? {
                    return Ok(flow);
                }
                let name = self.program_name(&c.target, pos)?;
                let variable = !matches!(c.target, Operand::Literal(_));
                if variable && self.unit.observed() {
                    self.sink("dynamic-program-load", pos, &name);
                }
                (name, self.options.dynam || variable)
            }
        };
        // ironwork runs no operating-system command: the CALL loads a program of that name or fails.
        if self.unit.observed()
            && OS_COMMAND_ROUTINES.contains(&name.as_str())
            && let Ok(text) = self.arguments_text(c)
        {
            self.sink("os-command", pos, &text);
        }
        if self.options.le_services == LeServices::Bind && crate::le::provides(&name) {
            return self.le_call(c, &name);
        }
        let strict = self.options.program_scope == ProgramScope::Strict;
        let found = if strict && callee::names(self.program.hidden.iter().map(String::as_str), &name) {
            Err(LoadError::NotFound)
        } else {
            self.unit.load_entry(&name, rt::callee::entry_copy(dynamic, self.options.dialect_of(Switched::EntryCalls)))
        };
        let found = found.and_then(|(index, entry)| {
            let contained = self.unit.programs[index].compiled.as_deref().is_some_and(|t| !t.program.containers.is_empty());
            match strict && contained && !callee::names(self.program.callable.iter().map(String::as_str), &name) {
                true => Err(LoadError::NotFound),
                false => Ok((index, entry)),
            }
        });
        let (index, entry) = match found {
            Ok(i) => i,
            Err(LoadError::NotFound) if crate::le::provides(&name) => return self.le_call(c, &name),
            Err(LoadError::NotFound) => {
                if let Some(flow) = self.virtual_print(c, &name)? {
                    return Ok(flow);
                }
                return match &c.on_exception {
                    Some(body) => self.run_block(body),
                    None => Err(crate::le::not_found(&format!("CALL {name}"), &name, dynamic, pos)),
                };
            }
            Err(LoadError::Compile(message)) => return Err(Abend::ironwork(format!("CALL {name}: {message}"), pos)),
        };
        let held = self.unit.programs[index].compiled.clone();
        let Some(compiled) = held.as_deref().or(self.first) else {
            return Err(Abend::ironwork(format!("CALL {name}: the run unit's first program cannot be CALLed from a function or a method"), pos));
        };
        self.unit.programs[index].dynamic |= dynamic;
        let program = &compiled.program;
        if self.unit.programs[index].active && !program.recursive {
            let unit = program.containers.last().map_or(&program.id, |c| &c.id);
            return Err(callee::recursive_call(&program.id, unit, pos));
        }
        self.unit.enter(pos)?;
        let result = self.call_nested(c, index, entry, compiled, dynamic);
        self.unit.depth -= 1;
        if let Some(flow) = result? {
            return Ok(flow);
        }
        match &c.not_on_exception {
            Some(body) => self.run_block(body),
            None => Ok(Flow::Next),
        }
    }

    /// A CALL no library answers that the virtual printer serves: SYSTEM or C$SYSTEM with an lp or
    /// lpr command, in a run given DD PRINTER. It returns lp's status, 0 printed or 1 not, through
    /// RETURNING or else RETURN-CODE.
    fn virtual_print(&mut self, c: &'p Call, name: &str) -> R<Option<Flow>> {
        use rt::virtual_printer::{self, Job};
        if !virtual_printer::ROUTINES.contains(&name) {
            return Ok(None);
        }
        let Some(printer) = self.unit.dds.get(virtual_printer::DD) else { return Ok(None) };
        let Some(job) = self.arguments_text(c).ok().as_deref().and_then(Job::parse) else { return Ok(None) };
        let dds = self.unit.dds.clone();
        let status: i16 = match virtual_printer::print(&dds, &printer, &job, &mut |event| self.unit.notify(event)) {
            Ok(()) => 0,
            Err(why) => {
                let _ = writeln!(self.unit.err, "ironwork: {}: CALL {name}: the virtual printer printed nothing: {why}", c.pos);
                1
            }
        };
        match &c.returning {
            Some(target) => {
                let dest = self.locate(target)?;
                self.assign(dest, Val::Num(Fixed::new(i128::from(status), Places::new(9, 0))), None, c.pos)?;
            }
            None => self.unit.set_return_code(i32::from(status), self.options.emulates_cobc()),
        }
        Ok(Some(match &c.not_on_exception {
            Some(body) => self.run_block(body)?,
            None => Flow::Next,
        }))
    }

    /// The entry a CALL's function-pointer or procedure-pointer holds, when SET TO ENTRY set it.
    fn entry_pointer(&mut self, target: &Operand) -> R<Option<rt::set::Entry>> {
        let Operand::Ref(r) = target else { return Ok(None) };
        let Ok(Resolved::Item(item)) = self.resolve(r) else { return Ok(None) };
        if self.layout.items[item].kind != Kind::ProgramPointer {
            return Ok(None);
        }
        let loc = self.locate(r)?;
        let Ok(value) = <[u8; 4]>::try_from(store::bytes(&self.unit.mem, loc)).map(u32::from_be_bytes) else { return Ok(None) };
        Ok(rt::set::entry_of(&self.unit.entries, value).cloned())
    }

    /// SET TO ENTRY's entry: its name and whether a CALL through it is dynamic, loaded when the
    /// SET runs (C140).
    fn entry_named(&mut self, entry: &Operand, pos: Pos) -> R<(String, bool)> {
        let name = self.program_name(entry, pos)?;
        let variable = !matches!(entry, Operand::Literal(_));
        if variable && self.unit.observed() {
            self.sink("dynamic-program-load", pos, &name);
        }
        let dynamic = self.options.dynam || variable;
        match self.unit.load_entry(&name, rt::callee::entry_copy(dynamic, self.options.dialect_of(Switched::EntryCalls))) {
            Ok(_) => Ok((name, dynamic)),
            Err(LoadError::NotFound) if crate::le::provides(&name) => Ok((name, dynamic)),
            Err(LoadError::NotFound) => Err(crate::le::not_found(&format!("SET TO ENTRY {name}"), &name, dynamic, pos)),
            Err(LoadError::Compile(message)) => Err(Abend::ironwork(format!("SET TO ENTRY {name}: {message}"), pos)),
        }
    }

    /// The CALL's callee run, through RETURNING; `Some` when the run unit or the logical level ends
    /// (C233). NOT ON EXCEPTION is the caller's, after the CALL's nesting is released, as INVOKE's
    /// and an LE service's are.
    fn call_nested(&mut self, c: &'p Call, index: usize, entry: Option<usize>, compiled: &Compiled, dynamic: bool) -> R<Option<Flow>> {
        let pos = c.pos;
        let mark = self.unit.mem.len();
        let (addresses, lengths) = callee::arguments(self, &call_args(&c.using), pos)?;
        self.parmcheck_set();
        // A dynamic CALL suspends the caller's handlers, as CBLPSHPOP(ON) does (C234).
        let suspends = dynamic && compiled.program.containers.is_empty();
        let containers = self.containers_of(&compiled.program);
        let by = By::Call { initial: compiled.program.initial };
        // A CALL with RETURNING does not set RETURN-CODE (Language Reference, CALL statement); cobc's does.
        let kept = c.returning.as_ref().filter(|_| !self.options.emulates_cobc()).map(|_| self.unit.kept_return_code());
        let (ending, returned) = callee::run(self, &Callee { index, by, mark: Some(mark), pos, lengths: &lengths }, |m| {
            let mut callee = Machine::activation_within(compiled, index, &mut *m.unit, false, containers)?;
            let entry = entry.and_then(|k| compiled.entries.get(k));
            callee.bind_linkage(&[], entry.map_or(&compiled.program.using, |e| &e.using), &addresses, true);
            (callee.cics_handlers, callee.first) = (m.cics_handlers.lend(suspends), m.first);
            let ending = callee.run_called(entry.map(|e| (e.paragraph, e.statement)));
            m.cics_handlers.take_back(&mut callee.cics_handlers, suspends && ending.is_ok());
            let returned = match (&compiled.program.returning, &ending) {
                (Some(item), Ok(_)) => Some(callee.returned(item, pos)?),
                _ => None,
            };
            Ok::<_, Abend>((ending, returned))
        })?;
        if ending? == Ending::StopRun {
            return Ok(Some(Flow::End(Ending::StopRun)));
        }
        if crate::cics::level_ended(self.unit) {
            return Ok(Some(Flow::End(Ending::Goback)));
        }
        self.parmcheck_test(c, &addresses, |unit| unit.programs[index].name.clone())?;
        // A program with no RETURNING phrase gives its RETURN-CODE, compiled for GnuCOBOL (C491).
        let returned = returned.or_else(|| self.options.emulates_cobc().then(|| self.unit.return_code_value(true)));
        if let Some(kept) = kept {
            self.unit.restore_return_code(kept);
        }
        if let (Some(target), Some(val)) = (&c.returning, returned) {
            let dest = self.locate_written(|m| m.locate(target))?;
            self.assign(dest, val, None, pos)?;
        }
        Ok(None)
    }

    /// Gives each PROCEDURE DIVISION USING item the address of the argument in its position.
    pub(crate) fn bind(&mut self, addresses: &[Option<usize>]) {
        let program = self.program;
        self.bind_linkage(&[], &program.using, addresses, false);
    }

    /// Binds this activation's LINKAGE records as `rt::callee::Bindings` does: the object's data
    /// `records`, each item of a PROCEDURE DIVISION or ENTRY USING list the argument in its position,
    /// and with `returning` the RETURNING item.
    fn bind_linkage(&mut self, records: &[(usize, usize)], using: &[Param], addresses: &[Option<usize>], returning: bool) {
        let layout = self.layout;
        let ordinal = |name: &str| layout.linkage_roots.iter().position(|&i| layout.items[i].name.as_deref() == Some(name));
        let using = using.iter().map(|param| ordinal(&param.name)).collect();
        let returning = self.program.returning.as_deref().filter(|_| returning).and_then(ordinal).map(|o| (o, layout.items[layout.linkage_roots[o]].size as usize));
        Bindings { records, using, addresses, returning }.bind(self.unit, &mut self.linkage);
    }

    fn returned(&mut self, name: &str, pos: Pos) -> R<Val> {
        let r = Ref { name: name.to_owned(), qualifiers: Vec::new(), subscripts: Vec::new(), refmod: None, pos };
        let loc = self.locate(&r)?;
        store::read(&self.facts(), &self.unit.mem, loc, pos)
    }

    fn set(&mut self, set: &SetStmt, pos: Pos) -> R<()> {
        match set {
            SetStmt::ConditionTrue(targets) | SetStmt::ConditionFalse(targets) => {
                let truth = matches!(set, SetStmt::ConditionTrue(_));
                for r in targets {
                    let Resolved::Condition(index) = self.resolve(r)? else {
                        return Err(Abend::ironwork(format!("SET {} TO {}: not a condition-name", r.name, if truth { "TRUE" } else { "FALSE" }), pos));
                    };
                    let condition = &self.layout.conditions[index];
                    let value = if truth { condition.values.first().map(|(v, _)| v) } else { condition.false_value.as_ref() };
                    let Some(value) = value else { continue };
                    let dest = self.locate_written(|m| m.locate_item(condition.item, r, false))?;
                    let val = self.literal_value(value, pos)?;
                    self.assign(dest, val, None, pos)?;
                }
            }
            SetStmt::To { targets, value } => {
                for r in targets {
                    let dest = self.locate_written(|m| m.locate(r))?;
                    let (val, src) = self.operand_with_loc(value, pos)?;
                    let (val, src) = rt::set::to(dest, val, src, pos)?;
                    self.assign(dest, val, src, pos)?;
                }
            }
            SetStmt::Entry { targets, entry } => {
                let (name, dynamic) = self.entry_named(entry, pos)?;
                let value = rt::set::entry(&mut self.unit.entries, &name, dynamic, pos)?;
                for r in targets {
                    let dest = self.locate_written(|m| m.locate(r))?;
                    self.assign(dest, Val::Address(value), None, pos)?;
                }
            }
            SetStmt::AddressOf { targets, value } => {
                let val = self.operand(value, pos)?;
                let offset = rt::set::address(val, self.unit.mem.len(), pos)?;
                for r in targets {
                    let Resolved::Item(i) = self.resolve(r)? else {
                        return Err(Abend::ironwork(format!("SET ADDRESS OF {}: not a data item", r.name), pos));
                    };
                    let Some(ordinal) = self.layout.items[i].linkage.filter(|_| self.layout.items[i].parent.is_none()) else {
                        return Err(Abend::ironwork(format!("SET ADDRESS OF {}: only a LINKAGE record can be given an address", r.name), pos));
                    };
                    self.linkage[ordinal as usize] = offset;
                }
            }
            SetStmt::UpDown { targets, down, by } => {
                let by = self.integer(by, pos)?;
                let targets: Vec<&Ref> = targets.iter().collect();
                rt::set::up_down(self, by, *down, &targets, pos)?;
            }
            SetStmt::Switches(_) => return Err(Abend::ironwork("SET ... TO ON or OFF reached the interpreter, which runs it as the SET ... TO TRUE compile makes it", pos)),
        }
        Ok(())
    }

    /// True when ARGUMENT-VALUE finds no word left.
    fn accept(&mut self, target: &Ref, from: AcceptFrom, pos: Pos) -> R<bool> {
        let dest = self.locate_written(|m| m.locate_receiving(target))?;
        rt::accept::accept(&self.facts(), self.unit, dest, from, &target.name, pos)
    }

    fn function(&mut self, f: &FunctionCall, within: Within) -> R<Val> {
        if let Some(udf) = self.user_function(&f.name) {
            let value = self.invoke_function(udf, f)?;
            return self.function_refmod(f, value);
        }
        if let Some(value) = self.storage_function(f)? {
            return self.function_refmod(f, value);
        }
        let mut args = self.function_arguments(f, within)?;
        let side = match f.modifier.as_deref() {
            Some("LEADING") => Some(TrimSide::Leading),
            Some("TRAILING") => Some(TrimSide::Trailing),
            _ => None,
        };
        let value = rt::intrinsic::function::evaluate(&mut intrinsic::Call { machine: self, f }, &f.name, side, &mut args, f.pos)?;
        self.function_refmod(f, value)
    }

    fn function_refmod(&mut self, f: &FunctionCall, value: Val) -> R<Val> {
        let pos = f.pos;
        let Some(rm) = &f.refmod else { return Ok(value) };
        rt::intrinsic::function::refmod(value, pos, || {
            let start = self.integer(&rm.start, pos)?;
            let length = match &rm.length {
                Some(l) => Some(self.integer(l, pos)?),
                None => None,
            };
            Ok((start, length))
        })
    }

    fn expr_value(&mut self, e: &Expr, pos: Pos) -> R<Val> {
        match e {
            Expr::Operand(op) => self.operand(op, pos),
            _ if self.uses_float(e)? || (divided_exponent(e) && self.static_dmax(e) > 0) => Ok(Val::Float(self.eval_float(e, self.options.arith.float_intermediate(), pos)?)),
            _ => {
                let dmax = self.dmax(e)?;
                Ok(Val::Num(self.eval_fixed(e, dmax, pos)?))
            }
        }
    }

    fn operand_kind(&mut self, op: &Operand) -> R<Option<Kind>> {
        Ok(match op {
            Operand::Ref(r) => Some(self.locate(r)?.kind),
            Operand::Function(f) => self.user_function(&f.name).map(|u| u.result.kind),
            _ => None,
        })
    }

    /// Fixed at lowering as `ArithStep.mode` (lower/plans.rs); the walker decides it on each execution.
    fn uses_float(&mut self, e: &Expr) -> R<bool> {
        Ok(match e {
            Expr::Operand(Operand::Function(f)) => self.is_floating_point(f)?,
            Expr::Operand(op) => matches!(self.operand_kind(op)?, Some(Kind::Float(_))),
            Expr::Neg(inner) => self.uses_float(inner)?,
            Expr::Bin(a, BinOp::Pow, b) => self.uses_float(a)? || self.uses_float(b)? || decimal_exponent(b, &mut |op| Ok::<_, Abend>(self.static_scale(op)))?,
            Expr::Bin(a, _, b) => self.uses_float(a)? || self.uses_float(b)?,
        })
    }

    /// An operand's decimal places from its description alone, as `dmax` finds them.
    fn static_scale(&mut self, op: &Operand) -> u32 {
        let kind = match op {
            Operand::Ref(r) if r.refmod.is_none() => match self.resolve(r) {
                Ok(Resolved::Item(i)) => Some(self.layout.items[i].kind),
                _ => None,
            },
            Operand::Function(f) => self.user_function(&f.name).map(|u| u.result.kind),
            _ => None,
        };
        kind.and_then(Kind::digits_scale).map_or(0, |(_, s)| s)
    }

    /// `dmax` without locating the operands.
    fn static_dmax(&mut self, e: &Expr) -> u32 {
        match e {
            Expr::Operand(Operand::Literal(Literal::Number(t))) => literal_fixed(t).map_or(0, |f| f.places.dec),
            Expr::Operand(op) => self.static_scale(op),
            Expr::Neg(inner) => self.static_dmax(inner),
            Expr::Bin(a, BinOp::Div | BinOp::Pow, _) => self.static_dmax(a),
            Expr::Bin(a, _, b) => self.static_dmax(a).max(self.static_dmax(b)),
        }
    }

    /// The most decimal places among an expression's operands, divisors and exponents aside. Fixed at
    /// lowering as `ArithPlan.dmax` (lower/plans.rs); the walker works it out on each execution.
    fn dmax(&mut self, e: &Expr) -> R<u32> {
        Ok(match e {
            Expr::Operand(Operand::Literal(Literal::Number(t))) => literal_fixed(t).map_or(0, |f| f.places.dec),
            Expr::Operand(Operand::Function(f)) => function_dmax(self.layout, self.functions, f),
            Expr::Operand(op) => self.operand_kind(op)?.and_then(Kind::digits_scale).map_or(0, |(_, s)| s),
            Expr::Neg(inner) => self.dmax(inner)?,
            Expr::Bin(a, BinOp::Div | BinOp::Pow, _) => self.dmax(a)?,
            Expr::Bin(a, _, b) => self.dmax(a)?.max(self.dmax(b)?),
        })
    }

    fn eval_fixed(&mut self, e: &Expr, dmax: u32, pos: Pos) -> R<Fixed> {
        self.eval_fixed_at(e, dmax, dmax, dmax, pos)
    }

    /// `e`'s top operation at `last` places and every operation below it at `inner`, a function's
    /// arguments at the statement's `dmax`.
    fn eval_fixed_at(&mut self, e: &Expr, last: u32, inner: u32, dmax: u32, pos: Pos) -> R<Fixed> {
        let arith = self.options.arith;
        match e {
            Expr::Operand(Operand::Function(f)) => {
                let val = self.function(f, Within::Fixed(dmax))?;
                arith::fixed_operand(val, last, pos)
            }
            Expr::Operand(op) => {
                let val = self.operand(op, pos)?;
                arith::fixed_operand(val, last, pos)
            }
            Expr::Neg(operand) => Ok(arith::fixed_neg(self.eval_fixed_at(operand, inner, inner, dmax, pos)?)),
            Expr::Bin(a, op, b) => {
                let x = self.eval_fixed_at(a, inner, inner, dmax, pos)?;
                if *op == BinOp::Pow {
                    let n = self.integer(b, pos)?;
                    return arith::pow(x, n, last, arith, self.options.emulates_cobc(), pos);
                }
                let y = self.eval_fixed_at(b, inner, inner, dmax, pos)?;
                if arith::divides_by_zero(*op, &y) {
                    let binary = self.binary_division(a, b)?;
                    return Err(arith::zero_divide(binary, pos));
                }
                arith::fixed_binop(x, *op, y, last, arith, pos)
            }
        }
    }

    /// Whether the compiler divides `a` by `b` with the fixed-point divide instruction: every
    /// operand of both an integer binary item or an integer literal, and one of them an item
    /// (assumption C55). Otherwise a fixed-point division is decimal.
    fn binary_division(&mut self, a: &Expr, b: &Expr) -> R<bool> {
        let mut items = 0;
        Ok(self.binary_operands(a, &mut items)? && self.binary_operands(b, &mut items)? && items > 0)
    }

    fn binary_operands(&mut self, e: &Expr, items: &mut usize) -> R<bool> {
        Ok(match e {
            Expr::Operand(Operand::Literal(Literal::Number(t))) => !t.contains('.'),
            Expr::Operand(Operand::Literal(Literal::Figurative(Figurative::Zero))) => true,
            Expr::Operand(Operand::LengthOf(_)) => {
                *items += 1;
                true
            }
            Expr::Operand(op @ Operand::Ref(_)) => {
                let binary = matches!(self.operand_kind(op)?, Some(Kind::Binary { scale: 0, .. } | Kind::Index));
                *items += usize::from(binary);
                binary
            }
            Expr::Operand(_) => false,
            Expr::Neg(inner) => self.binary_operands(inner, items)?,
            Expr::Bin(x, _, y) => self.binary_operands(x, items)? && self.binary_operands(y, items)?,
        })
    }

    fn eval_float(&mut self, e: &Expr, p: Precision, pos: Pos) -> R<Hfp> {
        match e {
            Expr::Operand(Operand::Function(f)) => {
                let val = self.function(f, Within::Float(p))?;
                arith::float_operand(val, p, pos)
            }
            Expr::Operand(op) => {
                let val = self.operand(op, pos)?;
                arith::float_operand(val, p, pos)
            }
            Expr::Neg(inner) => Ok(arith::float_neg(self.eval_float(inner, p, pos)?)),
            Expr::Bin(a, op, b) => {
                let (x, y) = (self.eval_float(a, p, pos)?, self.eval_float(b, p, pos)?);
                arith::float_binop(x, *op, y, p, self.options.emulates_cobc(), pos)
            }
        }
    }

    /// COMPUTE, ADD, SUBTRACT, MULTIPLY, DIVIDE: what the receivers share is computed before any is
    /// stored, then each receiver in turn gets it, or with `per_receiver` combines it with its own
    /// current value (Language Reference SC27-8713-03, p. 298, multiple results). A size error
    /// leaves the target unchanged when the statement handles it.
    fn arithmetic(&mut self, computations: &[(Target, Expr)], remainder: Option<&(Target, Expr, Expr)>, handler: Option<&'p SizeError>, per_receiver: bool, pos: Pos) -> R<Flow> {
        let mut size_error = false;
        let mut places = Dmax::default();
        // A COMP-1 or COMP-2 receiver makes the statement's arithmetic floating point (Programming
        // Guide SC27-8714-03, p. 800).
        let mut float_receiver = false;
        for (t, e) in computations {
            let loc = self.locate(&t.r)?;
            float_receiver |= matches!(loc.kind, Kind::Float(_));
            places = places.max(Dmax::receiver(loc.kind.digits_scale().map_or(0, |(_, s)| s), t.rounded, self.options.extra_place())).with(self.dmax(e)?);
        }
        if let Some((t, dividend, _)) = remainder {
            let loc = self.locate(&t.r)?;
            places = places.with(loc.kind.digits_scale().map_or(0, |(_, s)| s)).with(self.dmax(dividend)?);
        }
        let dmax = places.last;
        let mut quotient_target: Option<Loc> = None;
        let mut results = Vec::with_capacity(computations.len());
        for (t, e) in computations {
            let own = |x: &Expr| per_receiver && matches!(x, Expr::Operand(Operand::Ref(r)) if *r == t.r);
            let float = float_receiver || self.uses_float(e)? || (divided_exponent(e) && dmax > 0);
            let (shared, with) = match e {
                Expr::Bin(a, op, b) if *op != BinOp::Pow && own(a) => (b.as_ref(), Some((*op, true))),
                Expr::Bin(a, op, b) if *op != BinOp::Pow && own(b) => (a.as_ref(), Some((*op, false))),
                _ => (e, None),
            };
            let outcome = if float {
                self.eval_float(shared, self.options.arith.float_intermediate(), pos).map(Val::Float)
            } else {
                let last = if with.is_some() { places.inner } else { dmax };
                self.eval_fixed_at(shared, last, places.inner, dmax, pos).map(Val::Num)
            };
            results.push((t, shared, with, outcome));
        }
        let operands = match remainder {
            Some((_, dividend, divisor)) => Some((self.eval_fixed(dividend, dmax, pos)?, self.eval_fixed(divisor, dmax, pos)?)),
            None => None,
        };
        for (k, (t, shared, with, outcome)) in results.into_iter().enumerate() {
            // cobc reads a single sending item again for each receiver, after the one before it
            // is stored (ADD X TO X Y).
            let outcome = match outcome {
                Ok(Val::Float(_)) if k > 0 && with.is_some() && self.options.emulates_cobc() && matches!(shared, Expr::Operand(Operand::Ref(_))) => {
                    self.eval_float(shared, self.options.arith.float_intermediate(), pos).map(Val::Float)
                }
                Ok(Val::Num(_)) if k > 0 && with.is_some() && self.options.emulates_cobc() && matches!(shared, Expr::Operand(Operand::Ref(_))) => {
                    self.eval_fixed_at(shared, places.inner, places.inner, dmax, pos).map(Val::Num)
                }
                outcome => outcome,
            };
            let loc = self.locate(&t.r)?;
            quotient_target.get_or_insert(loc);
            let outcome = match (with, outcome) {
                (Some((op, receiver_first)), Ok(Val::Num(value))) => {
                    let current = self.eval_fixed(&Expr::Operand(Operand::Ref(t.r.clone())), dmax, pos)?;
                    let (x, y) = if receiver_first { (current, value) } else { (value, current) };
                    if arith::divides_by_zero(op, &y) {
                        let receiver = Expr::Operand(Operand::Ref(t.r.clone()));
                        let binary = self.binary_division(&receiver, shared)?;
                        Err(arith::zero_divide(binary, pos))
                    } else {
                        arith::fixed_binop(x, op, y, dmax, self.options.arith, pos).map(Val::Num)
                    }
                }
                (Some((op, receiver_first)), Ok(Val::Float(value))) => {
                    let p = self.options.arith.float_intermediate();
                    let current = self.eval_float(&Expr::Operand(Operand::Ref(t.r.clone())), p, pos)?;
                    let (x, y) = if receiver_first { (current, value) } else { (value, current) };
                    arith::float_binop(x, op, y, p, self.options.emulates_cobc(), pos).map(Val::Float)
                }
                (_, outcome) => outcome,
            };
            let Some(value) = arith::size_error(outcome, handler.is_some(), self.options.emulates_cobc())? else {
                size_error = true;
                continue;
            };
            size_error |= store::store_value(&self.facts(), self.unit, loc, value, t.rounded, handler.is_some(), pos)?;
        }
        if let (Some((t, _, _)), Some((x, y)), Some(q_loc)) = (remainder, operands, quotient_target)
            && let Some(r) = arith::remainder(x, y, places_of(q_loc.kind).dec, dmax, self.options.arith, pos)?
        {
            let r_loc = self.locate(&t.r)?;
            size_error |= store::store_value(&self.facts(), self.unit, r_loc, Val::Num(r), false, handler.is_some(), pos)?;
        }
        if let Some(h) = handler {
            return self.run_block(if size_error { &h.on } else { &h.not_on });
        }
        Ok(Flow::Next)
    }

    fn condition(&mut self, c: &Cond, pos: Pos) -> R<bool> {
        Ok(match c {
            Cond::Rel(a, op, b) => {
                let o = self.compare(a, b, pos)?;
                match op {
                    RelOp::Eq => o == Ordering::Equal,
                    RelOp::Ne => o != Ordering::Equal,
                    RelOp::Lt => o == Ordering::Less,
                    RelOp::Le => o != Ordering::Greater,
                    RelOp::Gt => o == Ordering::Greater,
                    RelOp::Ge => o != Ordering::Less,
                }
            }
            Cond::Not(inner) => !self.condition(inner, pos)?,
            Cond::And(a, b) => self.condition(a, pos)? && self.condition(b, pos)?,
            Cond::Or(a, b) => self.condition(a, pos)? || self.condition(b, pos)?,
            Cond::Class(e, class) => self.class(e, class, pos)?,
            Cond::NameOrRel { subject, op, negated, name } => match self.resolve(name)? {
                Resolved::Condition(_) => self.condition(&Cond::Name(name.clone()), pos)?,
                Resolved::Item(_) => self.condition(&Cond::Rel(subject.clone(), *op, Expr::Operand(Operand::Ref(name.clone()))), pos)? != *negated,
            },
            Cond::Name(r) => {
                let Resolved::Condition(index) = self.resolve(r)? else {
                    return Err(Abend::ironwork(format!("{} is a data item, not a condition", r.name), r.pos));
                };
                let condition = &self.layout.conditions[index];
                let loc = self.locate_item(condition.item, r, false)?;
                self.numcheck(loc, false, r.pos)?;
                let mut subject = None;
                for (low, high) in &condition.values {
                    let hit = match high {
                        None => self.compare_value(loc, &mut subject, low, r.pos, pos)? == Ordering::Equal,
                        Some(high) => self.compare_value(loc, &mut subject, low, r.pos, pos)? != Ordering::Less && self.compare_value(loc, &mut subject, high, r.pos, pos)? != Ordering::Greater,
                    };
                    if hit {
                        return Ok(true);
                    }
                }
                false
            }
        })
    }

    fn class(&mut self, e: &Expr, class: &Class, pos: Pos) -> R<bool> {
        if let Class::Named(name) = class {
            let (Expr::Operand(Operand::Ref(r)), Some(bits)) = (e, self.layout.class(name)) else {
                return Err(Abend::ironwork(format!("class-name {name} tests a data item the program defines a CLASS for"), pos));
            };
            let loc = self.locate(r)?;
            return Ok(store::byte_class(&self.facts(), &self.unit.mem, loc, ByteClass::Set { bits }));
        }
        if let (Class::Numeric | Class::Alphabetic | Class::AlphabeticLower | Class::AlphabeticUpper | Class::Dbcs | Class::Kanji, Expr::Operand(Operand::Ref(r))) = (class, e) {
            let loc = self.locate(r)?;
            let test = match (class, loc.kind) {
                (Class::Numeric, Kind::Packed { signed, .. }) => ByteClass::Packed { signed },
                (Class::Numeric, Kind::Zoned { signed, sign: None, .. }) => ByteClass::Zoned { signed },
                (Class::Numeric, _) => ByteClass::Digits,
                (Class::AlphabeticLower, _) => ByteClass::AlphabeticLower,
                (Class::AlphabeticUpper, _) => ByteClass::AlphabeticUpper,
                (Class::Dbcs, _) => ByteClass::Dbcs,
                (Class::Kanji, _) => ByteClass::Kanji,
                (_, _) => ByteClass::Alphabetic,
            };
            return Ok(store::byte_class(&self.facts(), &self.unit.mem, loc, test));
        }
        let test = match class {
            Class::Positive => SignTest::Positive,
            Class::Negative => SignTest::Negative,
            _ => SignTest::Zero,
        };
        store::sign_test(self.expr_value(e, pos)?, test, pos)
    }

    /// Object references are compared here; `rt::store::compare` compares everything else.
    fn compare(&mut self, a: &Expr, b: &Expr, pos: Pos) -> R<Ordering> {
        for (zoned, other, zoned_first) in [(a, b, true), (b, a, false)] {
            if let Some(image) = self.zoned_bytes_against(zoned, other)? {
                if let Expr::Operand(Operand::Ref(r)) = zoned
                    && self.checks_against(other)?
                {
                    let loc = self.locate(r)?;
                    self.numcheck(loc, false, r.pos)?;
                }
                let other = match other {
                    Expr::Operand(Operand::Ref(r)) if self.zone_sensitive(other)? => {
                        let loc = self.locate(r)?;
                        self.numcheck(loc, false, r.pos)?;
                        (Val::Bytes(Vec::new()), Some(loc))
                    }
                    _ => self.comparand_against(other, zoned, pos)?,
                };
                return store::compare_zoned_bytes(&self.facts(), &self.unit.mem, &image, other, zoned_first, pos);
            }
        }
        let (va, la) = self.comparand_against(a, b, pos)?;
        let (vb, lb) = self.comparand_against(b, a, pos)?;
        if let Some(o) = self.compare_references(a, b, (&va, la), (&vb, lb), pos)? {
            return Ok(o);
        }
        store::compare(&self.facts(), &self.unit.mem, (va, la), (vb, lb), pos)
    }

    /// Whether `e` is an alphanumeric item, literal or figurative constant other than ZERO.
    fn nonnumeric(&mut self, e: &Expr) -> R<bool> {
        Ok(match e {
            Expr::Operand(Operand::Literal(l)) => {
                matches!(l, Literal::Alnum(_) | Literal::Hex(_) | Literal::All(_)) || matches!(l, Literal::Figurative(f) if !matches!(f, Figurative::Zero | Figurative::Null))
            }
            Expr::Operand(Operand::Ref(o)) => store::nonnumeric(self.locate(o)?.kind),
            _ => false,
        })
    }

    /// Whether NUMCHECK tests a zoned item compared with `other`: always, but under ZON(NOALPHNUM)
    /// not against an alphanumeric operand (Programming Guide SC27-8714-03, pp. 389-390).
    fn checks_against(&mut self, other: &Expr) -> R<bool> {
        Ok(!store::noalphnum(&self.options) || !self.nonnumeric(other)?)
    }

    /// A comparand, NUMCHECK testing an item unless ZON(NOALPHNUM) exempts it against `other`.
    fn comparand_against(&mut self, e: &Expr, other: &Expr, pos: Pos) -> R<(Val, Option<Loc>)> {
        match e {
            Expr::Operand(Operand::Ref(r)) if !self.checks_against(other)? => {
                let loc = self.locate(r)?;
                Ok((store::read(&self.facts(), &self.unit.mem, loc, r.pos)?, Some(loc)))
            }
            _ => self.comparand(e, pos),
        }
    }

    /// The bytes of `e`, a zoned integer item, when `other` is nonnumeric: that comparison reads the
    /// item's bytes, never its value, so invalid data compares rather than abends.
    fn zoned_bytes_against(&mut self, e: &Expr, other: &Expr) -> R<Option<Vec<u8>>> {
        let Expr::Operand(Operand::Ref(r)) = e else { return Ok(None) };
        let nonnumeric = self.nonnumeric(other)?;
        // Where zones are compared, an unsigned zoned integer against zero or one of its own
        // length compares its zones too (assumptions C223, C262).
        let zones_count = (self.options.zones_compared_with_zero() || self.options.zones_compared_between_items())
            && self.zone_sensitive(e)?
            && match other {
                Expr::Operand(Operand::Literal(l)) => self.options.zones_compared_with_zero() && self.zero_literal(l),
                Expr::Operand(Operand::Ref(o)) => self.options.zones_compared_between_items() && self.zone_sensitive(other)? && self.locate(o)?.len == self.locate(r)?.len,
                _ => false,
            };
        if !nonnumeric && !zones_count {
            return Ok(None);
        }
        let loc = self.locate(r)?;
        Ok(store::compared_zoned_bytes(&self.facts(), &self.unit.mem, loc))
    }

    /// Whether `e` is an unsigned, unscaled zoned integer item.
    fn zone_sensitive(&mut self, e: &Expr) -> R<bool> {
        let Expr::Operand(Operand::Ref(r)) = e else { return Ok(false) };
        let loc = self.locate(r)?;
        Ok(self.zone_sensitive_at(loc))
    }

    fn zone_sensitive_at(&self, loc: Loc) -> bool {
        matches!(loc.kind, Kind::Zoned { scale: 0, signed: false, .. }) && self.layout.items.get(loc.item).is_none_or(|i| i.scaling == 0)
    }

    fn zero_literal(&self, l: &Literal) -> bool {
        match l {
            Literal::Figurative(Figurative::Zero) => true,
            Literal::Number(t) => literal_fixed(t).is_some_and(|f| store::zero(&Val::Num(f))),
            _ => false,
        }
    }

    /// A condition-name's value compared with its conditional variable at `loc` as the relation
    /// of the two would compare them: by the variable's bytes where `zoned_bytes_against` would
    /// take them, otherwise as numbers, the variable read once into `subject` at `at`.
    fn compare_value(&mut self, loc: Loc, subject: &mut Option<(Val, Option<Loc>)>, value: &Literal, at: Pos, pos: Pos) -> R<Ordering> {
        let nonnumeric = matches!(value, Literal::Alnum(_) | Literal::Hex(_) | Literal::All(_)) || matches!(value, Literal::Figurative(f) if !matches!(f, Figurative::Zero | Figurative::Null));
        let zones_count = self.options.zones_compared_with_zero() && self.zone_sensitive_at(loc) && self.zero_literal(value);
        if (nonnumeric || zones_count)
            && let Some(image) = store::compared_zoned_bytes(&self.facts(), &self.unit.mem, loc)
        {
            let value = self.literal_value(value, pos)?;
            return store::compare_zoned_bytes(&self.facts(), &self.unit.mem, &image, (value, None), true, pos);
        }
        let subject = match subject {
            Some(s) => s.clone(),
            None => subject.insert((store::read(&self.facts(), &self.unit.mem, loc, at)?, Some(loc))).clone(),
        };
        self.compare_literal(&subject, value, pos)
    }

    fn compare_literal(&mut self, subject: &(Val, Option<Loc>), literal: &Literal, pos: Pos) -> R<Ordering> {
        let value = self.literal_value(literal, pos)?;
        store::compare(&self.facts(), &self.unit.mem, subject.clone(), (value, None), pos)
    }

    fn comparand(&mut self, e: &Expr, pos: Pos) -> R<(Val, Option<Loc>)> {
        match e {
            Expr::Operand(op) => self.operand_with_loc(op, pos),
            _ => Ok((self.expr_value(e, pos)?, None)),
        }
    }

    fn display(&mut self, items: &[Operand], upon_console: bool, no_advancing: bool, pos: Pos) -> R<()> {
        let text = self.display_text(items, upon_console, pos)?;
        if self.unit.observed() {
            self.sink("log", pos, &text);
        }
        rt::display::write(&mut *self.unit.out, &text, no_advancing, pos)
    }

    /// The line and column a screen DISPLAY or ACCEPT is at, or None for the cursor: AT's number
    /// as LLCC or LLLCCC, or LINE and COLUMN, column 1 where only LINE is given and the cursor's
    /// line where only COLUMN is.
    fn screen_at(&mut self, screen: &ScreenPhrases, pos: Pos) -> R<Option<(usize, usize)>> {
        let number = |m: &mut Self, o: &Operand| -> R<usize> { Ok(m.integer(&Expr::Operand(o.clone()), pos)?.max(0) as usize) };
        Ok(match &screen.at {
            None => None,
            Some(ScreenAt::Combined(o)) => Some(rt::crt::line_column(number(self, o)? as u64)),
            Some(ScreenAt::LineColumn { line, column }) => {
                let row = match line {
                    Some(l) => number(self, l)?,
                    None => self.unit.crt.as_ref().map_or(1, |c| c.borrow().cursor_position().0),
                };
                let column = match column {
                    Some(c) => number(self, c)?,
                    None => 1,
                };
                Some((row, column))
            }
        })
    }

    /// DISPLAY's items as one line of text.
    fn display_text(&mut self, items: &[Operand], upon_console: bool, pos: Pos) -> R<String> {
        let mut text = String::new();
        for op in items {
            let shown = match op {
                Operand::Ref(r) => {
                    let loc = self.locate(r)?;
                    rt::display::place(&self.facts(), &self.unit.mem, loc, r.pos, upon_console)?
                }
                Operand::Literal(Literal::Number(t)) => rt::display::number(t, &self.facts()),
                other => {
                    let val = self.operand(other, pos)?;
                    rt::display::value(&self.facts(), val, pos, upon_console)?
                }
            };
            text.push_str(&shown);
        }
        Ok(text)
    }

    /// The implicit MOVEs of INITIALIZE to item `index` at `offset`.
    fn initialize(&mut self, index: usize, offset: usize, with: &InitializeWith, pos: Pos) -> R<()> {
        let layout = self.layout;
        for (i, at) in layout.initialize_receivers(index, with.filler) {
            let item = &layout.items[i];
            let loc = Loc { offset: offset + at as usize, len: item.size as usize, kind: item.kind, item: i };
            match (with.initial_value(layout.category(i), item.value.is_some()), &item.value) {
                (Some(InitialValue::Value), Some(value)) => {
                    let val = self.literal_value(value, pos)?;
                    self.assign(Loc { kind: value_kind(item.kind, value), ..loc }, val, None, pos)?;
                }
                (Some(InitialValue::Replacing(by)), _) => {
                    let (val, src) = self.operand_with_loc(by, pos)?;
                    self.assign(loc, val, src, pos)?;
                }
                (Some(_), _) => self.assign(loc, initial_default(item.kind), None, pos)?,
                (None, _) => {}
            }
        }
        Ok(())
    }
}

static NO_PHRASES: InitializeWith = InitializeWith { filler: false, value: Vec::new(), replacing: Vec::new(), default: false };

/// A CALL's USING phrase as `rt::callee` takes it: a data item BY REFERENCE by its place, BY VALUE
/// by its value, and anything else as BY CONTENT copies it.
fn call_args(using: &[Arg]) -> Vec<CallArg<&Ref, &Operand>> {
    using
        .iter()
        .map(|arg| match (arg.mode, &arg.value) {
            (_, None) => CallArg::Omitted,
            (ArgMode::Reference, Some(Operand::Ref(r))) => CallArg::Reference(r),
            (ArgMode::Value, Some(op)) => CallArg::Value(op),
            (_, Some(op)) => CallArg::Content(facts::chars(op)),
        })
        .collect()
}

/// INITIALIZE's implied sending item for an elementary receiver of `kind` (Language Reference
/// SC27-8713-03, p. 353), NULL for a pointer.
fn initial_default(kind: Kind) -> Val {
    match kind {
        Kind::Pointer => Val::Address(0),
        Kind::Alnum { .. } | Kind::AlnumEdited { .. } | Kind::National | Kind::Dbcs { .. } => Val::Fig(Figurative::Space),
        _ => Val::Fig(Figurative::Zero),
    }
}

/// What a screen DISPLAY clears before it writes.
pub(crate) fn clearing(screen: &ScreenPhrases) -> rt::crt::Clearing {
    rt::crt::Clearing { screen: screen.blank_screen, line: screen.blank_line, to_line_end: screen.erase_eol, to_screen_end: screen.erase_eos }
}
