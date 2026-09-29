//! The file statements. A sequential file streams through its DD; an indexed or relative file, or
//! a sequential file opened I-O, is held in memory ([`files::Keyed`]).

use super::*;
use crate::files::{self, Format, KeySpan, Keyed, Keying, Record};

/// What a failing file status means, for the message when no FILE STATUS or phrase takes it.
fn meaning(code: &str) -> &'static str {
    match code {
        "10" => "there is no next record",
        "21" => "the key is out of sequence",
        "22" => "a record with that key is already there",
        "23" => "there is no record with that key",
        "14" => "the record number is too large for the RELATIVE KEY",
        "24" => "the record number is outside the file",
        "43" => "the last statement on the file was not a successful READ",
        "44" => "the record is not the length of the one it replaces",
        "46" => "there is no next record: the last READ reached the end, or START found nothing",
        "47" => "the file is not open INPUT or I-O",
        "48" => "the file is not open for output",
        "49" => "the file is not open I-O",
        _ => "the statement failed",
    }
}

impl<'p> Machine<'p, '_, '_> {
    fn file_index(&self, name: &str, pos: Pos) -> R<usize> {
        self.program.files.iter().position(|f| f.name == name).ok_or_else(|| Abend::ironwork(format!("no file named {name}"), pos))
    }

    fn set_status(&mut self, k: usize, code: &str, pos: Pos) -> R<()> {
        if let Some(r) = &self.program.files[k].status {
            let loc = self.locate(r)?;
            let bytes = self.page.encode(code).map_err(|e| Abend::ironwork(e.to_string(), pos))?;
            self.assign(loc, Val::Bytes(bytes), None, pos)?;
        }
        Ok(())
    }

    /// Records an I/O status; with no FILE STATUS to hold it, a failing status ends the run.
    fn io_status(&mut self, k: usize, code: &str, message: String, pos: Pos) -> R<()> {
        self.set_status(k, code, pos)?;
        if self.program.files[k].status.is_none() && !code.starts_with('0') {
            return Err(Abend { code: format!("IO-{code}"), message, pos });
        }
        Ok(())
    }

    /// Sets the file status and runs the phrase it selects: the ON phrase for the condition it
    /// covers (AT END for 1x, INVALID KEY for 2x), the NOT phrase on success.
    fn conclude(&mut self, k: usize, code: &str, handlers: &'p Handlers, covers: char, verb: &str, pos: Pos) -> R<Flow> {
        let body = match code.chars().next() {
            Some('0') => handlers.not_on.as_deref(),
            Some(c) if c == covers => handlers.on.as_deref(),
            _ => None,
        };
        if let Some(body) = body {
            self.set_status(k, code, pos)?;
            return self.run_block(body);
        }
        let program = self.program;
        self.io_status(k, code, format!("{verb} {}: file status {code}: {}", program.files[k].name, meaning(code)), pos)?;
        Ok(Flow::Next)
    }

