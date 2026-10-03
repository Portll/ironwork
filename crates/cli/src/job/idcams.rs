//! IDCAMS over a step's SYSIN, and the alternate indexes and paths it defines: BLDINDEX builds an
//! alternate index's records from its base cluster, a change to a base cluster rebuilds the
//! alternate indexes in its upgrade set, and a DD that names a path reads the base cluster's
//! records in the order of the path's alternate index.

use super::catalog::{self, Entry, Form};
use super::print::{Input, Kind};
use super::{Allocated, Runner, delete_data_set, gdg_text, put, write_print};
use jcl::idcams::{AlternateIndex, Cluster, Command, Keys, Organization, Path as PathEntry, Target};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use zarch::ebcdic::CodePage;

/// What the commands write: lines for SYSPRINT, and the listings LISTCAT and PRINT send to an
/// OUTFILE, whose messages still go to SYSPRINT.
#[derive(Default)]
struct Listing {
    sysprint: Vec<String>,
    files: BTreeMap<String, Vec<String>>,
}

impl Listing {
    fn to(&mut self, dd: Option<&str>) -> &mut Vec<String> {
        match dd {
            Some(n) => self.files.entry(n.to_string()).or_default(),
            None => &mut self.sysprint,
        }
    }
}

/// IDCAMS over the step's SYSIN: each command's condition code is LASTCC, the highest is MAXCC
/// and the step's return code, and a code of 16 ends the commands. Messages go to SYSPRINT.
pub(super) fn run(runner: &Runner<'_>, dds: &[Allocated]) -> i16 {
    let step = Step { runner, dds, page: page() };
    let mut out = Listing::default();
    let cards = step.dd("SYSIN").and_then(|d| fs::read_to_string(&d.path).ok()).map(|t| t.lines().map(str::to_string).collect::<Vec<_>>()).unwrap_or_default();
    let commands = match jcl::idcams::parse(&cards) {
        Ok(c) => c,
        Err(e) => {
            out.sysprint.push(format!("IDCAMS: {e}"));
            out.sysprint.push("IDC0002I IDCAMS PROCESSING COMPLETE. MAXIMUM CONDITION CODE WAS 12".into());
            write_print(step.dd("SYSPRINT"), &out.sysprint);
            return 12;
        }
    };
    let mut cc = (0u16, 0u16);
    step.commands(&commands, &mut cc, &mut out);
    out.sysprint.push(format!("IDC0002I IDCAMS PROCESSING COMPLETE. MAXIMUM CONDITION CODE WAS {}", cc.1));
    write_print(step.dd("SYSPRINT"), &out.sysprint);
    for (name, lines) in &out.files {
        write_print(step.dd(name), lines);
    }
    cc.1 as i16
}

pub(super) fn page() -> &'static CodePage {
    numeric::options::Options::default().code_page()
}

struct Step<'s> {
    runner: &'s Runner<'s>,
    dds: &'s [Allocated],
    page: &'static CodePage,
}

fn shown(t: &Target) -> String {
    match t {
        Target::Dd(n) => format!("DD {n}"),
        Target::Dataset(n) => n.clone(),
    }
}

