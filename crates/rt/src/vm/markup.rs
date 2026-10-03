//! JSON GENERATE, JSON PARSE, XML GENERATE and XML PARSE (lir.md §9.13), as machine/json.rs,
//! machine/json/parse.rs, machine/xml.rs and machine/xml/generate.rs run them: the same locates,
//! reads and stores in the same order, over the tree each payload lays out.

mod parse;

use super::{Code, Facts, R, Vm, not_yet};
use crate::abend::Abend;
use crate::display::utf16_text;
use crate::json;
use crate::lir::{Ccsid, Convert, DebugId, IntExpr, JsonGenerate, JsonLeaf, JsonNode, JsonValue, Marker, Markup, MovePlan, PlaceId, Step, StorePlan, SymId, XmlForm, XmlGenerate, XmlRegister, XmlValue};
use crate::picture::Sym;
use crate::storage::{Kind, Loc, Val};
use crate::store::{self, ProgramFacts};
use crate::unit::Loader;
use crate::vocab::{Figurative, Pos};
use crate::xml::generate::{self as xml_text, BAD_NAMESPACE, BAD_PREFIX, ILLEGAL_CHARACTERS, NATIONAL_NOT_UTF8, RECEIVER_TOO_SMALL, SUBSTITUTED};
use crate::xml::{Encoding, UTF16};
use numeric::Options;
use std::rc::Rc;
use zarch::ebcdic::{CodePage, Collation};
use zarch::hfp::{Hfp, Precision};

#[derive(Default)]
pub(super) struct State {
    /// The subscripts of the JSON walk in progress: its item's, then one per table entered.
    pub(super) walk: Vec<u32>,
    /// Where each fragment register holds the current XML PARSE event's text, by `slot`: offset
    /// and length, empty outside a processing procedure.
    xml: [(usize, usize); 6],
}

fn slot(register: XmlRegister) -> usize {
    match register {
        XmlRegister::Text => 0,
        XmlRegister::NText => 1,
        XmlRegister::Namespace => 2,
        XmlRegister::NNamespace => 3,
        XmlRegister::Prefix => 4,
        XmlRegister::NPrefix => 5,
    }
}

/// An elementary value as GENERATE writes it.
enum Converted {
    Number(String),
    Chars(String),
}

/// What XML GENERATE carries through its tree: the element names' prefix, and whether it has met
/// a value that is not legal XML or a national value.
struct XmlWalk {
    prefix: String,
    illegal: bool,
    national: bool,
}

/// An element's attributes, as they go in its start tag, and its content.
#[derive(Default)]
struct Parts {
    attributes: String,
    content: String,
}

/// The program's facts, with the name a store plan gives a binary receiver for a TRUNC(OPT)
/// report where its `Loc` names no place, and the receiver's PICTURE scaling where it is known.
pub(super) struct Receiving<'p> {
    pub(super) facts: Facts<'p>,
    pub(super) name: Option<&'p str>,
    pub(super) scaling: Option<u32>,
}

impl ProgramFacts for Receiving<'_> {
    fn options(&self) -> Options {
        self.facts.options()
    }

    fn page(&self) -> &'static CodePage {
        self.facts.page()
    }

    fn figurative(&self, f: Figurative) -> u8 {
        self.facts.figurative(f)
    }

    fn collation(&self) -> &Collation {
        self.facts.collation()
    }

    fn ordinal(&self, byte: u8) -> u16 {
        self.facts.ordinal(byte)
    }

    fn character(&self, ordinal: i64) -> Option<u8> {
        self.facts.character(ordinal)
    }

    fn characters(&self) -> usize {
        self.facts.characters()
    }

    fn decimal_point(&self) -> char {
        self.facts.decimal_point()
    }

    fn edit(&self, edit: u32) -> (&[Sym], &str) {
        self.facts.edit(edit)
    }

    fn scaling(&self, item: usize) -> u32 {
        self.scaling.unwrap_or_else(|| self.facts.scaling(item))
    }

    fn item_name(&self, item: usize) -> String {
        self.name.map_or_else(|| self.facts.item_name(item), str::to_owned)
    }
}

