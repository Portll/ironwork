//! A program's SORT and MERGE (lir.md §9.6). Records are held in memory: the input phase gathers
//! them from USING files or RELEASE, a stable sort or a MERGE's selection orders them, and the
//! output phase hands them to GIVING files or RETURN. SORT-RETURN reports how the statement ended. A table SORT reorders the
//! table's elements in place.

use super::keys::{Collating, Format, KeyValue, decimal, order};
use crate::abend::{Abend, AbendCode, Ending, Signal};
use crate::fileio::{self, File, Files};
use crate::files::{FileStatus, Format as Records, Move, Record};
use crate::host::{self, Host};
use crate::lir::{FileSort, Organization, SortIo, SortKeys};
use crate::storage::{Kind, Loc, Val};
use crate::store::{self, ProgramFacts};
use crate::vocab::{OpenMode, Pos};
use numeric::precision::{Fixed, Places};
use numeric::{FastsrtAdvPrint, SortKeys as KeyReading, TruncCheck, assumptions};
use std::io::Write;
use std::rc::Rc;
use zarch::ebcdic;

type R<T> = Result<T, Abend>;

/// A key's place in the record (or table element) and how the program reads it.
#[derive(Clone, Debug)]
pub struct ItemKey {
    pub ascending: bool,
    pub offset: usize,
    pub len: usize,
    pub kind: Kind,
    pub item: usize,
    /// The order of an alphanumeric key's characters.
    pub collating: Collating,
}

/// The keys a lowered SORT carries.
pub fn item_keys(keys: &SortKeys) -> Vec<ItemKey> {
    let collating = keys.collating.as_ref().map_or(Collating::Ebcdic, |p| Collating::Positions(Rc::new(**p)));
    keys.keys
        .iter()
        .map(|k| ItemKey {
            ascending: k.ascending,
            offset: k.offset as usize,
            len: k.len as usize,
            kind: k.kind,
            item: k.item as usize,
            collating: if k.collated { collating.clone() } else { Collating::Ebcdic },
        })
        .collect()
}

/// A USING, GIVING or SD file as SELECT and FD declare it.
#[derive(Clone, Copy)]
pub struct SortFile<'a, P, X> {
    pub file: File<'a, P, X>,
    /// No RECORDING MODE V, and its smallest record as long as its largest.
    pub fixed: bool,
    /// FILE STATUS and RELATIVE KEY as written, which the FASTSRT reports name.
    pub status_name: Option<&'a str>,
    pub relative_name: Option<&'a str>,
}

/// An input or output procedure, as DEBUG-ITEM names how control came to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Procedure {
    SortInput,
    SortOutput,
    MergeOutput,
}

impl Procedure {
    pub fn name(self) -> &'static str {
        match self {
            Procedure::SortInput => "SORT INPUT",
            Procedure::SortOutput => "SORT OUTPUT",
            Procedure::MergeOutput => "MERGE OUTPUT",
        }
    }
}

/// What a SORT or MERGE asks of the executor beyond [`Files`]: its handles to the special registers,
/// procedures, files and keys the statement names, the file statements with their EXCEPTION/ERROR
/// procedures, running a procedure, and where the sort in progress is kept.
pub trait SortHost<'a, P: Copy, X: Copy>: Files<P, X> {
    type Register: Copy;
    type Procedure: Copy;
    type File;
    type Keys;
    fn locate_register(&mut self, register: Self::Register) -> R<Loc>;
    fn register_value(&mut self, register: Self::Register, pos: Pos) -> R<i64>;
    fn file_index(&self, file: &Self::File, pos: Pos) -> R<usize>;
    fn keys(&mut self, keys: &Self::Keys, pos: Pos) -> R<Vec<ItemKey>>;
    fn sort_file(&self, k: usize) -> SortFile<'a, P, X>;
    fn active(&mut self) -> &mut Option<Active>;
    /// OPEN of file `k`, as the statement runs it.
    fn open(&mut self, k: usize, mode: OpenMode, pos: Pos) -> R<()>;
    /// CLOSE of file `k`; true when it failed.
    fn close(&mut self, k: usize, pos: Pos) -> R<bool>;
    /// WRITE of the record at `loc` to file `k`, with no phrases; true when it failed.
    fn put(&mut self, k: usize, loc: Loc, pos: Pos) -> R<bool>;
    /// Records a status of file `k`, open in `mode` or being opened in it, and takes the file's
    /// error path when it fails; `open_or_close` whether the SORT or MERGE was opening or closing it.
    fn fail(&mut self, k: usize, status: FileStatus, mode: Option<OpenMode>, open_or_close: bool, message: String, pos: Pos) -> R<()>;
    /// Whether an EXCEPTION/ERROR procedure applies to file `k` in `mode`.
    fn has_error_procedure(&self, k: usize, mode: OpenMode) -> bool;
    /// Runs an input or output procedure; the Ending of a STOP RUN or GOBACK in it.
    fn run_procedure(&mut self, procedure: Self::Procedure, kind: Procedure, pos: Pos) -> R<Option<Ending>>;
    fn err(&mut self) -> &mut dyn Write;
}

/// A SORT or MERGE whose input or output procedure is running.
pub struct Active {
    sd: usize,
    keys: Vec<ItemKey>,
    phase: Phase,
}

