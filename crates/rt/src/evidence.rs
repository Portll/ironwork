//! Run journals in cobolwork's evidence format (cobolwork `docs/spec/evidence.md` §4-6), so one
//! verifier reads both tools: each record is canonical JSON hashed as
//! SHA-256("cobolwork-evidence/v1\n" || record without "hash"), chained by `prev` and `seq`, and
//! each closed run adds its tip to `ledger.jsonl`. Nothing written holds source text or a secret.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::digest::{hex, sha256};

const DOMAIN: &str = "cobolwork-evidence/v1\n";
const ZERO: &str = "0000000000000000000000000000000000000000000000000000000000000000";
pub const LEDGER: &str = "ledger.jsonl";
const LOCK: &str = "ledger.lock";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Str(String),
    Arr(Vec<Value>),
    Obj(BTreeMap<String, Value>),
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Self::Str(s.to_string())
    }
}
impl From<String> for Value {
    fn from(s: String) -> Self {
        Self::Str(s)
    }
}
impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Self::Int(n)
    }
}
impl From<u64> for Value {
    fn from(n: u64) -> Self {
        Self::Int(i64::try_from(n).unwrap_or(i64::MAX))
    }
}
impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}

/// Builds the fields of one record: `fields([("dd", "IN".into()), ...])`.
pub fn fields<const N: usize>(pairs: [(&str, Value); N]) -> BTreeMap<String, Value> {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

/// Strings escaped as JavaScript's JSON.stringify escapes them.
fn escape(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Keys sorted by UTF-16 code unit, as JavaScript sorts them, at every depth; no whitespace.
pub fn canonical(value: &Value) -> String {
    let mut out = String::new();
    write(value, &mut out);
    out
}

fn write(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(n) => out.push_str(&n.to_string()),
        Value::Str(s) => escape(s, out),
        Value::Arr(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write(item, out);
            }
            out.push(']');
        }
        Value::Obj(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            out.push('{');
            for (i, k) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                escape(k, out);
                out.push(':');
                write(&map[k], out);
            }
            out.push('}');
        }
    }
}

pub fn record_hash(record: &BTreeMap<String, Value>) -> String {
    let mut body = record.clone();
    body.remove("hash");
    let mut text = String::from(DOMAIN);
    text.push_str(&canonical(&Value::Obj(body)));
    hex(&sha256(text.as_bytes()))
}

/// Each kind ironwork writes: the file it goes in, the fields it may carry and those it must. Each
/// row stays within cobolwork's kinds table (fixtures/cobolwork/evidence/kinds.tsv), which its
/// verifier holds every record to.
const KINDS: [(&str, &str, &[&str], &[&str]); 13] = [
    ("open", "journal", &["tool", "toolVersion", "toolRevision", "command", "argv", "roots", "platform"], &["tool", "toolVersion", "command", "argv", "roots"]),
    ("input", "journal", &["root", "path", "sha256", "bytes"], &["root", "path", "sha256"]),
    ("dd", "journal", &["dd", "event", "mode", "sha256", "bytes"], &["dd", "event"]),
    ("call", "journal", &["program", "from", "sha256"], &["program"]),
    ("abend", "journal", &["code", "file", "line"], &["code"]),
    ("step", "journal", &["step", "pgm", "outcome"], &["step", "pgm", "outcome"]),
    ("sink", "journal", &["sink", "file", "line", "marker", "reached", "input"], &["sink", "file", "line"]),
    ("statement", "journal", &["file", "line", "capped"], &["file", "line"]),
    ("output", "journal", &["name", "sha256", "bytes", "path", "stdout"], &["name", "sha256"]),
    ("close", "journal", &["exit", "counts", "durationMs", "ledger"], &["exit"]),
    ("genesis", "ledger", &["createdAt"], &["createdAt"]),
    ("run", "ledger", &["run", "runChain", "runLength", "runTip"], &["run", "runChain", "runLength", "runTip"]),
    ("lock-broken", "ledger", &["holderPid", "ageMs"], &["holderPid", "ageMs"]),
];

fn kind_fields(kind: &str) -> Option<(&'static [&'static str], &'static [&'static str])> {
    KINDS.iter().find(|(name, ..)| *name == kind).map(|(_, _, allowed, required)| (*allowed, *required))
}

