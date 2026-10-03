//! IDCAMS LISTCAT: the catalog's entries, by name or with their attributes, laid out as z/OS 3.1
//! DFSMS Access Method Services' samples show them (idai200, "Interpreting LISTCAT output
//! listings"). The catalog is the data set directory: the VSAM objects in its catalog entries,
//! generation data groups with their generations, and every other data set as non-VSAM.

use super::catalog::{self, Entry};
use super::{GDG_MAGIC, Runner};
use jcl::idcams::{Entries, EntryType, Listcat, Organization};
use std::collections::BTreeMap;
use std::fs;

/// The name the listing gives the catalog (C354).
const CATALOG: &str = "IRONWORK.CATALOG";

/// What a listed name is.
#[derive(Debug, Clone)]
enum Kind {
    Vsam(Entry),
    /// The data or index component of a cluster or an alternate index.
    Component { owner: Entry, data: bool },
    /// A generation data group's base: its limit, SCRATCH and EMPTY, and its generations.
    Gdg { limit: usize, scratch: bool, empty: bool, generations: Vec<String> },
    /// A non-VSAM data set, and the generation data group it is a generation of.
    Nonvsam(Option<String>),
}

impl Kind {
    fn entry_type(&self) -> EntryType {
        match self {
            Kind::Vsam(Entry::Cluster(_)) => EntryType::Cluster,
            Kind::Vsam(Entry::AlternateIndex(_)) => EntryType::AlternateIndex,
            Kind::Vsam(Entry::Path(_)) => EntryType::Path,
            Kind::Component { data: true, .. } => EntryType::Data,
            Kind::Component { data: false, .. } => EntryType::Index,
            Kind::Gdg { .. } => EntryType::GenerationDataGroup,
            Kind::Nonvsam(_) => EntryType::Nonvsam,
        }
    }

    fn label(&self) -> &'static str {
        match self.entry_type() {
            EntryType::Cluster => "CLUSTER",
            EntryType::AlternateIndex => "AIX",
            EntryType::Path => "PATH",
            EntryType::Data => "DATA",
            EntryType::Index => "INDEX",
            EntryType::GenerationDataGroup => "GDG BASE",
            EntryType::Nonvsam => "NONVSAM",
        }
    }
}

/// Every name in the catalog.
fn every_name(runner: &Runner<'_>) -> BTreeMap<String, Kind> {
    let dir = &runner.datasets;
    let mut out = BTreeMap::new();
    for entry in catalog::all(dir) {
        if !matches!(entry, Entry::Path(_)) {
            let indexed = !matches!(&entry, Entry::Cluster(c) if c.organization != Organization::Indexed);
            for data in [true, false].into_iter().filter(|&d| d || indexed) {
                if let Some(n) = catalog::component_name(&entry, data) {
                    out.insert(n, Kind::Component { owner: entry.clone(), data });
                }
            }
        }
        out.insert(entry.name().to_string(), Kind::Vsam(entry));
    }
    let mut names: Vec<String> = fs::read_dir(dir).into_iter().flatten().flatten().filter_map(|e| e.file_name().to_str().map(str::to_string)).filter(|n| jcl::is_dsn(n) && !catalog::is_entry_file(n)).collect();
    names.sort();
    for name in &names {
        if let Some((limit, scratch, empty)) = gdg_base(runner, name) {
            let generations = runner.generations(name).iter().map(|g| format!("{name}.G{g:04}V00")).collect::<Vec<_>>();
            for g in &generations {
                out.insert(g.clone(), Kind::Nonvsam(Some(name.clone())));
            }
            out.insert(name.clone(), Kind::Gdg { limit, scratch, empty, generations });
        }
    }
    for name in names {
        out.entry(name).or_insert(Kind::Nonvsam(None));
    }
    out
}

fn gdg_base(runner: &Runner<'_>, name: &str) -> Option<(usize, bool, bool)> {
    let text = fs::read_to_string(runner.datasets.join(name)).ok()?;
    let words: Vec<&str> = text.lines().next()?.split_whitespace().collect();
    if words.first() != Some(&GDG_MAGIC) {
        return None;
    }
    let limit = words.get(1)?.strip_prefix("LIMIT=")?.parse().ok()?;
    Some((limit, words.contains(&"SCRATCH"), words.contains(&"EMPTY")))
}

