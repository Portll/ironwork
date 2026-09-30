//! The file statements. A sequential file streams through its DD; an indexed or relative file, or
//! a sequential file opened I-O, is held in memory ([`files::Keyed`]).

use super::*;
use crate::files::{self, FileStatus, Format, KeySpan, Keyed, Keying, Record};
use crate::unit::Event;
use crate::printer::{self, Space};

impl<'p> Machine<'p, '_, '_> {
    fn file_index(&self, name: &str, pos: Pos) -> R<usize> {
        self.program.files.iter().position(|f| f.name == name).ok_or_else(|| Abend::ironwork(format!("no file named {name}"), pos))
    }

    pub(super) fn set_status(&mut self, k: usize, status: impl Into<FileStatus>, pos: Pos) -> R<()> {
        if let Some(r) = &self.program.files[k].status {
            let loc = self.locate(r)?;
            let bytes = self.page.encode(status.into().as_str()).map_err(|e| Abend::ironwork(e.to_string(), pos))?;
            self.assign(loc, Val::Bytes(bytes), None, pos)?;
        }
        Ok(())
    }

    /// Records an I/O status; with no FILE STATUS to hold it, a failing status ends the run.
    pub(super) fn io_status(&mut self, k: usize, status: impl Into<FileStatus>, message: String, pos: Pos) -> R<()> {
        let status = status.into();
        self.set_status(k, status, pos)?;
        if self.program.files[k].status.is_none() && !status.covers('0') {
            return Err(Abend { code: AbendCode::Io(status), message, pos });
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
        (self.base + offset as usize, size as usize)
    }

    fn record_area(&self, k: usize) -> &[u8] {
        let (offset, size) = self.area(k);
        &self.unit.mem[offset..offset + size]
    }

    fn sequential(&self, k: usize) -> bool {
        let decl = &self.program.files[k];
        decl.access == Access::Sequential || decl.organization == Organization::Sequential
    }

    /// Where a data item in file k's record area lies within the record.
    fn record_span(&mut self, k: usize, r: &Ref, pos: Pos) -> R<KeySpan> {
        let loc = self.locate(r)?;
        let (start, size) = self.area(k);
        if loc.offset < start || loc.offset + loc.len > start + size {
            return Err(Abend::ironwork(format!("{} is not in a record of {}", r.name, self.program.files[k].name), pos));
        }
        Ok(KeySpan { offset: loc.offset - start, len: loc.len })
    }

    fn keying(&mut self, k: usize, pos: Pos) -> R<Keying> {
        let program = self.program;
        let decl = &program.files[k];
        Ok(match decl.organization {
            Organization::Relative => Keying::Relative,
            Organization::Indexed => {
                let prime = decl.record_key.as_ref().ok_or_else(|| Abend::ironwork(format!("{} has no RECORD KEY", decl.name), pos))?;
                let prime = self.record_span(k, prime, pos)?;
                let mut alternates = Vec::new();
                for (r, duplicates) in &decl.alternate_keys {
                    alternates.push((self.record_span(k, r, pos)?, *duplicates));
                }
                Keying::Indexed { prime, alternates }
            }
            _ => Keying::Position,
        })
    }

    /// Which key of an indexed file a data item names (0 the prime key, then each alternate), and
    /// its value. With `partial` (START) it may be a leading part of the key.
    fn key_named(&mut self, k: usize, keying: &Keying, r: &Ref, partial: bool, pos: Pos) -> R<(usize, Vec<u8>)> {
        let span = self.record_span(k, r, pos)?;
        let not_a_key = || Abend::ironwork(format!("{} is not a key of {}", r.name, self.program.files[k].name), pos);
        let Keying::Indexed { prime, alternates } = keying else { return Err(not_a_key()) };
        let fits = |key: &KeySpan| key.offset == span.offset && (span.len == key.len || partial && span.len < key.len);
        let which = std::iter::once(prime).chain(alternates.iter().map(|(s, _)| s)).position(fits).ok_or_else(not_a_key)?;
        Ok((which, span.of(self.record_area(k))))
    }

    fn relative_value(&mut self, k: usize, pos: Pos) -> R<i64> {
        let program = self.program;
        let decl = &program.files[k];
        let r = decl.relative_key.as_ref().ok_or_else(|| Abend::ironwork(format!("{} has no RELATIVE KEY", decl.name), pos))?;
        self.integer(&Expr::Operand(Operand::Ref(r.clone())), pos)
    }

    /// The RELATIVE KEY's record number as a key, or None when it is below 1.
    fn relative_number(&mut self, k: usize, pos: Pos) -> R<Option<Vec<u8>>> {
        let n = self.relative_value(k, pos)?;
        Ok((n >= 1).then(|| files::record_number(n as u64)))
    }

    /// Whether record number `n` fits file k's RELATIVE KEY item, as sequential READ and WRITE
    /// store it there.
    fn relative_fits(&mut self, k: usize, n: u64) -> R<bool> {
        let program = self.program;
        let Some(r) = &program.files[k].relative_key else { return Ok(true) };
        let loc = self.locate(r)?;
        Ok(match loc.kind.digits_scale() {
            Some((digits, scale)) => digits.saturating_sub(scale) >= 19 || n < 10u64.pow(digits - scale),
            None => true,
        })
    }

    /// Runs `op` on file k when it is open and held in memory.
    fn held<T>(&mut self, k: usize, op: impl FnOnce(&mut Self, OpenMode, Format, &mut Keyed) -> R<T>) -> R<Option<T>> {
        let Some(mut f) = self.unit.programs[self.me].files[k].take() else { return Ok(None) };
        let (mode, format) = (f.mode, f.format);
        let result = match f.keyed() {
            Some(keyed) => op(self, mode, format, keyed).map(Some),
            None => Ok(None),
        };
        self.unit.programs[self.me].files[k] = Some(f);
        result
    }

    fn is_held(&self, k: usize) -> bool {
        self.unit.programs[self.me].files[k].as_ref().is_some_and(|f| f.is_keyed())
    }

    /// A record as WRITE or REWRITE puts it in the file: a fixed record at the length of the area.
    fn record_bytes(&self, k: usize, loc: Loc, format: Format) -> Vec<u8> {
        let mut bytes = self.bytes(loc).to_vec();
        if format != Format::Variable {
            bytes.resize(self.area(k).1.max(bytes.len()), ebcdic::SPACE);
        }
        bytes
    }

    /// The file a record belongs to, and the record as it stands once FROM has moved into it.
    fn record_of(&mut self, record: &Ref, from: Option<&Operand>, verb: &str, pos: Pos) -> R<(usize, Loc)> {
        let dest = if from.is_some() { self.locate_receiving(record)? } else { self.locate(record)? };
        let Some(k) = self.layout.items.get(dest.item).and_then(|i| i.file).map(|k| k as usize) else {
            return Err(Abend::ironwork(format!("{verb} {}: not a record of a file", record.name), pos));
        };
        let Some(op) = from else { return Ok((k, dest)) };
        let (val, src) = self.operand_with_loc(op, pos)?;
        self.assign(dest, val, src, pos)?;
        Ok((k, self.locate(record)?))
    }

    /// Moves a record into file k's area, and to INTO's item; a variable-length record fills only
    /// its own length. True when the record was longer than the area.
    fn deliver(&mut self, k: usize, record: &[u8], variable: bool, into: Option<&Ref>, pos: Pos) -> R<bool> {
        let (offset, size) = self.area(k);
        let n = record.len().min(size);
        self.unit.mem[offset..offset + n].copy_from_slice(&record[..n]);
        if !variable {
            self.unit.mem[offset + n..offset + size].fill(ebcdic::SPACE);
        }
        if let Some(r) = into {
            let dest = self.locate_receiving(r)?;
            let bytes = self.unit.mem[offset..offset + if variable { n } else { size }].to_vec();
            self.assign(dest, Val::Bytes(bytes), None, pos)?;
        }
        Ok(record.len() > size)
    }

    pub(super) fn open_file(&mut self, mode: OpenMode, name: &str, pos: Pos) -> R<()> {
        let k = self.file_index(name, pos)?;
        let program = self.program;
        let decl = &program.files[k];
        if self.unit.programs[self.me].files[k].is_some() {
            return self.io_status(k, FileStatus::AlreadyOpen, format!("{name} is already open"), pos);
        }
        let default = match decl.organization {
            Organization::LineSequential => Format::Text,
            _ if decl.recording == Some('V') || decl.record_min != decl.record_max => Format::Variable,
            _ => Format::Fixed,
        };
        let dd = self.unit.dds.get(&decl.assign);
        if let Some(d) = &dd {
            self.unit.notify(Event::Open { dd: &decl.assign, mode, path: &d.path });
        }
        let no_dd = format!("{name}: no DD {} was given (--dd {}=path)", decl.assign, decl.assign);
        let held = match decl.organization {
            Organization::Indexed | Organization::Relative => true,
            Organization::Sequential => mode == OpenMode::InputOutput,
            Organization::LineSequential => false,
        };
        if held {
            let status = match &dd {
                Some(d) if mode == OpenMode::Output || d.path.exists() => FileStatus::Success,
                _ if decl.optional && mode != OpenMode::Output => FileStatus::SuccessOptional,
                None => return self.io_status(k, FileStatus::FileNotFound, no_dd, pos),
                Some(d) => return self.io_status(k, FileStatus::FileNotFound, format!("{name}: {}: no such file", d.path.display()), pos),
            };
            let keying = self.keying(k, pos)?;
            let format = dd.as_ref().and_then(|d| d.format).unwrap_or(default);
            return match files::open_keyed(dd.as_ref(), mode, format, keying, self.area(k).1, self.page) {
                Ok(f) => {
                    self.unit.programs[self.me].files[k] = Some(f);
                    self.set_status(k, status, pos)
                }
                Err(e) => self.io_status(k, FileStatus::PermanentError, format!("{name}: {e}"), pos),
            };
        }
        match dd {
            None if decl.optional && mode == OpenMode::Input => {
                self.unit.programs[self.me].files[k] = Some(files::absent());
                self.set_status(k, FileStatus::SuccessOptional, pos)
            }
            None => self.io_status(k, FileStatus::FileNotFound, no_dd, pos),
            Some(dd) => match files::open(&dd, mode, dd.format.unwrap_or(default)) {
                Ok(f) => {
                    self.unit.programs[self.me].files[k] = Some(f);
                    self.set_status(k, FileStatus::Success, pos)
                }
                Err(e) => {
                    let status = match e.kind() {
                        std::io::ErrorKind::NotFound => FileStatus::FileNotFound,
                        std::io::ErrorKind::Unsupported => FileStatus::OpenModeUnsupported,
                        _ => FileStatus::PermanentError,
                    };
                    self.io_status(k, status, format!("{name}: {}: {e}", dd.path.display()), pos)
                }
            },
        }
    }

    pub(super) fn close_file(&mut self, name: &str, pos: Pos) -> R<()> {
        let k = self.file_index(name, pos)?;
        match self.unit.programs[self.me].files[k].take() {
            None => self.io_status(k, FileStatus::NotOpen, format!("{name} is not open"), pos),
            Some(f) => match f.close() {
                Ok(()) => {
                    let assign = &self.program.files[k].assign;
                    if let Some(d) = self.unit.dds.get(assign) {
                        self.unit.notify(Event::Close { dd: assign, path: &d.path });
                    }
                    self.set_status(k, FileStatus::Success, pos)
                }
                Err(e) => self.io_status(k, FileStatus::PermanentError, format!("{name}: {e}"), pos),
            },
        }
    }

    pub(super) fn read_stmt(&mut self, r: &'p ReadStmt) -> R<Flow> {
        let k = self.file_index(&r.file, r.pos)?;
        if !self.is_held(k) {
            return self.read_stream(k, r);
        }
        let pos = r.pos;
        let program = self.program;
        let decl = &program.files[k];
        let sequential = self.sequential(k) || decl.access == Access::Dynamic && r.next;
        let (status, found, variable) = self
            .held(k, |m, mode, format, keyed| {
                let variable = format == Format::Variable;
                if !matches!(mode, OpenMode::Input | OpenMode::InputOutput) {
                    return Ok((FileStatus::NotOpenInput, None, variable));
                }
                if sequential {
                    return Ok(match keyed.read_next(r.previous) {
                        Err(status) => (status, None, variable),
                        Ok(None) => (FileStatus::AtEnd, None, variable),
                        Ok(Some(found)) if keyed.keying == Keying::Relative && !m.relative_fits(k, files::number_of(&found.key))? => (FileStatus::RelativeKeyOverflow, None, variable),
                        Ok(Some(found)) => (if found.duplicate { FileStatus::SuccessDuplicate } else { FileStatus::Success }, Some(found), variable),
                    });
                }
                let keying = keyed.keying.clone();
                let (which, value) = match (&keying, &r.key) {
                    (Keying::Indexed { .. }, Some(key)) => m.key_named(k, &keying, key, false, pos)?,
                    (Keying::Indexed { prime, .. }, None) => (0, prime.of(m.record_area(k))),
                    _ => match m.relative_number(k, pos)? {
                        Some(key) => (0, key),
                        None => {
                            keyed.lose_position();
                            return Ok((FileStatus::NotFound, None, variable));
                        }
                    },
                };
                Ok(match keyed.get(which, &value) {
                    None => {
                        keyed.lose_position();
                        (FileStatus::NotFound, None, variable)
                    }
                    Some(found) => {
                        keyed.read_at(which, &found.key);
                        (if found.duplicate { FileStatus::SuccessDuplicate } else { FileStatus::Success }, Some(found), variable)
                    }
                })
            })?
            .unwrap_or((FileStatus::NotOpenInput, None, false));
        let mut status = status;
        if let Some(found) = found {
            if sequential && decl.organization == Organization::Relative
                && let Some(rk) = &decl.relative_key
            {
                self.set_integer(rk, files::number_of(&found.key) as i64, pos)?;
            }
            if self.deliver(k, &found.record, variable, r.into.as_ref(), pos)? && status == FileStatus::Success {
                status = FileStatus::SuccessWrongLength;
            }
        }
        if sequential { self.conclude(k, status, &r.at_end, '1', "READ", pos) } else { self.conclude(k, status, &r.invalid, '2', "READ", pos) }
    }

    fn read_stream(&mut self, k: usize, r: &'p ReadStmt) -> R<Flow> {
        let pos = r.pos;
        let size = self.area(k).1;
        let Some(mut f) = self.unit.programs[self.me].files[k].take() else {
            return self.conclude(k, FileStatus::NotOpenInput, &r.at_end, '1', "READ", pos);
        };
        let added = self.carriage[k].is_some_and(|c| !c.reserved) && f.format != Format::Text;
        let read = f.read(size + usize::from(added));
        let format = f.format;
        let input = f.mode == OpenMode::Input;
        self.unit.programs[self.me].files[k] = Some(f);
        if !input {
            return self.conclude(k, FileStatus::NotOpenInput, &r.at_end, '1', "READ", pos);
        }
        let (record, wrong_length) = match read {
            Err(e) => {
                let program = self.program;
                self.io_status(k, FileStatus::PermanentError, format!("READ {}: {e}", program.files[k].name), pos)?;
                return Ok(Flow::Next);
            }
            Ok(Record::End) => return self.conclude(k, FileStatus::AtEnd, &r.at_end, '1', "READ", pos),
            Ok(Record::Data(bytes)) => (bytes, false),
            Ok(Record::WrongLength(bytes)) => (bytes, true),
        };
        let record = if added { record.get(1..).unwrap_or_default().to_vec() } else { record };
        let record = if format == Format::Text {
            let unknown = self.page.encode_char('?').unwrap_or(0x6F);
            String::from_utf8_lossy(&record).chars().map(|c| self.page.encode_char(c).unwrap_or(unknown)).collect()
        } else {
            record
        };
        let long = self.deliver(k, &record, format == Format::Variable, r.into.as_ref(), pos)?;
        self.conclude(k, if wrong_length || long { FileStatus::SuccessWrongLength } else { FileStatus::Success }, &r.at_end, '1', "READ", pos)
    }

    pub(super) fn write_stmt(&mut self, record: &Ref, from: Option<&Operand>, advancing: Option<&Advancing>, invalid: &'p Handlers, pos: Pos) -> R<Flow> {
        let (k, loc) = self.record_of(record, from, "WRITE", pos)?;
        self.write_record(k, loc, advancing, invalid, pos)
    }

    /// WRITE of the record at `loc` to file k.
    pub(super) fn write_record(&mut self, k: usize, loc: Loc, advancing: Option<&Advancing>, invalid: &'p Handlers, pos: Pos) -> R<Flow> {
        if !self.is_held(k) {
            let (before, space) = match advancing {
                Some(a) => self.advance(a, pos)?,
                None => (false, Space::Lines(1)),
            };
            self.write_stream(k, loc, before, space, pos)?;
            return Ok(Flow::Next);
        }
        let program = self.program;
        let decl = &program.files[k];
        let sequential = self.sequential(k);
        let status = self
            .held(k, |m, mode, format, keyed| {
                let allowed = match mode {
                    OpenMode::Output => true,
                    OpenMode::Extend => sequential,
                    OpenMode::InputOutput => !sequential,
                    OpenMode::Input => false,
                };
                if !allowed || keyed.keying == Keying::Position {
                    return Ok(FileStatus::NotOpenOutput);
                }
                keyed.last_read = None;
                let bytes = m.record_bytes(k, loc, format);
                let key = match &keyed.keying {
                    Keying::Indexed { prime, .. } => {
                        let key = prime.of(&bytes);
                        if sequential && keyed.highest_key().is_some_and(|h| key <= *h) {
                            return Ok(FileStatus::SequenceError);
                        }
                        key
                    }
                    _ if sequential => {
                        let n = keyed.highest_key().map_or(0, |h| files::number_of(h)) + 1;
                        if n > files::MAX_RELATIVE || !m.relative_fits(k, n)? {
                            return Ok(FileStatus::BoundaryViolation);
                        }
                        if let Some(rk) = &decl.relative_key {
                            m.set_integer(rk, n as i64, pos)?;
                        }
                        files::record_number(n)
                    }
                    _ => match m.relative_number(k, pos)? {
                        Some(key) if files::number_of(&key) <= files::MAX_RELATIVE => key,
                        _ => return Ok(FileStatus::BoundaryViolation),
                    },
                };
                Ok(match keyed.insert(key, bytes) {
                    Err(status) => status,
                    Ok(true) => FileStatus::SuccessDuplicate,
                    Ok(false) => FileStatus::Success,
                })
            })?
            .unwrap_or(FileStatus::NotOpenOutput);
        self.conclude(k, status, invalid, '2', "WRITE", pos)
    }

    /// A WRITE's ADVANCING phrase as a movement, BEFORE or AFTER the line; a count below zero
    /// moves as zero ([`numeric::assumptions::PRINT_CONTROL_RUN_TIME`]).
    fn advance(&mut self, a: &Advancing, pos: Pos) -> R<(bool, Space)> {
        Ok(match a {
            Advancing::Lines { before, count } => (*before, Space::Lines(self.integer(count, pos)?.max(0) as u64)),
            Advancing::Page { before } => (*before, Space::Channel(1)),
            Advancing::Mnemonic { before, name, environment } => {
                let space = printer::mnemonic_space(environment).ok_or_else(|| Abend::ironwork(format!("ADVANCING {name}: {environment} is not a printer channel"), pos))?;
                (*before, space)
            }
        })
    }

    /// A WRITE to a sequential file: a print file's records carry the control character, a text
    /// DD shows it as line and form feeds.
    pub(super) fn write_stream(&mut self, k: usize, loc: Loc, before: bool, space: Space, pos: Pos) -> R<()> {
        let name = self.program.files[k].name.clone();
        let Some(mut f) = self.unit.programs[self.me].files[k].take() else {
            return self.io_status(k, FileStatus::NotOpenOutput, format!("WRITE {name}: {}", FileStatus::NotOpenOutput.meaning()), pos);
        };
        if f.mode == OpenMode::Input {
            self.unit.programs[self.me].files[k] = Some(f);
            return self.io_status(k, FileStatus::NotOpenOutput, format!("WRITE {name}: {}", FileStatus::NotOpenOutput.meaning()), pos);
        }
        let carriage = self.carriage[k];
        let controls = carriage.map(|c| printer::controls(c.machine, before, space));
        let reserved = usize::from(carriage.is_some_and(|c| c.reserved));
        if let Some(c) = controls.filter(|_| reserved == 1 && loc.len > 0) {
            self.unit.mem[loc.offset] = c.data;
        }
        let bytes = self.record_bytes(k, loc, f.format);
        let written = match (f.format, controls) {
            (Format::Text, _) => {
                let line = self.page.decode(bytes.get(reserved..).unwrap_or_default()).trim_end().to_owned();
                let (ahead, behind) = printer::text_motion(before, space);
                f.print(ahead, &line, behind)
            }
            (_, None) => f.write(&bytes),
            (_, Some(c)) => c.records().try_for_each(|(control, line)| {
                let mut record = Vec::with_capacity(bytes.len() + 1);
                record.push(control);
                if line {
                    record.extend_from_slice(&bytes[reserved.min(bytes.len())..]);
                } else {
                    record.resize(bytes.len() + 1 - reserved.min(bytes.len()), ebcdic::SPACE);
                }
                f.write(&record)
            }),
        };
        self.unit.programs[self.me].files[k] = Some(f);
        match written {
            Ok(()) => self.set_status(k, FileStatus::Success, pos),
            Err(e) => self.io_status(k, FileStatus::PermanentError, format!("WRITE {name}: {e}"), pos),
        }
    }

    pub(super) fn rewrite_stmt(&mut self, record: &Ref, from: Option<&Operand>, invalid: &'p Handlers, pos: Pos) -> R<Flow> {
        let (k, loc) = self.record_of(record, from, "REWRITE", pos)?;
        let sequential = self.sequential(k);
        let status = self
            .held(k, |m, mode, format, keyed| {
                if mode != OpenMode::InputOutput {
                    return Ok(FileStatus::NotOpenInputOutput);
                }
                let bytes = m.record_bytes(k, loc, format);
                let prior = keyed.last_read.take();
                let key = if sequential {
                    let Some(prior) = prior else { return Ok(FileStatus::NoPriorRead) };
                    if keyed.prime_key(&bytes).is_some_and(|key| key != prior) {
                        return Ok(FileStatus::SequenceError);
                    }
                    prior
                } else {
                    match keyed.prime_key(&bytes) {
                        Some(key) => key,
                        None => match m.relative_number(k, pos)? {
                            Some(key) => key,
                            None => return Ok(FileStatus::NotFound),
                        },
                    }
                };
                if keyed.keying == Keying::Position && keyed.record(&key).is_some_and(|old| old.len() != bytes.len()) {
                    return Ok(FileStatus::RecordLengthChanged);
                }
                Ok(match keyed.replace(key, bytes) {
                    Err(status) => status,
                    Ok(true) => FileStatus::SuccessDuplicate,
                    Ok(false) => FileStatus::Success,
                })
            })?
            .unwrap_or(FileStatus::NotOpenInputOutput);
        self.conclude(k, status, invalid, '2', "REWRITE", pos)
    }

    pub(super) fn delete_stmt(&mut self, file: &str, invalid: &'p Handlers, pos: Pos) -> R<Flow> {
        let k = self.file_index(file, pos)?;
        let sequential = self.sequential(k);
        let status = self
            .held(k, |m, mode, _, keyed| {
                if mode != OpenMode::InputOutput || keyed.keying == Keying::Position {
                    return Ok(FileStatus::NotOpenInputOutput);
                }
                let prior = keyed.last_read.take();
                let key = match (&keyed.keying, sequential) {
                    (_, true) => match prior {
                        Some(key) => key,
                        None => return Ok(FileStatus::NoPriorRead),
                    },
                    (Keying::Indexed { prime, .. }, false) => prime.of(m.record_area(k)),
                    _ => match m.relative_number(k, pos)? {
                        Some(key) => key,
                        None => return Ok(FileStatus::NotFound),
                    },
                };
                Ok(if keyed.remove(&key).is_some() { FileStatus::Success } else { FileStatus::NotFound })
            })?
            .unwrap_or(FileStatus::NotOpenInputOutput);
        self.conclude(k, status, invalid, '2', "DELETE", pos)
    }

    pub(super) fn start_stmt(&mut self, file: &str, key: Option<&(RelOp, Ref)>, invalid: &'p Handlers, pos: Pos) -> R<Flow> {
        let k = self.file_index(file, pos)?;
        let (wanted, or_equal) = match key.map(|(op, _)| *op) {
            None | Some(RelOp::Eq) => (Ordering::Equal, false),
            Some(RelOp::Gt) => (Ordering::Greater, false),
            Some(RelOp::Ge) => (Ordering::Greater, true),
            Some(_) => return Err(Abend::ironwork("START KEY takes =, >, NOT < or >=", pos)),
        };
        let status = self
            .held(k, |m, mode, _, keyed| {
                if !matches!(mode, OpenMode::Input | OpenMode::InputOutput) || keyed.keying == Keying::Position {
                    return Ok(FileStatus::NotOpenInput);
                }
                let keying = keyed.keying.clone();
                let (which, value) = match (&keying, key) {
                    (Keying::Indexed { .. }, Some((_, r))) => m.key_named(k, &keying, r, true, pos)?,
                    (Keying::Indexed { prime, .. }, None) => (0, prime.of(m.record_area(k))),
                    (_, Some((_, r))) => (0, files::record_number(m.integer(&Expr::Operand(Operand::Ref(r.clone())), pos)?.max(0) as u64)),
                    (_, None) => (0, files::record_number(m.relative_value(k, pos)?.max(0) as u64)),
                };
                Ok(if keyed.start(which, wanted, or_equal, &value) { FileStatus::Success } else { FileStatus::NotFound })
            })?
            .unwrap_or(FileStatus::NotOpenInput);
        self.conclude(k, status, invalid, '2', "START", pos)
    }
}
