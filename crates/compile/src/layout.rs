//! WORKING-STORAGE as IBM lays it out: sizes by USAGE and PICTURE, REDEFINES sharing storage,
//! OCCURS repeating it, SYNCHRONIZED slack bytes before an item and after each occurrence, and
//! level-66 RENAMES over what is laid out. See [`numeric::assumptions::WORKING_STORAGE_LAYOUT`]
//! for where each 01 level starts.

use crate::picture::{self, Category, Sym};
use numeric::Qualify;
use syntax::ast::{DataEntry, Environment, FileDecl, Literal, Organization, Ref, Usage};
use syntax::{Error, Pos};
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
    /// The OCCURS DEPENDING ON table this group ends with, whose current count sets its length.
    pub odo: Option<usize>,
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

pub struct Layout {
    pub items: Vec<Item>,
    pub conditions: Vec<Condition>,
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
    /// The item of each LINKAGE record, in order.
    pub linkage_roots: Vec<usize>,
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
    local: &[DataEntry],
    notation: picture::Notation,
    qualify: Qualify,
    parmcheck: Option<(usize, u32)>,
) -> Result<Layout, Error> {
    let mut items: Vec<Item> = Vec::new();
    let mut usages: Vec<Option<Usage>> = Vec::new();
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
    for &(region, e) in &tagged {
        if region != group {
            open.clear();
            group = region;
            after_renames = false;
        }
        let file = region.filter(|&r| r != LINKAGE && r != LOCAL);
        let in_linkage = region == Some(LINKAGE);
        if e.occurs.is_some() && matches!(e.level, 1 | 66 | 77 | 88) {
            return Err(Error::at(e.pos, format!("OCCURS at level {:02}: Enterprise COBOL takes OCCURS only at levels 02 to 49", e.level)));
        }
        if e.level == 88 {
            if after_renames {
                return Err(Error::at(e.pos, "a level-88 entry after a level-66 entry: a RENAMES item cannot be a conditional variable"));
            }
            let item = *open.last().ok_or_else(|| Error::at(e.pos, "a level-88 entry with no item before it"))?;
            let name = e.name.clone().ok_or_else(|| Error::at(e.pos, "a level-88 entry needs a name"))?;
            conditions.push(Condition { name, item, values: e.condition_values.clone(), false_value: e.false_value.clone() });
            continue;
        }
        if e.renames.is_some() != (e.level == 66) {
            return Err(Error::at(e.pos, "RENAMES goes with level 66, and level 66 with RENAMES"));
        }
        if e.level == 66 {
            let record = open.first().copied().filter(|&r| items[r].level == 1).ok_or_else(|| Error::at(e.pos, "a level-66 entry must follow the entries of a level-01 record"))?;
            if e.name.is_none() || e.picture.is_some() || e.usage.is_some() || e.value.is_some() || e.redefines.is_some() || e.sync || e.sign.is_some() {
                return Err(Error::at(e.pos, "a level-66 entry has a name and a RENAMES clause, and nothing else"));
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
                odo: None,
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
                pos: e.pos,
            });
            usages.push(None);
            synchronized.push(false);
            after_renames = true;
            continue;
        }
        if !(e.level == 1 || e.level == 77 || (2..=49).contains(&e.level)) {
            return Err(Error::at(e.pos, format!("level {} is not a data level", e.level)));
        }
        if after_renames && e.level != 1 && e.level != 77 {
            return Err(Error::at(e.pos, format!("level {:02} after a level-66 entry: a record's RENAMES entries follow its last entry", e.level)));
        }
        after_renames = false;
        while open.last().is_some_and(|&i| items[i].level >= e.level || items[i].level == 77) {
            open.pop();
        }
        let parent = if e.level == 1 || e.level == 77 { None } else { open.last().copied() };
        if e.level != 1 && e.level != 77 && parent.is_none() {
            return Err(Error::at(e.pos, format!("level {} with no group to belong to", e.level)));
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
            odo: None,
            index_names: e.indexed_by.clone(),
            keys: e.keys.clone(),
            local: region == Some(LOCAL),
            kind: Kind::Group,
            value: e.value.clone().filter(|_| file.is_none() && !in_linkage),
            dims: Vec::new(),
            redefines: e.redefines.clone(),
            file,
            linkage: if in_linkage {
                Some(match parent {
                    None => {
                        linkage_roots.push(index);
                        linkage_roots.len() as u16 - 1
                    }
                    Some(p) => items[p].linkage.unwrap_or_default(),
                })
            } else {
                None
            },
            object_class: e.object_class.clone(),
            scaling: 0,
            pos: e.pos,
        });
        if e.occurs == Some(0) {
            return Err(Error::at(e.pos, "OCCURS 0 is not a table"));
        }
        let inherited = parent.and_then(|p| usages[p]);
        usages.push(e.usage.or(inherited));
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
        let pic = e.picture.as_deref().map(|p| picture::analyse_with(p, notation).map_err(|m| Error::at(e.pos, m))).transpose()?;
        items[index].kind = kind(e, &items[index], usages[index], pic.as_ref(), &mut edits)?;
        if currencies.len() < edits.len() {
            currencies.push(pic.as_ref().and_then(|p| p.currency.clone()).unwrap_or_default());
        }
        items[index].scaling = pic.as_ref().map_or(0, |p| p.scaling);
        items[index].size = elementary_size(&items[index], pic.as_ref().map(|p| p.size));
        if synchronized[index] && items[index].kind != Kind::Group {
            aligns[index] = alignment(items[index].kind);
        }
    }
    for (_, e) in &tagged {
        for name in &e.indexed_by {
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
                odo: None,
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
                pos: e.pos,
            });
        }
    }
    aligns.resize(items.len(), 1);
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
            return Err(Error::at(items[r].pos, format!("LOCAL-STORAGE exceeds the interpreter's {MAX_STORAGE} bytes")));
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
                return Err(Error::at(items[r].pos, format!("storage exceeds the interpreter's {MAX_STORAGE} bytes")));
            }
            place(&mut items, r, start, Vec::new());
            continue;
        }
        let offset = match &items[r].redefines {
            Some(target) => root_offsets.iter().find(|(n, _)| n == target).map(|&(_, o)| o).ok_or_else(|| {
                Error::at(items[r].pos, format!("REDEFINES {target}: no earlier 01-level item of that name"))
            })?,
            None => cursor.div_ceil(LEVEL_ALIGNMENT) * LEVEL_ALIGNMENT,
        };
        cursor = cursor.max(offset + items[r].size);
        if cursor > MAX_STORAGE {
            return Err(Error::at(items[r].pos, format!("WORKING-STORAGE exceeds the interpreter's {MAX_STORAGE} bytes")));
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
    for t in tables {
        let (mut child, mut at) = (t, items[t].parent);
        while let Some(a) = at {
            let last = items[a].children.iter().rev().find(|&&c| items[c].redefines.is_none()).copied();
            if last != Some(child) {
                return Err(Error::at(items[t].pos, "items after an OCCURS DEPENDING ON table in the same record are not supported yet"));
            }
            items[a].odo.get_or_insert(t);
            (child, at) = (a, items[a].parent);
        }
    }
    for (index, e) in renames {
        rename(&mut items, index, e, qualify)?;
    }
    let mut areas = Vec::new();
    for (k, &size) in own.iter().enumerate() {
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
        let fewer = items[r].odo.map_or(0, |t| {
            let t = &items[t];
            let outer: u32 = t.dims[..t.dims.len().saturating_sub(1)].iter().map(|&(_, n)| n).product();
            t.occurs.saturating_sub(t.occurs_min) * t.size * outer
        });
        let (least, most) = (items[r].size.saturating_sub(fewer), items[r].size);
        let lengths = &mut record_lengths[k as usize];
        *lengths = Some(lengths.map_or((least, most), |(l, m)| (l.min(least), m.max(most))));
    }
    Ok(Layout { items, conditions, edits, currencies, file_areas: areas, record_lengths, linkage_roots, local_size: local_cursor, size: cursor, file_names: Vec::new(), linage_counters: Vec::new(), qualify, parmcheck: buffer })
}

