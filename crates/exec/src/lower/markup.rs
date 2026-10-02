//! JSON GENERATE, JSON PARSE, XML GENERATE and XML PARSE (lir.md §9.13), as machine/json.rs,
//! machine/json/parse.rs, machine/xml.rs and machine/xml/generate.rs run them: the phrases resolved
//! and the tree each walk reaches laid out once, the XML special registers as places, and XML
//! PARSE's processing procedure as a range.

use super::data::{Side, Value};
use super::flow::Ctx;
use super::{Lower, LowerError, R, push, unsupported};
use crate::layout::Resolved;
use numeric::Trunc;
use rt::abend::AbendCode;
use rt::lir::{self, AbendId, Ccsid, Convert, Count, IntExpr, Markup, MarkupId, Named, NumberInto, Op, PlaceId, RangeId, RangeKind, SetTo, StorePlan, Terminator, XmlForm, XmlRegister};
use rt::storage::Kind;
use std::collections::{HashMap, HashSet};
use syntax::Pos;
use syntax::ast::{
    self, Encoding, Expr, Figurative, Flag, JsonConversion, JsonGenerate, JsonParse, Literal, Marker, NullIndicator, Operand, ParseConversion, Ref, Stmt, Suppression, XmlGenerate,
    XmlParse,
};

/// Lowering fails, or the walker abends IRONWORK with this message, at this position.
enum Fail {
    Lower(LowerError),
    Abend(String, Pos),
}

impl From<LowerError> for Fail {
    fn from(e: LowerError) -> Self {
        Fail::Lower(e)
    }
}

type M<T> = Result<T, Fail>;

/// What JSON GENERATE's phrases say about each item, by item index (`json_phrases`).
#[derive(Default)]
struct JsonPhrases<'g> {
    names: HashMap<usize, Option<String>>,
    suppressed: HashSet<usize>,
    suppressed_when: HashMap<usize, &'g [Figurative]>,
    every: Vec<(Option<bool>, &'g [Figurative])>,
    boolean: HashMap<usize, &'g Marker>,
    null_when: HashMap<usize, Figurative>,
    indicated: HashMap<usize, &'g NullIndicator>,
    indicators: HashSet<usize>,
}

/// What JSON PARSE's phrases say about each item (`parse_phrases`).
#[derive(Default)]
struct ParsePhrases<'j> {
    names: HashMap<usize, Option<String>>,
    suppressed: HashSet<usize>,
    ignored: HashSet<usize>,
    booleans: HashMap<usize, &'j Flag>,
    null_to: HashMap<usize, Figurative>,
    indicated: HashMap<usize, (&'j Flag, Option<&'j Ref>)>,
    indicators: HashSet<usize>,
}

/// What XML GENERATE's phrases say about each item (`xml_phrases`).
#[derive(Default)]
struct XmlPhrases<'x> {
    names: HashMap<usize, String>,
    forms: HashMap<usize, ast::XmlForm>,
    suppressed: HashSet<usize>,
    suppressed_when: HashMap<usize, &'x [Figurative]>,
    every: Vec<(Option<bool>, Option<ast::XmlForm>, &'x [Figurative])>,
    attributes: bool,
}

fn refusal(message: impl Into<String>, at: Pos) -> Fail {
    Fail::Abend(message.into(), at)
}

fn special(name: &str, pos: Pos) -> Ref {
    Ref { name: name.into(), qualifiers: Vec::new(), subscripts: Vec::new(), refmod: None, pos }
}

/// Whether EVERY [NUMERIC | NONNUMERIC] WHEN selects an item of `kind` for `f` (`every_selects`).
fn every_selects(kind: Kind, numeric: Option<bool>, f: Figurative) -> bool {
    if numeric.is_some_and(|n| n != kind.is_numeric()) {
        return false;
    }
    f == Figurative::Zero || matches!(kind, Kind::Alnum { .. } | Kind::AlnumEdited { .. } | Kind::NumericEdited { .. } | Kind::National | Kind::Zoned { scale: 0, .. })
}

fn form(f: ast::XmlForm) -> XmlForm {
    match f {
        ast::XmlForm::Attribute => XmlForm::Attribute,
        ast::XmlForm::Element => XmlForm::Element,
        ast::XmlForm::Content => XmlForm::Content,
    }
}