fn check(kind: &str, record: &BTreeMap<String, Value>) -> io::Result<()> {
    let bad = |m: String| io::Error::new(io::ErrorKind::InvalidInput, m);
    let (allowed, required) = kind_fields(kind).ok_or_else(|| bad(format!("no evidence record kind {kind}")))?;
    for k in record.keys() {
        if !allowed.contains(&k.as_str()) {
            return Err(bad(format!("{kind}: no field {k}")));
        }
    }
    for k in required {
        if !record.contains_key(*k) {
            return Err(bad(format!("{kind}: {k} is required")));
        }
    }
    Ok(())
}

/// Now, as JavaScript's Date.prototype.toISOString writes it.
pub fn iso_now() -> String {
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    iso(d.as_secs() as i64, d.subsec_millis())
}

pub fn iso(seconds: i64, millis: u32) -> String {
    let c = crate::calendar::civil(seconds);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z", c.year, c.month, c.day, c.hour, c.minute, c.second, millis)
}

/// `n` unpredictable bytes from the operating system, or, where it offers none, bytes that are
/// unique to this process and moment: a chain id must not repeat, and needs no secrecy.
fn random(n: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n];
    if File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut buf)).is_ok() {
        return buf;
    }
    use std::hash::{BuildHasher, Hasher};
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let mut keyed = std::collections::hash_map::RandomState::new().build_hasher();
    keyed.write_u128(d.as_nanos());
    let seed = format!("{}:{}:{:p}:{}", d.as_nanos(), std::process::id(), &buf, keyed.finish());
    sha256(seed.as_bytes())[..n.min(32)].to_vec()
}

fn open_new(path: &Path) -> io::Result<File> {
    let mut o = OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o.open(path)
}

fn refuse_link(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => Err(io::Error::new(io::ErrorKind::InvalidInput, format!("{} is a symbolic link, which evidence is not written through", path.display()))),
        _ => Ok(()),
    }
}

/// The evidence directory with runs/ and seals/, refusing a link at any of the three, and a
/// directory inside a tree the run reads.
pub fn prepare(dir: &Path, reads: &[PathBuf]) -> io::Result<PathBuf> {
    let absolute = std::path::absolute(dir)?;
    let mut existing = absolute.clone();
    let mut rest = Vec::new();
    while !existing.exists() {
        match (existing.file_name().map(|n| n.to_os_string()), existing.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name);
                existing = parent.to_path_buf();
            }
            _ => break,
        }
    }
    let mut intended = fs::canonicalize(&existing)?;
    for name in rest.iter().rev() {
        intended.push(name);
    }
    for root in reads {
        let tree = if root.as_os_str().is_empty() { Path::new(".") } else { root };
        if let Ok(r) = fs::canonicalize(tree)
            && intended.starts_with(&r)
        {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("the evidence directory {} is inside {}, which this run reads", dir.display(), root.display())));
        }
    }
    for p in [absolute.clone(), absolute.join("runs"), absolute.join("seals")] {
        refuse_link(&p)?;
        if !p.exists() {
            fs::create_dir_all(&p)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&p, fs::Permissions::from_mode(0o700))?;
            }
        }
        refuse_link(&p)?;
    }
    let made = fs::canonicalize(&absolute)?;
    if made != intended {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("the evidence directory {} resolved to {} while it was made", dir.display(), made.display())));
    }
    Ok(made)
}

struct Chain {
    chain: String,
    seq: i64,
    prev: String,
}

impl Chain {
    fn record(&mut self, kind: &str, mut fields: BTreeMap<String, Value>, at: &str) -> io::Result<String> {
        check(kind, &fields)?;
        fields.insert("v".into(), Value::Int(1));
        fields.insert("chain".into(), Value::Str(self.chain.clone()));
        fields.insert("seq".into(), Value::Int(self.seq));
        fields.insert("at".into(), Value::Str(at.to_string()));
        fields.insert("kind".into(), Value::Str(kind.to_string()));
        fields.insert("prev".into(), Value::Str(self.prev.clone()));
        let hash = record_hash(&fields);
        fields.insert("hash".into(), Value::Str(hash.clone()));
        self.prev = hash;
        self.seq += 1;
        Ok(format!("{}\n", canonical(&Value::Obj(fields))))
    }
}

pub struct Journal {
    pub id: String,
    pub path: PathBuf,
    dir: PathBuf,
    file: File,
    chain: Chain,
    counts: BTreeMap<String, i64>,
    started: Instant,
}

/// Where a closed run's tip went.
#[derive(Debug, PartialEq, Eq)]
pub enum Ledger {
    Recorded,
    Unrecorded(String),
}

