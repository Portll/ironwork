//! WORKING-STORAGE as IBM lays it out: sizes by USAGE and PICTURE, REDEFINES sharing storage,
//! OCCURS repeating it, SYNCHRONIZED slack bytes before an item and after each occurrence, and
//! level-66 RENAMES over what is laid out. See [`numeric::assumptions::WORKING_STORAGE_LAYOUT`]
//! for where each 01 level starts.

use crate::picture::{self, Category, Sym};
use numeric::{Native, Qualify};
use syntax::ast::{DataEntry, Environment, FileDecl, Literal, Organization, Ref, SignClause, Usage};
use syntax::messages::{IWC0001, IWC0002, Message};
use syntax::{Error, Pos};
use std::collections::HashMap;
use zarch::hfp::Precision;

pub use rt::storage::Kind;

#[derive(Clone, Debug)]
pub struct Item {
    pub name: Option<String>,
    pub level: u8,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub offset: u32,
    /// One occurrence.
    pub size: u32,
    pub occurs: u32,
    /// The fewest occurrences: OCCURS DEPENDING ON's minimum, else `occurs`.
    pub occurs_min: u32,
    /// Declared with OCCURS, so references to it take a subscript.
    pub table: bool,
    /// OCCURS ... DEPENDING ON: the item holding the current number of occurrences.
    pub depending_on: Option<syntax::ast::Ref>,
    /// The OCCURS DEPENDING ON tables within this group, other than one within another of them: the
    /// occurrences past each one's current count leave its length out.
    pub odo: Vec<usize>,
    /// The OCCURS DEPENDING ON tables ahead of this item in its record: the occurrences past each
    /// one's current count move it back (a variably located item, Language Reference SC27-8713-03,
    /// p. 206).
    pub moved_by: Vec<usize>,
    /// Whether an item after this one in its record moves with an OCCURS DEPENDING ON table in it.
    pub followed: bool,
    /// A table's INDEXED BY names, and its ASCENDING/DESCENDING keys.
    pub index_names: Vec<String>,
    pub keys: Vec<(bool, syntax::ast::Ref)>,
    /// A LOCAL-STORAGE item: its offset is from the activation's local storage.
    pub local: bool,
    pub kind: Kind,
    pub value: Option<Literal>,
    /// Stride and count of each OCCURS on the item and its ancestors, outermost first.
    pub dims: Vec<(u32, u32)>,
    pub redefines: Option<String>,
    /// The file whose record area holds the item, for a FILE SECTION record.
    pub file: Option<u16>,
    /// The LINKAGE record the item belongs to, by its position among LINKAGE 01 and 77 items; its
    /// offset is from that record's start, wherever the caller's argument puts it.
    pub linkage: Option<u16>,
    /// The class-name of a typed object reference.
    pub object_class: Option<String>,
    /// PICTURE scaling positions P right of the digits: the item's value is its digits times ten
    /// to this power.
    pub scaling: u32,
    /// Of category alphabetic: a PICTURE of the symbol A alone.
    pub alphabetic: bool,
    pub pos: Pos,
}

#[derive(Clone, Debug)]
pub struct Condition {
    pub name: String,
    pub item: usize,
    pub values: Vec<(Literal, Option<Literal>)>,
    /// The WHEN SET TO FALSE value.
    pub false_value: Option<Literal>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolved {
    Item(usize),
    Condition(usize),
}

/// Where a record addressed as a LINKAGE record is. Every other record is in the program's own
/// storage (Language Reference SC27-8713-03, pp. 63-66).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Binding {
    /// A LINKAGE record: an argument, SET ADDRESS OF or the runtime gives it its address.
    Argument,
    /// An EXTERNAL data record, or a record redefining one: the run unit's record of this name
    /// and size.
    External { name: String, size: u32 },
    /// A record of file k, an EXTERNAL file: the run unit's record area of that file-name.
    ExternalFile(u16),
    /// A GLOBAL record of a program containing this one, by its PROGRAM-ID and the record's name.
    Global { program: String, record: String, section: Section },
}

/// Where a GLOBAL record is in the program that declares it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Section {
    WorkingStorage,
    LocalStorage,
    Linkage,
    /// The record area of the file of this name.
    File(String),
}

pub struct Layout {
    pub items: Vec<Item>,
    pub conditions: Vec<Condition>,
    /// The items and the condition-names of each name, in declaration order: the candidates
    /// [`Layout::resolve`] qualifies.
    named_items: HashMap<String, Vec<usize>>,
    named_conditions: HashMap<String, Vec<usize>>,
    /// The positions of each edited PICTURE.
    pub edits: Vec<Vec<Sym>>,
    /// The currency sign value each edited PICTURE's currency symbol stands for, empty without one.
    pub currencies: Vec<String>,
    /// Offset and size of each file's record area, in declaration order.
    pub file_areas: Vec<(u32, u32)>,
    /// The least and greatest length of each file's level-01 records, an OCCURS DEPENDING ON table
    /// counted at its fewest and at its most occurrences (Language Reference SC27-8713-03, p. 188);
    /// None for a file with none.
    pub record_lengths: Vec<Option<(u32, u32)>>,
    /// The item of each LINKAGE record, in order: the LINKAGE SECTION's, then the records whose
    /// storage is elsewhere, as `bindings` says.
    pub linkage_roots: Vec<usize>,
    pub bindings: Vec<Binding>,
    /// How many programs out the program declaring each LINKAGE record is: 0 for its own.
    pub depths: Vec<u8>,
    /// For each file whose record area is not in the program's storage, the LINKAGE record
    /// bound to that area.
    pub bound_areas: Vec<Option<u16>>,
    /// Bytes of LOCAL-STORAGE each activation gets.
    pub local_size: u32,
    pub size: u32,
    /// Each FD and SD name, in declaration order: the highest qualifier of its records.
    pub file_names: Vec<String>,
    /// The LINAGE-COUNTER item of each file whose FD has LINAGE, which its file-name qualifies.
    pub linage_counters: Vec<Option<usize>>,
    /// The QUALIFY option [`Layout::resolve`] follows.
    pub qualify: Qualify,
    /// PARMCHECK's buffer: its offset, at the end of the WORKING-STORAGE the program declares, and
    /// its length (assumption [`numeric::assumptions::PARMCHECK_BUFFER`]).
    pub parmcheck: Option<(u32, u32)>,
    pub numcheck: crate::numcheck::NumcheckFacts,
    /// Each SPECIAL-NAMES class-name with its characters, one bit per byte value.
    pub classes: Vec<(String, [u8; 32])>,
}

const LEVEL_ALIGNMENT: u32 = 8;
/// The interpreter's ceiling on WORKING-STORAGE, well under Enterprise COBOL's own.
pub const MAX_STORAGE: u32 = 128 << 20;

