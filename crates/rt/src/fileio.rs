//! The file statements (lir.md §9.4): OPEN, CLOSE, READ, WRITE, REWRITE, DELETE and START on one
//! file, FILE STATUS, LINAGE and the print carriage. A sequential file streams through its DD; an
//! indexed or relative file, or a sequential file opened I-O, is held in memory ([`Keyed`]). Each
//! verb returns the status its phrases or the file's error path read; the executor runs those.

use crate::abend::Abend;
use crate::files::{self, Dd, FileStatus, Format, Keyed, Keying, Move, Open, Record};
use crate::host::{self, Host};
use crate::linage::{Geometry, Motion, Page};
use crate::lir::{Access, Carriage, Organization, Spacing, StartRel};
use crate::printer::{self, Controls};
use crate::storage::{Loc, Val};
use crate::store::{self, ProgramFacts};
use crate::unit::Event;
use crate::vocab::{Closing, OpenMode, Pos};
use numeric::precision::{Fixed, Places};
use std::cmp::Ordering;
use zarch::ebcdic;

type R<T> = Result<T, Abend>;

/// A file as SELECT and FD declare it ([`crate::lir::FileDesc`]), with the executor's handles: `P`
/// a data item, `X` an integer it evaluates. Its keys are the executor's to resolve
/// ([`Files::keying`]).
#[derive(Clone, Copy)]
pub struct File<'a, P, X> {
    pub index: usize,
    pub name: &'a str,
    /// The DD name ASSIGN gives.
    pub assign: &'a str,
    pub organization: Organization,
    pub access: Access,
    pub optional: bool,
    /// How records are held when the DD does not say.
    pub format: Format,
    pub status: Option<P>,
    pub relative: Option<P>,
    pub linage: Option<Linage<X>>,
    pub carriage: Option<Carriage>,
    /// Where the record area is in run-unit memory, and its length.
    pub area: (usize, usize),
    /// The shortest and longest variable-length record a READ takes without a record length
    /// conflict ([`crate::lir::FileDesc::read_lengths`]).
    pub read_lengths: (usize, usize),
    pub depending: Option<Depending<P>>,
}

/// RECORD IS VARYING DEPENDING ON: the item holding a record's length, and the shortest and longest
/// record the clause allows.
#[derive(Clone, Copy)]
pub struct Depending<P> {
    pub item: P,
    pub lengths: (usize, usize),
}

/// LINAGE's values, evaluated in this order whenever the page's geometry is taken, and
/// LINAGE-COUNTER.
#[derive(Clone, Copy)]
pub struct Linage<X> {
    pub lines: X,
    pub footing: Option<X>,
    pub top: Option<X>,
    pub bottom: Option<X>,
    pub counter: Option<Loc>,
}

/// READ ([`crate::lir::FileVerb::Read`]). `key` is READ ... KEY IS.
#[derive(Clone, Copy)]
pub struct Read<P> {
    pub sequential: bool,
    pub previous: bool,
    pub into: Option<P>,
    pub key: Option<P>,
}

/// WRITE's ADVANCING phrase ([`crate::lir::Advance`]). A mnemonic-name's `space` is None when its
/// environment-name is no printer channel.
#[derive(Clone, Copy)]
pub enum Advance<'a, X> {
    Lines { before: bool, count: X },
    Page { before: bool },
    Mnemonic { before: bool, space: Option<Spacing>, name: &'a str, environment: &'a str },
}

/// How a verb ended.
pub enum Outcome {
    /// FILE STATUS holds its status and nothing is left to run.
    Done,
    /// The status the statement's AT END (`at_end`) or INVALID KEY phrase, or else the file's
    /// error path, reads.
    Status { status: FileStatus, at_end: bool },
    /// A WRITE to a LINAGE file, with FILE STATUS set: END-OF-PAGE or NOT END-OF-PAGE runs.
    Page { end_of_page: bool },
    /// A failing status that takes the file's error path at once.
    Failed(Failure),
}

/// `mode` is the mode the file is open in, or being opened in.
pub struct Failure {
    pub status: FileStatus,
    pub mode: Option<OpenMode>,
    pub message: String,
}

