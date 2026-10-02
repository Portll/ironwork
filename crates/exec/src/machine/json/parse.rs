//! JSON PARSE: JSON text into an item's tree, by MOVE's rules (Language Reference SC27-8713-03,
//! pp. 382-396; JSON-CODE and JSON-STATUS from the Programming Guide SC27-8714-03, pp. 819-821).

use super::*;
use rt::intrinsic::numval::Number;
use rt::json::parse::{self as json, Value, fixed_value};
use rt::json::parse::{ANONYMOUS_ARRAY, DIFFERENT_DUPLICATE, INCOMPATIBLE, NO_MATCH, PARSE_ENCODING, UNCONVERTED_BOOLEAN};
use rt::json::parse::{LONG_ARRAY, LOST, NULL_ELEMENT, NULL_ITEM, SAME_DUPLICATE, SHORT_ARRAY, SIZE_ERROR, SUB, SUBSTITUTED, UNMATCHED_ITEM, UNMATCHED_NAME};
use std::collections::{HashMap, HashSet};

/// An exception ends the parse with its JSON-CODE.
type Code = Result<(), i64>;

/// What the phrases of one JSON PARSE say about each item of the tree.
#[derive(Default)]
struct Phrases<'j> {
    names: HashMap<usize, Option<String>>,
    suppressed: HashSet<usize>,
    ignore_all: bool,
    ignored: HashSet<usize>,
    booleans: HashMap<usize, &'j Flag>,
    null_to: HashMap<usize, Figurative>,
    indicated: HashMap<usize, (&'j Flag, Option<&'j Ref>)>,
    indicators: HashSet<usize>,
}

/// What parsing has met: JSON-STATUS's conditions, whether any value reached an item, and each
/// value taken so far, by item and offset.
#[derive(Default)]
struct Progress<'v> {
    status: i64,
    matched: bool,
    taken: HashMap<(usize, usize), &'v Value>,
}

