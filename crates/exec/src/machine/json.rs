//! JSON GENERATE: an item's tree as JSON text (Language Reference SC27-8713-03, pp. 369-382; JSON-CODE
//! values from the Programming Guide SC27-8714-03, p. 819).

use super::*;
use rt::json as text;
use std::collections::{HashMap, HashSet};

const RECEIVER_TOO_SMALL: i64 = 1;
const BAD_ENCODING: i64 = 2;
const UTF8: u16 = 1208;
const UTF16: u16 = 1200;
/// The single-byte EBCDIC code pages a JSON document may be written in (p. 375, Table 45).
const JSON_CCSIDS: &[u16] = &[1047, 1140, 37, 1141, 273, 1142, 277, 1143, 278, 1144, 280, 1145, 284, 1146, 285, 1147, 297, 1148, 500, 1149, 871];

/// What the phrases of one JSON GENERATE say about each item of the tree.
#[derive(Default)]
struct Phrases<'g> {
    names: HashMap<usize, Option<String>>,
    suppressed: HashSet<usize>,
    suppressed_when: HashMap<usize, &'g [Figurative]>,
    every: Vec<(Option<bool>, &'g [Figurative])>,
    boolean: HashMap<usize, &'g Marker>,
    null_when: HashMap<usize, Figurative>,
    indicated: HashMap<usize, &'g NullIndicator>,
    indicators: HashSet<usize>,
    spellings: Vec<(Pos, String)>,
}

impl<'p> Machine<'p, '_, '_> {
    fn item_of(&mut self, r: &Ref) -> R<usize> {
        match self.resolve(r)? {
            Resolved::Item(i) => Ok(i),
            _ => Err(Abend::ironwork(format!("{} is a condition-name, not a data item", r.name), r.pos)),
        }
    }

