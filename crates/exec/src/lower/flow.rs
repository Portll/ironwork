//! Control flow (lir.md §8): one entry block per paragraph, PERFORM as loops around `PerformEnter`
//! or an inline body, each `Flow` the walker returns as an explicit terminator, and the ranges and
//! triggers the declaratives run by (§9.10).

use super::cond::Test;
use super::data::{Side, scale};
use super::{Lower, LowerError, R, push, unsupported};
use crate::declaratives::{Span, debug_name};
use rt::abend::{AbendCode, Ending};
use rt::lir::{self, BlockId, DebugId, Op, RangeId, RangeKind, Terminator};
use rt::storage::Kind;
use syntax::Pos;
use syntax::ast::{BinOp, ExecKind, ExitKind, Expr, Loop, Object, Operand, ProcName, RelOp, SizeError, Sorting, Stmt, Subject, Target, Varying, When};

/// Blocks under construction; `current` is the one ops go into, None after a terminator.
#[derive(Default)]
pub(super) struct Blocks {
    open: Vec<Open>,
    current: Option<BlockId>,
}

#[derive(Default)]
struct Open {
    ops: Vec<Op>,
    at: Vec<DebugId>,
    end: Option<(Terminator, DebugId)>,
}

impl Blocks {
    /// The blocks and, per block, the debug entry of each op and of the terminator.
    pub(super) fn finish(self) -> R<(Vec<lir::Block>, Vec<Vec<DebugId>>)> {
        let mut blocks = Vec::with_capacity(self.open.len());
        let mut debug = Vec::with_capacity(self.open.len());
        for (b, open) in self.open.into_iter().enumerate() {
            let Some((end, at)) = open.end else { return Err(LowerError::Invalid(format!("block {b} has no terminator"))) };
            let mut ids = open.at;
            ids.push(at);
            blocks.push(lir::Block { ops: open.ops, end });
            debug.push(ids);
        }
        Ok((blocks, debug))
    }
}

/// Where a statement sits: its paragraph, the paragraph statement it is part of (for NEXT
/// SENTENCE), a position for statements that carry none, and the inline PERFORMs around it.
#[derive(Clone)]
pub(super) struct Ctx {
    para: usize,
    top: usize,
    pos: Pos,
    loops: Vec<Inline>,
}

/// An inline PERFORM's exit, which EXIT PERFORM takes, and continuation, which EXIT PERFORM CYCLE
/// takes.
#[derive(Clone, Copy)]
struct Inline {
    exit: BlockId,
    cont: BlockId,
}

enum Body<'a> {
    /// `resumes`: the PERFORM runs once and is a statement of its paragraph (C99).
    Range { range: RangeId, resumes: bool },
    Inline(&'a [Stmt]),
}

/// A VARYING or AFTER phrase: the MOVE of FROM, the step by BY, and the UNTIL test.
struct VaryLevel {
    from: Op,
    step: Op,
    until: Test,
}