/// What the file verbs ask of the executor beyond [`Host`].
pub trait Files<P: Copy, X: Copy>: Host<P> {
    /// The program's file `k` while it is open.
    fn slot(&mut self, k: usize) -> &mut Option<Open>;
    /// Whether CLOSE WITH LOCK has closed file `k`.
    fn locked(&mut self, k: usize) -> &mut bool;
    fn dd(&self, assign: &str) -> Option<Dd>;
    fn notify(&mut self, event: Event<'_>);
    fn int(&mut self, value: X, pos: Pos) -> R<i64>;
    /// How file `k` finds its records by key.
    fn keying(&mut self, k: usize, pos: Pos) -> R<Keying>;
    /// Which key of indexed file `k` the item names (0 the prime key, then each alternate), and
    /// its value in the record area. With `partial` (START) it may be a leading part of the key.
    fn key_value(&mut self, k: usize, keying: &Keying, key: P, partial: bool, pos: Pos) -> R<(usize, Vec<u8>)>;
}

pub fn set_status<P: Copy, X: Copy>(x: &mut impl Files<P, X>, file: &File<'_, P, X>, status: FileStatus, pos: Pos) -> R<()> {
    if let Some(p) = file.status {
        let loc = x.locate(p, false)?;
        let bytes = x.facts().page().encode(status.as_str()).map_err(|e| Abend::ironwork(e.to_string(), pos))?;
        x.assign(loc, Val::Bytes(bytes), None, pos)?;
    }
    Ok(())
}

/// A failure in the mode file `k` is open in now.
fn failed<P: Copy, X: Copy>(x: &mut impl Files<P, X>, file: &File<'_, P, X>, status: FileStatus, message: String) -> Outcome {
    let mode = x.slot(file.index).as_ref().map(|f| f.mode);
    Outcome::Failed(Failure { status, mode, message })
}

pub fn sequential<P, X>(file: &File<'_, P, X>) -> bool {
    file.access == Access::Sequential || file.organization == Organization::Sequential
}

/// Whether the file's DD holds a byte before each record for the printer control character: a
/// print file under ADV, unless the DD is text, which shows the character as line spacing.
pub fn adds_control_byte<P, X>(file: &File<'_, P, X>, format: Format) -> bool {
    file.carriage.is_some_and(|c| !c.reserved) && format != Format::Text
}

/// How the file's DD holds its records.
pub fn dd_format<P: Copy, X: Copy>(x: &impl Files<P, X>, file: &File<'_, P, X>) -> Format {
    x.dd(file.assign).and_then(|d| d.format).unwrap_or(file.format)
}

fn record_area<P: Copy>(x: &mut impl Host<P>, (offset, size): (usize, usize)) -> &[u8] {
    &x.mem()[offset..offset + size]
}

fn relative_value<P: Copy, X: Copy>(x: &mut impl Files<P, X>, file: &File<'_, P, X>, pos: Pos) -> R<i64> {
    let r = file.relative.ok_or_else(|| Abend::ironwork(format!("{} has no RELATIVE KEY", file.name), pos))?;
    x.integer(r, pos)
}

/// The RELATIVE KEY's record number as a key, or None when it is below 1.
fn relative_number<P: Copy, X: Copy>(x: &mut impl Files<P, X>, file: &File<'_, P, X>, pos: Pos) -> R<Option<Vec<u8>>> {
    let n = relative_value(x, file, pos)?;
    Ok((n >= 1).then(|| files::record_number(n as u64)))
}

/// Whether record number `n` fits the RELATIVE KEY item, as sequential READ and WRITE store it
/// there.
fn relative_fits<P: Copy, X: Copy>(x: &mut impl Files<P, X>, file: &File<'_, P, X>, n: u64) -> R<bool> {
    let Some(r) = file.relative else { return Ok(true) };
    let loc = x.locate(r, false)?;
    Ok(match loc.kind.digits_scale() {
        Some((digits, scale)) => digits.saturating_sub(scale) >= 19 || n < 10u64.pow(digits.saturating_sub(scale)),
        None => true,
    })
}

/// Runs `op` on file `k` when it is open and held in memory.
fn held<P: Copy, X: Copy, F: Files<P, X>, T>(x: &mut F, k: usize, op: impl FnOnce(&mut F, OpenMode, Format, &mut Keyed) -> R<T>) -> R<Option<T>> {
    let Some(mut f) = x.slot(k).take() else { return Ok(None) };
    let (mode, format) = (f.mode, f.format);
    let result = match f.keyed() {
        Some(keyed) => op(x, mode, format, keyed).map(Some),
        None => Ok(None),
    };
    *x.slot(k) = Some(f);
    result
}

fn is_held<P: Copy, X: Copy>(x: &mut impl Files<P, X>, k: usize) -> bool {
    x.slot(k).as_ref().is_some_and(|f| f.is_keyed())
}

/// Whether the file is a print file opened I-O whose records hold the byte ADV adds
/// ([`numeric::assumptions::PRINT_FILE_UPDATE`]).
fn held_control_byte<P: Copy, X: Copy>(x: &mut impl Files<P, X>, file: &File<'_, P, X>) -> bool {
    x.slot(file.index).as_ref().is_some_and(|f| f.is_keyed() && adds_control_byte(file, f.format))
}

/// A record as WRITE or REWRITE puts it in the file: a fixed record at the length of the area.
fn record_bytes<P: Copy, X: Copy>(x: &mut impl Files<P, X>, file: &File<'_, P, X>, loc: Loc, format: Format) -> Vec<u8> {
    let mut bytes = store::bytes(x.mem(), loc).to_vec();
    if format != Format::Variable {
        bytes.resize(file.area.1.max(bytes.len()), ebcdic::SPACE);
    }
    bytes
}

/// The record WRITE, REWRITE or RELEASE puts out: under DEPENDING ON the record area's first n
/// bytes, n the item's value, or status 44 when n is outside the clause's lengths (Language
/// Reference SC27-8713-03, pp. 188, 302).
pub fn record_length<P: Copy, X: Copy>(x: &mut impl Host<P>, file: &File<'_, P, X>, loc: Loc, pos: Pos) -> R<Result<Loc, FileStatus>> {
    let Some(d) = file.depending else { return Ok(Ok(loc)) };
    let n = x.integer(d.item, pos)?;
    let (shortest, longest) = d.lengths;
    if n < shortest as i64 || n > longest.min(file.area.1) as i64 {
        return Ok(Err(FileStatus::RecordLengthChanged));
    }
    Ok(Ok(Loc { offset: file.area.0, len: n as usize, ..loc }))
}

/// Moves a record into the file's area, and to INTO's item; a variable-length record fills only
/// its own length, which DEPENDING ON's item receives. True when the record was longer than the
/// area.
fn deliver<P: Copy, X: Copy>(x: &mut impl Files<P, X>, file: &File<'_, P, X>, record: &[u8], variable: bool, into: Option<P>, pos: Pos) -> R<bool> {
    let (offset, size) = file.area;
    let n = record.len().min(size);
    let mem = x.mem();
    mem[offset..offset + n].copy_from_slice(&record[..n]);
    if !variable {
        mem[offset + n..offset + size].fill(ebcdic::SPACE);
    }
    if let Some(t) = x.taint() {
        t.set(offset, n, true);
        t.set(offset + n, if variable { 0 } else { size - n }, false);
    }
    if let Some(d) = file.depending {
        host::set_integer(x, d.item, n as i64, pos)?;
    }
    if let Some(r) = into {
        let dest = x.locate(r, true)?;
        let moved = if variable { n } else { size };
        if let Some(t) = x.taint() {
            t.read(offset, moved);
        }
        let bytes = x.mem()[offset..offset + moved].to_vec();
        x.assign(dest, Val::Bytes(bytes), None, pos)?;
    }
    Ok(record.len() > size)
}

/// Whether a record READ delivered has a record length conflict, status 04: it was longer than the
/// record area, or it is a variable-length record outside the file's read lengths
/// ([`numeric::assumptions::VLR_RECORDS_CHECKED`]).
fn length_conflict<P, X>(file: &File<'_, P, X>, len: usize, variable: bool, long: bool) -> bool {
    let (shortest, longest) = file.read_lengths;
    long || variable && !(shortest..=longest).contains(&len)
}

/// OPEN: a file whose data set is unavailable is status 35, or 05 when it is OPTIONAL, which
/// OPEN EXTEND then creates (Language Reference SC27-8713-03, pp. 300-301).
pub fn open<P: Copy, X: Copy>(x: &mut impl Files<P, X>, file: &File<'_, P, X>, mode: OpenMode, pos: Pos) -> R<Outcome> {
    let (k, name) = (file.index, file.name);
    let failure = |status, message| Ok(Outcome::Failed(Failure { status, mode: Some(mode), message }));
    if *x.locked(k) {
        return failure(FileStatus::ClosedWithLock, format!("{name} was closed WITH LOCK"));
    }
    if x.slot(k).is_some() {
        return failure(FileStatus::AlreadyOpen, format!("{name} is already open"));
    }
    let page = match (&file.linage, mode) {
        (Some(_), OpenMode::Output | OpenMode::Extend) => Some(Page::opened(geometry(x, file, pos)?)),
        _ => None,
    };
    let default = file.format;
    let dd = x.dd(file.assign);
    if let Some(d) = &dd {
        x.notify(Event::Open { dd: file.assign, mode, path: &d.path });
    }
    let no_dd = format!("{name}: no DD {} was given (--dd {}=path)", file.assign, file.assign);
    let held = match file.organization {
        Organization::Indexed | Organization::Relative => true,
        Organization::Sequential => mode == OpenMode::InputOutput,
        Organization::LineSequential => false,
    };
    if held {
        let status = match &dd {
            Some(d) if mode == OpenMode::Output || d.path.exists() => FileStatus::Success,
            _ if file.optional && mode != OpenMode::Output => FileStatus::SuccessOptional,
            None => return failure(FileStatus::FileNotFound, no_dd),
            Some(d) => return failure(FileStatus::FileNotFound, format!("{name}: {}: no such file", d.path.display())),
        };
        let keying = x.keying(k, pos)?;
        let format = dd.as_ref().and_then(|d| d.format).unwrap_or(default);
        let record_len = file.area.1 + usize::from(adds_control_byte(file, format));
        return match files::open_keyed(dd.as_ref(), mode, format, keying, record_len, x.facts().page()) {
            Ok(f) => opened(x, file, f, status, pos),
            Err(e) => failure(FileStatus::PermanentError, format!("{name}: {e}")),
        };
    }
    match dd {
        _ if file.optional && mode == OpenMode::Input && dd.as_ref().is_none_or(|d| !d.path.exists()) => opened(x, file, files::absent(), FileStatus::SuccessOptional, pos),
        None => failure(FileStatus::FileNotFound, no_dd),
        Some(dd) if mode == OpenMode::Extend && !file.optional && !dd.path.exists() => failure(FileStatus::FileNotFound, format!("{name}: {}: no such file", dd.path.display())),
        Some(dd) => {
            let created = mode == OpenMode::Extend && !dd.path.exists();
            match files::open(&dd, mode, dd.format.unwrap_or(default)) {
                Ok(mut f) => {
                    f.page = page;
                    opened(x, file, f, if created { FileStatus::SuccessOptional } else { FileStatus::Success }, pos)
                }
                Err(e) => {
                    let status = match e.kind() {
                        std::io::ErrorKind::NotFound => FileStatus::FileNotFound,
                        std::io::ErrorKind::Unsupported => FileStatus::OpenModeUnsupported,
                        _ => FileStatus::PermanentError,
                    };
                    failure(status, format!("{name}: {}: {e}", dd.path.display()))
                }
            }
        }
    }
}

/// Holds the file open; OPEN sets a LINAGE file's LINAGE-COUNTER to 1 (Language Reference
/// SC27-8713-03, p. 24).
fn opened<P: Copy, X: Copy>(x: &mut impl Files<P, X>, file: &File<'_, P, X>, f: Open, status: FileStatus, pos: Pos) -> R<Outcome> {
    *x.slot(file.index) = Some(f);
    set_linage_counter(x, file, 1, pos)?;
    set_status(x, file, status, pos)?;
    Ok(Outcome::Done)
}

/// The page the file's LINAGE clause gives as its data items stand now.
fn geometry<P: Copy, X: Copy>(x: &mut impl Files<P, X>, file: &File<'_, P, X>, pos: Pos) -> R<Geometry> {
    let Some(linage) = file.linage else { return Err(Abend::ironwork(format!("{} has no LINAGE clause", file.name), pos)) };
    let body = x.int(linage.lines, pos)?;
    let footing = match linage.footing {
        Some(v) => Some(x.int(v, pos)?),
        None => None,
    };
    let top = match linage.top {
        Some(v) => x.int(v, pos)?,
        None => 0,
    };
    let bottom = match linage.bottom {
        Some(v) => x.int(v, pos)?,
        None => 0,
    };
    Geometry::new(body, footing, top, bottom).map_err(|why| Abend::ironwork(format!("{}: {why} ({})", file.name, numeric::assumptions::LINAGE_VALUES), pos))
}

fn set_linage_counter<P: Copy, X: Copy>(x: &mut impl Files<P, X>, file: &File<'_, P, X>, value: u64, pos: Pos) -> R<()> {
    let Some(loc) = file.linage.and_then(|l| l.counter) else { return Ok(()) };
    x.store_fixed(loc, &Fixed::new(value as i128, Places::new(19, 0)), pos)
}

/// CLOSE, and its phrases as IBM's table has them for a file on a medium without reels or units
/// (Language Reference for Enterprise COBOL 6.4, 'Effect of CLOSE statement on file types'): REEL
/// or UNIT leaves the file open and sets status 07, NO REWIND closes it with 07, and LOCK closes it
/// so that OPEN refuses it with 38.
pub fn close<P: Copy, X: Copy>(x: &mut impl Files<P, X>, file: &File<'_, P, X>, closing: Option<Closing>, pos: Pos) -> R<Outcome> {
    let name = file.name;
    if closing == Some(Closing::Volume) && x.slot(file.index).is_some() {
        set_status(x, file, FileStatus::SuccessNonReel, pos)?;
        return Ok(Outcome::Done);
    }
    match x.slot(file.index).take() {
        None => Ok(failed(x, file, FileStatus::NotOpen, format!("{name} is not open"))),
        Some(f) => {
            let mode = Some(f.mode);
            match f.close() {
                Ok(()) => {
                    if let Some(d) = x.dd(file.assign) {
                        x.notify(Event::Close { dd: file.assign, path: &d.path });
                    }
                    *x.locked(file.index) |= closing == Some(Closing::Lock);
                    let status = if closing == Some(Closing::NoRewind) { FileStatus::SuccessNonReel } else { FileStatus::Success };
                    set_status(x, file, status, pos)?;
                    Ok(Outcome::Done)
                }
                Err(e) => Ok(Outcome::Failed(Failure { status: FileStatus::PermanentError, mode, message: format!("{name}: {e}") })),
            }
        }
    }
}

/// READ of a held file by key or in sequence, else of a stream, whose phrase is AT END.
pub fn read<P: Copy, X: Copy>(x: &mut impl Files<P, X>, file: &File<'_, P, X>, r: Read<P>, pos: Pos) -> R<Outcome> {
    let k = file.index;
    if !is_held(x, k) {
        return read_stream(x, file, r.into, pos);
    }
    let sequential = r.sequential;
    let added = held_control_byte(x, file);
    let (status, found, variable) = held(x, k, |x, mode, format, keyed| {
        let variable = format == Format::Variable;
        if !matches!(mode, OpenMode::Input | OpenMode::InputOutput) {
            return Ok((FileStatus::NotOpenInput, None, variable));
        }
        if sequential {
            return Ok(match keyed.read_next(r.previous) {
                Err(status) => (status, None, variable),
                Ok(None) => (FileStatus::AtEnd, None, variable),
                Ok(Some(found)) if keyed.keying == Keying::Relative && !relative_fits(x, file, files::number_of(&found.key))? => (FileStatus::RelativeKeyOverflow, None, variable),
                Ok(Some(found)) => (if found.duplicate { FileStatus::SuccessDuplicate } else { FileStatus::Success }, Some(found), variable),
            });
        }
        let keying = keyed.keying.clone();
        let (which, value) = match (&keying, r.key) {
            (Keying::Indexed { .. }, Some(key)) => x.key_value(k, &keying, key, false, pos)?,
            (Keying::Indexed { prime, .. }, None) => (0, prime.of(record_area(x, file.area))),
            _ => match relative_number(x, file, pos)? {
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
        if sequential && file.organization == Organization::Relative
            && let Some(rk) = file.relative
        {
            host::set_integer(x, rk, files::number_of(&found.key) as i64, pos)?;
        }
        let record = found.record.get(usize::from(added)..).unwrap_or_default();
        let long = deliver(x, file, record, variable, r.into, pos)?;
        if length_conflict(file, record.len(), variable, long) && status == FileStatus::Success {
            status = FileStatus::SuccessWrongLength;
        }
    }
    Ok(Outcome::Status { status, at_end: sequential })
}

fn read_stream<P: Copy, X: Copy>(x: &mut impl Files<P, X>, file: &File<'_, P, X>, into: Option<P>, pos: Pos) -> R<Outcome> {
    let k = file.index;
    let at_end = |status| Ok(Outcome::Status { status, at_end: true });
    let size = file.area.1;
    let Some(mut f) = x.slot(k).take() else {
        return at_end(FileStatus::NotOpenInput);
    };
    let added = adds_control_byte(file, f.format);
    let read = f.read(size + usize::from(added));
    let format = f.format;
    let input = f.mode == OpenMode::Input;
    *x.slot(k) = Some(f);
    if !input {
        return at_end(FileStatus::NotOpenInput);
    }
    let (record, wrong_length) = match read {
        Err(e) => return Ok(failed(x, file, FileStatus::PermanentError, format!("READ {}: {e}", file.name))),
        Ok(Record::End) => return at_end(FileStatus::AtEnd),
        Ok(Record::Data(bytes)) => (bytes, false),
        Ok(Record::WrongLength(bytes)) => (bytes, true),
    };
    let record = if added { record.get(1..).unwrap_or_default().to_vec() } else { record };
    let record = if format == Format::Text {
        let page = x.facts().page();
        let unknown = page.encode_char('?').unwrap_or(0x6F);
        String::from_utf8_lossy(&record).chars().map(|c| page.encode_char(c).unwrap_or(unknown)).collect()
    } else {
        record
    };
    let variable = format == Format::Variable;
    let long = deliver(x, file, &record, variable, into, pos)?;
    at_end(if wrong_length || length_conflict(file, record.len(), variable, long) { FileStatus::SuccessWrongLength } else { FileStatus::Success })
}

/// WRITE of the record at `loc`: to a LINAGE file's page, a stream, or a held file.
pub fn write<P: Copy, X: Copy>(x: &mut impl Files<P, X>, file: &File<'_, P, X>, loc: Loc, advancing: Option<Advance<'_, X>>, pos: Pos) -> R<Outcome> {
    let k = file.index;
    let loc = match record_length(x, file, loc, pos)? {
        Ok(loc) => loc,
        Err(status) => return Ok(Outcome::Status { status, at_end: false }),
    };
    if paged(x, k) {
        return write_page(x, file, loc, advancing, pos);
    }
    if !is_held(x, k) {
        let (before, space) = match advancing {
            Some(a) => advance(x, a, pos)?,
            None => (false, Spacing::Lines(1)),
        };
        return write_stream(x, file, loc, before, space, pos);
    }
    let sequential = sequential(file);
    let status = held(x, k, |x, mode, format, keyed| {
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
        let bytes = record_bytes(x, file, loc, format);
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
                if n > files::MAX_RELATIVE || !relative_fits(x, file, n)? {
                    return Ok(FileStatus::BoundaryViolation);
                }
                if let Some(rk) = file.relative {
                    host::set_integer(x, rk, n as i64, pos)?;
                }
                files::record_number(n)
            }
            _ => match relative_number(x, file, pos)? {
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
    Ok(Outcome::Status { status, at_end: false })
}

fn paged<P: Copy, X: Copy>(x: &mut impl Files<P, X>, k: usize) -> bool {
    x.slot(k).as_ref().is_some_and(|f| f.page.is_some())
}

/// A WRITE to a LINAGE file: the page decides how far the paper moves, and once the record is
/// written LINAGE-COUNTER changes and an END-OF-PAGE phrase runs (Language Reference
/// SC27-8713-03, pp. 474-475; Programming Guide SC27-8714-03, p. 178).
fn write_page<P: Copy, X: Copy>(x: &mut impl Files<P, X>, file: &File<'_, P, X>, loc: Loc, advancing: Option<Advance<'_, X>>, pos: Pos) -> R<Outcome> {
    let k = file.index;
    let (before, motion) = match advancing {
        None => (false, Motion::Lines(1)),
        Some(Advance::Lines { before, count }) => (before, Motion::Lines(x.int(count, pos)?.max(0) as u64)),
        Some(Advance::Page { before }) => (before, Motion::Page),
        Some(Advance::Mnemonic { name, .. }) => {
            return Err(Abend::ironwork(format!("ADVANCING {name} on {}, whose FD has LINAGE, is not supported yet", file.name), pos));
        }
    };
    let Some(mut page) = x.slot(k).as_ref().and_then(|f| f.page) else { return Ok(Outcome::Done) };
    let step = page.write(before, motion, || geometry(x, file, pos))?;
    let controls = file.carriage.map(|c| printer::moving(c.machine, step.ahead, step.behind));
    let text = (Some(Move::Lines(step.ahead)), before.then_some(Move::Lines(step.behind)));
    if let failure @ Outcome::Failed(_) = put_line(x, file, loc, controls, text, pos)? {
        return Ok(failure);
    }
    if let Some(f) = x.slot(k).as_mut() {
        f.page = Some(page);
    }
    set_linage_counter(x, file, page.counter, pos)?;
    Ok(Outcome::Page { end_of_page: step.end_of_page })
}

/// A WRITE's ADVANCING phrase as a movement, BEFORE or AFTER the line; a count below zero moves as
/// zero ([`numeric::assumptions::PRINT_CONTROL_RUN_TIME`]).
fn advance<P: Copy, X: Copy>(x: &mut impl Files<P, X>, a: Advance<'_, X>, pos: Pos) -> R<(bool, Spacing)> {
    Ok(match a {
        Advance::Lines { before, count } => (before, Spacing::Lines(x.int(count, pos)?.max(0) as u64)),
        Advance::Page { before } => (before, Spacing::Channel(1)),
        Advance::Mnemonic { before, space, name, environment } => {
            let space = space.ok_or_else(|| Abend::ironwork(format!("ADVANCING {name}: {environment} is not a printer channel"), pos))?;
            (before, space)
        }
    })
}

/// A WRITE to a sequential file: a print file's records carry the control character, a text DD
/// shows it as line and form feeds.
pub fn write_stream<P: Copy, X: Copy>(x: &mut impl Files<P, X>, file: &File<'_, P, X>, loc: Loc, before: bool, space: Spacing, pos: Pos) -> R<Outcome> {
    let controls = file.carriage.map(|c| printer::controls(c.machine, before, space));
    put_line(x, file, loc, controls, printer::text_motion(before, space), pos)
}

/// Writes the record at `loc` to the sequential file behind `controls`, or to a text DD with the
/// paper moved as `text` says before and after its line.
fn put_line<P: Copy, X: Copy>(x: &mut impl Files<P, X>, file: &File<'_, P, X>, loc: Loc, controls: Option<Controls>, text: (Option<Move>, Option<Move>), pos: Pos) -> R<Outcome> {
    let (k, name) = (file.index, file.name);
    let Some(mut f) = x.slot(k).take() else {
        return Ok(failed(x, file, FileStatus::NotOpenOutput, format!("WRITE {name}: {}", FileStatus::NotOpenOutput.meaning())));
    };
    if f.mode == OpenMode::Input {
        *x.slot(k) = Some(f);
        return Ok(failed(x, file, FileStatus::NotOpenOutput, format!("WRITE {name}: {}", FileStatus::NotOpenOutput.meaning())));
    }
    let reserved = usize::from(file.carriage.is_some_and(|c| c.reserved));
    if let Some(c) = controls.filter(|_| reserved == 1 && loc.len > 0) {
        x.mem()[loc.offset] = c.data;
        host::mark(x, loc.offset, 1);
    }
    let bytes = record_bytes(x, file, loc, f.format);
    let written = match (f.format, controls) {
        (Format::Text, _) => {
            let line = x.facts().page().decode(bytes.get(reserved..).unwrap_or_default()).trim_end().to_owned();
            f.print(text.0, &line, text.1)
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
    *x.slot(k) = Some(f);
    match written {
        Ok(()) => set_status(x, file, FileStatus::Success, pos).map(|()| Outcome::Done),
        Err(e) => Ok(failed(x, file, FileStatus::PermanentError, format!("WRITE {name}: {e}"))),
    }
}

/// REWRITE of the record at `loc`; its phrase is INVALID KEY.
pub fn rewrite<P: Copy, X: Copy>(x: &mut impl Files<P, X>, file: &File<'_, P, X>, loc: Loc, pos: Pos) -> R<FileStatus> {
    let loc = match record_length(x, file, loc, pos)? {
        Ok(loc) => loc,
        Err(status) => return Ok(status),
    };
    let sequential = sequential(file);
    let added = held_control_byte(x, file);
    Ok(held(x, file.index, |x, mode, format, keyed| {
        if mode != OpenMode::InputOutput {
            return Ok(FileStatus::NotOpenInputOutput);
        }
        let mut bytes = record_bytes(x, file, loc, format);
        if added {
            let control = keyed.last_read.as_ref().and_then(|key| keyed.record(key)).and_then(|old| old.first().copied()).unwrap_or(ebcdic::SPACE);
            bytes.insert(0, control);
        }
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
                None => match relative_number(x, file, pos)? {
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
    .unwrap_or(FileStatus::NotOpenInputOutput))
}

/// DELETE; its phrase is INVALID KEY.
pub fn delete<P: Copy, X: Copy>(x: &mut impl Files<P, X>, file: &File<'_, P, X>, pos: Pos) -> R<FileStatus> {
    let sequential = sequential(file);
    Ok(held(x, file.index, |x, mode, _, keyed| {
        if mode != OpenMode::InputOutput || keyed.keying == Keying::Position {
            return Ok(FileStatus::NotOpenInputOutput);
        }
        let prior = keyed.last_read.take();
        let key = match (&keyed.keying, sequential) {
            (_, true) => match prior {
                Some(key) => key,
                None => return Ok(FileStatus::NoPriorRead),
            },
            (Keying::Indexed { prime, .. }, false) => prime.of(record_area(x, file.area)),
            _ => match relative_number(x, file, pos)? {
                Some(key) => key,
                None => return Ok(FileStatus::NotFound),
            },
        };
        Ok(if keyed.remove(&key).is_some() { FileStatus::Success } else { FileStatus::NotFound })
    })?
    .unwrap_or(FileStatus::NotOpenInputOutput))
}

/// START; `key` is START ... KEY, and its phrase is INVALID KEY.
pub fn start<P: Copy, X: Copy>(x: &mut impl Files<P, X>, file: &File<'_, P, X>, rel: StartRel, key: Option<P>, pos: Pos) -> R<FileStatus> {
    let (wanted, or_equal) = match rel {
        StartRel::Equal => (Ordering::Equal, false),
        StartRel::Greater => (Ordering::Greater, false),
        StartRel::NotLess => (Ordering::Greater, true),
    };
    let k = file.index;
    Ok(held(x, k, |x, mode, _, keyed| {
        if !matches!(mode, OpenMode::Input | OpenMode::InputOutput) || keyed.keying == Keying::Position {
            return Ok(FileStatus::NotOpenInput);
        }
        let keying = keyed.keying.clone();
        let (which, value) = match (&keying, key) {
            (Keying::Indexed { .. }, Some(r)) => x.key_value(k, &keying, r, true, pos)?,
            (Keying::Indexed { prime, .. }, None) => (0, prime.of(record_area(x, file.area))),
            (_, Some(r)) => (0, files::record_number(x.integer(r, pos)?.max(0) as u64)),
            (_, None) => (0, files::record_number(relative_value(x, file, pos)?.max(0) as u64)),
        };
        Ok(if keyed.start(which, wanted, or_equal, &value) { FileStatus::Success } else { FileStatus::NotFound })
    })?
    .unwrap_or(FileStatus::NotOpenInput))
}