impl Step<'_> {
    fn dd(&self, name: &str) -> Option<&Allocated> {
        self.dds.iter().find(|d| d.name == name)
    }

    /// The file a target names, whether it holds UTF-8 lines, and whether its DD is DISP=MOD.
    fn file(&self, t: &Target) -> Option<(PathBuf, bool, bool)> {
        match t {
            Target::Dd(n) => self.dd(n).map(|d| (d.path.clone(), d.text, d.append)),
            Target::Dataset(name) => Some((self.runner.catalog_path(name), self.runner.req.text, false)),
        }
    }

    /// The data set a target names: its name, or that of the data set its DD allocates.
    fn name(&self, t: &Target) -> Option<String> {
        match t {
            Target::Dataset(n) => Some(n.clone()),
            Target::Dd(n) => self.runner.name_of(&self.dd(n)?.path),
        }
    }

    fn commands(&self, commands: &[Command], cc: &mut (u16, u16), out: &mut Listing) {
        for c in commands {
            if cc.1 >= 16 {
                return;
            }
            match c {
                Command::Set { max: true, value } => cc.1 = *value,
                Command::Set { max: false, value } => {
                    cc.0 = *value;
                    cc.1 = cc.1.max(*value);
                }
                Command::If { max, op, value, then, otherwise } => {
                    let tested = if *max { cc.1 } else { cc.0 };
                    let branch = if op.holds(tested, *value) { then } else { otherwise };
                    self.commands(branch, cc, out);
                }
                other => {
                    let code = self.act(other, out);
                    out.sysprint.push(if code >= 12 { format!("IDC3003I FUNCTION TERMINATED. CONDITION CODE IS {code}") } else { format!("IDC0001I FUNCTION COMPLETED, HIGHEST CONDITION CODE WAS {code}") });
                    cc.0 = code;
                    cc.1 = cc.1.max(code);
                }
            }
        }
    }

    fn act(&self, c: &Command, out: &mut Listing) -> u16 {
        match c {
            Command::Delete(names) => names.iter().map(|n| self.delete(n, &mut out.sysprint)).max().unwrap_or(0),
            Command::DefineGdg { name, limit, scratch, empty } => {
                let path = self.runner.catalog_path(name);
                if path.exists() {
                    out.sysprint.push(format!("ironwork: DEFINE GDG {name}: the name is in use"));
                    return 8;
                }
                match fs::write(&path, gdg_text(*limit, *scratch, *empty)) {
                    Ok(()) => 0,
                    Err(e) => {
                        out.sysprint.push(format!("ironwork: DEFINE GDG {name}: {e}"));
                        12
                    }
                }
            }
            Command::DefineCluster(c) => self.define(Entry::Cluster(c.clone()), &mut out.sysprint),
            Command::DefineAlternateIndex(a) => match self.cluster(&a.relate) {
                Some(base) if base.organization == Organization::Indexed => {
                    if a.keys.offset + a.keys.length > base.record_size.maximum {
                        out.sysprint.push(format!("ironwork: DEFINE ALTERNATEINDEX {}: KEYS({} {}) does not fit in {}'s records of {} bytes", a.name, a.keys.length, a.keys.offset, base.name, base.record_size.maximum));
                        return 12;
                    }
                    self.define(Entry::AlternateIndex(a.clone()), &mut out.sysprint)
                }
                Some(base) => {
                    out.sysprint.push(format!("ironwork: DEFINE ALTERNATEINDEX {}: {} is not a key-sequenced cluster; an alternate index over another is not supported yet", a.name, base.name));
                    12
                }
                None => {
                    out.sysprint.push(format!("IDC3012I ENTRY {} NOT FOUND", a.relate));
                    8
                }
            },
            Command::DefinePath(p) => match catalog::get(&self.runner.datasets, &p.entry) {
                Some(Entry::AlternateIndex(_) | Entry::Cluster(_)) => self.define(Entry::Path(p.clone()), &mut out.sysprint),
                Some(Entry::Path(_)) => {
                    out.sysprint.push(format!("ironwork: DEFINE PATH {}: {} is a path; PATHENTRY names an alternate index or a cluster", p.name, p.entry));
                    12
                }
                None => {
                    out.sysprint.push(format!("IDC3012I ENTRY {} NOT FOUND", p.entry));
                    8
                }
            },
            Command::Bldindex { from, to } => self.bldindex(from, to, &mut out.sysprint),
            Command::Repro { from, to } => self.repro(from, to, &mut out.sysprint),
            Command::Listcat(l) => {
                let (mut listing, mut messages) = (Vec::new(), Vec::new());
                let code = super::listcat::listcat(self.runner, l, &mut listing, &mut messages);
                out.to(l.out.as_deref()).extend(listing);
                out.sysprint.extend(messages);
                code
            }
            Command::Print(p) => match self.input(&p.from) {
                Ok((name, records, kind)) => {
                    let (mut listing, mut messages) = (Vec::new(), Vec::new());
                    let code = super::print::print(p, &Input { name: &name, records, kind }, self.page, &mut listing, &mut messages);
                    out.to(p.out.as_deref()).extend(listing);
                    out.sysprint.extend(messages);
                    code
                }
                Err(e) => {
                    out.sysprint.push(format!("ironwork: PRINT {}: {e}", shown(&p.from)));
                    12
                }
            },
            Command::Set { .. } | Command::If { .. } => 0,
        }
    }

    fn cluster(&self, name: &str) -> Option<Cluster> {
        match catalog::get(&self.runner.datasets, name)? {
            Entry::Cluster(c) => Some(c),
            _ => None,
        }
    }

    /// Catalogs an entry, and makes an empty data set for a cluster or an alternate index. A
    /// cluster defined with RECATALOG catalogs the data set already there, records and all.
    fn define(&self, entry: Entry, print: &mut Vec<String>) -> u16 {
        let (name, dir) = (entry.name().to_string(), &self.runner.datasets);
        let path = self.runner.catalog_path(&name);
        let what = match &entry {
            Entry::Cluster(_) => "CLUSTER",
            Entry::AlternateIndex(_) => "ALTERNATEINDEX",
            Entry::Path(_) => "PATH",
        };
        let recatalog = matches!(&entry, Entry::Cluster(c) if c.recatalog);
        if catalog::get(dir, &name).is_some() || path.exists() && !recatalog {
            print.push(format!("ironwork: DEFINE {what} {name}: the data set exists"));
            return 8;
        }
        if recatalog && !path.is_file() {
            print.push(format!("ironwork: DEFINE CLUSTER {name} RECATALOG: there is no data set to catalog"));
            return 12;
        }
        let entry = match entry {
            Entry::Cluster(c) => Entry::Cluster(Cluster { recatalog: false, ..c }),
            other => other,
        };
        let made = match &entry {
            Entry::Path(_) => Ok(()),
            _ if recatalog => Ok(()),
            _ => fs::write(&path, b"").and_then(|()| exec::files::clear_open_mark(&path)),
        };
        match made.and_then(|()| catalog::put(dir, &entry)) {
            Ok(()) => 0,
            Err(e) => {
                print.push(format!("ironwork: DEFINE {what} {name}: {e}"));
                12
            }
        }
    }

    /// DELETE of one entry. A cluster takes its alternate indexes and paths with it, an alternate
    /// index its paths, and each its data and index components; a path takes nothing else.
    fn delete(&self, name: &str, print: &mut Vec<String>) -> u16 {
        let (runner, dir) = (self.runner, &self.runner.datasets);
        let Some(entry) = catalog::get(dir, name) else {
            if runner.gdg(name).is_some() {
                for number in runner.generations(name) {
                    let _ = fs::remove_file(runner.datasets.join(format!("{name}.G{number:04}V00")));
                }
            }
            if delete_data_set(&runner.catalog_path(name)).is_err() {
                print.push(format!("IDC3012I ENTRY {name} NOT FOUND"));
                return 8;
            }
            print.push(format!("IDC0550I ENTRY (A) {name} DELETED"));
            return 0;
        };
        if !matches!(entry, Entry::Path(_)) {
            for p in catalog::paths_through(dir, name) {
                self.remove(&Entry::Path(p), print);
            }
        }
        if let Entry::Cluster(c) = &entry {
            for a in catalog::alternate_indexes(dir, &c.name) {
                for p in catalog::paths_through(dir, &a.name) {
                    self.remove(&Entry::Path(p), print);
                }
                self.remove(&Entry::AlternateIndex(a), print);
            }
        }
        self.remove(&entry, print);
        0
    }

    /// Removes one entry, its data set and its components, each with its IDC0550I.
    fn remove(&self, entry: &Entry, print: &mut Vec<String>) {
        let name = entry.name();
        let (letter, indexed) = match entry {
            Entry::Path(_) => ('R', false),
            Entry::AlternateIndex(_) => ('G', true),
            Entry::Cluster(c) => ('C', c.organization == Organization::Indexed),
        };
        if !matches!(entry, Entry::Path(_)) {
            let _ = delete_data_set(&self.runner.catalog_path(name));
            if let Some(data) = catalog::component_name(entry, true) {
                print.push(format!("IDC0550I ENTRY (D) {data} DELETED"));
            }
            if let Some(index) = catalog::component_name(entry, false).filter(|_| indexed) {
                print.push(format!("IDC0550I ENTRY (I) {index} DELETED"));
            }
        }
        let _ = catalog::remove(&self.runner.datasets, name);
        print.push(format!("IDC0550I ENTRY ({letter}) {name} DELETED"));
    }

    fn repro(&self, from: &Target, to: &Target, print: &mut Vec<String>) -> u16 {
        let (Some((source, source_text, _)), Some((target, target_text, append))) = (self.file(from), self.file(to)) else {
            print.push(format!("ironwork: REPRO: {} or {} is not allocated to the step", shown(from), shown(to)));
            return 12;
        };
        if source_text != target_text {
            print.push(format!("ironwork: REPRO from {} to {}: one holds UTF-8 lines and the other z/OS records", shown(from), shown(to)));
            return 12;
        }
        if !target.is_file() {
            print.push(format!("ironwork: REPRO: {} does not exist", shown(to)));
            return 12;
        }
        if let Err(e) = fs::read(&source).and_then(|b| put(&target, &b, append)).and_then(|()| exec::files::clear_open_mark(&target)) {
            print.push(format!("ironwork: REPRO from {} to {}: {e}", shown(from), shown(to)));
            return 12;
        }
        match self.name(to).filter(|n| self.cluster(n).is_some()).map(|n| upgrade(self.runner, &n)) {
            Some(Err(e)) => {
                print.push(format!("ironwork: REPRO to {}: {e}", shown(to)));
                12
            }
            _ => 0,
        }
    }

    fn bldindex(&self, from: &Target, to: &[Target], print: &mut Vec<String>) -> u16 {
        let Some(base) = self.name(from).and_then(|n| self.cluster(&n)) else {
            print.push(format!("ironwork: BLDINDEX: {} is not a cluster ironwork's catalog holds", shown(from)));
            return 12;
        };
        let mut code = 0;
        for t in to {
            let dir = &self.runner.datasets;
            let aix = self.name(t).and_then(|n| match catalog::get(dir, &n)? {
                Entry::Path(p) => match catalog::get(dir, &p.entry)? {
                    Entry::AlternateIndex(a) => Some(a),
                    _ => None,
                },
                Entry::AlternateIndex(a) => Some(a),
                Entry::Cluster(_) => None,
            });
            let aix = aix.filter(|a| a.relate == base.name);
            let Some(aix) = aix else {
                print.push(format!("ironwork: BLDINDEX: {} is not an alternate index over {}", shown(t), base.name));
                code = code.max(12);
                continue;
            };
            let holds_records = |name: &str| fs::metadata(self.runner.catalog_path(name)).is_ok_and(|m| m.len() > 0);
            if !holds_records(&base.name) {
                print.push(format!("ironwork: BLDINDEX: {} holds no records; BLDINDEX needs at least one", base.name));
                code = code.max(12);
                continue;
            }
            if holds_records(&aix.name) && !aix.reuse {
                print.push(format!("ironwork: BLDINDEX: {} holds records and is not defined with REUSE", aix.name));
                code = code.max(12);
                continue;
            }
            match build(self.runner, &base, &aix) {
                Ok(built) => {
                    print.extend(built.messages);
                    code = code.max(built.code);
                }
                Err(e) => {
                    print.push(format!("ironwork: BLDINDEX {}: {e}", aix.name));
                    code = code.max(12);
                }
            }
        }
        code
    }

    /// PRINT's input: a catalogued object's records, a key-sequenced cluster's in key order as
    /// VSAM reads them whatever order REPRO left them in, or those of a DD or data set ironwork's
    /// catalog does not hold, by the DD's record format.
    fn input(&self, from: &Target) -> Result<(String, Vec<Vec<u8>>, Kind), String> {
        let (runner, page) = (self.runner, self.page);
        let name = self.name(from);
        match name.as_deref().and_then(|n| catalog::get(&runner.datasets, n)) {
            Some(Entry::Path(p)) => {
                let view = view(runner, &p)?;
                Ok((p.name, view.records, Kind::Keyed(view.keys)))
            }
            Some(entry) => {
                let mut records = catalog::read(&runner.catalog_path(entry.name()), catalog::form(&entry, runner.req.text), page)?;
                if let Entry::Cluster(Cluster { keys: Some(k), .. }) = &entry {
                    records.sort_by(|a, b| catalog::key_of(a, *k).cmp(&catalog::key_of(b, *k)));
                }
                let kind = match &entry {
                    Entry::Cluster(c) => match (c.organization, c.keys) {
                        (Organization::Indexed, Some(k)) => Kind::Keyed(k),
                        (Organization::Numbered, _) => Kind::Numbered,
                        (Organization::Linear, _) => return Err(format!("{} is a linear data set, which has no records", c.name)),
                        _ => Kind::Entry,
                    },
                    Entry::AlternateIndex(a) => Kind::Keyed(Keys { length: a.keys.length, offset: AIX_HEADER }),
                    Entry::Path(_) => unreachable!(),
                };
                Ok((entry.name().to_string(), records, kind))
            }
            None => {
                let dd = match from {
                    Target::Dd(n) => self.dd(n).ok_or_else(|| format!("DD {n} is not allocated to the step"))?,
                    Target::Dataset(n) if !runner.req.text => return Err(format!("the record format of {n} is not known; name it through a DD with DCB=(RECFM=,LRECL=)")),
                    Target::Dataset(n) => {
                        let records = String::from_utf8_lossy(&fs::read(runner.catalog_path(n)).map_err(|e| format!("{n}: {e}"))?).lines().map(|l| page.encode_lossy(l)).collect();
                        return Ok((n.clone(), records, Kind::Nonvsam));
                    }
                };
                let layout = super::layout(dd, None)?;
                let variable = matches!(layout, super::Layout::Variable);
                let records = super::read_records(dd, &layout, page)?.into_iter().map(|r| if variable { r[4.min(r.len())..].to_vec() } else { r }).collect();
                Ok((name.unwrap_or_else(|| shown(from)), records, Kind::Nonvsam))
            }
        }
    }
}

