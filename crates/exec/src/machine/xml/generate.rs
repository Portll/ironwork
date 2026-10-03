//! XML GENERATE: an item's tree as an XML document (Language Reference SC27-8713-03, pp. 484-494;
//! XML-CODE values from the Programming Guide SC27-8714-03, pp. 817-818).

use super::json::Converted;
use super::*;
use rt::xml::UTF16;
use rt::xml::generate::{self as text, BAD_NAMESPACE, BAD_PREFIX, ILLEGAL_CHARACTERS, NATIONAL_NOT_UTF8, RECEIVER_TOO_SMALL, SUBSTITUTED};
use std::collections::{HashMap, HashSet};

/// What the phrases of one XML GENERATE say about each item of the tree, and what generating it
/// has met so far.
#[derive(Default)]
struct Phrases<'x> {
    names: HashMap<usize, String>,
    forms: HashMap<usize, XmlForm>,
    suppressed: HashSet<usize>,
    suppressed_when: HashMap<usize, &'x [Figurative]>,
    every: Vec<(Option<bool>, Option<XmlForm>, &'x [Figurative])>,
    attributes: bool,
    suppressing: bool,
    /// "prefix:" before each element name, or nothing.
    prefix: String,
    spellings: Vec<(Pos, String)>,
    illegal: bool,
    national: bool,
}

/// An element's attributes, as they go in its start tag, and its content.
#[derive(Default)]
struct Parts {
    attributes: String,
    content: String,
}

