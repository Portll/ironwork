//! SORT, MERGE, RELEASE and RETURN. Records are held in memory: the input phase gathers them from
//! USING files or RELEASE, a stable sort orders them, and the output phase hands them to GIVING
//! files or RETURN. A table SORT reorders the table's elements in place.

use super::*;
use crate::files::{Format, Move, Record};
use numeric::{FastsrtAdvPrint, SortKeys, TruncCheck, assumptions};
use std::rc::Rc;

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
#[derive(Clone)]
struct Key {
    ascending: bool,
    offset: usize,
    len: usize,
    kind: Kind,
    item: usize,
    /// Each character's position in the collating sequence of an alphanumeric key, when it is not
    /// EBCDIC.
    positions: Option<Rc<[u8; 256]>>,
}

struct Entry {
    record: Vec<u8>,
    keys: Vec<KeyValue>,
}

/// A key's value as the comparison sees it.
enum KeyValue {
    Read(Val),
    /// A zoned or packed key as DFSORT reads a ZD, PD, CLO, CSL or CST field: its sign, and its
    /// digit nibbles as they stand.
    Decimal { negative: bool, digits: Vec<u8> },
    /// An alphanumeric key as the position of each of its characters in the collating sequence.
    Collated(Vec<u8>),
}

enum Input {
    Record(Vec<u8>),
    End,
    Failed(String),
}

/// A USING or GIVING file of a SORT or MERGE, and why FASTSRT cannot give DFSORT its I/O.
struct Fastsrt {
    input: bool,
    file: usize,
    refusal: Option<String>,
}

impl Fastsrt {
    fn dfsort(&self, fastsrt: bool) -> bool {
        fastsrt && self.refusal.is_none()
    }
}

/// The USING file whose data set DFSORT would read as its SORTIN.
fn sortin(plan: &[Fastsrt]) -> Option<&Fastsrt> {
    plan.iter().find(|f| f.input && f.refusal.is_none())
}

/// How a SORT or MERGE ended: Err holds why it failed.
type Outcome = Result<(), String>;