/// The bytes before the alternate key in each alternate index record: a flag byte, the number of
/// pointers in a halfword, the length of a pointer and the length of the alternate key.
pub(super) const AIX_HEADER: usize = 5;

/// The flag byte of an alternate index over a key-sequenced cluster, whose pointers are prime keys.
const KSDS_POINTERS: u8 = 0x01;

/// Up to the first ten bytes of a key in hexadecimal, as BLDINDEX's messages show one.
fn shown_key(key: &[u8]) -> String {
    key.iter().take(10).map(|b| format!("{b:02X}")).collect()
}

/// The most prime keys one alternate index record holds.
const MAX_POINTERS: usize = 32_767;

/// What BLDINDEX reported, and its condition code.
pub(super) struct Built {
    pub messages: Vec<String>,
    pub code: u16,
}

/// Builds the alternate index's records from its base cluster's: one per alternate key, holding
/// the prime keys of the records that have it in ascending order, as BLDINDEX sorts the
/// key-pointer pairs. A record too short to hold the alternate key is left out, a unique key
/// keeps its first prime key, and a record keeps the pointers that fit; each is a non-ending
/// error with condition code 4 (C353).
pub(super) fn build(runner: &Runner<'_>, base: &Cluster, aix: &AlternateIndex) -> Result<Built, String> {
    let (page, text) = (page(), runner.req.text);
    let prime = base.keys.ok_or_else(|| format!("{} has no prime key", base.name))?;
    let base_entry = Entry::Cluster(base.clone());
    let records = catalog::read(&runner.catalog_path(&base.name), catalog::form(&base_entry, text), page)?;
    let (mut pairs, mut messages) = (Vec::new(), Vec::new());
    for r in &records {
        match (catalog::key_of(r, aix.keys), catalog::key_of(r, prime)) {
            (Some(a), Some(p)) => pairs.push((a.to_vec(), p.to_vec())),
            _ => messages.push(format!("IDC1644I ALTERNATE INDEX KEY NOT IN BASE RECORD {}", shown_key(r.get(prime.offset..).unwrap_or_default()))),
        }
    }
    pairs.sort();
    let mut grouped: Vec<(Vec<u8>, Vec<Vec<u8>>)> = Vec::new();
    for (alternate, pointer) in pairs {
        match grouped.last_mut() {
            Some((a, pointers)) if *a == alternate => pointers.push(pointer),
            _ => grouped.push((alternate, vec![pointer])),
        }
    }
    let room = (aix.record_size.maximum.saturating_sub(AIX_HEADER + aix.keys.length) / prime.length).min(MAX_POINTERS);
    let mut out = Vec::with_capacity(grouped.len());
    for (alternate, mut pointers) in grouped {
        if aix.unique && pointers.len() > 1 {
            for extra in &pointers[1..] {
                messages.push(format!("IDC1645I NONUNIQUE AIX KEY {} PRIME KEY IS {}", shown_key(&alternate), shown_key(extra)));
            }
            pointers.truncate(1);
        }
        if pointers.len() > room {
            messages.push(format!("IDC1646I {} EXCESS PRIME KEY VALUES FOR AIX KEY {}", pointers.len() - room, shown_key(&alternate)));
            pointers.truncate(room);
        }
        let mut record = vec![KSDS_POINTERS];
        record.extend(u16::try_from(pointers.len()).unwrap_or(u16::MAX).to_be_bytes());
        record.push(prime.length as u8);
        record.push(aix.keys.length as u8);
        record.extend(&alternate);
        for p in &pointers {
            record.extend(p);
        }
        out.push(record);
    }
    catalog::write(&runner.catalog_path(&aix.name), Form::Variable, &out, page)?;
    let code = if messages.is_empty() { 0 } else { 4 };
    messages.push(if code == 0 { format!("IDC0652I {} SUCCESSFULLY BUILT", aix.name) } else { format!("IDC1653I {} BUILT WITH ERRORS", aix.name) });
    Ok(Built { messages, code })
}

