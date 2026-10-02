//! Files (lir.md §9.4): each file's declaration from its SELECT and FD, and each file statement as
//! `file_io` runs it, with what it finds by name on every execution resolved here.

use super::data::{Side, Value};
use super::flow::Ctx;
use super::{Lower, R, is_static, push, unsupported};
use crate::layout::Resolved;
use crate::printer::{self, Space};
use rt::files::Format;
use rt::lir::{self, Advance, FileDesc, FileOp, FileVerb, FromMove, IndexKeys, Op, Phrase, RecordSpan, RelativeKey, Spacing, StartKey, StartRel, Terminator};
use syntax::Pos;
use syntax::ast::{Access, Advancing, Expr, FileDecl, Handlers, LinageValue, Operand, Organization, Ref, RelOp, Stmt};

impl Lower<'_> {
    /// Every file's declaration, in the program's order, which file ops index.
    pub(super) fn files(&mut self) -> R<Vec<FileDesc>> {
        let program = self.program;
        let mut out = Vec::with_capacity(program.files.len());
        for (k, f) in program.files.iter().enumerate() {
            out.push(self.file_desc(k, f)?);
        }
        Ok(out)
    }

    fn file_desc(&mut self, k: usize, f: &FileDecl) -> R<FileDesc> {
        let status = match &f.status {
            None => None,
            Some(r) => {
                let place = self.place(r, false)?;
                Some((place, self.bytes_into(place)?))
            }
        };
        let keys = match (f.organization, &f.record_key) {
            (Organization::Indexed, Some(prime)) => {
                let prime = self.record_span(k, prime)?;
                let mut alternates = Vec::with_capacity(f.alternate_keys.len());
                for (r, duplicates) in &f.alternate_keys {
                    alternates.push((self.record_span(k, r)?, *duplicates));
                }
                Some(IndexKeys { prime, alternates })
            }
            (Organization::Indexed, None) => return unsupported("an indexed file without a RECORD KEY", f.pos),
            _ => None,
        };
        if f.record_depending.is_some() {
            return unsupported("RECORD IS VARYING DEPENDING ON", f.pos);
        }
        let relative = match &f.relative_key {
            None => None,
            Some(r) => {
                let place = self.place(r, false)?;
                let value = self.int_expr(&Expr::Operand(Operand::Ref(r.clone())), r.pos)?;
                let kind = self.kind_of(place);
                let store = self.store_plan(kind, self.place_items[place as usize])?;
                Some(RelativeKey { place, value, store, digits: kind.digits_scale().map(|(d, s)| d.saturating_sub(s)) })
            }
        };
        let counter = match self.layout.linage_counters.get(k).copied().flatten() {
            None => None,
            Some(i) => {
                let item = &self.layout.items[i];
                let r = Ref { name: item.name.clone().unwrap_or_else(|| "LINAGE-COUNTER".into()), qualifiers: Vec::new(), subscripts: Vec::new(), refmod: None, pos: item.pos };
                let place = self.item_place(i, &r, false)?;
                if !is_static(&self.places[place as usize]) || self.places[place as usize].base != lir::Base::Program {
                    return unsupported("a LINAGE-COUNTER outside WORKING-STORAGE", f.pos);
                }
                Some((place, self.store_plan(item.kind, Some(i))?))
            }
        };
        let linage = match (&f.linage, counter) {
            (None, None) => None,
            (None, Some(_)) => return unsupported("a LINAGE-COUNTER without LINAGE", f.pos),
            (Some(l), counter) => {
                let value = |lower: &mut Self, v: &Option<LinageValue>| v.as_ref().map(|v| lower.linage_value(v, f.pos)).transpose();
                Some(lir::Linage {
                    lines: self.linage_value(&l.lines, f.pos)?,
                    footing: value(self, &l.footing)?,
                    top: value(self, &l.top)?,
                    bottom: value(self, &l.bottom)?,
                    counter,
                })
            }
        };
        Ok(FileDesc {
            name: self.sym(&f.name),
            assign: self.sym(&f.assign),
            organization: match f.organization {
                Organization::Sequential => lir::Organization::Sequential,
                Organization::LineSequential => lir::Organization::LineSequential,
                Organization::Indexed => lir::Organization::Indexed,
                Organization::Relative => lir::Organization::Relative,
            },
            access: match f.access {
                Access::Sequential => lir::Access::Sequential,
                Access::Random => lir::Access::Random,
                Access::Dynamic => lir::Access::Dynamic,
            },
            optional: f.optional,
            format: match f.organization {
                Organization::LineSequential => Format::Text,
                _ if compile::variable_records(f, self.layout, k) => Format::Variable,
                _ => Format::Fixed,
            },
            read_lengths: compile::read_lengths(f, self.layout, k, self.c.options.vlr),
            fixed: !compile::variable_records(f, self.layout, k),
            record_min: f.record_min,
            status,
            keys,
            relative,
            linage,
            carriage: self.c.carriage.get(k).copied().flatten().map(|c| lir::Carriage { machine: c.machine, reserved: c.reserved }),
            sort: f.sort,
            error: self.c.declaratives.files.get(k).copied().flatten().map(|s| self.span_range(s, lir::RangeKind::UseProcedure)).transpose()?,
        })
    }

    /// `assign` of a status or a record's bytes, which reach it as alphanumeric bytes.
    pub(super) fn bytes_into(&mut self, place: lir::PlaceId) -> R<lir::MovePlan> {
        self.move_plan(&Side { value: Value::Bytes, src: None, digits: 0 }, self.kind_of(place), self.place_items[place as usize])
    }

    fn linage_value(&mut self, v: &LinageValue, pos: Pos) -> R<lir::IntExpr> {
        match v {
            LinageValue::Integer(n) => n.parse().map(lir::IntExpr::Const).or_else(|_| unsupported("a LINAGE integer past 64 bits", pos)),
            LinageValue::Data(r) => self.int_expr(&Expr::Operand(Operand::Ref(r.clone())), r.pos),
        }
    }

    /// `record_span`: where a data item of file k's record area lies within the record.
    fn record_span(&mut self, k: usize, r: &Ref) -> R<RecordSpan> {
        let place = self.place(r, false)?;
        let (start, size) = self.layout.file_areas[k];
        let q = &self.places[place as usize];
        if !is_static(q) || q.base != lir::Base::Program || q.offset < start || q.offset + q.len > start + size {
            return unsupported("a file key that is not a data item of its file's record area", r.pos);
        }
        Ok(RecordSpan { offset: q.offset - start, len: q.len })
    }

    /// `key_named`: which key of indexed file k a data item names, 0 for the prime key, and its
    /// span; with `partial` (START) it may be a leading part of the key.
    fn key_of_reference(&mut self, k: usize, keys: &IndexKeys, r: &Ref, partial: bool) -> R<(u8, RecordSpan)> {
        let span = self.record_span(k, r)?;
        let fits = |key: &RecordSpan| key.offset == span.offset && (span.len == key.len || partial && span.len < key.len);
        let which = std::iter::once(&keys.prime).chain(keys.alternates.iter().map(|(s, _)| s)).position(fits);
        match which.map(u8::try_from) {
            Some(Ok(which)) => Ok((which, span)),
            _ => unsupported("a READ or START KEY that is not a key of its file", r.pos),
        }
    }

    /// The file a statement names.
    fn file_index(&self, name: &str, pos: Pos) -> R<u16> {
        match self.program.files.iter().position(|f| f.name == name).map(u16::try_from) {
            Some(Ok(k)) => Ok(k),
            _ => unsupported("a file statement naming no file", pos),
        }
    }

    /// The file a record belongs to, the record as WRITE or REWRITE writes it, and FROM's move
    /// into it as a receiving item, located first (`record_of`).
    fn record_of(&mut self, record: &Ref, from: Option<&Operand>, pos: Pos) -> R<(u16, lir::PlaceId, Option<FromMove>)> {
        let file = match self.layout.resolve(&record.name, &record.qualifiers, record.pos) {
            Ok(Resolved::Item(i)) => self.layout.items[i].file,
            _ => None,
        };
        let Some(file) = file else { return unsupported("a WRITE or REWRITE of an item that is not a file's record", pos) };
        let from = match from {
            None => None,
            Some(op) => {
                let to = self.place(record, true)?;
                let sender = self.operand(op, pos)?;
                let plan = self.move_plan(&sender.side, self.kind_of(to), self.place_items[to as usize])?;
                Some(FromMove { from: sender.operand, to, plan, check: self.move_check(sender.operand, to) })
            }
        };
        Ok((file, self.place(record, false)?, from))
    }

    /// OPEN, CLOSE, READ, WRITE, REWRITE, DELETE and START, one op per file named, each followed
    /// by its phrases.
    pub(super) fn file_statement(&mut self, s: &Stmt, pos: Pos, ctx: &Ctx) -> R<()> {
        let program = self.program;
        match s {
            Stmt::Open { files, .. } => {
                for (mode, name) in files {
                    let file = self.file_index(name, pos)?;
                    self.file_op(FileOp { file, verb: FileVerb::Open(*mode), phrase: None, end_of_page: None }, [None, None, None, None], pos, ctx)?;
                }
            }
            Stmt::Close { files, .. } => {
                for (name, closing) in files {
                    let file = self.file_index(name, pos)?;
                    let verb = closing.map_or(FileVerb::Close, FileVerb::CloseWith);
                    self.file_op(FileOp { file, verb, phrase: None, end_of_page: None }, [None, None, None, None], pos, ctx)?;
                }
            }
            Stmt::Read(r) => {
                let file = self.file_index(&r.file, pos)?;
                let decl = &program.files[file as usize];
                // Only a file held in memory reads by key; any other takes AT END (`read_stream`).
                let sequential = decl.access == Access::Sequential
                    || decl.organization == Organization::Sequential
                    || decl.access == Access::Dynamic && r.next
                    || decl.organization == Organization::LineSequential;
                let handlers = if sequential { &r.at_end } else { &r.invalid };
                let into = match &r.into {
                    None => None,
                    Some(t) => {
                        let place = self.place(t, true)?;
                        Some((place, self.bytes_into(place)?))
                    }
                };
                let key = match (&r.key, decl.organization) {
                    (Some(key), Organization::Indexed) if !sequential => {
                        let keys = self.index_keys(file, pos)?;
                        self.key_of_reference(file as usize, &keys, key, false)?.0
                    }
                    _ => 0,
                };
                let verb = FileVerb::Read { sequential, previous: r.previous, into, key };
                self.file_op(FileOp { file, verb, phrase: phrase(handlers), end_of_page: None }, bodies(handlers, None), pos, ctx)?;
            }
            Stmt::Write { record, from, advancing, invalid, end_of_page, .. } => {
                let (file, record, from) = self.record_of(record, from.as_ref(), pos)?;
                let advancing = match advancing {
                    None => None,
                    Some(a) => Some(self.advance(a, file, pos)?),
                };
                let verb = FileVerb::Write { record, from, advancing };
                let op = FileOp { file, verb, phrase: phrase(invalid), end_of_page: phrase(end_of_page) };
                self.file_op(op, bodies(invalid, Some(end_of_page)), pos, ctx)?;
            }
            Stmt::Rewrite { record, from, invalid, .. } => {
                let (file, record, from) = self.record_of(record, from.as_ref(), pos)?;
                let op = FileOp { file, verb: FileVerb::Rewrite { record, from }, phrase: phrase(invalid), end_of_page: None };
                self.file_op(op, bodies(invalid, None), pos, ctx)?;
            }
            Stmt::Delete { file, invalid, .. } => {
                let file = self.file_index(file, pos)?;
                self.file_op(FileOp { file, verb: FileVerb::Delete, phrase: phrase(invalid), end_of_page: None }, bodies(invalid, None), pos, ctx)?;
            }
            Stmt::Start { file, key, invalid, .. } => {
                let file = self.file_index(file, pos)?;
                let rel = match key.as_ref().map(|(op, _)| *op) {
                    None | Some(RelOp::Eq) => StartRel::Equal,
                    Some(RelOp::Gt) => StartRel::Greater,
                    Some(RelOp::Ge) => StartRel::NotLess,
                    Some(_) => {
                        let abend = self.ironwork("START KEY takes =, >, NOT < or >=")?;
                        return self.end(Terminator::Abend(abend), pos);
                    }
                };
                let key = match (program.files[file as usize].organization, key) {
                    (Organization::Indexed, None) => StartKey::Prime,
                    (Organization::Indexed, Some((_, r))) => {
                        let keys = self.index_keys(file, pos)?;
                        let (key, span) = self.key_of_reference(file as usize, &keys, r, true)?;
                        StartKey::Named { key, span }
                    }
                    (Organization::Relative, None) => StartKey::RelativeKey,
                    (Organization::Relative, Some((_, r))) => StartKey::Relative(self.int_expr(&Expr::Operand(Operand::Ref(r.clone())), pos)?),
                    _ => return unsupported("START on a file that is neither indexed nor relative", pos),
                };
                let op = FileOp { file, verb: FileVerb::Start { rel, key }, phrase: phrase(invalid), end_of_page: None };
                self.file_op(op, bodies(invalid, None), pos, ctx)?;
            }
            _ => return Err(super::LowerError::Invalid("a file statement that is not one".into())),
        }
        Ok(())
    }

    fn index_keys(&self, file: u16, pos: Pos) -> R<IndexKeys> {
        match self.services.files.get(file as usize).and_then(|f| f.keys.clone()) {
            Some(keys) => Ok(keys),
            None => unsupported("a key of an indexed file without a RECORD KEY", pos),
        }
    }

    /// WRITE's ADVANCING phrase, a mnemonic-name as the movement its environment-name gives.
    fn advance(&mut self, a: &Advancing, file: u16, pos: Pos) -> R<Advance> {
        Ok(match a {
            Advancing::Lines { before, count } => Advance::Lines { before: *before, count: self.int_expr(count, pos)? },
            Advancing::Page { before } => Advance::Page { before: *before },
            Advancing::Mnemonic { before, environment, .. } => {
                let space = match printer::mnemonic_space(environment) {
                    _ if self.program.files[file as usize].linage.is_some() => return unsupported("ADVANCING a mnemonic-name on a file with LINAGE", pos),
                    Some(Space::Lines(n)) => Spacing::Lines(n),
                    Some(Space::Channel(c)) => Spacing::Channel(c),
                    Some(Space::PageMode) => Spacing::PageMode,
                    None => return unsupported("ADVANCING a mnemonic-name that is not a printer channel", pos),
                };
                Advance::Mnemonic { before: *before, space }
            }
        })
    }

    /// The op, then a `Select` of its arms: none, ON, NOT ON, END-OF-PAGE and NOT END-OF-PAGE, an
    /// arm whose phrase is not written going on as none does.
    fn file_op(&mut self, op: FileOp, bodies: [Option<&[Stmt]>; 4], pos: Pos, ctx: &Ctx) -> R<()> {
        let arms = op.arms();
        let id = push(&mut self.services.file_ops, op, "file statements")?;
        self.op(Op::File(id), pos)?;
        if arms == 0 {
            return Ok(());
        }
        let join = self.new_block()?;
        let mut targets = vec![join];
        let mut runs = Vec::new();
        for body in bodies.into_iter().take(arms - 1) {
            match body {
                Some(stmts) => {
                    let b = self.new_block()?;
                    targets.push(b);
                    runs.push((b, stmts));
                }
                None => targets.push(join),
            }
        }
        self.end(Terminator::Select(targets), pos)?;
        for (b, stmts) in runs {
            self.switch(b)?;
            self.statements(stmts, ctx)?;
            self.jump(join, pos)?;
        }
        self.switch(join)
    }
}

fn phrase(h: &Handlers) -> Option<Phrase> {
    (h.on.is_some() || h.not_on.is_some()).then_some(Phrase { on: h.on.is_some(), not_on: h.not_on.is_some() })
}

fn bodies<'s>(h: &'s Handlers, end_of_page: Option<&'s Handlers>) -> [Option<&'s [Stmt]>; 4] {
    [h.on.as_deref(), h.not_on.as_deref(), end_of_page.and_then(|e| e.on.as_deref()), end_of_page.and_then(|e| e.not_on.as_deref())]
}
