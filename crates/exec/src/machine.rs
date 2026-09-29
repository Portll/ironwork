//! The interpreter: one activation of one program, over the run unit's memory in EBCDIC. A
//! reference can reach anywhere in that memory, as a program compiled without SSRANGE can on
//! z/OS, but never outside it.

use crate::layout::{Item, Kind, Layout, Resolved};
use crate::unit::{ADDRESS_BASE, LoadError, RETURN_CODE, RunUnit};
use crate::Compiled;
use numeric::binary::{self, Binary};
use numeric::precision::{ArithError, Fixed, Places};
use numeric::{Numproc, Options, Trunc, float, sign};
use std::cmp::Ordering;
use std::collections::HashMap;
use syntax::Pos;
use syntax::ast::*;
use zarch::check::{ProgramCheck, ProgramMask};
use zarch::decimal::{self, Decimal};
use zarch::ebcdic::{self, CodePage, Collation};
use zarch::hfp::{Hfp, Precision};
use zarch::wide::U256;

mod cics;
mod cics_bms;
mod cics_files;
mod cics_services;
mod file_io;
mod le_services;
mod report;
mod sql;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Abend {
    pub code: String,
    pub message: String,
    pub pos: Pos,
}

impl Abend {
    fn check(c: ProgramCheck, pos: Pos) -> Self {
        Self { code: c.abend(), message: format!("{c:?} exception"), pos }
    }

    fn ironwork(message: impl Into<String>, pos: Pos) -> Self {
        Self { code: "IRONWORK".into(), message: message.into(), pos }
    }
}

type R<T> = Result<T, Abend>;

const DIVIDE_BY_ZERO: &str = "DIVIDE-BY-ZERO";
/// The reader of DISPLAY output went away, as `head` does: not the program's failure.
pub const CLOSED_OUTPUT: &str = "CLOSED-OUTPUT";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ending {
    Goback,
    StopRun,
    EndOfProgram,
}

enum Flow {
    Next,
    End(Ending),
    GoTo(usize),
    ExitParagraph,
    ExitSection,
    ExitPerform,
    ExitPerformCycle,
    NextSentence,
}

#[derive(Clone, Copy, Debug)]
struct Loc {
    offset: usize,
    len: usize,
    kind: Kind,
    item: usize,
}

#[derive(Clone, Debug)]
enum Val {
    Bytes(Vec<u8>),
    National(Vec<u8>),
    Num(Fixed),
    Float(Hfp),
    Fig(Figurative),
    All(Vec<u8>),
    /// A pointer value: [`ADDRESS_BASE`] plus an offset into run-unit memory, or 0 for NULL.
    Address(u32),
}

pub struct Machine<'p, 'u, 'w> {
    program: &'p Program,
    layout: &'p Layout,
    options: Options,
    ssrange: bool,
    page: &'static CodePage,
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
    unit: &'u mut RunUnit<'w>,
}

fn figurative_byte(f: Figurative) -> u8 {
    match f {
        Figurative::Zero => ebcdic::ZERO,
        Figurative::Space => ebcdic::SPACE,
        Figurative::HighValue => ebcdic::HIGH_VALUE,
        Figurative::LowValue => ebcdic::LOW_VALUE,
        Figurative::Quote => ebcdic::QUOTE,
        Figurative::Null => 0,
    }
}

fn figurative_unit(f: Figurative) -> u16 {
    match f {
        Figurative::Zero => 0x0030,
        Figurative::Space => 0x0020,
        Figurative::HighValue => 0xFFFF,
        Figurative::LowValue => 0x0000,
        Figurative::Quote => 0x0022,
        Figurative::Null => 0,
    }
}

fn fixed(negative: bool, magnitude: U256, places: Places) -> Fixed {
    Fixed { negative: negative && !magnitude.is_zero(), magnitude, places }
}

pub(crate) fn literal_fixed(text: &str) -> Option<Fixed> {
    let (negative, body) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let (int, frac) = body.split_once('.').unwrap_or((body, ""));
    let digits = format!("{int}{frac}");
    if digits.is_empty() || digits.len() > 31 {
        return None;
    }
    let magnitude: u128 = digits.parse().ok()?;
    Some(fixed(negative, U256::from_u128(magnitude), Places::new(int.len().max(1) as u32, frac.len() as u32)))
}

fn pow10(n: u32) -> U256 {
    U256::pow10(n)
}

/// The value's magnitude at `scale` decimal places, truncated or rounded half away from zero.
fn align(value: &Fixed, scale: u32, rounded: bool) -> Option<U256> {
    let from = value.places.dec;
    if scale >= from {
        return value.magnitude.checked_mul(pow10(scale - from));
    }
    let (q, r) = value.magnitude.div_rem(pow10(from - scale));
    let half = pow10(from - scale - 1).checked_mul(U256::from_u128(5))?;
    Some(if rounded && r >= half { q + U256::from_u128(1) } else { q })
}

fn compare_fixed(a: &Fixed, b: &Fixed) -> Ordering {
    let dec = a.places.dec.max(b.places.dec);
    let (ma, mb) = (align(a, dec, false).unwrap_or_default(), align(b, dec, false).unwrap_or_default());
    match (a.negative, b.negative) {
        (false, false) => ma.cmp(&mb),
        (true, true) => mb.cmp(&ma),
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
    }
}

fn places_of(kind: Kind) -> Places {
    let (digits, scale) = kind.digits_scale().unwrap_or((0, 0));
    Places::new(digits - scale, scale)
}

fn zoned_digits(magnitude: u128, digits: usize, sign_zone: u8) -> Vec<u8> {
    let mut out = vec![0xF0u8; digits];
    let mut m = magnitude;
    for b in out.iter_mut().rev() {
        *b = 0xF0 | (m % 10) as u8;
        m /= 10;
    }
    if let Some(last) = out.last_mut() {
        *last = (sign_zone << 4) | (*last & 0x0F);
    }
    out
}

impl<'p, 'u, 'w> Machine<'p, 'u, 'w> {
    /// An activation of loaded program `me`. Its storage is initialized on its first activation,
    /// after a CANCEL, and on every activation of an INITIAL program.
    pub fn activation(compiled: &'p Compiled, me: usize, unit: &'u mut RunUnit<'w>, main: bool) -> R<Self> {
        let base = unit.programs[me].base;
        let fresh = !unit.programs[me].initialized || compiled.program.initial;
        unit.programs[me].active = true;
        let mut m = Self {
            program: &compiled.program,
            layout: &compiled.layout,
            options: compiled.options,
            ssrange: compiled.ssrange,
            page: compiled.options.code_page(),
            resolved: HashMap::new(),
            me,
            base,
            linkage: vec![None; compiled.layout.linkage_roots.len()],
            local_base: 0,
            main,
            cics_handlers: cics::Handlers::default(),
            report_writer: &compiled.report_writer,
            unit,
        };
        if compiled.layout.local_size > 0 {
            m.local_base = m.unit.push_temporary(&vec![0; compiled.layout.local_size as usize]);
            m.initialize_values(true)?;
        }
        if fresh {
            m.unit.mem[base..base + compiled.layout.size as usize].fill(0);
            m.initialize_values(false)?;
            m.unit.programs[me].initialized = true;
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
            for k in 0..occurrences {
                let offset = base + item.offset as usize + self.occurrence_offset(item, k);
                let loc = Loc { offset, len: item.size as usize, kind: item.kind, item: index };
                let val = self.literal_value(&value, item.pos)?;
                self.assign(loc, val, None, item.pos)?;
            }
        }
        Ok(())
    }

    fn occurrence_offset(&self, item: &Item, mut k: u32) -> usize {
        let mut offset = 0usize;
        for &(stride, count) in item.dims.iter().rev() {
            offset += (k % count) as usize * stride as usize;
            k /= count;
        }
        offset
    }

    pub fn run_procedure(&mut self) -> R<Ending> {
        let mut start = self.program.report_writer.procedure_start;
        if self.program.paragraphs.len() <= start {
            return Ok(Ending::EndOfProgram);
        }
        let last = self.program.paragraphs.len() - 1;
        loop {
            match self.run_paragraphs(start, last)? {
                Flow::End(e) => return Ok(e),
                Flow::GoTo(t) => start = t,
                _ => return Ok(Ending::EndOfProgram),
            }
        }
    }