    fn area(&self, k: usize) -> (usize, usize) {
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

    /// The file a record belongs to, after FROM has moved into it.
    fn record_of(&mut self, record: &Ref, from: Option<&Operand>, verb: &str, pos: Pos) -> R<(usize, Loc)> {
        let loc = self.locate(record)?;
        let Some(k) = self.layout.items.get(loc.item).and_then(|i| i.file).map(|k| k as usize) else {
            return Err(Abend::ironwork(format!("{verb} {}: not a record of a file", record.name), pos));
        };
        if let Some(op) = from {
            let (val, src) = self.operand_with_loc(op, pos)?;
            self.assign(loc, val, src, pos)?;
        }
        Ok((k, loc))
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
            let dest = self.locate(r)?;
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
            return self.io_status(k, "41", format!("{name} is already open"), pos);
        }
        let default = match decl.organization {
            Organization::LineSequential => Format::Text,
            _ if decl.recording == Some('V') || decl.record_min != decl.record_max => Format::Variable,
            _ => Format::Fixed,
        };
        let dd = self.unit.dds.get(&decl.assign);
        let no_dd = format!("{name}: no DD {} was given (--dd {}=path)", decl.assign, decl.assign);
        let held = match decl.organization {
            Organization::Indexed | Organization::Relative => true,
            Organization::Sequential => mode == OpenMode::InputOutput,
            Organization::LineSequential => false,
        };
        if held {
            let status = match &dd {
                Some(d) if mode == OpenMode::Output || d.path.exists() => "00",
                _ if decl.optional && mode != OpenMode::Output => "05",
                None => return self.io_status(k, "35", no_dd, pos),
                Some(d) => return self.io_status(k, "35", format!("{name}: {}: no such file", d.path.display()), pos),
            };
            let keying = self.keying(k, pos)?;
            let format = dd.as_ref().and_then(|d| d.format).unwrap_or(default);
            return match files::open_keyed(dd.as_ref(), mode, format, keying, self.area(k).1, self.page) {
                Ok(f) => {
                    self.unit.programs[self.me].files[k] = Some(f);
                    self.set_status(k, status, pos)
                }
                Err(e) => self.io_status(k, "30", format!("{name}: {e}"), pos),
            };
        }
        match dd {
            None if decl.optional && mode == OpenMode::Input => {
                self.unit.programs[self.me].files[k] = Some(files::absent());
                self.set_status(k, "05", pos)
            }
            None => self.io_status(k, "35", no_dd, pos),
            Some(dd) => match files::open(&dd, mode, dd.format.unwrap_or(default)) {
                Ok(f) => {
                    self.unit.programs[self.me].files[k] = Some(f);
                    self.set_status(k, "00", pos)
                }
                Err(e) => {
                    let code = match e.kind() {
                        std::io::ErrorKind::NotFound => "35",
                        std::io::ErrorKind::Unsupported => "37",
                        _ => "30",
                    };
                    self.io_status(k, code, format!("{name}: {}: {e}", dd.path.display()), pos)
                }
            },
        }
    }