impl Journal {
    pub fn create(dir: &Path, reads: &[PathBuf], command: &str, argv: &[String], tool_version: &str) -> io::Result<Self> {
        let dir = prepare(dir, reads)?;
        let now = iso_now();
        let stamp: String = now.chars().filter(|c| c.is_ascii_digit() || *c == 'T').take(15).collect();
        let id = format!("{stamp}Z-{}", hex(&random(8)));
        let path = dir.join("runs").join(format!("{id}.jsonl"));
        let file = open_new(&path)?;
        let mut journal = Self { id, path, dir, file, chain: Chain { chain: hex(&random(16)), seq: 0, prev: ZERO.into() }, counts: BTreeMap::new(), started: Instant::now() };
        let roots = reads.iter().map(|r| Value::Str(r.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())).collect();
        journal.append_at(
            "open",
            fields([
                ("tool", "ironwork".into()),
                ("toolVersion", tool_version.into()),
                ("command", command.into()),
                ("argv", Value::Arr(argv.iter().map(|a| Value::Str(a.clone())).collect())),
                ("roots", Value::Arr(roots)),
                ("platform", std::env::consts::OS.into()),
            ]),
            &now,
        )?;
        Ok(journal)
    }

    fn append_at(&mut self, kind: &str, fields: BTreeMap<String, Value>, at: &str) -> io::Result<()> {
        let line = self.chain.record(kind, fields, at)?;
        self.file.write_all(line.as_bytes())?;
        *self.counts.entry(kind.to_string()).or_default() += 1;
        Ok(())
    }

    /// The hash of the last record written.
    pub fn tip(&self) -> &str {
        &self.chain.prev
    }

    pub fn append(&mut self, kind: &str, fields: BTreeMap<String, Value>) -> io::Result<()> {
        self.append_at(kind, fields, &iso_now())
    }

    /// Writes close, then the ledger record carrying this journal's tip.
    pub fn close(mut self, exit: Option<i64>) -> io::Result<Ledger> {
        let lock = self.dir.join(LOCK);
        let held = take_lock(&lock, LOCK_WAIT);
        let counts = self.counts.iter().map(|(k, v)| (k.clone(), Value::Int(*v))).collect();
        let ledger = if held.is_ok() { "recorded" } else { "unrecorded" };
        let duration = i64::try_from(self.started.elapsed().as_millis()).unwrap_or(i64::MAX);
        self.append("close", fields([("exit", exit.map_or(Value::Null, Value::Int)), ("counts", Value::Obj(counts)), ("durationMs", Value::Int(duration)), ("ledger", ledger.into())]))?;
        self.file.sync_all()?;
        let broken = match held {
            Ok(broken) => broken,
            Err(reason) => return Ok(Ledger::Unrecorded(reason)),
        };
        let result = append_ledger(&self.dir, &self.id, &self.chain, broken);
        let _ = fs::remove_file(&lock);
        match result {
            Ok(()) => Ok(Ledger::Recorded),
            Err(e) => Ok(Ledger::Unrecorded(e.to_string())),
        }
    }
}

const LOCK_WAIT: Duration = Duration::from_secs(5);
const LOCK_STALE_MS: i64 = 60_000;

/// A lock that was broken: the pid it names, if it names one, and its age.
struct Broken {
    pid: Option<i64>,
    age_ms: i64,
}