fn node_loc(offset: usize, len: u32, kind: Kind) -> Loc {
    Loc { offset, len: len as usize, kind, item: usize::MAX }
}

/// The CCSID an ENCODING operand gives, as GENERATE reads it: a number that is not one is none.
fn ccsid_of(val: Val) -> Option<u16> {
    match val {
        Val::Num(n) => n.to_i128().and_then(|c| u16::try_from(c).ok()),
        _ => None,
    }
}

impl<'p, L: Loader<Rc<Code>>> Vm<'p, '_, '_, L> {
    /// The statement, its code stored; Arm(1) for an exception and Arm(0) otherwise when it has
    /// either phrase.
    pub(super) fn markup(&mut self, m: &'p Markup, at: DebugId, pos: Pos) -> R<Step> {
        self.unit.unfollowed(match m {
            Markup::JsonGenerate(_) => "JSON GENERATE",
            Markup::XmlGenerate(_) => "XML GENERATE",
            Markup::XmlParse(_) => "XML PARSE",
            Markup::JsonParse(_) => "JSON PARSE",
        });
        let code = match m {
            Markup::JsonGenerate(g) => {
                let code = self.json_generate(g, at, pos)?;
                self.set_register(g.code.0, code, pos)?;
                code
            }
            Markup::XmlGenerate(x) => {
                let code = self.xml_generate(x, at, pos)?;
                self.set_register(x.code.0, code, pos)?;
                code
            }
            Markup::JsonParse(j) => {
                let (code, status) = self.json_parse(j, at, pos)?;
                self.set_register(j.code.0, code, pos)?;
                self.set_register(j.status.0, status, pos)?;
                code
            }
            Markup::XmlParse(x) => match self.xml_parse(x, pos)? {
                Ok(code) => {
                    self.set_register(x.code.0, code, pos)?;
                    code
                }
                Err(left) => return Ok(left),
            },
        };
        Ok(match m.phrases() {
            (false, false) => Step::Next,
            _ => Step::Arm(u8::from(code != 0)),
        })
    }

    /// `set_integer` of a special register or COUNT IN.
    fn set_register(&mut self, place: PlaceId, value: i64, pos: Pos) -> R<()> {
        let dest = self.loc(place)?;
        Ok(store::set_integer(&self.facts(), self.unit, dest, value, pos)?)
    }

    /// The statement's own subscripts, which begin its walk's.
    fn walk_from(&mut self, subscripts: &[IntExpr], pos: Pos) -> R<Vec<u32>> {
        let mut walk = Vec::with_capacity(subscripts.len());
        for s in subscripts {
            walk.push(self.int(s, pos)? as u32);
        }
        Ok(walk)
    }

