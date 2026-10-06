//! SORT, MERGE, RELEASE and RETURN, whose semantics are `rt::sort`: the files, keys, special
//! registers and procedures they name, and the phrases RETURN runs.

use super::facts::Int;
use super::*;
use crate::files::FileStatus;
use rt::lir::{FileSort, SortIo as Io};
pub(super) use rt::sort::Active;
use rt::sort::{self, Collating, ItemKey, Procedure, SortFile, SortHost};
use std::io::Write;
use std::rc::Rc;

static NO_HANDLERS: Handlers = Handlers { on: None, not_on: None };

/// A sort special register, named where the statement that reads or sets it is.
#[derive(Clone, Copy)]
pub(super) struct Register(&'static str, Pos);

impl<'p> Machine<'p, '_, '_> {
    pub(super) fn sorting(&mut self, s: &'p Sorting) -> R<Flow> {
        match s {
            Sorting::Sort(st) => match self.program.files.iter().position(|f| f.name == st.subject.name) {
                Some(sd) if st.subject.qualifiers.is_empty() && st.subject.subscripts.is_empty() => self.sort_file(st, sd),
                _ => self.sort_table(st).map(|()| Flow::Next),
            },
            Sorting::Release { record, from, pos } => self.release(record, from.as_ref(), *pos).map(|()| Flow::Next),
            Sorting::Return { file, into, at_end, pos } => self.return_record(file, into.as_ref(), at_end, *pos),
        }
    }

    fn register(&self, name: &str, pos: Pos) -> Ref {
        Ref { name: name.into(), qualifiers: Vec::new(), subscripts: Vec::new(), refmod: None, pos }
    }

    /// The keys of SD `sd`, placed within its records.
    fn file_keys(&mut self, st: &SortStmt, sd: usize, pos: Pos) -> R<Vec<ItemKey>> {
        let area = self.layout.file_areas[sd].0 as usize;
        let collating = self.key_collating(st, true)?;
        let mut out = Vec::new();
        for (ascending, r) in &st.keys {
            let Resolved::Item(i) = self.resolve(r)? else {
                return Err(Abend::ironwork(format!("{} is not a data item", r.name), pos));
            };
            let item = &self.layout.items[i];
            let offset = (item.offset as usize).checked_sub(area).ok_or_else(|| Abend::ironwork(format!("{} is not in the sort file's records", r.name), pos))?;
            let collating = if crate::sort::collates(item.kind) { collating.clone() } else { Collating::Ebcdic };
            out.push(ItemKey { ascending: *ascending, offset, len: item.size as usize, kind: item.kind, item: i, collating });
        }
        Ok(out)
    }

    /// The collating sequence of the alphanumeric keys: the COLLATING SEQUENCE phrase's, else a
    /// file SORT's or MERGE's PROGRAM COLLATING SEQUENCE (SC27-8713-03, p. 123); see
    /// TABLE_SORT_COLLATION in numeric::assumptions for a table SORT.
    fn key_collating(&self, st: &SortStmt, file: bool) -> R<Collating> {
        let named;
        let sequence = match &st.collating {
            Some(name) => {
                named = crate::collating::Sequence::named(&self.program.environment, name, self.page, self.options.quote)
                    .map_err(|(_, m)| Abend::ironwork(format!("COLLATING SEQUENCE {name}: {m}"), st.pos))?;
                &named
            }
            None if file => self.collating,
            None => return Ok(Collating::Ebcdic),
        };
        Ok(if sequence.is_native() { Collating::Ebcdic } else { Collating::Positions(Rc::new(sequence.positions())) })
    }

    fn fixed_length(&self, k: usize) -> bool {
        !compile::variable_records(&self.program.files[k], self.layout, k)
    }

    /// Runs `op`; true when a statement on file k failed in it.
    fn fails(&mut self, k: usize, op: impl FnOnce(&mut Self) -> R<()>) -> R<bool> {
        self.uses.failed = None;
        op(self)?;
        Ok(self.uses.failed == Some(k))
    }

    /// Paragraphs `start` to `end` as a procedure's range: a GO TO out of it carries on where it
    /// went, and the procedure ends when control passes the end of paragraph `end`.
    fn procedure_range(&mut self, start: usize, end: usize, arrival: declaratives::Arrival) -> R<Flow> {
        self.uses.arrival = arrival;
        self.perform_range(start, end, Some((0, self.program.paragraphs.len() - 1)), None)
    }

    fn sort_file(&mut self, st: &'p SortStmt, sd: usize) -> R<Flow> {
        let io = |io: &'p Option<SortIo>| {
            io.as_ref().map(|io| match io {
                SortIo::Files(names) => Io::Files(names.iter().map(String::as_str).collect()),
                SortIo::Procedure { from, thru } => Io::Procedure((from, thru.as_ref())),
            })
        };
        let plan = FileSort {
            sd: sd as u16,
            merge: st.merge,
            keys: (st, sd),
            input: io(&st.input),
            output: io(&st.output),
            sort_return: Register("SORT-RETURN", st.pos),
            sort_control: Register("SORT-CONTROL", st.pos),
        };
        Ok(match sort::sort(self, &plan, st.pos)? {
            Some(e) => Flow::End(e),
            None => Flow::Next,
        })
    }

    fn release(&mut self, record: &Ref, from: Option<&Operand>, pos: Pos) -> R<()> {
        let loc = if from.is_some() { self.locate_receiving(record)? } else { self.locate(record)? };
        let file = self.layout.items.get(loc.item).and_then(|i| i.file).map(usize::from);
        sort::release_ready(self, file, Register("SORT-RETURN", pos), &record.name, pos)?;
        let loc = match from {
            Some(op) => {
                let (val, src) = self.move_source(op, loc, pos)?;
                self.assign(loc, val, src, pos)?;
                self.locate(record)?
            }
            None => loc,
        };
        sort::release(self, loc, &record.name, pos)
    }

    fn return_record(&mut self, file: &str, into: Option<&'p Ref>, at_end: &'p Handlers, pos: Pos) -> R<Flow> {
        let k = self.program.files.iter().position(|f| f.name == file);
        let phrase = match sort::return_record(self, k, into, Register("SORT-RETURN", pos), file, pos)? {
            false => &at_end.on,
            true => &at_end.not_on,
        };
        match phrase {
            Some(body) => self.run_block(body),
            None => Ok(Flow::Next),
        }
    }

    /// A table SORT: the elements are reordered in place by their keys, or by the KEY phrase of the
    /// table's OCCURS when the statement gives none.
    fn sort_table(&mut self, st: &'p SortStmt) -> R<()> {
        let pos = st.pos;
        let Resolved::Item(t) = self.resolve(&st.subject)? else {
            return Err(Abend::ironwork(format!("SORT {}: not a table", st.subject.name), pos));
        };
        let layout = self.layout;
        let table = &layout.items[t];
        let count = self.occurrences(t, pos)? as usize;
        let mut first = Ref { refmod: None, ..st.subject.clone() };
        first.subscripts.push(Expr::Operand(Operand::Literal(Literal::Number("1".into()))));
        let base = self.locate(&first)?.offset;
        let keys = |m: &mut Self| m.table_keys(st, t);
        sort::sort_table::<&Ref, _>(self, base, count, table.size as usize, keys, &st.subject.name, pos)
    }

    /// The keys of table item `t`'s elements.
    fn table_keys(&mut self, st: &SortStmt, t: usize) -> R<Vec<ItemKey>> {
        let pos = st.pos;
        let layout = self.layout;
        let table = &layout.items[t];
        let named = if st.keys.is_empty() { &table.keys } else { &st.keys };
        let collating = self.key_collating(st, false)?;
        let mut keys = Vec::new();
        for (ascending, r) in named {
            let k = crate::sort::table_key(layout, t, &r.name).ok_or_else(|| Abend::ironwork(format!("{} is not a key of {}", r.name, st.subject.name), pos))?;
            let item = &layout.items[k];
            let collating = if crate::sort::collates(item.kind) { collating.clone() } else { Collating::Ebcdic };
            keys.push(ItemKey { ascending: *ascending, offset: item.offset.saturating_sub(table.offset) as usize, len: item.size as usize, kind: item.kind, item: k, collating });
        }
        Ok(keys)
    }
}