/// Lays out WORKING-STORAGE, then each file's record area, which all its 01 records share and
/// which is at least `record_max` bytes. Files whose `shared` entry names the same file share one
/// area, as large as the largest of them (see [`record_area_owners`]). `notation` is what
/// SPECIAL-NAMES changes in the PICTUREs; `qualify` how RENAMES and later references resolve.
/// Under PARMCHECK, `parmcheck` is how many of `entries` the program declares itself, the special
/// registers the compiler adds following them, and the bytes of the buffer that goes between
/// (Programming Guide SC27-8714-03, p. 397).
#[allow(clippy::too_many_arguments)]
pub fn build(
    entries: &[DataEntry],
    files: &[(&[DataEntry], Option<u32>)],
    shared: &[usize],
    linkage: &[DataEntry],
    own_linkage: usize,
    local: &[DataEntry],
    notation: picture::Notation,
    qualify: Qualify,
    parmcheck: Option<(usize, u32)>,
) -> Result<Layout, Error> {
    let mut items: Vec<Item> = Vec::new();
    let mut usages: Vec<Option<Usage>> = Vec::new();
    let mut signs: Vec<Option<SignClause>> = Vec::new();
    let mut synchronized: Vec<bool> = Vec::new();
    let mut conditions = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    let mut renames: Vec<(usize, &DataEntry)> = Vec::new();
    const LINKAGE: u16 = u16::MAX;
    const LOCAL: u16 = u16::MAX - 1;
    let tagged: Vec<(Option<u16>, &DataEntry)> = entries
        .iter()
        .map(|e| (None, e))
        .chain(files.iter().enumerate().flat_map(|(k, (records, _))| records.iter().map(move |e| (Some(k as u16), e))))
        .chain(linkage.iter().map(|e| (Some(LINKAGE), e)))
        .chain(local.iter().map(|e| (Some(LOCAL), e)))
        .collect();
    let mut group = None;
    let mut linkage_roots = Vec::new();
    let mut after_renames = false;
    let bound = bound_records(&tagged);
    // The LINKAGE entries after the program's own are the GLOBAL records of the programs containing
    // it; an index of one of their tables is the declaring program's item, bound as the record is.
    let inherited: Vec<bool> = std::iter::repeat_n(false, tagged.len() - linkage.len() - local.len()).chain((0..linkage.len()).map(|i| i >= own_linkage)).chain(std::iter::repeat_n(false, local.len())).collect();
    let mut bound_roots = Vec::new();
    for (&(region, e), &bound) in tagged.iter().zip(&bound) {
        if region != group {
            open.clear();
            group = region;
            after_renames = false;
        }
        let file = region.filter(|&r| r != LINKAGE && r != LOCAL);
        let in_linkage = region == Some(LINKAGE);
        if e.occurs.is_some() && matches!(e.level, 1 | 66 | 77 | 88) {
            return Err(syntax::messages::IWC0027.at(e.pos, format!("OCCURS at level {:02}: Enterprise COBOL takes OCCURS only at levels 02 to 49", e.level)));
        }
        if e.level == 88 {
            if after_renames {
                return Err(syntax::messages::IWC0028.at(e.pos, "a level-88 entry after a level-66 entry: a RENAMES item cannot be a conditional variable"));
            }
            let item = *open.last().ok_or_else(|| syntax::messages::IWC0029.at(e.pos, "a level-88 entry with no item before it"))?;
            let name = e.name.clone().ok_or_else(|| syntax::messages::IWC0030.at(e.pos, "a level-88 entry needs a name"))?;
            conditions.push(Condition { name, item, values: e.condition_values.clone(), false_value: e.false_value.clone() });
            continue;
        }
        if e.renames.is_some() != (e.level == 66) {
            return Err(syntax::messages::IWC0031.at(e.pos, "RENAMES goes with level 66, and level 66 with RENAMES"));
        }
        if e.level == 66 {
            let record = open.first().copied().filter(|&r| items[r].level == 1).ok_or_else(|| syntax::messages::IWC0032.at(e.pos, "a level-66 entry must follow the entries of a level-01 record"))?;
            if e.name.is_none() || e.picture.is_some() || e.usage.is_some() || e.value.is_some() || e.redefines.is_some() || e.sync || e.sign.is_some() {
                return Err(syntax::messages::IWC0033.at(e.pos, "a level-66 entry has a name and a RENAMES clause, and nothing else"));
            }
            renames.push((items.len(), e));
            items.push(Item {
                name: e.name.clone(),
                level: 66,
                parent: Some(record),
                children: Vec::new(),
                offset: 0,
                size: 0,
                occurs: 1,
                occurs_min: 1,
                table: false,
                depending_on: None,
                odo: Vec::new(),
                moved_by: Vec::new(),
                followed: false,
                index_names: Vec::new(),
                keys: Vec::new(),
                local: items[record].local,
                kind: Kind::Group,
                value: None,
                dims: Vec::new(),
                redefines: None,
                file,
                linkage: items[record].linkage,
                object_class: None,
                scaling: 0,
                alphabetic: false,
                pos: e.pos,
            });
            usages.push(None);
            signs.push(None);
            synchronized.push(false);
            after_renames = true;
            continue;
        }
        if !(e.level == 1 || e.level == 77 || (2..=49).contains(&e.level)) {
            return Err(syntax::messages::IWC0034.at(e.pos, format!("level {} is not a data level", e.level)));
        }
        if after_renames && e.level != 1 && e.level != 77 {
            return Err(syntax::messages::IWC0035.at(e.pos, format!("level {:02} after a level-66 entry: a record's RENAMES entries follow its last entry", e.level)));
        }
        after_renames = false;
        while open.last().is_some_and(|&i| items[i].level >= e.level || items[i].level == 77) {
            open.pop();
        }
        let parent = if e.level == 1 || e.level == 77 { None } else { open.last().copied() };
        if e.level != 1 && e.level != 77 && parent.is_none() {
            return Err(syntax::messages::IWC0036.at(e.pos, format!("level {} with no group to belong to", e.level)));
        }
        let index = items.len();
        items.push(Item {
            name: e.name.clone(),
            level: e.level,
            parent,
            children: Vec::new(),
            offset: 0,
            size: 0,
            occurs: e.occurs.unwrap_or(1),
            occurs_min: e.occurs_min.or(e.occurs).unwrap_or(1),
            table: e.occurs.is_some(),
            depending_on: e.depending_on.clone(),
            odo: Vec::new(),
            moved_by: Vec::new(),
            followed: false,
            index_names: e.indexed_by.clone(),
            keys: e.keys.clone(),
            local: region == Some(LOCAL),
            kind: Kind::Group,
            value: e.value.clone().filter(|_| file.is_none() && !in_linkage),
            dims: Vec::new(),
            redefines: e.redefines.clone(),
            file,
            linkage: if in_linkage {
                Some(match (parent, &e.redefines) {
                    (None, Some(target)) => linkage_roots.iter().position(|&r: &usize| items[r].name.as_ref() == Some(target)).ok_or_else(|| {
                        syntax::messages::IWC0037.at(e.pos, format!("REDEFINES {target}: no earlier 01-level item of that name"))
                    })? as u16,
                    (None, None) => {
                        linkage_roots.push(index);
                        linkage_roots.len() as u16 - 1
                    }
                    (Some(p), _) => items[p].linkage.unwrap_or_default(),
                })
            } else if bound {
                if parent.is_none() {
                    bound_roots.push(index);
                }
                Some(0)
            } else {
                None
            },
            object_class: e.object_class.clone(),
            scaling: 0,
            alphabetic: false,
            pos: e.pos,
        });
        if e.occurs == Some(0) {
            return Err(syntax::messages::IWC0038.at(e.pos, "OCCURS 0 is not a table"));
        }
        let inherited = parent.and_then(|p| usages[p]);
        usages.push(e.usage.or(inherited));
        signs.push(e.sign.or(parent.and_then(|p| signs[p])));
        synchronized.push(e.sync || parent.is_some_and(|p| synchronized[p]));
        if let Some(p) = parent {
            items[p].children.push(index);
        }
        open.push(index);
    }
    let (mut edits, mut currencies) = (Vec::new(), Vec::new());
    let mut aligns = vec![1u32; items.len()];
    for (index, (_, e)) in tagged.iter().filter(|(_, e)| e.level != 88).enumerate() {
        if e.level == 66 {
            continue;
        }
        let pic = e.picture.as_deref().map(|p| picture::analyse_with(p, notation).map_err(|(message, m)| message.at(e.pos, m))).transpose()?;
        items[index].kind = kind(e, &items[index], usages[index], signs[index], pic.as_ref(), &mut edits)?;
        if currencies.len() < edits.len() {
            currencies.push(pic.as_ref().and_then(|p| p.currency.clone()).unwrap_or_default());
        }
        items[index].scaling = pic.as_ref().map_or(0, |p| p.scaling);
        items[index].alphabetic = matches!(items[index].kind, Kind::Alnum { .. }) && e.picture.as_deref().is_some_and(crate::corresponding::is_alphabetic);
        items[index].size = elementary_size(&items[index], pic.as_ref().map(|p| p.size));
        if synchronized[index] && items[index].kind != Kind::Group {
            aligns[index] = alignment(items[index].kind);
        }
    }
    for ((_, e), &inherited) in tagged.iter().zip(&inherited) {
        for name in &e.indexed_by {
            let index = items.len();
            items.push(Item {
                name: Some(name.clone()),
                level: 77,
                parent: None,
                children: Vec::new(),
                offset: 0,
                size: 4,
                occurs: 1,
                occurs_min: 1,
                table: false,
                depending_on: None,
                odo: Vec::new(),
                moved_by: Vec::new(),
                followed: false,
                index_names: Vec::new(),
                keys: Vec::new(),
                local: false,
                kind: Kind::Index,
                value: None,
                dims: Vec::new(),
                redefines: None,
                file: None,
                linkage: None,
                object_class: None,
                scaling: 0,
                alphabetic: false,
                pos: e.pos,
            });
            if inherited {
                items[index].linkage = Some(linkage_roots.len() as u16);
                linkage_roots.push(index);
            }
        }
    }
    aligns.resize(items.len(), 1);
    let arguments = linkage_roots.len();
    for i in 0..items.len() {
        let mut r = i;
        while let Some(p) = items[r].parent {
            r = p;
        }
        if let Some(b) = bound_roots.iter().position(|&x| x == r) {
            items[i].linkage = Some((arguments + b) as u16);
        }
    }
    linkage_roots.extend(&bound_roots);
    let roots: Vec<usize> = (0..items.len()).filter(|&i| items[i].parent.is_none()).collect();
    for &r in &roots {
        measure(&mut items, &aligns, r, 0)?;
    }
    let mut local_cursor = 0u32;
    let local_roots: Vec<usize> = roots.iter().copied().filter(|&r| items[r].local).collect();
    for r in local_roots {
        let start = local_cursor.div_ceil(LEVEL_ALIGNMENT) * LEVEL_ALIGNMENT;
        local_cursor = start + items[r].size;
        if local_cursor > MAX_STORAGE {
            return Err(syntax::messages::IWL0003.at(items[r].pos, format!("LOCAL-STORAGE exceeds the interpreter's {MAX_STORAGE} bytes")));
        }
        place(&mut items, r, start, Vec::new());
    }
    let owner = |k: usize| shared.get(k).copied().filter(|&g| g < files.len()).unwrap_or(k);
    let mut own: Vec<u32> = files.iter().map(|f| f.1.unwrap_or(0)).collect();
    for &r in &roots {
        if let Some(k) = items[r].file {
            own[k as usize] = own[k as usize].max(items[r].size);
        }
    }
    let mut area_size = vec![0u32; files.len()];
    for (k, &size) in own.iter().enumerate() {
        area_size[owner(k)] = area_size[owner(k)].max(size);
    }
    let mut cursor = 0u32;
    let mut root_offsets: Vec<(String, u32)> = Vec::new();
    let mut area_starts: Vec<Option<u32>> = vec![None; files.len()];
    let mut pending = parmcheck.map(|(declared, bytes)| (entries[..declared.min(entries.len())].iter().filter(|e| e.level != 88).count(), bytes));
    let mut buffer = None;
    for &r in &roots {
        if items[r].local {
            continue;
        }
        if items[r].linkage.is_some() {
            place(&mut items, r, 0, Vec::new());
            continue;
        }
        if let Some((own, bytes)) = pending
            && r >= own
        {
            buffer = Some((cursor, bytes));
            cursor += bytes;
            pending = None;
        }
        if let Some(k) = items[r].file {
            let g = owner(k as usize);
            let start = *area_starts[g].get_or_insert(cursor.div_ceil(LEVEL_ALIGNMENT) * LEVEL_ALIGNMENT);
            cursor = cursor.max(start + area_size[g]);
            if cursor > MAX_STORAGE {
                return Err(syntax::messages::IWL0004.at(items[r].pos, format!("storage exceeds the interpreter's {MAX_STORAGE} bytes")));
            }
            place(&mut items, r, start, Vec::new());
            continue;
        }
        let offset = match &items[r].redefines {
            Some(target) => root_offsets.iter().find(|(n, _)| n == target).map(|&(_, o)| o).ok_or_else(|| {
                syntax::messages::IWC0037.at(items[r].pos, format!("REDEFINES {target}: no earlier 01-level item of that name"))
            })?,
            None => cursor.div_ceil(LEVEL_ALIGNMENT) * LEVEL_ALIGNMENT,
        };
        cursor = cursor.max(offset + items[r].size);
        if cursor > MAX_STORAGE {
            return Err(syntax::messages::IWL0005.at(items[r].pos, format!("WORKING-STORAGE exceeds the interpreter's {MAX_STORAGE} bytes")));
        }
        if let Some(name) = &items[r].name {
            root_offsets.push((name.clone(), offset));
        }
        place(&mut items, r, offset, Vec::new());
    }
    if let Some((_, bytes)) = pending {
        buffer = Some((cursor, bytes));
        cursor += bytes;
    }
    let tables: Vec<usize> = (0..items.len()).filter(|&i| items[i].depending_on.is_some()).collect();
    for &t in &tables {
        let (mut child, mut at, mut nested) = (t, items[t].parent, false);
        let (mut chain, mut followers) = (vec![t], Vec::new());
        while let Some(a) = at {
            nested |= child != t && items[child].depending_on.is_some();
            if !nested {
                items[a].odo.push(t);
            }
            let end = items[child].offset + items[child].size * items[child].occurs;
            let after = items[a].children.iter().skip_while(|&&c| c != child).skip(1).copied().filter(|&c| items[c].offset >= end);
            let before = followers.len();
            followers.extend(after);
            if followers.len() > before {
                chain.iter().for_each(|&c| items[c].followed = true);
            }
            chain.push(a);
            (child, at) = (a, items[a].parent);
        }
        let holds_another = tables.iter().any(|&u| u != t && ancestors(&items, u).any(|p| p == t));
        if !followers.is_empty() && (items[t].dims.len() > 1 || holds_another) {
            return Err(syntax::messages::IWR0012.at(items[t].pos, "items after an OCCURS DEPENDING ON table in the same record are not supported yet"));
        }
        while let Some(f) = followers.pop() {
            items[f].moved_by.push(t);
            followers.extend(items[f].children.iter().copied());
        }
    }
    for (index, e) in renames {
        rename(&mut items, index, e, qualify)?;
    }
    let mut bound_areas = vec![None; files.len()];
    let mut bindings = vec![Binding::Argument; arguments];
    for &r in &bound_roots {
        let binding = match items[r].file {
            Some(k) => {
                bound_areas[k as usize].get_or_insert(items[r].linkage.unwrap_or_default());
                Binding::ExternalFile(k)
            }
            None => {
                let name = items[r].redefines.clone().or_else(|| items[r].name.clone()).unwrap_or_default();
                let size = bound_roots.iter().find(|&&t| items[t].name.as_deref() == Some(name.as_str())).map_or(items[r].size, |&t| items[t].size);
                Binding::External { name, size }
            }
        };
        bindings.push(binding);
    }
    let mut areas = Vec::new();
    for (k, &size) in own.iter().enumerate() {
        if bound_areas[k].is_some() {
            areas.push((0, area_size[owner(k)]));
            continue;
        }
        let g = owner(k);
        let start = match area_starts[g] {
            Some(start) => start,
            None => {
                let start = cursor.div_ceil(LEVEL_ALIGNMENT) * LEVEL_ALIGNMENT;
                area_starts[g] = Some(start);
                cursor = cursor.max(start + area_size[g]);
                start
            }
        };
        areas.push((start, size));
    }
    let mut record_lengths: Vec<Option<(u32, u32)>> = vec![None; files.len()];
    for &r in &roots {
        let Some(k) = items[r].file else { continue };
        let fewer: u32 = items[r]
            .odo
            .iter()
            .map(|&t| {
                let t = &items[t];
                let outer: u32 = t.dims[..t.dims.len().saturating_sub(1)].iter().map(|&(_, n)| n).product();
                t.occurs.saturating_sub(t.occurs_min) * t.size * outer
            })
            .sum();
        let (least, most) = (items[r].size.saturating_sub(fewer), items[r].size);
        let lengths = &mut record_lengths[k as usize];
        *lengths = Some(lengths.map_or((least, most), |(l, m)| (l.min(least), m.max(most))));
    }
    Ok(Layout {
        named_items: by_name(items.iter().map(|i| i.name.as_deref())),
        named_conditions: by_name(conditions.iter().map(|c| Some(c.name.as_str()))),
        items,
        conditions,
        edits,
        currencies,
        file_areas: areas,
        record_lengths,
        linkage_roots,
        depths: vec![0; bindings.len()],
        bindings,
        bound_areas,
        local_size: local_cursor,
        size: cursor,
        file_names: Vec::new(),
        linage_counters: Vec::new(),
        qualify,
        parmcheck: buffer,
        numcheck: Default::default(),
        classes: Vec::new(),
    })
}