impl Lower<'_> {
    pub(super) fn new_block(&mut self) -> R<BlockId> {
        push(&mut self.blocks.open, Open::default(), "blocks")
    }

    /// The open block, or a new one when code follows a terminator and cannot be reached.
    fn current(&mut self) -> R<BlockId> {
        match self.blocks.current {
            Some(b) => Ok(b),
            None => {
                let b = self.new_block()?;
                self.blocks.current = Some(b);
                Ok(b)
            }
        }
    }

    pub(super) fn op(&mut self, op: Op, pos: Pos) -> R<()> {
        let at = self.at(pos);
        let b = self.current()? as usize;
        self.blocks.open[b].ops.push(op);
        self.blocks.open[b].at.push(at);
        Ok(())
    }

    pub(super) fn end(&mut self, end: Terminator, pos: Pos) -> R<()> {
        let at = self.at(pos);
        let b = self.current()? as usize;
        self.blocks.open[b].end = Some((end, at));
        self.blocks.current = None;
        Ok(())
    }

    /// Ends the open block, if there is one, with a jump to `to`.
    pub(super) fn jump(&mut self, to: BlockId, pos: Pos) -> R<()> {
        match self.blocks.current {
            Some(_) => self.end(Terminator::Jump(to), pos),
            None => Ok(()),
        }
    }

    pub(super) fn switch(&mut self, b: BlockId) -> R<()> {
        if let Some(open) = self.blocks.current {
            return Err(LowerError::Invalid(format!("block {open} left open for block {b}")));
        }
        self.blocks.current = Some(b);
        Ok(())
    }

    fn sentence(&mut self, para: usize, k: usize) -> R<BlockId> {
        if let Some(&b) = self.sentences.get(&(para, k)) {
            return Ok(b);
        }
        let b = self.new_block()?;
        self.sentences.insert((para, k), b);
        Ok(b)
    }

    /// The PROCEDURE DIVISION: every range first, since a paragraph's end and each GO TO depend on
    /// all of them, then each paragraph from its entry block.
    pub(super) fn procedure(&mut self) -> R<Vec<lir::Paragraph>> {
        let program = self.program;
        let n = program.paragraphs.len();
        if u32::try_from(n + 1).is_err() {
            return Err(LowerError::Exceeds("paragraphs", Pos::default()));
        }
        for p in &program.paragraphs {
            self.collect(&p.statements)?;
        }
        self.segments = self.altered.iter().any(|&p| program.paragraphs[p].priority >= 50);
        self.entries = (0..n).map(|_| self.new_block()).collect::<R<_>>()?;
        let mut paragraphs = Vec::with_capacity(n);
        for (p, para) in program.paragraphs.iter().enumerate() {
            self.switch(self.entries[p])?;
            self.paragraph(p)?;
            paragraphs.push(lir::Paragraph {
                name: self.sym(&para.name),
                is_section: para.is_section,
                entry: self.entries[p],
                section_end: crate::section_end(program, p) as u32,
                priority: para.priority,
                at: self.at(para.pos),
                abandoned: None,
            });
        }
        for (p, para) in program.paragraphs.iter().enumerate() {
            if self.ranges.iter().any(|r| r.last as usize == p) {
                let message = format!(
                    "control passed the end of {}, which is armed to return to a PERFORM that control left by GO TO; ironwork returns there only to a PERFORM that runs once and is not inside another statement",
                    para.name
                );
                paragraphs[p].abandoned = Some(self.abend(AbendCode::Ironwork, &message, Some(para.pos))?);
            }
        }
        Ok(paragraphs)
    }

    /// The open modes' EXCEPTION/ERROR procedures and, under the DEBUG option, DEBUG-ITEM, with a
    /// range for each debugging section that serves a paragraph.
    pub(super) fn declaratives(&mut self) -> R<lir::Declaratives> {
        let table = &self.c.declaratives;
        let mut modes = [None; 4];
        for (mode, span) in modes.iter_mut().zip(table.modes) {
            *mode = span.map(|s| self.span_range(s, RangeKind::UseProcedure)).transpose()?;
        }
        for (span, _) in table.triggers.iter().flatten() {
            self.span_range(*span, RangeKind::Debugging)?;
        }
        let debug_item = table.debug_item.filter(|_| self.debugging).map(|i| (self.layout.items[i].offset, self.layout.items[i].size));
        Ok(lir::Declaratives { modes, debug_item })
    }

    /// The range a declarative section runs as.
    pub(super) fn span_range(&mut self, (first, last): Span, kind: RangeKind) -> R<RangeId> {
        self.intern_range(first as u32, last as u32, kind)
    }

    fn intern_range(&mut self, first: u32, last: u32, kind: RangeKind) -> R<RangeId> {
        if let Some(&r) = self.range_ids.get(&(first, last, kind)) {
            return Ok(r);
        }
        let r = push(&mut self.ranges, lir::Range { first, last, kind }, "ranges")?;
        self.range_ids.insert((first, last, kind), r);
        Ok(r)
    }

    /// The debugging section that runs before paragraph p, and the name DEBUG-NAME gives it.
    fn trigger(&self, p: usize) -> Option<(Span, String)> {
        self.c.declaratives.triggers.get(p).cloned().flatten()
    }

    /// Every PERFORM range, and every paragraph an ALTER names.
    fn collect(&mut self, stmts: &[Stmt]) -> R<()> {
        for s in stmts {
            match s {
                Stmt::PerformProc { from, thru, pos, .. } => {
                    self.range(from, thru.as_ref(), *pos)?;
                }
                Stmt::Alter { pairs, .. } => {
                    for (from, _) in pairs {
                        if let Ok((p, _)) = crate::procedure(self.program, from) {
                            self.altered.insert(p);
                        }
                    }
                }
                _ => {}
            }
            for body in crate::oo::bodies(s) {
                self.collect(body)?;
            }
        }
        Ok(())
    }

    /// The paragraphs an out-of-line PERFORM runs, as `crate::procedure` resolves them.
    fn range(&mut self, from: &ProcName, thru: Option<&ProcName>, pos: Pos) -> R<RangeId> {
        let program = self.program;
        let Ok((first, first_end)) = crate::procedure(program, from) else { return unsupported("a PERFORM of a procedure the walker cannot find", pos) };
        let last = match thru {
            None => first_end,
            Some(t) => match crate::procedure(program, t) {
                Ok((_, last)) => last,
                Err(_) => return unsupported("a PERFORM of a procedure the walker cannot find", pos),
            },
        };
        self.intern_range(first as u32, last as u32, RangeKind::Perform)
    }

    /// `run_sentences` over one paragraph, entered as `run_region` enters it: its segment, its
    /// debugging section, then the GO TO an ALTER set. A separator period starts a block NEXT
    /// SENTENCE can reach, and an ENTRY statement one a CALL can enter.
    fn paragraph(&mut self, p: usize) -> R<()> {
        let para = &self.program.paragraphs[p];
        if self.segments {
            self.op(Op::EnterSegment(para.priority), para.pos)?;
        }
        if let Some((span, name)) = self.trigger(p) {
            let name = self.sym(&name);
            let range = self.span_range(span, RangeKind::Debugging)?;
            let next = self.new_block()?;
            self.end(Terminator::Debug { range, name, next }, para.pos)?;
            self.switch(next)?;
        }
        if self.debugging && para.is_section {
            self.op(Op::DebugLine(para.pos.line), para.pos)?;
        }
        if self.altered.contains(&p) {
            let body = self.new_block()?;
            self.end(Terminator::AlteredGoTo { para: p as u32, otherwise: body }, para.pos)?;
            self.switch(body)?;
        }
        for (i, s) in para.statements.iter().enumerate() {
            let pos = stmt_pos(s).unwrap_or(para.pos);
            match s {
                Stmt::SentenceEnd => {
                    let b = self.sentence(p, i + 1)?;
                    self.jump(b, pos)?;
                    self.switch(b)?;
                }
                Stmt::Entry { .. } => {
                    self.debug_line(pos)?;
                    let b = self.new_block()?;
                    self.jump(b, pos)?;
                    self.switch(b)?;
                    self.entry_blocks.insert((p, i + 1), b);
                }
                _ => self.statement(s, &Ctx { para: p, top: i, pos, loops: Vec::new() })?,
            }
        }
        if self.blocks.current.is_some() {
            let end = self.paragraph_end(p, p + 1);
            self.end(end, para.pos)?;
        }
        Ok(())
    }

    /// Leaving paragraph `p` for `next`: a plain jump when no range's last paragraph lies from `p`
    /// to `next` − 1, since no return point is armed there and no region ends there, and no
    /// debugging section reads how control came to `next` (lir.md §8.2).
    fn paragraph_end(&self, p: usize, next: usize) -> Terminator {
        let completes = self.ranges.iter().any(|r| p <= r.last as usize && (r.last as usize) < next);
        if next < self.entries.len() && !completes && self.trigger(next).is_none() {
            Terminator::Jump(self.entries[next])
        } else {
            Terminator::ParagraphEnd { next: next as u32 }
        }
    }

    /// Under the DEBUG option, the line register takes a statement's line as it starts.
    fn debug_line(&mut self, pos: Pos) -> R<()> {
        if self.debugging { self.op(Op::DebugLine(pos.line), pos) } else { Ok(()) }
    }

    pub(super) fn statements(&mut self, stmts: &[Stmt], ctx: &Ctx) -> R<()> {
        for s in stmts {
            self.statement(s, ctx)?;
        }
        Ok(())
    }

    fn statement(&mut self, s: &Stmt, ctx: &Ctx) -> R<()> {
        if let Some(at) = stmt_pos(s) {
            self.debug_line(at)?;
        }
        let pos = stmt_pos(s).unwrap_or(ctx.pos);
        let inner = Ctx { pos, ..ctx.clone() };
        match s {
            Stmt::Move { from, to, .. } => {
                for r in to {
                    let dest = self.place(r, true)?;
                    let sender = self.operand(from, pos)?;
                    let plan = self.move_plan(&sender.side, self.kind_of(dest), self.place_items[dest as usize])?;
                    self.op(Op::Move { from: sender.operand, to: dest, plan }, pos)?;
                }
            }
            Stmt::Compute { targets, expr, size_error, .. } => {
                let computations: Vec<(&Target, &Expr)> = targets.iter().map(|t| (t, expr)).collect();
                self.arithmetic(&computations, None, size_error.as_ref(), pos, &inner)?;
            }
            Stmt::Arith(a) => {
                let computations: Vec<(&Target, &Expr)> = a.computations.iter().map(|(t, e)| (t, e)).collect();
                self.arithmetic(&computations, a.remainder.as_ref(), a.size_error.as_ref(), pos, &inner)?;
            }
            Stmt::If { cond, then, otherwise, .. } => {
                let test = self.test(cond, pos)?;
                let (yes, no, join) = (self.new_block()?, self.new_block()?, self.new_block()?);
                self.branch(test, yes, no, pos)?;
                self.switch(yes)?;
                self.statements(then, &inner)?;
                self.jump(join, pos)?;
                self.switch(no)?;
                self.statements(otherwise, &inner)?;
                self.jump(join, pos)?;
                self.switch(join)?;
            }
            Stmt::Evaluate { subjects, whens, other, .. } => self.evaluate(subjects, whens, other, pos, &inner)?,
            Stmt::PerformProc { from, thru, repeat, .. } => {
                let range = self.range(from, thru.as_ref(), pos)?;
                let own = self.program.paragraphs[ctx.para].statements.get(ctx.top).is_some_and(|top| std::ptr::eq(top, s));
                let body = Body::Range { range, resumes: own && matches!(repeat, Loop::Once) };
                self.perform(repeat, body, pos, &inner)?;
            }
            Stmt::PerformInline { body, repeat, .. } => self.perform(repeat, Body::Inline(body), pos, &inner)?,
            Stmt::Display { items, no_advancing, .. } => {
                let plan = self.display_plan(items, *no_advancing, pos)?;
                self.op(Op::Display(plan), pos)?;
            }
            Stmt::Initialize { targets, .. } => {
                for r in targets {
                    let target = self.place(r, false)?;
                    let plan = self.init_plan(target)?;
                    self.op(Op::Initialize { target, plan }, pos)?;
                }
            }
            // Unaltered, a GO TO with no target does nothing; the paragraph's entry holds the altered one.
            Stmt::GoTo { target: None, .. } | Stmt::Entry { .. } => {}
            Stmt::GoToDepending { targets, on, .. } => {
                let value = self.int_expr(&Expr::Operand(Operand::Ref(on.clone())), pos)?;
                let mut paragraphs = Vec::with_capacity(targets.len());
                for target in targets {
                    let Ok((t, _)) = crate::procedure(self.program, target) else { return unsupported("a GO TO DEPENDING ON target the walker cannot resolve", pos) };
                    paragraphs.push(t as u32);
                }
                let next = self.new_block()?;
                self.end(Terminator::Switch { value, targets: paragraphs, otherwise: next }, pos)?;
                self.switch(next)?;
            }
            Stmt::Alter { pairs, .. } => {
                let mut altered = Vec::with_capacity(pairs.len());
                for (from, to) in pairs {
                    let (Ok((para, _)), Ok((target, _))) = (crate::procedure(self.program, from), crate::procedure(self.program, to)) else {
                        return unsupported("an ALTER the walker cannot resolve", pos);
                    };
                    self.op(Op::Alter { para: para as u32, to: target as u32 }, pos)?;
                    altered.push((para, to));
                }
                if !self.c.declaratives.declarative_alters.contains(&pos) {
                    for (para, to) in altered {
                        let Some((span, name)) = self.trigger(para) else { continue };
                        let name = self.sym(&name);
                        let contents = self.sym(&debug_name(to));
                        let range = self.span_range(span, RangeKind::Debugging)?;
                        self.op(Op::DebugAlter { range, name, contents }, pos)?;
                    }
                }
            }
            Stmt::Open { .. } | Stmt::Close { .. } | Stmt::Read(_) | Stmt::Write { .. } | Stmt::Rewrite { .. } | Stmt::Delete { .. } | Stmt::Start { .. } => {
                self.file_statement(s, pos, &inner)?
            }
            Stmt::Set { set, .. } => self.set(set, pos)?,
            Stmt::Call(c) => match self.call_plan(c, pos)? {
                Ok(plan) => {
                    self.op(Op::Call(plan), pos)?;
                    self.phrases(c.on_exception.as_deref(), c.not_on_exception.as_deref(), pos, &inner)?;
                }
                Err(abend) => self.end(Terminator::Abend(abend), pos)?,
            },
            Stmt::Cancel { targets, .. } => {
                for t in targets {
                    let name = self.operand(t, pos)?.operand;
                    self.op(Op::Cancel(name), pos)?;
                }
            }
            Stmt::Invoke(i) => {
                let plan = self.invoke_plan(i, pos)?;
                self.op(Op::Invoke(plan), pos)?;
                self.phrases(i.on_exception.as_deref(), i.not_on_exception.as_deref(), pos, &inner)?;
            }
            Stmt::GoTo { target: Some(target), .. } => {
                let Ok((t, _)) = crate::procedure(self.program, target) else { return unsupported("a GO TO the walker cannot resolve", pos) };
                self.unnest(ctx.loops.len(), pos)?;
                let (p, t32, n) = (ctx.para as u32, t as u32, self.entries.len() as u32);
                let holds = |r: &lir::Range, q: u32| {
                    let (lo, hi) = r.region(n);
                    lo <= q && q <= hi
                };
                let stays = self.ranges.iter().all(|r| !holds(r, p) || holds(r, t32)) && self.trigger(t).is_none();
                self.end(if stays { Terminator::Jump(self.entries[t]) } else { Terminator::GoTo(t32) }, pos)?;
            }
            Stmt::Goback { .. } | Stmt::ExitMethod { .. } => self.end(Terminator::End(Ending::Goback), pos)?,
            Stmt::StopRun { .. } => self.end(Terminator::End(Ending::StopRun), pos)?,
            Stmt::ExitProgram { .. } => {
                let next = self.new_block()?;
                self.end(Terminator::ExitProgram { next }, pos)?;
                self.switch(next)?;
            }
            Stmt::Continue | Stmt::SentenceEnd | Stmt::Exit { kind: ExitKind::Plain, .. } => {}
            Stmt::Exit { kind: ExitKind::Paragraph, .. } => self.leave(ctx.para + 1, ctx, pos)?,
            Stmt::Exit { kind: ExitKind::Section, .. } => self.leave(crate::section_end(self.program, ctx.para) + 1, ctx, pos)?,
            Stmt::Exit { kind: ExitKind::Perform, .. } => match ctx.loops.last() {
                Some(l) => self.end(Terminator::Jump(l.exit), pos)?,
                None => self.leave(ctx.para + 1, ctx, pos)?,
            },
            Stmt::Exit { kind: ExitKind::PerformCycle, .. } => match ctx.loops.last() {
                Some(l) => self.end(Terminator::Jump(l.cont), pos)?,
                None => self.leave(ctx.para + 1, ctx, pos)?,
            },
            Stmt::NextSentence => {
                let stmts = &self.program.paragraphs[ctx.para].statements;
                let after = stmts[ctx.top..].iter().position(|s| *s == Stmt::SentenceEnd).map(|j| ctx.top + j + 1);
                self.unnest(ctx.loops.len(), pos)?;
                let end = match after {
                    Some(k) => Terminator::Jump(self.sentence(ctx.para, k)?),
                    None => self.paragraph_end(ctx.para, ctx.para + 1),
                };
                self.end(end, pos)?;
            }
            other => return unsupported(statement_name(other), pos),
        }
        Ok(())
    }

    /// Releases the inline PERFORMs a transfer leaves, as the walker's unwinding does.
    fn unnest(&mut self, loops: usize, pos: Pos) -> R<()> {
        if loops == 0 {
            return Ok(());
        }
        let n = u8::try_from(loops).map_err(|_| LowerError::Exceeds("inline PERFORMs around one statement", pos))?;
        self.op(Op::Unnest(n), pos)
    }

    /// EXIT PARAGRAPH, EXIT SECTION, and EXIT PERFORM outside an inline PERFORM.
    fn leave(&mut self, next: usize, ctx: &Ctx, pos: Pos) -> R<()> {
        self.unnest(ctx.loops.len(), pos)?;
        let end = self.paragraph_end(ctx.para, next);
        self.end(end, pos)
    }

    /// A branch on `test`; a test with an abending leaf becomes branches in its order.
    fn branch(&mut self, test: Test, then: BlockId, otherwise: BlockId, pos: Pos) -> R<()> {
        if !test.abends() {
            let cond = self.fold(&test)?;
            return self.end(Terminator::Branch { cond, then, otherwise }, pos);
        }
        match test {
            Test::Cond(cond) => self.end(Terminator::Branch { cond, then, otherwise }, pos),
            Test::Abend(abend, at) => self.end(Terminator::Abend(abend), at),
            Test::Not(a) => self.branch(*a, otherwise, then, pos),
            Test::And(a, b) => {
                let mid = self.new_block()?;
                self.branch(*a, mid, otherwise, pos)?;
                self.switch(mid)?;
                self.branch(*b, then, otherwise, pos)
            }
            Test::Or(a, b) => {
                let mid = self.new_block()?;
                self.branch(*a, then, mid, pos)?;
                self.switch(mid)?;
                self.branch(*b, then, otherwise, pos)
            }
        }
    }

    /// ON EXCEPTION and NOT ON EXCEPTION after a CALL or INVOKE op, which returns Arm(1) and Arm(0)
    /// when either phrase is written.
    fn phrases(&mut self, on: Option<&[Stmt]>, not_on: Option<&[Stmt]>, pos: Pos, ctx: &Ctx) -> R<()> {
        if on.is_none() && not_on.is_none() {
            return Ok(());
        }
        let (normal, exception, join) = (self.new_block()?, self.new_block()?, self.new_block()?);
        self.end(Terminator::Select(vec![normal, exception]), pos)?;
        self.switch(normal)?;
        self.statements(not_on.unwrap_or_default(), ctx)?;
        self.jump(join, pos)?;
        self.switch(exception)?;
        self.statements(on.unwrap_or_default(), ctx)?;
        self.jump(join, pos)?;
        self.switch(join)
    }

    fn arithmetic(&mut self, computations: &[(&Target, &Expr)], remainder: Option<&(Target, Expr, Expr)>, handler: Option<&SizeError>, pos: Pos, ctx: &Ctx) -> R<()> {
        let plan = self.arith_plan(computations, remainder, handler.is_some(), pos)?;
        self.op(Op::Arith(plan), pos)?;
        let Some(handler) = handler else { return Ok(()) };
        let (not_on, on, join) = (self.new_block()?, self.new_block()?, self.new_block()?);
        self.end(Terminator::Select(vec![not_on, on]), pos)?;
        self.switch(not_on)?;
        self.statements(&handler.not_on, ctx)?;
        self.jump(join, pos)?;
        self.switch(on)?;
        self.statements(&handler.on, ctx)?;
        self.jump(join, pos)?;
        self.switch(join)
    }

    /// `Stmt::Evaluate`: a chain of branches, each WHEN's alternatives in turn, each subject
    /// evaluated again at each comparison (lir.md §11, item 6).
    fn evaluate(&mut self, subjects: &[Subject], whens: &[When], other: &[Stmt], pos: Pos, ctx: &Ctx) -> R<()> {
        let join = self.new_block()?;
        for when in whens.iter().filter(|w| !w.alternatives.is_empty()) {
            let (body, fail) = (self.new_block()?, self.new_block()?);
            for (k, alternative) in when.alternatives.iter().enumerate() {
                let next = if k + 1 == when.alternatives.len() { fail } else { self.new_block()? };
                for (subject, object) in subjects.iter().zip(alternative) {
                    if self.blocks.current.is_none() {
                        break;
                    }
                    self.pair(subject, object, next, pos)?;
                }
                self.jump(body, pos)?;
                if next != fail {
                    self.switch(next)?;
                }
            }
            self.switch(body)?;
            self.statements(&when.body, ctx)?;
            self.jump(join, pos)?;
            self.switch(fail)?;
        }
        self.statements(other, ctx)?;
        self.jump(join, pos)?;
        self.switch(join)
    }

    /// One subject against one object (`alternative_matches`): falls through when they match.
    fn pair(&mut self, subject: &Subject, object: &Object, fail: BlockId, pos: Pos) -> R<()> {
        let test = match (subject, object) {
            (_, Object::Any) => return Ok(()),
            (Subject::Bool(b), Object::Bool(o)) => return if b == o { Ok(()) } else { self.jump(fail, pos) },
            (Subject::Bool(b), Object::Cond(c)) | (Subject::Cond(c), Object::Bool(b)) => {
                let t = self.test(c, pos)?;
                if *b { t } else { t.not() }
            }
            (Subject::Cond(c), Object::Cond(d)) => {
                let tc = self.test(c, pos)?;
                let (yes, no, pass) = (self.new_block()?, self.new_block()?, self.new_block()?);
                self.branch(tc, yes, no, pos)?;
                self.switch(yes)?;
                let td = self.test(d, pos)?;
                self.branch(td, pass, fail, pos)?;
                self.switch(no)?;
                let td = self.test(d, pos)?;
                self.branch(td, fail, pass, pos)?;
                return self.switch(pass);
            }
            (Subject::Expr(e), Object::Value { not, from, thru }) => {
                let t = match thru {
                    None => self.relation(e, RelOp::Eq, from, pos)?,
                    Some(thru) => Test::And(Box::new(self.relation(e, RelOp::Ge, from, pos)?), Box::new(self.relation(e, RelOp::Le, thru, pos)?)),
                };
                if *not { t.not() } else { t }
            }
            _ => Test::Abend(self.ironwork("a WHEN object of a different kind from its subject")?, pos),
        };
        let pass = self.new_block()?;
        self.branch(test, pass, fail, pos)?;
        self.switch(pass)
    }

    fn temp(&mut self, pos: Pos) -> R<lir::TempId> {
        let t = self.temps;
        self.temps = t.checked_add(1).ok_or(LowerError::Exceeds("PERFORM TIMES counters", pos))?;
        Ok(t)
    }

    /// PERFORM as `repeat` runs it: the depth raised once, the loop, and the depth released at its
    /// exit (lir.md §8.3). Each TIMES statement has a counter of its own. A PERFORM that can resume
    /// goes on after the exit in a block of its own, where the resume enters.
    fn perform(&mut self, repeat: &Loop, body: Body<'_>, pos: Pos, ctx: &Ctx) -> R<()> {
        self.op(Op::Nest, pos)?;
        let exit = self.new_block()?;
        let after = match body {
            Body::Range { resumes: true, .. } => Some(lir::Resume { para: ctx.para as u32, block: self.new_block()? }),
            _ => None,
        };
        match repeat {
            Loop::Once => self.run_body(&body, exit, exit, after, pos, ctx)?,
            Loop::Times(count) => {
                let temp = self.temp(pos)?;
                let n = self.int_expr(count, pos)?;
                self.op(Op::SetTemp(temp, n), pos)?;
                let (head, run) = (self.new_block()?, self.new_block()?);
                self.jump(head, pos)?;
                self.switch(head)?;
                let counter = self.cond(lir::Cond::Counter(temp))?;
                self.end(Terminator::Branch { cond: counter, then: run, otherwise: exit }, pos)?;
                self.switch(run)?;
                self.op(Op::DecTemp(temp), pos)?;
                self.run_body(&body, head, exit, None, pos, ctx)?;
            }
            Loop::Until { cond, test_after } => {
                let until = self.test(cond, pos)?;
                let run = self.new_block()?;
                if *test_after {
                    let cont = self.new_block()?;
                    self.jump(run, pos)?;
                    self.switch(run)?;
                    self.run_body(&body, cont, exit, None, pos, ctx)?;
                    self.switch(cont)?;
                    self.branch(until, exit, run, pos)?;
                } else {
                    let head = self.new_block()?;
                    self.jump(head, pos)?;
                    self.switch(head)?;
                    self.branch(until, exit, run, pos)?;
                    self.switch(run)?;
                    self.run_body(&body, head, exit, None, pos, ctx)?;
                }
            }
            Loop::Varying { varying, after, test_after } => {
                let levels: Vec<&Varying> = std::iter::once(&**varying).chain(after).collect();
                self.varying(&levels, *test_after, &body, exit, pos, ctx)?;
            }
        }
        self.switch(exit)?;
        self.op(Op::Unnest(1), pos)?;
        match after {
            Some(resume) => {
                self.jump(resume.block, pos)?;
                self.switch(resume.block)
            }
            None => Ok(()),
        }
    }

    /// PERFORM VARYING and its AFTER phrases as `vary` runs them, one loop per variable, the last
    /// varying fastest: an outer variable is augmented before the one inside it is set to its FROM
    /// value again. TEST BEFORE sets every variable first; TEST AFTER sets each inner one as its
    /// loop is entered.
    fn varying(&mut self, levels: &[&Varying], test_after: bool, body: &Body<'_>, exit: BlockId, pos: Pos, ctx: &Ctx) -> R<()> {
        let mut vary = Vec::with_capacity(levels.len());
        for v in levels {
            vary.push(self.vary_level(v, pos)?);
        }
        let n = vary.len() - 1;
        let tests: Vec<BlockId> = (0..=n).map(|_| self.new_block()).collect::<R<_>>()?;
        let steps: Vec<BlockId> = (0..=n).map(|_| self.new_block()).collect::<R<_>>()?;
        let run = self.new_block()?;
        if test_after {
            // Block k sets variable k + 1 and enters its loop; the last is the body.
            let tops: Vec<BlockId> = (0..n).map(|_| self.new_block()).collect::<R<_>>()?;
            let top = |k: usize| if k < n { tops[k] } else { run };
            self.op(vary[0].from.clone(), pos)?;
            self.jump(top(0), pos)?;
            for k in 0..n {
                self.switch(tops[k])?;
                self.op(vary[k + 1].from.clone(), pos)?;
                self.jump(top(k + 1), pos)?;
            }
            self.switch(run)?;
            self.run_body(body, tests[n], exit, None, pos, ctx)?;
            for (k, level) in vary.into_iter().enumerate() {
                self.switch(tests[k])?;
                let then = if k == 0 { exit } else { tests[k - 1] };
                self.branch(level.until, then, steps[k], pos)?;
                self.switch(steps[k])?;
                self.op(level.step, pos)?;
                self.jump(top(k), pos)?;
            }
        } else {
            for level in &vary {
                self.op(level.from.clone(), pos)?;
            }
            self.jump(tests[0], pos)?;
            let froms: Vec<Op> = vary.iter().map(|level| level.from.clone()).collect();
            for (k, level) in vary.into_iter().enumerate() {
                self.switch(tests[k])?;
                let then = if k == 0 { exit } else { steps[k - 1] };
                let inner = if k == n { run } else { tests[k + 1] };
                self.branch(level.until, then, inner, pos)?;
                self.switch(steps[k])?;
                self.op(level.step, pos)?;
                if k < n {
                    self.op(froms[k + 1].clone(), pos)?;
                }
                self.jump(tests[k], pos)?;
            }
            self.switch(run)?;
            self.run_body(body, steps[n], exit, None, pos, ctx)?;
        }
        Ok(())
    }

    /// One VARYING or AFTER phrase: FROM stored with MOVE rules, the BY step, and the UNTIL test.
    fn vary_level(&mut self, v: &Varying, pos: Pos) -> R<VaryLevel> {
        let var = self.place(&v.var, false)?;
        let kind = self.kind_of(var);
        if !matches!(kind, Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. } | Kind::Index) {
            return unsupported("a PERFORM VARYING variable that is not a fixed-point numeric item", v.var.pos);
        }
        let Expr::Operand(from) = &v.from else { return unsupported("PERFORM VARYING FROM an arithmetic expression", pos) };
        let from = self.operand(from, pos)?;
        let item = self.place_items[var as usize];
        let plan = self.move_plan(&Side { src: None, ..from.side }, kind, item)?;
        let from = Op::Move { from: from.operand, to: var, plan };
        let sum = Expr::Bin(Box::new(Expr::Operand(Operand::Ref(v.var.clone()))), BinOp::Add, Box::new(v.by.clone()));
        let dmax = scale(kind).max(self.dmax(&sum)?);
        let prepass = self.dmax_places(&v.by)?;
        let by = self.expr(&v.by, pos)?;
        let store = self.store_plan(kind, item)?;
        let step = Op::Step { var, by, plan: lir::StepPlan { dmax, store }, prepass };
        let until = self.test(&v.until, pos)?;
        Ok(VaryLevel { from, step, until })
    }

    /// One iteration: enter the range and return to `cont`, or run the inline body, where EXIT
    /// PERFORM takes `exit` and EXIT PERFORM CYCLE `cont`. Under DEBUG each entry sets the line register.
    fn run_body(&mut self, body: &Body<'_>, cont: BlockId, exit: BlockId, resume: Option<lir::Resume>, pos: Pos, ctx: &Ctx) -> R<()> {
        match body {
            Body::Range { range, .. } => {
                self.debug_line(pos)?;
                self.end(Terminator::PerformEnter { range: *range, ret: cont, resume }, pos)
            }
            Body::Inline(stmts) => {
                let mut inner = Ctx { pos, ..ctx.clone() };
                inner.loops.push(Inline { exit, cont });
                self.statements(stmts, &inner)?;
                self.jump(cont, pos)
            }
        }
    }
}