/// The entries listed beneath a name in a listing that groups them: a cluster's or an alternate
/// index's components and paths, and a generation data group's generations.
fn associated(all: &BTreeMap<String, Kind>, name: &str) -> Vec<String> {
    let Some(kind) = all.get(name) else { return Vec::new() };
    match kind {
        Kind::Vsam(Entry::Path(_)) | Kind::Component { .. } | Kind::Nonvsam(_) => Vec::new(),
        Kind::Gdg { generations, .. } => generations.clone(),
        Kind::Vsam(entry) => {
            let components = all.iter().filter(|(_, k)| matches!(k, Kind::Component { owner, .. } if owner.name() == entry.name())).map(|(n, k)| (matches!(k, Kind::Component { data: false, .. }), n.clone()));
            let mut components: Vec<(bool, String)> = components.collect();
            components.sort();
            let paths = all.iter().filter(|(_, k)| matches!(k, Kind::Vsam(Entry::Path(p)) if p.entry == entry.name())).map(|(n, _)| n.clone());
            components.into_iter().map(|(_, n)| n).chain(paths).collect()
        }
    }
}

/// Whether a name matches a generic one qualifier for qualifier: an asterisk is any one
/// qualifier and a percent sign any one character. A level matches the names it begins.
fn matches(name: &str, pattern: &str, level: bool) -> bool {
    let (names, wanted): (Vec<&str>, Vec<&str>) = (name.split('.').collect(), pattern.split('.').collect());
    let same_count = if level { names.len() >= wanted.len() } else { names.len() == wanted.len() };
    same_count && wanted.iter().zip(&names).all(|(w, n)| *w == "*" || (w.len() == n.len() && w.chars().zip(n.chars()).all(|(a, b)| a == '%' || a == b)))
}

/// One field of an ALL listing: a name, dashes and its value, 24 columns wide.
fn field(name: &str, value: &str) -> String {
    let dashes = 24usize.saturating_sub(name.len() + value.len()).max(1);
    format!("{name}{}{value}", "-".repeat(dashes))
}

fn fields(pairs: &[(&str, String)]) -> String {
    let cells: Vec<String> = pairs.iter().map(|(n, v)| field(n, v)).collect();
    format!("       {}", cells.join("     "))
}

fn association(kind: &str, name: &str) -> String {
    format!("       {kind:-<9}{name}")
}

