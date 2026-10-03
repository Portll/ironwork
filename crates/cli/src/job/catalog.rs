//! The catalog entries of VSAM objects. Each is a file beside the object's name holding the
//! DEFINE that made it, read back through the IDCAMS parser; the suffix's qualifier is longer
//! than eight characters, so no data set name can be one. A cluster's and an alternate index's
//! records are the data set at the name; a path has an entry and no data set.

use jcl::idcams::{AlternateIndex, Cluster, Command, Components, Keys, Organization, Path as PathEntry};
use std::fs;
use std::path::{Path, PathBuf};
use zarch::ebcdic::{self, CodePage};

const SUFFIX: &str = ".catalog-entry";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Entry {
    Cluster(Cluster),
    AlternateIndex(AlternateIndex),
    Path(PathEntry),
}

impl Entry {
    pub(super) fn name(&self) -> &str {
        match self {
            Entry::Cluster(c) => &c.name,
            Entry::AlternateIndex(a) => &a.name,
            Entry::Path(p) => &p.name,
        }
    }

    fn cards(&self) -> Vec<String> {
        let organization = |o: Organization| match o {
            Organization::Indexed => "INDEXED",
            Organization::Nonindexed => "NONINDEXED",
            Organization::Numbered => "NUMBERED",
            Organization::Linear => "LINEAR",
        };
        let keys = |k: Keys| format!("KEYS({} {})", k.length, k.offset);
        let named = |c: &Components| c.data.iter().map(|n| format!("DATA(NAME({n}))")).chain(c.index.iter().map(|n| format!("INDEX(NAME({n}))"))).collect::<Vec<_>>();
        let parts = match self {
            Entry::Cluster(c) => {
                let mut p = vec![format!("CLUSTER(NAME({})", c.name), organization(c.organization).to_string()];
                p.extend(c.keys.map(keys));
                p.push(format!("RECORDSIZE({} {}))", c.record_size.average, c.record_size.maximum));
                p.extend(named(&c.components));
                p
            }
            Entry::AlternateIndex(a) => {
                let mut p = vec![
                    format!("ALTERNATEINDEX(NAME({})", a.name),
                    format!("RELATE({})", a.relate),
                    keys(a.keys),
                    if a.unique { "UNIQUEKEY" } else { "NONUNIQUEKEY" }.into(),
                    if a.upgrade { "UPGRADE" } else { "NOUPGRADE" }.into(),
                    if a.reuse { "REUSE" } else { "NOREUSE" }.into(),
                    format!("RECORDSIZE({} {}))", a.record_size.average, a.record_size.maximum),
                ];
                p.extend(named(&a.components));
                p
            }
            Entry::Path(p) => vec![format!("PATH(NAME({})", p.name), format!("PATHENTRY({})", p.entry), format!("{})", if p.update { "UPDATE" } else { "NOUPDATE" })],
        };
        let last = parts.len() - 1;
        std::iter::once(" DEFINE -".to_string()).chain(parts.into_iter().enumerate().map(|(i, p)| format!("   {p}{}", if i < last { " -" } else { "" }))).collect()
    }
}

/// The name of a cluster's or an alternate index's data or index component: the one DEFINE gave,
/// or the one VSAM generates (z/OS 3.1 DFSMS Using Data Sets, "Naming a cluster"). VSAM makes up
/// the qualifiers it adds to a name longer than 42 characters; ironwork derives them from the
/// name (C352).
pub(super) fn component_name(entry: &Entry, data: bool) -> Option<String> {
    let (name, given) = match entry {
        Entry::Cluster(c) => (&c.name, &c.components),
        Entry::AlternateIndex(a) => (&a.name, &a.components),
        Entry::Path(_) => return None,
    };
    if let Some(n) = if data { &given.data } else { &given.index } {
        return Some(n.clone());
    }
    let qualifiers: Vec<&str> = name.split('.').collect();
    let (long, short) = if data { ("DATA", "D") } else { ("INDEX", "I") };
    Some(if qualifiers.last() == Some(&"CLUSTER") {
        format!("{}.{long}", qualifiers[..qualifiers.len() - 1].join("."))
    } else if name.len() <= 38 {
        format!("{name}.{long}")
    } else if name.len() <= 42 {
        format!("{name}.{short}")
    } else {
        let kept = &qualifiers[..(qualifiers.len() - 1).min(4)];
        let mut out: Vec<String> = kept.iter().map(|q| q.to_string()).collect();
        let mut seed = name.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)) ^ u64::from(data);
        while out.len() < 5 {
            let qualifier: String = (0..8)
                .map(|i| {
                    seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                    let n = (seed >> 33) as usize;
                    if i == 0 { b"ABCDEFGHIJKLMNOPQRSTUVWXYZ"[n % 26] as char } else { b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"[n % 36] as char }
                })
                .collect();
            out.push(qualifier);
        }
        out.join(".")
    })
}

pub(super) fn entry_file(datasets: &Path, name: &str) -> PathBuf {
    datasets.join(format!("{name}{SUFFIX}"))
}

/// Whether a file in the data set directory is a catalog entry rather than a data set.
pub(super) fn is_entry_file(file_name: &str) -> bool {
    file_name.ends_with(SUFFIX)
}

