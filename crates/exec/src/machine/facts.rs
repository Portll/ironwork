//! What `rt` asks of the running program, answered from the interpreter's layout, and the
//! statement inputs `rt` takes, built from the AST with the walker's references as handles.

use super::*;
use crate::collating::Sequence;
use crate::picture::Sym;
use rt::host::{Host, Values};
use rt::lir::{self, Chars, Replacement};
use rt::store::ProgramFacts;
use rt::text;

#[derive(Clone, Copy)]
pub struct Facts<'p> {
    layout: &'p Layout,
    collating: &'p Sequence,
    options: Options,
    page: &'static CodePage,
    decimal_point: char,
}

impl ProgramFacts for Facts<'_> {
    fn options(&self) -> Options {
        self.options
    }

    fn page(&self) -> &'static CodePage {
        self.page
    }

    fn figurative(&self, f: Figurative) -> u8 {
        self.collating.figurative(f)
    }

    fn collation(&self) -> &Collation {
        self.collating.collation()
    }

    fn ordinal(&self, byte: u8) -> u16 {
        self.collating.ordinal(byte)
    }

    fn character(&self, ordinal: i64) -> Option<u8> {
        self.collating.character(ordinal)
    }

    fn characters(&self) -> usize {
        self.collating.count()
    }

    fn decimal_point(&self) -> char {
        self.decimal_point
    }

    fn edit(&self, edit: u32) -> (&[Sym], &str) {
        (&self.layout.edits[edit as usize], &self.layout.currencies[edit as usize])
    }

    fn scaling(&self, item: usize) -> u32 {
        self.layout.items.get(item).map_or(0, |i| i.scaling)
    }

    /// RETURN-CODE has no item of its own.
    fn item_name(&self, item: usize) -> String {
        self.layout.items.get(item).map_or("RETURN-CODE".into(), |i| i.name.clone().unwrap_or_else(|| "FILLER".into()))
    }
}

impl<'p> Machine<'p, '_, '_> {
    /// Holds no borrow of the machine, so a call can take it beside `self.unit`.
    pub(super) fn facts(&self) -> Facts<'p> {
        Facts { layout: self.layout, collating: self.collating, options: self.options, page: self.page, decimal_point: self.decimal_point() }
    }

    /// An INSPECT phrase with a figurative REPLACING value as its character.
    pub(super) fn inspect_phrase<'a>(&self, p: &'a InspectPhrase) -> text::InspectPhrase<&'a Ref, &'a Operand> {
        let by = p.by.as_ref().map(|op| match op {
            Operand::Literal(Literal::Figurative(f)) => Replacement::Fill(self.collating.figurative(*f)),
            op => Replacement::Chars(chars(op)),
        });
        text::InspectPhrase { mode: p.mode, pattern: p.pattern.as_ref().map(chars), by, counter: p.counter.as_ref(), bounds: bounds(&p.bounds) }
    }
}

/// An operand as STRING, UNSTRING and INSPECT take it: an item's storage, or anything else's value.
pub(super) fn chars(op: &Operand) -> Chars<&Ref, &Operand> {
    match op {
        Operand::Ref(r) => Chars::Place(r),
        other => Chars::Value(other),
    }
}

pub(super) fn bounds(bounds: &[Bound]) -> Vec<lir::Bound<&Ref, &Operand>> {
    bounds.iter().map(|b| lir::Bound { after: b.after, value: chars(&b.value) }).collect()
}

impl<'a, 'p> Host<&'a Ref> for Machine<'p, '_, '_> {
    type Facts = Facts<'p>;

    fn facts(&self) -> Facts<'p> {
        Machine::facts(self)
    }

    fn mem(&mut self) -> &mut [u8] {
        &mut self.unit.mem
    }

    fn locate(&mut self, place: &'a Ref, receiving: bool) -> R<Loc> {
        self.locate_as(place, receiving)
    }

    fn integer(&mut self, place: &'a Ref, pos: Pos) -> R<i64> {
        Machine::integer(self, &Expr::Operand(Operand::Ref(place.clone())), pos)
    }

    fn assign(&mut self, dest: Loc, val: Val, src: Option<Loc>, pos: Pos) -> R<()> {
        Machine::assign(self, dest, val, src, pos)
    }

    fn store_fixed(&mut self, dest: Loc, value: &Fixed, pos: Pos) -> R<()> {
        Machine::store_fixed(self, dest, value, false, pos)
    }
}

impl<'a> Values<&'a Ref, &'a Operand> for Machine<'_, '_, '_> {
    fn value(&mut self, operand: &&'a Operand, pos: Pos) -> R<Val> {
        self.operand(operand, pos)
    }
}