/// The attributes an ALL listing shows for one entry, of those ironwork's catalog keeps.
fn detail(runner: &Runner<'_>, all: &BTreeMap<String, Kind>, name: &str, kind: &Kind) -> Vec<String> {
    let mut out = Vec::new();
    let records = |entry: &Entry| catalog::read(&runner.catalog_path(entry.name()), catalog::form(entry, runner.req.text), super::idcams::page()).map_or(0, |r| r.len());
    let under = |owner: &Entry| match owner {
        Entry::AlternateIndex(_) => "AIX",
        _ => "CLUSTER",
    };
    match kind {
        Kind::Vsam(entry) => {
            out.push("     ASSOCIATIONS".into());
            match entry {
                Entry::Path(p) => {
                    let through = all.get(&p.entry).map(Kind::label).unwrap_or("CLUSTER");
                    out.push(association(through, &p.entry));
                    let base = match all.get(&p.entry) {
                        Some(Kind::Vsam(Entry::AlternateIndex(a))) => {
                            out.extend(associated(all, &a.name).iter().filter(|n| matches!(all.get(*n), Some(Kind::Component { .. }))).map(|n| association(all[n].label(), n)));
                            a.relate.clone()
                        }
                        _ => p.entry.clone(),
                    };
                    out.extend(associated(all, &base).iter().filter(|n| matches!(all.get(*n), Some(Kind::Component { .. }))).map(|n| association(all[n].label(), n)));
                    out.push("     ATTRIBUTES".into());
                    out.push(format!("       {}", if p.update { "UPDATE" } else { "NOUPDATE" }));
                }
                Entry::AlternateIndex(a) => {
                    let (components, paths): (Vec<String>, Vec<String>) = associated(all, name).into_iter().partition(|n| matches!(all.get(n), Some(Kind::Component { .. })));
                    out.extend(components.iter().map(|n| association(all[n].label(), n)));
                    out.push(association("CLUSTER", &a.relate));
                    out.extend(paths.iter().map(|n| association("PATH", n)));
                    out.push("     ATTRIBUTES".into());
                    out.push(format!("       {}", if a.upgrade { "UPGRADE" } else { "NOUPGRADE" }));
                }
                Entry::Cluster(c) => {
                    let (components, paths): (Vec<String>, Vec<String>) = associated(all, name).into_iter().partition(|n| matches!(all.get(n), Some(Kind::Component { .. })));
                    out.extend(components.iter().map(|n| association(all[n].label(), n)));
                    out.extend(catalog::alternate_indexes(&runner.datasets, &c.name).iter().map(|a| association("AIX", &a.name)));
                    out.extend(paths.iter().map(|n| association("PATH", n)));
                }
            }
        }
        Kind::Component { owner, data } => {
            out.push("     ASSOCIATIONS".into());
            out.push(association(under(owner), owner.name()));
            out.push("     ATTRIBUTES".into());
            let (keylen, rkp) = match owner {
                Entry::Cluster(c) => c.keys.map_or((0, 0), |k| (k.length, k.offset)),
                Entry::AlternateIndex(a) => (a.keys.length, super::idcams::AIX_HEADER),
                Entry::Path(_) => (0, 0),
            };
            let size = match owner {
                Entry::Cluster(c) => Some(c.record_size),
                Entry::AlternateIndex(a) => Some(a.record_size),
                Entry::Path(_) => None,
            };
            match (data, size) {
                (true, Some(s)) => {
                    out.push(fields(&[("KEYLEN", keylen.to_string()), ("AVGLRECL", s.average.to_string())]));
                    out.push(fields(&[("RKP", rkp.to_string()), ("MAXLRECL", s.maximum.to_string())]));
                }
                _ => {
                    out.push(fields(&[("KEYLEN", keylen.to_string())]));
                    out.push(fields(&[("RKP", rkp.to_string())]));
                }
            }
            if let Entry::AlternateIndex(a) = owner {
                out.push(fields(&[("AXRKP", a.keys.offset.to_string())]));
            }
            if *data {
                let organization = match owner {
                    Entry::Cluster(c) => match c.organization {
                        Organization::Indexed => "INDEXED",
                        Organization::Nonindexed => "NONINDEXED",
                        Organization::Numbered => "NUMBERED",
                        Organization::Linear => "LINEAR",
                    },
                    _ => "INDEXED",
                };
                let unique = match owner {
                    Entry::AlternateIndex(a) if a.unique => "UNIQKEY",
                    _ => "NONUNIQKEY",
                };
                out.push(format!("       {organization:<14}{unique}"));
                out.push("     STATISTICS".into());
                out.push(fields(&[("REC-TOTAL", records(owner).to_string())]));
            }
        }
        Kind::Gdg { limit, scratch, empty, generations } => {
            out.push("     ATTRIBUTES".into());
            out.push(format!("{}     {:<11}{}", fields(&[("LIMIT", limit.to_string())]), if *scratch { "SCRATCH" } else { "NOSCRATCH" }, if *empty { "EMPTY" } else { "NOEMPTY" }));
            out.push("     ASSOCIATIONS".into());
            out.extend(generations.iter().map(|g| association("NONVSAM", g)));
        }
        Kind::Nonvsam(Some(base)) => {
            out.push("     ASSOCIATIONS".into());
            out.push(association("GDG", base));
        }
        Kind::Nonvsam(None) => out.push("     ASSOCIATIONS--------(NULL)".into()),
    }
    out
}