/// The ledger lock, broken as cobolwork breaks it (cobolwork `docs/spec/evidence.md` §6): a lock
/// older than a minute whose holder is not running. It is aged from the time it holds or, holding
/// none, from its modification time. The break goes into the ledger.
fn take_lock(path: &Path, wait: Duration) -> Result<Option<Broken>, String> {
    let mut broken = None;
    let deadline = Instant::now() + wait;
    loop {
        match open_new(path) {
            Ok(mut f) => {
                let _ = writeln!(f, "{} {}", std::process::id(), now_ms());
                return Ok(broken);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                if let Some(seen) = read_lock(path)
                    && seen.age_ms > LOCK_STALE_MS
                    && !seen.pid.is_some_and(running)
                    && break_stale(path, &seen)
                {
                    broken = Some(Broken { pid: seen.pid, age_ms: seen.age_ms });
                    continue;
                }
                if Instant::now() >= deadline {
                    return Err("the ledger lock could not be had".into());
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

fn now_ms() -> i64 {
    i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis()).unwrap_or(i64::MAX)
}

/// A lock as read: its contents, its file's identity, the pid it names and its age.
struct SeenLock {
    text: Vec<u8>,
    file: u64,
    pid: Option<i64>,
    age_ms: i64,
}

fn file_id(meta: &fs::Metadata) -> u64 {
    #[cfg(unix)]
    {
        std::os::unix::fs::MetadataExt::ino(meta)
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        0
    }
}

fn read_lock(path: &Path) -> Option<SeenLock> {
    let meta = fs::symlink_metadata(path).ok()?;
    let text = fs::read(path).ok()?;
    let mut numbers = String::from_utf8_lossy(&text).split_whitespace().map(|w| w.parse::<i64>().ok().filter(|n| *n > 0)).collect::<Vec<_>>().into_iter();
    let pid = numbers.next().flatten();
    let since = numbers.next().flatten();
    let age_ms = match (pid, since) {
        (Some(_), Some(since)) => now_ms().saturating_sub(since),
        _ => meta.modified().ok().and_then(|t| t.elapsed().ok()).map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX)),
    };
    Some(SeenLock { text, file: file_id(&meta), pid, age_ms })
}

/// Whether process `pid` is running; where that cannot be told, it is taken as not running.
fn running(pid: i64) -> bool {
    #[cfg(target_os = "linux")]
    {
        Path::new("/proc").join(pid.to_string()).exists()
    }
    #[cfg(all(unix, not(target_os = "linux")))]
    {
        std::process::Command::new("ps").args(["-p", &pid.to_string(), "-o", "pid="]).output().is_ok_and(|o| o.status.success() && !o.stdout.trim_ascii().is_empty())
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

/// Moves the stale lock `seen` aside before removing it, so a lock a peer took after it was read
/// is put back instead of removed. Returns whether the stale lock was the one broken.
fn break_stale(path: &Path, seen: &SeenLock) -> bool {
    let aside = path.with_file_name(format!("{LOCK}.{}.{}", std::process::id(), hex(&random(4))));
    if fs::rename(path, &aside).is_err() {
        return false;
    }
    let same = fs::symlink_metadata(&aside).is_ok_and(|m| file_id(&m) == seen.file) && fs::read(&aside).is_ok_and(|t| t == seen.text);
    if !same {
        let _ = fs::hard_link(&aside, path);
    }
    let _ = fs::remove_file(&aside);
    same
}

/// The last line of the ledger, or None for an empty or absent one; an unterminated last line is
/// refused rather than extended.
fn ledger_tail(path: &Path) -> io::Result<Option<String>> {
    // nosemgrep: rust.actix.path-traversal.tainted-path.tainted-path -- the ledger path the run was given
    let mut f = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let size = f.metadata()?.len();
    if size == 0 {
        return Ok(None);
    }
    let window = size.min(65536);
    f.seek(SeekFrom::Start(size - window))?;
    let mut buf = Vec::new();
    f.take(window).read_to_end(&mut buf)?;
    let text = String::from_utf8_lossy(&buf);
    let body = text.strip_suffix('\n').ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "ledger.jsonl ends in a partial line"))?;
    match body.rsplit_once('\n') {
        Some((_, last)) => Ok(Some(last.to_string())),
        None if window < size => Err(io::Error::new(io::ErrorKind::InvalidData, "the ledger's last line is longer than any ledger record")),
        None => Ok(Some(body.to_string())),
    }
}

/// The value of a top-level string or integer field in one canonical record line.
fn field_of(line: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":");
    let at = line.find(&needle)? + needle.len();
    let rest = &line[at..];
    if let Some(s) = rest.strip_prefix('"') {
        return s.find('"').map(|end| s[..end].to_string());
    }
    Some(rest.chars().take_while(|c| c.is_ascii_digit()).collect())
}

fn append_ledger(dir: &Path, run: &str, journal: &Chain, broken: Option<Broken>) -> io::Result<()> {
    let path = dir.join(LEDGER);
    refuse_link(&path)?;
    let mut chain = match ledger_tail(&path)? {
        Some(line) => {
            let bad = || io::Error::new(io::ErrorKind::InvalidData, "the ledger's last line is not a ledger record");
            let hex_of = |key: &str, len: usize| field_of(&line, key).filter(|v| v.len() == len && v.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
            let seq: i64 = field_of(&line, "seq").and_then(|s| s.parse().ok()).ok_or_else(bad)?;
            Chain { chain: hex_of("chain", 32).ok_or_else(bad)?, seq: seq + 1, prev: hex_of("hash", 64).ok_or_else(bad)? }
        }
        None => Chain { chain: hex(&random(16)), seq: 0, prev: ZERO.into() },
    };
    let mut out = String::new();
    let now = iso_now();
    if chain.seq == 0 {
        out.push_str(&chain.record("genesis", fields([("createdAt", now.clone().into())]), &now)?);
    }
    if let Some(b) = broken {
        out.push_str(&chain.record("lock-broken", fields([("holderPid", b.pid.map_or(Value::Null, Value::Int)), ("ageMs", Value::Int(b.age_ms))]), &now)?);
    }
    out.push_str(&chain.record(
        "run",
        fields([("run", run.into()), ("runChain", journal.chain.clone().into()), ("runLength", Value::Int(journal.seq)), ("runTip", journal.prev.clone().into())]),
        &now,
    )?);
    let mut f = OpenOptions::new().append(true).create(true).open(&path)?;
    f.write_all(out.as_bytes())?;
    f.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> PathBuf {
        let p = std::env::temp_dir().join(format!("iw-evidence-{}", hex(&random(6))));
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn canonical_form_matches_javascript() {
        let v = Value::Obj(fields([("b", Value::Int(2)), ("a", "q\"\\\n\u{1}é".into()), ("Z", Value::Arr(vec![Value::Null, Value::Bool(true)]))]));
        assert_eq!(canonical(&v), "{\"Z\":[null,true],\"a\":\"q\\\"\\\\\\n\\u0001é\",\"b\":2}");
    }

    #[test]
    fn the_hash_is_over_the_domain_tag_and_the_record_without_its_hash() {
        let r = fields([("kind", "abend".into()), ("code", "S0C7".into()), ("hash", "ignored".into())]);
        assert_eq!(record_hash(&r), hex(&sha256(b"cobolwork-evidence/v1\n{\"code\":\"S0C7\",\"kind\":\"abend\"}")));
    }

    #[test]
    fn a_closed_journal_chains_and_reaches_the_ledger() {
        let dir = temp();
        let mut j = Journal::create(&dir.join("ev"), &[], "run", &["run".into()], "0.0.0").unwrap();
        j.append("dd", fields([("dd", "IN".into()), ("event", "open".into()), ("mode", "INPUT".into()), ("sha256", hex(&sha256(b"x")).into()), ("bytes", Value::Int(1))])).unwrap();
        let path = j.path.clone();
        let id = j.id.clone();
        assert_eq!(j.close(Some(0)).unwrap(), Ledger::Recorded);
        let lines: Vec<String> = fs::read_to_string(&path).unwrap().lines().map(String::from).collect();
        assert_eq!(lines.len(), 3);
        let mut prev = ZERO.to_string();
        for (i, line) in lines.iter().enumerate() {
            assert_eq!(field_of(line, "seq").unwrap(), i.to_string());
            assert_eq!(field_of(line, "prev").unwrap(), prev);
            prev = field_of(line, "hash").unwrap();
        }
        let ledger = fs::read_to_string(dir.join("ev").join(LEDGER)).unwrap();
        let run = ledger.lines().last().unwrap();
        assert_eq!(field_of(run, "run").unwrap(), id);
        assert_eq!(field_of(run, "runTip").unwrap(), prev);
        assert_eq!(field_of(run, "runLength").unwrap(), "3");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_unknown_field_is_refused() {
        let dir = temp();
        let mut j = Journal::create(&dir.join("ev"), &[], "run", &[], "0.0.0").unwrap();
        assert!(j.append("dd", fields([("dd", "IN".into()), ("event", "open".into()), ("record", "SECRET".into())])).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    /// cobolwork's verifier refuses a kind it does not know, a field it does not list, and a record
    /// without a field it requires.
    #[test]
    fn every_kind_ironwork_writes_is_one_cobolworks_verifier_accepts() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/cobolwork/evidence/kinds.tsv");
        let table = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let words = |cell: &str| cell.split(' ').filter(|w| !w.is_empty()).map(String::from).collect::<Vec<_>>();
        let theirs: BTreeMap<&str, (&str, Vec<String>, Vec<String>)> = table
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| match l.split('\t').collect::<Vec<_>>()[..] {
                [kind, file, fields, required] => (kind, (file, words(fields), words(required))),
                ref row => panic!("a row has four cells: {row:?}"),
            })
            .collect();
        for (kind, file, allowed, required) in KINDS {
            let (their_file, their_fields, their_required) = theirs.get(kind).unwrap_or_else(|| panic!("cobolwork's verifier knows no {kind} record"));
            assert_eq!(file, *their_file, "{kind}: cobolwork reads it from the {their_file}");
            for field in allowed {
                assert!(their_fields.iter().any(|f| f == field), "{kind}: cobolwork's verifier refuses {field}");
            }
            for field in their_required {
                assert!(required.contains(&field.as_str()), "{kind}: cobolwork's verifier requires {field}, which ironwork may leave out");
            }
        }
    }

    #[test]
    fn an_evidence_directory_inside_a_tree_read_is_refused() {
        let dir = temp();
        assert!(prepare(&dir.join("src").join("ev"), std::slice::from_ref(&dir)).is_err());
        assert!(!dir.join("src").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    fn closed(ev: &Path) -> Ledger {
        Journal::create(ev, &[], "run", &[], "0.0.0").unwrap().close(Some(0)).unwrap()
    }

    fn ledger_lines(ev: &Path) -> Vec<String> {
        fs::read_to_string(ev.join(LEDGER)).unwrap().lines().map(String::from).collect()
    }

    #[test]
    fn an_empty_lock_is_aged_from_its_modification_time_and_names_no_holder() {
        let dir = temp();
        let ev = dir.join("ev");
        prepare(&ev, &[]).unwrap();
        let lock = File::create(ev.join(LOCK)).unwrap();
        lock.set_modified(SystemTime::now() - Duration::from_secs(120)).unwrap();
        drop(lock);
        assert_eq!(closed(&ev), Ledger::Recorded);
        assert!(!ev.join(LOCK).exists());
        let lines = ledger_lines(&ev);
        assert_eq!(lines.iter().map(|l| field_of(l, "kind").unwrap()).collect::<Vec<_>>(), ["genesis", "lock-broken", "run"]);
        assert!(lines[1].contains("\"holderPid\":null"), "{}", lines[1]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_stale_lock_is_aged_from_the_time_it_holds_and_names_its_holder() {
        let dir = temp();
        let ev = dir.join("ev");
        prepare(&ev, &[]).unwrap();
        let mut gone = std::process::Command::new(std::env::current_exe().unwrap()).arg("--list").stdout(std::process::Stdio::null()).spawn().unwrap();
        gone.wait().unwrap();
        fs::write(ev.join(LOCK), format!("{} {}\n", gone.id(), now_ms() - 120_000)).unwrap();
        assert_eq!(closed(&ev), Ledger::Recorded);
        let lines = ledger_lines(&ev);
        assert_eq!(field_of(&lines[1], "kind").unwrap(), "lock-broken");
        assert_eq!(field_of(&lines[1], "holderPid").unwrap(), gone.id().to_string());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_stale_lock_whose_holder_is_running_is_waited_for() {
        let dir = temp();
        let lock = dir.join(LOCK);
        let held = format!("{} {}\n", std::process::id(), now_ms() - 120_000);
        fs::write(&lock, &held).unwrap();
        assert!(take_lock(&lock, Duration::from_millis(100)).is_err());
        assert_eq!(fs::read_to_string(&lock).unwrap(), held);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_lock_taken_after_a_stale_one_was_read_is_put_back() {
        let dir = temp();
        let lock = dir.join(LOCK);
        fs::write(&lock, format!("999999 {}\n", now_ms() - 120_000)).unwrap();
        let stale = read_lock(&lock).unwrap();
        fs::remove_file(&lock).unwrap();
        let peer = format!("{} {}\n", std::process::id(), now_ms());
        fs::write(&lock, &peer).unwrap();
        let locks = || fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).filter(|n| n.starts_with(LOCK)).collect::<Vec<_>>();
        assert!(!break_stale(&lock, &stale));
        assert_eq!(fs::read_to_string(&lock).unwrap(), peer);
        assert_eq!(locks(), [LOCK]);
        assert!(break_stale(&lock, &read_lock(&lock).unwrap()));
        assert!(locks().is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_ledger_whose_last_line_is_no_ledger_record_is_not_extended() {
        let dir = temp();
        let ev = dir.join("ev");
        prepare(&ev, &[]).unwrap();
        for tail in ["{\"seq\":0}\n".to_string(), format!("{{\"chain\":\"{}\",\"hash\":\"{}\",\"seq\":0}}\n", "a".repeat(31), "b".repeat(64)), format!("{}\n", "x".repeat(70_000))] {
            fs::write(ev.join(LEDGER), &tail).unwrap();
            assert!(matches!(closed(&ev), Ledger::Unrecorded(_)), "{}", &tail[..20]);
            assert_eq!(fs::read_to_string(ev.join(LEDGER)).unwrap(), tail);
        }
        fs::remove_dir_all(dir).unwrap();
    }
}