/// Rebuilds the alternate indexes in a base cluster's upgrade set after the cluster changed.
pub(super) fn upgrade(runner: &Runner<'_>, base: &str) -> Result<(), String> {
    let Some(Entry::Cluster(cluster)) = catalog::get(&runner.datasets, base) else { return Ok(()) };
    for aix in catalog::alternate_indexes(&runner.datasets, base).into_iter().filter(|a| a.upgrade) {
        build(runner, &cluster, &aix)?;
    }
    Ok(())
}

/// The base cluster's records as a path presents them, and the key that orders them.
pub(super) struct View {
    pub base: Cluster,
    /// The alternate index the path goes through; None for a path over the cluster itself.
    pub aix: Option<AlternateIndex>,
    pub records: Vec<Vec<u8>>,
    pub keys: Keys,
}

/// The records a path reaches: through an alternate index, each base record its index records
/// point to, in alternate key order and, under one key, in the order of the pointers. A pointer
/// to a prime key the base cluster no longer holds reaches nothing.
pub(super) fn view(runner: &Runner<'_>, path: &PathEntry) -> Result<View, String> {
    let (dir, page, text) = (&runner.datasets, page(), runner.req.text);
    let (aix, base_name) = match catalog::get(dir, &path.entry) {
        Some(Entry::AlternateIndex(a)) => (Some(a.clone()), a.relate),
        Some(Entry::Cluster(c)) => (None, c.name),
        _ => return Err(format!("{}'s PATHENTRY {} is not in ironwork's catalog", path.name, path.entry)),
    };
    let Some(Entry::Cluster(base)) = catalog::get(dir, &base_name) else { return Err(format!("{}'s base cluster {base_name} is not in ironwork's catalog", path.name)) };
    let prime = base.keys.ok_or_else(|| format!("{base_name} has no prime key"))?;
    let records = catalog::read(&runner.catalog_path(&base_name), catalog::form(&Entry::Cluster(base.clone()), text), page)?;
    let Some(aix) = aix else {
        let mut records = records;
        records.sort_by(|a, b| catalog::key_of(a, prime).cmp(&catalog::key_of(b, prime)));
        return Ok(View { base, aix: None, records, keys: prime });
    };
    let by_prime: BTreeMap<&[u8], &Vec<u8>> = records.iter().filter_map(|r| Some((catalog::key_of(r, prime)?, r))).collect();
    let mut seen = Vec::new();
    for index in catalog::read(&runner.catalog_path(&aix.name), Form::Variable, page)? {
        let (count, width) = match index.get(..AIX_HEADER) {
            Some(h) => (usize::from(u16::from_be_bytes([h[1], h[2]])), usize::from(h[3])),
            None => return Err(format!("{} holds a record shorter than its header", aix.name)),
        };
        let pointers = index.get(AIX_HEADER + usize::from(index[4])..).unwrap_or_default();
        for p in pointers.chunks(width.max(1)).take(count) {
            if let Some(r) = by_prime.get(p) {
                seen.push((*r).clone());
            }
        }
    }
    Ok(View { base, keys: aix.keys, aix: Some(aix), records: seen })
}