/// The order of two records by their key values, most significant key first. Every comparison is
/// exact, so the order is total.
fn order(a: &[KeyValue], b: &[KeyValue], keys: &[Key]) -> Ordering {
    for ((x, y), key) in a.iter().zip(b).zip(keys) {
        let o = match (x, y) {
            (KeyValue::Read(Val::Num(x)), KeyValue::Read(Val::Num(y))) => compare_fixed(x, y),
            (KeyValue::Read(Val::Float(x)), KeyValue::Read(Val::Float(y))) => float_order(*x, *y),
            (KeyValue::Read(Val::National(x)), KeyValue::Read(Val::National(y))) => compare_national(x, y),
            (KeyValue::Read(Val::Bytes(x)), KeyValue::Read(Val::Bytes(y))) => ebcdic::compare_alphanumeric(x, y, &Collation::Native),
            (KeyValue::Collated(x), KeyValue::Collated(y)) => x.cmp(y),
            (KeyValue::Decimal { negative: false, digits: x }, KeyValue::Decimal { negative: false, digits: y }) => x.cmp(y),
            (KeyValue::Decimal { negative: true, digits: x }, KeyValue::Decimal { negative: true, digits: y }) => y.cmp(x),
            (KeyValue::Decimal { negative, .. }, KeyValue::Decimal { .. }) => if *negative { Ordering::Less } else { Ordering::Greater },
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

/// A zoned or packed key's sign and digit nibbles as DFSORT reads them, or None for other keys. See
/// SORT_DECIMAL_KEYS, SORT_KEY_INVALID_DIGITS and SORT_NEGATIVE_ZERO in numeric::assumptions.
fn dfsort_decimal(bytes: &[u8], kind: Kind) -> Option<(bool, Vec<u8>)> {
    let negative = |sign: u8| sign % 2 == 1 && sign != 0xF;
    let low = |b: &[u8]| b.iter().map(|b| b & 0x0F).collect();
    match kind {
        Kind::Packed { .. } => {
            let (last, body) = bytes.split_last()?;
            let mut digits: Vec<u8> = body.iter().flat_map(|b| [b >> 4, b & 0x0F]).collect();
            digits.push(last >> 4);
            Some((negative(last & 0x0F), digits))
        }
        Kind::Zoned { sign: Some(SignClause { separate: true, position }), .. } => {
            let (sign, body) = if position == SignPosition::Leading { bytes.split_first()? } else { bytes.split_last()? };
            Some((*sign == 0x60, low(body)))
        }
        Kind::Zoned { sign: Some(SignClause { separate: false, position: SignPosition::Leading }), .. } => Some((negative(bytes.first()? >> 4), low(bytes))),
        Kind::Zoned { .. } => Some((negative(bytes.last()? >> 4), low(bytes))),
        _ => None,
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
    fn file_keys(&mut self, st: &SortStmt, sd: usize, pos: Pos) -> R<Vec<Key>> {
        let area = self.layout.file_areas[sd].0 as usize;
        let positions = self.key_positions(st, true)?;
        let mut out = Vec::new();
        for (ascending, r) in &st.keys {
            let Resolved::Item(i) = self.resolve(r)? else {
                return Err(Abend::ironwork(format!("{} is not a data item", r.name), pos));
            };
            let item = &self.layout.items[i];
            let offset = (item.offset as usize).checked_sub(area).ok_or_else(|| Abend::ironwork(format!("{} is not in the sort file's records", r.name), pos))?;
            let positions = positions.clone().filter(|_| crate::sort::collates(item.kind));
            out.push(Key { ascending: *ascending, offset, len: item.size as usize, kind: item.kind, item: i, positions });
        }
        Ok(out)
    }

    /// The collating sequence of the alphanumeric keys when it is not EBCDIC: the COLLATING
    /// SEQUENCE phrase's, else a file SORT's or MERGE's PROGRAM COLLATING SEQUENCE
    /// (SC27-8713-03, p. 123); see TABLE_SORT_COLLATION in numeric::assumptions for a table SORT.
    fn key_positions(&self, st: &SortStmt, file: bool) -> R<Option<Rc<[u8; 256]>>> {
        let named;
        let sequence = match &st.collating {
            Some(name) => {
                named = crate::collating::Sequence::named(&self.program.environment, name, self.page)
                    .map_err(|m| Abend::ironwork(format!("COLLATING SEQUENCE {name}: {m}"), st.pos))?;
                &named
            }
            None if file => self.collating,
            None => return Ok(None),
        };
        Ok((!sequence.is_native()).then(|| Rc::new(sequence.positions())))
    }

    /// Each key's value, from storage at `base`: a zoned or packed key as DFSORT reads it when
    /// `dfsort`, an alphanumeric key by its collating sequence, every other key as the program
    /// would read it.
    fn key_values(&mut self, base: usize, keys: &[Key], dfsort: bool, pos: Pos) -> R<Vec<KeyValue>> {
        let mut out = Vec::with_capacity(keys.len());
        for k in keys {
            let loc = Loc { offset: base + k.offset, len: k.len, kind: k.kind, item: k.item };
            out.push(match (dfsort.then(|| dfsort_decimal(self.bytes(loc), k.kind)).flatten(), &k.positions) {
                (Some((negative, digits)), _) => KeyValue::Decimal { negative, digits },
                (None, Some(positions)) => KeyValue::Collated(self.bytes(loc).iter().map(|&b| positions[b as usize]).collect()),
                (None, None) => KeyValue::Read(self.read(loc, pos)?),
            });
        }
        Ok(out)
    }

    fn fixed_length(&self, k: usize) -> bool {
        let decl = &self.program.files[k];
        decl.recording != Some('V') && decl.record_min == decl.record_max
    }

    /// A record as it enters the sort: fitted to `extent`, the length of the sort's records (the
    /// SD's, unless DFSORT reads a longer data set), and keyed at the SD's places from its first
    /// byte. See [`assumptions::SORT_RECORD_LENGTHS`] and [`assumptions::FASTSRT_ADV_PRINT`].
    fn entry(&mut self, sd: usize, keys: &[Key], mut record: Vec<u8>, extent: usize, pos: Pos) -> R<Result<Entry, String>> {
        let (area, size) = self.area(sd);
        if self.fixed_length(sd) {
            record.resize(extent, ebcdic::SPACE);
        } else {
            record.truncate(extent);
        }
        if keys.iter().any(|k| k.offset + k.len > record.len()) {
            return Ok(Err(format!("a record of {} bytes ends inside a key", record.len())));
        }
        let held = record.len().min(size);
        self.unit.mem[area..area + held].copy_from_slice(&record[..held]);
        let keys = self.key_values(area, keys, self.options.sort_keys == SortKeys::Dfsort, pos)?;
        Ok(Ok(Entry { record, keys }))
    }

    /// Runs `op`; true when a statement on file k failed in it.
    fn fails(&mut self, k: usize, op: impl FnOnce(&mut Self) -> R<()>) -> R<bool> {
        self.uses.failed = None;
        op(self)?;
        Ok(self.uses.failed == Some(k))
    }

    /// After a USING or GIVING file fails: with an EXCEPTION/ERROR procedure for it, which has run,
    /// the file's processing ends there and the SORT or MERGE goes on unless the procedure set
    /// SORT-RETURN to 16 ([`assumptions::SORT_FILE_DECLARATIVE`]); without one it fails.
    fn after_failure(&mut self, k: usize, mode: OpenMode, why: String, pos: Pos) -> R<Outcome> {
        if self.error_declarative(k, Some(mode)).is_none() {
            return Ok(Err(why));
        }
        if self.sort_return(pos)? == 16 {
            return Ok(Err(stopped_by_program()));
        }
        Ok(Ok(()))
    }

    fn is_open(&self, k: usize) -> bool {
        self.unit.programs[self.me].files[k].is_some()
    }

    /// Opens a USING or GIVING file, which must not be open already.
    fn open_for_sort(&mut self, k: usize, mode: OpenMode, pos: Pos) -> R<Outcome> {
        let program = self.program;
        let name = &program.files[k].name;
        if self.is_open(k) {
            self.io_failure(k, "41", Some(mode), format!("{name} is open, and a SORT or MERGE opens it itself"), pos)?;
            return Ok(Err(format!("{name} is already open (file status 41)")));
        }
        self.open_file(mode, name, pos)?;
        if self.is_open(k) { Ok(Ok(())) } else { Ok(Err(format!("OPEN {name} failed"))) }
    }

    fn close_for_sort(&mut self, k: usize, pos: Pos) -> R<Outcome> {
        let program = self.program;
        let name = &program.files[k].name;
        if self.fails(k, |m| m.close_file(name, pos))? { Ok(Err(format!("CLOSE {name} failed"))) } else { Ok(Ok(())) }
    }

    /// The next record of an open USING file, with the file status READ would set. A print file's
    /// record under ADV keeps the byte before it only when `dfsort` reads it
    /// ([`assumptions::FASTSRT_PRINT_RECORDS`]).
    fn next_input(&mut self, k: usize, dfsort: bool, pos: Pos) -> R<Input> {
        let size = self.area(k).1;
        let Some(mut f) = self.unit.programs[self.me].files[k].take() else { return Ok(Input::Failed("the file is not open".into())) };
        let text = f.format == Format::Text && !f.is_keyed();
        let added = self.adds_control_byte(k, f.format);
        let read = match f.keyed() {
            Some(keyed) => match keyed.step(false) {
                Ok(Some(found)) => Ok(Record::Data(found.record)),
                Ok(None) => Ok(Record::End),
                Err(code) => Err((code, "there is no next record".to_owned())),
            },
            None => f.read(size + usize::from(added)).map_err(|e| ("30", e.to_string())),
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
        if added && !dfsort {
            return Ok(Input::Record(record.get(1..).unwrap_or_default().to_vec()));
        }
        if !text {
            return Ok(Input::Record(record));
        }
        let unknown = self.page.encode_char('?').unwrap_or(0x6F);
        Ok(Input::Record(String::from_utf8_lossy(&record).chars().map(|c| self.page.encode_char(c).unwrap_or(unknown)).collect()))
    }

    /// Reads every USING file to its end, in order. A MERGE's files must each be in the merge order.
    fn gather(&mut self, st: &SortStmt, sd: usize, keys: &[Key], plan: &[Fastsrt]) -> R<Result<Vec<Entry>, String>> {
        let mut entries: Vec<Entry> = Vec::new();
        for f in plan.iter().filter(|f| f.input) {
            let read = if f.dfsort(self.options.fastsrt) {
                self.by_dfsort(f.file, |m| m.read_using(st, sd, keys, f.file, true))?
            } else {
                self.read_using(st, sd, keys, f.file, false)?
            };
            match read {
                Ok(records) => entries.extend(records),
                Err(why) => return Ok(Err(why)),
            }
        }
        Ok(Ok(entries))
    }

    /// One USING file, opened, read to its end and closed.
    fn read_using(&mut self, st: &SortStmt, sd: usize, keys: &[Key], k: usize, dfsort: bool) -> R<Result<Vec<Entry>, String>> {
        let pos = st.pos;
        let program = self.program;
        let name = &program.files[k].name;
        if let Err(why) = self.open_for_sort(k, OpenMode::Input, pos)? {
            return Ok(self.after_failure(k, OpenMode::Input, why, pos)?.map(|()| Vec::new()));
        }
        let extent = self.area(sd).1 + if dfsort { self.control_byte(k) } else { 0 };
        let mut entries = Vec::new();
        let mut closed = false;
        loop {
            let failure = match self.next_input(k, dfsort, pos)? {
                Input::End => break,
                Input::Failed(why) => {
                    self.close_file(name, pos)?;
                    if let Err(why) = self.after_failure(k, OpenMode::Input, why, pos)? {
                        return Ok(Err(why));
                    }
                    closed = true;
                    break;
                }
                Input::Record(r) => match self.entry(sd, keys, r, extent, pos)? {
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
        if !closed
            && let Err(why) = self.close_for_sort(k, pos)?
            && let Err(why) = self.after_failure(k, OpenMode::Input, why, pos)?
        {
            return Ok(Err(why));
        }
        if dfsort && entries.is_empty() && matches!(program.files[k].organization, Organization::Indexed | Organization::Relative) {
            return Ok(Err(format!("{name} is an empty VSAM file, which FASTSRT cannot take as input (see {})", assumptions::FASTSRT_FAILURE)));
        }
        if st.merge
            && let Some(n) = entries.windows(2).position(|w| order(&w[0].keys, &w[1].keys, keys) == Ordering::Greater)
        {
            return Ok(Err(format!("record {} of {name} is out of the merge order (see {})", n + 2, assumptions::MERGE_OUT_OF_SEQUENCE_FAILS)));
        }
        Ok(Ok(entries))
    }

    /// Writes every record to each GIVING file, as WRITE without phrases would, or as DFSORT writes
    /// a sequential file's data set.
    fn scatter(&mut self, records: &[Vec<u8>], sd: usize, plan: &[Fastsrt], pos: Pos) -> R<Outcome> {
        for f in plan.iter().filter(|f| !f.input) {
            let written = if f.dfsort(self.options.fastsrt) {
                self.by_dfsort(f.file, |m| m.write_giving(records, f.file, sd, true, pos))?
            } else {
                self.write_giving(records, f.file, sd, false, pos)?
            };
            if written.is_err() {
                return Ok(written);
            }
        }
        Ok(Ok(()))
    }

    /// One GIVING file, opened, written and closed. COBOL takes each record at the SD's length at
    /// most ([`assumptions::FASTSRT_ADV_PRINT`]).
    fn write_giving(&mut self, records: &[Vec<u8>], k: usize, sd: usize, dfsort: bool, pos: Pos) -> R<Outcome> {
        let program = self.program;
        let name = &program.files[k].name;
        if let Err(why) = self.open_for_sort(k, OpenMode::Output, pos)? {
            return self.after_failure(k, OpenMode::Output, why, pos);
        }
        let (area, size) = self.area(k);
        let limit = size.min(self.area(sd).1);
        for record in records {
            if dfsort && program.files[k].organization == Organization::Sequential {
                if let Err(why) = self.dfsort_put(k, record)? {
                    self.close_file(name, pos)?;
                    return Ok(Err(why));
                }
                continue;
            }
            let len = record.len().min(limit);
            self.unit.mem[area..area + len].copy_from_slice(&record[..len]);
            let loc = Loc { offset: area, len, kind: Kind::Alnum { justified: false }, item: usize::MAX };
            if self.fails(k, |m| m.write_record(k, loc, None, &NO_HANDLERS, pos).map(drop))? {
                self.close_file(name, pos)?;
                return self.after_failure(k, OpenMode::Output, format!("WRITE {name} failed"), pos);
            }
        }
        match self.close_for_sort(k, pos)? {
            Err(why) => self.after_failure(k, OpenMode::Output, why, pos),
            closed => Ok(closed),
        }
    }

    /// 1 when file k's data set holds a byte for ADV's printer control character before each
    /// record, which DFSORT reads and writes as part of the record; else 0.
    fn control_byte(&self, k: usize) -> usize {
        usize::from(self.adds_control_byte(k, self.dd_format(k)))
    }

    /// A record as DFSORT writes it to a sequential GIVING file's data set: with no printer control
    /// character, a fixed-length record padded with X'00' or cut to the data set's length, and a
    /// variable-length one longer than that a failure. A text DD shows each record as a line. See
    /// FASTSRT_PRINT_RECORDS, FASTSRT_RECORD_LENGTHS and FASTSRT_ADV_PRINT in numeric::assumptions.
    fn dfsort_put(&mut self, k: usize, record: &[u8]) -> R<Outcome> {
        let program = self.program;
        let name = &program.files[k].name;
        let Some(mut f) = self.unit.programs[self.me].files[k].take() else { return Ok(Err(format!("{name} is not open"))) };
        let lrecl = self.area(k).1 + self.control_byte(k);
        let written = match f.format {
            Format::Text => {
                let line = self.page.decode(record).trim_end().to_owned();
                f.print(Some(Move::Lines(1)), &line, None)
            }
            Format::Fixed => {
                let mut bytes = record.to_vec();
                bytes.resize(lrecl, 0x00);
                f.write(&bytes)
            }
            Format::Variable if record.len() > lrecl => {
                self.unit.programs[self.me].files[k] = Some(f);
                let why = format!("a {}-byte record is longer than the largest of GIVING {name}'s data set, {lrecl} bytes: ICE217A (see {})", record.len(), assumptions::FASTSRT_RECORD_LENGTHS);
                return Ok(Err(why));
            }
            Format::Variable => f.write(record),
        };
        self.unit.programs[self.me].files[k] = Some(f);
        Ok(written.map_err(|e| format!("writing {name} failed: {e}")))
    }

    /// DFSORT's check of its data sets before it sorts: a fixed-length GIVING data set whose records
    /// are longer than the SD's, with no USING data set of DFSORT's own to pad from, fails the SORT
    /// ([`assumptions::FASTSRT_ADV_PRINT`]).
    fn dfsort_refuses(&self, sd: usize, plan: &[Fastsrt]) -> Option<String> {
        if !self.options.fastsrt || sortin(plan).is_some() {
            return None;
        }
        let length = self.area(sd).1;
        plan.iter().filter(|f| !f.input && f.refusal.is_none() && self.dd_format(f.file) == Format::Fixed).find_map(|f| {
            let lrecl = self.area(f.file).1 + self.control_byte(f.file);
            let name = &self.program.files[f.file].name;
            (lrecl > length).then(|| {
                format!("GIVING {name}'s data set has {lrecl}-byte records, and with no USING file of its own DFSORT does not pad the SD's {length}-byte records: ICE043A (see {})", assumptions::FASTSRT_ADV_PRINT)
            })
        })
    }

    /// Runs `op` on file k as DFSORT does its I/O under FASTSRT: the file's FILE STATUS and RELATIVE
    /// KEY keep the values they had, and a failure fails the sort rather than the run. See
    /// FASTSRT_STATUS and FASTSRT_FAILURE in numeric::assumptions.
    fn by_dfsort<T>(&mut self, k: usize, op: impl FnOnce(&mut Self) -> R<Result<T, String>>) -> R<Result<T, String>> {
        let program = self.program;
        let decl = &program.files[k];
        let mut kept = Vec::new();
        for r in decl.status.iter().chain(&decl.relative_key) {
            let loc = self.locate(r)?;
            kept.push((loc, self.bytes(loc).to_vec()));
        }
        let was_open = self.is_open(k);
        let result = match op(self) {
            Err(a) if a.code.starts_with("IO-") => Ok(Err(a.message)),
            other => other,
        };
        if !was_open && let Some(f) = self.unit.programs[self.me].files[k].take() {
            let _ = f.close();
        }
        for (loc, bytes) in kept {
            self.write(loc, &bytes);
        }
        result
    }

    /// Each USING and GIVING file, and whether IBM's rules let FASTSRT give DFSORT its I/O
    /// (FASTSRT_FILES in numeric::assumptions).
    fn fastsrt_plan(&self, st: &SortStmt, sd: usize, pos: Pos) -> R<Vec<Fastsrt>> {
        let mut plan = Vec::new();
        for (input, io) in [(true, &st.input), (false, &st.output)] {
            let Some(SortIo::Files(names)) = io else { continue };
            for name in names {
                let Some(k) = self.program.files.iter().position(|f| f.name == *name) else {
                    return Err(Abend::ironwork(format!("no file named {name}"), pos));
                };
                let refusal = self.fastsrt_refusal(st, sd, k, input, names.len(), &plan);
                plan.push(Fastsrt { input, file: k, refusal });
            }
        }
        Ok(plan)
    }

    fn fastsrt_refusal(&self, st: &SortStmt, sd: usize, k: usize, input: bool, count: usize, earlier: &[Fastsrt]) -> Option<String> {
        let decl = &self.program.files[k];
        let format = |k: usize| if self.fixed_length(k) { "fixed" } else { "variable" };
        let dfsort_reads = |f: &Fastsrt| f.input && f.refusal.is_none() && (f.file == k || self.program.files[f.file].assign == decl.assign);
        Some(if st.merge {
            "it applies only to SORT".into()
        } else if count > 1 {
            format!("{} names more than one file", if input { "USING" } else { "GIVING" })
        } else if decl.organization == Organization::LineSequential {
            "it is a line-sequential file".into()
        } else if decl.organization == Organization::Relative && !self.fixed_length(k) {
            "it is a variable-length relative file".into()
        } else if !input && decl.linage.is_some() {
            "its FD has LINAGE".into()
        } else if self.error_declarative(k, Some(if input { OpenMode::Input } else { OpenMode::Output })).is_some() {
            "an EXCEPTION/ERROR procedure applies to it".into()
        } else if self.carriage[k].is_some_and(|c| !c.reserved) && self.options.fastsrt_adv_print == FastsrtAdvPrint::Exclude {
            format!("it is a print file, whose records ADV makes a byte longer than its FD's {} ({})", self.area(k).1, FastsrtAdvPrint::Exclude.flag())
        } else if self.fixed_length(k) != self.fixed_length(sd) {
            format!("its records are {}-length and the SD's {}-length", format(k), format(sd))
        } else if self.area(k).1 != self.area(sd).1 {
            format!("its largest record is {} bytes and the SD's {}", self.area(k).1, self.area(sd).1)
        } else if !input && earlier.iter().any(dfsort_reads) {
            "it is also the USING file, whose I/O DFSORT does".into()
        } else {
            return None;
        })
    }

    /// Checked mode: where FASTSRT changes what the program sees of a SORT, and where FASTSRT was
    /// asked for and cannot apply.
    fn report_fastsrt(&mut self, st: &SortStmt, sd: usize, plan: &[Fastsrt]) {
        if self.options.trunc_check == TruncCheck::Silent {
            return;
        }
        let verb = if st.merge { "MERGE" } else { "SORT" };
        let fastsrt = self.options.fastsrt;
        let program = self.program;
        let mut reports = Vec::new();
        for f in plan {
            let decl = &program.files[f.file];
            let phrase = if f.input { "USING" } else { "GIVING" };
            match &f.refusal {
                Some(why) if fastsrt => reports.push(format!("FASTSRT does not apply to {phrase} {}: {why}; COBOL does its I/O", decl.name)),
                Some(_) => {}
                None => {
                    if let Some(records) = self.dfsort_records(f, sd, plan) {
                        let does = if fastsrt { "FASTSRT: DFSORT does" } else { "under FASTSRT, DFSORT would do" };
                        reports.push(format!("{does} the I/O of {phrase} {}{records}", decl.name));
                    }
                    let mut kept: Vec<String> = decl.status.iter().map(|r| format!("FILE STATUS {}", r.name)).collect();
                    if !f.input && decl.organization == Organization::Relative {
                        kept.extend(decl.relative_key.iter().map(|r| format!("RELATIVE KEY {}", r.name)));
                    }
                    if kept.is_empty() {
                        continue;
                    }
                    let kept = kept.join(" and ");
                    reports.push(if fastsrt {
                        format!("FASTSRT: DFSORT does the I/O of {phrase} {}, so its {kept} is not updated by the {verb}", decl.name)
                    } else {
                        format!("under FASTSRT, DFSORT would do the I/O of {phrase} {} and its {kept} would not be updated by the {verb}", decl.name)
                    });
                }
            }
        }
        for report in reports {
            let _ = writeln!(self.unit.err, "ironwork: {}: {verb} {}: {report} (-silent stops these reports)", st.pos, st.subject.name);
        }
    }

    /// How DFSORT's records for file f differ from COBOL's: a print file's control character, and
    /// a GIVING data set's record length against the sort's. None when they do not.
    fn dfsort_records(&self, f: &Fastsrt, sd: usize, plan: &[Fastsrt]) -> Option<String> {
        let length = self.area(sd).1;
        let byte = self.control_byte(f.file);
        let choice = self.options.fastsrt_adv_print.flag();
        let mut notes = Vec::new();
        match self.carriage[f.file] {
            Some(c) if c.reserved && !f.input => notes.push(", a print file under NOADV, whose records DFSORT writes as the SD holds them, with no printer control character".to_owned()),
            Some(c) if !c.reserved && byte == 1 && f.input => notes.push(format!(
                ", a print file under ADV taken by {choice}, whose records DFSORT reads as its data set holds them, {} bytes with the printer control character first, so each key is read a byte before where the FD has it",
                length + 1
            )),
            Some(c) if !c.reserved && byte == 1 => notes.push(format!(", a print file under ADV taken by {choice}, whose records DFSORT writes with no printer control character")),
            _ => {}
        }
        if !f.input && self.dd_format(f.file) == Format::Fixed && self.program.files[f.file].organization == Organization::Sequential {
            let lrecl = self.area(f.file).1 + byte;
            let sorted = sortin(plan).map(|u| length + self.control_byte(u.file));
            match sorted {
                Some(r) if lrecl > r => notes.push(format!("; DFSORT pads each record with X'00' to the data set's {lrecl} bytes (ICE171I)")),
                Some(r) if lrecl < r => notes.push(format!("; DFSORT cuts each {r}-byte record to the data set's {lrecl} bytes (ICE171I)")),
                _ => {}
            }
        }
        (!notes.is_empty()).then(|| notes.concat())
    }

    /// Runs an input or output procedure with `active` in progress. An Err outcome is a stop
    /// signalled by RELEASE or RETURN.
    fn run_sort_procedure(&mut self, from: &ProcName, thru: Option<&ProcName>, active: Active, arrival: declaratives::Arrival, pos: Pos) -> R<(Result<Flow, String>, Option<Active>)> {
        let (start, first_end) = self.procedure(from, pos)?;
        let end = match thru {
            Some(t) => self.procedure(t, pos)?.1,
            None => first_end,
        };
        self.sort = Some(active);
        let nested = self.nest(pos);
        let flow = nested.and_then(|()| {
            let flow = self.procedure_range(start, end, arrival);
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
    fn procedure_range(&mut self, start: usize, end: usize, arrival: declaratives::Arrival) -> R<Flow> {
        let last = self.program.paragraphs.len() - 1;
        self.uses.arrival = arrival;
        let mut flow = self.run_paragraphs(start, end)?;
        while let Flow::GoTo(t) = flow {
            self.uses.arrival = declaratives::Arrival::GoTo;
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
        let keys = self.file_keys(st, sd, pos)?;
        let plan = self.fastsrt_plan(st, sd, pos)?;
        self.report_fastsrt(st, sd, &plan);
        if let Some(why) = self.dfsort_refuses(sd, &plan) {
            return self.sort_end(st, Err(why));
        }
        let mut entries = match &st.input {
            Some(SortIo::Files(_)) => match self.gather(st, sd, &keys, &plan)? {
                Ok(entries) => entries,
                Err(why) => return self.sort_end(st, Err(why)),
            },
            Some(SortIo::Procedure { from, thru }) => {
                let active = Active { sd, keys: keys.clone(), phase: Phase::Input(Vec::new()) };
                let (flow, active) = self.run_sort_procedure(from, thru.as_ref(), active, declaratives::Arrival::Sort("SORT INPUT"), pos)?;
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
            Some(SortIo::Files(_)) => {
                let outcome = self.scatter(&records, sd, &plan, pos)?;
                self.sort_end(st, outcome)
            }
            Some(SortIo::Procedure { from, thru }) => {
                let active = Active { sd, keys, phase: Phase::Output { records, next: 0 } };
                let procedure = if st.merge { "MERGE OUTPUT" } else { "SORT OUTPUT" };
                match self.run_sort_procedure(from, thru.as_ref(), active, declaratives::Arrival::Sort(procedure), pos)?.0 {
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
        let mut loc = if from.is_some() { self.locate_receiving(record)? } else { self.locate(record)? };
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
            loc = self.locate(record)?;
        }
        let bytes = if self.fixed_length(sd) {
            let (area, size) = self.area(sd);
            self.unit.mem[area..area + size].to_vec()
        } else {
            self.bytes(loc).to_vec()
        };
        let extent = self.area(sd).1;
        match self.entry(sd, &keys, bytes, extent, pos)? {
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
            let dest = self.locate_receiving(r)?;
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
        let positions = self.key_positions(st, false)?;
        let mut keys = Vec::new();
        for (ascending, r) in named {
            let k = crate::sort::table_key(layout, t, &r.name).ok_or_else(|| Abend::ironwork(format!("{} is not a key of {}", r.name, st.subject.name), pos))?;
            let item = &layout.items[k];
            let positions = positions.clone().filter(|_| crate::sort::collates(item.kind));
            keys.push(Key { ascending: *ascending, offset: item.offset.saturating_sub(table.offset) as usize, len: item.size as usize, kind: item.kind, item: k, positions });
        }
        let mut entries = Vec::with_capacity(count);
        for i in 0..count {
            let at = base + i * stride;
            let values = self.key_values(at, &keys, false, pos)?;
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