    fn json_phrases<'g>(&mut self, g: &'g JsonGenerate) -> R<Phrases<'g>> {
        let mut p = Phrases::default();
        for (r, name) in &g.names {
            let item = self.item_of(r)?;
            let name = match name {
                None => None,
                Some(Literal::Alnum(s) | Literal::National(s)) => Some(s.clone()),
                Some(_) => return Err(Abend::ironwork("JSON GENERATE NAME takes an alphanumeric or national literal", g.pos)),
            };
            p.names.insert(item, name);
        }
        for s in &g.suppress {
            match s {
                Suppression::Item { item, when } if when.is_empty() => {
                    p.suppressed.insert(self.item_of(item)?);
                }
                Suppression::Item { item, when } => {
                    p.suppressed_when.insert(self.item_of(item)?, when);
                }
                Suppression::Every { numeric, when } => p.every.push((*numeric, when)),
            }
        }
        for (r, conversion) in &g.converting {
            let item = self.item_of(r)?;
            match conversion {
                JsonConversion::Boolean(m) => {
                    p.boolean.insert(item, m);
                }
                JsonConversion::Null(f) => {
                    p.null_when.insert(item, *f);
                }
            }
        }
        for i in &g.indicating {
            let item = self.item_of(&i.item)?;
            p.indicated.insert(item, i);
            let indicator = match (&i.indicator, &i.marker) {
                (Some(r), _) => self.item_of(r)?,
                (None, Marker::Condition(c)) => self.layout.conditions.iter().find(|k| k.name == c.name).map(|k| k.item).ok_or_else(|| Abend::ironwork(format!("{} is not a condition-name", c.name), c.pos))?,
                (None, Marker::Literal(_)) => return Err(Abend::ironwork("INDICATING ... USING a literal takes IN and the indicator", g.pos)),
            };
            p.indicators.insert(indicator);
        }
        let program = self.program;
        let entries = program.working_storage.iter().chain(&program.local_storage).chain(&program.linkage).chain(program.files.iter().flat_map(|f| &f.records));
        p.spellings = entries.filter_map(|e| Some((e.pos, e.spelled.clone()?))).collect();
        Ok(p)
    }

    /// Items JSON GENERATE leaves out wherever they are: unnamed elementary items, REDEFINES and
    /// RENAMES items with what is under them, and the phrases' null indicators.
    fn json_ignored(&self, item: usize, p: &Phrases) -> bool {
        let i = &self.layout.items[item];
        (i.name.is_none() && i.children.is_empty()) || i.redefines.is_some() || i.level == 66 || p.indicators.contains(&item)
    }

    fn json_name(&self, item: usize, p: &Phrases) -> Option<String> {
        if let Some(name) = p.names.get(&item) {
            return name.clone();
        }
        let i = &self.layout.items[item];
        Some(p.spellings.iter().find(|(at, _)| *at == i.pos).map(|(_, s)| s.clone()).or_else(|| i.name.clone()).unwrap_or_default())
    }

    fn subscripted(r: &Ref, subscripts: &[u32], dims: usize) -> Ref {
        let subscripts = subscripts.iter().take(dims).map(|s| Expr::Operand(Operand::Literal(Literal::Number(s.to_string())))).collect();
        Ref { subscripts, ..r.clone() }
    }

    fn marker_holds(&mut self, marker: &Marker, byte: u8, subscripts: &[u32], pos: Pos) -> R<bool> {
        match marker {
            Marker::Literal(Literal::Alnum(s)) => Ok(self.page.encode(s).ok().and_then(|b| b.first().copied()) == Some(byte)),
            Marker::Literal(_) => Err(Abend::ironwork("a one-character alphanumeric literal", pos)),
            Marker::Condition(c) => {
                let dims = self.layout.conditions.iter().find(|k| k.name == c.name).map_or(0, |k| self.layout.items[k.item].dims.len());
                self.condition(&Cond::Name(Self::subscripted(c, subscripts, dims)), pos)
            }
        }
    }

    /// Whether an item equals a figurative constant: numerically for ZERO and a numeric item,
    /// otherwise character by character.
    fn equals_figurative(&self, loc: Loc, f: Figurative, pos: Pos) -> R<bool> {
        let bytes = self.bytes(loc);
        if f == Figurative::Zero && loc.kind.is_numeric() {
            return Ok(match self.read(loc, pos)? {
                Val::Num(x) => x.magnitude.is_zero(),
                Val::Float(h) => h.fraction == 0,
                _ => false,
            });
        }
        if loc.kind == Kind::National {
            let unit = figurative_unit(f);
            return Ok(bytes.chunks(2).all(|c| c == unit.to_be_bytes()));
        }
        let byte = self.collating.figurative(f);
        Ok(bytes.iter().all(|&b| b == byte))
    }

    /// Whether EVERY [NUMERIC | NONNUMERIC] WHEN selects an item for this figurative constant (pp. 376-377).
    fn every_selects(kind: Kind, numeric: Option<bool>, f: Figurative) -> bool {
        let class_numeric = kind.is_numeric();
        if numeric.is_some_and(|n| n != class_numeric) {
            return false;
        }
        let display = matches!(kind, Kind::Alnum { .. } | Kind::AlnumEdited { .. } | Kind::NumericEdited { .. } | Kind::National | Kind::Zoned { scale: 0, .. });
        match f {
            Figurative::Zero => true,
            _ => display,
        }
    }

    fn json_elementary(&mut self, item: usize, loc: Loc, subscripts: &[u32], p: &Phrases, pos: Pos) -> R<Option<String>> {
        if let Some(i) = p.indicated.get(&item) {
            let indicator = match &i.indicator {
                Some(r) => r.clone(),
                None => {
                    let Marker::Condition(c) = &i.marker else { unreachable!("checked in json_phrases") };
                    let k = self.layout.conditions.iter().find(|k| k.name == c.name).map(|k| k.item).unwrap_or(item);
                    Ref { name: self.layout.items[k].name.clone().unwrap_or_default(), qualifiers: Vec::new(), subscripts: Vec::new(), refmod: None, pos }
                }
            };
            let k = self.item_of(&indicator)?;
            let at = self.locate(&Self::subscripted(&indicator, subscripts, self.layout.items[k].dims.len()))?;
            let byte = self.bytes(at).first().copied().unwrap_or(0);
            if self.marker_holds(&i.marker, byte, subscripts, pos)? {
                return Ok(Some("null".into()));
            }
        }
        if let Some(&f) = p.null_when.get(&item)
            && self.equals_figurative(loc, f, pos)?
        {
            return Ok(Some("null".into()));
        }
        if let Some(when) = p.suppressed_when.get(&item) {
            for &f in *when {
                if self.equals_figurative(loc, f, pos)? {
                    return Ok(None);
                }
            }
        }
        for (numeric, when) in &p.every {
            for &f in *when {
                if Self::every_selects(loc.kind, *numeric, f) && self.equals_figurative(loc, f, pos)? {
                    return Ok(None);
                }
            }
        }
        if let Some(m) = p.boolean.get(&item) {
            let byte = self.bytes(loc).first().copied().unwrap_or(0);
            return Ok(Some(if self.marker_holds(m, byte, subscripts, pos)? { "true" } else { "false" }.into()));
        }
        let bytes = self.bytes(loc).to_vec();
        Ok(Some(match loc.kind {
            Kind::Alnum { justified } => text::string(text::trimmed(&self.page.decode(&bytes), justified)),
            Kind::AlnumEdited { .. } | Kind::NumericEdited { .. } => text::string(text::trimmed(&self.page.decode(&bytes), false)),
            Kind::National => text::string(text::trimmed(&utf16_text(&bytes), false)),
            Kind::Float(precision) => text::float_number(Hfp::from_bytes(precision, &bytes), if precision == Precision::Short { 8 } else { 17 }),
            Kind::Zoned { digits, scale, .. } | Kind::Packed { digits, scale, .. } => self.json_fixed(loc, digits.saturating_sub(scale) + self.scaling(loc), pos)?,
            Kind::Binary { digits, scale, native, .. } => {
                let integers = if native || self.options.trunc == Trunc::Bin {
                    let whole = match digits {
                        0..=4 => 5,
                        5..=9 => 10,
                        _ => 20,
                    };
                    whole - scale.min(whole)
                } else {
                    digits.saturating_sub(scale) + self.scaling(loc)
                };
                self.json_fixed(loc, integers, pos)?
            }
            Kind::Index => self.json_fixed(loc, 10, pos)?,
            Kind::Group => text::string(text::trimmed(&self.page.decode(&bytes), false)),
            Kind::Pointer | Kind::ObjectReference | Kind::ProgramPointer => {
                return Err(Abend::ironwork(format!("JSON GENERATE: {} is a pointer or object reference", self.layout.items[item].name.as_deref().unwrap_or("FILLER")), pos));
            }
        }))
    }

    fn json_fixed(&self, loc: Loc, integers: u32, pos: Pos) -> R<String> {
        match self.read(loc, pos)? {
            Val::Num(x) => Ok(text::fixed_number(x.negative, x.magnitude, x.places.dec, integers)),
            _ => Err(Abend::ironwork("JSON GENERATE: a numeric item without a numeric value", pos)),
        }
    }

    /// One occurrence of an item: a group's object, or an elementary item's value; `None` when it is
    /// suppressed, or is a group whose members all are.
    fn json_value(&mut self, item: usize, offset: usize, subscripts: &mut Vec<u32>, p: &Phrases, pos: Pos) -> R<Option<String>> {
        let layout = self.layout;
        let i = &layout.items[item];
        if i.children.is_empty() || i.kind != Kind::Group {
            let loc = Loc { offset, len: i.size as usize, kind: i.kind, item };
            return self.json_elementary(item, loc, subscripts, p, pos);
        }
        let mut members = Vec::new();
        let mut any_eligible = false;
        self.json_members(item, offset, subscripts, p, pos, &mut members, &mut any_eligible)?;
        Ok((!members.is_empty() || !any_eligible).then(|| format!("{{{}}}", members.join(","))))
    }

    /// The name/value pairs of a group's members; an unnamed group's members join its parent's.
    #[allow(clippy::too_many_arguments)]
    fn json_members(&mut self, item: usize, offset: usize, subscripts: &mut Vec<u32>, p: &Phrases, pos: Pos, members: &mut Vec<String>, any_eligible: &mut bool) -> R<()> {
        let layout = self.layout;
        let i = &layout.items[item];
        for &c in &i.children {
            if self.json_ignored(c, p) {
                continue;
            }
            if p.suppressed.contains(&c) {
                *any_eligible = true;
                continue;
            }
            let child = &layout.items[c];
            let child_offset = offset + (child.offset - i.offset) as usize;
            if child.name.is_none() && !child.table {
                self.json_members(c, child_offset, subscripts, p, pos, members, any_eligible)?;
                continue;
            }
            *any_eligible = true;
            if let Some(pair) = self.json_member(c, child_offset, subscripts, p, pos)? {
                members.push(pair);
            }
        }
        Ok(())
    }

    fn json_member(&mut self, item: usize, offset: usize, subscripts: &mut Vec<u32>, p: &Phrases, pos: Pos) -> R<Option<String>> {
        let name = text::string(&self.json_name(item, p).unwrap_or_default());
        let (table, size) = (self.layout.items[item].table, self.layout.items[item].size as usize);
        if !table {
            return Ok(self.json_value(item, offset, subscripts, p, pos)?.map(|v| format!("{name}:{v}")));
        }
        let count = self.occurrences(item, pos)?;
        let mut elements = Vec::with_capacity(count as usize);
        let mut all_suppressed = count > 0;
        for k in 0..count {
            subscripts.push(k + 1);
            let element = self.json_value(item, offset + k as usize * size, subscripts, p, pos)?;
            subscripts.pop();
            if let Some(e) = element {
                all_suppressed = false;
                elements.push(e);
            }
        }
        Ok((!all_suppressed).then(|| format!("{name}:[{}]", elements.join(","))))
    }

    fn json_document(&mut self, g: &JsonGenerate, p: &Phrases) -> R<String> {
        let from = self.item_of(&g.from)?;
        let loc = self.locate(&g.from)?;
        let mut subscripts = Vec::with_capacity(g.from.subscripts.len());
        for s in &g.from.subscripts {
            subscripts.push(self.integer(s, g.pos)? as u32);
        }
        let value = self.json_value(from, loc.offset, &mut subscripts, p, g.pos)?.unwrap_or_else(|| "{}".into());
        Ok(match self.json_name(from, p) {
            Some(name) => format!("{{{}:{value}}}", text::string(&name)),
            None => value,
        })
    }

    /// The document's bytes for the receiver, and the size of one character position; `None` for
    /// an encoding the statement cannot write.
    fn json_encode(&mut self, g: &JsonGenerate, document: &str, national: bool) -> R<Option<(Vec<u8>, usize)>> {
        let ccsid = match &g.encoding {
            None => None,
            Some(Encoding::FromCodepage) => Some(self.options.codepage),
            Some(Encoding::Ccsid(op)) => match self.operand(op, g.pos)? {
                Val::Num(n) => n.to_i128().and_then(|c| u16::try_from(c).ok()),
                _ => None,
            },
        };
        Ok(match (national, ccsid) {
            (true, None | Some(UTF16)) => Some((document.encode_utf16().flat_map(u16::to_be_bytes).collect(), 2)),
            (true, _) => None,
            (false, None | Some(UTF8)) => Some((document.as_bytes().to_vec(), 1)),
            (false, Some(c)) if JSON_CCSIDS.contains(&c) => CodePage::by_ccsid(c).map(|page| (document.chars().map(|ch| page.encode_char(ch).unwrap_or(0x3F)).collect(), 1)),
            (false, Some(_)) => None,
        })
    }

    fn json_code(&mut self, code: i64, pos: Pos) -> R<()> {
        self.set_integer(&Ref { name: "JSON-CODE".into(), qualifiers: Vec::new(), subscripts: Vec::new(), refmod: None, pos }, code, pos)
    }

    pub(super) fn json_generate(&mut self, g: &'p JsonGenerate) -> R<Flow> {
        let p = self.json_phrases(g)?;
        let document = self.json_document(g, &p)?;
        let receiver = self.locate_receiving(&g.receiver)?;
        let national = receiver.kind == Kind::National;
        let code = match self.json_encode(g, &document, national)? {
            None => BAD_ENCODING,
            Some((bytes, unit)) => {
                let fits = bytes.len() <= receiver.len;
                let written = if fits { bytes.len() } else { receiver.len - receiver.len % unit };
                self.unit.mem[receiver.offset..receiver.offset + written].copy_from_slice(&bytes[..written]);
                if let Some(count) = &g.count {
                    self.set_integer(count, (written / unit) as i64, g.pos)?;
                }
                if fits { 0 } else { RECEIVER_TOO_SMALL }
            }
        };
        self.json_code(code, g.pos)?;
        let handler = if code == 0 { &g.not_on_exception } else { &g.on_exception };
        match handler {
            Some(body) => self.run_block(body),
            None => Ok(Flow::Next),
        }
    }
}
