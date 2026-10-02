//! JSON GENERATE, JSON PARSE, XML GENERATE and XML PARSE (lir.md §9.13): each statement's
//! phrases, and the tree of items its walk reaches with every choice the walker makes by name or
//! kind made once. Trees nest by index: a node's members come after it in its node table.

use super::{AbendId, CondId, ConstId, Count, IntExpr, MovePlan, Operand, PlaceId, RangeId, StorePlan, SymId};
use crate::storage::Kind;
use crate::vocab::Figurative;
use crate::{codec_enum, codec_struct};
use zarch::hfp::Precision;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Markup {
    JsonGenerate(JsonGenerate),
    XmlGenerate(XmlGenerate),
    XmlParse(XmlParse),
    JsonParse(JsonParse),
}

impl Markup {
    /// ON EXCEPTION and NOT ON EXCEPTION as written: with either, the op returns Arm(1) for an
    /// exception and Arm(0) otherwise, for a `Select` of two blocks.
    pub fn phrases(&self) -> (bool, bool) {
        match self {
            Markup::JsonGenerate(g) => (g.on_exception, g.not_on_exception),
            Markup::XmlGenerate(g) => (g.on_exception, g.not_on_exception),
            Markup::XmlParse(p) => (p.on_exception, p.not_on_exception),
            Markup::JsonParse(p) => (p.on_exception, p.not_on_exception),
        }
    }
}

/// The CCSID a statement's document is in: none written, the program's CODEPAGE (JSON's ENCODING
/// FROM CODEPAGE), or an operand read as the statement starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ccsid {
    Unnamed,
    CodePage,
    Operand(Operand),
}

/// An elementary item's value as JSON and XML GENERATE write it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Convert {
    /// Alphanumeric and edited items, and a group with no members: its bytes decoded in the
    /// program's code page, trimmed of trailing spaces, or of leading ones when `justified`.
    Chars { justified: bool },
    /// UTF-16 units, trimmed of trailing spaces.
    National,
    /// COMP-1 (8 decimals) or COMP-2 (17).
    Float(Precision),
    /// Zoned, packed, binary and index items: the value, read as its kind reads it, with at least
    /// `integers` integer positions before the leading zeros are trimmed.
    Fixed { integers: u32 },
    /// A pointer, procedure-pointer or object reference: this IRONWORK abend when it is reached.
    Refused(AbendId),
}

/// A USING value that stands for true or null.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Marker {
    /// The first byte equals this literal's first, in the program's code page; never when the
    /// literal has none.
    Byte(Option<u8>),
    /// A condition-name, its conditional variable located with the walk's subscripts.
    Condition(CondId),
    /// The IRONWORK abend the walker gives when it tests the marker.
    Refused(AbendId),
}

/// JSON GENERATE. In order: FROM is located, its subscripts evaluated, which begin the walk's
/// subscripts (`IntExpr::Walk`), and the tree walked from `nodes[0]`; then the receiver is located,
/// the encoding read, the document written, COUNT IN and JSON-CODE stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonGenerate {
    /// FROM, or its table's first element when it names the whole table.
    pub from: PlaceId,
    pub subscripts: Vec<IntExpr>,
    /// `nodes[0]` is FROM, with `occurs` when FROM names a whole table.
    pub nodes: Vec<JsonNode>,
    /// The name the document wraps FROM's value in, as a JSON string; None for NAME ... OMITTED.
    pub name: Option<SymId>,
    pub receiver: PlaceId,
    pub encoding: Ccsid,
    pub count: Option<(PlaceId, StorePlan)>,
    pub code: (PlaceId, StorePlan),
    pub on_exception: bool,
    pub not_on_exception: bool,
}

/// An item of the tree, `offset` bytes into the occurrence of the node that holds it, `len` its
/// size; a table's elements are `len` apart, `occurs` of them. `name` is a member's JSON string.
/// Each occurrence, of a group or an elementary item, is null when `indicator`'s marker holds or
/// it equals `null`, tested in that order before its value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonNode {
    pub offset: u32,
    pub len: u32,
    pub kind: Kind,
    pub name: SymId,
    pub occurs: Option<Count>,
    /// INDICATING: the indicator, located with the walk's subscripts, whose first byte `Marker`
    /// tests; or the abend of locating it with too few.
    pub indicator: Option<(Result<PlaceId, AbendId>, Marker)>,
    /// CONVERTING ... TO JSON NULL: null when the item equals this figurative constant.
    pub null: Option<Figurative>,
    pub value: JsonValue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JsonValue {
    /// A group's members, in order, an unnamed group's among them. With `eligible` (some member
    /// is not ignored), a group whose members are all left out is left out; a table whose
    /// elements all are is left out.
    Object { members: Vec<u32>, eligible: bool },
    Leaf(JsonLeaf),
}