/// The name an unsupported statement is refused by.
fn statement_name(s: &Stmt) -> &'static str {
    match s {
        Stmt::Open { .. } => "OPEN",
        Stmt::Close { .. } => "CLOSE",
        Stmt::Read(_) => "READ",
        Stmt::Write { .. } => "WRITE",
        Stmt::Rewrite { .. } => "REWRITE",
        Stmt::Delete { .. } => "DELETE",
        Stmt::Start { .. } => "START",
        Stmt::Call(_) => "CALL",
        Stmt::Cancel { .. } => "CANCEL",
        Stmt::Set { .. } => "SET",
        Stmt::Accept { .. } => "ACCEPT",
        Stmt::String(_) => "STRING",
        Stmt::Unstring(_) => "UNSTRING",
        Stmt::Inspect(_) => "INSPECT",
        Stmt::Search(se) if se.all => "SEARCH ALL",
        Stmt::Search(_) => "SEARCH",
        Stmt::Sorting(so) => match &**so {
            Sorting::Sort(st) if st.merge => "MERGE",
            Sorting::Sort(_) => "SORT",
            Sorting::Release { .. } => "RELEASE",
            Sorting::Return { .. } => "RETURN",
        },
        Stmt::Exec(block) => match block.kind {
            ExecKind::Sql => "EXEC SQL",
            ExecKind::Cics => "EXEC CICS",
            ExecKind::Dli => "EXEC DLI",
            ExecKind::Other => "EXEC",
        },
        Stmt::GoToDepending { .. } => "GO TO DEPENDING ON",
        Stmt::Alter { .. } => "ALTER",
        Stmt::Entry { .. } => "ENTRY",
        Stmt::Report(_) => "Report Writer",
        Stmt::Invoke(_) => "INVOKE",
        Stmt::JsonGenerate(_) => "JSON GENERATE",
        _ => "this statement",
    }
}

