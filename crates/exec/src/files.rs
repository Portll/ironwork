//! Files. ASSIGN names a DD, and only the operator maps a DD to a host file, as JCL does, so a
//! program reaches no file it was not given. A binary DD holds z/OS records byte for byte:
//! fixed-length records back to back, or variable-length records each behind a 4-byte RDW. A text
//! DD holds UTF-8 lines, converted through the program's code page, and placed as a printer would
//! place them ([`Open::print`]). Sequential files stream;
//! indexed and relative files are held in memory (see [`Keyed`]).

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::ops::Bound;
use std::path::PathBuf;
use syntax::ast::OpenMode;
use zarch::ebcdic::{self, CodePage};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Fixed,
    Variable,
    Text,
}

impl Format {
    pub fn from_keyword(word: &str) -> Option<Format> {
        match word.to_ascii_lowercase().as_str() {
            "text" => Some(Format::Text),
            "fixed" | "f" => Some(Format::Fixed),
            "variable" | "v" => Some(Format::Variable),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dd {
    pub path: PathBuf,
    pub format: Option<Format>,
}

/// The DDs a run may use: given explicitly, or read from `DD_<NAME>` in the environment.
#[derive(Clone, Debug, Default)]
pub struct Dds {
    given: HashMap<String, Dd>,
    environment: bool,
}

fn dd_value(value: &str) -> Result<Dd, String> {
    let (path, format) = value.rsplit_once(':').and_then(|(p, word)| Some((p, Some(Format::from_keyword(word)?)))).unwrap_or((value, None));
    if path.is_empty() {
        return Err(format!("DD {value}: no path"));
    }
    Ok(Dd { path: PathBuf::from(path), format })
}

impl Dds {
    /// DDs from `NAME=path[:text|:fixed|:variable]` specs, and from the environment when `environment`.
    pub fn new(specs: &[String], environment: bool) -> Result<Self, String> {
        let mut given = HashMap::new();
        for spec in specs {
            let (name, value) = spec.split_once('=').ok_or_else(|| format!("--dd {spec}: expected NAME=path"))?;
            given.insert(name.to_ascii_uppercase(), dd_value(value)?);
        }
        Ok(Self { given, environment })
    }

    pub fn get(&self, name: &str) -> Option<Dd> {
        if let Some(dd) = self.given.get(name) {
            return Some(dd.clone());
        }
        if !self.environment {
            return None;
        }
        std::env::var(format!("DD_{name}")).ok().and_then(|v| dd_value(&v).ok())
    }
}

enum Handle {
    Reader(BufReader<File>),
    Writer(BufWriter<File>),
    Keyed(Box<Keyed>),
    Empty,
}

/// Where a key lies in a record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeySpan {
    pub offset: usize,
    pub len: usize,
}

impl KeySpan {
    pub fn of(&self, record: &[u8]) -> Vec<u8> {
        let mut key: Vec<u8> = record.iter().skip(self.offset).take(self.len).copied().collect();
        key.resize(self.len, ebcdic::SPACE);
        key
    }
}

/// How the records of a file held in memory are keyed. Record numbers and positions are held as
/// eight big-endian bytes, so that they sort.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Keying {
    /// A sequential file opened I-O, by position.
    Position,
    /// A relative file, by record number. In the DD an empty slot is a record of no bytes, or of
    /// zero bytes only.
    Relative,
    /// An indexed file: the prime key, then each alternate key and whether it allows duplicates.
    Indexed { prime: KeySpan, alternates: Vec<(KeySpan, bool)> },
}

/// The highest relative record number: past it WRITE is a boundary violation (status 24). It
/// stands in for the space a cluster is defined with, so that a stray record number cannot make
/// CLOSE write millions of empty slots.
pub const MAX_RELATIVE: u64 = 16_777_215;

pub fn record_number(n: u64) -> Vec<u8> {
    n.to_be_bytes().to_vec()
}

pub fn number_of(key: &[u8]) -> u64 {
    u64::from_be_bytes(key.try_into().unwrap_or([0; 8]))
}

/// An alternate index entry: alternate key, arrival number, prime key. Duplicates come back in
/// the order they arrived, as VSAM keeps them; records loaded from a DD arrive in prime-key order,
/// as BLDINDEX builds an alternate index.
type Entry = (Vec<u8>, u64, Vec<u8>);

struct Alternate {
    span: KeySpan,
    duplicates: bool,
    index: BTreeSet<Entry>,
    arrival: HashMap<Vec<u8>, u64>,
}

impl Alternate {
    fn holders<'a>(&'a self, value: &'a [u8]) -> impl Iterator<Item = &'a Entry> {
        self.index.range((value.to_vec(), 0, Vec::new())..).take_while(move |(v, ..)| v == value)
    }
}

/// The file position indicator.
#[derive(Clone, Debug)]
enum Cursor {
    /// After OPEN: READ NEXT reads the first record by the prime key.
    First,
    /// A key of reference (0 the prime key, then each alternate), a place in its index, and
    /// whether the record there is itself next (after START) or was the last read.
    At { which: usize, at: Entry, inclusive: bool },
    /// After an AT END condition or a START that found nothing: READ NEXT fails with status 46.
    Undefined,
}

/// A record READ found.
pub struct Found {
    pub key: Vec<u8>,
    pub record: Vec<u8>,
    /// The next record by the alternate key of reference has the same key (status 02).
    pub duplicate: bool,
}

/// An indexed or relative file, or a sequential file opened I-O, held in memory from OPEN to
/// CLOSE. The DD holds the records in key (or record-number) order, as an IDCAMS REPRO unload of a
/// KSDS or RRDS does, and CLOSE writes them back that way when they changed.
pub struct Keyed {
    pub keying: Keying,
    records: BTreeMap<Vec<u8>, Vec<u8>>,
    alternates: Vec<Alternate>,
    arrivals: u64,
    cursor: Cursor,
    /// The key of the record the last statement on the file, a successful READ, returned: what a
    /// sequential REWRITE or DELETE acts on.
    pub last_read: Option<Vec<u8>>,
    dirty: bool,
    path: Option<PathBuf>,
    record_len: usize,
    page: &'static CodePage,
}

impl Keyed {
    fn new(keying: Keying, path: Option<PathBuf>, record_len: usize, page: &'static CodePage) -> Self {
        let alternates = match &keying {
            Keying::Indexed { alternates, .. } => {
                alternates.iter().map(|&(span, duplicates)| Alternate { span, duplicates, index: BTreeSet::new(), arrival: HashMap::new() }).collect()
            }
            _ => Vec::new(),
        };
        Self { keying, records: BTreeMap::new(), alternates, arrivals: 0, cursor: Cursor::First, last_read: None, dirty: true, path, record_len, page }
    }