/// The names of item `start` and each group above it, nearest first: the hierarchy of names that
/// qualifies an item `start` holds or a condition-name of `start`. FILLER and unnamed items give
/// none.
fn names_from(items: &[Item], start: Option<usize>) -> impl Iterator<Item = &str> {
    std::iter::successors(start, |&p| items[p].parent).filter_map(|p| items[p].name.as_deref())
}

/// Gives level-66 entry `index` the storage and attributes of what it renames (Language Reference
/// SC27-8713-03, pp. 228-229): one item as that item is, or from the start of the first item
/// through the end of the last as an alphanumeric group.
fn rename(items: &mut [Item], index: usize, e: &DataEntry, qualify: Qualify) -> Result<(), Error> {
    let Some((first, last)) = &e.renames else { return Ok(()) };
    let record = items[index].parent.unwrap_or(index);
    let find = |r: &Ref| -> Result<usize, Error> {
        let err = |m: String| Err(Error::at(r.pos, format!("RENAMES {}: {m}", r.name)));
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
        return Err(Error::at(first.pos, format!("RENAMES {}: a level-66 entry cannot rename a level-01 record", first.name)));
    }
    let a = find(first)?;
    let Some(last) = last else {
        let (offset, size, kind, scaling, odo) = (items[a].offset, items[a].size, items[a].kind, items[a].scaling, items[a].odo);
        let it = &mut items[index];
        (it.offset, it.size, it.kind, it.scaling, it.odo) = (offset, size, kind, scaling, odo);
        return Ok(());
    };
    let b = find(last)?;
    let end = |i: usize| items[i].offset + items[i].size;
    let mut up = items[b].parent;
    while let Some(p) = up {
        if p == a {
            return Err(Error::at(last.pos, format!("RENAMES {} THRU {}: the last item cannot be within the first", first.name, last.name)));
        }
        up = items[p].parent;
    }
    if a == b || items[b].offset < items[a].offset || end(b) < end(a) {
        return Err(Error::at(last.pos, format!("RENAMES {} THRU {}: the last item must start and end no earlier than the first", first.name, last.name)));
    }
    let root = |mut i: usize| {
        while let Some(p) = items[i].parent {
            i = p;
        }
        i
    };
    if let Some(t) = (0..items.len()).find(|&i| items[i].depending_on.is_some() && root(i) == record && items[i].offset >= items[a].offset && items[i].offset < end(b)) {
        return Err(Error::at(items[t].pos, format!("RENAMES {} THRU {}: no OCCURS DEPENDING ON between them", first.name, last.name)));
    }
    let (offset, size) = (items[a].offset, end(b) - items[a].offset);
    let it = &mut items[index];
    (it.offset, it.size, it.kind) = (offset, size, Kind::Group);
    Ok(())
}