/// The index of each entry under its name, ascending; an unnamed entry is under none.
fn by_name<'a>(names: impl Iterator<Item = Option<&'a str>>) -> HashMap<String, Vec<usize>> {
    let mut index: HashMap<String, Vec<usize>> = HashMap::new();
    for (k, name) in names.enumerate() {
        if let Some(name) = name {
            index.entry(name.to_owned()).or_default().push(k);
        }
    }
    index
}

/// The names of item `start` and each group above it, nearest first: the hierarchy of names that
/// qualifies an item `start` holds or a condition-name of `start`. FILLER and unnamed items give
/// none.
fn names_from(items: &[Item], start: Option<usize>) -> impl Iterator<Item = &str> {
    std::iter::successors(start, |&p| items[p].parent).filter_map(|p| items[p].name.as_deref())
}

/// Which entries belong to records whose storage the run unit holds: an EXTERNAL record of
/// WORKING-STORAGE or of an EXTERNAL file, or a WORKING-STORAGE record redefining an EXTERNAL one
/// (Language Reference SC27-8713-03, p. 197).
fn bound_records(tagged: &[(Option<u16>, &DataEntry)]) -> Vec<bool> {
    let mut externals: Vec<&str> = Vec::new();
    let mut current = false;
    let mut out = Vec::with_capacity(tagged.len());
    for &(region, e) in tagged {
        if matches!(e.level, 1 | 77) {
            current = match region {
                None => e.external || e.redefines.as_deref().is_some_and(|t| externals.contains(&t)),
                Some(r) => r < u16::MAX - 1 && e.external,
            };
            if current && region.is_none() && e.external {
                externals.extend(e.name.as_deref());
            }
        }
        out.push(current);
    }
    out
}