/// Tested in this order, after the node's null tests: `suppress` (left out), `boolean`, `convert`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonLeaf {
    /// SUPPRESS: the item's own WHEN constants, then each EVERY phrase's that selects its kind.
    pub suppress: Vec<Figurative>,
    /// CONVERTING ... TO BOOLEAN: true when the marker holds of the item's first byte.
    pub boolean: Option<Marker>,
    pub convert: Convert,
}

/// XML GENERATE. In order: the receiver is located and the encoding read, which may end the
/// statement with XML-CODE 411, 414 or 415; NAMESPACE is read (416), then NAMESPACE-PREFIX for a
/// namespace that is not empty (419); FROM is located, its subscripts evaluated, and the tree walked from `nodes[0]`; a national
/// value in a single-byte document ends it with 420; then the document is written and COUNT IN and
/// XML-CODE stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XmlGenerate {
    pub receiver: PlaceId,
    /// `Unnamed` is UTF-16 for a national receiver and the program's CODEPAGE otherwise.
    pub encoding: Ccsid,
    pub namespace: Option<Operand>,
    pub prefix: Option<Operand>,
    pub declaration: bool,
    pub from: PlaceId,
    pub subscripts: Vec<IntExpr>,
    /// `nodes[0]` is FROM: its element, which SUPPRESS never leaves out and which takes the
    /// namespace declaration, or an elementary item's element whatever its form.
    pub nodes: Vec<XmlNode>,
    /// SUPPRESS is written, so an element left with no attributes and no content is left out.
    pub suppressing: bool,
    pub count: Option<(PlaceId, StorePlan)>,
    pub code: (PlaceId, StorePlan),
    pub on_exception: bool,
    pub not_on_exception: bool,
}

/// As `JsonNode`; `name` is the element or attribute name as the document writes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XmlNode {
    pub offset: u32,
    pub len: u32,
    pub kind: Kind,
    pub name: SymId,
    pub occurs: Option<Count>,
    pub value: XmlValue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum XmlValue {
    /// A named group's element: each occurrence of each member, in order.
    Element { members: Vec<u32> },
    /// An unnamed group, each occurrence's members joining the element that holds it.
    Members { members: Vec<u32> },
    /// `suppress`: the item's own WHEN constants, or when it has none each EVERY phrase's that
    /// selects its kind and form. A value that is not legal XML goes in hexadecimal, as an element
    /// named "hex." and the name, and XML-CODE is 417.
    Leaf { form: XmlForm, suppress: Vec<Figurative>, convert: Convert },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XmlForm {
    Attribute,
    Element,
    Content,
}

/// XML PARSE. The document is located, the encoding read (a CCSID ironwork has no page for abends
/// IRONWORK), and the document located again and decoded; then each event the scanner reports
/// sets XML-EVENT, XML-CODE and XML-INFORMATION and the fragment registers (`Base::Xml`), and runs
/// `procedure` through `Procedures::run` with the arrival PERFORM LOOP, at the statement's depth.
/// A procedure that leaves ends the statement with its step. After it completes, XML-CODE is read:
/// a warning the scanner reports as an EXCEPTION event (an undeclared prefix) goes on when it is 0,
/// and that or any other EXCEPTION otherwise ends the parse with the scanner's code;
/// END-OF-DOCUMENT ends it with 0; at END-OF-INPUT 1 locates the document again for the next
/// segment and any other value ends the input; after any other event -1 ends the parse with -1.
/// XML-CODE then takes the result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XmlParse {
    pub document: PlaceId,
    pub encoding: Option<Operand>,
    /// RETURNING NATIONAL.
    pub national: bool,
    pub procedure: RangeId,
    pub event: PlaceId,
    pub code: (PlaceId, StorePlan),
    pub information: (PlaceId, StorePlan),
    /// XML-CODE as read after each event.
    pub code_value: IntExpr,
    pub on_exception: bool,
    pub not_on_exception: bool,
}