    pub fn prime_key(&self, record: &[u8]) -> Option<Vec<u8>> {
        match &self.keying {
            Keying::Indexed { prime, .. } => Some(prime.of(record)),
            _ => None,
        }
    }

    pub fn highest_key(&self) -> Option<&Vec<u8>> {
        self.records.keys().next_back()
    }

    pub fn record(&self, key: &[u8]) -> Option<&Vec<u8>> {
        self.records.get(key)
    }

    fn key_len(&self, which: usize) -> usize {
        match (&self.keying, which) {
            (Keying::Indexed { prime, .. }, 0) => prime.len,
            (Keying::Indexed { .. }, n) => self.alternates[n - 1].span.len,
            _ => 8,
        }
    }

    /// The record whose key `which` is `value`: for an alternate key, the first of any duplicates.
    pub fn get(&self, which: usize, value: &[u8]) -> Option<Found> {
        if which == 0 {
            return self.records.get(value).map(|r| Found { key: value.to_vec(), record: r.clone(), duplicate: false });
        }
        let mut hits = self.alternates[which - 1].holders(value);
        let key = hits.next()?.2.clone();
        Some(Found { record: self.records[&key].clone(), key, duplicate: hits.next().is_some() })
    }

    /// Adds a record, or replaces the one with its key: Err with status 22 when it would take a
    /// unique alternate key another record holds. Ok(true) when it shares an alternate key that
    /// allows duplicates (status 02).
    fn put(&mut self, key: Vec<u8>, record: Vec<u8>) -> Result<bool, &'static str> {
        let mut shared = false;
        for alt in &self.alternates {
            let value = alt.span.of(&record);
            let others = alt.holders(&value).any(|(_, _, k)| *k != key);
            if others && !alt.duplicates {
                return Err("22");
            }
            shared |= others;
        }
        let old = self.records.get(&key).cloned();
        let mut arrivals = self.arrivals;
        for alt in &mut self.alternates {
            let now = alt.span.of(&record);
            if let Some(old) = &old {
                let was = alt.span.of(old);
                if was == now {
                    continue;
                }
                let seq = alt.arrival[&key];
                alt.index.remove(&(was, seq, key.clone()));
            }
            alt.index.insert((now, arrivals, key.clone()));
            alt.arrival.insert(key.clone(), arrivals);
            arrivals += 1;
        }
        self.arrivals = arrivals;
        self.records.insert(key, record);
        self.dirty = true;
        Ok(shared)
    }