/// Carries what a program wrote through a path back to its base cluster: records by prime key,
/// those it removed taken out, then the path's alternate index rebuilt and, for a path defined
/// with UPDATE, the rest of the upgrade set.
pub(super) fn write_through(runner: &Runner<'_>, path: &PathEntry, before: &[Vec<u8>], after: &[Vec<u8>]) -> Result<(), String> {
    let (page, text) = (page(), runner.req.text);
    let view = view(runner, path)?;
    let prime = view.base.keys.ok_or_else(|| format!("{} has no prime key", view.base.name))?;
    let base_path = runner.catalog_path(&view.base.name);
    let form = catalog::form(&Entry::Cluster(view.base.clone()), text);
    let mut records: BTreeMap<Vec<u8>, Vec<u8>> = catalog::read(&base_path, form, page)?.into_iter().filter_map(|r| Some((catalog::key_of(&r, prime)?.to_vec(), r))).collect();
    let kept: std::collections::BTreeSet<&[u8]> = after.iter().filter_map(|r| catalog::key_of(r, prime)).collect();
    for gone in before.iter().filter_map(|r| catalog::key_of(r, prime)).filter(|k| !kept.contains(k)) {
        records.remove(gone);
    }
    for r in after {
        let key = catalog::key_of(r, prime).ok_or_else(|| format!("{}: a record of {} bytes ends before the prime key", path.name, r.len()))?;
        records.insert(key.to_vec(), r.clone());
    }
    catalog::write(&base_path, form, &records.into_values().collect::<Vec<_>>(), page)?;
    for aix in catalog::alternate_indexes(&runner.datasets, &view.base.name) {
        if path.update && aix.upgrade || view.aix.as_ref().is_some_and(|a| a.name == aix.name) {
            build(runner, &view.base, &aix)?;
        }
    }
    Ok(())
}