/// Gives level-66 entry `index` the storage and attributes of what it renames (Language Reference
/// SC27-8713-03, pp. 228-229): one item as that item is, or from the start of the first item
/// through the end of the last as an alphanumeric group.
fn rename(items: &mut [Item], index: usize, e: &DataEntry, qualify: Qualify) -> Result<(), Error> {
    let Some((first, last)) = &e.renames else { return Ok(()) };
    let record = items[index].parent.unwrap_or(index);
    let find = |r: &Ref| -> Result<usize, Error> {
        let err = |m: String| Err(syntax::messages::IWC0039.at(r.pos, format!("RENAMES {}: {m}", r.name)));
        if !r.subscripts.is_empty() || r.refmod.is_some() {
            return err("a renamed item is named without subscripts or reference modification".into());
        }
        let in_record = |mut at: usize| {
            let mut wanted = r.qualifiers.iter().peekable();
            while let Some(p) = items[at].parent {
                if wanted.peek().is_some_and(|q| items[p].name.as_deref() == Some(q.as_str())) {
                    wanted.next();
                }
                at = p;
            }
            at == record && wanted.next().is_none()
        };
        let mut found: Vec<usize> = (0..items.len()).filter(|&i| i != record && items[i].level != 66 && items[i].name.as_deref() == Some(r.name.as_str()) && in_record(i)).collect();
        if found.len() > 1 && qualify == Qualify::Extend {
            let complete: Vec<usize> = found.iter().copied().filter(|&i| names_from(items, items[i].parent).eq(r.qualifiers.iter().map(String::as_str))).collect();
            if complete.len() == 1 {
                found = complete;
            }
        }
        let &[t] = found.as_slice() else {
            let record_name = items[record].name.clone().unwrap_or_default();
            return err(if found.is_empty() { format!("no item of that name below {record_name}, other than a level-66 entry") } else { "ambiguous; qualify it with OF or IN".into() });
        };
        if !items[t].dims.is_empty() {
            return err("a renamed item must not have OCCURS, nor belong to a group that has it".into());
        }
        Ok(t)
    };
    if first.name == items[record].name.clone().unwrap_or_default() && first.qualifiers.is_empty() {
        return Err(syntax::messages::IWC0040.at(first.pos, format!("RENAMES {}: a level-66 entry cannot rename a level-01 record", first.name)));
    }
    let a = find(first)?;
    let Some(last) = last else {
        let (offset, size, kind, scaling, odo, moved_by, followed) =
            (items[a].offset, items[a].size, items[a].kind, items[a].scaling, items[a].odo.clone(), items[a].moved_by.clone(), items[a].followed);
        let it = &mut items[index];
        (it.offset, it.size, it.kind, it.scaling, it.odo, it.moved_by, it.followed) = (offset, size, kind, scaling, odo, moved_by, followed);
        return Ok(());
    };
    let b = find(last)?;
    let end = |i: usize| items[i].offset + items[i].size;
    let mut up = items[b].parent;
    while let Some(p) = up {
        if p == a {
            return Err(syntax::messages::IWC0041.at(last.pos, format!("RENAMES {} THRU {}: the last item cannot be within the first", first.name, last.name)));
        }
        up = items[p].parent;
    }
    if a == b || items[b].offset < items[a].offset || end(b) < end(a) {
        return Err(syntax::messages::IWC0042.at(last.pos, format!("RENAMES {} THRU {}: the last item must start and end no earlier than the first", first.name, last.name)));
    }
    let root = |mut i: usize| {
        while let Some(p) = items[i].parent {
            i = p;
        }
        i
    };
    if let Some(t) = (0..items.len()).find(|&i| items[i].depending_on.is_some() && root(i) == record && items[i].offset >= items[a].offset && items[i].offset < end(b)) {
        return Err(syntax::messages::IWC0043.at(items[t].pos, format!("RENAMES {} THRU {}: no OCCURS DEPENDING ON between them", first.name, last.name)));
    }
    let (offset, size, moved_by) = (items[a].offset, end(b) - items[a].offset, items[a].moved_by.clone());
    let it = &mut items[index];
    (it.offset, it.size, it.kind, it.moved_by) = (offset, size, Kind::Group, moved_by);
    Ok(())
}