    /// Adds a record: Err with status 22 when its prime key, or a unique alternate key, is already
    /// there; Ok(true) when it shares an alternate key that allows duplicates (status 02).
    pub fn insert(&mut self, key: Vec<u8>, record: Vec<u8>) -> Result<bool, &'static str> {
        if self.records.contains_key(&key) {
            return Err("22");
        }
        self.put(key, record)
    }

    /// Replaces a record: Err with status 23 when none has its key, 22 as for [`Keyed::insert`].
    pub fn replace(&mut self, key: Vec<u8>, record: Vec<u8>) -> Result<bool, &'static str> {
        if !self.records.contains_key(&key) {
            return Err("23");
        }
        self.put(key, record)
    }

    pub fn remove(&mut self, key: &[u8]) -> Option<Vec<u8>> {
        let record = self.records.remove(key)?;
        for alt in &mut self.alternates {
            if let Some(seq) = alt.arrival.remove(key) {
                alt.index.remove(&(alt.span.of(&record), seq, key.to_vec()));
            }
        }
        self.dirty = true;
        Some(record)
    }

    /// START: positions at the first record whose key `which`, compared over the length of `value`
    /// (a longer `value` is cut to the key's length), is `wanted` to `value`, or equal when
    /// `or_equal`. False, and no position, when there is none.
    pub fn start(&mut self, which: usize, wanted: Ordering, or_equal: bool, value: &[u8]) -> bool {
        let value = &value[..value.len().min(self.key_len(which))];
        let fits = |k: &[u8]| {
            let o = k[..value.len().min(k.len())].cmp(value);
            o == wanted || (or_equal && o == Ordering::Equal)
        };
        let from = (value.to_vec(), 0, Vec::new());
        let found = match which {
            0 => self.records.range(from.0..).map(|(k, _)| k).find(|k| fits(k)).map(|k| (k.clone(), 0, Vec::new())),
            n => self.alternates[n - 1].index.range(from..).find(|(k, ..)| fits(k)).cloned(),
        };
        self.last_read = None;
        match found {
            Some(at) => {
                self.cursor = Cursor::At { which, at, inclusive: true };
                true
            }
            None => {
                self.lose_position();
                false
            }
        }
    }

    /// After a READ that found nothing: READ NEXT has no valid next record.
    pub fn lose_position(&mut self) {
        self.cursor = Cursor::Undefined;
        self.last_read = None;
    }

    /// Makes the record just read by key `which` the one READ NEXT goes on from.
    pub fn read_at(&mut self, which: usize, key: &[u8]) {
        let at = match which {
            0 => (key.to_vec(), 0, Vec::new()),
            n => {
                let alt = &self.alternates[n - 1];
                (alt.span.of(&self.records[key]), alt.arrival[key], key.to_vec())
            }
        };
        self.cursor = Cursor::At { which, at, inclusive: false };
        self.last_read = Some(key.to_vec());
    }

    /// The first record by prime key after `from` (at it too when `inclusive`), or before it when
    /// `backward`: what a CICS browse reads, holding its own position rather than the file's.
    pub fn seek(&self, from: &[u8], inclusive: bool, backward: bool) -> Option<(Vec<u8>, Vec<u8>)> {
        let edge = if inclusive { Bound::Included(from.to_vec()) } else { Bound::Excluded(from.to_vec()) };
        let mut hits = if backward { self.records.range((Bound::Unbounded, edge)) } else { self.records.range((edge, Bound::Unbounded)) };
        let hit = if backward { hits.next_back() } else { hits.next() };
        hit.map(|(k, r)| (k.clone(), r.clone()))
    }

    /// READ NEXT, or READ PREVIOUS when `backward`: None at the end, Err(46) with no position.
    pub fn step(&mut self, backward: bool) -> Result<Option<Found>, &'static str> {
        let (which, bound) = match &self.cursor {
            Cursor::Undefined => return Err("46"),
            Cursor::First => (0, Bound::Unbounded),
            Cursor::At { which, at, inclusive: true } => (*which, Bound::Included(at.clone())),
            Cursor::At { which, at, inclusive: false } => (*which, Bound::Excluded(at.clone())),
        };
        let range = if backward { (Bound::Unbounded, bound) } else { (bound, Bound::Unbounded) };
        let hit = if which == 0 {
            let mut keys = self.records.range((range.0.map(|(k, ..)| k), range.1.map(|(k, ..)| k))).map(|(k, _)| k);
            let key = if backward { keys.next_back() } else { keys.next() };
            key.map(|k| (k.clone(), false))
        } else {
            let mut entries = self.alternates[which - 1].index.range(range);
            let entry = if backward { entries.next_back() } else { entries.next() };
            entry.map(|(value, _, key)| {
                let after = if backward { entries.next_back() } else { entries.next() };
                (key.clone(), after.is_some_and(|(v, ..)| v == value))
            })
        };
        let Some((key, duplicate)) = hit else {
            self.lose_position();
            return Ok(None);
        };
        self.read_at(which, &key);
        Ok(Some(Found { record: self.records[&key].clone(), key, duplicate }))
    }

    fn inbound(&self, raw: Vec<u8>, format: Format) -> Vec<u8> {
        let mut record = match format {
            Format::Text => self.page.encode_lossy(&String::from_utf8_lossy(&raw)),
            _ => raw,
        };
        if format != Format::Variable {
            record.resize(self.record_len.max(record.len()), ebcdic::SPACE);
        }
        record
    }

    fn outbound(&self, record: &[u8], format: Format) -> Vec<u8> {
        match format {
            Format::Text => format!("{}\n", self.page.decode(record).trim_end()).into_bytes(),
            Format::Fixed => {
                let mut r = record.to_vec();
                r.resize(self.record_len, ebcdic::SPACE);
                r
            }
            Format::Variable => record.to_vec(),
        }
    }

    fn save(&self, format: Format) -> io::Result<()> {
        let Some(path) = self.path.as_ref().filter(|_| self.dirty) else { return Ok(()) };
        let mut out = Open { mode: OpenMode::Output, format, handle: Handle::Writer(BufWriter::new(File::create(path)?)), head: Head::Start };
        let empty = match format {
            Format::Fixed => vec![0; self.record_len],
            Format::Variable => Vec::new(),
            Format::Text => b"\n".to_vec(),
        };
        let mut next = 1;
        for (key, record) in &self.records {
            if self.keying == Keying::Relative {
                for _ in next..number_of(key) {
                    out.write(&empty)?;
                }
                next = number_of(key) + 1;
            }
            out.write(&self.outbound(record, format))?;
        }
        out.close()
    }
}