    /// A store's facts, naming the binary receiver its plan names.
    pub(super) fn receiving(&self, store: Option<&StorePlan>) -> Receiving<'p> {
        let name = match store {
            Some(StorePlan::Binary { name, .. }) => Some(self.sym(*name)),
            _ => None,
        };
        Receiving { facts: self.facts(), name, scaling: None }
    }

    /// MOVE into a receiver by its plan.
    fn move_into(&mut self, dest: Loc, val: Val, plan: &MovePlan, at: DebugId, pos: Pos) -> R<()> {
        let store = match plan {
            MovePlan::Refused(abend) => return Err(self.abend(*abend, Some(at)).into()),
            MovePlan::Numeric { store, .. } => Some(store),
            _ => None,
        };
        let facts = self.receiving(store);
        Ok(store::assign(&facts, self.unit, dest, val, None, pos)?)
    }

    /// The document written into the receiver, as much of it as fits in whole character
    /// positions, and COUNT IN; whether it all fits.
    fn write_document(&mut self, receiver: Loc, bytes: &[u8], unit: usize, count: Option<(PlaceId, StorePlan)>, pos: Pos) -> R<bool> {
        let fits = bytes.len() <= receiver.len;
        let written = json::write_document(&mut self.unit.mem, receiver, bytes, unit);
        if let Some((place, _)) = count {
            self.set_register(place, (written / unit) as i64, pos)?;
        }
        Ok(fits)
    }

    /// XML-TEXT and the other fragment registers, with any reference modification, which is
    /// checked whatever SSRANGE says.
    pub(super) fn xml_register(&mut self, register: XmlRegister, id: PlaceId, pos: Pos) -> R<Loc> {
        let place = &self.p.places[id as usize];
        let (offset, len) = self.markup.xml[slot(register)];
        let kind = place.kind;
        let Some(rm) = &place.refmod else {
            let loc = Loc { offset, len, kind, item: id as usize };
            self.unit.taint_read(loc);
            return Ok(loc);
        };
        let unit = if kind == Kind::National { 2 } else { 1 };
        let start = self.int(&rm.start, pos)?;
        let length = match &rm.length {
            Some(l) => self.int(l, pos)?,
            None => (len / unit) as i64 - start + 1,
        };
        if start < 1 || length < 0 || (start - 1 + length) as usize * unit > len {
            let message = format!("reference modification ({start}:{length}) of {} is outside its {len} bytes", self.sym(place.name));
            return Err(Abend::ironwork(message, pos).into());
        }
        let loc = Loc { offset: offset + (start as usize - 1) * unit, len: length as usize * unit, kind, item: id as usize };
        self.unit.taint_read(loc);
        Ok(loc)
    }

    fn equals(&self, loc: Loc, f: Figurative, pos: Pos) -> R<bool> {
        Ok(json::equals_figurative(&self.facts(), &self.unit.mem, loc, f, pos)?)
    }

    /// `converted`: an elementary item's value, trimmed, as JSON and XML GENERATE write it.
    fn converted(&mut self, convert: Convert, loc: Loc, at: DebugId, pos: Pos) -> R<Converted> {
        let bytes = store::bytes(&self.unit.mem, loc).to_vec();
        let chars = |t: &str, justified: bool| Converted::Chars(json::trimmed(t, justified).to_owned());
        Ok(match convert {
            Convert::Chars { justified } => chars(&self.facts().page().decode(&bytes), justified),
            Convert::National => chars(&utf16_text(&bytes), false),
            Convert::Float(precision) => Converted::Number(json::float_number(Hfp::from_bytes(precision, &bytes), if precision == Precision::Short { 8 } else { 17 })),
            Convert::Fixed { integers } => match self.read(loc, pos)? {
                Val::Num(x) => Converted::Number(json::fixed_number(x.negative, x.magnitude, x.places.dec, integers)),
                _ => return Err(Abend::ironwork("a numeric item without a numeric value", pos).into()),
            },
            Convert::Refused(abend) => return Err(self.abend(abend, Some(at)).into()),
        })
    }

    fn json_generate(&mut self, g: &'p JsonGenerate, at: DebugId, pos: Pos) -> R<i64> {
        let from = self.loc(g.from)?.offset;
        self.markup.walk = self.walk_from(&g.subscripts, pos)?;
        let value = if g.nodes[0].occurs.is_some() {
            self.json_array(g, 0, from, at, pos)?.unwrap_or_else(|| "[]".into())
        } else {
            self.json_value(g, 0, from, at, pos)?.unwrap_or_else(|| "{}".into())
        };
        let document = match g.name {
            Some(name) => format!("{{{}:{value}}}", self.sym(name)),
            None => value,
        };
        let receiver = self.loc(g.receiver)?;
        let national = receiver.kind == Kind::National;
        let ccsid = match g.encoding {
            Ccsid::Unnamed => None,
            Ccsid::CodePage => Some(self.p.options.options.codepage),
            Ccsid::Operand(o) => ccsid_of(self.value(o)?),
        };
        let Some((bytes, unit)) = json::encoded(&document, national, ccsid) else { return Ok(json::BAD_ENCODING) };
        let fits = self.write_document(receiver, &bytes, unit, g.count, pos)?;
        Ok(if fits { 0 } else { json::RECEIVER_TOO_SMALL })
    }

    /// One occurrence of node `k`: null, a group's object, or an elementary item's value; None when
    /// it is left out.
    fn json_value(&mut self, g: &'p JsonGenerate, k: usize, offset: usize, at: DebugId, pos: Pos) -> R<Option<String>> {
        let node = &g.nodes[k];
        let loc = node_loc(offset, node.len, node.kind);
        if self.json_null(node, loc, at, pos)? {
            return Ok(Some("null".into()));
        }
        let (members, eligible) = match &node.value {
            JsonValue::Leaf(leaf) => return self.json_leaf(leaf, loc, at, pos),
            JsonValue::Object { members, eligible } => (members, *eligible),
        };
        let mut pairs = Vec::new();
        for &m in members {
            let member = &g.nodes[m as usize];
            let name = self.sym(member.name);
            let at_member = offset + member.offset as usize;
            let value = if member.occurs.is_some() { self.json_array(g, m as usize, at_member, at, pos)? } else { self.json_value(g, m as usize, at_member, at, pos)? };
            if let Some(v) = value {
                pairs.push(format!("{name}:{v}"));
            }
        }
        Ok((!pairs.is_empty() || !eligible).then(|| format!("{{{}}}", pairs.join(","))))
    }

    /// Table `k`'s occurrences as an array; None when every one is left out.
    fn json_array(&mut self, g: &'p JsonGenerate, k: usize, offset: usize, at: DebugId, pos: Pos) -> R<Option<String>> {
        let node = &g.nodes[k];
        let count = match &node.occurs {
            Some(c) => self.count(c, pos)?,
            None => 1,
        };
        let mut elements = Vec::with_capacity(count as usize);
        let mut all_left_out = count > 0;
        for i in 0..count {
            self.markup.walk.push(i + 1);
            let element = self.json_value(g, k, offset + i as usize * node.len as usize, at, pos)?;
            self.markup.walk.pop();
            if let Some(e) = element {
                all_left_out = false;
                elements.push(e);
            }
        }
        Ok((!all_left_out).then(|| format!("[{}]", elements.join(","))))
    }

    /// `json_null`: the indicator's marker holds, or the item equals CONVERTING's constant.
    fn json_null(&mut self, node: &JsonNode, loc: Loc, at: DebugId, pos: Pos) -> R<bool> {
        if let Some((indicator, marker)) = &node.indicator {
            let indicator = match indicator {
                Ok(place) => self.loc(*place)?,
                Err(abend) => return Err(self.abend(*abend, Some(at)).into()),
            };
            let byte = store::bytes(&self.unit.mem, indicator).first().copied().unwrap_or(0);
            if self.marker_holds(marker, byte, at, pos)? {
                return Ok(true);
            }
        }
        match node.null {
            Some(f) => self.equals(loc, f, pos),
            None => Ok(false),
        }
    }

    fn json_leaf(&mut self, leaf: &JsonLeaf, loc: Loc, at: DebugId, pos: Pos) -> R<Option<String>> {
        for &f in &leaf.suppress {
            if self.equals(loc, f, pos)? {
                return Ok(None);
            }
        }
        if let Some(marker) = &leaf.boolean {
            let byte = store::bytes(&self.unit.mem, loc).first().copied().unwrap_or(0);
            return Ok(Some(if self.marker_holds(marker, byte, at, pos)? { "true" } else { "false" }.into()));
        }
        Ok(Some(match self.converted(leaf.convert, loc, at, pos)? {
            Converted::Number(n) => n,
            Converted::Chars(c) => json::string(&c),
        }))
    }

    fn marker_holds(&mut self, marker: &Marker, byte: u8, at: DebugId, pos: Pos) -> R<bool> {
        match *marker {
            Marker::Byte(b) => Ok(b == Some(byte)),
            Marker::Condition(c) => self.cond(c, pos),
            Marker::Refused(abend) => Err(self.abend(abend, Some(at)).into()),
        }
    }

    fn xml_generate(&mut self, x: &'p XmlGenerate, at: DebugId, pos: Pos) -> R<i64> {
        let receiver = self.loc(x.receiver)?;
        let national = receiver.kind == Kind::National;
        let ccsid = match x.encoding {
            Ccsid::Operand(o) => ccsid_of(self.value(o)?),
            Ccsid::Unnamed if national => Some(UTF16),
            Ccsid::Unnamed => Some(self.p.options.options.codepage),
            Ccsid::CodePage => return Err(not_yet("XML GENERATE ENCODING FROM CODEPAGE")),
        };
        let encoding = match xml_text::encoding(national, ccsid, matches!(x.encoding, Ccsid::Operand(_))) {
            Ok(encoding) => encoding,
            Err(code) => return Ok(code),
        };
        let namespace = match x.namespace {
            Some(o) => self.xml_operand_text(o, pos)?.trim_end_matches(' ').to_owned(),
            None => String::new(),
        };
        if !xml_text::legal(&namespace) {
            return Ok(BAD_NAMESPACE);
        }
        let prefix = match x.prefix {
            Some(o) if !namespace.is_empty() => self.xml_operand_text(o, pos)?.trim_end_matches(' ').to_owned(),
            _ => String::new(),
        };
        if !prefix.is_empty() && !xml_text::valid_prefix(&prefix) {
            return Ok(BAD_PREFIX);
        }
        let declaration = match (namespace.is_empty(), prefix.is_empty()) {
            (true, _) => String::new(),
            (false, true) => format!(" xmlns=\"{}\"", xml_text::escaped(&namespace)),
            (false, false) => format!(" xmlns:{prefix}=\"{}\"", xml_text::escaped(&namespace)),
        };
        let mut w = XmlWalk { prefix: if prefix.is_empty() { String::new() } else { format!("{prefix}:") }, illegal: false, national: false };
        let mut document = String::new();
        if x.declaration {
            document = format!("<?xml version=\"1.0\" encoding=\"{}\"?>", xml_text::encoding_name(ccsid.unwrap_or(UTF16)));
        }
        document.push_str(&self.xml_root(x, &mut w, &declaration, at, pos)?);
        if w.national && matches!(encoding, Encoding::Page(_)) {
            return Ok(NATIONAL_NOT_UTF8);
        }
        let substituted = matches!(encoding, Encoding::Page(page) if document.chars().any(|c| page.encode_char(c).is_none()));
        let (bytes, unit) = (encoding.encode(&document), if national { 2 } else { 1 });
        let fits = self.write_document(receiver, &bytes, unit, x.count, pos)?;
        Ok(if !fits {
            RECEIVER_TOO_SMALL
        } else if w.illegal {
            ILLEGAL_CHARACTERS
        } else if substituted {
            SUBSTITUTED
        } else {
            0
        })
    }

    fn xml_operand_text(&mut self, o: crate::lir::Operand, pos: Pos) -> R<String> {
        Ok(match self.value(o)? {
            Val::Bytes(b) => self.facts().page().decode(&b),
            Val::National(b) => utf16_text(&b),
            _ => return Err(Abend::ironwork("XML GENERATE: a namespace or its prefix is an alphanumeric or national item or literal", pos).into()),
        })
    }

    /// FROM's element, which takes the namespace declaration.
    fn xml_root(&mut self, x: &'p XmlGenerate, w: &mut XmlWalk, declaration: &str, at: DebugId, pos: Pos) -> R<String> {
        let from = self.loc(x.from)?;
        self.walk_from(&x.subscripts, pos)?;
        let root = &x.nodes[0];
        match &root.value {
            XmlValue::Element { .. } => Ok(self.xml_group(x, 0, from.offset, w, Some(declaration), at, pos)?.unwrap_or_default()),
            XmlValue::Leaf { convert, .. } => {
                let (name, value, _) = self.xml_leaf(root.name, from, *convert, w, at, pos)?;
                let prefix = &w.prefix;
                Ok(format!("<{prefix}{name}{declaration}>{value}</{prefix}{name}>"))
            }
            XmlValue::Members { .. } => Err(not_yet("XML GENERATE FROM an unnamed group")),
        }
    }

    /// Group `k`'s element; None when SUPPRESS has left it with no attributes and no content,
    /// unless it is the root.
    #[allow(clippy::too_many_arguments)]
    fn xml_group(&mut self, x: &'p XmlGenerate, k: usize, offset: usize, w: &mut XmlWalk, root: Option<&str>, at: DebugId, pos: Pos) -> R<Option<String>> {
        let mut parts = Parts::default();
        self.xml_members(x, k, offset, w, &mut parts, at, pos)?;
        if root.is_none() && x.suppressing && parts.attributes.is_empty() && parts.content.is_empty() {
            return Ok(None);
        }
        let (name, prefix) = (self.sym(x.nodes[k].name), &w.prefix);
        Ok(Some(format!("<{prefix}{name}{}{}>{}</{prefix}{name}>", root.unwrap_or_default(), parts.attributes, parts.content)))
    }

    /// Each occurrence of each member of group `k`, in order; an unnamed group's members join it.
    #[allow(clippy::too_many_arguments)]
    fn xml_members(&mut self, x: &'p XmlGenerate, k: usize, offset: usize, w: &mut XmlWalk, parts: &mut Parts, at: DebugId, pos: Pos) -> R<()> {
        let (XmlValue::Element { members } | XmlValue::Members { members }) = &x.nodes[k].value else { return Ok(()) };
        for &m in members {
            let node = &x.nodes[m as usize];
            let first = offset + node.offset as usize;
            let count = match &node.occurs {
                Some(c) => self.count(c, pos)?,
                None => 1,
            };
            for i in 0..count as usize {
                let offset = first + i * node.len as usize;
                match &node.value {
                    XmlValue::Members { .. } => self.xml_members(x, m as usize, offset, w, parts, at, pos)?,
                    XmlValue::Element { .. } => {
                        if let Some(element) = self.xml_group(x, m as usize, offset, w, None, at, pos)? {
                            parts.content.push_str(&element);
                        }
                    }
                    XmlValue::Leaf { form, suppress, convert } => {
                        let loc = node_loc(offset, node.len, node.kind);
                        if self.xml_suppressed(loc, suppress, pos)? {
                            continue;
                        }
                        let (name, value, legal) = self.xml_leaf(node.name, loc, *convert, w, at, pos)?;
                        let prefix = &w.prefix;
                        match form {
                            XmlForm::Attribute => parts.attributes.push_str(&format!(" {name}=\"{value}\"")),
                            XmlForm::Content if legal => parts.content.push_str(&value),
                            _ => parts.content.push_str(&format!("<{prefix}{name}>{value}</{prefix}{name}>")),
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn xml_suppressed(&self, loc: Loc, suppress: &[Figurative], pos: Pos) -> R<bool> {
        for &f in suppress {
            if self.equals(loc, f, pos)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// An elementary item's name and value as the document holds them, and whether the value is
    /// legal XML; a value that is not goes in hexadecimal under the name "hex." prefixes.
    fn xml_leaf(&mut self, name: SymId, loc: Loc, convert: Convert, w: &mut XmlWalk, at: DebugId, pos: Pos) -> R<(String, String, bool)> {
        let name = self.sym(name);
        let (Converted::Number(value) | Converted::Chars(value)) = self.converted(convert, loc, at, pos)?;
        w.national |= loc.kind == Kind::National;
        if xml_text::legal(&value) {
            return Ok((name.to_owned(), xml_text::escaped(&value), true));
        }
        w.illegal = true;
        let hex = store::bytes(&self.unit.mem, loc).iter().map(|b| format!("{b:02X}")).collect();
        Ok((format!("hex.{name}"), hex, false))
    }
}
