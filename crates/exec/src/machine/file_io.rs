//! The file statements, whose semantics are `rt::fileio`: the files and keys they name, and the
//! phrase or EXCEPTION/ERROR procedure the status each returns selects.

use super::*;
use super::facts::advance;
use crate::files::{FileStatus, Format, KeySpan, Keying};
use rt::fileio::{self, Outcome, Read};
use rt::lir::{self, StartRel};

impl<'p> Machine<'p, '_, '_> {
    fn file_index(&self, name: &str, pos: Pos) -> R<usize> {
        self.program.files.iter().position(|f| f.name == name).ok_or_else(|| Abend::ironwork(format!("no file named {name}"), pos))
    }

    pub(super) fn set_status(&mut self, k: usize, status: impl Into<FileStatus>, pos: Pos) -> R<()> {
        let file = self.file_desc(k);
        fileio::set_status(self, &file, status.into(), pos)
    }

    /// Records an I/O status of file k in the mode it is open in.
    pub(super) fn io_status(&mut self, k: usize, status: impl Into<FileStatus>, message: String, pos: Pos) -> R<()> {
        let mode = self.unit.file_ref(self.me, k).as_ref().map(|f| f.mode);
        self.io_failure(k, status, mode, false, message, pos)
    }

    /// Records an I/O status of file k, open in `mode` or being opened in it. A failing status runs
    /// the file's EXCEPTION/ERROR procedure, once FILE STATUS holds it
    /// ([`numeric::assumptions::ERROR_DECLARATIVE_STATUSES`]); with none, and no FILE STATUS either,
    /// it ends the run.
    pub(super) fn io_failure(&mut self, k: usize, status: impl Into<FileStatus>, mode: Option<OpenMode>, open_or_close: bool, message: String, pos: Pos) -> R<()> {
        let status = status.into();
        self.set_status(k, status, pos)?;
        if status.covers('0') {
            return Ok(());
        }
        self.uses.failed = Some(k);
        if let Some(procedure) = self.error_declarative(k, mode) {
            return self.run_error_declarative(procedure, pos);
        }
        if self.global_declarative(k, mode, pos)? {
            return Ok(());
        }
        if self.program.files[k].status.is_none()
            && let Some(abend) = fileio::unhandled(status, self.file_desc(k).organization, open_or_close, &self.program.files[k].name, &self.program.id, message, pos)
        {
            return Err(abend);
        }
        Ok(())
    }

    /// Sets the file status and runs the phrase it selects: the ON phrase for the condition of
    /// `class` (AT END for 1x, INVALID KEY for 2x), the NOT phrase on success.
    fn conclude(&mut self, k: usize, status: FileStatus, handlers: &'p Handlers, class: char, verb: &str, pos: Pos) -> R<Flow> {
        let body = if status.covers('0') {
            handlers.not_on.as_deref()
        } else if status.covers(class) {
            handlers.on.as_deref()
        } else {
            None
        };
        if let Some(body) = body {
            self.set_status(k, status, pos)?;
            return self.run_block(body);
        }
        let program = self.program;
        let message = format!("{verb} {}: file status {}: {}", program.files[k].name, status.as_str(), status.meaning());
        self.io_status(k, status, message, pos)?;
        Ok(Flow::Next)
    }

    pub(super) fn area(&self, k: usize) -> (usize, usize) {
        let (offset, size) = self.layout.file_areas[k];
        let base = match self.layout.bound_areas[k] {
            Some(ordinal) => self.linkage[ordinal as usize].unwrap_or_default(),
            None => self.base,
        };
        (base + offset as usize, size as usize)
    }

    fn record_area(&self, k: usize) -> &[u8] {
        let (offset, size) = self.area(k);
        &self.unit.mem[offset..offset + size]
    }

    /// Where a data item in file k's record area lies within the record.
    fn record_span(&mut self, k: usize, r: &Ref, pos: Pos) -> R<KeySpan> {
        let loc = self.locate(r)?;
        let (start, size) = self.area(k);
        if loc.offset < start || loc.offset + loc.len > start + size {
            return Err(Abend::ironwork(format!("{} is not in a record of {}", r.name, self.program.files[k].name), pos));
        }
        Ok(KeySpan::new(loc.offset - start, loc.len))
    }

    /// File k's key `r`: its record span, or the pieces a split key joins.
    fn key_span(&mut self, k: usize, r: &Ref, pos: Pos) -> R<KeySpan> {
        let Some(items) = self.program.files[k].split_key(&r.name) else { return self.record_span(k, r, pos) };
        let mut pieces = Vec::with_capacity(items.len());
        for item in items {
            let span = self.record_span(k, item, pos)?;
            pieces.push((span.offset, span.len));
        }
        Ok(KeySpan::joined(pieces))
    }