/// JSON PARSE. In order: the source is located, the encoding read (ENCODING FROM CODEPAGE of a
/// national source is JSON-CODE 109 before it is), the text decoded and parsed; then, for a
/// document that parses, INTO is located and its subscripts evaluated, which begin the walk's, and
/// each value goes into the item whose name matches it, by MOVE's rules; JSON-CODE and then
/// JSON-STATUS are stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonParse {
    pub source: PlaceId,
    pub encoding: Ccsid,
    /// INTO, or its table's first element when it names the whole table.
    pub into: PlaceId,
    pub subscripts: Vec<IntExpr>,
    /// `nodes[0]` is INTO, with `occurs` when INTO names a whole table. Its name `Omitted` makes
    /// the document INTO's value; otherwise the document is an object whose members INTO's name
    /// matches.
    pub nodes: Vec<ParseNode>,
    /// IGNORING JSON NULL FOR ALL.
    pub ignore_all: bool,
    pub code: (PlaceId, StorePlan),
    pub status: (PlaceId, StorePlan),
    pub on_exception: bool,
    pub not_on_exception: bool,
}

/// As `JsonNode`. Each item takes a value in this order: a second value at the same item and
/// offset is a duplicate; `indicator`; then a null goes to `null`'s MOVE, or is ignored, or is a
/// status; an object fills a group's members; any other value goes to a leaf.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseNode {
    pub offset: u32,
    pub len: u32,
    pub kind: Kind,
    pub name: Named,
    pub occurs: Option<Count>,
    /// IGNORING JSON NULL FOR the item.
    pub ignored: bool,
    pub indicator: Option<Indicator>,
    /// CONVERTING ... FROM JSON NULL: the figurative constant and its MOVE into the item.
    pub null: Option<(Figurative, MovePlan)>,
    pub value: ParseValue,
}

/// How a JSON name matches an item: a NAME literal exactly, a data-name whatever the case of its
/// letters, or never (NAME ... OMITTED).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Named {
    Exactly(SymId),
    Folded(SymId),
    Omitted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseValue {
    /// A group's members a name can reach, an unnamed group's among them, unnamed tables not.
    Object { members: Vec<u32> },
    Leaf(ParseLeaf),
    /// SUPPRESS: a name reaches it, and it takes nothing.
    Suppressed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseLeaf {
    /// CONVERTING ... FROM BOOLEAN: true sets the first value, false the second.
    pub boolean: Option<Flag>,
    /// A string's characters into an alphanumeric, alphanumeric-edited or national item: bytes in
    /// the program's code page, X'3F' for any it lacks, or UTF-16, by this MOVE. None for any
    /// other item, which takes the number a string of digits spells.
    pub text: Option<MovePlan>,
    pub number: NumberInto,
}

/// How a number goes into a leaf.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumberInto {
    Float(MovePlan),
    /// Zoned, packed or binary: stored with a size error a status, not an exception.
    Store(StorePlan),
    /// Numeric-edited, by MOVE.
    Edited(MovePlan),
    /// An integer only: an alphanumeric or alphanumeric-edited item takes it as MOVE takes an
    /// unsigned integer of as many digits, a national item its digits as UTF-16.
    Digits,
    /// An alphabetic item, or any other no number moves to.
    Incompatible,
}

/// INDICATING: the indicator IN names, located with the walk's subscripts whatever the flag,
/// then the flag, on for a null.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Indicator {
    pub place: Option<Result<PlaceId, AbendId>>,
    pub flag: Flag,
}

/// A USING phrase: what a true or null value sets, then what a false or other value sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flag {
    /// SET condition-name TO TRUE, or TO FALSE.
    Set { on: SetTo, off: SetTo },
    /// One of two literals moved into the indicator, or the item.
    Literals { on: (ConstId, MovePlan), off: (ConstId, MovePlan) },
}

/// SET of a condition-name, its conditional variable located with the walk's subscripts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetTo {
    /// It has no such value.
    Nothing,
    Move { place: PlaceId, value: ConstId, plan: MovePlan },
    /// It is not a condition-name, or the walk has too few subscripts for it.
    Refused(AbendId),
}

/// The XML special registers whose length is the current event's fragment's: empty outside a
/// processing procedure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XmlRegister {
    Text,
    NText,
    Namespace,
    NNamespace,
    Prefix,
    NPrefix,
}

