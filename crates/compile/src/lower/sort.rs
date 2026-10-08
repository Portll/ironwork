//! SORT, MERGE, RELEASE and RETURN (lir.md §9.6), as machine/sort.rs runs them over `rt::sort`: the
//! SD, keys, collating sequence, USING and GIVING files and special registers it finds by name on
//! each execution resolved once, and each INPUT or OUTPUT PROCEDURE a range the op runs.

use super::flow::Ctx;
use super::{Lower, R, push, unsupported};
use crate::layout::Resolved;
use crate::sort::{collates, table_key};
use rt::lir::{FileSort, FromMove, Op, PlaceId, RangeId, RangeKind, ReleasePlan, ReturnPlan, SortIo, SortKey, SortKeys, SortPlan, TableSort, Terminator};
use syntax::Pos;
use syntax::ast::{self, Expr, Handlers, Literal, Operand, ProcName, Ref, SortStmt, Sorting};

fn register(name: &str, pos: Pos) -> Ref {
    Ref { name: name.into(), qualifiers: Vec::new(), subscripts: Vec::new(), refmod: None, pos }
}

impl Lower<'_> {
    /// The SD a SORT or MERGE names, or None for a table SORT, as the walker tells them apart.
    pub(super) fn sort_file_of(&self, st: &SortStmt) -> Option<usize> {
        let s = &st.subject;
        self.program.files.iter().position(|f| f.name == s.name).filter(|_| s.qualifiers.is_empty() && s.subscripts.is_empty())
    }

    /// The ranges of a file SORT's or MERGE's procedures.
    pub(super) fn collect_sort(&mut self, st: &SortStmt) -> R<()> {
        if self.sort_file_of(st).is_none() {
            return Ok(());
        }
        for io in [&st.input, &st.output].into_iter().flatten() {
            if let ast::SortIo::Procedure { from, thru } = io {
                self.sort_procedure(from, thru.as_ref(), st.pos)?;
            }
        }
        Ok(())
    }

    /// An INPUT or OUTPUT PROCEDURE, which the walker finds by name only when the sort reaches it.
    fn sort_procedure(&mut self, from: &ProcName, thru: Option<&ProcName>, pos: Pos) -> R<RangeId> {
        let program = self.program;
        let Ok((first, first_end)) = crate::procedure(program, from) else { return unsupported("a SORT or MERGE procedure the walker cannot find", pos) };
        let last = match thru {
            None => first_end,
            Some(t) => match crate::procedure(program, t) {
                Ok((_, last)) => last,
                Err(_) => return unsupported("a SORT or MERGE procedure the walker cannot find", pos),
            },
        };
        self.span_range((first, last), RangeKind::SortProcedure)
    }

    pub(super) fn sorting(&mut self, so: &Sorting, pos: Pos, ctx: &Ctx) -> R<()> {
        match so {
            Sorting::Sort(st) => {
                // A stable sort keeps equal keys in order with DUPLICATES or without it.
                let SortStmt { merge: _, subject: _, keys: _, duplicates: _, collating: _, input: _, output: _, pos: _ } = st;
                let plan = match self.sort_file_of(st) {
                    Some(sd) => SortPlan::File(self.file_sort(st, sd)?),
                    None => SortPlan::Table(self.table_sort(st)?),
                };
                let id = push(&mut self.services.sorts, plan, "SORT and MERGE statements")?;
                self.op(Op::Sort(id), pos)
            }
            Sorting::Release { record, from, pos } => {
                let plan = self.release_plan(record, from.as_ref(), *pos)?;
                let id = push(&mut self.services.releases, plan, "RELEASE statements")?;
                self.op(Op::Release(id), *pos)
            }
            Sorting::Return { file, into, at_end, pos } => {
                let plan = self.return_plan(file, into.as_ref(), *pos)?;
                let id = push(&mut self.services.returns, plan, "RETURN statements")?;
                self.op(Op::Return(id), *pos)?;
                self.at_end(at_end, *pos, ctx)
            }
        }
    }

    /// AT END and NOT AT END after RETURN, which returns Arm(0) at end and Arm(1) for a record.
    fn at_end(&mut self, at_end: &Handlers, pos: Pos, ctx: &Ctx) -> R<()> {
        let Handlers { on, not_on } = at_end;
        let (end, record, join) = (self.new_block()?, self.new_block()?, self.new_block()?);
        self.end(Terminator::Select(vec![end, record]), pos)?;
        self.switch(end)?;
        self.statements(on.as_deref().unwrap_or_default(), ctx)?;
        self.jump(join, pos)?;
        self.switch(record)?;
        self.statements(not_on.as_deref().unwrap_or_default(), ctx)?;
        self.jump(join, pos)?;
        self.switch(join)
    }

    /// SORT-RETURN or SORT-CONTROL, named where the statement is.
    fn sort_register(&mut self, name: &str, pos: Pos) -> R<PlaceId> {
        if !matches!(self.layout.resolve(name, &[], pos), Ok(Resolved::Item(_))) {
            return unsupported("a sort statement in a program without the sort special registers", pos);
        }
        self.place(&register(name, pos), false)
    }

    fn file_sort(&mut self, st: &SortStmt, sd: usize) -> R<FileSort> {
        let pos = st.pos;
        let sort_control = self.sort_register("SORT-CONTROL", pos)?;
        let sort_return = self.sort_register("SORT-RETURN", pos)?;
        let keys = self.file_keys(st, sd)?;
        let input = st.input.as_ref().map(|io| self.sort_io(io, pos)).transpose()?;
        let output = st.output.as_ref().map(|io| self.sort_io(io, pos)).transpose()?;
        let sd = u16::try_from(sd).map_err(|_| super::LowerError::Exceeds("files", pos))?;
        Ok(FileSort { sd, merge: st.merge, keys, input, output, sort_return, sort_control })
    }

    fn sort_io(&mut self, io: &ast::SortIo, pos: Pos) -> R<SortIo> {
        Ok(match io {
            ast::SortIo::Procedure { from, thru } => SortIo::Procedure(self.sort_procedure(from, thru.as_ref(), pos)?),
            ast::SortIo::Files(names) => {
                let mut files = Vec::with_capacity(names.len());
                for name in names {
                    match self.program.files.iter().position(|f| f.name == *name).map(u16::try_from) {
                        Some(Ok(k)) => files.push(k),
                        Some(Err(_)) => return Err(super::LowerError::Exceeds("files", pos)),
                        None => return unsupported("a USING or GIVING file the walker cannot find", pos),
                    }
                }
                SortIo::Files(files)
            }
        })
    }

    /// The collating sequence of the alphanumeric keys when it is not EBCDIC: the COLLATING
    /// SEQUENCE phrase's, else for a file SORT or MERGE the program's.
    fn key_collating(&self, st: &SortStmt, file: bool) -> R<Option<Box<[u8; 256]>>> {
        let named;
        let sequence = match &st.collating {
            Some(name) => match crate::collating::Sequence::named(&self.program.environment, name, self.page, self.c.options.quote) {
                Ok(s) => {
                    named = s;
                    &named
                }
                Err(_) => return unsupported("a COLLATING SEQUENCE the walker refuses when the SORT runs", st.pos),
            },
            None if file => &self.c.collating,
            None => return Ok(None),
        };
        Ok((!sequence.is_native()).then(|| Box::new(sequence.positions())))
    }

    /// `file_keys`: each key placed within the SD's records.
    fn file_keys(&mut self, st: &SortStmt, sd: usize) -> R<SortKeys> {
        let layout = self.layout;
        let area = layout.file_areas[sd].0;
        let collating = self.key_collating(st, true)?;
        let mut keys = Vec::with_capacity(st.keys.len());
        for (ascending, r) in &st.keys {
            let Ok(Resolved::Item(i)) = layout.resolve(&r.name, &r.qualifiers, r.pos) else { return unsupported("a SORT key that is not a data item", r.pos) };
            let item = &layout.items[i];
            let offset = if item.file == Some(sd as u16) { item.offset - area } else { layout.offset_in_record(i) };
            keys.push(SortKey { ascending: *ascending, offset, len: item.size, kind: item.kind, item: i as u32, collated: collates(item.kind) });
        }
        Ok(SortKeys { keys, collating })
    }

    /// `sort_table`: the count, then the first element, then the keys of its OCCURS or the
    /// statement's, which the walker finds within the element.
    fn table_sort(&mut self, st: &SortStmt) -> R<TableSort> {
        let pos = st.pos;
        let layout = self.layout;
        let Ok(Resolved::Item(t)) = layout.resolve(&st.subject.name, &st.subject.qualifiers, st.subject.pos) else {
            return unsupported("a table SORT of a name that is not a data item", pos);
        };
        let table = &layout.items[t];
        let count = self.occurs(t, pos)?;
        let mut first = Ref { refmod: None, ..st.subject.clone() };
        first.subscripts.push(Expr::Operand(Operand::Literal(Literal::Number("1".into()))));
        let first = self.place(&first, false)?;
        let named = if st.keys.is_empty() { &table.keys } else { &st.keys };
        let collating = self.key_collating(st, false)?;
        let mut keys = Vec::with_capacity(named.len());
        for (ascending, r) in named {
            let Some(k) = table_key(layout, t, &r.name) else { return unsupported("a table SORT key that is not within its element", r.pos) };
            let item = &layout.items[k];
            let offset = item.offset.saturating_sub(table.offset);
            keys.push(SortKey { ascending: *ascending, offset, len: item.size, kind: item.kind, item: k as u32, collated: collates(item.kind) });
        }
        let name = self.sym(&st.subject.name);
        Ok(TableSort { first, count, stride: table.size, keys: SortKeys { keys, collating }, name })
    }

    /// `release`: the record located (as a receiving item when FROM moves into it), then FROM's
    /// MOVE, and the record located again as it is released.
    fn release_plan(&mut self, record: &Ref, from: Option<&Operand>, pos: Pos) -> R<ReleasePlan> {
        let place = self.place(record, false)?;
        let file = self.place_items[place as usize].and_then(|i| self.layout.items[i].file);
        let sort_return = self.sort_register("SORT-RETURN", pos)?;
        let from = match from {
            None => None,
            Some(op) => {
                let to = self.place(record, true)?;
                let sender = self.operand(op, pos)?;
                let plan = self.move_plan(&sender.side, self.kind_of(to), self.place_items[to as usize])?;
                Some(FromMove { from: sender.operand, to, plan, check: self.move_check(sender.operand, to) })
            }
        };
        Ok(ReleasePlan { record: place, file, from, sort_return, name: self.sym(&record.name) })
    }

    fn return_plan(&mut self, file: &str, into: Option<&Ref>, pos: Pos) -> R<ReturnPlan> {
        let k = match self.program.files.iter().position(|f| f.name == file).map(u16::try_from) {
            Some(Ok(k)) => Some(k),
            Some(Err(_)) => return Err(super::LowerError::Exceeds("files", pos)),
            None => None,
        };
        let sort_return = self.sort_register("SORT-RETURN", pos)?;
        let into = match into {
            None => None,
            Some(r) => {
                let place = self.place(r, true)?;
                Some((place, self.bytes_into(place)?))
            }
        };
        Ok(ReturnPlan { file: k, into, sort_return, name: self.sym(file) })
    }
}
