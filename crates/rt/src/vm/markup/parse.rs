//! JSON PARSE and XML PARSE: a document into the items its names reach, and a document's events
//! through the processing procedure.

use super::super::flow::{Arrival, Exit};
use super::super::value::constant;
use super::{Code, R, Vm, ccsid_of, node_loc, not_yet, slot};
use crate::abend::{Abend, AbendCode};
use crate::intrinsic::numval::Number;
use crate::json::parse::{self as json, Invalid, Value, fixed_value};
use crate::json::parse::{ANONYMOUS_ARRAY, DIFFERENT_DUPLICATE, INCOMPATIBLE, NO_MATCH, PARSE_ENCODING, UNCONVERTED_BOOLEAN};
use crate::json::parse::{LONG_ARRAY, LOST, NULL_ELEMENT, NULL_ITEM, SAME_DUPLICATE, SHORT_ARRAY, SIZE_ERROR, SUB, SUBSTITUTED, UNMATCHED_ITEM, UNMATCHED_NAME};
use crate::lir::{Ccsid, DebugId, Flag, JsonParse, Named, NumberInto, ParseLeaf, ParseValue, SetTo, Step, XmlParse, XmlRegister};
use crate::storage::{Kind, Loc, Val};
use crate::store::{self, ProgramFacts};
use crate::unit::Loader;
use crate::vocab::Pos;
use crate::xml::{Encoding, Event, EventKind, Scanner, Step as Scan, UTF8};
use std::collections::HashMap;
use std::rc::Rc;
use zarch::ebcdic::CodePage;

/// An exception ends the parse with its JSON-CODE.
type Outcome = Result<(), i64>;

/// What parsing has met: JSON-STATUS's conditions, whether any value reached an item, and each
/// value taken so far, by node and offset.
#[derive(Default)]
struct Progress<'v> {
    status: i64,
    matched: bool,
    taken: HashMap<(usize, usize), &'v Value>,
}

/// How the parse goes on from a value it took: the statement and its position.
#[derive(Clone, Copy)]
struct At<'p> {
    j: &'p JsonParse,
    debug: DebugId,
    pos: Pos,
}

fn national(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_be_bytes).collect()
}