/// The groups item `i` belongs to, nearest first.
fn ancestors(items: &[Item], i: usize) -> impl Iterator<Item = usize> + '_ {
    std::iter::successors(items[i].parent, |&p| items[p].parent)
}

/// The boundary a SYNCHRONIZED item of this kind is aligned on (Language Reference SC27-8713-03,
/// pp. 232-233); 1 for the kinds SYNCHRONIZED leaves where they are.
fn alignment(kind: Kind) -> u32 {
    match kind {
        Kind::Binary { native: Native::BinaryChar, .. } => 1,
        Kind::Binary { digits, signed, native: native @ (Native::CompX | Native::Comp5Bytes), .. } => match (numeric::binary::Binary { digits: digits as u8, signed, native }).bytes() {
            bytes @ (2 | 4) => bytes as u32,
            8 => 4,
            _ => 1,
        },
        Kind::Binary { digits: 0..=4, .. } => 2,
        Kind::Binary { .. } | Kind::Float(Precision::Short) | Kind::Pointer | Kind::Index | Kind::ObjectReference | Kind::ProgramPointer => 4,
        Kind::Float(_) => 8,
        _ => 1,
    }
}

/// An elementary item's kind. `sign` is its own SIGN clause or the nearest group's above it, which
/// applies to a signed zoned item alone (Language Reference SC27-8713-03, p. 231).
fn kind(e: &DataEntry, item: &Item, usage: Option<Usage>, sign: Option<SignClause>, pic: Option<&picture::Picture>, edits: &mut Vec<Vec<Sym>>) -> Result<Kind, Error> {
    let err = |message: Message, m: String| message.at(e.pos, m);
    let usage = usage.unwrap_or_default();
    let handle = matches!(usage, Usage::ObjectReference | Usage::ProgramPointer);
    let elementary = e.picture.is_some() || (handle || matches!(usage, Usage::Float1 | Usage::Float2 | Usage::Pointer | Usage::Index | Usage::BinaryChar { .. })) && item.children.is_empty();
    if !elementary {
        return if item.children.is_empty() { Err(err(syntax::messages::IWC0235, "an elementary item needs a PICTURE".into())) } else { Ok(Kind::Group) };
    }
    if !item.children.is_empty() {
        return Err(err(syntax::messages::IWC0236, "a group item cannot have a PICTURE".into()));
    }
    if handle {
        if e.picture.is_some() || e.value.as_ref().is_some_and(|v| *v != Literal::Figurative(syntax::ast::Figurative::Null)) {
            return Err(err(syntax::messages::IWC0237, "an object reference, function-pointer or procedure-pointer takes no PICTURE and only VALUE NULL".into()));
        }
        return Ok(if usage == Usage::ObjectReference { Kind::ObjectReference } else { Kind::ProgramPointer });
    }
    if let Usage::Pointer | Usage::Index = usage {
        if e.picture.is_some() {
            return Err(err(syntax::messages::IWC0238, "POINTER and INDEX items take no PICTURE".into()));
        }
        return Ok(if usage == Usage::Pointer { Kind::Pointer } else { Kind::Index });
    }
    if let Usage::Float1 | Usage::Float2 = usage {
        if e.picture.is_some() {
            return Err(err(syntax::messages::IWC0239, "COMP-1 and COMP-2 items take no PICTURE".into()));
        }
        return Ok(Kind::Float(if usage == Usage::Float1 { Precision::Short } else { Precision::Long }));
    }
    if let Usage::BinaryChar { signed } = usage {
        if e.picture.is_some() {
            return Err(err(syntax::messages::IWC0294, "BINARY-CHAR takes no PICTURE".into()));
        }
        return Ok(Kind::Binary { digits: 3, scale: 0, signed, native: Native::BinaryChar });
    }
    let Some(pic) = pic else { return Err(err(syntax::messages::IWC0235, "an elementary item needs a PICTURE".into())) };
    let blank_numeric;
    let pic = match pic.category {
        Category::Numeric if e.blank_when_zero && usage == Usage::Display => {
            blank_numeric = picture::blank_when_zero(pic).map_err(|(message, m)| message.at(e.pos, m))?;
            &blank_numeric
        }
        Category::NumericEdited => pic,
        Category::Numeric if e.blank_when_zero && usage == Usage::National => return Err(err(syntax::messages::IWR0054, "BLANK WHEN ZERO on a USAGE NATIONAL item is not supported yet".into())),
        _ if e.blank_when_zero => return Err(err(syntax::messages::IWC0240, "BLANK WHEN ZERO needs a numeric or numeric-edited item of USAGE DISPLAY or NATIONAL".into())),
        _ => pic,
    };
    let k = match (pic.category, usage) {
        (Category::NumericEdited, Usage::Display) => {
            edits.push(pic.edit.clone().unwrap_or_default());
            Kind::NumericEdited { edit: edits.len() as u32 - 1, digits: pic.digits, scale: pic.scale, blank_when_zero: e.blank_when_zero }
        }
        (Category::AlphanumericEdited, Usage::Display) => {
            edits.push(pic.edit.clone().unwrap_or_default());
            Kind::AlnumEdited { edit: edits.len() as u32 - 1 }
        }
        (Category::Numeric, Usage::Display) => Kind::Zoned { digits: pic.digits, scale: pic.scale, signed: pic.signed, sign: sign.filter(|_| pic.signed) },
        (Category::Numeric, Usage::Packed) => Kind::Packed { digits: pic.digits, scale: pic.scale, signed: pic.signed },
        (Category::Numeric, Usage::Binary | Usage::NativeBinary | Usage::CompX) if pic.digits <= 18 => {
            let native = match usage {
                Usage::NativeBinary => Native::Comp5,
                Usage::CompX => Native::CompX,
                _ => Native::No,
            };
            Kind::Binary { digits: pic.digits, scale: pic.scale, signed: pic.signed, native }
        }
        (Category::Numeric, Usage::Binary | Usage::NativeBinary | Usage::CompX) => return Err(err(syntax::messages::IWC0241, "a binary item holds at most 18 digits".into())),
        (Category::Alphanumeric, Usage::CompX | Usage::NativeBinary) if pic.size <= 8 => {
            let native = if usage == Usage::CompX { Native::CompX } else { Native::Comp5Bytes };
            Kind::Binary { digits: u32::from(numeric::binary::alphanumeric_digits(pic.size)), scale: 0, signed: false, native }
        }
        (Category::Alphanumeric, Usage::CompX | Usage::NativeBinary) => {
            let usage = if usage == Usage::CompX { "COMP-X" } else { "COMP-5" };
            return Err(err(syntax::messages::IWC0303, format!("PIC X({}) {usage}: {} bytes of binary, and ironwork's binary items hold at most eight", pic.size, pic.size)));
        }
        (Category::Alphanumeric, Usage::Display) => Kind::Alnum { justified: e.justified },
        (Category::Dbcs | Category::National, Usage::Dbcs) => {
            if pic.edit.is_some() && e.justified {
                return Err(err(syntax::messages::IWC0242, "JUSTIFIED cannot be given for a DBCS item whose PICTURE has B".into()));
            }
            let edit = pic.edit.clone().map(|syms| {
                edits.push(syms);
                edits.len() as u32 - 1
            });
            Kind::Dbcs { justified: e.justified, edit }
        }
        (Category::Dbcs, _) => return Err(err(syntax::messages::IWC0243, "a PICTURE with G needs USAGE DISPLAY-1 (Language Reference SC27-8713-03, p. 214)".into())),
        (Category::National, Usage::Display | Usage::National) if pic.edit.is_some() => return Err(err(syntax::messages::IWR0055, "a national-edited PICTURE is not supported yet".into())),
        (Category::National, Usage::Display | Usage::National) => Kind::National,
        (category, usage) => return Err(err(syntax::messages::IWR0056, format!("a {category:?} PICTURE with USAGE {usage:?} is not supported yet"))),
    };
    if e.sign.is_some() && matches!(k, Kind::Zoned { signed: false, .. }) {
        return Err(err(syntax::messages::IWC0244, "a SIGN clause needs an S in the PICTURE".into()));
    }
    Ok(k)
}