pub struct Open {
    pub mode: OpenMode,
    pub format: Format,
    handle: Handle,
    head: Head,
}

/// A movement of the paper, as a text DD shows it: line feeds, or a form feed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    Lines(u64),
    Page,
}

/// Where a text DD's print position is: before its first line, on a line not yet ended (and
/// whether anything shows on it), or at the start of a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Head {
    Start,
    OnLine(bool),
    Fresh,
}

/// What a READ found.
pub enum Record {
    Data(Vec<u8>),
    /// A record whose length did not match: the bytes read, for FILE STATUS 04.
    WrongLength(Vec<u8>),
    End,
}

/// Opens a file held in memory. With no DD (an OPTIONAL file) or no file at the DD's path, it
/// starts empty; OUTPUT always starts empty. `record_len` is the longest record.
pub fn open_keyed(dd: Option<&Dd>, mode: OpenMode, format: Format, keying: Keying, record_len: usize, page: &'static CodePage) -> io::Result<Open> {
    let path = dd.filter(|_| mode != OpenMode::Input).map(|d| d.path.clone());
    let mut keyed = Keyed::new(keying, path, record_len, page);
    if let Some(dd) = dd.filter(|d| mode != OpenMode::Output && d.path.exists()) {
        let mut reader = open(dd, OpenMode::Input, format)?;
        let mut number = 0u64;
        loop {
            let raw = match reader.read(record_len)? {
                Record::End => break,
                Record::Data(r) | Record::WrongLength(r) => r,
            };
            number += 1;
            if keyed.keying == Keying::Relative && raw.iter().all(|&b| b == 0) {
                continue;
            }
            let record = keyed.inbound(raw, format);
            let key = keyed.prime_key(&record).unwrap_or_else(|| record_number(number));
            keyed.insert(key, record).map_err(|_| io::Error::new(io::ErrorKind::InvalidData, format!("{}: two records with one key", dd.path.display())))?;
        }
        keyed.dirty = false;
    }
    Ok(Open { mode, format, handle: Handle::Keyed(Box::new(keyed)), head: Head::Start })
}