impl<'p, L: Loader<Rc<Code>>> Vm<'p, '_, '_, L> {
    /// JSON-CODE and JSON-STATUS.
    pub(super) fn json_parse(&mut self, j: &'p JsonParse, debug: DebugId, pos: Pos) -> R<(i64, i64)> {
        let source = self.loc(j.source)?;
        let bytes = store::bytes(&self.unit.mem, source).to_vec();
        let national = source.kind == Kind::National;
        let ccsid = match j.encoding {
            Ccsid::Unnamed => None,
            Ccsid::CodePage if national => return Ok((PARSE_ENCODING, 0)),
            Ccsid::CodePage => Some(self.p.options.options.codepage),
            Ccsid::Operand(o) => Some(ccsid_of(self.value(o)?).unwrap_or(0)),
        };
        let text = match json::text(bytes, national, ccsid) {
            Ok(text) => text,
            Err(code) => return Ok((code, 0)),
        };
        let document = match json::parse(&text) {
            Ok(document @ (Value::Object(_) | Value::Array(_))) => document,
            Ok(_) => return Ok((Invalid::Malformed.code(), 0)),
            Err(invalid) => return Ok((invalid.code(), 0)),
        };
        let into = self.loc(j.into)?.offset;
        let walk = self.walk_from(&j.subscripts, pos)?;
        let at = At { j, debug, pos };
        let mut g = Progress::default();
        let code = if j.nodes[0].name == Named::Omitted {
            self.parse_root(at, into, &walk, &document, &mut g)?
        } else {
            let Value::Object(pairs) = &document else { return Ok((ANONYMOUS_ARRAY, 0)) };
            let mut code = Ok(());
            for (name, value) in pairs {
                if !self.name_matches(j.nodes[0].name, name) {
                    g.status |= UNMATCHED_NAME;
                    continue;
                }
                code = self.parse_root(at, into, &walk, value, &mut g)?;
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

    /// A NAME phrase's literal matches exactly; a data-name matches whatever the case of its letters.
    fn name_matches(&self, named: Named, name: &str) -> bool {
        match named {
            Named::Exactly(literal) => self.sym(literal) == name,
            Named::Folded(data_name) => self.sym(data_name).eq_ignore_ascii_case(name),
            Named::Omitted => false,
        }
    }

    /// A value into the statement's own item: one occurrence, or a whole table.
    fn parse_root<'v>(&mut self, at: At<'p>, offset: usize, walk: &[u32], value: &'v Value, g: &mut Progress<'v>) -> R<Outcome> {
        self.markup.walk = walk.to_vec();
        if at.j.nodes[0].occurs.is_some() { self.parse_member(at, 0, offset, value, g) } else { self.parse_value(at, 0, offset, value, false, g) }
    }

    /// One JSON value into one occurrence of node `k`; `element` when the value is an array's.
    fn parse_value<'v>(&mut self, at: At<'p>, k: usize, offset: usize, value: &'v Value, element: bool, g: &mut Progress<'v>) -> R<Outcome> {
        if let Some(&earlier) = g.taken.get(&(k, offset)) {
            if earlier != value {
                return Ok(Err(DIFFERENT_DUPLICATE));
            }
            g.status |= SAME_DUPLICATE;
            return Ok(Ok(()));
        }
        g.taken.insert((k, offset), value);
        let node = &at.j.nodes[k];
        let null = *value == Value::Null;
        if let Some(indicator) = &node.indicator {
            let target = match indicator.place {
                Some(Ok(place)) => Some(self.loc(place)?),
                Some(Err(abend)) => return Err(self.abend(abend, Some(at.debug)).into()),
                None => None,
            };
            self.parse_flag(&indicator.flag, target, null, at)?;
        }
        if null {
            g.matched = true;
            if node.indicator.is_some() {
                return Ok(Ok(()));
            }
            if let Some((f, plan)) = &node.null {
                self.move_into(node_loc(offset, node.len, node.kind), Val::Fig(*f), plan, at.debug, at.pos)?;
            } else if !(at.j.ignore_all || node.ignored) {
                g.status |= if element { NULL_ELEMENT } else { NULL_ITEM };
            }
            return Ok(Ok(()));
        }
        match &node.value {
            ParseValue::Object { members } => {
                let Value::Object(pairs) = value else { return Ok(Err(INCOMPATIBLE)) };
                self.parse_object(at, members, offset, pairs, g)
            }
            ParseValue::Leaf(leaf) => {
                g.matched = true;
                self.parse_elementary(at, leaf, node_loc(offset, node.len, node.kind), value, g)
            }
            ParseValue::Suppressed => Err(not_yet("a value into an item JSON PARSE SUPPRESS names")),
        }
    }

    /// Each pair into the first member its name matches.
    fn parse_object<'v>(&mut self, at: At<'p>, members: &'p [u32], offset: usize, pairs: &'v [(String, Value)], g: &mut Progress<'v>) -> R<Outcome> {
        let nodes = &at.j.nodes;
        let mut reached = vec![false; members.len()];
        for (name, value) in pairs {
            let Some(i) = members.iter().position(|&m| self.name_matches(nodes[m as usize].name, name)) else {
                g.status |= UNMATCHED_NAME;
                continue;
            };
            reached[i] = true;
            let m = members[i] as usize;
            if nodes[m].value == ParseValue::Suppressed {
                continue;
            }
            if let Err(code) = self.parse_member(at, m, offset + nodes[m].offset as usize, value, g)? {
                return Ok(Err(code));
            }
        }
        if members.iter().zip(&reached).any(|(&m, &r)| !r && nodes[m as usize].value != ParseValue::Suppressed) {
            g.status |= UNMATCHED_ITEM;
        }
        Ok(Ok(()))
    }