    /// How file k finds its records by key. OPEN takes each key's place in the record and reads
    /// none of its bytes.
    pub(super) fn keying(&mut self, k: usize, pos: Pos) -> R<Keying> {
        let was = self.unit.writing(true);
        let keying = self.key_spans(k, pos);
        self.unit.writing(was);
        keying
    }

    fn key_spans(&mut self, k: usize, pos: Pos) -> R<Keying> {
        let program = self.program;
        let decl = &program.files[k];
        Ok(match decl.organization {
            Organization::Relative => Keying::Relative,
            Organization::Indexed => {
                let prime = decl.record_key.as_ref().ok_or_else(|| Abend::ironwork(format!("{} has no RECORD KEY", decl.name), pos))?;
                let prime = self.key_span(k, prime, pos)?;
                let mut alternates = Vec::new();
                for (r, duplicates) in &decl.alternate_keys {
                    alternates.push((self.key_span(k, r, pos)?, *duplicates));
                }
                Keying::Indexed { prime, alternates }
            }
            _ => Keying::Position,
        })
    }

    /// Which key of an indexed file a data item names (0 the prime key, then each alternate), and
    /// its value. With `partial` (START) it may be a leading part of the key.
    pub(super) fn key_named(&mut self, k: usize, keying: &Keying, r: &Ref, partial: bool, pos: Pos) -> R<(usize, Vec<u8>)> {
        let decl = &self.program.files[k];
        if decl.split_key(&r.name).is_some() {
            let names = decl.record_key.iter().chain(decl.alternate_keys.iter().map(|(a, _)| a));
            if let Some(which) = names.into_iter().position(|n| n.name == r.name)
                && let Some(key) = keying.keys().get(which)
            {
                return Ok((which, key.of(self.record_area(k))));
            }
        }
        let span = self.record_span(k, r, pos)?;
        let not_a_key = || Abend::ironwork(format!("{} is not a key of {}", r.name, self.program.files[k].name), pos);
        let Keying::Indexed { prime, alternates } = keying else { return Err(not_a_key()) };
        let fits = |key: &KeySpan| key.offset == span.offset && (span.len == key.len || partial && span.len < key.len);
        let which = std::iter::once(prime).chain(alternates.iter().map(|(s, _)| s)).position(fits).ok_or_else(not_a_key)?;
        Ok((which, span.of(self.record_area(k))))
    }

    /// The file a record belongs to, and the record as it stands once FROM has moved into it.
    fn record_of(&mut self, record: &Ref, from: Option<&Operand>, verb: &str, pos: Pos) -> R<(usize, Loc)> {
        let dest = if from.is_some() { self.locate_receiving(record)? } else { self.locate(record)? };
        let Some(k) = self.layout.items.get(dest.item).and_then(|i| i.file).map(|k| k as usize) else {
            return Err(Abend::ironwork(format!("{verb} {}: not a record of a file", record.name), pos));
        };
        let Some(op) = from else { return Ok((k, dest)) };
        let (val, src) = self.move_source(op, dest, pos)?;
        self.assign(dest, val, src, pos)?;
        Ok((k, self.locate(record)?))
    }

    /// How file k's records are held when its DD does not say.
    pub(super) fn described_format(&self, k: usize) -> Format {
        let decl = &self.program.files[k];
        match decl.organization {
            Organization::LineSequential => Format::Text,
            _ if compile::variable_records(decl, self.layout, k) => Format::Variable,
            _ => Format::Fixed,
        }
    }

    /// What a verb that returned no status for its phrases leaves to run: the file's error path,
    /// or END-OF-PAGE or NOT END-OF-PAGE.
    fn settle(&mut self, k: usize, outcome: Outcome, end_of_page: Option<&'p Handlers>, pos: Pos) -> R<Flow> {
        let phrase = match outcome {
            Outcome::Failed(f) => return self.io_failure(k, f.status, f.mode, f.open_or_close, f.message, pos).map(|()| Flow::Next),
            Outcome::Page { end_of_page: true } => end_of_page.and_then(|h| h.on.as_deref()),
            Outcome::Page { end_of_page: false } => end_of_page.and_then(|h| h.not_on.as_deref()),
            Outcome::Done | Outcome::Status { .. } => None,
        };
        match phrase {
            Some(body) => self.run_block(body),
            None => Ok(Flow::Next),
        }
    }

    pub(super) fn open_file(&mut self, mode: OpenMode, name: &str, pos: Pos) -> R<()> {
        let k = self.file_index(name, pos)?;
        let file = self.file_desc(k);
        let outcome = fileio::open(self, &file, mode, pos)?;
        self.settle(k, outcome, None, pos).map(drop)
    }

    pub(super) fn linage_value(&mut self, v: &LinageValue, pos: Pos) -> R<i64> {
        match v {
            LinageValue::Integer(n) => n.parse().map_err(|_| Abend::ironwork(format!("LINAGE {n} is too large"), pos)),
            LinageValue::Data(r) => self.integer(&Expr::Operand(Operand::Ref(r.clone())), pos),
        }
    }