    pub(super) fn close_file(&mut self, name: &str, pos: Pos) -> R<()> {
        let k = self.file_index(name, pos)?;
        match self.unit.programs[self.me].files[k].take() {
            None => self.io_status(k, "42", format!("{name} is not open"), pos),
            Some(f) => match f.close() {
                Ok(()) => self.set_status(k, "00", pos),
                Err(e) => self.io_status(k, "30", format!("{name}: {e}"), pos),
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
        let (code, found, variable) = self
            .held(k, |m, mode, format, keyed| {
                let variable = format == Format::Variable;
                if !matches!(mode, OpenMode::Input | OpenMode::InputOutput) {
                    return Ok(("47", None, variable));
                }
                if sequential {
                    return Ok(match keyed.step(r.previous) {
                        Err(code) => (code, None, variable),
                        Ok(None) => ("10", None, variable),
                        Ok(Some(found)) if keyed.keying == Keying::Relative && !m.relative_fits(k, files::number_of(&found.key))? => ("14", None, variable),
                        Ok(Some(found)) => (if found.duplicate { "02" } else { "00" }, Some(found), variable),
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
                            return Ok(("23", None, variable));
                        }
                    },
                };
                Ok(match keyed.get(which, &value) {
                    None => {
                        keyed.lose_position();
                        ("23", None, variable)
                    }
                    Some(found) => {
                        keyed.read_at(which, &found.key);
                        (if found.duplicate { "02" } else { "00" }, Some(found), variable)
                    }
                })
            })?
            .unwrap_or(("47", None, false));
        let mut code = code;
        if let Some(found) = found {
            if sequential && decl.organization == Organization::Relative
                && let Some(rk) = &decl.relative_key
            {
                self.set_integer(rk, files::number_of(&found.key) as i64, pos)?;
            }
            if self.deliver(k, &found.record, variable, r.into.as_ref(), pos)? && code == "00" {
                code = "04";
            }
        }
        if sequential { self.conclude(k, code, &r.at_end, '1', "READ", pos) } else { self.conclude(k, code, &r.invalid, '2', "READ", pos) }
    }

    fn read_stream(&mut self, k: usize, r: &'p ReadStmt) -> R<Flow> {
        let pos = r.pos;
        let size = self.area(k).1;
        let Some(mut f) = self.unit.programs[self.me].files[k].take() else {
            return self.conclude(k, "47", &r.at_end, '1', "READ", pos);
        };
        let read = f.read(size);
        let format = f.format;
        let input = f.mode == OpenMode::Input;
        self.unit.programs[self.me].files[k] = Some(f);
        if !input {
            return self.conclude(k, "47", &r.at_end, '1', "READ", pos);
        }
        let (record, wrong_length) = match read {
            Err(e) => {
                let program = self.program;
                self.io_status(k, "30", format!("READ {}: {e}", program.files[k].name), pos)?;
                return Ok(Flow::Next);
            }
            Ok(Record::End) => return self.conclude(k, "10", &r.at_end, '1', "READ", pos),
            Ok(Record::Data(bytes)) => (bytes, false),
            Ok(Record::WrongLength(bytes)) => (bytes, true),
        };
        let record = if format == Format::Text {
            let unknown = self.page.encode_char('?').unwrap_or(0x6F);
            String::from_utf8_lossy(&record).chars().map(|c| self.page.encode_char(c).unwrap_or(unknown)).collect()
        } else {
            record
        };
        let long = self.deliver(k, &record, format == Format::Variable, r.into.as_ref(), pos)?;
        self.conclude(k, if wrong_length || long { "04" } else { "00" }, &r.at_end, '1', "READ", pos)
    }

    pub(super) fn write_stmt(&mut self, record: &Ref, from: Option<&Operand>, advancing: Option<&Advancing>, invalid: &'p Handlers, pos: Pos) -> R<Flow> {
        let (k, loc) = self.record_of(record, from, "WRITE", pos)?;
        if !self.is_held(k) {
            self.write_stream(k, loc, advancing, pos)?;
            return Ok(Flow::Next);
        }
        let program = self.program;
        let decl = &program.files[k];
        let sequential = self.sequential(k);
        let code = self
            .held(k, |m, mode, format, keyed| {
                let allowed = match mode {
                    OpenMode::Output => true,
                    OpenMode::Extend => sequential,
                    OpenMode::InputOutput => !sequential,
                    OpenMode::Input => false,
                };
                if !allowed || keyed.keying == Keying::Position {
                    return Ok("48");
                }
                keyed.last_read = None;
                let bytes = m.record_bytes(k, loc, format);
                let key = match &keyed.keying {
                    Keying::Indexed { prime, .. } => {
                        let key = prime.of(&bytes);
                        if sequential && keyed.highest_key().is_some_and(|h| key <= *h) {
                            return Ok("21");
                        }
                        key
                    }
                    _ if sequential => {
                        let n = keyed.highest_key().map_or(0, |h| files::number_of(h)) + 1;
                        if n > files::MAX_RELATIVE || !m.relative_fits(k, n)? {
                            return Ok("24");
                        }
                        if let Some(rk) = &decl.relative_key {
                            m.set_integer(rk, n as i64, pos)?;
                        }
                        files::record_number(n)
                    }
                    _ => match m.relative_number(k, pos)? {
                        Some(key) if files::number_of(&key) <= files::MAX_RELATIVE => key,
                        _ => return Ok("24"),
                    },
                };
                Ok(match keyed.insert(key, bytes) {
                    Err(code) => code,
                    Ok(true) => "02",
                    Ok(false) => "00",
                })
            })?
            .unwrap_or("48");
        self.conclude(k, code, invalid, '2', "WRITE", pos)
    }

    fn write_stream(&mut self, k: usize, loc: Loc, advancing: Option<&Advancing>, pos: Pos) -> R<()> {
        let name = self.program.files[k].name.clone();
        let Some(mut f) = self.unit.programs[self.me].files[k].take() else {
            return self.io_status(k, "48", format!("WRITE {name}: {}", meaning("48")), pos);
        };
        if f.mode == OpenMode::Input {
            self.unit.programs[self.me].files[k] = Some(f);
            return self.io_status(k, "48", format!("WRITE {name}: {}", meaning("48")), pos);
        }
        let bytes = self.record_bytes(k, loc, f.format);
        let written = match f.format {
            Format::Fixed | Format::Variable => f.write(&bytes),
            Format::Text => {
                let line = self.page.decode(&bytes).trim_end().to_owned();
                let lines = |n: i64| "\n".repeat(n.max(0) as usize);
                let text = match advancing {
                    None => format!("{line}\n"),
                    Some(Advancing::Page { before: false }) => format!("\u{c}{line}\n"),
                    Some(Advancing::Page { before: true }) => format!("{line}\n\u{c}"),
                    Some(Advancing::Lines { before, count }) => {
                        let n = self.integer(count, pos)?;
                        if *before { format!("{line}{}", lines(n)) } else { format!("{}{line}\n", lines(n - 1)) }
                    }
                };
                f.write(text.as_bytes())
            }
        };
        self.unit.programs[self.me].files[k] = Some(f);
        match written {
            Ok(()) => self.set_status(k, "00", pos),
            Err(e) => self.io_status(k, "30", format!("WRITE {name}: {e}"), pos),
        }
    }

    pub(super) fn rewrite_stmt(&mut self, record: &Ref, from: Option<&Operand>, invalid: &'p Handlers, pos: Pos) -> R<Flow> {
        let (k, loc) = self.record_of(record, from, "REWRITE", pos)?;
        let sequential = self.sequential(k);
        let code = self
            .held(k, |m, mode, format, keyed| {
                if mode != OpenMode::InputOutput {
                    return Ok("49");
                }
                let bytes = m.record_bytes(k, loc, format);
                let prior = keyed.last_read.take();
                let key = if sequential {
                    let Some(prior) = prior else { return Ok("43") };
                    if keyed.prime_key(&bytes).is_some_and(|key| key != prior) {
                        return Ok("21");
                    }
                    prior
                } else {
                    match keyed.prime_key(&bytes) {
                        Some(key) => key,
                        None => match m.relative_number(k, pos)? {
                            Some(key) => key,
                            None => return Ok("23"),
                        },
                    }
                };
                if keyed.keying == Keying::Position && keyed.record(&key).is_some_and(|old| old.len() != bytes.len()) {
                    return Ok("44");
                }
                Ok(match keyed.replace(key, bytes) {
                    Err(code) => code,
                    Ok(true) => "02",
                    Ok(false) => "00",
                })
            })?
            .unwrap_or("49");
        self.conclude(k, code, invalid, '2', "REWRITE", pos)
    }

    pub(super) fn delete_stmt(&mut self, file: &str, invalid: &'p Handlers, pos: Pos) -> R<Flow> {
        let k = self.file_index(file, pos)?;
        let sequential = self.sequential(k);
        let code = self
            .held(k, |m, mode, _, keyed| {
                if mode != OpenMode::InputOutput || keyed.keying == Keying::Position {
                    return Ok("49");
                }
                let prior = keyed.last_read.take();
                let key = match (&keyed.keying, sequential) {
                    (_, true) => match prior {
                        Some(key) => key,
                        None => return Ok("43"),
                    },
                    (Keying::Indexed { prime, .. }, false) => prime.of(m.record_area(k)),
                    _ => match m.relative_number(k, pos)? {
                        Some(key) => key,
                        None => return Ok("23"),
                    },
                };
                Ok(if keyed.remove(&key).is_some() { "00" } else { "23" })
            })?
            .unwrap_or("49");
        self.conclude(k, code, invalid, '2', "DELETE", pos)
    }

    pub(super) fn start_stmt(&mut self, file: &str, key: Option<&(RelOp, Ref)>, invalid: &'p Handlers, pos: Pos) -> R<Flow> {
        let k = self.file_index(file, pos)?;
        let (wanted, or_equal) = match key.map(|(op, _)| *op) {
            None | Some(RelOp::Eq) => (Ordering::Equal, false),
            Some(RelOp::Gt) => (Ordering::Greater, false),
            Some(RelOp::Ge) => (Ordering::Greater, true),
            Some(_) => return Err(Abend::ironwork("START KEY takes =, >, NOT < or >=", pos)),
        };
        let code = self
            .held(k, |m, mode, _, keyed| {
                if !matches!(mode, OpenMode::Input | OpenMode::InputOutput) || keyed.keying == Keying::Position {
                    return Ok("47");
                }
                let keying = keyed.keying.clone();
                let (which, value) = match (&keying, key) {
                    (Keying::Indexed { .. }, Some((_, r))) => m.key_named(k, &keying, r, true, pos)?,
                    (Keying::Indexed { prime, .. }, None) => (0, prime.of(m.record_area(k))),
                    (_, Some((_, r))) => (0, files::record_number(m.integer(&Expr::Operand(Operand::Ref(r.clone())), pos)?.max(0) as u64)),
                    (_, None) => (0, files::record_number(m.relative_value(k, pos)?.max(0) as u64)),
                };
                Ok(if keyed.start(which, wanted, or_equal, &value) { "00" } else { "23" })
            })?
            .unwrap_or("47");
        self.conclude(k, code, invalid, '2', "START", pos)
    }
}