enum Phase {
    Input(Vec<Entry>),
    Output { records: Vec<Vec<u8>>, next: usize },
}

struct Entry {
    record: Vec<u8>,
    keys: Vec<KeyValue>,
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

/// How a procedure ended: a STOP RUN or GOBACK in it, or Err with why RELEASE or RETURN stopped the
/// sort.
type Ended = Result<Option<Ending>, String>;

fn stop(why: String, pos: Pos) -> Abend {
    Abend { code: AbendCode::Signal(Signal::SortStopped), message: why, pos, file: None }
}

fn stopped_by_program() -> String {
    format!("SORT-RETURN was set to 16 (see {})", assumptions::SORT_RETURN_STOPS)
}

fn ascending(keys: &[ItemKey]) -> Vec<bool> {
    keys.iter().map(|k| k.ascending).collect()
}

fn set_register<'a, P: Copy, X: Copy, H: SortHost<'a, P, X>>(x: &mut H, register: H::Register, value: i64, pos: Pos) -> R<()> {
    let dest = x.locate_register(register)?;
    x.store_fixed(dest, &Fixed::new(value as i128, Places::new(19, 0)), pos)
}

/// Each key's value, from storage at `base`: a zoned or packed key as DFSORT reads it when
/// `dfsort`, an alphanumeric key by its collating sequence, every other key as the program would
/// read it.
pub fn key_values<P: Copy>(x: &mut impl Host<P>, base: usize, keys: &[ItemKey], dfsort: bool, pos: Pos) -> R<Vec<KeyValue>> {
    let mut out = Vec::with_capacity(keys.len());
    for k in keys {
        let loc = Loc { offset: base + k.offset, len: k.len, kind: k.kind, item: k.item };
        let bytes = store::bytes(x.mem(), loc);
        let value = match (dfsort.then(|| Format::of_decimal(k.kind).and_then(|f| decimal(bytes, f))).flatten(), &k.collating) {
            (Some((negative, digits)), _) => KeyValue::Decimal { negative, digits },
            (None, Collating::Positions(_)) => KeyValue::Collated(k.collating.collate(bytes)),
            (None, Collating::Ebcdic) => KeyValue::Read(host::read(x, loc, pos)?),
        };
        out.push(value);
    }
    Ok(out)
}

/// Sort control statements would change what DFSORT does, and none are read here.
fn refuse_control_statements<'a, P: Copy, X: Copy, H: SortHost<'a, P, X>>(x: &mut H, sort_control: H::Register, pos: Pos) -> R<()> {
    let loc = x.locate_register(sort_control)?;
    let page = x.facts().page();
    let dd = page.decode(store::bytes(x.mem(), loc)).trim().to_ascii_uppercase();
    if !dd.is_empty() && x.dd(&dd).is_some() {
        return Err(Abend::ironwork(format!("DD {dd} holds sort control statements, which ironwork for COBOL does not read"), pos));
    }
    Ok(())
}

/// A record as it enters the sort: fitted to `extent`, the length of the sort's records (the
/// SD's, unless DFSORT reads a longer data set), and keyed at the SD's places from its first
/// byte. See [`assumptions::SORT_RECORD_LENGTHS`] and [`assumptions::FASTSRT_ADV_PRINT`].
fn entry<'a, P: Copy, X: Copy, H: SortHost<'a, P, X>>(x: &mut H, sd: usize, keys: &[ItemKey], mut record: Vec<u8>, extent: usize, pos: Pos) -> R<Result<Entry, String>> {
    let f = x.sort_file(sd);
    let (area, size) = f.file.area;
    if f.fixed {
        record.resize(extent, ebcdic::SPACE);
    } else {
        record.truncate(extent);
    }
    if keys.iter().any(|k| k.offset + k.len > record.len()) {
        return Ok(Err(format!("a record of {} bytes ends inside a key", record.len())));
    }
    let held = record.len().min(size);
    x.mem()[area..area + held].copy_from_slice(&record[..held]);
    let dfsort = x.facts().options().sort_keys == KeyReading::Dfsort;
    let keys = key_values(x, area, keys, dfsort, pos)?;
    Ok(Ok(Entry { record, keys }))
}

/// After a USING or GIVING file fails: with an EXCEPTION/ERROR procedure for it, which has run,
/// the file's processing ends there and the SORT or MERGE goes on unless the procedure set
/// SORT-RETURN to 16 ([`assumptions::SORT_FILE_DECLARATIVE`]); without one it fails.
fn after_failure<'a, P: Copy, X: Copy, H: SortHost<'a, P, X>>(x: &mut H, k: usize, mode: OpenMode, why: String, sort_return: H::Register, pos: Pos) -> R<Outcome> {
    if !x.has_error_procedure(k, mode) {
        return Ok(Err(why));
    }
    if x.register_value(sort_return, pos)? == 16 {
        return Ok(Err(stopped_by_program()));
    }
    Ok(Ok(()))
}

fn is_open<P: Copy, X: Copy>(x: &mut impl Files<P, X>, k: usize) -> bool {
    x.slot(k).is_some()
}

/// Opens a USING or GIVING file, which must not be open already.
fn open_for_sort<'a, P: Copy, X: Copy, H: SortHost<'a, P, X>>(x: &mut H, k: usize, mode: OpenMode, pos: Pos) -> R<Outcome> {
    let name = x.sort_file(k).file.name;
    if is_open(x, k) {
        x.fail(k, FileStatus::AlreadyOpen, Some(mode), true, format!("{name} is open, and a SORT or MERGE opens it itself"), pos)?;
        return Ok(Err(format!("{name} is already open (file status {})", FileStatus::AlreadyOpen.as_str())));
    }
    x.open(k, mode, pos)?;
    if is_open(x, k) { Ok(Ok(())) } else { Ok(Err(format!("OPEN {name} failed"))) }
}

fn close_for_sort<'a, P: Copy, X: Copy, H: SortHost<'a, P, X>>(x: &mut H, k: usize, pos: Pos) -> R<Outcome> {
    let name = x.sort_file(k).file.name;
    if x.close(k, pos)? { Ok(Err(format!("CLOSE {name} failed"))) } else { Ok(Ok(())) }
}

/// Records an I/O status of file k in the mode it is open in.
fn io_status<'a, P: Copy, X: Copy, H: SortHost<'a, P, X>>(x: &mut H, k: usize, status: FileStatus, message: String, pos: Pos) -> R<()> {
    let mode = x.slot(k).as_ref().map(|f| f.mode);
    x.fail(k, status, mode, false, message, pos)
}

/// The next record of an open USING file, with the file status READ would set. A print file's
/// record under ADV keeps the byte before it only when `dfsort` reads it
/// ([`assumptions::FASTSRT_PRINT_RECORDS`]).
fn next_input<'a, P: Copy, X: Copy, H: SortHost<'a, P, X>>(x: &mut H, k: usize, dfsort: bool, pos: Pos) -> R<Input> {
    let file = x.sort_file(k).file;
    let size = file.area.1;
    let Some(mut f) = x.slot(k).take() else { return Ok(Input::Failed("the file is not open".into())) };
    let text = f.format == Records::Text && !f.is_keyed();
    let added = fileio::adds_control_byte(&file, f.format);
    let read = match f.keyed() {
        Some(keyed) => match keyed.step(false) {
            Ok(Some(found)) => Ok(Record::Data(found.record)),
            Ok(None) => Ok(Record::End),
            Err(code) => Err((FileStatus::from(code), "there is no next record".to_owned())),
        },
        None => f.read(size + usize::from(added)).map_err(|e| (FileStatus::PermanentError, e.to_string())),
    };
    *x.slot(k) = Some(f);
    let (record, code) = match read {
        Err((code, message)) => {
            io_status(x, k, code, format!("{}: {message}", file.name), pos)?;
            return Ok(Input::Failed(format!("reading {} failed (file status {})", file.name, code.as_str())));
        }
        Ok(Record::End) => {
            fileio::set_status(x, &file, FileStatus::AtEnd, pos)?;
            return Ok(Input::End);
        }
        Ok(Record::Data(r)) => (r, FileStatus::Success),
        Ok(Record::WrongLength(r)) => (r, FileStatus::SuccessWrongLength),
    };
    fileio::set_status(x, &file, code, pos)?;
    if added && !dfsort {
        return Ok(Input::Record(record.get(1..).unwrap_or_default().to_vec()));
    }
    if !text {
        return Ok(Input::Record(record));
    }
    let page = x.facts().page();
    let unknown = page.encode_char('?').unwrap_or(0x6F);
    Ok(Input::Record(String::from_utf8_lossy(&record).chars().map(|c| page.encode_char(c).unwrap_or(unknown)).collect()))
}

/// What one SORT or MERGE runs with: its SD, keys and FASTSRT plan, and SORT-RETURN.
struct Run<'k, G> {
    sd: usize,
    keys: &'k [ItemKey],
    sort_return: G,
    pos: Pos,
}