fn elementary_size(item: &Item, e_size: Option<u32>) -> u32 {
    match item.kind {
        Kind::Group => 0,
        Kind::Alnum { .. } | Kind::NumericEdited { .. } | Kind::AlnumEdited { .. } => e_size.unwrap_or(0),
        Kind::National | Kind::Dbcs { .. } => 2 * e_size.unwrap_or(0),
        Kind::Zoned { digits, sign, .. } => digits + sign.is_some_and(|s| s.separate) as u32,
        Kind::Packed { digits, .. } => digits / 2 + 1,
        Kind::Binary { digits, signed, native, .. } => numeric::binary::Binary { digits: digits as u8, signed, native }.bytes() as u32,
        Kind::Float(p) => p.bytes() as u32,
        Kind::Pointer | Kind::Index | Kind::ObjectReference | Kind::ProgramPointer => 4,
    }
}

/// Sizes the item and its descendants; children's offsets are relative to their parent here.
/// `base` is where the item starts in its record, the 01 level on a doubleword, which is what a
/// SYNCHRONIZED item's boundary is reckoned from (Language Reference SC27-8713-03, pp. 233-235).
fn measure(items: &mut [Item], aligns: &[u32], index: usize, base: u32) -> Result<(), Error> {
    if items[index].kind != Kind::Group {
        return Ok(());
    }
    let children = items[index].children.clone();
    let (mut cursor, mut extent) = (0u32, 0u32);
    let mut placed: Vec<(Option<String>, u32)> = Vec::new();
    let mut previous: Option<usize> = None;
    for c in children {
        let redefined = match items[c].redefines.clone() {
            Some(target) => Some(placed.iter().rev().find(|(n, _)| n.as_deref() == Some(target.as_str())).map(|&(_, o)| o).ok_or_else(|| {
                syntax::messages::IWC0044.at(items[c].pos, format!("REDEFINES {target}: no earlier item of that name at this level"))
            })?),
            None => None,
        };
        let start = redefined.unwrap_or(cursor);
        let m = first_alignment(items, aligns, c);
        let slack = (m - (base + start) % m) % m;
        if slack > 0 && redefined.is_some() {
            return Err(syntax::messages::IWC0045.at(items[c].pos, format!("a SYNCHRONIZED item at the start of a REDEFINES would need {slack} slack bytes: the redefined item must be on a {m}-byte boundary")));
        }
        if slack > 0 {
            give_slack(items, previous, cursor, slack);
        }
        let offset = start + slack;
        measure(items, aligns, c, base + offset)?;
        if items[c].table {
            let m = widest_alignment(items, aligns, c);
            items[c].size = items[c].size.div_ceil(m) * m;
        }
        let too_large = || syntax::messages::IWL0006.at(items[c].pos, format!("an item larger than the interpreter's {MAX_STORAGE} bytes"));
        let span = items[c].size.checked_mul(items[c].occurs).filter(|&s| s <= MAX_STORAGE).ok_or_else(too_large)?;
        if redefined.is_none() {
            cursor = offset.checked_add(span).filter(|&s| s <= MAX_STORAGE).ok_or_else(too_large)?;
        }
        extent = extent.max(offset + span);
        items[c].offset = offset;
        placed.push((items[c].name.clone(), offset));
        previous = Some(c);
    }
    items[index].size = cursor.max(extent);
    Ok(())
}