pub(super) fn get(datasets: &Path, name: &str) -> Option<Entry> {
    let text = fs::read_to_string(entry_file(datasets, name)).ok()?;
    let cards: Vec<String> = text.lines().map(str::to_string).collect();
    match jcl::idcams::parse(&cards).ok()?.pop()? {
        Command::DefineCluster(c) => Some(Entry::Cluster(c)),
        Command::DefineAlternateIndex(a) => Some(Entry::AlternateIndex(a)),
        Command::DefinePath(p) => Some(Entry::Path(p)),
        _ => None,
    }
}

pub(super) fn put(datasets: &Path, entry: &Entry) -> std::io::Result<()> {
    let mut text = entry.cards().join("\n");
    text.push('\n');
    fs::write(entry_file(datasets, entry.name()), text)
}

pub(super) fn remove(datasets: &Path, name: &str) -> std::io::Result<()> {
    fs::remove_file(entry_file(datasets, name))
}

/// Every entry in the directory, by name.
pub(super) fn all(datasets: &Path) -> Vec<Entry> {
    let mut names: Vec<String> = fs::read_dir(datasets).into_iter().flatten().flatten().filter_map(|e| e.file_name().to_str()?.strip_suffix(SUFFIX).map(str::to_string)).collect();
    names.sort();
    names.iter().filter_map(|n| get(datasets, n)).collect()
}

/// The alternate indexes over a base cluster.
pub(super) fn alternate_indexes(datasets: &Path, base: &str) -> Vec<AlternateIndex> {
    all(datasets).into_iter().filter_map(|e| if let Entry::AlternateIndex(a) = e && a.relate == base { Some(a) } else { None }).collect()
}

/// The paths through an alternate index.
pub(super) fn paths_through(datasets: &Path, aix: &str) -> Vec<PathEntry> {
    all(datasets).into_iter().filter_map(|e| if let Entry::Path(p) = e && p.entry == aix { Some(p) } else { None }).collect()
}

/// How a VSAM data set holds its records: a text data set's lines through the code page, padded
/// with blanks to the longest record so a key past a line's end reads blanks (an empty line, a
/// relative record cluster's empty slot, stays empty), records of one
/// length back to back when RECORDSIZE's average is its maximum, or each behind
/// an RDW. An alternate index's records are always behind RDWs, since they hold binary counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Form {
    Lines(usize),
    Fixed(usize),
    Variable,
}

pub(super) fn form(entry: &Entry, text: bool) -> Form {
    match entry {
        Entry::AlternateIndex(_) => Form::Variable,
        Entry::Cluster(c) if text => Form::Lines(c.record_size.maximum),
        Entry::Cluster(c) if c.record_size.average == c.record_size.maximum => Form::Fixed(c.record_size.maximum),
        _ => Form::Variable,
    }
}

/// The records of a VSAM data set, each without its RDW.
pub(super) fn read(path: &Path, form: Form, page: &CodePage) -> Result<Vec<Vec<u8>>, String> {
    let shown = path.display();
    let bytes = fs::read(path).map_err(|e| format!("{shown}: {e}"))?;
    match form {
        Form::Lines(longest) => Ok(String::from_utf8_lossy(&bytes)
            .lines()
            .map(|l| {
                let mut r = page.encode_lossy(l);
                if !r.is_empty() {
                    r.resize(r.len().max(longest), ebcdic::SPACE);
                }
                r
            })
            .collect()),
        Form::Fixed(n) => {
            if !bytes.len().is_multiple_of(n) {
                return Err(format!("{shown} holds {} bytes, not a whole number of {n}-byte records", bytes.len()));
            }
            Ok(bytes.chunks(n).map(<[u8]>::to_vec).collect())
        }
        Form::Variable => {
            let (mut out, mut at) = (Vec::new(), 0);
            while at < bytes.len() {
                let len = bytes.get(at..at + 2).map(|b| usize::from(u16::from_be_bytes([b[0], b[1]]))).filter(|&l| l >= 4 && at + l <= bytes.len()).ok_or_else(|| format!("{shown} has a record descriptor word that is not valid at byte {at}"))?;
                out.push(bytes[at + 4..at + len].to_vec());
                at += len;
            }
            Ok(out)
        }
    }
}

pub(super) fn write(path: &Path, form: Form, records: &[Vec<u8>], page: &CodePage) -> Result<(), String> {
    let mut bytes = Vec::new();
    for r in records {
        match form {
            Form::Lines(_) => {
                bytes.extend(page.decode(r).trim_end().as_bytes());
                bytes.push(b'\n');
            }
            Form::Fixed(n) => {
                bytes.extend(r.iter().take(n));
                bytes.resize(bytes.len() + n.saturating_sub(r.len()), ebcdic::SPACE);
            }
            Form::Variable => {
                let len = u16::try_from(r.len() + 4).map_err(|_| format!("{}: a record of {} bytes is longer than an RDW can describe", path.display(), r.len()))?;
                bytes.extend(len.to_be_bytes());
                bytes.extend([0, 0]);
                bytes.extend(r);
            }
        }
    }
    fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// The bytes of a key in a record, or None when the record ends before the key does.
pub(super) fn key_of(record: &[u8], keys: Keys) -> Option<&[u8]> {
    record.get(keys.offset..keys.offset + keys.length)
}