/// Reads every USING file to its end, in order: each file's records.
fn gather<'a, P: Copy, X: Copy, H: SortHost<'a, P, X>>(x: &mut H, run: &Run<'_, H::Register>, plan: &[Fastsrt]) -> R<Result<Vec<Vec<Entry>>, String>> {
    let mut files: Vec<Vec<Entry>> = Vec::new();
    for f in plan.iter().filter(|f| f.input) {
        let read = if f.dfsort(x.facts().options().fastsrt) {
            by_dfsort(x, f.file, |x| read_using(x, run, f.file, true))?
        } else {
            read_using(x, run, f.file, false)?
        };
        match read {
            Ok(records) => files.push(records),
            Err(why) => return Ok(Err(why)),
        }
    }
    Ok(Ok(files))
}

/// A stable sort keeps equal keys in the order they came
/// ([`assumptions::SORT_EQUAL_KEYS_IN_ORDER`]).
fn sorted(mut entries: Vec<Entry>, ascending: &[bool]) -> Vec<Entry> {
    entries.sort_by(|a, b| order(&a.keys, &b.keys, ascending));
    entries
}

/// A MERGE's records in the order its selection outputs them: each time the lowest of the records
/// at the head of each file, the earliest file's when keys are equal, whether or not each file is
/// in the merge order ([`assumptions::MERGE_EQUAL_KEYS_BY_FILE`],
/// [`assumptions::MERGE_SEQUENCE_UNCHECKED`]).
fn merged(files: Vec<Vec<Entry>>, ascending: &[bool]) -> Vec<Entry> {
    let mut out = Vec::with_capacity(files.iter().map(Vec::len).sum());
    let mut heads: Vec<std::collections::VecDeque<Entry>> = files.into_iter().map(Into::into).collect();
    while let Some(k) = (0..heads.len()).filter_map(|k| heads[k].front().map(|e| (k, e))).min_by(|(_, a), (_, b)| order(&a.keys, &b.keys, ascending)).map(|(k, _)| k) {
        out.extend(heads[k].pop_front());
    }
    out
}

