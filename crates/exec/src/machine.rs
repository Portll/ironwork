//! The interpreter: one activation of one program, over the run unit's memory in EBCDIC. A
//! reference can reach anywhere in that memory, as a program compiled without SSRANGE can on
//! z/OS, but never outside it.

pub use rt::abend::{Abend, Ending};
use crate::abend::{AbendCode, Signal};
use crate::layout::{Item, Kind, Layout, Resolved};
use rt::storage::{Loc, Val};
pub(crate) use rt::storage::literal_fixed;
use crate::unit::{ADDRESS_BASE, Event, LoadError, RETURN_CODE, RunUnit};
use crate::Compiled;
use numeric::precision::{self, Fixed, Places};
use numeric::{Options, Trunc, float};
use rt::fixed::{align, compare_fixed, places_of, zoned_digits};
use rt::arith;
use rt::display::utf16_text;
use rt::lir::{ByteClass, ConvertTable, Converting, SignTest, StringSource, TrimSide};
use rt::loc;
use rt::store::{self, compare_national};
use rt::text::UnstringField;
use std::cmp::Ordering;
use std::collections::HashMap;
use syntax::Pos;
use syntax::ast::*;
use zarch::decimal::{self, Decimal};
use zarch::ebcdic::{self, CodePage, Collation};
use zarch::hfp::{Hfp, Precision};

mod cics;
mod cics_bms;
mod cics_files;
mod cics_services;
mod declaratives;
mod facts;
mod file_io;
mod intrinsic;
mod json;
mod le_services;
mod oo;
mod perform;
mod report;
mod sort;
mod sql;
mod xml;

type R<T> = Result<T, Abend>;