    pub(super) fn close_file(&mut self, name: &str, pos: Pos) -> R<()> {
        self.close_file_with(name, None, pos)
    }

    pub(super) fn close_file_with(&mut self, name: &str, closing: Option<Closing>, pos: Pos) -> R<()> {
        let k = self.file_index(name, pos)?;
        let file = self.file_desc(k);
        let outcome = fileio::close(self, &file, closing, pos)?;
        self.settle(k, outcome, None, pos).map(drop)
    }

    pub(super) fn read_stmt(&mut self, r: &'p ReadStmt) -> R<Flow> {
        let k = self.file_index(&r.file, r.pos)?;
        let file = self.file_desc(k);
        let sequential = fileio::sequential(&file) || file.access == lir::Access::Dynamic && r.next;
        let read = Read { sequential, previous: r.previous, into: r.into.as_ref(), key: r.key.as_ref() };
        match fileio::read(self, &file, read, r.pos)? {
            Outcome::Status { status, at_end: true } => self.conclude(k, status, &r.at_end, '1', "READ", r.pos),
            Outcome::Status { status, at_end: false } => self.conclude(k, status, &r.invalid, '2', "READ", r.pos),
            outcome => self.settle(k, outcome, None, r.pos),
        }
    }

    pub(super) fn write_stmt(&mut self, record: &Ref, from: Option<&Operand>, advancing: Option<&Advancing>, invalid: &'p Handlers, end_of_page: &'p Handlers, pos: Pos) -> R<Flow> {
        let (k, loc) = self.record_of(record, from, "WRITE", pos)?;
        self.write_file(k, loc, advancing, invalid, Some(end_of_page), pos)
    }

    /// WRITE of the record at `loc` to file k.
    pub(super) fn write_record(&mut self, k: usize, loc: Loc, advancing: Option<&Advancing>, invalid: &'p Handlers, pos: Pos) -> R<Flow> {
        self.write_file(k, loc, advancing, invalid, None, pos)
    }

    fn write_file(&mut self, k: usize, loc: Loc, advancing: Option<&Advancing>, invalid: &'p Handlers, end_of_page: Option<&'p Handlers>, pos: Pos) -> R<Flow> {
        let file = self.file_desc(k);
        match fileio::write(self, &file, loc, advancing.map(advance), pos)? {
            Outcome::Status { status, .. } => self.conclude(k, status, invalid, '2', "WRITE", pos),
            outcome => self.settle(k, outcome, end_of_page, pos),
        }
    }

    pub(super) fn write_stream(&mut self, k: usize, loc: Loc, before: bool, space: lir::Spacing, pos: Pos) -> R<()> {
        let file = self.file_desc(k);
        let outcome = fileio::write_stream(self, &file, loc, before, space, pos)?;
        self.settle(k, outcome, None, pos).map(drop)
    }

    pub(super) fn rewrite_stmt(&mut self, record: &Ref, from: Option<&Operand>, invalid: &'p Handlers, pos: Pos) -> R<Flow> {
        let (k, loc) = self.record_of(record, from, "REWRITE", pos)?;
        let file = self.file_desc(k);
        let status = fileio::rewrite(self, &file, loc, pos)?;
        self.conclude(k, status, invalid, '2', "REWRITE", pos)
    }

    pub(super) fn delete_files(&mut self, names: &[String], pos: Pos) -> R<()> {
        for name in names {
            let k = self.file_index(name, pos)?;
            let file = self.file_desc(k);
            let outcome = fileio::delete_file(self, &file, pos)?;
            self.settle(k, outcome, None, pos)?;
        }
        Ok(())
    }

    pub(super) fn delete_stmt(&mut self, file: &str, invalid: &'p Handlers, pos: Pos) -> R<Flow> {
        let k = self.file_index(file, pos)?;
        let desc = self.file_desc(k);
        let status = fileio::delete(self, &desc, pos)?;
        self.conclude(k, status, invalid, '2', "DELETE", pos)
    }

    pub(super) fn start_stmt(&mut self, file: &str, key: Option<&(RelOp, Ref)>, invalid: &'p Handlers, pos: Pos) -> R<Flow> {
        let k = self.file_index(file, pos)?;
        let rel = match key.map(|(op, _)| *op) {
            None | Some(RelOp::Eq) => StartRel::Equal,
            Some(RelOp::Gt) => StartRel::Greater,
            Some(RelOp::Ge) => StartRel::NotLess,
            Some(RelOp::Lt) => StartRel::Less,
            Some(RelOp::Le) => StartRel::NotGreater,
            Some(_) => return Err(Abend::ironwork("START KEY takes =, >, NOT < or >=", pos)),
        };
        let desc = self.file_desc(k);
        let status = fileio::start(self, &desc, rel, key.map(|(_, r)| r), pos)?;
        self.conclude(k, status, invalid, '2', "START", pos)
    }
}