/// One USING file, opened, read to its end and closed.
fn read_using<'a, P: Copy, X: Copy, H: SortHost<'a, P, X>>(x: &mut H, run: &Run<'_, H::Register>, k: usize, dfsort: bool) -> R<Result<Vec<Entry>, String>> {
    let (pos, sd, keys) = (run.pos, run.sd, run.keys);
    let file = x.sort_file(k).file;
    let name = file.name;
    if let Err(why) = open_for_sort(x, k, OpenMode::Input, pos)? {
        return Ok(after_failure(x, k, OpenMode::Input, why, run.sort_return, pos)?.map(|()| Vec::new()));
    }
    let extent = x.sort_file(sd).file.area.1 + if dfsort { control_byte(x, &file) } else { 0 };
    let mut entries = Vec::new();
    let mut closed = false;
    loop {
        let failure = match next_input(x, k, dfsort, pos)? {
            Input::End => break,
            Input::Failed(why) => {
                x.close(k, pos)?;
                if let Err(why) = after_failure(x, k, OpenMode::Input, why, run.sort_return, pos)? {
                    return Ok(Err(why));
                }
                closed = true;
                break;
            }
            Input::Record(r) => match entry(x, sd, keys, r, extent, pos)? {
                Ok(e) => {
                    entries.push(e);
                    continue;
                }
                Err(why) => format!("{name}: {why}"),
            },
        };
        x.close(k, pos)?;
        return Ok(Err(failure));
    }
    if !closed
        && let Err(why) = close_for_sort(x, k, pos)?
        && let Err(why) = after_failure(x, k, OpenMode::Input, why, run.sort_return, pos)?
    {
        return Ok(Err(why));
    }
    if dfsort && entries.is_empty() && matches!(file.organization, Organization::Indexed | Organization::Relative) {
        return Ok(Err(format!("{name} is an empty VSAM file, which FASTSRT cannot take as input (see {})", assumptions::FASTSRT_FAILURE)));
    }
    Ok(Ok(entries))
}

/// Writes every record to each GIVING file, as WRITE without phrases would, or as DFSORT writes
/// a sequential file's data set.
fn scatter<'a, P: Copy, X: Copy, H: SortHost<'a, P, X>>(x: &mut H, run: &Run<'_, H::Register>, records: &[Vec<u8>], plan: &[Fastsrt]) -> R<Outcome> {
    for f in plan.iter().filter(|f| !f.input) {
        let written = if f.dfsort(x.facts().options().fastsrt) {
            by_dfsort(x, f.file, |x| write_giving(x, run, records, f.file, true))?
        } else {
            write_giving(x, run, records, f.file, false)?
        };
        if written.is_err() {
            return Ok(written);
        }
    }
    Ok(Ok(()))
}

/// One GIVING file, opened, written and closed. COBOL takes each record at the SD's length at
/// most ([`assumptions::FASTSRT_ADV_PRINT`]).
fn write_giving<'a, P: Copy, X: Copy, H: SortHost<'a, P, X>>(x: &mut H, run: &Run<'_, H::Register>, records: &[Vec<u8>], k: usize, dfsort: bool) -> R<Outcome> {
    let pos = run.pos;
    let file = x.sort_file(k).file;
    let name = file.name;
    if let Err(why) = open_for_sort(x, k, OpenMode::Output, pos)? {
        return after_failure(x, k, OpenMode::Output, why, run.sort_return, pos);
    }
    let (area, size) = file.area;
    let limit = size.min(x.sort_file(run.sd).file.area.1);
    for record in records {
        if dfsort && file.organization == Organization::Sequential {
            if let Err(why) = dfsort_put(x, k, record)? {
                x.close(k, pos)?;
                return Ok(Err(why));
            }
            continue;
        }
        let len = record.len().min(limit);
        x.mem()[area..area + len].copy_from_slice(&record[..len]);
        let loc = Loc { offset: area, len, kind: Kind::Alnum { justified: false }, item: usize::MAX };
        if x.put(k, loc, pos)? {
            x.close(k, pos)?;
            return after_failure(x, k, OpenMode::Output, format!("WRITE {name} failed"), run.sort_return, pos);
        }
    }
    match close_for_sort(x, k, pos)? {
        Err(why) => after_failure(x, k, OpenMode::Output, why, run.sort_return, pos),
        closed => Ok(closed),
    }
}

/// 1 when the file's data set holds a byte for ADV's printer control character before each
/// record, which DFSORT reads and writes as part of the record; else 0.
fn control_byte<P: Copy, X: Copy>(x: &impl Files<P, X>, file: &File<'_, P, X>) -> usize {
    usize::from(fileio::adds_control_byte(file, fileio::dd_format(x, file)))
}