fn stmt_pos(s: &Stmt) -> Option<Pos> {
    use syntax::report::ReportStmt;
    Some(match s {
        Stmt::Move { pos, .. }
        | Stmt::Compute { pos, .. }
        | Stmt::If { pos, .. }
        | Stmt::PerformInline { pos, .. }
        | Stmt::PerformProc { pos, .. }
        | Stmt::Evaluate { pos, .. }
        | Stmt::Display { pos, .. }
        | Stmt::Open { pos, .. }
        | Stmt::Close { pos, .. }
        | Stmt::Write { pos, .. }
        | Stmt::Rewrite { pos, .. }
        | Stmt::Delete { pos, .. }
        | Stmt::Start { pos, .. }
        | Stmt::Initialize { pos, .. }
        | Stmt::GoTo { pos, .. }
        | Stmt::Goback { pos }
        | Stmt::ExitProgram { pos }
        | Stmt::Cancel { pos, .. }
        | Stmt::Set { pos, .. }
        | Stmt::Accept { pos, .. }
        | Stmt::ExitMethod { pos }
        | Stmt::StopRun { pos }
        | Stmt::GoToDepending { pos, .. }
        | Stmt::Alter { pos, .. }
        | Stmt::Entry { pos, .. } => *pos,
        Stmt::Arith(a) => a.pos,
        Stmt::Read(r) => r.pos,
        Stmt::Call(c) => c.pos,
        Stmt::String(st) => st.pos,
        Stmt::Unstring(u) => u.pos,
        Stmt::Inspect(i) => i.pos,
        Stmt::Search(se) => se.pos,
        Stmt::Exec(block) => block.pos,
        Stmt::Invoke(i) => i.pos,
        Stmt::JsonGenerate(g) => g.pos,
        Stmt::Report(r) => match &**r {
            ReportStmt::Initiate { pos, .. } | ReportStmt::Generate { pos, .. } | ReportStmt::Terminate { pos, .. } | ReportStmt::Suppress { pos } => *pos,
        },
        Stmt::Sorting(so) => match &**so {
            Sorting::Sort(st) => st.pos,
            Sorting::Release { pos, .. } | Sorting::Return { pos, .. } => *pos,
        },
        Stmt::NextSentence | Stmt::SentenceEnd | Stmt::Continue | Stmt::Exit { .. } => return None,
    })
}