impl<'p> SortHost<'p, &'p Ref, Int<'p>> for Machine<'p, '_, '_> {
    type Register = Register;
    type Procedure = (&'p ProcName, Option<&'p ProcName>);
    type File = &'p str;
    type Keys = (&'p SortStmt, usize);

    fn locate_register(&mut self, Register(name, pos): Register) -> R<Loc> {
        let r = self.register(name, pos);
        self.locate(&r)
    }

    fn register_value(&mut self, Register(name, at): Register, pos: Pos) -> R<i64> {
        let r = self.register(name, at);
        self.integer(&Expr::Operand(Operand::Ref(r)), pos)
    }

    fn file_index(&self, name: &&'p str, pos: Pos) -> R<usize> {
        self.program.files.iter().position(|f| f.name == *name).ok_or_else(|| Abend::ironwork(format!("no file named {name}"), pos))
    }

    fn keys(&mut self, &(st, sd): &(&'p SortStmt, usize), pos: Pos) -> R<Vec<ItemKey>> {
        self.file_keys(st, sd, pos)
    }

    fn sort_file(&self, k: usize) -> SortFile<'p, &'p Ref, Int<'p>> {
        let program = self.program;
        let decl = &program.files[k];
        SortFile {
            file: self.file_desc(k),
            fixed: self.fixed_length(k),
            status_name: decl.status.as_ref().map(|r| r.name.as_str()),
            relative_name: decl.relative_key.as_ref().map(|r| r.name.as_str()),
        }
    }

    fn active(&mut self) -> &mut Option<Active> {
        &mut self.sort
    }

    fn open(&mut self, k: usize, mode: OpenMode, pos: Pos) -> R<()> {
        let program = self.program;
        self.open_file(mode, &program.files[k].name, pos)
    }

    fn close(&mut self, k: usize, pos: Pos) -> R<bool> {
        let program = self.program;
        self.fails(k, |m| m.close_file(&program.files[k].name, pos))
    }

    fn put(&mut self, k: usize, loc: Loc, pos: Pos) -> R<bool> {
        self.fails(k, |m| m.write_record(k, loc, None, &NO_HANDLERS, pos).map(drop))
    }

    fn fail(&mut self, k: usize, status: FileStatus, mode: Option<OpenMode>, open_or_close: bool, message: String, pos: Pos) -> R<()> {
        self.io_failure(k, status, mode, open_or_close, message, pos)
    }

    fn has_error_procedure(&self, k: usize, mode: OpenMode) -> bool {
        self.error_declarative(k, Some(mode)).is_some()
    }

    fn run_procedure(&mut self, (from, thru): (&'p ProcName, Option<&'p ProcName>), kind: Procedure, pos: Pos) -> R<Option<Ending>> {
        let (start, first_end) = self.procedure(from, pos)?;
        let end = match thru {
            Some(t) => self.procedure(t, pos)?.1,
            None => first_end,
        };
        let nested = self.unit.enter(pos);
        let flow = nested.and_then(|()| {
            let flow = self.procedure_range(start, end, declaratives::Arrival::Sort(kind.name()));
            self.unit.depth -= 1;
            flow
        });
        Ok(match flow? {
            Flow::End(e) => Some(e),
            _ => None,
        })
    }

    fn err(&mut self) -> &mut dyn Write {
        &mut *self.unit.err
    }
}