/// A record as DFSORT writes it to a sequential GIVING file's data set: with no printer control
/// character, a fixed-length record padded with X'00' or cut to the data set's length, and a
/// variable-length one longer than that a failure. A text DD shows each record as a line. See
/// FASTSRT_PRINT_RECORDS, FASTSRT_RECORD_LENGTHS and FASTSRT_ADV_PRINT in numeric::assumptions.
fn dfsort_put<'a, P: Copy, X: Copy, H: SortHost<'a, P, X>>(x: &mut H, k: usize, record: &[u8]) -> R<Outcome> {
    let file = x.sort_file(k).file;
    let name = file.name;
    let Some(mut f) = x.slot(k).take() else { return Ok(Err(format!("{name} is not open"))) };
    let lrecl = file.area.1 + control_byte(x, &file);
    let written = match f.format {
        Records::Text => {
            let line = x.facts().page().decode(record).trim_end().to_owned();
            f.print(Some(Move::Lines(1)), &line, None)
        }
        Records::Fixed => {
            let mut bytes = record.to_vec();
            bytes.resize(lrecl, 0x00);
            f.write(&bytes)
        }
        Records::Variable if record.len() > lrecl => {
            *x.slot(k) = Some(f);
            let why = format!("a {}-byte record is longer than the largest of GIVING {name}'s data set, {lrecl} bytes: ICE217A (see {})", record.len(), assumptions::FASTSRT_RECORD_LENGTHS);
            return Ok(Err(why));
        }
        Records::Variable => f.write(record),
    };
    *x.slot(k) = Some(f);
    Ok(written.map_err(|e| format!("writing {name} failed: {e}")))
}

/// DFSORT's check of its data sets before it sorts: a fixed-length GIVING data set whose records
/// are longer than the SD's, with no USING data set of DFSORT's own to pad from, fails the SORT
/// ([`assumptions::FASTSRT_ADV_PRINT`]).
fn dfsort_refuses<'a, P: Copy, X: Copy, H: SortHost<'a, P, X>>(x: &H, sd: usize, plan: &[Fastsrt]) -> Option<String> {
    if !x.facts().options().fastsrt || sortin(plan).is_some() {
        return None;
    }
    let length = x.sort_file(sd).file.area.1;
    plan.iter().filter(|f| !f.input && f.refusal.is_none() && fileio::dd_format(x, &x.sort_file(f.file).file) == Records::Fixed).find_map(|f| {
        let file = x.sort_file(f.file).file;
        let lrecl = file.area.1 + control_byte(x, &file);
        (lrecl > length).then(|| {
            format!("GIVING {}'s data set has {lrecl}-byte records, and with no USING file of its own DFSORT does not pad the SD's {length}-byte records: ICE043A (see {})", file.name, assumptions::FASTSRT_ADV_PRINT)
        })
    })
}

/// Runs `op` on file k as DFSORT does its I/O under FASTSRT: the file's FILE STATUS and RELATIVE
/// KEY keep the values they had, and a failure fails the sort rather than the run. See
/// FASTSRT_STATUS and FASTSRT_FAILURE in numeric::assumptions.
fn by_dfsort<'a, P: Copy, X: Copy, H: SortHost<'a, P, X>, T>(x: &mut H, k: usize, op: impl FnOnce(&mut H) -> R<Result<T, String>>) -> R<Result<T, String>> {
    let file = x.sort_file(k).file;
    let mut kept = Vec::new();
    for p in file.status.into_iter().chain(file.relative) {
        let loc = x.locate(p, false)?;
        kept.push((loc, store::bytes(x.mem(), loc).to_vec()));
    }
    let was_open = is_open(x, k);
    let result = match op(x) {
        Err(a) if fileio::is_unhandled_io(&a) => Ok(Err(a.message)),
        other => other,
    };
    if !was_open && let Some(f) = x.slot(k).take() {
        let _ = f.close();
    }
    for (loc, bytes) in kept {
        store::write(x.mem(), loc, &bytes);
    }
    result
}

/// Each USING and GIVING file, and whether IBM's rules let FASTSRT give DFSORT its I/O
/// (FASTSRT_FILES in numeric::assumptions).
fn fastsrt_plan<'a, P: Copy, X: Copy, H: SortHost<'a, P, X>>(x: &H, sort: &FileSort<H::Register, H::Procedure, H::Keys, H::File>, sd: usize, pos: Pos) -> R<Vec<Fastsrt>> {
    let mut plan = Vec::new();
    for (input, io) in [(true, &sort.input), (false, &sort.output)] {
        let Some(SortIo::Files(names)) = io else { continue };
        for name in names {
            let k = x.file_index(name, pos)?;
            let refusal = fastsrt_refusal(x, sort.merge, sd, k, input, names.len(), &plan);
            plan.push(Fastsrt { input, file: k, refusal });
        }
    }
    Ok(plan)
}