impl Lower<'_> {
    /// XML-TEXT and the other fragment registers the program does not declare itself.
    pub(super) fn xml_register(&mut self, r: &Ref) -> R<lir::Place> {
        let register = match r.name.as_str() {
            "XML-TEXT" => XmlRegister::Text,
            "XML-NTEXT" => XmlRegister::NText,
            "XML-NAMESPACE" => XmlRegister::Namespace,
            "XML-NNAMESPACE" => XmlRegister::NNamespace,
            "XML-NAMESPACE-PREFIX" => XmlRegister::Prefix,
            _ => XmlRegister::NPrefix,
        };
        let kind = if register.national() { Kind::National } else { Kind::Alnum { justified: false } };
        let refmod = match &r.refmod {
            None => None,
            Some(rm) => {
                let start = self.int_expr(&rm.start, r.pos)?;
                let length = rm.length.as_ref().map(|l| self.int_expr(l, r.pos)).transpose()?;
                Some(lir::RefMod { start, length, check: self.c.ssrange })
            }
        };
        Ok(lir::Place { base: lir::Base::Xml(register), offset: 0, len: 0, kind, scaling: 0, subscripts: Vec::new(), odo: None, refmod, name: self.sym(&r.name), at: self.at(r.pos), numcheck: Default::default() })
    }

    /// The range XML PARSE's PROCESSING PROCEDURE names, as the walker's `procedure` finds it.
    pub(super) fn processing(&mut self, x: &XmlParse) -> R<RangeId> {
        let program = self.program;
        let Ok((first, first_end)) = crate::procedure(program, &x.procedure) else { return unsupported("a PROCESSING PROCEDURE the walker cannot find", x.pos) };
        let last = match &x.thru {
            None => first_end,
            Some(t) => match crate::procedure(program, t) {
                Ok((_, last)) => last,
                Err(_) => return unsupported("a PROCESSING PROCEDURE the walker cannot find", x.pos),
            },
        };
        self.span_range((first, last), RangeKind::Processing)
    }

    /// The statement's op and its exception phrases, or the abend the walker gives on reaching it.
    pub(super) fn markup(&mut self, s: &Stmt, pos: Pos, ctx: &Ctx) -> R<()> {
        let (lowered, on, not_on) = match s {
            Stmt::JsonGenerate(g) => (self.json_generate(g), &g.on_exception, &g.not_on_exception),
            Stmt::XmlGenerate(x) => (self.xml_generate(x), &x.on_exception, &x.not_on_exception),
            Stmt::XmlParse(x) => (self.xml_parse(x).map_err(Fail::from), &x.on_exception, &x.not_on_exception),
            Stmt::JsonParse(j) => (self.json_parse(j), &j.on_exception, &j.not_on_exception),
            _ => return Err(LowerError::Invalid("a statement that is not JSON or XML lowered as one".into())),
        };
        let markup = match lowered {
            Ok(m) => m,
            Err(Fail::Lower(e)) => return Err(e),
            Err(Fail::Abend(message, at)) => {
                let abend = self.ironwork(&message)?;
                return self.end(Terminator::Abend(abend), at);
            }
        };
        let id: MarkupId = push(&mut self.services.markup, markup, "JSON and XML statements")?;
        self.op(Op::Markup(id), pos)?;
        self.phrases(on.as_deref(), not_on.as_deref(), pos, ctx)
    }

    /// A payload's abend for what the walker refuses inside the statement, at its own position.
    fn refused(&mut self, fail: Fail) -> R<AbendId> {
        match fail {
            Fail::Abend(message, at) => self.abend(AbendCode::Ironwork, &message, Some(at)),
            Fail::Lower(e) => Err(e),
        }
    }

    /// `item_of`: a data item, or the walker's abend for a condition-name or a name it cannot find.
    fn item_of(&self, r: &Ref) -> M<usize> {
        match self.layout.resolve(&r.name, &r.qualifiers, r.pos) {
            Ok(Resolved::Item(i)) => Ok(i),
            Ok(Resolved::Condition(_)) => Err(refusal(format!("{} is a condition-name, not a data item", r.name), r.pos)),
            Err(e) => Err(refusal(e.message, r.pos)),
        }
    }

    /// `variable_of`: a condition-name's index and its conditional variable.
    fn variable_of(&self, c: &Ref) -> M<(usize, usize)> {
        match self.layout.resolve(&c.name, &c.qualifiers, c.pos) {
            Ok(Resolved::Condition(k)) => Ok((k, self.layout.conditions[k].item)),
            Ok(Resolved::Item(_)) => Err(refusal(format!("{} is not a condition-name", c.name), c.pos)),
            Err(e) => Err(refusal(e.message, c.pos)),
        }
    }

    /// Each data entry whose name the source spells in mixed case, by its position (`spellings`).
    fn spelled(&self, item: usize) -> Option<String> {
        let program = self.program;
        let at = self.layout.items[item].pos;
        let mut entries = program.working_storage.iter().chain(&program.local_storage).chain(&program.linkage).chain(program.files.iter().flat_map(|f| &f.records));
        entries.find_map(|e| e.spelled.clone().filter(|_| e.pos == at))
    }

    fn register(&mut self, name: &str, pos: Pos) -> R<(PlaceId, StorePlan)> {
        let place = self.place(&special(name, pos), false)?;
        let store = self.store_plan(self.kind_of(place), self.place_items[place as usize])?;
        Ok((place, store))
    }

    fn counted(&mut self, r: &Ref) -> R<(PlaceId, StorePlan)> {
        let place = self.place(r, false)?;
        let store = self.store_plan(self.kind_of(place), self.place_items[place as usize])?;
        Ok((place, store))
    }

    /// `occurrences`: a table's declared count, or its OCCURS DEPENDING ON object's.
    pub(super) fn occurs(&mut self, item: usize, pos: Pos) -> R<Count> {
        let i = &self.layout.items[item];
        let Some(object) = &i.depending_on else { return Ok(Count::Fixed(i.occurs)) };
        let (max, element) = (i.occurs, i.size);
        let object = self.int_expr(&Expr::Operand(Operand::Ref(object.clone())), pos)?;
        Ok(Count::Odo(lir::Odo { object, max, element, check: self.c.ssrange }))
    }

    /// `converted`: how a value of `kind` in item `item` is written.
    fn convert(&mut self, item: usize, kind: Kind, statement: &str, pos: Pos) -> M<Convert> {
        let i = &self.layout.items[item];
        if i.scaling > 0 {
            return Err(Fail::Lower(LowerError::Unsupported("JSON or XML GENERATE of an item with PICTURE scaling positions", pos)));
        }
        let binary_integers = |digits: u32, scale: u32, native: bool| {
            if native || self.c.options.trunc == Trunc::Bin {
                let whole = match digits {
                    0..=4 => 5,
                    5..=9 => 10,
                    _ => 20,
                };
                whole - scale.min(whole)
            } else {
                digits.saturating_sub(scale)
            }
        };
        Ok(match kind {
            Kind::Alnum { justified } => Convert::Chars { justified },
            Kind::AlnumEdited { .. } | Kind::NumericEdited { .. } | Kind::Group => Convert::Chars { justified: false },
            Kind::National => Convert::National,
            Kind::Float(precision) => Convert::Float(precision),
            Kind::Zoned { digits, scale, .. } | Kind::Packed { digits, scale, .. } => Convert::Fixed { integers: digits.saturating_sub(scale) },
            Kind::Binary { digits, scale, native, .. } => Convert::Fixed { integers: binary_integers(digits, scale, native) },
            Kind::Index => Convert::Fixed { integers: 10 },
            Kind::Pointer | Kind::ObjectReference | Kind::ProgramPointer => {
                let name = i.name.as_deref().unwrap_or("FILLER");
                Convert::Refused(self.ironwork(&format!("{statement}: {name} is a pointer or object reference"))?)
            }
        })
    }

    /// The item a statement names, and its subscripts as the walk takes them.
    fn root(&mut self, r: &Ref, whole: bool, pos: Pos) -> R<(PlaceId, Vec<IntExpr>)> {
        let located = if whole {
            let first = Expr::Operand(Operand::Literal(Literal::Number("1".into())));
            Ref { subscripts: r.subscripts.iter().cloned().chain([first]).collect(), ..r.clone() }
        } else {
            r.clone()
        };
        let from = self.place(&located, false)?;
        let subscripts = r.subscripts.iter().map(|s| self.int_expr(s, pos)).collect::<R<_>>()?;
        Ok((from, subscripts))
    }

    /// Item `item` located with the first of the walk's `depth` subscripts, as the walker's
    /// `locate_item` locates `named` with them.
    fn walked(&mut self, item: usize, named: &Ref, depth: usize) -> M<PlaceId> {
        if named.refmod.is_some() {
            return Err(Fail::Lower(LowerError::Unsupported("a JSON phrase naming a reference-modified item", named.pos)));
        }
        let dims = self.layout.items[item].dims.len();
        if depth < dims {
            return Err(refusal(format!("{} takes {dims} subscripts, not {depth}", named.name), named.pos));
        }
        let one = Expr::Operand(Operand::Literal(Literal::Number("1".into())));
        let place = self.item_place(item, &Ref { subscripts: vec![one; dims], ..named.clone() }, false)?;
        for (k, s) in self.places[place as usize].subscripts.iter_mut().enumerate() {
            s.value = IntExpr::Walk(u8::try_from(k).map_err(|_| LowerError::Exceeds("subscripts of a JSON walk", named.pos))?);
        }
        Ok(place)
    }

    fn walked_or_refused(&mut self, item: usize, named: &Ref, depth: usize) -> R<Result<PlaceId, AbendId>> {
        match self.walked(item, named, depth) {
            Ok(place) => Ok(Ok(place)),
            Err(fail) => self.refused(fail).map(Err),
        }
    }

    /// `marker_holds` of a CONVERTING or INDICATING phrase, at the walk's `depth`.
    fn marker(&mut self, marker: &Marker, depth: usize, pos: Pos) -> R<lir::Marker> {
        Ok(match marker {
            Marker::Literal(Literal::Alnum(s)) => lir::Marker::Byte(self.page.encode(s).ok().and_then(|b| b.first().copied())),
            Marker::Literal(_) => lir::Marker::Refused(self.ironwork("a one-character alphanumeric literal")?),
            Marker::Condition(c) => {
                let walked = self.variable_of(c).and_then(|(index, variable)| Ok((index, self.walked(variable, c, depth)?)));
                match walked {
                    Ok((index, subject)) => {
                        let test = self.condition_values(index, subject, pos)?;
                        lir::Marker::Condition(self.fold(&test)?)
                    }
                    Err(fail) => lir::Marker::Refused(self.refused(fail)?),
                }
            }
        })
    }

    fn json_phrases<'g>(&mut self, g: &'g JsonGenerate) -> M<JsonPhrases<'g>> {
        let mut p = JsonPhrases::default();
        for (r, name) in &g.names {
            let item = self.item_of(r)?;
            let name = match name {
                None => None,
                Some(Literal::Alnum(s) | Literal::National(s)) => Some(s.clone()),
                Some(_) => return Err(refusal("JSON GENERATE NAME takes an alphanumeric or national literal", g.pos)),
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
                Suppression::Every { numeric, when, .. } => p.every.push((*numeric, when)),
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
                (None, Marker::Condition(c)) => self.variable_of(c)?.1,
                (None, Marker::Literal(_)) => return Err(refusal("INDICATING ... USING a literal takes IN and the indicator", g.pos)),
            };
            p.indicators.insert(indicator);
        }
        Ok(p)
    }

    fn json_ignored(&self, item: usize, p: &JsonPhrases) -> bool {
        let i = &self.layout.items[item];
        if i.redefines.is_some() || i.level == 66 || p.indicators.contains(&item) {
            return true;
        }
        if i.children.is_empty() || i.kind != Kind::Group {
            return i.name.is_none();
        }
        i.children.iter().all(|&c| self.json_ignored(c, p))
    }

    fn json_name(&self, item: usize, p: &JsonPhrases) -> Option<String> {
        if let Some(name) = p.names.get(&item) {
            return name.clone();
        }
        Some(self.spelled(item).or_else(|| self.layout.items[item].name.clone()).unwrap_or_default())
    }

    fn json_generate(&mut self, g: &JsonGenerate) -> M<Markup> {
        // Names every phrase, so one the parser gains is lowered or refused before it compiles.
        let JsonGenerate { receiver: _, from: _, count: _, names: _, suppress: _, converting: _, indicating: _, encoding: _, on_exception: _, not_on_exception: _, pos: _ } = g;
        let p = self.json_phrases(g)?;
        let from = self.item_of(&g.from)?;
        let whole = self.layout.items[from].table && self.layout.items[from].dims.len() == g.from.subscripts.len() + 1;
        let (from_place, subscripts) = self.root(&g.from, whole, g.pos)?;
        let mut nodes = Vec::new();
        let occurs = if whole { Some(self.occurs(from, g.pos)?) } else { None };
        let depth = subscripts.len() + usize::from(whole);
        self.json_node(from, 0, occurs, depth, &p, g.pos, &mut nodes)?;
        let name = self.json_name(from, &p).map(|n| self.sym(&rt::json::string(&n)));
        let receiver = self.place(&g.receiver, true)?;
        let encoding = match &g.encoding {
            None => Ccsid::Unnamed,
            Some(Encoding::FromCodepage) => Ccsid::CodePage,
            Some(Encoding::Ccsid(op)) => Ccsid::Operand(self.operand(op, g.pos)?.operand),
        };
        let count = g.count.as_ref().map(|r| self.counted(r)).transpose()?;
        let code = self.register("JSON-CODE", g.pos)?;
        let (on_exception, not_on_exception) = (g.on_exception.is_some(), g.not_on_exception.is_some());
        Ok(Markup::JsonGenerate(lir::JsonGenerate { from: from_place, subscripts, nodes, name, receiver, encoding, count, code, on_exception, not_on_exception }))
    }

    /// Item `item`'s node, `offset` bytes into its holder, and its members after it; `depth` is
    /// the walk's subscripts inside one occurrence.
    #[allow(clippy::too_many_arguments)]
    fn json_node(&mut self, item: usize, offset: u32, occurs: Option<Count>, depth: usize, p: &JsonPhrases, pos: Pos, nodes: &mut Vec<lir::JsonNode>) -> M<u32> {
        let i = &self.layout.items[item];
        let name = self.sym(&rt::json::string(&self.json_name(item, p).unwrap_or_default()));
        let (len, kind) = (i.size, i.kind);
        let indicator = self.json_indicator(item, depth, p, pos)?;
        let null = p.null_when.get(&item).copied();
        let placeholder = lir::JsonValue::Object { members: Vec::new(), eligible: false };
        let k = push(nodes, lir::JsonNode { offset, len, kind, name, occurs, indicator, null, value: placeholder }, "JSON GENERATE's items")?;
        let value = if i.children.is_empty() || i.kind != Kind::Group {
            lir::JsonValue::Leaf(self.json_leaf(item, depth, p, pos)?)
        } else {
            let (mut members, mut eligible) = (Vec::new(), false);
            self.json_members(item, item, depth, p, pos, nodes, &mut members, &mut eligible)?;
            lir::JsonValue::Object { members, eligible }
        };
        nodes[k as usize].value = value;
        Ok(k)
    }

    /// `json_members` of `group`, whose members an unnamed group's join, at offsets from `holder`.
    #[allow(clippy::too_many_arguments)]
    fn json_members(&mut self, holder: usize, group: usize, depth: usize, p: &JsonPhrases, pos: Pos, nodes: &mut Vec<lir::JsonNode>, members: &mut Vec<u32>, eligible: &mut bool) -> M<()> {
        let layout = self.layout;
        for &c in &layout.items[group].children {
            if self.json_ignored(c, p) {
                continue;
            }
            if p.suppressed.contains(&c) {
                *eligible = true;
                continue;
            }
            let child = &layout.items[c];
            if child.name.is_none() && !child.table {
                self.json_members(holder, c, depth, p, pos, nodes, members, eligible)?;
                continue;
            }
            *eligible = true;
            let offset = child.offset - layout.items[holder].offset;
            let (occurs, inner) = if child.table { (Some(self.occurs(c, pos)?), depth + 1) } else { (None, depth) };
            members.push(self.json_node(c, offset, occurs, inner, p, pos, nodes)?);
        }
        Ok(())
    }

    /// INDICATING's indicator and marker for item `item`, at the walk's `depth`.
    fn json_indicator(&mut self, item: usize, depth: usize, p: &JsonPhrases, pos: Pos) -> M<Option<(Result<PlaceId, AbendId>, lir::Marker)>> {
        let Some(i) = p.indicated.get(&item) else { return Ok(None) };
        let (k, named) = match (&i.indicator, &i.marker) {
            (Some(r), _) => (self.item_of(r)?, r),
            (None, Marker::Condition(c)) => (self.variable_of(c)?.1, c),
            (None, Marker::Literal(_)) => return Err(LowerError::Invalid("INDICATING a literal without IN".into()).into()),
        };
        let at = self.walked_or_refused(k, named, depth)?;
        Ok(Some((at, self.marker(&i.marker, depth, pos)?)))
    }

    fn json_leaf(&mut self, item: usize, depth: usize, p: &JsonPhrases, pos: Pos) -> M<lir::JsonLeaf> {
        let kind = self.layout.items[item].kind;
        let mut suppress: Vec<Figurative> = p.suppressed_when.get(&item).map(|w| w.to_vec()).unwrap_or_default();
        for (numeric, when) in &p.every {
            suppress.extend(when.iter().copied().filter(|&f| every_selects(kind, *numeric, f)));
        }
        let boolean = match p.boolean.get(&item) {
            Some(m) => Some(self.marker(m, depth, pos)?),
            None => None,
        };
        let convert = self.convert(item, kind, "JSON GENERATE", pos)?;
        Ok(lir::JsonLeaf { suppress, boolean, convert })
    }

    fn xml_phrases<'x>(&mut self, x: &'x XmlGenerate) -> M<XmlPhrases<'x>> {
        let mut p = XmlPhrases { attributes: x.attributes, ..XmlPhrases::default() };
        for (r, name) in &x.names {
            let item = self.item_of(r)?;
            let (Literal::Alnum(s) | Literal::National(s)) = name else {
                return Err(refusal("XML GENERATE NAME takes an alphanumeric or national literal", x.pos));
            };
            p.names.insert(item, s.clone());
        }
        for (r, f) in &x.types {
            p.forms.insert(self.item_of(r)?, *f);
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

    fn xml_name(&self, item: usize, p: &XmlPhrases) -> String {
        if let Some(name) = p.names.get(&item) {
            return name.clone();
        }
        let spelled = self.spelled(item);
        rt::xml::generate::name(spelled.as_deref().or(self.layout.items[item].name.as_deref()).unwrap_or_default())
    }

    fn xml_form(&self, item: usize, p: &XmlPhrases) -> ast::XmlForm {
        if let Some(&f) = p.forms.get(&item) {
            return f;
        }
        let i = &self.layout.items[item];
        if p.attributes && !i.table && i.name.is_some() { ast::XmlForm::Attribute } else { ast::XmlForm::Element }
    }

    fn xml_generate(&mut self, x: &XmlGenerate) -> M<Markup> {
        // Names every phrase, so one the parser gains is lowered or refused before it compiles.
        let XmlGenerate {
            receiver: _, from: _, count: _, encoding: _, declaration: _, attributes: _, namespace: _, prefix: _, names: _, types: _, suppress: _, on_exception: _, not_on_exception: _, pos: _,
        } = x;
        let receiver = self.place(&x.receiver, true)?;
        let encoding = match &x.encoding {
            None => Ccsid::Unnamed,
            Some(op) => Ccsid::Operand(self.operand(op, x.pos)?.operand),
        };
        let namespace = x.namespace.as_ref().map(|op| self.operand(op, x.pos).map(|o| o.operand)).transpose()?;
        let prefix = x.prefix.as_ref().map(|op| self.operand(op, x.pos).map(|o| o.operand)).transpose()?;
        // The walker resolves the phrases only once the encoding and namespace are found good.
        let p = match self.xml_phrases(x) {
            Ok(p) => p,
            Err(Fail::Abend(..)) => return Err(Fail::Lower(LowerError::Unsupported("an XML GENERATE phrase that names no data item", x.pos))),
            Err(e) => return Err(e),
        };
        let from = self.item_of(&x.from)?;
        let (from_place, subscripts) = self.root(&x.from, false, x.pos)?;
        let mut nodes = Vec::new();
        let i = &self.layout.items[from];
        let (len, kind) = (i.size, i.kind);
        let name = self.sym(&self.xml_name(from, &p));
        if kind == Kind::Group && !i.children.is_empty() {
            self.xml_group(from, 0, None, name, false, &p, x.pos, &mut nodes)?;
        } else {
            let kind = self.kind_of(from_place);
            let convert = self.convert(from, kind, "XML GENERATE", x.pos)?;
            let value = lir::XmlValue::Leaf { form: XmlForm::Element, suppress: Vec::new(), convert };
            push(&mut nodes, lir::XmlNode { offset: 0, len, kind, name, occurs: None, value }, "XML GENERATE's items")?;
        }
        let count = x.count.as_ref().map(|r| self.counted(r)).transpose()?;
        let code = self.register("XML-CODE", Pos::default())?;
        Ok(Markup::XmlGenerate(lir::XmlGenerate {
            receiver,
            encoding,
            namespace,
            prefix,
            declaration: x.declaration,
            from: from_place,
            subscripts,
            nodes,
            suppressing: !x.suppress.is_empty(),
            count,
            code,
            on_exception: x.on_exception.is_some(),
            not_on_exception: x.not_on_exception.is_some(),
        }))
    }

    /// A group's node, named for an element or unnamed for its members to join its holder's.
    #[allow(clippy::too_many_arguments)]
    fn xml_group(&mut self, item: usize, offset: u32, occurs: Option<Count>, name: lir::SymId, unnamed: bool, p: &XmlPhrases, pos: Pos, nodes: &mut Vec<lir::XmlNode>) -> M<u32> {
        let i = &self.layout.items[item];
        let (len, kind) = (i.size, i.kind);
        let placeholder = lir::XmlValue::Element { members: Vec::new() };
        let k = push(nodes, lir::XmlNode { offset, len, kind, name, occurs, value: placeholder }, "XML GENERATE's items")?;
        let mut members = Vec::new();
        let layout = self.layout;
        for &c in &layout.items[item].children {
            if self.xml_ignored(c) || p.suppressed.contains(&c) {
                continue;
            }
            let child = &layout.items[c];
            let offset = child.offset - layout.items[item].offset;
            let occurs = if child.table { Some(self.occurs(c, pos)?) } else { None };
            let name = self.sym(&self.xml_name(c, p));
            let node = if child.name.is_none() {
                self.xml_group(c, offset, occurs, name, true, p, pos, nodes)?
            } else if child.kind == Kind::Group && !child.children.is_empty() {
                self.xml_group(c, offset, occurs, name, false, p, pos, nodes)?
            } else {
                let form = self.xml_form(c, p);
                let suppress = match p.suppressed_when.get(&c) {
                    Some(when) => when.to_vec(),
                    None => p
                        .every
                        .iter()
                        .filter(|(_, every_form, _)| every_form.is_none_or(|f| f == form))
                        .flat_map(|(numeric, _, when)| when.iter().copied().filter(|&f| every_selects(child.kind, *numeric, f)))
                        .collect(),
                };
                let convert = self.convert(c, child.kind, "XML GENERATE", pos)?;
                let value = lir::XmlValue::Leaf { form: self::form(form), suppress, convert };
                push(nodes, lir::XmlNode { offset, len: child.size, kind: child.kind, name, occurs, value }, "XML GENERATE's items")?
            };
            members.push(node);
        }
        nodes[k as usize].value = if unnamed { lir::XmlValue::Members { members } } else { lir::XmlValue::Element { members } };
        Ok(k)
    }

    fn xml_parse(&mut self, x: &XmlParse) -> R<Markup> {
        // Names every phrase, so one the parser gains is lowered or refused before it compiles.
        let XmlParse { document: _, encoding: _, returning_national: _, procedure: _, thru: _, on_exception: _, not_on_exception: _, pos: _ } = x;
        let procedure = self.processing(x)?;
        let document = self.place(&x.document, false)?;
        let encoding = x.encoding.as_ref().map(|op| self.operand(op, x.pos).map(|o| o.operand)).transpose()?;
        let event = self.place(&special("XML-EVENT", Pos::default()), false)?;
        let code = self.register("XML-CODE", Pos::default())?;
        let information = self.register("XML-INFORMATION", Pos::default())?;
        let code_value = self.int_expr(&Expr::Operand(Operand::Ref(special("XML-CODE", Pos::default()))), x.pos)?;
        Ok(Markup::XmlParse(lir::XmlParse {
            document,
            encoding,
            national: x.returning_national,
            procedure,
            event,
            code,
            information,
            code_value,
            on_exception: x.on_exception.is_some(),
            not_on_exception: x.not_on_exception.is_some(),
        }))
    }

    fn parse_phrases<'j>(&mut self, j: &'j JsonParse) -> M<ParsePhrases<'j>> {
        let mut p = ParsePhrases::default();
        for (r, name) in &j.names {
            let item = self.item_of(r)?;
            let name = match name {
                None => None,
                Some(Literal::Alnum(s) | Literal::National(s)) => Some(s.clone()),
                Some(_) => return Err(refusal("JSON PARSE NAME takes an alphanumeric or national literal", j.pos)),
            };
            p.names.insert(item, name);
        }
        for r in &j.suppress {
            p.suppressed.insert(self.item_of(r)?);
        }
        for r in j.ignoring.iter().flatten() {
            p.ignored.insert(self.item_of(r)?);
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
                (Flag::Condition(c) | Flag::Conditions(c, _), None) => self.variable_of(c)?.1,
                (Flag::Literals(..), None) => return Err(refusal("INDICATING ... USING two literals takes IN and the indicator", j.pos)),
            };
            p.indicators.insert(indicator);
        }
        Ok(p)
    }

    fn parse_ignored(&self, item: usize, p: &ParsePhrases) -> bool {
        let i = &self.layout.items[item];
        if i.redefines.is_some() || i.level == 66 || p.indicators.contains(&item) {
            return true;
        }
        if i.children.is_empty() || i.kind != Kind::Group {
            return i.name.is_none();
        }
        i.children.iter().all(|&c| self.parse_ignored(c, p))
    }

    fn parse_named(&mut self, item: usize, p: &ParsePhrases) -> Named {
        match p.names.get(&item) {
            Some(Some(literal)) => Named::Exactly(self.sym(literal)),
            Some(None) => Named::Omitted,
            None => match &self.layout.items[item].name {
                Some(name) => Named::Folded(self.sym(name)),
                None => Named::Omitted,
            },
        }
    }

    fn json_parse(&mut self, j: &JsonParse) -> M<Markup> {
        // Names every phrase, so one the parser gains is lowered or refused before it compiles.
        let JsonParse {
            source: _, into: _, detail: _, ignoring: _, indicating: _, encoding: _, names: _, suppress: _, converting: _, on_exception: _, not_on_exception: _, pos: _,
        } = j;
        let p = self.parse_phrases(j)?;
        let source = self.place(&j.source, false)?;
        let encoding = match &j.encoding {
            None => Ccsid::Unnamed,
            Some(Encoding::FromCodepage) => Ccsid::CodePage,
            Some(Encoding::Ccsid(op)) => Ccsid::Operand(self.operand(op, j.pos)?.operand),
        };
        // The walker resolves INTO only once the document has parsed.
        let root = match self.item_of(&j.into) {
            Ok(item) => item,
            Err(Fail::Abend(..)) => return Err(Fail::Lower(LowerError::Unsupported("a JSON PARSE INTO that names no data item", j.pos))),
            Err(e) => return Err(e),
        };
        let whole = self.layout.items[root].table && self.layout.items[root].dims.len() == j.into.subscripts.len() + 1;
        let (into, subscripts) = self.root(&j.into, whole, j.pos)?;
        let occurs = if whole { Some(self.occurs(root, j.pos)?) } else { None };
        let depth = subscripts.len() + usize::from(whole);
        let mut nodes = Vec::new();
        self.parse_node(root, 0, occurs, depth, &p, j.pos, &mut nodes)?;
        let code = self.register("JSON-CODE", j.pos)?;
        let status = self.register("JSON-STATUS", j.pos)?;
        Ok(Markup::JsonParse(lir::JsonParse {
            source,
            encoding,
            into,
            subscripts,
            nodes,
            ignore_all: j.ignoring.iter().any(Option::is_none),
            code,
            status,
            on_exception: j.on_exception.is_some(),
            not_on_exception: j.not_on_exception.is_some(),
        }))
    }

    /// Item `item`'s node and the members a name can reach after it, as `parse_value` fills them.
    #[allow(clippy::too_many_arguments)]
    fn parse_node(&mut self, item: usize, offset: u32, occurs: Option<Count>, depth: usize, p: &ParsePhrases, pos: Pos, nodes: &mut Vec<lir::ParseNode>) -> M<u32> {
        let i = &self.layout.items[item];
        let (len, kind) = (i.size, i.kind);
        let group = kind == Kind::Group && !i.children.is_empty();
        let name = self.parse_named(item, p);
        let indicator = match p.indicated.get(&item) {
            None => None,
            Some(&(flag, indicator)) => {
                let (place, target) = match indicator {
                    None => (None, None),
                    Some(r) => {
                        let k = self.item_of(r)?;
                        (Some(self.walked_or_refused(k, r, depth)?), Some(k))
                    }
                };
                Some(lir::Indicator { place, flag: self.parse_flag(flag, target, depth, pos)? })
            }
        };
        let null = match p.null_to.get(&item) {
            Some(&f) => Some((f, self.move_plan(&Side { value: Value::Fig(f), src: None, digits: 0 }, kind, Some(item))?)),
            None => None,
        };
        let node = lir::ParseNode { offset, len, kind, name, occurs, ignored: p.ignored.contains(&item), indicator, null, value: lir::ParseValue::Suppressed };
        let k = push(nodes, node, "JSON PARSE's items")?;
        let value = if group {
            let mut members = Vec::new();
            self.parse_members(item, item, depth, p, pos, nodes, &mut members)?;
            lir::ParseValue::Object { members }
        } else {
            lir::ParseValue::Leaf(self.parse_leaf(item, depth, p, pos)?)
        };
        nodes[k as usize].value = value;
        Ok(k)
    }

    /// `parse_members` of `group`: an unnamed group's members join it, an unnamed table is out of
    /// reach, and a suppressed member is reached but takes nothing.
    #[allow(clippy::too_many_arguments)]
    fn parse_members(&mut self, holder: usize, group: usize, depth: usize, p: &ParsePhrases, pos: Pos, nodes: &mut Vec<lir::ParseNode>, members: &mut Vec<u32>) -> M<()> {
        let layout = self.layout;
        for &c in &layout.items[group].children {
            if self.parse_ignored(c, p) {
                continue;
            }
            let child = &layout.items[c];
            let offset = child.offset - layout.items[holder].offset;
            match (child.name.is_none(), child.table) {
                (true, false) => self.parse_members(holder, c, depth, p, pos, nodes, members)?,
                (true, true) => {}
                (false, _) if p.suppressed.contains(&c) => {
                    let name = self.parse_named(c, p);
                    let node = lir::ParseNode {
                        offset,
                        len: child.size,
                        kind: child.kind,
                        name,
                        occurs: None,
                        ignored: false,
                        indicator: None,
                        null: None,
                        value: lir::ParseValue::Suppressed,
                    };
                    members.push(push(nodes, node, "JSON PARSE's items")?);
                }
                (false, table) => {
                    let (occurs, inner) = if table { (Some(self.occurs(c, pos)?), depth + 1) } else { (None, depth) };
                    members.push(self.parse_node(c, offset, occurs, inner, p, pos, nodes)?);
                }
            }
        }
        Ok(())
    }

    /// `parse_elementary`, `parse_string` and `parse_number` by the item's kind.
    fn parse_leaf(&mut self, item: usize, depth: usize, p: &ParsePhrases, pos: Pos) -> M<lir::ParseLeaf> {
        let i = &self.layout.items[item];
        let kind = i.kind;
        if i.scaling > 0 {
            return Err(Fail::Lower(LowerError::Unsupported("JSON PARSE into an item with PICTURE scaling positions", pos)));
        }
        let boolean = match p.booleans.get(&item) {
            Some(flag) => Some(self.parse_flag(flag, Some(item), depth, pos)?),
            None => None,
        };
        let side = |value| Side { value, src: None, digits: 0 };
        let text = match kind {
            Kind::Alnum { .. } | Kind::AlnumEdited { .. } => Some(self.move_plan(&side(Value::Bytes), kind, Some(item))?),
            Kind::National => Some(self.move_plan(&side(Value::National), kind, Some(item))?),
            _ => None,
        };
        let number = match kind {
            Kind::Float(_) => NumberInto::Float(self.move_plan(&side(Value::Float), kind, Some(item))?),
            Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. } => NumberInto::Store(self.store_plan(kind, Some(item))?),
            Kind::NumericEdited { .. } => NumberInto::Edited(self.move_plan(&side(Value::Num(None)), kind, Some(item))?),
            Kind::Alnum { .. } if self.layout.items[item].alphabetic => NumberInto::Incompatible,
            Kind::Alnum { .. } | Kind::AlnumEdited { .. } | Kind::National => NumberInto::Digits,
            _ => NumberInto::Incompatible,
        };
        Ok(lir::ParseLeaf { boolean, text, number })
    }

    /// `parse_flag`: two literals go into `target`, an item index; condition-names are SET.
    fn parse_flag(&mut self, flag: &Flag, target: Option<usize>, depth: usize, pos: Pos) -> R<lir::Flag> {
        Ok(match flag {
            Flag::Condition(c) => lir::Flag::Set { on: self.set_to(c, true, depth, pos)?, off: self.set_to(c, false, depth, pos)? },
            Flag::Conditions(yes, no) => lir::Flag::Set { on: self.set_to(yes, true, depth, pos)?, off: self.set_to(no, true, depth, pos)? },
            Flag::Literals(yes, no) => {
                let Some(t) = target else { return Err(LowerError::Invalid("two JSON PARSE literals with no item to go in".into())) };
                let kind = self.layout.items[t].kind;
                let (on, on_side) = self.literal_const(yes, pos)?;
                let (off, off_side) = self.literal_const(no, pos)?;
                lir::Flag::Literals { on: (on, self.move_plan(&on_side, kind, Some(t))?), off: (off, self.move_plan(&off_side, kind, Some(t))?) }
            }
        })
    }

    /// `condition_at` and SET of condition-name `c` TO TRUE or TO FALSE, at the walk's `depth`.
    fn set_to(&mut self, c: &Ref, truth: bool, depth: usize, pos: Pos) -> R<SetTo> {
        let (index, variable) = match self.variable_of(c) {
            Ok(found) => found,
            Err(fail) => return self.refused(fail).map(SetTo::Refused),
        };
        let condition = &self.layout.conditions[index];
        let value = if truth { condition.values.first().map(|(v, _)| v) } else { condition.false_value.as_ref() };
        let Some(value) = value else { return Ok(SetTo::Nothing) };
        let place = match self.walked(variable, c, depth) {
            Ok(place) => place,
            Err(fail) => return self.refused(fail).map(SetTo::Refused),
        };
        let (value, side) = self.literal_const(value, pos)?;
        let plan = self.move_plan(&side, self.kind_of(place), self.place_items[place as usize])?;
        Ok(SetTo::Move { place, value, plan })
    }
}