impl<'p> Machine<'p, '_, '_> {
    fn xml_phrases<'x>(&mut self, x: &'x XmlGenerate) -> R<Phrases<'x>> {
        let mut p = Phrases { attributes: x.attributes, suppressing: !x.suppress.is_empty(), spellings: self.spellings(), ..Default::default() };
        for (r, name) in &x.names {
            let item = self.item_of(r)?;
            let (Literal::Alnum(s) | Literal::National(s)) = name else {
                return Err(Abend::ironwork("XML GENERATE NAME takes an alphanumeric or national literal", x.pos));
            };
            p.names.insert(item, s.clone());
        }
        for (r, form) in &x.types {
            p.forms.insert(self.item_of(r)?, *form);
        }
        for s in &x.suppress {
            match s {
                Suppression::Item { item, when } if when.is_empty() => {
                    p.suppressed.insert(self.item_of(item)?);
                }
                Suppression::Item { item, when } => {
                    p.suppressed_when.insert(self.item_of(item)?, when);
                }
                Suppression::Every { numeric, form, when } => p.every.push((*numeric, *form, when)),
            }
        }
        Ok(p)
    }

    /// Items XML GENERATE leaves out: unnamed elementary items, REDEFINES and RENAMES items with what
    /// is under them, and groups whose members all are left out.
    fn xml_ignored(&self, item: usize) -> bool {
        let i = &self.layout.items[item];
        if i.redefines.is_some() || i.level == 66 {
            return true;
        }
        if i.children.is_empty() || i.kind != Kind::Group {
            return i.name.is_none();
        }
        i.children.iter().all(|&c| self.xml_ignored(c))
    }

    fn xml_name(&self, item: usize, p: &Phrases) -> String {
        if let Some(name) = p.names.get(&item) {
            return name.clone();
        }
        let i = &self.layout.items[item];
        let spelled = p.spellings.iter().find(|(at, _)| *at == i.pos).map(|(_, s)| s.as_str());
        text::name(spelled.or(i.name.as_deref()).unwrap_or_default())
    }

    fn xml_form(&self, item: usize, p: &Phrases) -> XmlForm {
        if let Some(&form) = p.forms.get(&item) {
            return form;
        }
        let i = &self.layout.items[item];
        if p.attributes && !i.table && i.name.is_some() { XmlForm::Attribute } else { XmlForm::Element }
    }

    /// Whether SUPPRESS leaves out an elementary item: its own WHEN phrase, if it has one, decides
    /// instead of any EVERY phrase.
    fn xml_suppressed(&self, item: usize, loc: Loc, form: XmlForm, p: &Phrases, pos: Pos) -> R<bool> {
        if let Some(when) = p.suppressed_when.get(&item) {
            for &f in *when {
                if rt::json::equals_figurative(&self.facts(), &self.unit.mem, loc, f, pos)? {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        for &(numeric, every_form, when) in &p.every {
            if every_form.is_some_and(|f| f != form) {
                continue;
            }
            for &f in when {
                if Self::every_selects(loc.kind, numeric, f) && rt::json::equals_figurative(&self.facts(), &self.unit.mem, loc, f, pos)? {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// An elementary item's name and value as the document holds them, and whether the value is
    /// legal XML; a value that is not goes in hexadecimal under the name "hex." prefixes (417).
    fn xml_leaf(&mut self, item: usize, loc: Loc, p: &mut Phrases, pos: Pos) -> R<(String, String, bool)> {
        let name = self.xml_name(item, p);
        let (Converted::Number(value) | Converted::Chars(value)) = self.converted(item, loc, "XML GENERATE", pos)?;
        p.national |= loc.kind == Kind::National;
        if text::legal(&value) {
            return Ok((name, text::escaped(&value), true));
        }
        p.illegal = true;
        let hex = store::bytes(&self.unit.mem, loc).iter().map(|b| format!("{b:02X}")).collect();
        Ok((format!("hex.{name}"), hex, false))
    }

    /// A group's element; `None` when SUPPRESS has left it with no attributes and no content,
    /// unless it is the root, whose start tag also takes `root`'s namespace declaration.
    fn xml_group(&mut self, item: usize, offset: usize, subscripts: &mut Vec<u32>, p: &mut Phrases, pos: Pos, root: Option<&str>) -> R<Option<String>> {
        let mut parts = Parts::default();
        self.xml_members(item, offset, subscripts, p, pos, &mut parts)?;
        if root.is_none() && p.suppressing && parts.attributes.is_empty() && parts.content.is_empty() {
            return Ok(None);
        }
        let (name, prefix) = (self.xml_name(item, p), &p.prefix);
        Ok(Some(format!("<{prefix}{name}{}{}>{}</{prefix}{name}>", root.unwrap_or_default(), parts.attributes, parts.content)))
    }

    /// Each occurrence of each member of a group, in order; an unnamed group's members join its
    /// parent's.
    fn xml_members(&mut self, item: usize, offset: usize, subscripts: &mut Vec<u32>, p: &mut Phrases, pos: Pos, parts: &mut Parts) -> R<()> {
        let layout = self.layout;
        let i = &layout.items[item];
        for &c in &i.children {
            if self.xml_ignored(c) || p.suppressed.contains(&c) {
                continue;
            }
            let child = &layout.items[c];
            let first = offset + (child.offset - i.offset) as usize - self.moved_within(c, item, pos)?;
            let count = if child.table { self.occurrences(c, pos)? } else { 1 };
            for k in 0..count {
                if child.table {
                    subscripts.push(k + 1);
                }
                let at = first + k as usize * child.size as usize;
                let done = if child.name.is_none() { self.xml_members(c, at, subscripts, p, pos, parts) } else { self.xml_member(c, at, subscripts, p, pos, parts) };
                if child.table {
                    subscripts.pop();
                }
                done?;
            }
        }
        Ok(())
    }

    fn xml_member(&mut self, item: usize, offset: usize, subscripts: &mut Vec<u32>, p: &mut Phrases, pos: Pos, parts: &mut Parts) -> R<()> {
        let layout = self.layout;
        let i = &layout.items[item];
        if i.kind == Kind::Group && !i.children.is_empty() {
            if let Some(element) = self.xml_group(item, offset, subscripts, p, pos, None)? {
                parts.content.push_str(&element);
            }
            return Ok(());
        }
        let loc = Loc { offset, len: i.size as usize, kind: i.kind, item };
        let form = self.xml_form(item, p);
        if self.xml_suppressed(item, loc, form, p, pos)? {
            return Ok(());
        }
        let (name, value, legal) = self.xml_leaf(item, loc, p, pos)?;
        let prefix = &p.prefix;
        match form {
            XmlForm::Attribute => parts.attributes.push_str(&format!(" {name}=\"{value}\"")),
            XmlForm::Content if legal => parts.content.push_str(&value),
            _ => parts.content.push_str(&format!("<{prefix}{name}>{value}</{prefix}{name}>")),
        }
        Ok(())
    }

    fn xml_root(&mut self, x: &XmlGenerate, p: &mut Phrases, namespace: &str) -> R<String> {
        let layout = self.layout;
        let from = self.item_of(&x.from)?;
        let loc = self.locate(&x.from)?;
        let mut subscripts = Vec::with_capacity(x.from.subscripts.len());
        for s in &x.from.subscripts {
            subscripts.push(self.integer(s, x.pos)? as u32);
        }
        let i = &layout.items[from];
        if i.kind == Kind::Group && !i.children.is_empty() {
            return Ok(self.xml_group(from, loc.offset, &mut subscripts, p, x.pos, Some(namespace))?.unwrap_or_default());
        }
        let (name, value, _) = self.xml_leaf(from, loc, p, x.pos)?;
        let prefix = &p.prefix;
        Ok(format!("<{prefix}{name}{namespace}>{value}</{prefix}{name}>"))
    }

    fn xml_operand_text(&mut self, op: &Operand, pos: Pos) -> R<String> {
        Ok(match self.operand(op, pos)? {
            Val::Bytes(b) => self.page.decode(&b),
            Val::National(b) => utf16_text(&b),
            _ => return Err(Abend::ironwork("XML GENERATE: a namespace or its prefix is an alphanumeric or national item or literal", pos)),
        })
    }

    /// Writes the document to the receiver and returns XML-CODE.
    fn xml_generated(&mut self, x: &XmlGenerate) -> R<i64> {
        let receiver = self.locate_receiving(&x.receiver)?;
        let national = receiver.kind == Kind::National;
        let ccsid = match &x.encoding {
            Some(op) => match self.operand(op, x.pos)? {
                Val::Num(n) => n.to_i128().and_then(|c| u16::try_from(c).ok()),
                _ => None,
            },
            None if national => Some(UTF16),
            None => Some(self.options.codepage),
        };
        let encoding = match text::encoding(national, ccsid, x.encoding.is_some()) {
            Ok(encoding) => encoding,
            Err(code) => return Ok(code),
        };
        let namespace = match &x.namespace {
            Some(op) => self.xml_operand_text(op, x.pos)?.trim_end_matches(' ').to_owned(),
            None => String::new(),
        };
        if !text::legal(&namespace) {
            return Ok(BAD_NAMESPACE);
        }
        let prefix = match &x.prefix {
            Some(op) if !namespace.is_empty() => self.xml_operand_text(op, x.pos)?.trim_end_matches(' ').to_owned(),
            _ => String::new(),
        };
        if !prefix.is_empty() && !text::valid_prefix(&prefix) {
            return Ok(BAD_PREFIX);
        }
        let declaration = match (namespace.is_empty(), prefix.is_empty()) {
            (true, _) => String::new(),
            (false, true) => format!(" xmlns=\"{}\"", text::escaped(&namespace)),
            (false, false) => format!(" xmlns:{prefix}=\"{}\"", text::escaped(&namespace)),
        };
        let mut p = self.xml_phrases(x)?;
        if !prefix.is_empty() {
            p.prefix = format!("{prefix}:");
        }
        let mut document = String::new();
        if x.declaration {
            document = format!("<?xml version=\"1.0\" encoding=\"{}\"?>", text::encoding_name(ccsid.unwrap_or(UTF16)));
        }
        document.push_str(&self.xml_root(x, &mut p, &declaration)?);
        if p.national && matches!(encoding, Encoding::Page(_)) {
            return Ok(NATIONAL_NOT_UTF8);
        }
        let substituted = matches!(encoding, Encoding::Page(page) if document.chars().any(|c| page.encode_char(c).is_none()));
        let (bytes, unit) = (encoding.encode(&document), if national { 2 } else { 1 });
        let fits = bytes.len() <= receiver.len;
        let written = rt::json::write_document(&mut self.unit.mem, receiver, &bytes, unit);
        if let Some(count) = &x.count {
            self.set_integer(count, (written / unit) as i64, x.pos)?;
        }
        Ok(if !fits {
            RECEIVER_TOO_SMALL
        } else if p.illegal {
            ILLEGAL_CHARACTERS
        } else if substituted {
            SUBSTITUTED
        } else {
            0
        })
    }

    pub(in crate::machine) fn xml_generate(&mut self, x: &'p XmlGenerate) -> R<Flow> {
        self.unit.unfollowed("XML GENERATE");
        let code = self.xml_generated(x)?;
        self.set_integer(&special("XML-CODE"), code, x.pos)?;
        let handler = if code == 0 { &x.not_on_exception } else { &x.on_exception };
        match handler {
            Some(body) => self.run_block(body),
            None => Ok(Flow::Next),
        }
    }
}
