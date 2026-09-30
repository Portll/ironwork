//! Control flow (lir.md §8): one entry block per paragraph, PERFORM as loops around `PerformEnter`
//! or an inline body, and each `Flow` the walker returns as an explicit terminator.

use super::cond::Test;
use super::data::{Side, scale};
use super::{Lower, LowerError, R, push, unsupported};
use rt::lir::{self, BlockId, DebugId, Ending, Op, RangeId, Terminator};
use rt::storage::Kind;
use syntax::Pos;
use syntax::ast::{BinOp, ExecKind, ExitKind, Expr, Loop, Object, Operand, ProcName, RelOp, SizeError, Sorting, Stmt, Subject, Target, When};

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
struct Ctx {
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
    Range(RangeId),
    Inline(&'a [Stmt]),
}

impl Lower<'_> {
    fn new_block(&mut self) -> R<BlockId> {
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

    fn op(&mut self, op: Op, pos: Pos) -> R<()> {
        let at = self.at(pos);
        let b = self.current()? as usize;
        self.blocks.open[b].ops.push(op);
        self.blocks.open[b].at.push(at);
        Ok(())
    }

    fn end(&mut self, end: Terminator, pos: Pos) -> R<()> {
        let at = self.at(pos);
        let b = self.current()? as usize;
        self.blocks.open[b].end = Some((end, at));
        self.blocks.current = None;
        Ok(())
    }

    /// Ends the open block, if there is one, with a jump to `to`.
    fn jump(&mut self, to: BlockId, pos: Pos) -> R<()> {
        match self.blocks.current {
            Some(_) => self.end(Terminator::Jump(to), pos),
            None => Ok(()),
        }
    }

    fn switch(&mut self, b: BlockId) -> R<()> {
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

    /// The PROCEDURE DIVISION: every PERFORM range first, since a paragraph's end and each GO TO
    /// depend on all of them, then each paragraph from its entry block.
    pub(super) fn procedure(&mut self) -> R<Vec<lir::Paragraph>> {
        let program = self.program;
        let n = program.paragraphs.len();
        if u32::try_from(n + 1).is_err() {
            return Err(LowerError::Exceeds("paragraphs", Pos::default()));
        }
        for p in &program.paragraphs {
            self.collect_ranges(&p.statements)?;
        }
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
                at: self.at(para.pos),
            });
        }
        Ok(paragraphs)
    }

    fn collect_ranges(&mut self, stmts: &[Stmt]) -> R<()> {
        for s in stmts {
            if let Stmt::PerformProc { from, thru, pos, .. } = s {
                self.range(from, thru.as_ref(), *pos)?;
            }
            for body in crate::oo::bodies(s) {
                self.collect_ranges(body)?;
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
        let key = (first as u32, last as u32);
        if let Some(&r) = self.range_ids.get(&key) {
            return Ok(r);
        }
        let r = push(&mut self.ranges, lir::Range { first: key.0, last: key.1, kind: lir::RangeKind::Perform }, "PERFORM ranges")?;
        self.range_ids.insert(key, r);
        Ok(r)
    }

    /// `run_sentences` over one paragraph; a separator period starts a block NEXT SENTENCE can
    /// reach.
    fn paragraph(&mut self, p: usize) -> R<()> {
        let para = &self.program.paragraphs[p];
        for (i, s) in para.statements.iter().enumerate() {
            let pos = stmt_pos(s).unwrap_or(para.pos);
            if *s == Stmt::SentenceEnd {
                let b = self.sentence(p, i + 1)?;
                self.jump(b, pos)?;
                self.switch(b)?;
                continue;
            }
            self.statement(s, &Ctx { para: p, top: i, pos, loops: Vec::new() })?;
        }
        if self.blocks.current.is_some() {
            let end = self.paragraph_end(p, p + 1);
            self.end(end, para.pos)?;
        }
        Ok(())
    }

    /// Leaving paragraph `p` for `next`: a plain jump when no range's last paragraph lies from `p`
    /// to `next` − 1, since no frame can complete there (lir.md §8.2).
    fn paragraph_end(&self, p: usize, next: usize) -> Terminator {
        let completes = self.ranges.iter().any(|r| p <= r.last as usize && (r.last as usize) < next);
        if next < self.entries.len() && !completes { Terminator::Jump(self.entries[next]) } else { Terminator::ParagraphEnd { next: next as u32 } }
    }

    fn statements(&mut self, stmts: &[Stmt], ctx: &Ctx) -> R<()> {
        for s in stmts {
            self.statement(s, ctx)?;
        }
        Ok(())
    }

    fn statement(&mut self, s: &Stmt, ctx: &Ctx) -> R<()> {
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
                self.perform(repeat, Body::Range(range), pos, &inner)?;
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
            Stmt::GoTo { target, .. } => {
                let Ok((t, _)) = crate::procedure(self.program, target) else { return unsupported("a GO TO the walker cannot resolve", pos) };
                self.unnest(ctx.loops.len(), pos)?;
                let (p, t32) = (ctx.para as u32, t as u32);
                let holds = |r: &lir::Range, q: u32| r.first <= q && q <= r.last;
                let end = if self.ranges.iter().all(|r| !holds(r, p) || holds(r, t32)) { Terminator::Jump(self.entries[t]) } else { Terminator::GoTo(t32) };
                self.end(end, pos)?;
            }
            Stmt::Goback { .. } | Stmt::ExitMethod { .. } => self.end(Terminator::End(Ending::Goback), pos)?,
            Stmt::StopRun { .. } => self.end(Terminator::End(Ending::StopRun), pos)?,
            Stmt::ExitProgram { .. } => {
                let next = self.new_block()?;
                self.end(Terminator::ExitProgram { next }, pos)?;
                self.switch(next)?;
            }
            Stmt::Continue | Stmt::SentenceEnd | Stmt::Exit(ExitKind::Plain) => {}
            Stmt::Exit(ExitKind::Paragraph) => self.leave(ctx.para + 1, ctx, pos)?,
            Stmt::Exit(ExitKind::Section) => self.leave(crate::section_end(self.program, ctx.para) + 1, ctx, pos)?,
            Stmt::Exit(ExitKind::Perform) => match ctx.loops.last() {
                Some(l) => self.end(Terminator::Jump(l.exit), pos)?,
                None => self.leave(ctx.para + 1, ctx, pos)?,
            },
            Stmt::Exit(ExitKind::PerformCycle) => match ctx.loops.last() {
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
    /// evaluated again at each comparison (lir.md §11, item 7).
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
    /// exit (lir.md §8.3). Each TIMES statement has a counter of its own.
    fn perform(&mut self, repeat: &Loop, body: Body<'_>, pos: Pos, ctx: &Ctx) -> R<()> {
        self.op(Op::Nest, pos)?;
        let exit = self.new_block()?;
        match repeat {
            Loop::Once => self.run_body(&body, exit, exit, pos, ctx)?,
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
                self.run_body(&body, head, exit, pos, ctx)?;
            }
            Loop::Until { cond, test_after } => {
                let until = self.test(cond, pos)?;
                let run = self.new_block()?;
                if *test_after {
                    let cont = self.new_block()?;
                    self.jump(run, pos)?;
                    self.switch(run)?;
                    self.run_body(&body, cont, exit, pos, ctx)?;
                    self.switch(cont)?;
                    self.branch(until, exit, run, pos)?;
                } else {
                    let head = self.new_block()?;
                    self.jump(head, pos)?;
                    self.switch(head)?;
                    self.branch(until, exit, run, pos)?;
                    self.switch(run)?;
                    self.run_body(&body, head, exit, pos, ctx)?;
                }
            }
            Loop::Varying { varying, test_after } => {
                let var = self.place(&varying.var, false)?;
                let kind = self.kind_of(var);
                if !matches!(kind, Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. } | Kind::Index) {
                    return unsupported("a PERFORM VARYING variable that is not a fixed-point numeric item", varying.var.pos);
                }
                let Expr::Operand(from) = &varying.from else { return unsupported("PERFORM VARYING FROM an arithmetic expression", pos) };
                let from = self.operand(from, pos)?;
                let item = self.place_items[var as usize];
                let plan = self.move_plan(&Side { src: None, ..from.side }, kind, item)?;
                self.op(Op::Move { from: from.operand, to: var, plan }, pos)?;
                let step = Expr::Bin(Box::new(Expr::Operand(Operand::Ref(varying.var.clone()))), BinOp::Add, Box::new(varying.by.clone()));
                let dmax = scale(kind).max(self.dmax(&step)?);
                let prepass = self.dmax_places(&varying.by)?;
                let by = self.expr(&varying.by, pos)?;
                let store = self.store_plan(kind, item)?;
                let step = Op::Step { var, by, plan: lir::StepPlan { dmax, store }, prepass };
                let until = self.test(&varying.until, pos)?;
                let (run, cont) = (self.new_block()?, self.new_block()?);
                if *test_after {
                    self.jump(run, pos)?;
                    self.switch(run)?;
                    self.run_body(&body, cont, exit, pos, ctx)?;
                    self.switch(cont)?;
                    let stepping = self.new_block()?;
                    self.branch(until, exit, stepping, pos)?;
                    self.switch(stepping)?;
                    self.op(step, pos)?;
                    self.jump(run, pos)?;
                } else {
                    let head = self.new_block()?;
                    self.jump(head, pos)?;
                    self.switch(head)?;
                    self.branch(until, exit, run, pos)?;
                    self.switch(run)?;
                    self.run_body(&body, cont, exit, pos, ctx)?;
                    self.switch(cont)?;
                    self.op(step, pos)?;
                    self.jump(head, pos)?;
                }
            }
        }
        self.switch(exit)?;
        self.op(Op::Unnest(1), pos)
    }

    /// One iteration: enter the range and return to `cont`, or run the inline body, where EXIT
    /// PERFORM takes `exit` and EXIT PERFORM CYCLE `cont`.
    fn run_body(&mut self, body: &Body<'_>, cont: BlockId, exit: BlockId, pos: Pos, ctx: &Ctx) -> R<()> {
        match body {
            Body::Range(range) => self.end(Terminator::PerformEnter { range: *range, ret: cont }, pos),
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
        Stmt::Report(_) => "Report Writer",
        Stmt::Invoke(_) => "INVOKE",
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
        | Stmt::StopRun { pos } => *pos,
        Stmt::Arith(a) => a.pos,
        Stmt::Read(r) => r.pos,
        Stmt::Call(c) => c.pos,
        Stmt::String(st) => st.pos,
        Stmt::Unstring(u) => u.pos,
        Stmt::Inspect(i) => i.pos,
        Stmt::Search(se) => se.pos,
        Stmt::Exec(block) => block.pos,
        Stmt::Invoke(i) => i.pos,
        Stmt::Report(r) => match &**r {
            ReportStmt::Initiate { pos, .. } | ReportStmt::Generate { pos, .. } | ReportStmt::Terminate { pos, .. } | ReportStmt::Suppress { pos } => *pos,
        },
        Stmt::Sorting(so) => match &**so {
            Sorting::Sort(st) => st.pos,
            Sorting::Release { pos, .. } | Sorting::Return { pos, .. } => *pos,
        },
        Stmt::NextSentence | Stmt::SentenceEnd | Stmt::Continue | Stmt::Exit(_) => return None,
    })
}