/// The boundary a SYNCHRONIZED item of this kind is aligned on (Language Reference SC27-8713-03,
/// pp. 232-233); 1 for the kinds SYNCHRONIZED leaves where they are.
fn alignment(kind: Kind) -> u32 {
    match kind {
        Kind::Binary { digits: 0..=4, .. } => 2,
        Kind::Binary { .. } | Kind::Float(Precision::Short) | Kind::Pointer | Kind::Index | Kind::ObjectReference | Kind::ProgramPointer => 4,
        Kind::Float(_) => 8,
        _ => 1,
    }
}

fn kind(e: &DataEntry, item: &Item, usage: Option<Usage>, pic: Option<&picture::Picture>, edits: &mut Vec<Vec<Sym>>) -> Result<Kind, Error> {
    let err = |m: String| Error::at(e.pos, m);
    let usage = usage.unwrap_or_default();
    let handle = matches!(usage, Usage::ObjectReference | Usage::ProgramPointer);
    let elementary = e.picture.is_some() || (handle || matches!(usage, Usage::Float1 | Usage::Float2 | Usage::Pointer | Usage::Index)) && item.children.is_empty();
    if !elementary {
        return if item.children.is_empty() { Err(err("an elementary item needs a PICTURE".into())) } else { Ok(Kind::Group) };
    }
    if !item.children.is_empty() {
        return Err(err("a group item cannot have a PICTURE".into()));
    }
    if handle {
        if e.picture.is_some() || e.value.as_ref().is_some_and(|v| *v != Literal::Figurative(syntax::ast::Figurative::Null)) {
            return Err(err("an object reference, function-pointer or procedure-pointer takes no PICTURE and only VALUE NULL".into()));
        }
        return Ok(if usage == Usage::ObjectReference { Kind::ObjectReference } else { Kind::ProgramPointer });
    }
    if let Usage::Pointer | Usage::Index = usage {
        if e.picture.is_some() {
            return Err(err("POINTER and INDEX items take no PICTURE".into()));
        }
        return Ok(if usage == Usage::Pointer { Kind::Pointer } else { Kind::Index });
    }
    if let Usage::Float1 | Usage::Float2 = usage {
        if e.picture.is_some() {
            return Err(err("COMP-1 and COMP-2 items take no PICTURE".into()));
        }
        return Ok(Kind::Float(if usage == Usage::Float1 { Precision::Short } else { Precision::Long }));
    }
    let Some(pic) = pic else { return Err(err("an elementary item needs a PICTURE".into())) };
    let blank_numeric;
    let pic = match pic.category {
        Category::Numeric if e.blank_when_zero && usage == Usage::Display => {
            blank_numeric = picture::blank_when_zero(pic).map_err(err)?;
            &blank_numeric
        }
        Category::NumericEdited => pic,
        Category::Numeric if e.blank_when_zero && usage == Usage::National => return Err(err("BLANK WHEN ZERO on a USAGE NATIONAL item is not supported yet".into())),
        _ if e.blank_when_zero => return Err(err("BLANK WHEN ZERO needs a numeric or numeric-edited item of USAGE DISPLAY or NATIONAL".into())),
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
        (Category::Numeric, Usage::Display) => Kind::Zoned { digits: pic.digits, scale: pic.scale, signed: pic.signed, sign: e.sign },
        (Category::Numeric, Usage::Packed) => Kind::Packed { digits: pic.digits, scale: pic.scale, signed: pic.signed },
        (Category::Numeric, Usage::Binary | Usage::NativeBinary) if pic.digits <= 18 => {
            Kind::Binary { digits: pic.digits, scale: pic.scale, signed: pic.signed, native: usage == Usage::NativeBinary }
        }
        (Category::Numeric, Usage::Binary | Usage::NativeBinary) => return Err(err("a binary item holds at most 18 digits".into())),
        (Category::Alphanumeric, Usage::Display) => Kind::Alnum { justified: e.justified },
        (Category::National, Usage::Display | Usage::National) => Kind::National,
        (category, usage) => return Err(err(format!("a {category:?} PICTURE with USAGE {usage:?} is not supported yet"))),
    };
    if let Kind::Zoned { sign: Some(_), signed: false, .. } = k {
        return Err(err("a SIGN clause needs an S in the PICTURE".into()));
    }
    Ok(k)
}