/// The boundary the first elementary item within `index` is aligned on.
fn first_alignment(items: &[Item], aligns: &[u32], index: usize) -> u32 {
    match items[index].children.first() {
        Some(&c) if items[index].kind == Kind::Group => first_alignment(items, aligns, c),
        _ => aligns[index],
    }
}

/// The widest boundary of any elementary item within `index`, to which each occurrence of a table
/// is padded so that every occurrence aligns as the first does.
fn widest_alignment(items: &[Item], aligns: &[u32], index: usize) -> u32 {
    items[index].children.iter().map(|&c| widest_alignment(items, aligns, c)).fold(aligns[index], u32::max)
}

/// Slack bytes before a SYNCHRONIZED item belong at the level of the elementary item before it
/// (Language Reference SC27-8713-03, p. 234): so they lengthen the group sibling that ends where
/// they start, and that group's last group, down to the elementary item's own group. A table or
/// a redefinition keeps its length and leaves them to the group it is in.
fn give_slack(items: &mut [Item], previous: Option<usize>, cursor: u32, slack: u32) {
    let Some(mut g) = previous else { return };
    let mut end = cursor;
    while items[g].kind == Kind::Group && !items[g].table && items[g].redefines.is_none() && items[g].offset + items[g].size == end {
        end = items[g].size;
        items[g].size += slack;
        match items[g].children.last() {
            Some(&c) => g = c,
            None => return,
        }
    }
}

fn place(items: &mut [Item], index: usize, offset: u32, mut dims: Vec<(u32, u32)>) {
    items[index].offset = offset;
    if items[index].table {
        dims.push((items[index].size, items[index].occurs));
    }
    items[index].dims = dims.clone();
    for c in items[index].children.clone() {
        let relative = items[c].offset;
        place(items, c, offset + relative, dims.clone());
    }
}

impl Item {
    /// The index a SEARCH of this table steps: VARYING's index-name when it is one of the table's
    /// own, else the table's first index-name, else VARYING's item (Language Reference
    /// SC27-8713-03, p. 437).
    pub fn search_index(&self, varying: Option<&Ref>, pos: Pos) -> Option<Ref> {
        match (varying.filter(|v| self.index_names.contains(&v.name)), self.index_names.first()) {
            (Some(own), _) => Some(own.clone()),
            (None, Some(name)) => Some(Ref { name: name.clone(), qualifiers: Vec::new(), subscripts: Vec::new(), refmod: None, pos }),
            (None, None) => varying.cloned(),
        }
    }
}

impl Layout {
    /// Where item `index`, at its first occurrence, lies within its outermost table, and that
    /// table's bytes with every OCCURS at its maximum: the region SSRANGE checks a reference's
    /// address against (Programming Guide SC27-8714-03, p. 411). None for an item in no table.
    pub fn table_range(&self, index: usize) -> Option<(u32, u32)> {
        let &(stride, count) = self.items[index].dims.first()?;
        let mut table = index;
        while let Some(p) = self.items[table].parent.filter(|&p| !self.items[p].dims.is_empty()) {
            table = p;
        }
        Some((self.items[index].offset - self.items[table].offset, stride * count))
    }

    /// The characters of class-name `name`, one bit per byte value.
    pub fn class(&self, name: &str) -> Option<[u8; 32]> {
        self.classes.iter().find(|(n, _)| n == name).map(|&(_, bits)| bits)
    }