fn fastsrt_refusal<'a, P: Copy, X: Copy, H: SortHost<'a, P, X>>(x: &H, merge: bool, sd: usize, k: usize, input: bool, count: usize, earlier: &[Fastsrt]) -> Option<String> {
    let (this, the_sd) = (x.sort_file(k), x.sort_file(sd));
    let decl = this.file;
    let format = |f: &SortFile<'_, P, X>| if f.fixed { "fixed" } else { "variable" };
    let dfsort_reads = |f: &Fastsrt| f.input && f.refusal.is_none() && (f.file == k || x.sort_file(f.file).file.assign == decl.assign);
    let options = x.facts().options();
    Some(if merge {
        "it applies only to SORT".into()
    } else if count > 1 {
        format!("{} names more than one file", if input { "USING" } else { "GIVING" })
    } else if decl.organization == Organization::LineSequential {
        "it is a line-sequential file".into()
    } else if decl.organization == Organization::Relative && !this.fixed {
        "it is a variable-length relative file".into()
    } else if !input && decl.linage.is_some() {
        "its FD has LINAGE".into()
    } else if x.has_error_procedure(k, if input { OpenMode::Input } else { OpenMode::Output }) {
        "an EXCEPTION/ERROR procedure applies to it".into()
    } else if decl.carriage.is_some_and(|c| !c.reserved) && options.fastsrt_adv_print == FastsrtAdvPrint::Exclude {
        format!("it is a print file, whose records ADV makes a byte longer than its FD's {} ({})", decl.area.1, FastsrtAdvPrint::Exclude.flag())
    } else if this.fixed != the_sd.fixed {
        format!("its records are {}-length and the SD's {}-length", format(&this), format(&the_sd))
    } else if decl.area.1 != the_sd.file.area.1 {
        format!("its largest record is {} bytes and the SD's {}", decl.area.1, the_sd.file.area.1)
    } else if !input && earlier.iter().any(dfsort_reads) {
        "it is also the USING file, whose I/O DFSORT does".into()
    } else {
        return None;
    })
}

/// Checked mode: where FASTSRT changes what the program sees of a SORT, and where FASTSRT was
/// asked for and cannot apply.
fn report_fastsrt<'a, P: Copy, X: Copy, H: SortHost<'a, P, X>>(x: &mut H, merge: bool, sd: usize, plan: &[Fastsrt], pos: Pos) {
    let options = x.facts().options();
    if options.trunc_check == TruncCheck::Silent {
        return;
    }
    let verb = if merge { "MERGE" } else { "SORT" };
    let fastsrt = options.fastsrt;
    let mut reports = Vec::new();
    for f in plan {
        let this = x.sort_file(f.file);
        let name = this.file.name;
        let phrase = if f.input { "USING" } else { "GIVING" };
        match &f.refusal {
            Some(why) if fastsrt => reports.push(format!("FASTSRT does not apply to {phrase} {name}: {why}; COBOL does its I/O")),
            Some(_) => {}
            None => {
                if let Some(records) = dfsort_records(x, f, sd, plan) {
                    let does = if fastsrt { "FASTSRT: DFSORT does" } else { "under FASTSRT, DFSORT would do" };
                    reports.push(format!("{does} the I/O of {phrase} {name}{records}"));
                }
                let mut kept: Vec<String> = this.status_name.iter().map(|r| format!("FILE STATUS {r}")).collect();
                if !f.input && this.file.organization == Organization::Relative {
                    kept.extend(this.relative_name.iter().map(|r| format!("RELATIVE KEY {r}")));
                }
                if kept.is_empty() {
                    continue;
                }
                let kept = kept.join(" and ");
                reports.push(if fastsrt {
                    format!("FASTSRT: DFSORT does the I/O of {phrase} {name}, so its {kept} is not updated by the {verb}")
                } else {
                    format!("under FASTSRT, DFSORT would do the I/O of {phrase} {name} and its {kept} would not be updated by the {verb}")
                });
            }
        }
    }
    let sd_name = x.sort_file(sd).file.name;
    for report in reports {
        let _ = writeln!(x.err(), "ironwork: {pos}: {verb} {sd_name}: {report} (-silent stops these reports)");
    }
}

