//! SORT, MERGE, RELEASE and RETURN. Records are held in memory: the input phase gathers them from
//! USING files or RELEASE, a stable sort orders them, and the output phase hands them to GIVING
//! files or RETURN. A table SORT reorders the table's elements in place.

use super::*;
use crate::files::{Format, Record};
use numeric::assumptions;

/// The signal a RELEASE or RETURN raises to stop the operation: control passes to the statement
/// after the SORT or MERGE, whose message is the reason.
const STOPPED: &str = "SORT-STOPPED";

static NO_HANDLERS: Handlers = Handlers { on: None, not_on: None };

fn stop(why: String, pos: Pos) -> Abend {
    Abend { code: STOPPED.into(), message: why, pos }
}

fn stopped_by_program() -> String {
    format!("SORT-RETURN was set to 16 (see {})", assumptions::SORT_RETURN_STOPS)
}

/// A SORT or MERGE whose input or output procedure is running.
pub(super) struct Active {
    sd: usize,
    keys: Vec<Key>,
    phase: Phase,
}

enum Phase {
    Input(Vec<Entry>),
    Output { records: Vec<Vec<u8>>, next: usize },
}

/// A key's place in the record (or table element) and how it reads.
#[derive(Clone, Copy)]
struct Key {
    ascending: bool,
    offset: usize,
    len: usize,
    kind: Kind,
    item: usize,
}

struct Entry {
    record: Vec<u8>,
    keys: Vec<Val>,
}

enum Input {
    Record(Vec<u8>),
    End,
    Failed(String),
}

/// How a SORT or MERGE ended: Err holds why it failed.
type Outcome = Result<(), String>;

/// The order of two records by their key values, most significant key first. Every comparison is
/// exact, so the order is total.
fn order(a: &[Val], b: &[Val], keys: &[Key]) -> Ordering {
    for ((x, y), key) in a.iter().zip(b).zip(keys) {
        let o = match (x, y) {
            (Val::Num(x), Val::Num(y)) => compare_fixed(x, y),
            (Val::Float(x), Val::Float(y)) => float_order(*x, *y),
            (Val::National(x), Val::National(y)) => compare_national(x, y),
            (Val::Bytes(x), Val::Bytes(y)) => ebcdic::compare_alphanumeric(x, y, &Collation::Native),
            _ => Ordering::Equal,
        };
        let o = if key.ascending { o } else { o.reverse() };
        if o != Ordering::Equal {
            return o;
        }
    }
    Ordering::Equal
}