impl XmlRegister {
    pub fn national(self) -> bool {
        matches!(self, XmlRegister::NText | XmlRegister::NNamespace | XmlRegister::NPrefix)
    }
}

codec_enum!(Markup { JsonGenerate(g) = 0, XmlGenerate(g) = 1, XmlParse(p) = 2, JsonParse(p) = 3 });
codec_enum!(Ccsid { Unnamed = 0, CodePage = 1, Operand(o) = 2 });
codec_enum!(Convert { Chars { justified } = 0, National = 1, Float(precision) = 2, Fixed { integers } = 3, Refused(abend) = 4 });
codec_enum!(Marker { Byte(b) = 0, Condition(c) = 1, Refused(abend) = 2 });
codec_struct!(JsonGenerate { from, subscripts, nodes, name, receiver, encoding, count, code, on_exception, not_on_exception } check json_valid);
codec_struct!(JsonNode { offset, len, kind, name, occurs, indicator, null, value });
codec_enum!(JsonValue { Object { members, eligible } = 0, Leaf(leaf) = 1 });
codec_struct!(JsonLeaf { suppress, boolean, convert });
codec_struct!(XmlGenerate {
    receiver, encoding, namespace, prefix, declaration, from, subscripts, nodes, suppressing, count, code, on_exception, not_on_exception,
} check xml_valid);
codec_struct!(XmlNode { offset, len, kind, name, occurs, value });
codec_enum!(XmlValue { Element { members } = 0, Members { members } = 1, Leaf { form, suppress, convert } = 2 });
codec_enum!(XmlForm { Attribute = 0, Element = 1, Content = 2 });
codec_struct!(XmlParse { document, encoding, national, procedure, event, code, information, code_value, on_exception, not_on_exception });
codec_struct!(JsonParse { source, encoding, into, subscripts, nodes, ignore_all, code, status, on_exception, not_on_exception } check parse_valid);
codec_struct!(ParseNode { offset, len, kind, name, occurs, ignored, indicator, null, value });
codec_enum!(Named { Exactly(name) = 0, Folded(name) = 1, Omitted = 2 });
codec_enum!(ParseValue { Object { members } = 0, Leaf(leaf) = 1, Suppressed = 2 });
codec_struct!(ParseLeaf { boolean, text, number });
codec_enum!(NumberInto { Float(plan) = 0, Store(store) = 1, Edited(plan) = 2, Digits = 3, Incompatible = 4 });
codec_struct!(Indicator { place, flag });
codec_enum!(Flag { Set { on, off } = 0, Literals { on, off } = 1 });
codec_enum!(SetTo { Nothing = 0, Move { place, value, plan } = 1, Refused(abend) = 2 });
codec_enum!(XmlRegister { Text = 0, NText = 1, Namespace = 2, NNamespace = 3, Prefix = 4, NPrefix = 5 });

/// A tree with a root, whose every member comes after the node that holds it, so no walk loops.
fn tree_valid<'n>(nodes: usize, members: impl Iterator<Item = (usize, &'n [u32])>) -> Result<(), String> {
    if nodes == 0 {
        return Err("a markup tree with no root".into());
    }
    for (k, list) in members {
        if let Some(&m) = list.iter().find(|&&m| m as usize <= k || m as usize >= nodes) {
            return Err(format!("markup node {k} holds node {m} of {nodes}"));
        }
    }
    Ok(())
}

fn json_valid(g: &JsonGenerate) -> Result<(), String> {
    tree_valid(g.nodes.len(), g.nodes.iter().enumerate().filter_map(|(k, n)| match &n.value {
        JsonValue::Object { members, .. } => Some((k, &members[..])),
        JsonValue::Leaf(_) => None,
    }))
}

fn xml_valid(g: &XmlGenerate) -> Result<(), String> {
    tree_valid(g.nodes.len(), g.nodes.iter().enumerate().filter_map(|(k, n)| match &n.value {
        XmlValue::Element { members } | XmlValue::Members { members } => Some((k, &members[..])),
        XmlValue::Leaf { .. } => None,
    }))
}

fn parse_valid(p: &JsonParse) -> Result<(), String> {
    tree_valid(p.nodes.len(), p.nodes.iter().enumerate().filter_map(|(k, n)| match &n.value {
        ParseValue::Object { members } => Some((k, &members[..])),
        _ => None,
    }))
}