/// The routines cobolwork reads a CALL of as running an operating-system command, whose arguments
/// the input trace checks.
const OS_COMMAND_ROUTINES: &[&str] = &["SYSTEM", "C$SYSTEM", "CBL_EXEC_RUN_UNIT", "CBL_GC_HOSTED", "BXPSYSTM"];

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
    program: &'p Program,
    layout: &'p Layout,
    options: Options,
    ssrange: bool,
    page: &'static CodePage,
    collating: &'p crate::collating::Sequence,
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
    /// HANDLE CONDITION, IGNORE CONDITION and HANDLE ABEND, which belong to the program level.
    cics_handlers: cics::Handlers,
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
        let (base, fresh) = unit.activate(me, compiled.program.initial);
        let mut m = Self {
            program: &compiled.program,
            layout: &compiled.layout,
            options: compiled.options,
            ssrange: compiled.ssrange,
            page: compiled.options.code_page(),
            collating: &compiled.collating,
            resolved: HashMap::new(),
            me,
            base,
            linkage: vec![None; compiled.layout.linkage_roots.len()],
            local_base: 0,
            main,
            cics_handlers: cics::Handlers::default(),
            report_writer: &compiled.report_writer,
            carriage: &compiled.carriage,
            oo: oo::Frame::default(),
            sort: None,
            segment: 0,
            declaratives: &compiled.declaratives,
            uses: declaratives::State::default(),
            returns: perform::Returns::new(compiled.program.paragraphs.len()),
            xml: xml::Registers::default(),
            unit,
        };
        if compiled.layout.local_size > 0 {
            m.local_base = m.unit.push_temporary(&vec![0; compiled.layout.local_size as usize]);
            m.initialize_values(true)?;
        }
        if fresh {
            m.unit.mem[base..base + compiled.layout.size as usize].fill(0);
            m.initialize_values(false)?;
            m.unit.initialized(me);
        }
        Ok(m)
    }

    /// Applies VALUE clauses: to WORKING-STORAGE and file records, or to LOCAL-STORAGE.
    fn initialize_values(&mut self, local: bool) -> R<()> {
        let base = if local { self.local_base } else { self.base };
        for index in 0..self.layout.items.len() {
            let item = &self.layout.items[index];
            let Some(value) = item.value.clone().filter(|_| item.linkage.is_none() && item.local == local) else { continue };
            let occurrences: u32 = item.dims.iter().map(|&(_, n)| n).product::<u32>().max(1);
            // An alphanumeric VALUE fills a numeric-edited item as alphanumeric data (Language Reference p. 246).
            let kind = match (item.kind, &value) {
                (Kind::NumericEdited { .. }, Literal::Alnum(_) | Literal::Figurative(_) | Literal::All(_)) => Kind::Alnum { justified: false },
                (kind, _) => kind,
            };
            for k in 0..occurrences {
                let offset = base + item.offset as usize + self.occurrence_offset(item, k);
                let loc = Loc { offset, len: item.size as usize, kind, item: index };
                let val = self.literal_value(&value, item.pos)?;
                self.assign(loc, val, None, item.pos)?;
            }
        }
        Ok(())
    }

    fn occurrence_offset(&self, item: &Item, k: u32) -> usize {
        loc::occurrence_offset(&item.dims, k)
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
        match self.statement(s) {
            Err(Abend { code: AbendCode::Signal(Signal::DeclarativeExit), .. }) => Ok(self.declarative_exit()),
            flow => flow,
        }
    }

    fn statement(&mut self, s: &'p Stmt) -> R<Flow> {
        match s {
            Stmt::Move { from, to, pos } => {
                for r in to {
                    let dest = self.locate_receiving(r)?;
                    let (val, src) = self.operand_with_loc(from, *pos)?;
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
            Stmt::Display { items, no_advancing, pos } => self.display(items, *no_advancing, *pos)?,
            Stmt::Open { files, pos } => {
                for (mode, name) in files {
                    self.open_file(*mode, name, *pos)?;
                }
            }
            Stmt::Close { files, pos } => {
                for name in files {
                    self.close_file(name, *pos)?;
                }
            }
            Stmt::Read(r) => return self.read_stmt(r),
            Stmt::Write { record, from, advancing, invalid, end_of_page, pos } => return self.write_stmt(record, from.as_ref(), advancing.as_ref(), invalid, end_of_page, *pos),
            Stmt::Rewrite { record, from, invalid, pos } => return self.rewrite_stmt(record, from.as_ref(), invalid, *pos),
            Stmt::Delete { file, invalid, pos } => return self.delete_stmt(file, invalid, *pos),
            Stmt::Start { file, key, invalid, pos } => return self.start_stmt(file, key.as_ref(), invalid, *pos),
            Stmt::Initialize { targets, pos } => {
                for r in targets {
                    let loc = self.locate(r)?;
                    if loc.item == usize::MAX {
                        self.write(loc, &vec![0; loc.len]);
                    } else {
                        self.initialize(loc.item, loc.offset, *pos)?;
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
                    self.cancel(&name, *pos)?;
                }
            }
            Stmt::Set { set, pos } => self.set(set, *pos)?,
            Stmt::Accept { target, from, pos } => self.accept(target, *from, *pos)?,
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
                return Err(Abend {
                    code: AbendCode::Exec,
                    message: format!("EXEC {kind} {} was reached: ironwork for COBOL checks EXEC statements but does not run them yet", block.command),
                    pos: block.pos,
                    file: None,
                });
            }
            Stmt::Invoke(i) => return self.invoke(i),
            Stmt::JsonGenerate(g) => return self.json_generate(g),
            Stmt::XmlParse(x) => return self.xml_parse(x),
            Stmt::ExitMethod { .. } => return Ok(Flow::End(Ending::Goback)),
            Stmt::SentenceEnd => {}
            Stmt::StopRun { .. } => return Ok(Flow::End(Ending::StopRun)),
            Stmt::Exit { kind: ExitKind::Paragraph, .. } => return Ok(Flow::ExitParagraph),
            Stmt::Exit { kind: ExitKind::Section, .. } => return Ok(Flow::ExitSection),
            Stmt::Exit { kind: ExitKind::Perform, .. } => return Ok(Flow::ExitPerform),
            Stmt::Exit { kind: ExitKind::PerformCycle, .. } => return Ok(Flow::ExitPerformCycle),
            Stmt::Continue | Stmt::Exit { kind: ExitKind::Plain, .. } => {}
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
        self.nest(pos)?;
        let flow = self.repeat_nested(repeat, pos, body);
        self.unit.depth -= 1;
        flow
    }

    fn nest(&mut self, pos: Pos) -> R<()> {
        self.unit.enter(pos)
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
        let var = self.locate(&v.var)?;
        let start = self.expr_value(&v.from, pos)?;
        self.assign(var, start, None, pos)
    }

    fn vary_by(&mut self, v: &Varying, pos: Pos) -> R<()> {
        let var = self.locate(&v.var)?;
        let step = Expr::Bin(Box::new(Expr::Operand(Operand::Ref(v.var.clone()))), BinOp::Add, Box::new(v.by.clone()));
        let dmax = var.kind.digits_scale().map_or(0, |(_, s)| s).max(self.dmax(&step)?);
        let next = self.eval_fixed(&step, dmax, pos)?;
        self.store_fixed(var, &next, false, pos)
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

    /// The receiving item of MOVE, ACCEPT, STRING, UNSTRING, READ and RETURN INTO, and WRITE,
    /// REWRITE and RELEASE FROM: a group holding the object of its own OCCURS DEPENDING ON is its
    /// maximum length (Language Reference SC27-8713-03, pp. 205-206).
    fn locate_receiving(&mut self, r: &Ref) -> R<Loc> {
        self.locate_as(r, true)
    }

    fn locate_as(&mut self, r: &Ref, receiving: bool) -> R<Loc> {
        if let Some(loc) = self.oo_register(r)?.or(self.xml_register(r)?) {
            return Ok(loc);
        }
        if r.name == "RETURN-CODE" && r.qualifiers.is_empty() && !self.layout.items.iter().any(|i| i.name.as_deref() == Some("RETURN-CODE")) {
            return Ok(Loc { offset: RETURN_CODE, len: 2, kind: Kind::Binary { digits: 4, scale: 0, signed: true, native: false }, item: usize::MAX });
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
        for (&(stride, count), sub) in item.dims.iter().zip(&r.subscripts) {
            let s = self.integer(sub, r.pos)?;
            offset += loc::subscript(s, stride, self.ssrange.then_some(count), &r.name, r.pos)?;
        }
        let (mut len, mut kind) = (item.size as i64, item.kind);
        if let Some(t) = item.odo
            && !(receiving && r.refmod.is_none() && self.object_within(t, index)?)
        {
            let table = &layout.items[t];
            let current = self.occurrences(t, r.pos)?;
            len = loc::odo_len(len, table.occurs, current, table.size);
        }
        if let Some(rm) = &r.refmod {
            let start = self.integer(&rm.start, r.pos)?;
            let length = match &rm.length {
                Some(l) => Some(self.integer(l, r.pos)?),
                None => None,
            };
            // A national item's character positions are two bytes, and a part of it is national.
            let unit = if kind == Kind::National { 2 } else { 1 };
            let (from, length) = loc::refmod(len / unit, start, length, self.ssrange, &r.name, r.pos)?;
            offset += from * unit;
            len = length * unit;
            if kind != Kind::National {
                kind = Kind::Alnum { justified: false };
            }
        }
        let (offset, len) = loc::within(offset, len, self.unit.mem.len(), &r.name, r.pos)?;
        Ok(Loc { offset, len, kind, item: index })
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

    fn bytes(&self, loc: Loc) -> &[u8] {
        store::bytes(&self.unit.mem, loc)
    }

    fn write(&mut self, loc: Loc, bytes: &[u8]) {
        store::write(&mut self.unit.mem, loc, bytes);
    }

    fn integer(&mut self, e: &Expr, pos: Pos) -> R<i64> {
        let dmax = self.dmax(e)?;
        let v = self.eval_fixed(e, dmax, pos)?;
        let whole = align(&v, 0, false).and_then(|m| m.to_u128()).and_then(|m| i64::try_from(m).ok());
        let whole = whole.ok_or_else(|| Abend::ironwork("an integer operand beyond 64 bits", pos))?;
        Ok(if v.negative { -whole } else { whole })
    }

    fn literal_value(&self, lit: &Literal, pos: Pos) -> R<Val> {
        Ok(match lit {
            Literal::Alnum(s) => Val::Bytes(self.page.encode(s).map_err(|e| Abend::ironwork(e.to_string(), pos))?),
            Literal::Hex(b) => Val::Bytes(b.clone()),
            Literal::National(s) => Val::National(s.encode_utf16().flat_map(u16::to_be_bytes).collect()),
            Literal::Number(t) => Val::Num(literal_fixed(t).ok_or_else(|| Abend::ironwork(format!("the literal {t} has more than 31 digits"), pos))?),
            Literal::Figurative(f) => Val::Fig(*f),
            Literal::All(inner) => match self.literal_value(inner, pos)? {
                Val::Bytes(b) => Val::All(b),
                Val::Fig(f) => Val::Fig(f),
                _ => return Err(Abend::ironwork("ALL takes an alphanumeric literal", pos)),
            },
        })
    }

    /// What a DECIMAL-POINT IS COMMA program shows for a decimal point.
    fn decimal_point(&self) -> char {
        if self.program.environment.decimal_point_comma { ',' } else { '.' }
    }

    /// The cs of NUMVAL-C and TEST-NUMVAL-C without argument-2 (assumption C102).
    fn default_currency(&self) -> String {
        match self.program.environment.currency.as_slice() {
            [only] => only.value.clone(),
            _ => "$".to_owned(),
        }
    }

    fn read(&self, loc: Loc, pos: Pos) -> R<Val> {
        store::read(&self.facts(), &self.unit.mem, loc, pos)
    }

    fn operand_with_loc(&mut self, op: &Operand, pos: Pos) -> R<(Val, Option<Loc>)> {
        if let Operand::Ref(r) = op {
            let loc = self.locate(r)?;
            return Ok((self.read(loc, r.pos)?, Some(loc)));
        }
        Ok((self.operand(op, pos)?, None))
    }

    fn operand(&mut self, op: &Operand, pos: Pos) -> R<Val> {
        match op {
            Operand::Ref(r) => {
                let loc = self.locate(r)?;
                self.read(loc, r.pos)
            }
            Operand::Literal(lit) => self.literal_value(lit, pos),
            Operand::LengthOf(r) => {
                let loc = self.locate(r)?;
                Ok(Val::Num(Fixed::new(loc.len as i128, Places::new(9, 0))))
            }
            Operand::Function(f) => self.function(f),
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
        let overflow = rt::text::unstring(self, &u.source, u.pointer.as_ref(), &delimiters, &into, u.tallying.as_ref(), u.pos)?;
        self.overflow_branch(overflow, &u.on_overflow, &u.not_on_overflow)
    }

    fn inspect(&mut self, i: &Inspect) -> R<()> {
        let tallying: Vec<_> = i.tallying.iter().map(|p| self.inspect_phrase(p)).collect();
        let replacing: Vec<_> = i.replacing.iter().map(|p| self.inspect_phrase(p)).collect();
        let converting = i.converting.as_ref().map(|(from, to, bounds)| Converting { table: ConvertTable::Operands { from: facts::chars(from), to: facts::chars(to) }, bounds: facts::bounds(bounds) });
        rt::text::inspect(self, &i.target, &tallying, &replacing, converting.as_ref(), i.pos)
    }

    fn search(&mut self, se: &'p Search) -> R<Flow> {
        let pos = se.pos;
        let Resolved::Item(t) = self.resolve(&se.table)? else {
            return Err(Abend::ironwork(format!("SEARCH {}: not a table", se.table.name), pos));
        };
        let layout = self.layout;
        let table = &layout.items[t];
        let count = self.occurrences(t, pos)? as i64;
        let index = match (&se.varying, table.index_names.first()) {
            (_, Some(name)) => Ref { name: name.clone(), qualifiers: Vec::new(), subscripts: Vec::new(), refmod: None, pos },
            (Some(v), None) => v.clone(),
            (None, None) => return Err(Abend::ironwork(format!("SEARCH {}: the table has no INDEXED BY", se.table.name), pos)),
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
        let source = self.unit.programs[self.me].source.clone();
        let program = self.program;
        let file = match (pos.file, &source) {
            (0, Some(path)) => path.to_str().unwrap_or_default(),
            (i, _) => program.sources.get(i as usize).map_or("", String::as_str),
        };
        self.unit.notify(Event::Sink { kind, file, line: pos.line, operand });
    }

    /// The bytes a CALL passes, one argument after another, as the code page reads them.
    fn arguments_text(&mut self, c: &Call) -> R<String> {
        let mut text = String::new();
        for op in c.using.iter().filter_map(|a| a.value.as_ref()) {
            let bytes = match op {
                Operand::Ref(r) => {
                    let loc = self.locate(r)?;
                    self.bytes(loc).to_vec()
                }
                _ => self.content_argument(op, c.pos)?,
            };
            text.push_str(&self.page.decode(&bytes));
        }
        Ok(text)
    }

    fn call(&mut self, c: &'p Call) -> R<Flow> {
        if let Some(flow) = self.call_through_pointer(c)? {
            return Ok(flow);
        }
        let pos = c.pos;
        let name = self.program_name(&c.target, pos)?;
        let variable = !matches!(c.target, Operand::Literal(_));
        if self.unit.observed() {
            if variable {
                self.sink("dynamic-program-load", pos, &name);
            }
            // ironwork runs no operating-system command: the CALL loads a program of that name or fails.
            if OS_COMMAND_ROUTINES.contains(&name.as_str())
                && let Ok(text) = self.arguments_text(c)
            {
                self.sink("os-command", pos, &text);
            }
        }
        let dynamic = self.options.dynam || variable;
        let (index, entry) = match self.unit.load_entry(&name, dynamic) {
            Ok(i) => i,
            Err(LoadError::NotFound) if crate::le::provides(&name) => return self.le_call(c, &name),
            Err(LoadError::NotFound) => {
                return match &c.on_exception {
                    Some(body) => self.run_block(body),
                    None => Err(Abend { code: AbendCode::ModuleNotFound, message: crate::le::missing(&name), pos, file: None }),
                };
            }
            Err(LoadError::Compile(message)) => return Err(Abend::ironwork(format!("CALL {name}: {message}"), pos)),
        };
        let Some(compiled) = self.unit.programs[index].compiled.clone() else {
            return Err(Abend::ironwork(format!("CALL {name}: the first program of the run unit is already active"), pos));
        };
        if self.unit.programs[index].active && !compiled.program.recursive {
            return Err(Abend::ironwork(format!("CALL {name}: the program is already active and is not RECURSIVE"), pos));
        }
        self.nest(pos)?;
        let result = self.call_nested(c, index, entry, compiled);
        self.unit.depth -= 1;
        result
    }

    fn call_nested(&mut self, c: &'p Call, index: usize, entry: Option<usize>, compiled: std::rc::Rc<Compiled>) -> R<Flow> {
        let pos = c.pos;
        let mark = self.unit.mem.len();
        let mut addresses = Vec::new();
        for arg in &c.using {
            let Some(op) = &arg.value else {
                addresses.push(None);
                continue;
            };
            let at = match (arg.mode, op) {
                (ArgMode::Reference, Operand::Ref(r)) => {
                    let loc = self.locate(r)?;
                    loc.offset
                }
                (ArgMode::Value, _) => {
                    let bytes = self.value_argument(op, pos)?;
                    self.unit.push_temporary(&bytes)
                }
                (_, _) => {
                    let bytes = self.content_argument(op, pos)?;
                    self.unit.push_temporary(&bytes)
                }
            };
            addresses.push(Some(at));
        }
        let outcome = {
            let mut callee = Machine::activation(&compiled, index, &mut *self.unit, false)?;
            let entry = entry.and_then(|k| compiled.entries.get(k));
            callee.bind_using(entry.map_or(&compiled.program.using, |e| &e.using), &addresses);
            callee.bind_returning();
            let ending = callee.run_from(entry.map(|e| (e.paragraph, e.statement)));
            let returned = match (&compiled.program.returning, &ending) {
                (Some(item), Ok(_)) => Some(callee.returned(item, pos)?),
                _ => None,
            };
            (ending, returned)
        };
        self.unit.programs[index].active = false;
        if compiled.program.initial {
            self.unit.programs[index].initialized = false;
        }
        self.unit.release_temporaries(mark);
        let (ending, returned) = outcome;
        if ending? == Ending::StopRun {
            return Ok(Flow::End(Ending::StopRun));
        }
        if let (Some(target), Some(val)) = (&c.returning, returned) {
            let dest = self.locate(target)?;
            self.assign(dest, val, None, pos)?;
        }
        match &c.not_on_exception {
            Some(body) => self.run_block(body),
            None => Ok(Flow::Next),
        }
    }

    /// Gives each PROCEDURE DIVISION USING item the address of the argument in its position.
    fn bind(&mut self, addresses: &[Option<usize>]) {
        let program = self.program;
        self.bind_using(&program.using, addresses);
    }

    /// Gives each item of a PROCEDURE DIVISION or ENTRY USING list the address of the argument in
    /// its position.
    fn bind_using(&mut self, using: &[Param], addresses: &[Option<usize>]) {
        for (param, address) in using.iter().zip(addresses) {
            if let Some(ordinal) = self.layout.linkage_roots.iter().position(|&i| self.layout.items[i].name.as_deref() == Some(param.name.as_str())) {
                self.linkage[ordinal] = *address;
            }
        }
    }

    /// The RETURNING item is in the LINKAGE SECTION, but no argument addresses it: the runtime
    /// gives it storage of its own for the call.
    fn bind_returning(&mut self) {
        let Some(name) = &self.program.returning else { return };
        let Some(ordinal) = self.layout.linkage_roots.iter().position(|&i| self.layout.items[i].name.as_deref() == Some(name.as_str())) else { return };
        let size = self.layout.items[self.layout.linkage_roots[ordinal]].size as usize;
        self.linkage[ordinal] = Some(self.unit.push_temporary(&vec![0; size]));
    }

    fn returned(&mut self, name: &str, pos: Pos) -> R<Val> {
        let r = Ref { name: name.to_owned(), qualifiers: Vec::new(), subscripts: Vec::new(), refmod: None, pos };
        let loc = self.locate(&r)?;
        self.read(loc, pos)
    }

    /// A BY CONTENT argument: a copy of the item, or of the literal as its own data item would hold it.
    fn content_argument(&mut self, op: &Operand, pos: Pos) -> R<Vec<u8>> {
        if let Operand::Ref(r) = op {
            let loc = self.locate(r)?;
            return Ok(self.bytes(loc).to_vec());
        }
        Ok(match self.operand(op, pos)? {
            Val::Bytes(b) | Val::All(b) | Val::National(b) => b,
            Val::Fig(f) => vec![self.collating.figurative(f)],
            Val::Address(a) => a.to_be_bytes().to_vec(),
            Val::Num(f) if matches!(op, Operand::LengthOf(_)) => (align(&f, 0, false).and_then(|m| m.to_u128()).unwrap_or(0) as u32).to_be_bytes().to_vec(),
            Val::Num(f) => {
                let digits = f.places.total().max(1);
                let magnitude = align(&f, f.places.dec, false).and_then(|m| m.to_u128()).unwrap_or(0);
                zoned_digits(magnitude, digits as usize, if f.negative { decimal::MINUS } else { decimal::UNSIGNED })
            }
            Val::Float(h) => h.to_bytes(),
        })
    }

    /// A BY VALUE argument: an integer as a binary fullword, an address, or the bytes of a one-character item.
    fn value_argument(&mut self, op: &Operand, pos: Pos) -> R<Vec<u8>> {
        Ok(match self.operand(op, pos)? {
            Val::Num(f) => {
                let whole = align(&f, 0, false).and_then(|m| m.to_u128()).and_then(|m| i32::try_from(m).ok()).ok_or_else(|| Abend::ironwork("a BY VALUE integer beyond a fullword", pos))?;
                (if f.negative { -whole } else { whole }).to_be_bytes().to_vec()
            }
            Val::Address(a) => a.to_be_bytes().to_vec(),
            Val::Fig(Figurative::Null) => vec![0; 4],
            Val::Bytes(b) => b,
            _ => return Err(Abend::ironwork("this BY VALUE argument is not supported", pos)),
        })
    }

    fn cancel(&mut self, name: &str, pos: Pos) -> R<()> {
        let Some(index) = self.unit.find(name) else { return Ok(()) };
        if self.unit.programs[index].active {
            return Err(Abend::ironwork(format!("CANCEL {name}: the program is active"), pos));
        }
        let files: Vec<_> = self.unit.programs[index].files.iter_mut().filter_map(Option::take).collect();
        for f in files {
            f.close().map_err(|e| Abend::ironwork(format!("CANCEL {name}: {e}"), pos))?;
        }
        self.unit.programs[index].initialized = false;
        Ok(())
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
                    let dest = self.locate_item(condition.item, r, false)?;
                    let val = self.literal_value(value, pos)?;
                    self.assign(dest, val, None, pos)?;
                }
            }
            SetStmt::To { targets, value } => {
                for r in targets {
                    let dest = self.locate(r)?;
                    let (val, src) = self.operand_with_loc(value, pos)?;
                    let (val, src) = rt::set::to(dest, val, src, pos)?;
                    self.assign(dest, val, src, pos)?;
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
        }
        Ok(())
    }

    fn accept(&mut self, target: &Ref, from: AcceptFrom, pos: Pos) -> R<()> {
        let dest = self.locate_receiving(target)?;
        rt::accept::accept(&self.facts(), self.unit, dest, from, &target.name, pos)
    }

    fn function(&mut self, f: &FunctionCall) -> R<Val> {
        if let Some(value) = self.storage_function(f)? {
            return self.function_refmod(f, value);
        }
        let args = self.function_arguments(f)?;
        let side = match f.modifier.as_deref() {
            Some("LEADING") => Some(TrimSide::Leading),
            Some("TRAILING") => Some(TrimSide::Trailing),
            _ => None,
        };
        let value = rt::intrinsic::function::evaluate(&mut intrinsic::Call { machine: self, f }, &f.name, side, args, f.pos)?;
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
            _ if self.uses_float(e)? => Ok(Val::Float(self.eval_float(e, self.options.arith.float_intermediate(), pos)?)),
            _ => {
                let dmax = self.dmax(e)?;
                Ok(Val::Num(self.eval_fixed(e, dmax, pos)?))
            }
        }
    }

    fn operand_kind(&mut self, op: &Operand) -> R<Option<Kind>> {
        Ok(match op {
            Operand::Ref(r) => Some(self.locate(r)?.kind),
            _ => None,
        })
    }

    /// Fixed at lowering as `ArithStep.mode` (lower/plans.rs); the walker decides it on each execution.
    fn uses_float(&mut self, e: &Expr) -> R<bool> {
        Ok(match e {
            Expr::Operand(Operand::Function(f)) => self.is_floating_point(f)?,
            Expr::Operand(op) => matches!(self.operand_kind(op)?, Some(Kind::Float(_))),
            Expr::Neg(inner) => self.uses_float(inner)?,
            Expr::Bin(a, _, b) => self.uses_float(a)? || self.uses_float(b)?,
        })
    }

    /// The most decimal places among an expression's operands, divisors and exponents aside. Fixed at
    /// lowering as `ArithPlan.dmax` (lower/plans.rs); the walker works it out on each execution.
    fn dmax(&mut self, e: &Expr) -> R<u32> {
        Ok(match e {
            Expr::Operand(Operand::Literal(Literal::Number(t))) => literal_fixed(t).map_or(0, |f| f.places.dec),
            Expr::Operand(op) => self.operand_kind(op)?.and_then(Kind::digits_scale).map_or(0, |(_, s)| s),
            Expr::Neg(inner) => self.dmax(inner)?,
            Expr::Bin(a, BinOp::Div | BinOp::Pow, _) => self.dmax(a)?,
            Expr::Bin(a, _, b) => self.dmax(a)?.max(self.dmax(b)?),
        })
    }

    fn eval_fixed(&mut self, e: &Expr, dmax: u32, pos: Pos) -> R<Fixed> {
        let arith = self.options.arith;
        match e {
            Expr::Operand(op) => {
                let val = self.operand(op, pos)?;
                arith::fixed_operand(val, dmax, pos)
            }
            Expr::Neg(inner) => Ok(arith::fixed_neg(self.eval_fixed(inner, dmax, pos)?)),
            Expr::Bin(a, op, b) => {
                let x = self.eval_fixed(a, dmax, pos)?;
                if *op == BinOp::Pow {
                    let n = self.integer(b, pos)?;
                    return arith::pow(x, n, dmax, arith, pos);
                }
                let y = self.eval_fixed(b, dmax, pos)?;
                if arith::divides_by_zero(*op, &y) {
                    let binary = self.binary_division(a, b)?;
                    return Err(arith::zero_divide(binary, pos));
                }
                arith::fixed_binop(x, *op, y, dmax, arith, pos)
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
            Expr::Operand(op) => {
                let val = self.operand(op, pos)?;
                arith::float_operand(val, p, pos)
            }
            Expr::Neg(inner) => Ok(arith::float_neg(self.eval_float(inner, p, pos)?)),
            Expr::Bin(a, op, b) => {
                let (x, y) = (self.eval_float(a, p, pos)?, self.eval_float(b, p, pos)?);
                arith::float_binop(x, *op, y, p, pos)
            }
        }
    }

    /// COMPUTE, ADD, SUBTRACT, MULTIPLY, DIVIDE: what the receivers share is computed before any is
    /// stored, then each receiver in turn gets it, or with `per_receiver` combines it with its own
    /// current value (Language Reference SC27-8713-03, p. 298, multiple results). A size error
    /// leaves the target unchanged when the statement handles it.
    fn arithmetic(&mut self, computations: &[(Target, Expr)], remainder: Option<&(Target, Expr, Expr)>, handler: Option<&'p SizeError>, per_receiver: bool, pos: Pos) -> R<Flow> {
        let mut size_error = false;
        let mut dmax = 0;
        for (t, e) in computations {
            let loc = self.locate(&t.r)?;
            dmax = dmax.max(precision::receiver_dec(loc.kind.digits_scale().map_or(0, |(_, s)| s), t.rounded)).max(self.dmax(e)?);
        }
        if let Some((t, dividend, _)) = remainder {
            let loc = self.locate(&t.r)?;
            dmax = dmax.max(loc.kind.digits_scale().map_or(0, |(_, s)| s)).max(self.dmax(dividend)?);
        }
        let mut quotient_target: Option<Loc> = None;
        let mut results = Vec::with_capacity(computations.len());
        for (t, e) in computations {
            let own = |x: &Expr| per_receiver && matches!(x, Expr::Operand(Operand::Ref(r)) if *r == t.r);
            let float = self.uses_float(e)?;
            let (shared, with) = match e {
                Expr::Bin(a, op, b) if *op != BinOp::Pow && own(a) => (b.as_ref(), Some((*op, true))),
                Expr::Bin(a, op, b) if *op != BinOp::Pow && own(b) => (a.as_ref(), Some((*op, false))),
                _ => (e, None),
            };
            let outcome = if float {
                self.eval_float(shared, self.options.arith.float_intermediate(), pos).map(Val::Float)
            } else {
                self.eval_fixed(shared, dmax, pos).map(Val::Num)
            };
            results.push((t, shared, with, outcome));
        }
        let operands = match remainder {
            Some((_, dividend, divisor)) => Some((self.eval_fixed(dividend, dmax, pos)?, self.eval_fixed(divisor, dmax, pos)?)),
            None => None,
        };
        for (t, shared, with, outcome) in results {
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
                    arith::float_binop(x, op, y, p, pos).map(Val::Float)
                }
                (_, outcome) => outcome,
            };
            let Some(value) = arith::size_error(outcome, handler.is_some())? else {
                size_error = true;
                continue;
            };
            size_error |= self.store_value(loc, value, t.rounded, handler.is_some(), pos)?;
        }
        if let (Some((t, _, _)), Some((x, y)), Some(q_loc)) = (remainder, operands, quotient_target)
            && let Some(r) = arith::remainder(x, y, places_of(q_loc.kind).dec, dmax, self.options.arith, pos)?
        {
            let r_loc = self.locate(&t.r)?;
            size_error |= self.store_value(r_loc, Val::Num(r), false, handler.is_some(), pos)?;
        }
        if let Some(h) = handler {
            return self.run_block(if size_error { &h.on } else { &h.not_on });
        }
        Ok(Flow::Next)
    }

    /// Stores an arithmetic result; returns whether it was a size error.
    fn store_value(&mut self, loc: Loc, value: Val, rounded: bool, keep_on_size_error: bool, pos: Pos) -> R<bool> {
        store::store_value(&self.facts(), self.unit, loc, value, rounded, keep_on_size_error, pos)
    }

    fn store_fixed(&mut self, loc: Loc, value: &Fixed, rounded: bool, pos: Pos) -> R<()> {
        store::store_fixed(&self.facts(), self.unit, loc, value, rounded, pos)
    }

    fn store_fixed_checked(&mut self, loc: Loc, value: &Fixed, rounded: bool, keep_on_size_error: bool, pos: Pos) -> R<bool> {
        store::store_fixed_checked(&self.facts(), self.unit, loc, value, rounded, keep_on_size_error, pos)
    }

    /// MOVE, and VALUE at start-up, into one receiving item.
    fn assign(&mut self, dest: Loc, val: Val, src: Option<Loc>, pos: Pos) -> R<()> {
        store::assign(&self.facts(), self.unit, dest, val, src, pos)
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
            Cond::Class(e, class) => self.class(e, *class, pos)?,
            Cond::NameOrRel { subject, op, name } => match self.resolve(name)? {
                Resolved::Condition(_) => self.condition(&Cond::Name(name.clone()), pos)?,
                Resolved::Item(_) => self.condition(&Cond::Rel(subject.clone(), *op, Expr::Operand(Operand::Ref(name.clone()))), pos)?,
            },
            Cond::Name(r) => {
                let Resolved::Condition(index) = self.resolve(r)? else {
                    return Err(Abend::ironwork(format!("{} is a data item, not a condition", r.name), r.pos));
                };
                let condition = &self.layout.conditions[index];
                let loc = self.locate_item(condition.item, r, false)?;
                let subject = (self.read(loc, r.pos)?, Some(loc));
                for (low, high) in &condition.values {
                    let hit = match high {
                        None => self.compare_literal(&subject, low, pos)? == Ordering::Equal,
                        Some(high) => self.compare_literal(&subject, low, pos)? != Ordering::Less && self.compare_literal(&subject, high, pos)? != Ordering::Greater,
                    };
                    if hit {
                        return Ok(true);
                    }
                }
                false
            }
        })
    }

    fn class(&mut self, e: &Expr, class: Class, pos: Pos) -> R<bool> {
        if let (Class::Numeric | Class::Alphabetic, Expr::Operand(Operand::Ref(r))) = (class, e) {
            let loc = self.locate(r)?;
            let test = match (class, loc.kind) {
                (Class::Numeric, Kind::Packed { signed, .. }) => ByteClass::Packed { signed },
                (Class::Numeric, Kind::Zoned { signed, sign: None, .. }) => ByteClass::Zoned { signed },
                (Class::Numeric, _) => ByteClass::Digits,
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
        let (va, la) = self.comparand(a, pos)?;
        let (vb, lb) = self.comparand(b, pos)?;
        if let Some(o) = self.compare_references(a, b, (&va, la), (&vb, lb), pos)? {
            return Ok(o);
        }
        store::compare(&self.facts(), &self.unit.mem, (va, la), (vb, lb), pos)
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

    fn display(&mut self, items: &[Operand], no_advancing: bool, pos: Pos) -> R<()> {
        let mut text = String::new();
        for op in items {
            let shown = match op {
                Operand::Ref(r) => {
                    let loc = self.locate(r)?;
                    rt::display::place(&self.facts(), &self.unit.mem, loc, r.pos)?
                }
                Operand::Literal(Literal::Number(t)) => rt::display::number(t, &self.facts()),
                other => {
                    let val = self.operand(other, pos)?;
                    rt::display::value(&self.facts(), val, pos)?
                }
            };
            text.push_str(&shown);
        }
        if self.unit.observed() {
            self.sink("log", pos, &text);
        }
        rt::display::write(&mut *self.unit.out, &text, no_advancing, pos)
    }

    fn initialize(&mut self, index: usize, offset: usize, pos: Pos) -> R<()> {
        let layout = self.layout;
        let item = &layout.items[index];
        if matches!(item.kind, Kind::Index | Kind::ObjectReference | Kind::ProgramPointer) {
            return Ok(());
        }
        if item.kind != Kind::Group {
            let loc = Loc { offset, len: item.size as usize, kind: item.kind, item: index };
            let val = match item.kind {
                Kind::Pointer => Val::Address(0),
                Kind::Alnum { .. } | Kind::National => Val::Fig(Figurative::Space),
                _ => Val::Fig(Figurative::Zero),
            };
            return self.assign(loc, val, None, pos);
        }
        for &c in &item.children {
            let child = &layout.items[c];
            if child.redefines.is_some() || child.name.is_none() {
                continue;
            }
            for k in 0..child.occurs {
                self.initialize(c, offset + (child.offset - item.offset) as usize + (k * child.size) as usize, pos)?;
            }
        }
        Ok(())
    }
}

fn flatten_and<'c>(cond: &'c Cond, out: &mut Vec<&'c Cond>) {
    match cond {
        Cond::And(a, b) => {
            flatten_and(a, out);
            flatten_and(b, out);
        }
        other => out.push(other),
    }
}

/// In a SEARCH ALL condition, the key item and the value it must equal.
fn key_term<'c>(terms: &[&'c Cond], key: &str) -> Option<(&'c Expr, &'c Expr)> {
    let is_key = |e: &Expr| matches!(e, Expr::Operand(Operand::Ref(r)) if r.name == key);
    terms.iter().find_map(|t| match t {
        Cond::Rel(a, RelOp::Eq, b) if is_key(a) => Some((a, b)),
        Cond::Rel(a, RelOp::Eq, b) if is_key(b) => Some((b, a)),
        _ => None,
    })
}