    /// An elementary item's category as INITIALIZE's phrases name it, a floating-point item's as
    /// NUMERIC (assumption [`numeric::assumptions::INITIALIZE_FLOAT_NUMERIC`]); None for a group
    /// and for a pointer, index or object reference.
    pub fn category(&self, i: usize) -> Option<syntax::ast::DataCategory> {
        use syntax::ast::DataCategory;
        let item = &self.items[i];
        Some(match item.kind {
            Kind::Alnum { .. } if item.alphabetic => DataCategory::Alphabetic,
            Kind::Alnum { .. } => DataCategory::Alphanumeric,
            Kind::AlnumEdited { .. } => DataCategory::AlphanumericEdited,
            Kind::National => DataCategory::National,
            Kind::Dbcs { .. } => DataCategory::Dbcs,
            Kind::NumericEdited { .. } => DataCategory::NumericEdited,
            Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. } | Kind::Float(_) => DataCategory::Numeric,
            Kind::Group | Kind::Pointer | Kind::Index | Kind::ObjectReference | Kind::ProgramPointer => return None,
        })
    }

    /// The category of the elementary item reference modification makes of `item` of `kind`, or of
    /// a special register of `kind` when there is no item: national when `kind` is, alphabetic for
    /// an alphabetic item, and alphanumeric otherwise (Language Reference SC27-8713-03, p. 76;
    /// assumption [`numeric::assumptions::INITIALIZE_REFERENCE_MODIFIED`]).
    pub fn refmod_category(&self, item: Option<usize>, kind: Kind) -> syntax::ast::DataCategory {
        use syntax::ast::DataCategory;
        match item.and_then(|i| self.category(i)) {
            Some(DataCategory::Alphabetic) => DataCategory::Alphabetic,
            _ if kind == Kind::National => DataCategory::National,
            _ => DataCategory::Alphanumeric,
        }
    }

    /// LENGTH OF a table element written without subscripts gives one occurrence's length (Language
    /// Reference SC27-8713-03, p. 23): the reference with each subscript 1.
    pub fn length_of_ref<'r>(&self, r: &'r Ref) -> std::borrow::Cow<'r, Ref> {
        use syntax::ast::{Expr, Operand};
        match self.resolve(&r.name, &r.qualifiers, r.pos) {
            Ok(Resolved::Item(i)) if r.subscripts.is_empty() && !self.items[i].dims.is_empty() => {
                let one = Expr::Operand(Operand::Literal(Literal::Number("1".into())));
                std::borrow::Cow::Owned(Ref { subscripts: vec![one; self.items[i].dims.len()], ..r.clone() })
            }
            _ => std::borrow::Cow::Borrowed(r),
        }
    }

    /// The elementary items INITIALIZE of item `i` may move to, each occurrence with its offset from
    /// `i`'s start (Language Reference SC27-8713-03, p. 352, rules 1a and 1b): none under a
    /// REDEFINES, no FILLER unless `filler`, and no index, object reference or program pointer.
    pub fn initialize_receivers(&self, i: usize, filler: bool) -> Vec<(usize, u32)> {
        let mut out = Vec::new();
        self.receivers_under(i, 0, filler, &mut out);
        out
    }

    fn receivers_under(&self, i: usize, offset: u32, filler: bool, out: &mut Vec<(usize, u32)>) {
        let item = &self.items[i];
        match item.kind {
            Kind::Index | Kind::ObjectReference | Kind::ProgramPointer => {}
            Kind::Group => {
                for &c in &item.children {
                    let child = &self.items[c];
                    if child.redefines.is_some() || child.name.is_none() && child.kind != Kind::Group && !filler {
                        continue;
                    }
                    for k in 0..child.occurs {
                        self.receivers_under(c, offset + (child.offset - item.offset) + k * child.size, filler, out);
                    }
                }
            }
            _ => out.push((i, offset)),
        }
    }

    /// Names the files, so that a file-name qualifies its records and its LINAGE-COUNTER.
    pub fn name_files(&mut self, files: &[FileDecl], linage_counters: Vec<Option<usize>>) {
        self.file_names = files.iter().map(|f| f.name.clone()).collect();
        self.linage_counters = linage_counters;
    }

    /// How many programs out the program declaring a name is: 0 for this program's own.
    fn depth(&self, r: Resolved) -> u8 {
        let mut i = match r {
            Resolved::Item(i) => i,
            Resolved::Condition(c) => self.conditions[c].item,
        };
        while let Some(p) = self.items[i].parent {
            i = p;
        }
        self.items[i].linkage.and_then(|l| self.depths.get(l as usize)).copied().unwrap_or_default()
    }

    /// Whether LINKAGE record `ordinal` is one an argument or SET ADDRESS OF addresses.
    pub fn is_argument(&self, ordinal: usize) -> bool {
        self.bindings.get(ordinal).is_none_or(|b| *b == Binding::Argument)
    }

    /// The file whose name qualifies item `i`: the file of its record, or the one it is the
    /// LINAGE-COUNTER of.
    fn file_qualifying(&self, mut i: usize) -> Option<&str> {
        while let Some(p) = self.items[i].parent {
            i = p;
        }
        let k = self.items[i].file.map(usize::from).or_else(|| self.linage_counters.iter().position(|&c| c == Some(i)))?;
        self.file_names.get(k).map(String::as_str)
    }

    /// Whether `qualifiers` are the complete set of a candidate: every name of its hierarchy, the
    /// file-name of its FD or SD allowed last but not needed (assumption
    /// [`numeric::assumptions::COMPLETE_SET_OF_QUALIFIERS`]).
    fn complete(&self, candidate: Resolved, qualifiers: &[String]) -> bool {
        let (start, own) = match candidate {
            Resolved::Item(i) => (self.items[i].parent, i),
            Resolved::Condition(c) => (Some(self.conditions[c].item), self.conditions[c].item),
        };
        let given = || qualifiers.iter().map(String::as_str);
        given().eq(names_from(&self.items, start)) || given().eq(names_from(&self.items, start).chain(self.file_qualifying(own)))
    }

    /// A data item, condition-name or LINAGE-COUNTER named with its qualifiers, the last of which
    /// may be the file-name of an FD or SD (Language Reference SC27-8713-03, pp. 69-70). Under
    /// QUALIFY(EXTEND) a reference the standard's rules find ambiguous names the one candidate it
    /// gives a complete set of qualifiers, if only one (Programming Guide SC27-8714-03, p. 400;
    /// Language Reference SC27-8713-03, pp. 67-68).
    pub fn resolve(&self, name: &str, qualifiers: &[String], pos: Pos) -> Result<Resolved, Error> {
        let within = |mut at: Option<usize>, own: usize| {
            let mut wanted = qualifiers.iter();
            let mut next = wanted.next();
            while let (Some(q), Some(i)) = (next, at) {
                if self.items[i].name.as_deref() == Some(q.as_str()) {
                    next = wanted.next();
                }
                at = self.items[i].parent;
            }
            match next {
                None => true,
                Some(file) => wanted.next().is_none() && self.file_qualifying(own) == Some(file.as_str()),
            }
        };
        fn candidates<'a>(named: &'a HashMap<String, Vec<usize>>, name: &str) -> &'a [usize] {
            named.get(name).map_or(&[], Vec::as_slice)
        }
        let mut found: Vec<Resolved> = candidates(&self.named_items, name).iter().filter(|&&i| within(self.items[i].parent, i)).map(|&i| Resolved::Item(i)).collect();
        found.extend(candidates(&self.named_conditions, name).iter().filter(|&&c| within(Some(self.conditions[c].item), self.conditions[c].item)).map(|&c| Resolved::Condition(c)));
        if found.len() > 1 {
            let nearest = found.iter().map(|&r| self.depth(r)).min().unwrap_or_default();
            found.retain(|&r| self.depth(r) == nearest);
        }
        if found.len() > 1 && self.qualify == Qualify::Extend {
            let complete: Vec<Resolved> = found.iter().copied().filter(|&r| self.complete(r, qualifiers)).collect();
            if let [one] = complete.as_slice() {
                return Ok(*one);
            }
        }
        match found.as_slice() {
            [one] => Ok(*one),
            [] => Err(IWC0001.at(pos, format!("{name} is not defined"))),
            _ => Err(IWC0002.at(pos, format!("{name} is ambiguous; qualify it with OF or IN"))),
        }
    }
}

/// The file whose record area each file uses: its own, or the first file of its SAME RECORD AREA
/// clause. SAME AREA shares the record area of the VSAM (indexed and relative) files it names and
/// is documentation for the rest: SAME_AREA_VSAM in numeric::assumptions.
pub fn record_area_owners(files: &[FileDecl], environment: &Environment) -> Result<Vec<usize>, Error> {
    let mut owner: Vec<usize> = (0..files.len()).collect();
    let root = |owner: &[usize], mut k: usize| {
        while owner[k] != k {
            k = owner[k];
        }
        k
    };
    let clauses = environment.same_record_areas.iter().map(|c| ("SAME RECORD AREA", c)).chain(environment.same_areas.iter().map(|c| ("SAME AREA", c)));
    for (clause, names) in clauses {
        let mut members = Vec::new();
        for name in names {
            let k = files.iter().position(|f| f.name == *name).ok_or_else(|| syntax::messages::IWC0046.at(Pos::default(), format!("{clause} names {name}, which is not a file")))?;
            if clause == "SAME RECORD AREA" || matches!(files[k].organization, Organization::Indexed | Organization::Relative) {
                members.push(k);
            }
        }
        for pair in members.windows(2) {
            let (a, b) = (root(&owner, pair[0]), root(&owner, pair[1]));
            owner[a.max(b)] = a.min(b);
        }
    }
    Ok((0..files.len()).map(|k| root(&owner, k)).collect())
}