pub fn open(dd: &Dd, mode: OpenMode, format: Format) -> io::Result<Open> {
    let handle = match mode {
        OpenMode::Input => Handle::Reader(BufReader::new(File::open(&dd.path)?)),
        OpenMode::Output => Handle::Writer(BufWriter::new(File::create(&dd.path)?)),
        OpenMode::Extend => Handle::Writer(BufWriter::new(OpenOptions::new().append(true).create(true).open(&dd.path)?)),
        OpenMode::InputOutput => return Err(io::Error::new(io::ErrorKind::Unsupported, "OPEN I-O of a line-sequential file")),
    };
    Ok(Open { mode, format, handle, head: Head::Start })
}

/// An OPTIONAL input file with no DD: every READ is at end.
pub fn absent() -> Open {
    Open { mode: OpenMode::Input, format: Format::Fixed, handle: Handle::Empty, head: Head::Start }
}

impl Open {
    pub fn keyed(&mut self) -> Option<&mut Keyed> {
        match &mut self.handle {
            Handle::Keyed(k) => Some(k),
            _ => None,
        }
    }

    pub fn is_keyed(&self) -> bool {
        matches!(self.handle, Handle::Keyed(_))
    }

    pub fn read(&mut self, fixed_len: usize) -> io::Result<Record> {
        let reader = match &mut self.handle {
            Handle::Reader(r) => r,
            Handle::Empty => return Ok(Record::End),
            Handle::Writer(_) | Handle::Keyed(_) => return Err(io::Error::other("a sequential READ of a file not open for it")),
        };
        match self.format {
            Format::Fixed => {
                let mut buf = Vec::with_capacity(fixed_len);
                reader.by_ref().take(fixed_len as u64).read_to_end(&mut buf)?;
                Ok(match buf.len() {
                    0 => Record::End,
                    n if n == fixed_len => Record::Data(buf),
                    _ => Record::WrongLength(buf),
                })
            }
            Format::Variable => {
                let mut rdw = [0u8; 4];
                match reader.read_exact(&mut rdw) {
                    Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(Record::End),
                    other => other?,
                }
                let len = u16::from_be_bytes([rdw[0], rdw[1]]) as usize;
                if len < 4 {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, format!("an RDW of length {len}")));
                }
                let mut buf = vec![0u8; len - 4];
                reader.read_exact(&mut buf)?;
                Ok(Record::Data(buf))
            }
            Format::Text => {
                let mut line = Vec::new();
                if reader.read_until(b'\n', &mut line)? == 0 {
                    return Ok(Record::End);
                }
                while matches!(line.last(), Some(b'\n' | b'\r')) {
                    line.pop();
                }
                Ok(Record::Data(line))
            }
        }
    }

    pub fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        let Handle::Writer(w) = &mut self.handle else {
            return Err(io::Error::other("WRITE to a file not opened for output"));
        };
        match self.format {
            Format::Fixed | Format::Text => w.write_all(bytes),
            Format::Variable => {
                let len = u16::try_from(bytes.len() + 4).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "a record over 32,760 bytes"))?;
                w.write_all(&len.to_be_bytes())?;
                w.write_all(&[0, 0])?;
                w.write_all(bytes)
            }
        }
    }

    /// Writes a line to a text DD where a printer would put it: after `before`, over a line not
    /// yet ended when nothing moved the paper (after a carriage return, unless one of the two is
    /// blank), then `after`. The first line of the DD takes one line of `before` as its own start,
    /// as a printer at the top of a form prints a single-spaced first line on line 1.
    pub fn print(&mut self, before: Option<Move>, line: &str, after: Option<Move>) -> io::Result<()> {
        if let Some(m) = before {
            self.feed(m)?;
        }
        let Handle::Writer(w) = &mut self.handle else {
            return Err(io::Error::other("WRITE to a file not opened for output"));
        };
        let shown = matches!(self.head, Head::OnLine(true));
        if shown && !line.is_empty() {
            w.write_all(b"\r")?;
        }
        w.write_all(line.as_bytes())?;
        self.head = Head::OnLine(shown || !line.is_empty());
        match after {
            Some(m) => self.feed(m),
            None => Ok(()),
        }
    }

    fn feed(&mut self, m: Move) -> io::Result<()> {
        let Handle::Writer(w) = &mut self.handle else {
            return Err(io::Error::other("WRITE to a file not opened for output"));
        };
        match (m, self.head) {
            (Move::Lines(0), _) => return Ok(()),
            (Move::Lines(n), head) => {
                for _ in 0..n - u64::from(head == Head::Start) {
                    w.write_all(b"\n")?;
                }
            }
            (Move::Page, Head::OnLine(_)) => w.write_all(b"\n\x0c")?,
            (Move::Page, _) => w.write_all(b"\x0c")?,
        }
        self.head = Head::Fresh;
        Ok(())
    }

    pub fn close(self) -> io::Result<()> {
        match self.handle {
            Handle::Writer(mut w) => {
                if matches!(self.head, Head::OnLine(_)) {
                    w.write_all(b"\n")?;
                }
                w.flush()
            }
            Handle::Keyed(k) => k.save(self.format),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dd_specs() {
        let dds = Dds::new(&["IN=/tmp/a.txt:text".into(), "out=/tmp/b.dat".into()], false).unwrap();
        assert_eq!(dds.get("IN"), Some(Dd { path: "/tmp/a.txt".into(), format: Some(Format::Text) }));
        assert_eq!(dds.get("OUT").unwrap().format, None);
        assert!(dds.get("NONE").is_none());
        assert!(Dds::new(&["NOEQUALS".into()], false).is_err());
    }

    #[test]
    fn variable_records_round_trip_through_rdws() {
        let path = std::env::temp_dir().join(format!("ironwork-vb-{}", std::process::id()));
        let dd = Dd { path: path.clone(), format: None };
        let mut out = open(&dd, OpenMode::Output, Format::Variable).unwrap();
        out.write(b"AB").unwrap();
        out.write(b"CDEF").unwrap();
        out.close().unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), [0, 6, 0, 0, b'A', b'B', 0, 8, 0, 0, b'C', b'D', b'E', b'F']);
        let mut input = open(&dd, OpenMode::Input, Format::Variable).unwrap();
        assert!(matches!(input.read(0).unwrap(), Record::Data(d) if d == b"AB"));
        assert!(matches!(input.read(0).unwrap(), Record::Data(d) if d == b"CDEF"));
        assert!(matches!(input.read(0).unwrap(), Record::End));
    }
}
