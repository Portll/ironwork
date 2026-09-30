//! WORKING-STORAGE as IBM lays it out: sizes by USAGE and PICTURE, REDEFINES sharing storage,
//! OCCURS repeating it. See [`numeric::assumptions::WORKING_STORAGE_LAYOUT`] for where each 01
//! level starts.

use crate::picture::{self, Category, Sym};
use syntax::ast::{DataEntry, Environment, FileDecl, Literal, Organization, Usage};
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
    pub pos: Pos,
}

#[derive(Clone, Debug)]
pub struct Condition {
    pub name: String,
    pub item: usize,
    pub values: Vec<(Literal, Option<Literal>)>,
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
    /// Offset and size of each file's record area, in declaration order.
    pub file_areas: Vec<(u32, u32)>,
    /// The item of each LINKAGE record, in order.
    pub linkage_roots: Vec<usize>,
    /// Bytes of LOCAL-STORAGE each activation gets.
    pub local_size: u32,
    pub size: u32,
    /// Each FD and SD name, in declaration order: the highest qualifier of its records.
    pub file_names: Vec<String>,
    /// The LINAGE-COUNTER item of each file whose FD has LINAGE, which its file-name qualifies.
    pub linage_counters: Vec<Option<usize>>,
}

const LEVEL_ALIGNMENT: u32 = 8;
/// The interpreter's ceiling on WORKING-STORAGE, well under Enterprise COBOL's own.
pub const MAX_STORAGE: u32 = 128 << 20;

/// Lays out WORKING-STORAGE, then each file's record area, which all its 01 records share and
/// which is at least `record_max` bytes. Files whose `shared` entry names the same file share one
/// area, as large as the largest of them (see [`record_area_owners`]).
pub fn build(entries: &[DataEntry], files: &[(&[DataEntry], Option<u32>)], shared: &[usize], linkage: &[DataEntry], local: &[DataEntry]) -> Result<Layout, Error> {
    let mut items: Vec<Item> = Vec::new();
    let mut usages: Vec<Option<Usage>> = Vec::new();
    let mut conditions = Vec::new();
    let mut open: Vec<usize> = Vec::new();
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
    for &(region, e) in &tagged {
        if region != group {
            open.clear();
            group = region;
        }
        let file = region.filter(|&r| r != LINKAGE && r != LOCAL);
        let in_linkage = region == Some(LINKAGE);


        if e.level == 88 {
            let item = *open.last().ok_or_else(|| Error::at(e.pos, "a level-88 entry with no item before it"))?;
            let name = e.name.clone().ok_or_else(|| Error::at(e.pos, "a level-88 entry needs a name"))?;
            conditions.push(Condition { name, item, values: e.condition_values.clone() });
            continue;
        }
        if e.level == 66 {
            return Err(Error::at(e.pos, "RENAMES (level 66) is not supported yet"));
        }
        if !(e.level == 1 || e.level == 77 || (2..=49).contains(&e.level)) {
            return Err(Error::at(e.pos, format!("level {} is not a data level", e.level)));
        }
        if e.sync {
            return Err(Error::at(e.pos, "SYNCHRONIZED is not supported yet"));
        }
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
            pos: e.pos,
        });
        if (e.level == 1 || e.level == 77) && e.occurs.is_some() {
            return Err(Error::at(e.pos, "OCCURS is not allowed at level 01 or 77"));
        }
        if e.occurs == Some(0) {
            return Err(Error::at(e.pos, "OCCURS 0 is not a table"));
        }
        let inherited = parent.and_then(|p| usages[p]);
        usages.push(e.usage.or(inherited));
        if let Some(p) = parent {
            items[p].children.push(index);
        }
        open.push(index);
    }
    let mut edits = Vec::new();
    for (index, (_, e)) in tagged.iter().filter(|(_, e)| e.level != 88).enumerate() {
        items[index].kind = kind(e, &items[index], usages[index], &mut edits)?;
        items[index].size = elementary_size(&items[index], declared_size(e));
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
                pos: e.pos,
            });
        }
    }
    let roots: Vec<usize> = (0..items.len()).filter(|&i| items[i].parent.is_none()).collect();
    for &r in &roots {
        measure(&mut items, r)?;
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
    for &r in &roots {
        if items[r].local {
            continue;
        }
        if items[r].linkage.is_some() {
            place(&mut items, r, 0, Vec::new());
            continue;
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
    Ok(Layout { items, conditions, edits, file_areas: areas, linkage_roots, local_size: local_cursor, size: cursor, file_names: Vec::new(), linage_counters: Vec::new() })
}

fn kind(e: &DataEntry, item: &Item, usage: Option<Usage>, edits: &mut Vec<Vec<Sym>>) -> Result<Kind, Error> {
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
    let pic = picture::analyse(e.picture.as_deref().unwrap()).map_err(err)?;
    if e.blank_when_zero && pic.category != Category::NumericEdited {
        return Err(err("BLANK WHEN ZERO is supported on numeric-edited items only, so far".into()));
    }
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
fn measure(items: &mut [Item], index: usize) -> Result<(), Error> {
    if items[index].kind != Kind::Group {
        return Ok(());
    }
    let children = items[index].children.clone();
    let (mut cursor, mut extent) = (0u32, 0u32);
    let mut placed: Vec<(Option<String>, u32)> = Vec::new();
    for c in children {
        measure(items, c)?;
        let offset = match items[c].redefines.clone() {
            Some(target) => placed.iter().rev().find(|(n, _)| n.as_deref() == Some(target.as_str())).map(|&(_, o)| o).ok_or_else(|| {
                Error::at(items[c].pos, format!("REDEFINES {target}: no earlier item of that name at this level"))
            })?,
            None => cursor,
        };
        let too_large = || Error::at(items[c].pos, format!("an item larger than the interpreter's {MAX_STORAGE} bytes"));
        let span = items[c].size.checked_mul(items[c].occurs).filter(|&s| s <= MAX_STORAGE).ok_or_else(too_large)?;
        if items[c].redefines.is_none() {
            cursor = cursor.checked_add(span).filter(|&s| s <= MAX_STORAGE).ok_or_else(too_large)?;
        }
        extent = extent.max(offset + span);
        items[c].offset = offset;
        placed.push((items[c].name.clone(), offset));
    }
    items[index].size = cursor.max(extent);
    Ok(())
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

    /// A data item, condition-name or LINAGE-COUNTER named with its qualifiers, the last of which
    /// may be the file-name of an FD or SD (Language Reference SC27-8713-03, pp. 69-70).
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
        match found.as_slice() {
            [one] => Ok(*one),
            [] => Err(Error::at(pos, format!("{name} is not defined"))),
            _ => Err(Error::at(pos, format!("{name} is ambiguous; qualify it with OF or IN"))),
        }
    }
}

fn declared_size(e: &DataEntry) -> Option<u32> {
    e.picture.as_deref().and_then(|p| picture::analyse(p).ok()).map(|p| p.size)
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