/// How DFSORT's records for file f differ from COBOL's: a print file's control character, and
/// a GIVING data set's record length against the sort's. None when they do not.
fn dfsort_records<'a, P: Copy, X: Copy, H: SortHost<'a, P, X>>(x: &H, f: &Fastsrt, sd: usize, plan: &[Fastsrt]) -> Option<String> {
    let length = x.sort_file(sd).file.area.1;
    let file = x.sort_file(f.file).file;
    let byte = control_byte(x, &file);
    let choice = x.facts().options().fastsrt_adv_print.flag();
    let mut notes = Vec::new();
    match file.carriage {
        Some(c) if c.reserved && !f.input => notes.push(", a print file under NOADV, whose records DFSORT writes as the SD holds them, with no printer control character".to_owned()),
        Some(c) if !c.reserved && byte == 1 && f.input => notes.push(format!(
            ", a print file under ADV taken by {choice}, whose records DFSORT reads as its data set holds them, {} bytes with the printer control character first, so each key is read a byte before where the FD has it",
            length + 1
        )),
        Some(c) if !c.reserved && byte == 1 => notes.push(format!(", a print file under ADV taken by {choice}, whose records DFSORT writes with no printer control character")),
        _ => {}
    }
    if !f.input && fileio::dd_format(x, &file) == Records::Fixed && file.organization == Organization::Sequential {
        let lrecl = file.area.1 + byte;
        let sorted = sortin(plan).map(|u| length + control_byte(x, &x.sort_file(u.file).file));
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
fn run_sort_procedure<'a, P: Copy, X: Copy, H: SortHost<'a, P, X>>(x: &mut H, procedure: H::Procedure, active: Active, kind: Procedure, pos: Pos) -> R<(Ended, Option<Active>)> {
    *x.active() = Some(active);
    let ended = x.run_procedure(procedure, kind, pos);
    let active = x.active().take();
    match ended {
        Err(a) if a.code == AbendCode::Signal(Signal::SortStopped) => Ok((Err(a.message), active)),
        Err(a) => Err(a),
        Ok(ended) => Ok((Ok(ended), active)),
    }
}

/// A SORT or MERGE of a file. A failure is reported as DFSORT reports one on SYSOUT, with
/// SORT-RETURN 16, and the run goes on; a STOP RUN or GOBACK in a procedure ends it.
pub fn sort<'a, P: Copy, X: Copy, H: SortHost<'a, P, X>>(x: &mut H, plan: &FileSort<H::Register, H::Procedure, H::Keys, H::File>, pos: Pos) -> R<Option<Ending>> {
    crate::host::unfollowed(x, "SORT and MERGE");
    let sd = usize::from(plan.sd);
    let name = x.sort_file(sd).file.name;
    let verb = if plan.merge { "MERGE" } else { "SORT" };
    if x.active().is_some() {
        return Err(Abend::ironwork(format!("{verb} {name}: another SORT or MERGE is in progress"), pos));
    }
    refuse_control_statements(x, plan.sort_control, pos)?;
    set_register(x, plan.sort_return, 0, pos)?;
    let keys = x.keys(&plan.keys, pos)?;
    let fastsrt = fastsrt_plan(x, plan, sd, pos)?;
    report_fastsrt(x, plan.merge, sd, &fastsrt, pos);
    let end = |x: &mut H, outcome| sort_end(x, plan.merge, name, plan.sort_return, outcome, pos);
    if let Some(why) = dfsort_refuses(x, sd, &fastsrt) {
        return end(x, Err(why));
    }
    let run = Run { sd, keys: &keys, sort_return: plan.sort_return, pos };
    let up = ascending(&keys);
    let entries = match &plan.input {
        Some(SortIo::Files(_)) => match gather(x, &run, &fastsrt)? {
            Ok(files) if plan.merge => merged(files, &up),
            Ok(files) => sorted(files.into_iter().flatten().collect(), &up),
            Err(why) => return end(x, Err(why)),
        },
        Some(SortIo::Procedure(procedure)) => {
            let active = Active { sd, keys: keys.clone(), phase: Phase::Input(Vec::new()) };
            let (ended, active) = run_sort_procedure(x, *procedure, active, Procedure::SortInput, pos)?;
            match ended {
                Err(why) => return end(x, Err(why)),
                Ok(Some(e)) => return Ok(Some(e)),
                Ok(None) => {}
            }
            if x.register_value(plan.sort_return, pos)? == 16 {
                return end(x, Err(stopped_by_program()));
            }
            match active.map(|a| a.phase) {
                Some(Phase::Input(entries)) => sorted(entries, &up),
                _ => Vec::new(),
            }
        }
        None => return Err(Abend::ironwork(format!("{verb} {name}: no input"), pos)),
    };
    let records: Vec<Vec<u8>> = entries.into_iter().map(|e| e.record).collect();
    match &plan.output {
        Some(SortIo::Files(_)) => {
            let outcome = scatter(x, &run, &records, &fastsrt)?;
            end(x, outcome)
        }
        Some(SortIo::Procedure(procedure)) => {
            let active = Active { sd, keys: keys.clone(), phase: Phase::Output { records, next: 0 } };
            let kind = if plan.merge { Procedure::MergeOutput } else { Procedure::SortOutput };
            match run_sort_procedure(x, *procedure, active, kind, pos)?.0 {
                Err(why) => end(x, Err(why)),
                Ok(Some(e)) => Ok(Some(e)),
                Ok(None) => end(x, Ok(())),
            }
        }
        None => Err(Abend::ironwork(format!("{verb} {name}: no output"), pos)),
    }
}

/// Sets SORT-RETURN. A failure is reported as DFSORT reports one on SYSOUT, and the run goes on.
fn sort_end<'a, P: Copy, X: Copy, H: SortHost<'a, P, X>>(x: &mut H, merge: bool, name: &str, sort_return: H::Register, outcome: Outcome, pos: Pos) -> R<Option<Ending>> {
    let code = if outcome.is_ok() { 0 } else { 16 };
    set_register(x, sort_return, code, pos)?;
    if let Err(why) = outcome {
        let verb = if merge { "MERGE" } else { "SORT" };
        let _ = writeln!(x.err(), "ironwork: {pos}: {verb} {name} failed: {why}; SORT-RETURN is 16");
    }
    Ok(None)
}

/// RELEASE's checks, before FROM moves into the record: an input procedure is running, `file` (the
/// record's) is the one being sorted, and SORT-RETURN does not stop the sort. `name` is the record
/// as written.
pub fn release_ready<'a, P: Copy, X: Copy, H: SortHost<'a, P, X>>(x: &mut H, file: Option<usize>, sort_return: H::Register, name: &str, pos: Pos) -> R<()> {
    crate::host::unfollowed(x, "SORT and MERGE");
    let Some(Active { sd, phase: Phase::Input(_), .. }) = x.active() else {
        return Err(Abend::ironwork(format!("RELEASE {name}: no SORT input procedure is running"), pos));
    };
    if file != Some(*sd) {
        return Err(Abend::ironwork(format!("RELEASE {name}: not a record of the file being sorted"), pos));
    }
    if x.register_value(sort_return, pos)? == 16 {
        return Err(stop(stopped_by_program(), pos));
    }
    Ok(())
}

/// RELEASE of the record at `loc`, once [`release_ready`] has passed: a fixed-length SD's whole
/// record area, else the record.
pub fn release<'a, P: Copy, X: Copy, H: SortHost<'a, P, X>>(x: &mut H, loc: Loc, name: &str, pos: Pos) -> R<()> {
    crate::host::unfollowed(x, "SORT and MERGE");
    let Some(Active { sd, keys, phase: Phase::Input(_) }) = x.active() else {
        return Err(Abend::ironwork(format!("RELEASE {name}: no SORT input procedure is running"), pos));
    };
    let (sd, keys) = (*sd, keys.clone());
    let f = x.sort_file(sd);
    let (area, size) = f.file.area;
    let Ok(loc) = fileio::record_length(x, &f.file, loc, pos)? else {
        return Err(stop(format!("RELEASE {name}: the record's length is outside its RECORD clause"), pos));
    };
    let bytes = if f.fixed { x.mem()[area..area + size].to_vec() } else { store::bytes(x.mem(), loc).to_vec() };
    match entry(x, sd, &keys, bytes, size, pos)? {
        Ok(e) => {
            if let Some(Active { phase: Phase::Input(entries), .. }) = x.active() {
                entries.push(e);
            }
            Ok(())
        }
        Err(why) => Err(stop(format!("RELEASE {name}: {why}"), pos)),
    }
}

/// RETURN of `file` (`name` as written) into its record area, and moved to `into`: true when a
/// record came, false at end.
pub fn return_record<'a, P: Copy, X: Copy, H: SortHost<'a, P, X>>(x: &mut H, file: Option<usize>, into: Option<P>, sort_return: H::Register, name: &str, pos: Pos) -> R<bool> {
    crate::host::unfollowed(x, "SORT and MERGE");
    let Some(Active { sd, phase: Phase::Output { .. }, .. }) = x.active() else {
        return Err(Abend::ironwork(format!("RETURN {name}: no SORT or MERGE output procedure is running"), pos));
    };
    let sd = *sd;
    if file != Some(sd) {
        return Err(Abend::ironwork(format!("RETURN {name}: not the file being sorted or merged"), pos));
    }
    if x.register_value(sort_return, pos)? == 16 {
        return Err(stop(stopped_by_program(), pos));
    }
    let record = match x.active() {
        Some(Active { phase: Phase::Output { records, next }, .. }) if *next < records.len() => {
            *next += 1;
            Some(std::mem::take(&mut records[*next - 1]))
        }
        _ => None,
    };
    let Some(record) = record else { return Ok(false) };
    let f = x.sort_file(sd);
    let (area, size) = f.file.area;
    let len = record.len().min(size);
    x.mem()[area..area + len].copy_from_slice(&record[..len]);
    if f.fixed {
        x.mem()[area + len..area + size].fill(ebcdic::SPACE);
    }
    if let Some(d) = f.file.depending {
        host::set_integer(x, d.item, len as i64, pos)?;
    }
    if let Some(p) = into {
        let dest = x.locate(p, true)?;
        let bytes = x.mem()[area..area + if f.fixed { size } else { len }].to_vec();
        x.assign(dest, Val::Bytes(bytes), None, pos)?;
    }
    Ok(true)
}

/// A table SORT of `count` elements of `stride` bytes from `base`, each moved whole to its place by
/// its keys, which `keys` gives once the table is known to lie in storage. `name` is the table as
/// written.
pub fn sort_table<P: Copy, H: Host<P>>(x: &mut H, base: usize, count: usize, stride: usize, keys: impl FnOnce(&mut H) -> R<Vec<ItemKey>>, name: &str, pos: Pos) -> R<()> {
    crate::host::unfollowed(x, "SORT and MERGE");
    if base + count * stride > x.mem().len() {
        return Err(Abend::ironwork(format!("SORT {name} reaches outside the run unit's storage"), pos));
    }
    let keys = keys(x)?;
    let mut entries = Vec::with_capacity(count);
    for i in 0..count {
        let at = base + i * stride;
        let values = key_values(x, at, &keys, false, pos)?;
        entries.push(Entry { record: x.mem()[at..at + stride].to_vec(), keys: values });
    }
    let up = ascending(&keys);
    entries.sort_by(|a, b| order(&a.keys, &b.keys, &up));
    for (i, e) in entries.iter().enumerate() {
        let at = base + i * stride;
        x.mem()[at..at + stride].copy_from_slice(&e.record);
    }
    Ok(())
}