    /// A value into member `k`: a table's occurrences take an array's elements in turn.
    fn parse_member<'v>(&mut self, at: At<'p>, k: usize, offset: usize, value: &'v Value, g: &mut Progress<'v>) -> R<Outcome> {
        let node = &at.j.nodes[k];
        let Some(occurs) = &node.occurs else { return self.parse_value(at, k, offset, value, false, g) };
        let elements = match value {
            Value::Array(elements) => elements,
            Value::Null => {
                g.matched = true;
                if !(at.j.ignore_all || node.ignored) {
                    g.status |= NULL_ITEM;
                }
                return Ok(Ok(()));
            }
            _ => return Ok(Err(INCOMPATIBLE)),
        };
        let count = self.count(occurs, at.pos)? as usize;
        if elements.len() < count {
            g.status |= SHORT_ARRAY;
        }
        if elements.len() > count {
            g.status |= LONG_ARRAY;
        }
        for (i, e) in elements.iter().take(count).enumerate() {
            self.markup.walk.push(i as u32 + 1);
            let code = self.parse_value(at, k, offset + i * node.len as usize, e, true, g);
            self.markup.walk.pop();
            if let Err(code) = code? {
                return Ok(Err(code));
            }
        }
        Ok(Ok(()))
    }

    /// Sets a USING phrase's first value, `on`, or its second; two literals go in `target`.
    fn parse_flag(&mut self, flag: &Flag, target: Option<Loc>, on: bool, at: At<'p>) -> R<()> {
        match flag {
            Flag::Set { on: yes, off: no } => match if on { yes } else { no } {
                SetTo::Nothing => Ok(()),
                SetTo::Move { place, value, plan } => {
                    let dest = self.loc(*place)?;
                    let val = constant(&self.p.consts[*value as usize]);
                    self.move_into(dest, val, plan, at.debug, at.pos)
                }
                SetTo::Refused(abend) => Err(self.abend(*abend, Some(at.debug)).into()),
            },
            Flag::Literals { on: yes, off: no } => {
                let (value, plan) = if on { yes } else { no };
                let val = constant(&self.p.consts[*value as usize]);
                let Some(target) = target else { return Err(Abend::ironwork("JSON PARSE: two literals need the item they go in", at.pos).into()) };
                self.move_into(target, val, plan, at.debug, at.pos)
            }
        }
    }

    fn parse_elementary(&mut self, at: At<'p>, leaf: &'p ParseLeaf, loc: Loc, value: &Value, g: &mut Progress) -> R<Outcome> {
        match value {
            Value::Bool(truth) => {
                let Some(flag) = &leaf.boolean else { return Ok(Err(UNCONVERTED_BOOLEAN)) };
                self.parse_flag(flag, Some(loc), *truth, at)?;
                Ok(Ok(()))
            }
            Value::String(s) => self.parse_string(at, leaf, loc, s, g),
            Value::Number(_) if leaf.number == NumberInto::Digits && matches!(loc.kind, Kind::Alnum { .. }) => {
                Err(not_yet("a JSON number into an alphanumeric item, which the walker refuses when its PICTURE is alphabetic"))
            }
            Value::Number(n) => {
                let (negative, int, frac) = json::decimal(n);
                let integer = !n.contains(['.', 'e', 'E']);
                self.parse_number(at, leaf, loc, (negative, &int, &frac), integer, g)
            }
            Value::Object(_) | Value::Array(_) | Value::Null => Ok(Err(INCOMPATIBLE)),
        }
    }

    /// A string into a receiver: its characters in the program's code page, or UTF-16, or, for a
    /// numeric receiver, the number it spells.
    fn parse_string(&mut self, at: At<'p>, leaf: &'p ParseLeaf, loc: Loc, s: &str, g: &mut Progress) -> R<Outcome> {
        let text = || leaf.text.as_ref().ok_or_else(|| not_yet("a JSON string into a character item with no MOVE plan"));
        match loc.kind {
            Kind::Alnum { .. } | Kind::AlnumEdited { .. } => {
                let page = self.facts().page();
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
                self.move_into(loc, Val::Bytes(bytes), text()?, at.debug, at.pos)?;
                Ok(Ok(()))
            }
            Kind::National => {
                let units: Vec<u16> = s.encode_utf16().collect();
                if units.get(loc.len / 2..).is_some_and(|lost| lost.iter().any(|&u| u != 0x20)) {
                    g.status |= LOST;
                }
                self.move_into(loc, Val::National(units.iter().flat_map(|u| u.to_be_bytes()).collect()), text()?, at.debug, at.pos)?;
                Ok(Ok(()))
            }
            _ => match json::numeric_string(s, self.facts().decimal_point()) {
                Some((negative, int, frac)) => self.parse_number(at, leaf, loc, (negative, &int, &frac), true, g),
                None => Ok(Err(INCOMPATIBLE)),
            },
        }
    }

    /// A number into a receiver, by MOVE; an alphanumeric or national receiver takes only a number
    /// written as an integer.
    fn parse_number(&mut self, at: At<'p>, leaf: &'p ParseLeaf, loc: Loc, (negative, int, frac): (bool, &str, &str), integer: bool, g: &mut Progress) -> R<Outcome> {
        match &leaf.number {
            NumberInto::Float(plan) => {
                let Kind::Float(precision) = loc.kind else { return Err(not_yet("a JSON number into a floating-point plan for another item")) };
                let digits = format!("{int}{frac}");
                let kept = &digits[..digits.len().min(38)];
                let number = Number { negative, digits: kept.parse().unwrap_or(0), decimals: frac.len() as u32, exponent: (digits.len() - kept.len()) as i32 };
                let value = number.to_real().to_hfp(precision).map_err(|c| Abend::check(c, at.pos))?;
                self.move_into(loc, Val::Float(value), plan, at.debug, at.pos)?;
            }
            NumberInto::Store(plan) => {
                let (Kind::Zoned { scale, .. } | Kind::Packed { scale, .. } | Kind::Binary { scale, .. }) = loc.kind else {
                    return Err(not_yet("a JSON number into a fixed-point plan for another item"));
                };
                let (fixed, cut) = fixed_value(negative, int, &frac[..frac.len().min(scale as usize)]);
                let facts = self.receiving(Some(plan));
                if store::store_fixed_checked(&facts, self.unit, loc, &fixed, false, false, at.pos)? || cut {
                    g.status |= SIZE_ERROR;
                }
            }
            NumberInto::Edited(plan) => self.move_into(loc, Val::Num(fixed_value(negative, int, frac).0), plan, at.debug, at.pos)?,
            NumberInto::Digits if integer => {
                let width = if loc.kind == Kind::National { loc.len / 2 } else { loc.len };
                if int.len() > width {
                    g.status |= LOST;
                }
                let digits = if int.is_empty() { "0" } else { int };
                let value = if loc.kind == Kind::National { Val::National(national(digits)) } else { Val::Num(fixed_value(false, digits, "").0) };
                store::assign(&self.facts(), self.unit, loc, value, None, at.pos)?;
            }
            NumberInto::Digits | NumberInto::Incompatible => return Ok(Err(INCOMPATIBLE)),
        }
        Ok(Ok(()))
    }

    /// XML-CODE as the parse ends it, or the transfer by which the processing procedure left.
    pub(super) fn xml_parse(&mut self, x: &'p XmlParse, pos: Pos) -> R<Result<i64, Step>> {
        let document = self.loc(x.document)?;
        let ccsid = match x.encoding {
            Some(o) => ccsid_of(self.value(o)?),
            None => None,
        };
        let encoding = match (document.kind, ccsid) {
            (Kind::National, _) => Encoding::National,
            (_, Some(UTF8)) => Encoding::Utf8,
            (_, Some(c)) => match CodePage::by_ccsid(c) {
                Some(page) => Encoding::Page(page),
                None => return Err(Abend::ironwork(format!("XML PARSE: CCSID {c} is not a code page ironwork for COBOL carries"), pos).into()),
            },
            (_, None) => Encoding::Page(self.facts().page()),
        };
        let national_out = matches!(encoding, Encoding::National) || x.national;
        let representable: Box<dyn Fn(char) -> bool> = match encoding {
            Encoding::Page(page) if !national_out => Box::new(move |c| page.encode_char(c).is_some()),
            _ => Box::new(|_| true),
        };
        let mut carry = Vec::new();
        let text = self.xml_segment(x, encoding, &mut carry)?;
        let mut seen = text.clone();
        // An EXCEPTION's XML-TEXT is the current segment up to the error.
        let mut segment_start = 0;
        let mut scanner = Scanner::new(&text, &*representable);
        let mark = self.unit.mem.len();
        let code = loop {
            let (event, exception, warning) = match scanner.advance() {
                Scan::Event(e) if e.kind == EventKind::Exception => {
                    let code = i64::from(e.code);
                    (e, Some(code), true)
                }
                Scan::Event(e) => (e, None, false),
                Scan::EndOfInput => (Event { kind: EventKind::EndOfInput, text: String::new(), namespace: String::new(), prefix: String::new(), information: 0, code: 0 }, None, false),
                Scan::Error(m) => {
                    let upto: String = seen.chars().take(m.offset).skip(segment_start).collect();
                    let code = m.why.code();
                    (Event { kind: EventKind::Exception, text: upto, namespace: String::new(), prefix: String::new(), information: 0, code }, Some(i64::from(code)), false)
                }
                Scan::Done => break 0,
            };
            let ran = self.xml_event(x, &event, exception.unwrap_or(0), encoding, national_out, pos);
            self.markup.xml = Default::default();
            self.unit.release_temporaries(mark);
            match ran? {
                Exit::Completed => {}
                Exit::Left(step) => return Ok(Err(step)),
                Exit::End(ending) => return Ok(Err(Step::End(ending))),
            }
            let code = self.int(&x.code_value, pos)?;
            match event.kind {
                EventKind::Exception if warning && code == 0 => {}
                EventKind::Exception => break exception.unwrap_or(code),
                _ if code == -1 => break -1,
                EventKind::EndOfInput if code == 1 => {
                    let segment = self.xml_segment(x, encoding, &mut carry)?;
                    segment_start = seen.chars().count();
                    seen.push_str(&segment);
                    scanner.feed(&segment);
                }
                EventKind::EndOfInput if code == 0 => {
                    if !carry.is_empty() {
                        let rest = encoding.decode(&std::mem::take(&mut carry));
                        seen.push_str(&rest);
                        scanner.feed(&rest);
                    }
                    scanner.finish();
                }
                EventKind::EndOfDocument if code == 0 => break 0,
                _ if code == 0 => {}
                kind => {
                    let message = format!("IGZ0230S XML PARSE: the processing procedure set XML-CODE to {code} at event {}", kind.name());
                    return Err(Abend { code: AbendCode::user(4038), message, pos, file: None }.into());
                }
            }
        };
        Ok(Ok(code))
    }

    /// The document located again and decoded as the next segment.
    fn xml_segment(&mut self, x: &XmlParse, encoding: Encoding, carry: &mut Vec<u8>) -> R<String> {
        let loc = self.loc(x.document)?;
        Ok(encoding.decode_segment(carry, store::bytes(&self.unit.mem, loc)))
    }

    fn fragment(&mut self, bytes: &[u8]) -> (usize, usize) {
        if bytes.is_empty() {
            return (0, 0);
        }
        (self.unit.push_temporary(bytes), bytes.len())
    }

    /// Sets the registers for one event and runs the processing procedure for it.
    fn xml_event(&mut self, x: &XmlParse, e: &Event, code: i64, encoding: Encoding, national_out: bool, pos: Pos) -> R<Exit> {
        let name = self.facts().page().encode_lossy(&format!("{:<30}", e.kind.name()));
        let loc = self.loc(x.event)?;
        store::write(&mut self.unit.mem, loc, &name[..30]);
        self.set_register(x.code.0, code, pos)?;
        self.set_register(x.information.0, e.information.into(), pos)?;
        let national_character = matches!(e.kind, EventKind::ContentNationalCharacter | EventKind::AttributeNationalCharacter);
        let mut registers = [(0, 0); 6];
        if national_out || national_character {
            registers[slot(XmlRegister::NText)] = self.fragment(&national(&e.text));
            registers[slot(XmlRegister::NNamespace)] = self.fragment(&national(&e.namespace));
            registers[slot(XmlRegister::NPrefix)] = self.fragment(&national(&e.prefix));
        } else {
            registers[slot(XmlRegister::Text)] = self.fragment(&encoding.encode(&e.text));
            registers[slot(XmlRegister::Namespace)] = self.fragment(&encoding.encode(&e.namespace));
            registers[slot(XmlRegister::Prefix)] = self.fragment(&encoding.encode(&e.prefix));
        }
        self.markup.xml = registers;
        self.run_procedure(x.procedure, Arrival::Perform)
    }
}