/// Floating-point keys in numeric order, by exact value: normalized, the characteristic then the
/// fraction.
fn float_order(a: Hfp, b: Hfp) -> Ordering {
    let exact = |h: Hfp| {
        if h.fraction == 0 {
            return (0, 0, 0);
        }
        let top = 4 * h.precision.digits() - 4;
        let (mut exponent, mut fraction) = (h.characteristic as i32, h.fraction);
        while fraction >> top == 0 {
            fraction <<= 4;
            exponent -= 1;
        }
        (if h.negative { -1 } else { 1 }, exponent, fraction)
    };
    let ((sa, ea, fa), (sb, eb, fb)) = (exact(a), exact(b));
    match sa.cmp(&sb) {
        Ordering::Equal if sa < 0 => (eb, fb).cmp(&(ea, fa)),
        Ordering::Equal => (ea, fa).cmp(&(eb, fb)),
        other => other,
    }
}

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

    fn sort_return(&mut self, pos: Pos) -> R<i64> {
        let r = self.register("SORT-RETURN", pos);
        self.integer(&Expr::Operand(Operand::Ref(r)), pos)
    }

    /// Sort control statements would change what DFSORT does, and none are read here.
    fn refuse_control_statements(&mut self, pos: Pos) -> R<()> {
        let loc = self.locate(&self.register("SORT-CONTROL", pos))?;
        let dd = self.page.decode(self.bytes(loc)).trim().to_ascii_uppercase();
        if !dd.is_empty() && self.unit.dds.get(&dd).is_some() {
            return Err(Abend::ironwork(format!("DD {dd} holds sort control statements, which ironwork for COBOL does not read"), pos));
        }
        Ok(())
    }

    /// The keys of SD `sd`, placed within its records.
    fn file_keys(&mut self, sd: usize, keys: &[(bool, Ref)], pos: Pos) -> R<Vec<Key>> {
        let area = self.layout.file_areas[sd].0 as usize;
        let mut out = Vec::new();
        for (ascending, r) in keys {
            let Resolved::Item(i) = self.resolve(r)? else {
                return Err(Abend::ironwork(format!("{} is not a data item", r.name), pos));
            };
            let item = &self.layout.items[i];
            let offset = (item.offset as usize).checked_sub(area).ok_or_else(|| Abend::ironwork(format!("{} is not in the sort file's records", r.name), pos))?;
            out.push(Key { ascending: *ascending, offset, len: item.size as usize, kind: item.kind, item: i });
        }
        Ok(out)
    }

    /// Each key's value, read from storage at `base` as the program would read it.
    fn key_values(&mut self, base: usize, keys: &[Key], pos: Pos) -> R<Vec<Val>> {
        let mut out = Vec::with_capacity(keys.len());
        for k in keys {
            out.push(self.read(Loc { offset: base + k.offset, len: k.len, kind: k.kind, item: k.item }, pos)?);
        }
        Ok(out)
    }

    fn fixed_length(&self, k: usize) -> bool {
        let decl = &self.program.files[k];
        decl.recording != Some('V') && decl.record_min == decl.record_max
    }

    /// A record as it enters the sort: fitted to the SD's record length and keyed. See
    /// [`assumptions::SORT_RECORD_LENGTHS`].
    fn entry(&mut self, sd: usize, keys: &[Key], mut record: Vec<u8>, pos: Pos) -> R<Result<Entry, String>> {
        let (area, size) = self.area(sd);
        if self.fixed_length(sd) {
            record.resize(size, ebcdic::SPACE);
        } else {
            record.truncate(size);
        }
        if keys.iter().any(|k| k.offset + k.len > record.len()) {
            return Ok(Err(format!("a record of {} bytes ends inside a key", record.len())));
        }
        self.unit.mem[area..area + record.len()].copy_from_slice(&record);
        let keys = self.key_values(area, keys, pos)?;
        Ok(Ok(Entry { record, keys }))
    }

    fn status_failed(&mut self, k: usize) -> R<bool> {
        let program = self.program;
        let Some(r) = &program.files[k].status else { return Ok(false) };
        let loc = self.locate(r)?;
        Ok(self.bytes(loc).first().is_some_and(|&b| b != ebcdic::ZERO))
    }

    fn is_open(&self, k: usize) -> bool {
        self.unit.programs[self.me].files[k].is_some()
    }

    /// Opens a USING or GIVING file, which must not be open already.
    fn open_for_sort(&mut self, k: usize, mode: OpenMode, pos: Pos) -> R<Outcome> {
        let program = self.program;
        let name = &program.files[k].name;
        if self.is_open(k) {
            self.io_status(k, "41", format!("{name} is open, and a SORT or MERGE opens it itself"), pos)?;
            return Ok(Err(format!("{name} is already open (file status 41)")));
        }
        self.open_file(mode, name, pos)?;
        if self.is_open(k) { Ok(Ok(())) } else { Ok(Err(format!("OPEN {name} failed"))) }
    }

    fn close_for_sort(&mut self, k: usize, pos: Pos) -> R<Outcome> {
        let program = self.program;
        let name = &program.files[k].name;
        self.close_file(name, pos)?;
        if self.status_failed(k)? { Ok(Err(format!("CLOSE {name} failed"))) } else { Ok(Ok(())) }
    }

    /// The next record of an open USING file, with the file status READ would set.
    fn next_input(&mut self, k: usize, pos: Pos) -> R<Input> {
        let size = self.area(k).1;
        let Some(mut f) = self.unit.programs[self.me].files[k].take() else { return Ok(Input::Failed("the file is not open".into())) };
        let text = f.format == Format::Text && !f.is_keyed();
        let read = match f.keyed() {
            Some(keyed) => match keyed.step(false) {
                Ok(Some(found)) => Ok(Record::Data(found.record)),
                Ok(None) => Ok(Record::End),
                Err(code) => Err((code, "there is no next record".to_owned())),
            },
            None => f.read(size).map_err(|e| ("30", e.to_string())),
        };
        self.unit.programs[self.me].files[k] = Some(f);
        let program = self.program;
        let (record, code) = match read {
            Err((code, message)) => {
                self.io_status(k, code, format!("{}: {message}", program.files[k].name), pos)?;
                return Ok(Input::Failed(format!("reading {} failed (file status {code})", program.files[k].name)));
            }
            Ok(Record::End) => {
                self.set_status(k, "10", pos)?;
                return Ok(Input::End);
            }
            Ok(Record::Data(r)) => (r, "00"),
            Ok(Record::WrongLength(r)) => (r, "04"),
        };
        self.set_status(k, code, pos)?;
        if !text {
            return Ok(Input::Record(record));
        }
        let unknown = self.page.encode_char('?').unwrap_or(0x6F);
        Ok(Input::Record(String::from_utf8_lossy(&record).chars().map(|c| self.page.encode_char(c).unwrap_or(unknown)).collect()))
    }

    /// Reads every USING file to its end, in order. A MERGE's files must each be in the merge order.
    fn gather(&mut self, st: &SortStmt, sd: usize, keys: &[Key], files: &[String]) -> R<Result<Vec<Entry>, String>> {
        let pos = st.pos;
        let mut entries: Vec<Entry> = Vec::new();
        for name in files {
            let Some(k) = self.program.files.iter().position(|f| f.name == *name) else {
                return Err(Abend::ironwork(format!("no file named {name}"), pos));
            };
            if let Err(why) = self.open_for_sort(k, OpenMode::Input, pos)? {
                return Ok(Err(why));
            }
            let first = entries.len();
            loop {
                let failure = match self.next_input(k, pos)? {
                    Input::End => break,
                    Input::Failed(why) => why,
                    Input::Record(r) => match self.entry(sd, keys, r, pos)? {
                        Ok(e) => {
                            entries.push(e);
                            continue;
                        }
                        Err(why) => format!("{name}: {why}"),
                    },
                };
                self.close_file(name, pos)?;
                return Ok(Err(failure));
            }
            if let Err(why) = self.close_for_sort(k, pos)? {
                return Ok(Err(why));
            }
            if st.merge
                && let Some(n) = entries[first..].windows(2).position(|w| order(&w[0].keys, &w[1].keys, keys) == Ordering::Greater)
            {
                return Ok(Err(format!("record {} of {name} is out of the merge order (see {})", n + 2, assumptions::MERGE_OUT_OF_SEQUENCE_FAILS)));
            }
        }
        Ok(Ok(entries))
    }

    /// Writes every record to each GIVING file, as WRITE without phrases would.
    fn scatter(&mut self, records: &[Vec<u8>], files: &[String], pos: Pos) -> R<Outcome> {
        for name in files {
            let Some(k) = self.program.files.iter().position(|f| f.name == *name) else {
                return Err(Abend::ironwork(format!("no file named {name}"), pos));
            };
            if let Err(why) = self.open_for_sort(k, OpenMode::Output, pos)? {
                return Ok(Err(why));
            }
            let (area, size) = self.area(k);
            for record in records {
                let len = record.len().min(size);
                self.unit.mem[area..area + len].copy_from_slice(&record[..len]);
                let loc = Loc { offset: area, len, kind: Kind::Alnum { justified: false }, item: usize::MAX };
                self.write_record(k, loc, None, &NO_HANDLERS, pos)?;
                if self.status_failed(k)? {
                    self.close_file(name, pos)?;
                    return Ok(Err(format!("WRITE {name} failed")));
                }
            }
            if let Err(why) = self.close_for_sort(k, pos)? {
                return Ok(Err(why));
            }
        }
        Ok(Ok(()))
    }

    /// Runs an input or output procedure with `active` in progress. An Err outcome is a stop
    /// signalled by RELEASE or RETURN.
    fn run_sort_procedure(&mut self, from: &ProcName, thru: Option<&ProcName>, active: Active, pos: Pos) -> R<(Result<Flow, String>, Option<Active>)> {
        let (start, first_end) = self.procedure(from, pos)?;
        let end = match thru {
            Some(t) => self.procedure(t, pos)?.1,
            None => first_end,
        };
        self.sort = Some(active);
        let nested = self.nest(pos);
        let flow = nested.and_then(|()| {
            let flow = self.procedure_range(start, end);
            self.unit.depth -= 1;
            flow
        });
        let active = self.sort.take();
        match flow {
            Err(a) if a.code == STOPPED => Ok((Err(a.message), active)),
            Err(a) => Err(a),
            Ok(flow) => Ok((Ok(flow), active)),
        }
    }

    /// Paragraphs `start` to `end` as a procedure's range: a GO TO out of it carries on where it
    /// went, and the procedure ends when control falls through the end of paragraph `end`.
    fn procedure_range(&mut self, start: usize, end: usize) -> R<Flow> {
        let last = self.program.paragraphs.len() - 1;
        let mut flow = self.run_paragraphs(start, end)?;
        while let Flow::GoTo(t) = flow {
            flow = if t <= end {
                self.run_paragraphs(t, end)?
            } else {
                match self.run_paragraphs(t, last)? {
                    Flow::Next => Flow::End(Ending::EndOfProgram),
                    other => other,
                }
            };
        }
        Ok(flow)
    }

    fn sort_file(&mut self, st: &'p SortStmt, sd: usize) -> R<Flow> {
        let pos = st.pos;
        let verb = if st.merge { "MERGE" } else { "SORT" };
        if self.sort.is_some() {
            return Err(Abend::ironwork(format!("{verb} {}: another SORT or MERGE is in progress", st.subject.name), pos));
        }
        self.refuse_control_statements(pos)?;
        let sort_return = self.register("SORT-RETURN", pos);
        self.set_integer(&sort_return, 0, pos)?;
        let keys = self.file_keys(sd, &st.keys, pos)?;
        let mut entries = match &st.input {
            Some(SortIo::Files(files)) => match self.gather(st, sd, &keys, files)? {
                Ok(entries) => entries,
                Err(why) => return self.sort_end(st, Err(why)),
            },
            Some(SortIo::Procedure { from, thru }) => {
                let active = Active { sd, keys: keys.clone(), phase: Phase::Input(Vec::new()) };
                let (flow, active) = self.run_sort_procedure(from, thru.as_ref(), active, pos)?;
                match flow {
                    Err(why) => return self.sort_end(st, Err(why)),
                    Ok(Flow::End(e)) => return Ok(Flow::End(e)),
                    Ok(_) => {}
                }
                if self.sort_return(pos)? == 16 {
                    return self.sort_end(st, Err(stopped_by_program()));
                }
                match active.map(|a| a.phase) {
                    Some(Phase::Input(entries)) => entries,
                    _ => Vec::new(),
                }
            }
            None => return Err(Abend::ironwork(format!("{verb} {}: no input", st.subject.name), pos)),
        };
        // A stable sort keeps equal keys in the order they came: SORT_EQUAL_KEYS_IN_ORDER and
        // MERGE_EQUAL_KEYS_BY_FILE in numeric::assumptions.
        entries.sort_by(|a, b| order(&a.keys, &b.keys, &keys));
        let records: Vec<Vec<u8>> = entries.into_iter().map(|e| e.record).collect();
        match &st.output {
            Some(SortIo::Files(files)) => {
                let outcome = self.scatter(&records, files, pos)?;
                self.sort_end(st, outcome)
            }
            Some(SortIo::Procedure { from, thru }) => {
                let active = Active { sd, keys, phase: Phase::Output { records, next: 0 } };
                match self.run_sort_procedure(from, thru.as_ref(), active, pos)?.0 {
                    Err(why) => self.sort_end(st, Err(why)),
                    Ok(Flow::End(e)) => Ok(Flow::End(e)),
                    Ok(_) => self.sort_end(st, Ok(())),
                }
            }
            None => Err(Abend::ironwork(format!("{verb} {}: no output", st.subject.name), pos)),
        }
    }

    /// Sets SORT-RETURN. A failure is reported as DFSORT reports one on SYSOUT, and the run goes on.
    fn sort_end(&mut self, st: &SortStmt, outcome: Outcome) -> R<Flow> {
        let pos = st.pos;
        let code = if outcome.is_ok() { 0 } else { 16 };
        let sort_return = self.register("SORT-RETURN", pos);
        self.set_integer(&sort_return, code, pos)?;
        if let Err(why) = outcome {
            let verb = if st.merge { "MERGE" } else { "SORT" };
            let _ = writeln!(self.unit.err, "ironwork: {pos}: {verb} {} failed: {why}; SORT-RETURN is 16", st.subject.name);
        }
        Ok(Flow::Next)
    }

    fn release(&mut self, record: &Ref, from: Option<&Operand>, pos: Pos) -> R<()> {
        let loc = self.locate(record)?;
        let file = self.layout.items.get(loc.item).and_then(|i| i.file).map(usize::from);
        let Some(Active { sd, keys, phase: Phase::Input(_) }) = &self.sort else {
            return Err(Abend::ironwork(format!("RELEASE {}: no SORT input procedure is running", record.name), pos));
        };
        let (sd, keys) = (*sd, keys.clone());
        if file != Some(sd) {
            return Err(Abend::ironwork(format!("RELEASE {}: not a record of the file being sorted", record.name), pos));
        }
        if self.sort_return(pos)? == 16 {
            return Err(stop(stopped_by_program(), pos));
        }
        if let Some(op) = from {
            let (val, src) = self.operand_with_loc(op, pos)?;
            self.assign(loc, val, src, pos)?;
        }
        let bytes = if self.fixed_length(sd) {
            let (area, size) = self.area(sd);
            self.unit.mem[area..area + size].to_vec()
        } else {
            self.bytes(loc).to_vec()
        };
        match self.entry(sd, &keys, bytes, pos)? {
            Ok(e) => {
                if let Some(Active { phase: Phase::Input(entries), .. }) = &mut self.sort {
                    entries.push(e);
                }
                Ok(())
            }
            Err(why) => Err(stop(format!("RELEASE {}: {why}", record.name), pos)),
        }
    }

    fn return_record(&mut self, file: &str, into: Option<&Ref>, at_end: &'p Handlers, pos: Pos) -> R<Flow> {
        let k = self.program.files.iter().position(|f| f.name == file);
        let Some(Active { sd, phase: Phase::Output { .. }, .. }) = &self.sort else {
            return Err(Abend::ironwork(format!("RETURN {file}: no SORT or MERGE output procedure is running"), pos));
        };
        let sd = *sd;
        if k != Some(sd) {
            return Err(Abend::ironwork(format!("RETURN {file}: not the file being sorted or merged"), pos));
        }
        if self.sort_return(pos)? == 16 {
            return Err(stop(stopped_by_program(), pos));
        }
        let record = match &mut self.sort {
            Some(Active { phase: Phase::Output { records, next }, .. }) if *next < records.len() => {
                *next += 1;
                Some(std::mem::take(&mut records[*next - 1]))
            }
            _ => None,
        };
        let Some(record) = record else {
            return match &at_end.on {
                Some(body) => self.run_block(body),
                None => Ok(Flow::Next),
            };
        };
        let (area, size) = self.area(sd);
        let len = record.len().min(size);
        self.unit.mem[area..area + len].copy_from_slice(&record[..len]);
        if self.fixed_length(sd) {
            self.unit.mem[area + len..area + size].fill(ebcdic::SPACE);
        }
        if let Some(r) = into {
            let dest = self.locate(r)?;
            let bytes = self.unit.mem[area..area + if self.fixed_length(sd) { size } else { len }].to_vec();
            self.assign(dest, Val::Bytes(bytes), None, pos)?;
        }
        match &at_end.not_on {
            Some(body) => self.run_block(body),
            None => Ok(Flow::Next),
        }
    }

    /// A table SORT: the elements are reordered in place by their keys, or by the KEY phrase of the
    /// table's OCCURS when the statement gives none.
    fn sort_table(&mut self, st: &SortStmt) -> R<()> {
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
        let stride = table.size as usize;
        if base + count * stride > self.unit.mem.len() {
            return Err(Abend::ironwork(format!("SORT {} reaches outside the run unit's storage", st.subject.name), pos));
        }
        let named = if st.keys.is_empty() { &table.keys } else { &st.keys };
        let mut keys = Vec::new();
        for (ascending, r) in named {
            let k = crate::sort::table_key(layout, t, &r.name).ok_or_else(|| Abend::ironwork(format!("{} is not a key of {}", r.name, st.subject.name), pos))?;
            let item = &layout.items[k];
            keys.push(Key { ascending: *ascending, offset: item.offset.saturating_sub(table.offset) as usize, len: item.size as usize, kind: item.kind, item: k });
        }
        let mut entries = Vec::with_capacity(count);
        for i in 0..count {
            let at = base + i * stride;
            let values = self.key_values(at, &keys, pos)?;
            entries.push(Entry { record: self.unit.mem[at..at + stride].to_vec(), keys: values });
        }
        entries.sort_by(|a, b| order(&a.keys, &b.keys, &keys));
        for (i, e) in entries.iter().enumerate() {
            let at = base + i * stride;
            self.unit.mem[at..at + stride].copy_from_slice(&e.record);
        }
        Ok(())
    }
}