fn elementary_size(item: &Item, e_size: Option<u32>) -> u32 {
    match item.kind {
        Kind::Group => 0,
        Kind::Alnum { .. } | Kind::NumericEdited { .. } | Kind::AlnumEdited { .. } => e_size.unwrap_or(0),
        Kind::National => 2 * e_size.unwrap_or(0),
        Kind::Zoned { digits, sign, .. } => digits + sign.is_some_and(|s| s.separate) as u32,
        Kind::Packed { digits, .. } => digits / 2 + 1,
        Kind::Binary { digits, .. } => match digits {
            0..=4 => 2,
            5..=9 => 4,
            _ => 8,
        },
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
                Error::at(items[c].pos, format!("REDEFINES {target}: no earlier item of that name at this level"))
            })?),
            None => None,
        };
        let start = redefined.unwrap_or(cursor);
        let m = first_alignment(items, aligns, c);
        let slack = (m - (base + start) % m) % m;
        if slack > 0 && redefined.is_some() {
            return Err(Error::at(items[c].pos, format!("a SYNCHRONIZED item at the start of a REDEFINES would need {slack} slack bytes: the redefined item must be on a {m}-byte boundary")));
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
        let too_large = || Error::at(items[c].pos, format!("an item larger than the interpreter's {MAX_STORAGE} bytes"));
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

impl Layout {
    /// Names the files, so that a file-name qualifies its records and its LINAGE-COUNTER.
    pub fn name_files(&mut self, files: &[FileDecl], linage_counters: Vec<Option<usize>>) {
        self.file_names = files.iter().map(|f| f.name.clone()).collect();
        self.linage_counters = linage_counters;
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
        let mut found: Vec<Resolved> = self
            .items
            .iter()
            .enumerate()
            .filter(|(i, it)| it.name.as_deref() == Some(name) && within(it.parent, *i))
            .map(|(i, _)| Resolved::Item(i))
            .collect();
        found.extend(
            self.conditions.iter().enumerate().filter(|(_, c)| c.name == name && within(Some(c.item), c.item)).map(|(i, _)| Resolved::Condition(i)),
        );
        if found.len() > 1 && self.qualify == Qualify::Extend {
            let complete: Vec<Resolved> = found.iter().copied().filter(|&r| self.complete(r, qualifiers)).collect();
            if let [one] = complete.as_slice() {
                return Ok(*one);
            }
        }
        match found.as_slice() {
            [one] => Ok(*one),
            [] => Err(Error::at(pos, format!("{name} is not defined"))),
            _ => Err(Error::at(pos, format!("{name} is ambiguous; qualify it with OF or IN"))),
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
            let k = files.iter().position(|f| f.name == *name).ok_or_else(|| Error::at(Pos::default(), format!("{clause} names {name}, which is not a file")))?;
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