    fn run_paragraphs(&mut self, from: usize, to: usize) -> R<Flow> {
        let program = self.program;
        let mut i = from;
        while i <= to {
            match self.run_sentences(&program.paragraphs[i].statements)? {
                Flow::Next | Flow::ExitParagraph => i += 1,
                Flow::ExitSection => i = crate::section_end(program, i) + 1,
                Flow::GoTo(t) if (from..=to).contains(&t) => i = t,
                Flow::ExitPerform | Flow::ExitPerformCycle => i += 1,
                other => return Ok(other),
            }
        }
        Ok(Flow::Next)
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

    fn exec(&mut self, s: &'p Stmt) -> R<Flow> {
        match s {
            Stmt::Move { from, to, pos } => {
                for r in to {
                    let dest = self.locate(r)?;
                    let (val, src) = self.operand_with_loc(from, *pos)?;
                    self.assign(dest, val, src, *pos)?;
                }
            }
            Stmt::Compute { targets, expr, size_error, pos } => {
                let computations: Vec<(Target, Expr)> = targets.iter().map(|t| (t.clone(), expr.clone())).collect();
                return self.arithmetic(&computations, None, size_error.as_ref(), *pos);
            }
            Stmt::Arith(a) => return self.arithmetic(&a.computations, a.remainder.as_ref(), a.size_error.as_ref(), a.pos),
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
                return self.repeat(repeat, *pos, &mut |m: &mut Self| m.run_paragraphs(start, end));
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
            Stmt::Write { record, from, advancing, invalid, pos } => return self.write_stmt(record, from.as_ref(), advancing.as_ref(), invalid, *pos),
            Stmt::Rewrite { record, from, invalid, pos } => return self.rewrite_stmt(record, from.as_ref(), invalid, *pos),
            Stmt::Delete { file, invalid, pos } => return self.delete_stmt(file, invalid, *pos),
            Stmt::Start { file, key, invalid, pos } => return self.start_stmt(file, key.as_ref(), invalid, *pos),
            Stmt::Initialize { targets, pos } => {
                for r in targets {
                    let loc = self.locate(r)?;
                    if loc.item == usize::MAX {
                        self.write(loc, &[0, 0]);
                    } else {
                        self.initialize(loc.item, loc.offset, *pos)?;
                    }
                }
            }
            Stmt::GoTo { target, pos } => return Ok(Flow::GoTo(self.procedure(target, *pos)?.0)),
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
                    code: "EXEC".into(),
                    message: format!("EXEC {kind} {} was reached: ironwork for COBOL checks EXEC statements but does not run them yet", block.command),
                    pos: block.pos,
                });
            }
            Stmt::SentenceEnd => {}
            Stmt::StopRun { .. } => return Ok(Flow::End(Ending::StopRun)),
            Stmt::Exit(ExitKind::Paragraph) => return Ok(Flow::ExitParagraph),
            Stmt::Exit(ExitKind::Section) => return Ok(Flow::ExitSection),
            Stmt::Exit(ExitKind::Perform) => return Ok(Flow::ExitPerform),
            Stmt::Exit(ExitKind::PerformCycle) => return Ok(Flow::ExitPerformCycle),
            Stmt::Continue | Stmt::Exit(ExitKind::Plain) => {}
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
        if self.unit.depth >= crate::unit::MAX_DEPTH {
            return Err(Abend::ironwork(format!("PERFORM and CALL nest deeper than {}", crate::unit::MAX_DEPTH), pos));
        }
        self.unit.depth += 1;
        Ok(())
    }

    fn repeat_nested(&mut self, repeat: &'p Loop, pos: Pos, body: &mut dyn FnMut(&mut Self) -> R<Flow>) -> R<Flow> {
        enum Step {
            Again,
            Leave,
            Out(Flow),
        }
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
            Loop::Varying { varying, test_after } => {
                let var = self.locate(&varying.var)?;
                let start = self.expr_value(&varying.from, pos)?;
                self.assign(var, start, None, pos)?;
                loop {
                    if !test_after && self.condition(&varying.until, pos)? {
                        break;
                    }
                    match run(self)? {
                        Step::Again => {}
                        Step::Leave => break,
                        Step::Out(f) => return Ok(f),
                    }
                    if *test_after && self.condition(&varying.until, pos)? {
                        break;
                    }
                    let var = self.locate(&varying.var)?;
                    let step = Expr::Bin(Box::new(Expr::Operand(Operand::Ref(varying.var.clone()))), BinOp::Add, Box::new(varying.by.clone()));
                    let dmax = var.kind.digits_scale().map_or(0, |(_, s)| s).max(self.dmax(&step)?);
                    let next = self.eval_fixed(&step, dmax, pos)?;
                    self.store_fixed(var, &next, false, pos)?;
                }
            }
        }
        Ok(Flow::Next)
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
        if r.name == "RETURN-CODE" && r.qualifiers.is_empty() && !self.layout.items.iter().any(|i| i.name.as_deref() == Some("RETURN-CODE")) {
            return Ok(Loc { offset: RETURN_CODE, len: 2, kind: Kind::Binary { digits: 4, scale: 0, signed: true, native: false }, item: usize::MAX });
        }
        let Resolved::Item(index) = self.resolve(r)? else {
            return Err(Abend::ironwork(format!("{} is a condition-name, not a data item", r.name), r.pos));
        };
        let layout = self.layout;
        let item = &layout.items[index];
        if r.subscripts.len() != item.dims.len() {
            return Err(Abend::ironwork(format!("{} takes {} subscripts, not {}", r.name, item.dims.len(), r.subscripts.len()), r.pos));
        }
        let base = match item.linkage {
            Some(l) => self.linkage[l as usize].ok_or_else(|| Abend {
                code: "S0C4".into(),
                message: format!("{} is a LINKAGE item with no address: no argument was passed for it, and no SET ADDRESS OF gave it one", r.name),
                pos: r.pos,
            })?,
            None if item.local => self.local_base,
            None => self.base,
        };
        let mut offset = (base + item.offset as usize) as i64;
        for (&(stride, count), sub) in item.dims.iter().zip(&r.subscripts) {
            let s = self.integer(sub, r.pos)?;
            if self.ssrange && (s < 1 || s > count as i64) {
                return Err(Abend::ironwork(format!("subscript {s} of {} is out of range 1 to {count} (SSRANGE)", r.name), r.pos));
            }
            offset += (s - 1) * stride as i64;
        }
        let (mut len, mut kind) = (item.size as i64, item.kind);
        if let Some(t) = item.odo {
            let table = &layout.items[t];
            let current = self.occurrences(t, r.pos)?;
            len -= (table.occurs - current) as i64 * table.size as i64;
        }
        if let Some(rm) = &r.refmod {
            let start = self.integer(&rm.start, r.pos)?;
            let length = match &rm.length {
                Some(l) => self.integer(l, r.pos)?,
                None => len - start + 1,
            };
            if self.ssrange && (start < 1 || length < 1 || start + length - 1 > len) {
                return Err(Abend::ironwork(format!("reference modification ({start}:{length}) of {} is out of range (SSRANGE)", r.name), r.pos));
            }
            offset += start - 1;
            len = length;
            kind = Kind::Alnum { justified: false };
        }
        if offset < 0 || len < 0 || offset + len > self.unit.mem.len() as i64 {
            return Err(Abend::ironwork(format!("{} reaches outside the run unit's storage", r.name), r.pos));
        }
        Ok(Loc { offset: offset as usize, len: len as usize, kind, item: index })
    }

    /// The current count of an OCCURS DEPENDING ON table, kept within its declared maximum so that
    /// a bad count never reaches past the table's storage.
    fn occurrences(&mut self, table: usize, pos: Pos) -> R<u32> {
        let layout = self.layout;
        let item = &layout.items[table];
        let Some(object) = &item.depending_on else { return Ok(item.occurs) };
        let count = self.integer(&Expr::Operand(Operand::Ref(object.clone())), pos)?;
        if self.ssrange && !(0..=item.occurs as i64).contains(&count) {
            return Err(Abend::ironwork(format!("{} = {count} is outside the OCCURS DEPENDING ON range 0 to {} (SSRANGE)", object.name, item.occurs), pos));
        }
        Ok(count.clamp(0, item.occurs as i64) as u32)
    }

    fn bytes(&self, loc: Loc) -> &[u8] {
        &self.unit.mem[loc.offset..loc.offset + loc.len]
    }

    fn write(&mut self, loc: Loc, bytes: &[u8]) {
        self.unit.mem[loc.offset..loc.offset + loc.len].copy_from_slice(bytes);
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

    fn read(&self, loc: Loc, pos: Pos) -> R<Val> {
        let bytes = self.bytes(loc);
        let places = places_of(loc.kind);
        Ok(match loc.kind {
            Kind::Group | Kind::Alnum { .. } | Kind::NumericEdited { .. } | Kind::AlnumEdited { .. } => Val::Bytes(bytes.to_vec()),
            Kind::National => Val::National(bytes.to_vec()),
            Kind::Pointer => Val::Address(u32::from_be_bytes(bytes.try_into().unwrap())),
            Kind::Index => Val::Num(Fixed::new(i32::from_be_bytes(bytes.try_into().unwrap()) as i128, Places::new(9, 0))),
            Kind::Float(p) => Val::Float(Hfp::from_bytes(p, bytes)),
            Kind::Binary { digits, signed, native, .. } => Val::Num(Fixed::new(Binary { digits: digits as u8, signed, native }.load(bytes), places)),
            Kind::Packed { signed, .. } => {
                let d = crate::codec::packed(bytes, signed, self.options.numproc).map_err(|c| Abend::check(c, pos))?;
                Val::Num(fixed(d.negative, U256::from_u128(d.magnitude), places))
            }
            Kind::Zoned { digits, signed, sign, .. } => Val::Num(self.zoned_value(bytes, digits, signed, sign, places, pos)?),
        })
    }

    /// A zoned operand enters arithmetic through PACK, which keeps only the sign's zone.
    fn zoned_value(&self, bytes: &[u8], digits: u32, signed: bool, sign: Option<SignClause>, places: Places, pos: Pos) -> R<Fixed> {
        let d = crate::codec::zoned(bytes, digits, signed, sign, self.options.numproc).map_err(|c| Abend::check(c, pos))?;
        Ok(fixed(d.negative, U256::from_u128(d.magnitude), places))
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

    /// An operand as STRING, UNSTRING and INSPECT see it: its bytes, a figurative constant as one
    /// character, a numeric literal as its digits.
    fn natural_bytes(&mut self, op: &Operand, pos: Pos) -> R<Vec<u8>> {
        if let Operand::Ref(r) = op {
            let loc = self.locate(r)?;
            return Ok(self.bytes(loc).to_vec());
        }
        Ok(match self.operand(op, pos)? {
            Val::Bytes(b) | Val::All(b) | Val::National(b) => b,
            Val::Fig(f) => vec![figurative_byte(f)],
            Val::Num(f) => zoned_digits(f.magnitude.to_u128().unwrap_or(0), f.places.total() as usize, decimal::UNSIGNED),
            _ => return Err(Abend::ironwork("this operand has no characters to work on", pos)),
        })
    }

    fn set_integer(&mut self, r: &Ref, value: i64, pos: Pos) -> R<()> {
        let dest = self.locate(r)?;
        self.store_fixed(dest, &Fixed::new(value as i128, Places::new(19, 0)), false, pos)
    }

    fn overflow_branch(&mut self, overflow: bool, on: &'p Option<Vec<Stmt>>, not_on: &'p Option<Vec<Stmt>>) -> R<Flow> {
        match (overflow, on, not_on) {
            (true, Some(body), _) | (false, _, Some(body)) => self.run_block(body),
            _ => Ok(Flow::Next),
        }
    }

    fn string_stmt(&mut self, st: &'p StringStmt) -> R<Flow> {
        let pos = st.pos;
        let dest = self.locate(&st.into)?;
        let mut pointer = match &st.pointer {
            Some(r) => self.integer(&Expr::Operand(Operand::Ref(r.clone())), pos)?,
            None => 1,
        };
        let len = dest.len as i64;
        let mut overflow = pointer < 1 || pointer > len;
        if !overflow {
            'sources: for (op, delimiter) in &st.sources {
                let bytes = self.natural_bytes(op, pos)?;
                let delimiter = match delimiter {
                    Delimiter::Size => None,
                    Delimiter::By(d) => Some(self.natural_bytes(d, pos)?),
                };
                for b in crate::strings::delimited(&bytes, delimiter.as_deref()) {
                    if pointer > len {
                        overflow = true;
                        break 'sources;
                    }
                    self.unit.mem[dest.offset + pointer as usize - 1] = b;
                    pointer += 1;
                }
            }
        }
        if let Some(r) = &st.pointer {
            self.set_integer(r, pointer, pos)?;
        }
        self.overflow_branch(overflow, &st.on_overflow, &st.not_on_overflow)
    }

    fn unstring(&mut self, u: &'p Unstring) -> R<Flow> {
        let pos = u.pos;
        let source_loc = self.locate(&u.source)?;
        let source = self.bytes(source_loc).to_vec();
        let len = source.len() as i64;
        let mut pointer = match &u.pointer {
            Some(r) => self.integer(&Expr::Operand(Operand::Ref(r.clone())), pos)?,
            None => 1,
        };
        let mut delimiters = Vec::new();
        for (all, d) in &u.delimiters {
            delimiters.push((*all, self.natural_bytes(d, pos)?));
        }
        let mut overflow = pointer < 1 || pointer > len;
        let mut fields = 0i64;
        if !overflow {
            for into in &u.into {
                if pointer > len {
                    break;
                }
                let start = pointer as usize - 1;
                let dest = self.locate(&into.target)?;
                let (end, matched) = if delimiters.is_empty() {
                    ((start + dest.len).min(source.len()), None)
                } else {
                    match crate::strings::next_delimiter(&source, start, &delimiters) {
                        Some((at, k)) => (at, Some(k)),
                        None => (source.len(), None),
                    }
                };
                self.assign(dest, Val::Bytes(source[start..end].to_vec()), None, pos)?;
                let delimiter = matched.map(|k| delimiters[k].1.clone());
                if let Some(r) = &into.delimiter_in {
                    let d = self.locate(r)?;
                    self.assign(d, delimiter.clone().map_or(Val::Fig(Figurative::Space), Val::Bytes), None, pos)?;
                }
                if let Some(r) = &into.count_in {
                    self.set_integer(r, (end - start) as i64, pos)?;
                }
                let next = match matched {
                    Some(k) => crate::strings::past_delimiter(&source, end, &delimiters[k].1, delimiters[k].0),
                    None => end,
                };
                pointer = next as i64 + 1;
                fields += 1;
            }
            overflow = pointer <= len;
        }
        if let Some(r) = &u.pointer {
            self.set_integer(r, pointer, pos)?;
        }
        if let Some(r) = &u.tallying {
            let dest = self.locate(r)?;
            let Val::Num(current) = self.read(dest, pos)? else {
                return Err(Abend::ironwork("TALLYING IN needs a numeric item", pos));
            };
            let total = current.add(Fixed::new(fields as i128, Places::new(19, 0)), 0, self.options.arith).map_err(|_| Abend::ironwork("TALLYING", pos))?;
            self.store_fixed(dest, &total, false, pos)?;
        }
        self.overflow_branch(overflow, &u.on_overflow, &u.not_on_overflow)
    }

    fn inspect(&mut self, i: &Inspect) -> R<()> {
        let pos = i.pos;
        let loc = self.locate(&i.target)?;
        let mut data = self.bytes(loc).to_vec();
        let tallied = self.phrases(&data, &i.tallying, pos)?;
        let counts = crate::strings::inspect(&mut data, &tallied);
        for (phrase, count) in i.tallying.iter().zip(counts) {
            let Some(counter) = &phrase.counter else { continue };
            let dest = self.locate(counter)?;
            let Val::Num(current) = self.read(dest, pos)? else {
                return Err(Abend::ironwork("a TALLYING counter must be numeric", pos));
            };
            let total = current.add(Fixed::new(count as i128, Places::new(19, 0)), 0, self.options.arith).map_err(|_| Abend::ironwork("TALLYING", pos))?;
            self.store_fixed(dest, &total, false, pos)?;
        }
        let mut replacing = self.phrases(&data, &i.replacing, pos)?;
        if let Some((from, to, bounds)) = &i.converting {
            let (from, to) = (self.natural_bytes(from, pos)?, self.natural_bytes(to, pos)?);
            if from.len() != to.len() {
                return Err(Abend::ironwork("CONVERTING needs operands of the same length", pos));
            }
            let (start, end) = self.region(&data, bounds, pos)?;
            let mut seen = Vec::new();
            for (f, t) in from.into_iter().zip(to) {
                if !seen.contains(&f) {
                    seen.push(f);
                    replacing.push(crate::strings::Phrase { mode: InspectMode::All, pattern: vec![f], by: Some(vec![t]), start, end });
                }
            }
        }
        crate::strings::inspect(&mut data, &replacing);
        self.write(loc, &data);
        Ok(())
    }

    fn region(&mut self, data: &[u8], bounds: &[Bound], pos: Pos) -> R<(usize, usize)> {
        let (mut before, mut after) = (None, None);
        for b in bounds {
            let v = self.natural_bytes(&b.value, pos)?;
            if b.after { after = Some(v) } else { before = Some(v) }
        }
        Ok(crate::strings::region(data, before.as_deref(), after.as_deref()))
    }

    fn phrases(&mut self, data: &[u8], phrases: &[InspectPhrase], pos: Pos) -> R<Vec<crate::strings::Phrase>> {
        let mut out = Vec::new();
        for p in phrases {
            let pattern = match &p.pattern {
                Some(op) => self.natural_bytes(op, pos)?,
                None => Vec::new(),
            };
            let len = pattern.len().max(1);
            let by = match &p.by {
                Some(Operand::Literal(Literal::Figurative(f))) => Some(vec![figurative_byte(*f); len]),
                Some(op) => Some(self.natural_bytes(op, pos)?),
                None => None,
            };
            if by.as_ref().is_some_and(|b| b.len() != len) {
                return Err(Abend::ironwork("a REPLACING value must be as long as what it replaces", pos));
            }
            let (start, end) = self.region(data, &p.bounds, pos)?;
            out.push(crate::strings::Phrase { mode: p.mode, pattern, by, start, end });
        }
        Ok(out)
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

    /// An address back to an offset in run-unit memory, refusing one that is NULL or outside it.
    fn offset_of(&self, address: u32, pos: Pos) -> R<Option<usize>> {
        if address == 0 {
            return Ok(None);
        }
        let offset = address.checked_sub(ADDRESS_BASE).map(|o| o as usize).filter(|&o| o < self.unit.mem.len());
        offset.map(Some).ok_or_else(|| Abend { code: "S0C4".into(), message: format!("address {address:08X} is outside the run unit's storage"), pos })
    }

    fn program_name(&mut self, op: &Operand, pos: Pos) -> R<String> {
        Ok(match self.operand(op, pos)? {
            Val::Bytes(b) => self.page.decode(&b).trim().to_ascii_uppercase(),
            _ => return Err(Abend::ironwork("a program name must be alphanumeric", pos)),
        })
    }

    fn call(&mut self, c: &'p Call) -> R<Flow> {
        let pos = c.pos;
        let name = self.program_name(&c.target, pos)?;
        let index = match self.unit.load(&name) {
            Ok(i) => i,
            Err(LoadError::NotFound) if crate::le::provides(&name) => return self.le_call(c, &name),
            Err(LoadError::NotFound) => {
                return match &c.on_exception {
                    Some(body) => self.run_block(body),
                    None => Err(Abend { code: "S806".into(), message: crate::le::missing(&name), pos }),
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
        let result = self.call_nested(c, index, compiled);
        self.unit.depth -= 1;
        result
    }

    fn call_nested(&mut self, c: &'p Call, index: usize, compiled: std::rc::Rc<Compiled>) -> R<Flow> {
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
            callee.bind(&addresses);
            callee.bind_returning();
            let ending = callee.run_procedure();
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
        for (param, address) in self.program.using.iter().zip(addresses) {
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
            Val::Fig(f) => vec![figurative_byte(f)],
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
            SetStmt::ConditionTrue(targets) => {
                for r in targets {
                    let Resolved::Condition(index) = self.resolve(r)? else {
                        return Err(Abend::ironwork(format!("SET {} TO TRUE: not a condition-name", r.name), pos));
                    };
                    let condition = &self.layout.conditions[index];
                    let Some((value, _)) = condition.values.first().cloned() else { continue };
                    let item = &self.layout.items[condition.item];
                    let subject = Ref { name: item.name.clone().unwrap_or_default(), qualifiers: Vec::new(), subscripts: r.subscripts.clone(), refmod: None, pos };
                    let dest = self.locate(&subject)?;
                    let val = self.literal_value(&value, pos)?;
                    self.assign(dest, val, None, pos)?;
                }
            }
            SetStmt::To { targets, value } => {
                for r in targets {
                    let dest = self.locate(r)?;
                    let (val, src) = self.operand_with_loc(value, pos)?;
                    match (dest.kind, val) {
                        (Kind::Pointer, v @ (Val::Address(_) | Val::Fig(Figurative::Null))) => self.assign(dest, v, None, pos)?,
                        (Kind::Pointer, _) => return Err(Abend::ironwork("SET a pointer TO ADDRESS OF, NULL or another pointer", pos)),
                        (_, v) => self.assign(dest, v, src, pos)?,
                    }
                }
            }
            SetStmt::AddressOf { targets, value } => {
                let address = match self.operand(value, pos)? {
                    Val::Address(a) => a,
                    Val::Fig(Figurative::Null) => 0,
                    _ => return Err(Abend::ironwork("SET ADDRESS OF takes a pointer, ADDRESS OF or NULL", pos)),
                };
                let offset = self.offset_of(address, pos)?;
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
                let step = self.integer(by, pos)?;
                let step = if *down { -step } else { step };
                for r in targets {
                    let dest = self.locate(r)?;
                    match self.read(dest, pos)? {
                        Val::Address(a) => {
                            let moved = u32::try_from(a as i64 + step).map_err(|_| Abend::ironwork("a pointer moved below zero", pos))?;
                            self.write(dest, &moved.to_be_bytes());
                        }
                        Val::Num(f) => {
                            let next = f.add(Fixed::new(step as i128, Places::new(19, 0)), 0, self.options.arith).map_err(|_| Abend::ironwork("SET UP/DOWN", pos))?;
                            self.store_fixed(dest, &next, false, pos)?;
                        }
                        _ => return Err(Abend::ironwork("SET UP BY and DOWN BY take an index, integer or pointer", pos)),
                    }
                }
            }
        }
        Ok(())
    }

    fn accept(&mut self, target: &Ref, from: AcceptFrom, pos: Pos) -> R<()> {
        let dest = self.locate(target)?;
        let (seconds, hundredths) = self.unit.now();
        let (year, month, day, hour, minute, second, yday, wday) = crate::unit::civil(seconds);
        let digits = |text: String| Val::Num(literal_fixed(&text).expect("digits"));
        let val = match from {
            AcceptFrom::Date { four_digit_year: true } => digits(format!("{year:04}{month:02}{day:02}")),
            AcceptFrom::Date { four_digit_year: false } => digits(format!("{:02}{month:02}{day:02}", year % 100)),
            AcceptFrom::Day { four_digit_year: true } => digits(format!("{year:04}{yday:03}")),
            AcceptFrom::Day { four_digit_year: false } => digits(format!("{:02}{yday:03}", year % 100)),
            AcceptFrom::DayOfWeek => digits(format!("{wday}")),
            AcceptFrom::Time => digits(format!("{hour:02}{minute:02}{second:02}{hundredths:02}")),
            AcceptFrom::Sysin => {
                let mut line = String::new();
                let read = match self.unit.sysin.as_mut() {
                    Some(r) => r.read_line(&mut line).map_err(|e| Abend::ironwork(format!("ACCEPT: {e}"), pos))?,
                    None => 0,
                };
                if read == 0 {
                    let _ = writeln!(self.unit.err, "ironwork: {pos}: ACCEPT found SYSIN at its end; {} is unchanged", target.name);
                    return Ok(());
                }
                let unknown = self.page.encode_char('?').unwrap_or(0x6F);
                Val::Bytes(line.trim_end_matches(['\n', '\r']).chars().map(|c| self.page.encode_char(c).unwrap_or(unknown)).collect())
            }
        };
        self.assign(dest, val, None, pos)
    }

    fn function(&mut self, f: &FunctionCall) -> R<Val> {
        let pos = f.pos;
        let mut args: Vec<Val> = Vec::new();
        for a in &f.args {
            args.push(self.expr_value(a, pos)?);
        }
        let arity = |n: std::ops::RangeInclusive<usize>| {
            if n.contains(&args.len()) { Ok(()) } else { Err(Abend::ironwork(format!("FUNCTION {} takes {n:?} arguments", f.name), pos)) }
        };
        let bytes_of = |v: &Val| match v {
            Val::Bytes(b) | Val::All(b) => Ok(b.clone()),
            Val::Fig(fig) => Ok(vec![figurative_byte(*fig)]),
            _ => Err(Abend::ironwork(format!("FUNCTION {} needs an alphanumeric argument", f.name), pos)),
        };
        let value = match f.name.as_str() {
            "CHAR" => {
                arity(1..=1)?;
                let n = self.integer(&f.args[0], pos)?;
                if !(1..=256).contains(&n) {
                    return Err(Abend::ironwork(format!("FUNCTION CHAR({n}) is outside 1 to 256"), pos));
                }
                Val::Bytes(vec![(n - 1) as u8])
            }
            "ORD" => {
                arity(1..=1)?;
                let b = bytes_of(&args[0])?;
                let first = *b.first().ok_or_else(|| Abend::ironwork("FUNCTION ORD of an empty argument", pos))?;
                Val::Num(Fixed::new(first as i128 + 1, Places::new(3, 0)))
            }
            "NATIONAL-OF" => {
                arity(1..=2)?;
                let ccsid = if args.len() == 2 { self.integer(&f.args[1], pos)? as u16 } else { self.options.codepage };
                let page = CodePage::by_ccsid(ccsid).ok_or_else(|| Abend::ironwork(format!("CCSID {ccsid} is not a code page ironwork for COBOL carries"), pos))?;
                Val::National(page.to_utf16be(&bytes_of(&args[0])?))
            }
            "LENGTH" => {
                arity(1..=1)?;
                let n = match &args[0] {
                    Val::Bytes(b) | Val::All(b) => b.len(),
                    Val::National(b) => b.len() / 2,
                    Val::Num(v) => v.places.total() as usize,
                    _ => return Err(Abend::ironwork("FUNCTION LENGTH of this argument is not supported yet", pos)),
                };
                Val::Num(Fixed::new(n as i128, Places::new(9, 0)))
            }
            "NUMVAL" | "NUMVAL-C" => {
                arity(1..=2)?;
                let currency = match args.get(1) {
                    Some(v) => self.page.decode(&bytes_of(v)?),
                    None => "$".to_owned(),
                };
                let text = self.page.decode(&bytes_of(&args[0])?);
                Val::Num(numval(&text, (f.name == "NUMVAL-C").then_some(currency.as_str())).unwrap_or_else(|| Fixed::new(0, Places::new(1, 0))))
            }
            "TRIM" => {
                arity(1..=1)?;
                let b = bytes_of(&args[0])?;
                let first = b.iter().position(|&c| c != ebcdic::SPACE);
                let last = b.iter().rposition(|&c| c != ebcdic::SPACE);
                let trimmed = match (first, last, f.modifier.as_deref()) {
                    (None, _, _) | (_, None, _) => Vec::new(),
                    (Some(s), _, Some("LEADING")) => b[s..].to_vec(),
                    (_, Some(e), Some("TRAILING")) => b[..=e].to_vec(),
                    (Some(s), Some(e), _) => b[s..=e].to_vec(),
                };
                Val::Bytes(trimmed)
            }
            "MOD" | "REM" | "INTEGER" | "INTEGER-PART" | "ABS" => {
                arity(if matches!(f.name.as_str(), "MOD" | "REM") { 2..=2 } else { 1..=1 })?;
                let number = |v: &Val| match v {
                    Val::Num(x) => Ok(*x),
                    _ => Err(Abend::ironwork(format!("FUNCTION {} needs numeric arguments", f.name), pos)),
                };
                let x = number(&args[0])?;
                let dec = if args.len() == 2 { x.places.dec.max(number(&args[1])?.places.dec) } else { x.places.dec };
                let scaled = |v: &Fixed| -> R<i128> {
                    let m = align(v, dec, false).and_then(|m| m.to_u128()).and_then(|m| i128::try_from(m).ok()).ok_or_else(|| Abend::ironwork("an argument beyond 38 digits", pos))?;
                    Ok(if v.negative { -m } else { m })
                };
                let unit = 10i128.pow(dec);
                let a = scaled(&x)?;
                let result = match f.name.as_str() {
                    "ABS" => a.abs(),
                    "INTEGER" => a.div_euclid(unit) * unit,
                    "INTEGER-PART" => a / unit * unit,
                    other => {
                        let b = scaled(&number(&args[1])?)?;
                        if b == 0 {
                            return Err(Abend::ironwork(format!("FUNCTION {other} by zero"), pos));
                        }
                        if other == "MOD" { a - b * a.div_euclid(b) } else { a - b * (a / b) }
                    }
                };
                Val::Num(Fixed::new(result, Places::new(31 - dec.min(31), dec)))
            }
            "MIN" | "MAX" => {
                if args.is_empty() {
                    return Err(Abend::ironwork(format!("FUNCTION {} needs arguments", f.name), pos));
                }
                let want = if f.name == "MIN" { Ordering::Less } else { Ordering::Greater };
                let mut best = 0;
                for i in 1..args.len() {
                    let o = match (&args[i], &args[best]) {
                        (Val::Num(x), Val::Num(y)) => compare_fixed(x, y),
                        (x, y) => bytes_of(x)?.cmp(&bytes_of(y)?),
                    };
                    if o == want {
                        best = i;
                    }
                }
                args.swap_remove(best)
            }
            "INTEGER-OF-DATE" => {
                arity(1..=1)?;
                let n = self.integer(&f.args[0], pos)?;
                let (y, m, d) = (n / 10000, n / 100 % 100, n % 100);
                if !(1601..=9999).contains(&y) || !(1..=12).contains(&m) || !(1..=days_in_month(y, m)).contains(&d) {
                    return Err(Abend::ironwork(format!("FUNCTION INTEGER-OF-DATE({n}): not a date from 1601 to 9999"), pos));
                }
                Val::Num(Fixed::new((days_from_civil(y, m, d) - days_from_civil(1600, 12, 31)) as i128, Places::new(7, 0)))
            }
            "DATE-OF-INTEGER" => {
                arity(1..=1)?;
                let n = self.integer(&f.args[0], pos)?;
                if !(1..=3_067_671).contains(&n) {
                    return Err(Abend::ironwork(format!("FUNCTION DATE-OF-INTEGER({n}): outside 1 to 3067671"), pos));
                }
                let (y, m, d, ..) = crate::unit::civil((days_from_civil(1600, 12, 31) + n) * 86_400);
                Val::Num(Fixed::new((y * 10000 + m as i64 * 100 + d as i64) as i128, Places::new(8, 0)))
            }
            "CURRENT-DATE" => {
                arity(0..=0)?;
                let (seconds, hundredths) = self.unit.now();
                let (y, mo, d, h, mi, s, _, _) = crate::unit::civil(seconds);
                let text = format!("{y:04}{mo:02}{d:02}{h:02}{mi:02}{s:02}{hundredths:02}+0000");
                Val::Bytes(self.page.encode(&text).map_err(|e| Abend::ironwork(e.to_string(), pos))?)
            }
            "UPPER-CASE" | "LOWER-CASE" | "REVERSE" => {
                arity(1..=1)?;
                let text = self.page.decode(&bytes_of(&args[0])?);
                let changed: String = match f.name.as_str() {
                    "UPPER-CASE" => text.to_uppercase(),
                    "LOWER-CASE" => text.to_lowercase(),
                    _ => text.chars().rev().collect(),
                };
                Val::Bytes(self.page.encode(&changed).map_err(|e| Abend::ironwork(e.to_string(), pos))?)
            }
            other => return Err(Abend::ironwork(format!("FUNCTION {other} is not supported yet"), pos)),
        };
        match (&f.refmod, value) {
            (None, v) => Ok(v),
            (Some(rm), Val::Bytes(b)) => {
                let start = self.integer(&rm.start, pos)? as usize;
                let len = match &rm.length {
                    Some(l) => self.integer(l, pos)? as usize,
                    None => b.len() + 1 - start,
                };
                b.get(start - 1..start - 1 + len).map(|s| Val::Bytes(s.to_vec())).ok_or_else(|| Abend::ironwork("reference modification past the function result", pos))
            }
            (Some(_), _) => Err(Abend::ironwork("reference modification of a non-alphanumeric function result", pos)),
        }
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

    fn uses_float(&mut self, e: &Expr) -> R<bool> {
        Ok(match e {
            Expr::Operand(op) => matches!(self.operand_kind(op)?, Some(Kind::Float(_))),
            Expr::Neg(inner) => self.uses_float(inner)?,
            Expr::Bin(a, _, b) => self.uses_float(a)? || self.uses_float(b)?,
        })
    }

    /// The most decimal places among an expression's operands, divisors and exponents aside.
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
        let wrap = |r: Result<Fixed, ArithError>| {
            r.map_err(|e| match e {
                ArithError::DivideByZero => Abend { code: DIVIDE_BY_ZERO.into(), message: "division by zero".into(), pos },
                ArithError::BeyondModel => Abend::ironwork("an intermediate result wider than 256 bits", pos),
            })
        };
        match e {
            Expr::Operand(op) => match self.operand(op, pos)? {
                Val::Num(f) => Ok(f),
                Val::Fig(Figurative::Zero) => Ok(Fixed::new(0, Places::new(1, 0))),
                _ => Err(Abend::ironwork("a non-numeric operand in arithmetic", pos)),
            },
            Expr::Neg(inner) => {
                let v = self.eval_fixed(inner, dmax, pos)?;
                Ok(fixed(!v.negative, v.magnitude, v.places))
            }
            Expr::Bin(a, op, b) => {
                let x = self.eval_fixed(a, dmax, pos)?;
                if *op == BinOp::Pow {
                    let n = self.integer(b, pos)?;
                    if !(0..=31).contains(&n) {
                        return Err(Abend::ironwork("exponentiation other than by an integer from 0 to 31 is not supported yet", pos));
                    }
                    let mut acc = Fixed::new(1, Places::new(1, 0));
                    for _ in 0..n {
                        acc = wrap(acc.mul(x, dmax, arith))?;
                    }
                    return Ok(acc);
                }
                let y = self.eval_fixed(b, dmax, pos)?;
                wrap(match op {
                    BinOp::Add => x.add(y, dmax, arith),
                    BinOp::Sub => x.sub(y, dmax, arith),
                    BinOp::Mul => x.mul(y, dmax, arith),
                    _ => x.div(y, dmax, arith),
                })
            }
        }
    }

    fn eval_float(&mut self, e: &Expr, p: Precision, pos: Pos) -> R<Hfp> {
        let mask = ProgramMask::default();
        let check = |r: Result<Hfp, ProgramCheck>| r.map_err(|c| Abend::check(c, pos));
        match e {
            Expr::Operand(op) => match self.operand(op, pos)? {
                Val::Float(h) if h.precision.digits() <= p.digits() => Ok(h.lengthen(p)),
                Val::Float(h) => Ok(float::narrow(h, p)),
                Val::Num(f) => check(float::from_fixed(f, p, mask)),
                Val::Fig(Figurative::Zero) => Ok(Hfp::zero(p)),
                _ => Err(Abend::ironwork("a non-numeric operand in arithmetic", pos)),
            },
            Expr::Neg(inner) => {
                let v = self.eval_float(inner, p, pos)?;
                Ok(if v.fraction == 0 { v } else { Hfp { negative: !v.negative, ..v } })
            }
            Expr::Bin(a, op, b) => {
                let (x, y) = (self.eval_float(a, p, pos)?, self.eval_float(b, p, pos)?);
                check(match op {
                    BinOp::Add => x.add(y, mask),
                    BinOp::Sub => x.sub(y, mask),
                    BinOp::Mul => x.mul(y, p, mask),
                    BinOp::Div => x.div(y, mask),
                    BinOp::Pow => return Err(Abend::ironwork("floating-point exponentiation is not supported yet", pos)),
                })
            }
        }
    }

    /// COMPUTE, ADD, SUBTRACT, MULTIPLY, DIVIDE: each target receives its expression. A size error
    /// leaves the target unchanged when the statement handles it.
    fn arithmetic(&mut self, computations: &[(Target, Expr)], remainder: Option<&(Target, Expr, Expr)>, handler: Option<&'p SizeError>, pos: Pos) -> R<Flow> {
        let mut size_error = false;
        let mut dmax = 0;
        for (t, e) in computations {
            let loc = self.locate(&t.r)?;
            dmax = dmax.max(loc.kind.digits_scale().map_or(0, |(_, s)| s)).max(self.dmax(e)?);
        }
        if let Some((t, dividend, _)) = remainder {
            let loc = self.locate(&t.r)?;
            dmax = dmax.max(loc.kind.digits_scale().map_or(0, |(_, s)| s)).max(self.dmax(dividend)?);
        }
        let mut quotient_target: Option<Loc> = None;
        for (t, e) in computations {
            let loc = self.locate(&t.r)?;
            quotient_target.get_or_insert(loc);
            let outcome = if self.uses_float(e)? {
                self.eval_float(e, self.options.arith.float_intermediate(), pos).map(Val::Float)
            } else {
                self.eval_fixed(e, dmax, pos).map(Val::Num)
            };
            let value = match outcome {
                Err(a) if a.code == DIVIDE_BY_ZERO => {
                    if handler.is_none() {
                        return Err(Abend::check(ProgramCheck::DecimalDivide, pos));
                    }
                    size_error = true;
                    continue;
                }
                other => other?,
            };
            size_error |= self.store_value(loc, value, t.rounded, handler.is_some(), pos)?;
        }
        if let (Some((t, dividend, divisor)), Some(q_loc)) = (remainder, quotient_target) {
            let arith = self.options.arith;
            let (x, y) = (self.eval_fixed(dividend, dmax, pos)?, self.eval_fixed(divisor, dmax, pos)?);
            if !y.magnitude.is_zero() {
                let q = x.div(y, dmax, arith).map_err(|_| Abend::ironwork("remainder", pos))?;
                let q_places = places_of(q_loc.kind);
                let q = fixed(q.negative, align(&q, q_places.dec, false).unwrap_or_default(), Places::new(q.places.int, q_places.dec));
                let r = q.mul(y, dmax, arith).and_then(|p| x.sub(p, dmax, arith)).map_err(|_| Abend::ironwork("remainder", pos))?;
                let r_loc = self.locate(&t.r)?;
                size_error |= self.store_value(r_loc, Val::Num(r), false, handler.is_some(), pos)?;
            }
        }
        if let Some(h) = handler {
            return self.run_block(if size_error { &h.on } else { &h.not_on });
        }
        Ok(Flow::Next)
    }

    /// Stores an arithmetic result; returns whether it was a size error.
    fn store_value(&mut self, loc: Loc, value: Val, rounded: bool, keep_on_size_error: bool, pos: Pos) -> R<bool> {
        match (loc.kind, value) {
            (Kind::Float(p), Val::Float(h)) => {
                let h = if h.precision.digits() > p.digits() { float::narrow(h, p) } else { h.lengthen(p) };
                self.write(loc, &h.to_bytes());
                Ok(false)
            }
            (Kind::Float(p), Val::Num(f)) => {
                let h = float::from_fixed(f, p, ProgramMask::default()).map_err(|c| Abend::check(c, pos))?;
                self.write(loc, &h.to_bytes());
                Ok(false)
            }
            (kind, Val::Float(h)) => {
                let (f, overflow) = float::to_fixed(h, places_of(kind), rounded);
                if overflow && keep_on_size_error {
                    return Ok(true);
                }
                Ok(self.store_fixed_checked(loc, &f, false, keep_on_size_error, pos)? || overflow)
            }
            (_, Val::Num(f)) => self.store_fixed_checked(loc, &f, rounded, keep_on_size_error, pos),
            _ => Err(Abend::ironwork("a non-numeric arithmetic result", pos)),
        }
    }

    fn store_fixed(&mut self, loc: Loc, value: &Fixed, rounded: bool, pos: Pos) -> R<()> {
        self.store_fixed_checked(loc, value, rounded, false, pos).map(|_| ())
    }

    fn store_fixed_checked(&mut self, loc: Loc, value: &Fixed, rounded: bool, keep_on_size_error: bool, pos: Pos) -> R<bool> {
        let beyond = || Abend::ironwork("a value wider than 256 bits", pos);
        let (bytes, size_error) = match loc.kind {
            Kind::Zoned { digits, scale, signed, sign } => {
                let m = align(value, scale, rounded).ok_or_else(beyond)?;
                let cap = pow10(digits);
                let kept = m.div_rem(cap).1.to_u128().unwrap();
                let negative = signed && value.negative && kept != 0;
                (self.zoned_image(kept, digits, signed, negative, sign), m >= cap)
            }
            Kind::Packed { digits, scale, signed } => {
                let m = align(value, scale, rounded).ok_or_else(beyond)?;
                let cap = pow10(digits);
                let kept = m.div_rem(cap).1.to_u128().unwrap();
                let mut out = vec![0u8; loc.len];
                decimal::encode(&mut out, Decimal { negative: signed && value.negative && kept != 0, magnitude: kept }).map_err(|c| Abend::check(c, pos))?;
                if !signed {
                    *out.last_mut().unwrap() |= 0x0F;
                }
                (out, m >= cap)
            }
            Kind::Binary { digits, scale, signed, native } => {
                let m = align(value, scale, rounded).ok_or_else(beyond)?;
                let magnitude = m.to_u128().and_then(|m| i128::try_from(m).ok()).ok_or_else(beyond)?;
                let v = if value.negative { -magnitude } else { magnitude };
                let item = Binary { digits: digits as u8, signed, native };
                let stored = binary::store(item, v, &self.options);
                if let Some(d) = stored.divergence {
                    let name = self.layout.items.get(loc.item).map_or("RETURN-CODE".into(), |i| i.name.clone().unwrap_or_else(|| "FILLER".into()));
                    let _ = writeln!(
                        self.unit.err,
                        "ironwork: {pos}: TRUNC(OPT) store of {} into {name} PIC {}9({digits}) BINARY: the PICTURE keeps {}, the binary field {}; {} was stored (-silent stops these reports)",
                        d.value,
                        if signed { "S" } else { "" },
                        d.decimal,
                        d.binary,
                        d.binary
                    );
                }
                let bits = 8 * item.bytes() as u32;
                let binary_range = if signed { v >= -(1i128 << (bits - 1)) && v < (1i128 << (bits - 1)) } else { (0..(1i128 << bits)).contains(&v.abs()) };
                let exceeds = if native || self.options.trunc == Trunc::Bin { !binary_range } else { v.unsigned_abs() >= 10u128.pow(digits) };
                (stored.bytes, exceeds)
            }
            Kind::Float(p) => (float::from_fixed(*value, p, ProgramMask::default()).map_err(|c| Abend::check(c, pos))?.to_bytes(), false),
            Kind::Index => {
                let whole = align(value, 0, false).and_then(|m| m.to_u128()).and_then(|m| i32::try_from(m).ok()).ok_or_else(beyond)?;
                ((if value.negative { -whole } else { whole }).to_be_bytes().to_vec(), false)
            }
            Kind::NumericEdited { edit, digits, scale, blank_when_zero } => {
                let m = align(value, scale, rounded).ok_or_else(beyond)?;
                let cap = pow10(digits);
                let kept = m.div_rem(cap).1.to_u128().unwrap();
                let text = crate::edit::numeric(&self.layout.edits[edit as usize], digits, value.negative && kept != 0, kept, blank_when_zero);
                (self.page.encode(&text).map_err(|e| Abend::ironwork(e.to_string(), pos))?, m >= cap)
            }
            _ => return Err(Abend::ironwork("a numeric value stored into a non-numeric item", pos)),
        };
        if size_error && keep_on_size_error {
            return Ok(true);
        }
        self.write(loc, &bytes);
        Ok(size_error)
    }

    fn zoned_image(&self, magnitude: u128, digits: u32, signed: bool, negative: bool, sign: Option<SignClause>) -> Vec<u8> {
        let zone = match (signed, negative) {
            (false, _) => decimal::UNSIGNED,
            (true, true) => decimal::MINUS,
            (true, false) => decimal::PLUS,
        };
        match sign {
            Some(SignClause { separate: true, position }) => {
                let body = zoned_digits(magnitude, digits as usize, decimal::UNSIGNED);
                let s = if negative { 0x60 } else { 0x4E };
                if position == SignPosition::Leading { [&[s][..], &body].concat() } else { [&body[..], &[s]].concat() }
            }
            Some(SignClause { separate: false, position: SignPosition::Leading }) => {
                let mut body = zoned_digits(magnitude, digits as usize, decimal::UNSIGNED);
                body[0] = (zone << 4) | (body[0] & 0x0F);
                body
            }
            _ => zoned_digits(magnitude, digits as usize, zone),
        }
    }

    /// MOVE, and VALUE at start-up, into one receiving item.
    fn assign(&mut self, dest: Loc, val: Val, src: Option<Loc>, pos: Pos) -> R<()> {
        match dest.kind {
            Kind::Group | Kind::Alnum { .. } => {
                let justified = matches!(dest.kind, Kind::Alnum { justified: true });
                let image = self.alnum_image(&val, src, dest.len, pos)?;
                let mut out = vec![ebcdic::SPACE; dest.len];
                if justified && image.len() < dest.len {
                    out[dest.len - image.len()..].copy_from_slice(&image);
                } else if justified {
                    out.copy_from_slice(&image[image.len() - dest.len..]);
                } else {
                    let n = image.len().min(dest.len);
                    out[..n].copy_from_slice(&image[..n]);
                }
                self.write(dest, &out);
            }
            Kind::National => {
                let units: Vec<u16> = match val {
                    Val::National(b) => b.chunks(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect(),
                    Val::Bytes(b) => self.page.decode(&b).encode_utf16().collect(),
                    Val::Fig(f) => vec![figurative_unit(f); dest.len / 2],
                    _ => return Err(Abend::ironwork("this value cannot be moved to a national item", pos)),
                };
                let mut out: Vec<u8> = units.iter().take(dest.len / 2).flat_map(|u| u.to_be_bytes()).collect();
                while out.len() < dest.len {
                    out.extend_from_slice(&0x0020u16.to_be_bytes());
                }
                self.write(dest, &out);
            }
            Kind::Pointer => match val {
                Val::Address(a) => self.write(dest, &a.to_be_bytes()),
                Val::Fig(Figurative::Null) => self.write(dest, &[0; 4]),
                _ => return Err(Abend::ironwork("a pointer takes an address: use SET ... TO ADDRESS OF or NULL", pos)),
            },
            Kind::Index => match val {
                Val::Num(f) => self.store_fixed(dest, &f, false, pos)?,
                _ => return Err(Abend::ironwork("an index takes an occurrence number", pos)),
            },
            Kind::AlnumEdited { edit } => {
                let positions = self.layout.edits[edit as usize].iter().filter(|s| !matches!(s, crate::picture::Sym::Insert(_))).count();
                let image = self.alnum_image(&val, src, positions, pos)?;
                let page = self.page;
                let out = crate::edit::alphanumeric(&self.layout.edits[edit as usize], &image, ebcdic::SPACE, |c| page.encode_char(c).unwrap_or(ebcdic::SPACE));
                self.write(dest, &out);
            }
            Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. } | Kind::NumericEdited { .. } => match val {
                Val::Num(f) => {
                    if let (Some(s), Kind::Packed { digits, scale, signed: true }, Numproc::Pfd) = (src, dest.kind, self.options.numproc)
                        && s.kind == dest.kind
                        && digits > 0
                        && scale == places_of(s.kind).dec
                    {
                        let copied = sign::move_packed(self.bytes(s), true, Numproc::Pfd);
                        self.write(dest, &copied);
                    } else {
                        self.store_fixed(dest, &f, false, pos)?;
                    }
                }
                Val::Float(h) => {
                    let (f, _) = float::to_fixed(h, places_of(dest.kind), false);
                    self.store_fixed(dest, &f, false, pos)?;
                }
                Val::Fig(Figurative::Zero) => self.store_fixed(dest, &Fixed::new(0, Places::new(1, 0)), false, pos)?,
                Val::Fig(f) => self.write(dest, &vec![figurative_byte(f); dest.len]),
                Val::All(b) => {
                    let fill: Vec<u8> = b.iter().copied().cycle().take(dest.len).collect();
                    self.write(dest, &fill);
                }
                Val::Bytes(b) => {
                    let v = match src.map(|s| s.kind) {
                        Some(Kind::NumericEdited { edit, digits, scale, .. }) => {
                            let (negative, magnitude) = crate::edit::de_edit(&self.layout.edits[edit as usize], &self.page.decode(&b));
                            fixed(negative, U256::from_u128(magnitude), Places::new(digits - scale, scale))
                        }
                        _ => {
                            let places = Places::new(b.len() as u32, 0);
                            self.zoned_value(&b, b.len() as u32, false, None, places, pos)?
                        }
                    };
                    self.store_fixed(dest, &v, false, pos)?;
                }
                Val::National(_) => return Err(Abend::ironwork("a national value cannot be moved to a numeric item", pos)),
                Val::Address(_) => return Err(Abend::ironwork("a pointer cannot be moved to a numeric item", pos)),
            },
            Kind::Float(p) => {
                let h = match val {
                    Val::Float(h) if h.precision.digits() > p.digits() => float::narrow(h, p),
                    Val::Float(h) => h.lengthen(p),
                    Val::Num(f) => float::from_fixed(f, p, ProgramMask::default()).map_err(|c| Abend::check(c, pos))?,
                    Val::Fig(Figurative::Zero) => Hfp::zero(p),
                    _ => return Err(Abend::ironwork("this value cannot be moved to a floating-point item", pos)),
                };
                self.write(dest, &h.to_bytes());
            }
        }
        Ok(())
    }

    /// The bytes an alphanumeric receiver of `len` gets from `val`.
    fn alnum_image(&self, val: &Val, src: Option<Loc>, len: usize, pos: Pos) -> R<Vec<u8>> {
        Ok(match val {
            Val::Bytes(b) => b.clone(),
            Val::All(b) => b.iter().copied().cycle().take(len.max(b.len())).collect(),
            Val::Fig(f) => vec![figurative_byte(*f); len],
            Val::Num(f) if f.places.dec == 0 => {
                let digits = src.and_then(|s| s.kind.digits_scale()).map_or(f.places.total(), |(d, _)| d);
                zoned_digits(f.magnitude.to_u128().unwrap_or(0), digits as usize, decimal::UNSIGNED)
            }
            Val::National(_) => return Err(Abend::ironwork("a national value cannot be moved to an alphanumeric item", pos)),
            _ => return Err(Abend::ironwork("only an integer numeric value can be moved to an alphanumeric item", pos)),
        })
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
                let item = &self.layout.items[condition.item];
                let subject = Ref {
                    name: item.name.clone().unwrap_or_default(),
                    qualifiers: Vec::new(),
                    subscripts: r.subscripts.clone(),
                    refmod: None,
                    pos: r.pos,
                };
                let subject = Expr::Operand(Operand::Ref(subject));
                let values = condition.values.clone();
                for (low, high) in values {
                    let low = Expr::Operand(Operand::Literal(low));
                    let hit = match high {
                        None => self.compare(&subject, &low, pos)? == Ordering::Equal,
                        Some(high) => {
                            self.compare(&subject, &low, pos)? != Ordering::Less
                                && self.compare(&subject, &Expr::Operand(Operand::Literal(high)), pos)? != Ordering::Greater
                        }
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
            let bytes = self.bytes(loc).to_vec();
            return Ok(match (class, loc.kind) {
                (Class::Numeric, Kind::Packed { signed, .. }) => {
                    decimal::tp(&bytes).is_ok_and(|cc| cc.0 == 0) && (signed || bytes.last().is_some_and(|b| b & 0x0F == 0x0F))
                }
                (Class::Numeric, Kind::Zoned { signed, sign: None, .. }) => bytes.iter().enumerate().all(|(i, &b)| {
                    let zone_ok = if i + 1 == bytes.len() && signed { matches!(b >> 4, 0xC | 0xD | 0xF) } else { b >> 4 == 0xF };
                    zone_ok && b & 0x0F <= 9
                }),
                (Class::Numeric, _) => bytes.iter().all(|b| (0xF0..=0xF9).contains(b)),
                (_, _) => bytes.iter().all(|&b| b == ebcdic::SPACE || self.page.decode_byte(b).is_ascii_alphabetic()),
            });
        }
        let v = match self.expr_value(e, pos)? {
            Val::Num(f) => f,
            Val::Float(h) => float::to_fixed(h, Places::new(31, 0), false).0,
            _ => return Err(Abend::ironwork("a sign condition on a non-numeric operand", pos)),
        };
        Ok(match class {
            Class::Positive => !v.negative && !v.magnitude.is_zero(),
            Class::Negative => v.negative,
            _ => v.magnitude.is_zero(),
        })
    }

    fn compare(&mut self, a: &Expr, b: &Expr, pos: Pos) -> R<Ordering> {
        let (va, la) = self.comparand(a, pos)?;
        let (vb, lb) = self.comparand(b, pos)?;
        if let (Some(x), Some(y)) = (la, lb)
            && let (Kind::Packed { .. }, true, Numproc::Pfd) = (x.kind, x.kind == y.kind, self.options.numproc)
        {
            return sign::compare_packed(self.bytes(x), self.bytes(y), Numproc::Pfd).map_err(|c| Abend::check(c, pos));
        }
        let address = |v: &Val| match v {
            Val::Address(a) => Some(*a),
            Val::Fig(Figurative::Null) => Some(0),
            _ => None,
        };
        if matches!(va, Val::Address(_)) || matches!(vb, Val::Address(_)) {
            return match (address(&va), address(&vb)) {
                (Some(x), Some(y)) => Ok(x.cmp(&y)),
                _ => Err(Abend::ironwork("a pointer compared with something other than a pointer or NULL", pos)),
            };
        }
        let numeric = |v: &Val| matches!(v, Val::Num(_) | Val::Float(_));
        match (&va, &vb) {
            (Val::Float(_), _) | (_, Val::Float(_)) if (numeric(&va) || matches!(va, Val::Fig(Figurative::Zero))) && (numeric(&vb) || matches!(vb, Val::Fig(Figurative::Zero))) => {
                let to_float = |v: &Val| -> R<Hfp> {
                    Ok(match v {
                        Val::Float(h) => h.lengthen(Precision::Extended),
                        Val::Num(f) => float::from_fixed(*f, Precision::Extended, ProgramMask::default()).map_err(|c| Abend::check(c, pos))?,
                        _ => Hfp::zero(Precision::Extended),
                    })
                };
                Ok(to_float(&va)?.compare(to_float(&vb)?))
            }
            (Val::Num(x), Val::Num(y)) => Ok(compare_fixed(x, y)),
            (Val::Num(x), Val::Fig(Figurative::Zero)) => Ok(compare_fixed(x, &Fixed::new(0, Places::new(1, 0)))),
            (Val::Fig(Figurative::Zero), Val::Num(y)) => Ok(compare_fixed(&Fixed::new(0, Places::new(1, 0)), y)),
            (Val::National(x), Val::National(y)) => Ok(compare_national(x, y)),
            _ => {
                let len = self.image_len(&va, la).max(self.image_len(&vb, lb));
                let x = self.alnum_image(&va, la, len, pos)?;
                let y = self.alnum_image(&vb, lb, len, pos)?;
                Ok(ebcdic::compare_alphanumeric(&x, &y, &Collation::Native))
            }
        }
    }

    fn image_len(&self, v: &Val, loc: Option<Loc>) -> usize {
        match (v, loc) {
            (_, Some(l)) if !l.kind.is_numeric() => l.len,
            (Val::Bytes(b) | Val::All(b), _) => b.len(),
            (Val::Num(f), Some(l)) => l.kind.digits_scale().map_or(f.places.total(), |(d, _)| d) as usize,
            (Val::Num(f), None) => f.places.total() as usize,
            _ => 1,
        }
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
            match op {
                Operand::Ref(r) => {
                    let loc = self.locate(r)?;
                    match loc.kind {
                        Kind::National => text.push_str(&utf16_text(self.bytes(loc))),
                        Kind::Packed { digits, signed, .. } | Kind::Binary { digits, signed, .. } => {
                            let Val::Num(f) = self.read(loc, r.pos)? else { unreachable!() };
                            let zone = match (signed, f.negative) {
                                (false, _) => decimal::UNSIGNED,
                                (true, true) => decimal::MINUS,
                                (true, false) => decimal::PLUS,
                            };
                            let whole = match loc.kind {
                                Kind::Binary { native, .. } => native || self.options.trunc == Trunc::Bin,
                                _ => false,
                            };
                            let shown = if whole {
                                let width = match loc.len {
                                    2 => 5,
                                    4 => 10,
                                    _ if signed => 19,
                                    _ => 20,
                                };
                                zoned_digits(f.magnitude.to_u128().unwrap_or(0), width, zone)
                            } else {
                                zoned_digits(f.magnitude.div_rem(pow10(digits)).1.to_u128().unwrap_or(0), digits as usize, zone)
                            };
                            text.push_str(&self.page.decode(&shown));
                        }
                        Kind::Float(_) => return Err(Abend::ironwork("DISPLAY of a floating-point item is not supported yet", r.pos)),
                        Kind::Pointer | Kind::Index => return Err(Abend::ironwork("DISPLAY of a pointer or index is not supported", r.pos)),
                        _ => text.push_str(&self.page.decode(self.bytes(loc))),
                    }
                }
                Operand::Literal(Literal::Number(t)) => text.push_str(t),
                other => match self.operand(other, pos)? {
                    Val::Bytes(b) | Val::All(b) => text.push_str(&self.page.decode(&b)),
                    Val::National(b) => text.push_str(&utf16_text(&b)),
                    Val::Fig(f) => text.push(self.page.decode_byte(figurative_byte(f))),
                    Val::Num(f) => text.push_str(&self.page.decode(&zoned_digits(f.magnitude.to_u128().unwrap_or(0), f.places.total() as usize, decimal::UNSIGNED))),
                    Val::Float(_) => return Err(Abend::ironwork("DISPLAY of a floating-point value is not supported yet", pos)),
                    Val::Address(_) => return Err(Abend::ironwork("DISPLAY of a pointer is not supported", pos)),
                },
            }
        }
        let result = if no_advancing { write!(self.unit.out, "{text}") } else { writeln!(self.unit.out, "{text}") };
        result.map_err(|e| match e.kind() {
            std::io::ErrorKind::BrokenPipe => Abend { code: CLOSED_OUTPUT.into(), message: "standard output closed".into(), pos },
            _ => Abend::ironwork(format!("DISPLAY: {e}"), pos),
        })
    }

    fn initialize(&mut self, index: usize, offset: usize, pos: Pos) -> R<()> {
        let layout = self.layout;
        let item = &layout.items[index];
        if item.kind == Kind::Index {
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

fn utf16_text(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes.chunks(2).map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect();
    String::from_utf16_lossy(&units)
}

fn compare_national(a: &[u8], b: &[u8]) -> Ordering {
    let unit = |s: &[u8], i: usize| if i + 1 < s.len() { u16::from_be_bytes([s[i], s[i + 1]]) } else { 0x0020 };
    let len = a.len().max(b.len());
    (0..len).step_by(2).map(|i| unit(a, i).cmp(&unit(b, i))).find(|o| o.is_ne()).unwrap_or(Ordering::Equal)
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

/// Days since 1970-01-01 of a proleptic Gregorian date.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let (y, m) = if month <= 2 { (year - 1, month + 9) } else { (year, month - 3) };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + (153 * m + 2) / 5 + day - 1;
    era * 146_097 + doe - 719_468
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// NUMVAL and NUMVAL-C: spaces, one sign (leading + or -, trailing + - CR or DB), digits with at most
/// one decimal point; NUMVAL-C also allows the currency sign and commas. None when the text is
/// anything else.
fn numval(text: &str, currency: Option<&str>) -> Option<Fixed> {
    let mut t = text.trim().to_ascii_uppercase();
    let mut negative = false;
    for (suffix, minus) in [("CR", true), ("DB", true), ("-", true), ("+", false)] {
        if let Some(rest) = t.strip_suffix(suffix) {
            negative = minus;
            t = rest.trim_end().to_owned();
            break;
        }
    }
    if let Some(rest) = t.strip_prefix('-') {
        negative = true;
        t = rest.trim_start().to_owned();
    } else if let Some(rest) = t.strip_prefix('+') {
        t = rest.trim_start().to_owned();
    }
    if let Some(c) = currency {
        t = t.trim_start_matches(c.trim()).trim_start().replace(',', "");
    }
    let (int, frac) = t.split_once('.').unwrap_or((&t, ""));
    if int.is_empty() && frac.is_empty() || !int.chars().chain(frac.chars()).all(|c| c.is_ascii_digit()) || int.len() + frac.len() > 31 {
        return None;
    }
    let digits: u128 = format!("{int}{frac}").parse().unwrap_or(0);
    let f = Fixed::new(digits as i128, Places::new(int.len().max(1) as u32, frac.len() as u32));
    Some(if negative { Fixed { negative: digits != 0, ..f } } else { f })
}