/// LISTCAT into `out`, with condition code 4 when a named entry is not in the catalog.
pub(super) fn listcat(runner: &Runner<'_>, command: &Listcat, out: &mut Vec<String>) -> u16 {
    let all = every_name(runner);
    let wanted = |kind: &Kind| command.types.is_empty() || command.types.contains(&kind.entry_type());
    let generic = matches!(&command.entries, Entries::Named(n) if n.iter().any(|n| n.contains('*') || n.contains('%')));
    let flat = generic || !command.types.is_empty() || matches!(command.entries, Entries::Level(_));
    let mut lines: Vec<(String, bool)> = Vec::new();
    let mut code = 0;
    let mut messages = Vec::new();
    let nested_names: Vec<&String> = if flat { Vec::new() } else { all.keys().filter(|n| all.keys().any(|top| associated(&all, top).contains(n))).collect() };
    let mut list = |name: &String| {
        if !flat {
            if !lines.iter().any(|(n, _)| n == name) {
                lines.push((name.clone(), false));
                lines.extend(associated(&all, name).into_iter().map(|n| (n, true)));
            }
        } else if wanted(&all[name]) {
            lines.push((name.clone(), false));
        }
    };
    match &command.entries {
        Entries::All => {
            for name in all.keys().filter(|n| flat || !nested_names.contains(n)) {
                list(name);
            }
        }
        Entries::Level(level) => {
            for name in all.keys().filter(|n| matches(n, level, true)) {
                list(name);
            }
        }
        Entries::Named(names) => {
            for pattern in names {
                let found: Vec<&String> = all.keys().filter(|n| matches(n, pattern, false)).collect();
                if found.is_empty() {
                    messages.push(format!("IDC3012I ENTRY {pattern} NOT FOUND"));
                    code = 4;
                }
                for name in found {
                    list(name);
                }
            }
        }
    }
    out.push(format!("{:29}LISTING FROM CATALOG -- {CATALOG}", ""));
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for (name, nested) in &lines {
        let kind = &all[name];
        let label = format!("{}{} ", if *nested { "   " } else { "" }, kind.label());
        out.push(format!("{label:-<15} {name}"));
        if command.all {
            out.extend(detail(runner, &all, name, kind));
        }
        let counted = match kind.entry_type() {
            EntryType::AlternateIndex => "AIX",
            EntryType::Cluster => "CLUSTER",
            EntryType::Data => "DATA",
            EntryType::GenerationDataGroup => "GDG",
            EntryType::Index => "INDEX",
            EntryType::Nonvsam => "NONVSAM",
            EntryType::Path => "PATH",
        };
        *counts.entry(counted).or_default() += 1;
    }
    out.extend(messages);
    out.push(format!("{:9}THE NUMBER OF ENTRIES PROCESSED WAS:", ""));
    for label in ["AIX", "ALIAS", "CLUSTER", "DATA", "GDG", "INDEX", "NONVSAM", "PAGESPACE", "PATH", "SPACE", "USERCATALOG"] {
        out.push(count(label, counts.get(label).copied().unwrap_or(0)));
    }
    out.push(count("TOTAL", lines.len()));
    out.push(format!("{:9}THE NUMBER OF PROTECTED ENTRIES SUPPRESSED WAS 0", ""));
    code
}

/// A line of the count block: the label, dashes and the count, 24 columns wide.
fn count(label: &str, n: usize) -> String {
    let n = n.to_string();
    format!("{:19}{label} {}{n}", "", "-".repeat(23usize.saturating_sub(label.len() + n.len())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_names_match_a_qualifier_each_and_levels_match_what_they_begin() {
        let names = ["A.A.B", "A.B.B", "A.B.B.C", "A.B.B.C.C", "A.C.C", "A.D", "A.E", "A"];
        let hits = |p: &str, level: bool| names.iter().filter(|n| matches(n, p, level)).copied().collect::<Vec<_>>();
        assert_eq!(hits("A.*", false), ["A.D", "A.E"]);
        assert_eq!(hits("A.*.B", false), ["A.A.B", "A.B.B"]);
        assert_eq!(hits("A.*.B", true), ["A.A.B", "A.B.B", "A.B.B.C", "A.B.B.C.C"]);
        assert_eq!(hits("A", true).len(), 8);
    }

    #[test]
    fn counts_and_fields_keep_their_width() {
        assert_eq!(count("AIX", 1), "                   AIX -------------------1");
        assert_eq!(count("TOTAL", 27), "                   TOTAL ----------------27");
        assert_eq!(count("TOTAL", 6), "                   TOTAL -----------------6");
        assert_eq!(count("USERCATALOG", 0), "                   USERCATALOG -----------0");
        assert_eq!(field("KEYLEN", "4"), "KEYLEN-----------------4");
        assert_eq!(field("MAXLRECL", "32600"), "MAXLRECL-----------32600");
        assert_eq!(association("DATA", "USER.DUMMY.CLDATA"), "       DATA-----USER.DUMMY.CLDATA");
    }
}