impl<'p> Machine<'p, '_, '_> {
    fn parse_phrases<'j>(&mut self, j: &'j JsonParse) -> R<Phrases<'j>> {
        let mut p = Phrases::default();
        for (r, name) in &j.names {
            let item = self.item_of(r)?;
            let name = match name {
                None => None,
                Some(Literal::Alnum(s) | Literal::National(s)) => Some(s.clone()),
                Some(_) => return Err(Abend::ironwork("JSON PARSE NAME takes an alphanumeric or national literal", j.pos)),
            };
            p.names.insert(item, name);
        }
        for r in &j.suppress {
            p.suppressed.insert(self.item_of(r)?);
        }
        for r in &j.ignoring {
            match r {
                None => p.ignore_all = true,
                Some(r) => {
                    p.ignored.insert(self.item_of(r)?);
                }
            }
        }
        for (r, conversion) in &j.converting {
            let item = self.item_of(r)?;
            match conversion {
                ParseConversion::Boolean(flag) => {
                    p.booleans.insert(item, flag.as_ref());
                }
                ParseConversion::Null(f) => {
                    p.null_to.insert(item, *f);
                }
            }
        }
        for (r, flag, indicator) in &j.indicating {
            p.indicated.insert(self.item_of(r)?, (flag, indicator.as_ref()));
            let indicator = match (flag, indicator) {
                (_, Some(i)) => self.item_of(i)?,
                (Flag::Condition(c) | Flag::Conditions(c, _), None) => self.variable_of(c)?,
                (Flag::Literals(..), None) => return Err(Abend::ironwork("INDICATING ... USING two literals takes IN and the indicator", j.pos)),
            };
            p.indicators.insert(indicator);
        }
        Ok(p)
    }

    /// Items JSON PARSE leaves alone: unnamed elementary items, REDEFINES and RENAMES items with
    /// what is under them, the null indicators, and groups whose members all are left alone.
    fn parse_ignored(&self, item: usize, p: &Phrases) -> bool {
        let i = &self.layout.items[item];
        if i.redefines.is_some() || i.level == 66 || p.indicators.contains(&item) {
            return true;
        }
        if i.children.is_empty() || i.kind != Kind::Group {
            return i.name.is_none();
        }
        i.children.iter().all(|&c| self.parse_ignored(c, p))
    }

    /// A NAME phrase's literal matches exactly; a data-name matches whatever the case of its letters.
    fn parse_name_matches(&self, item: usize, name: &str, p: &Phrases) -> bool {
        match p.names.get(&item) {
            Some(literal) => literal.as_deref() == Some(name),
            None => self.layout.items[item].name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(name)),
        }
    }

    /// A group's members that JSON names can reach, with their offsets; an unnamed group's members
    /// are its parent's, and suppressed ones are reached but left alone.
    fn parse_members(&self, item: usize, offset: usize, p: &Phrases, out: &mut Vec<(usize, usize)>) {
        let layout = self.layout;
        let i = &layout.items[item];
        for &c in &i.children {
            if self.parse_ignored(c, p) {
                continue;
            }
            let child = &layout.items[c];
            let at = offset + (child.offset - i.offset) as usize;
            match (child.name.is_none(), child.table) {
                (true, false) => self.parse_members(c, at, p, out),
                (true, true) => {}
                (false, _) => out.push((c, at)),
            }
        }
    }

    fn condition_at(&mut self, c: &Ref, subscripts: &[u32]) -> R<Ref> {
        let dims = self.layout.items[self.variable_of(c)?].dims.len();
        Ok(Self::subscripted(c, subscripts, dims))
    }

    /// Sets a USING phrase's first value, `on`, or its second; two literals go in `target`.
    fn parse_flag(&mut self, flag: &Flag, target: Option<Loc>, on: bool, subscripts: &[u32], pos: Pos) -> R<()> {
        match flag {
            Flag::Condition(c) => {
                let r = self.condition_at(c, subscripts)?;
                self.set(&if on { SetStmt::ConditionTrue(vec![r]) } else { SetStmt::ConditionFalse(vec![r]) }, pos)
            }
            Flag::Conditions(yes, no) => {
                let r = self.condition_at(if on { yes } else { no }, subscripts)?;
                self.set(&SetStmt::ConditionTrue(vec![r]), pos)
            }
            Flag::Literals(yes, no) => {
                let value = self.operand(&Operand::Literal((if on { yes } else { no }).clone()), pos)?;
                let target = target.ok_or_else(|| Abend::ironwork("JSON PARSE: two literals need the item they go in", pos))?;
                self.assign(target, value, None, pos)
            }
        }
    }

    fn parse_loc(&self, item: usize, offset: usize) -> Loc {
        let i = &self.layout.items[item];
        Loc { offset, len: i.size as usize, kind: i.kind, item }
    }

    /// One JSON value into one occurrence of an item; `element` when the value is an array's.
    #[allow(clippy::too_many_arguments)]
    fn parse_value<'v>(&mut self, item: usize, offset: usize, subscripts: &mut Vec<u32>, value: &'v Value, element: bool, p: &Phrases, g: &mut Progress<'v>, pos: Pos) -> R<Code> {
        if let Some(&earlier) = g.taken.get(&(item, offset)) {
            if earlier != value {
                return Ok(Err(DIFFERENT_DUPLICATE));
            }
            g.status |= SAME_DUPLICATE;
            return Ok(Ok(()));
        }
        g.taken.insert((item, offset), value);
        let null = *value == Value::Null;
        if let Some(&(flag, indicator)) = p.indicated.get(&item) {
            let target = match indicator {
                Some(r) => {
                    let dims = self.layout.items[self.item_of(r)?].dims.len();
                    Some(self.locate(&Self::subscripted(r, subscripts, dims))?)
                }
                None => None,
            };
            self.parse_flag(flag, target, null, subscripts, pos)?;
        }
        let i = &self.layout.items[item];
        let group = i.kind == Kind::Group && !i.children.is_empty();
        if null {
            g.matched = true;
            if p.indicated.contains_key(&item) {
                return Ok(Ok(()));
            }
            if let Some(&f) = p.null_to.get(&item) {
                self.assign(self.parse_loc(item, offset), Val::Fig(f), None, pos)?;
            } else if !(p.ignore_all || p.ignored.contains(&item)) {
                g.status |= if element { NULL_ELEMENT } else { NULL_ITEM };
            }
            return Ok(Ok(()));
        }
        if group {
            let Value::Object(pairs) = value else { return Ok(Err(INCOMPATIBLE)) };
            return self.parse_object(item, offset, subscripts, pairs, p, g, pos);
        }
        g.matched = true;
        self.parse_elementary(item, self.parse_loc(item, offset), subscripts, value, p, g, pos)
    }

    #[allow(clippy::too_many_arguments)]
    fn parse_object<'v>(&mut self, item: usize, offset: usize, subscripts: &mut Vec<u32>, pairs: &'v [(String, Value)], p: &Phrases, g: &mut Progress<'v>, pos: Pos) -> R<Code> {
        let mut members = Vec::new();
        self.parse_members(item, offset, p, &mut members);
        let mut reached = HashSet::new();
        for (name, value) in pairs {
            let Some(&(child, at)) = members.iter().find(|&&(c, _)| self.parse_name_matches(c, name, p)) else {
                g.status |= UNMATCHED_NAME;
                continue;
            };
            reached.insert(child);
            if p.suppressed.contains(&child) {
                continue;
            }
            // A variably located member is placed by the counts as they stand when its pair is read.
            let at = at - self.moved_within(child, item, pos)?;
            if let Err(code) = self.parse_member(child, at, subscripts, value, p, g, pos)? {
                return Ok(Err(code));
            }
        }
        if members.iter().any(|(c, _)| !reached.contains(c) && !p.suppressed.contains(c)) {
            g.status |= UNMATCHED_ITEM;
        }
        Ok(Ok(()))
    }

    /// A value into a member: a table's occurrences take an array's elements in turn.
    #[allow(clippy::too_many_arguments)]
    fn parse_member<'v>(&mut self, item: usize, offset: usize, subscripts: &mut Vec<u32>, value: &'v Value, p: &Phrases, g: &mut Progress<'v>, pos: Pos) -> R<Code> {
        let (table, size) = (self.layout.items[item].table, self.layout.items[item].size as usize);
        if !table {
            return self.parse_value(item, offset, subscripts, value, false, p, g, pos);
        }
        let elements = match value {
            Value::Array(elements) => elements,
            Value::Null => {
                g.matched = true;
                if !(p.ignore_all || p.ignored.contains(&item)) {
                    g.status |= NULL_ITEM;
                }
                return Ok(Ok(()));
            }
            _ => return Ok(Err(INCOMPATIBLE)),
        };
        let count = self.occurrences(item, pos)? as usize;
        if elements.len() < count {
            g.status |= SHORT_ARRAY;
        }
        if elements.len() > count {
            g.status |= LONG_ARRAY;
        }
        for (k, e) in elements.iter().take(count).enumerate() {
            subscripts.push(k as u32 + 1);
            let code = self.parse_value(item, offset + k * size, subscripts, e, true, p, g, pos);
            subscripts.pop();
            if let Err(code) = code? {
                return Ok(Err(code));
            }
        }
        Ok(Ok(()))
    }

    #[allow(clippy::too_many_arguments)]
    fn parse_elementary(&mut self, item: usize, loc: Loc, subscripts: &[u32], value: &Value, p: &Phrases, g: &mut Progress, pos: Pos) -> R<Code> {
        match value {
            Value::Bool(truth) => {
                let Some(flag) = p.booleans.get(&item) else { return Ok(Err(UNCONVERTED_BOOLEAN)) };
                self.parse_flag(flag, Some(loc), *truth, subscripts, pos)?;
                Ok(Ok(()))
            }
            Value::String(s) => self.parse_string(loc, s, g, pos),
            Value::Number(_) if self.alphabetic(item) => Ok(Err(INCOMPATIBLE)),
            Value::Number(n) => {
                let (negative, int, frac) = json::decimal(n);
                let integer = !n.contains(['.', 'e', 'E']);
                self.parse_number(loc, negative, &int, &frac, integer, g, pos)
            }
            Value::Object(_) | Value::Array(_) | Value::Null => Ok(Err(INCOMPATIBLE)),
        }
    }

    /// A string into a receiver: its characters in the program's code page, or UTF-16, or, for a
    /// numeric receiver, the number it spells (p. 396).
    fn parse_string(&mut self, loc: Loc, s: &str, g: &mut Progress, pos: Pos) -> R<Code> {
        match loc.kind {
            Kind::Alnum { .. } | Kind::AlnumEdited { .. } => {
                let page = self.page;
                let mut substituted = false;
                let bytes: Vec<u8> = s
                    .chars()
                    .map(|c| {
                        page.encode_char(c).unwrap_or_else(|| {
                            substituted = true;
                            SUB
                        })
                    })
                    .collect();
                let space = page.encode_char(' ').unwrap_or(0x40);
                if substituted {
                    g.status |= SUBSTITUTED;
                }
                if bytes.get(loc.len..).is_some_and(|lost| lost.iter().any(|&b| b != space)) {
                    g.status |= LOST;
                }
                self.assign(loc, Val::Bytes(bytes), None, pos)?;
                Ok(Ok(()))
            }
            Kind::National => {
                let units: Vec<u16> = s.encode_utf16().collect();
                if units.get(loc.len / 2..).is_some_and(|lost| lost.iter().any(|&u| u != 0x20)) {
                    g.status |= LOST;
                }
                self.assign(loc, Val::National(units.iter().flat_map(|u| u.to_be_bytes()).collect()), None, pos)?;
                Ok(Ok(()))
            }
            _ => match json::numeric_string(s, self.decimal_point()) {
                Some((negative, int, frac)) => self.parse_number(loc, negative, &int, &frac, true, g, pos),
                None => Ok(Err(INCOMPATIBLE)),
            },
        }
    }

    /// Whether data item `item` is of category alphabetic, which no JSON number moves to (p. 395,
    /// Table 46).
    fn alphabetic(&self, item: usize) -> bool {
        let at = self.layout.items[item].pos;
        let program = self.program;
        let mut entries = program.working_storage.iter().chain(&program.local_storage).chain(&program.linkage).chain(program.files.iter().flat_map(|f| &f.records));
        entries.any(|e| e.pos == at && e.picture.as_deref().is_some_and(compile::is_alphabetic))
    }

    /// A number into a receiver, by MOVE; an alphanumeric or national receiver takes only a number
    /// written as an integer, with no decimal point or exponent (p. 395, Table 46).
    #[allow(clippy::too_many_arguments)]
    fn parse_number(&mut self, loc: Loc, negative: bool, int: &str, frac: &str, integer: bool, g: &mut Progress, pos: Pos) -> R<Code> {
        match loc.kind {
            Kind::Float(precision) => {
                let digits = format!("{int}{frac}");
                let kept = &digits[..digits.len().min(38)];
                let number = Number { negative, digits: kept.parse().unwrap_or(0), decimals: frac.len() as u32, exponent: (digits.len() - kept.len()) as i32 };
                let value = number.to_real().to_hfp(precision).map_err(|c| Abend::check(c, pos))?;
                self.assign(loc, Val::Float(value), None, pos)?;
            }
            Kind::Zoned { scale, .. } | Kind::Packed { scale, .. } | Kind::Binary { scale, .. } => {
                let (fixed, cut) = fixed_value(negative, int, &frac[..frac.len().min(scale as usize)]);
                if self.store_fixed_checked(loc, &fixed, false, false, pos)? || cut {
                    g.status |= SIZE_ERROR;
                }
            }
            Kind::NumericEdited { .. } => self.assign(loc, Val::Num(fixed_value(negative, int, frac).0), None, pos)?,
            Kind::Alnum { .. } | Kind::AlnumEdited { .. } | Kind::National if integer => {
                let width = if loc.kind == Kind::National { loc.len / 2 } else { loc.len };
                if int.len() > width {
                    g.status |= LOST;
                }
                let digits = if int.is_empty() { "0" } else { int };
                let value = if loc.kind == Kind::National { Val::National(digits.encode_utf16().flat_map(u16::to_be_bytes).collect()) } else { Val::Num(fixed_value(false, digits, "").0) };
                self.assign(loc, value, None, pos)?;
            }
            _ => return Ok(Err(INCOMPATIBLE)),
        }
        Ok(Ok(()))
    }

    /// The source's text: UTF-8 unless ENCODING names an EBCDIC code page, UTF-16 when national.
    fn parse_text(&mut self, j: &JsonParse) -> R<Result<String, i64>> {
        let loc = self.locate(&j.source)?;
        let bytes = self.bytes(loc).to_vec();
        let national = loc.kind == Kind::National;
        let ccsid = match &j.encoding {
            None => None,
            Some(Encoding::FromCodepage) if national => return Ok(Err(PARSE_ENCODING)),
            Some(Encoding::FromCodepage) => Some(self.options.codepage),
            Some(Encoding::Ccsid(op)) => Some(match self.operand(op, j.pos)? {
                Val::Num(n) => n.to_i128().and_then(|c| u16::try_from(c).ok()).unwrap_or(0),
                _ => 0,
            }),
        };
        Ok(json::text(bytes, national, ccsid))
    }

    /// A value into the statement's own item: one occurrence, or a whole table.
    fn parse_root<'v>(&mut self, root: &(usize, usize, Vec<u32>, bool), value: &'v Value, p: &Phrases, g: &mut Progress<'v>, pos: Pos) -> R<Code> {
        let (item, offset, subscripts, whole) = root;
        let mut subscripts = subscripts.clone();
        if *whole { self.parse_member(*item, *offset, &mut subscripts, value, p, g, pos) } else { self.parse_value(*item, *offset, &mut subscripts, value, false, p, g, pos) }
    }

    /// JSON-CODE and JSON-STATUS.
    fn parse_document(&mut self, j: &JsonParse, p: &Phrases) -> R<(i64, i64)> {
        let text = match self.parse_text(j)? {
            Ok(text) => text,
            Err(code) => return Ok((code, 0)),
        };
        let document = match json::parse(&text) {
            Ok(document @ (Value::Object(_) | Value::Array(_))) => document,
            Ok(_) => return Ok((json::Invalid::Malformed.code(), 0)),
            Err(invalid) => return Ok((invalid.code(), 0)),
        };
        let root = self.json_root(&j.into, j.pos)?;
        let mut g = Progress::default();
        let code = if p.names.get(&root.0) == Some(&None) {
            self.parse_root(&root, &document, p, &mut g, j.pos)?
        } else {
            let Value::Object(pairs) = &document else { return Ok((ANONYMOUS_ARRAY, 0)) };
            let mut code = Ok(());
            for (name, value) in pairs {
                if !self.parse_name_matches(root.0, name, p) {
                    g.status |= UNMATCHED_NAME;
                    continue;
                }
                code = self.parse_root(&root, value, p, &mut g, j.pos)?;
                if code.is_err() {
                    break;
                }
            }
            code
        };
        Ok(match code {
            Err(code) => (code, g.status),
            Ok(()) if !g.matched => (NO_MATCH, g.status),
            Ok(()) => (0, g.status),
        })
    }

    pub(in crate::machine) fn json_parse(&mut self, j: &'p JsonParse) -> R<Flow> {
        let p = self.parse_phrases(j)?;
        let (code, status) = self.parse_document(j, &p)?;
        self.json_code(code, j.pos)?;
        self.set_integer(&Ref { name: "JSON-STATUS".into(), qualifiers: Vec::new(), subscripts: Vec::new(), refmod: None, pos: j.pos }, status, j.pos)?;
        let handler = if code == 0 { &j.not_on_exception } else { &j.on_exception };
        match handler {
            Some(body) => self.run_block(body),
            None => Ok(Flow::Next),
        }
    }
}
