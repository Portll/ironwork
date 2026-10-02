//! What `rt` asks of the running program, answered from the interpreter's layout, and the
//! statement inputs `rt` takes, built from the AST with the walker's references as handles.

use super::*;
use crate::collating::Sequence;
use crate::picture::Sym;
use crate::files::{Dd, Keying, Open};
use crate::printer::{self, Space};
use rt::callee::Arguments;
use rt::fileio::{self, Advance, Files};
use rt::host::{Host, Values};
use rt::lir::{self, Chars, Replacement};
use rt::store::{LaxRedefinition, ProgramFacts};
use rt::text;
use rt::unit::UnitHost;
use std::rc::Rc;

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

    fn lax_redefinition(&self, item: usize) -> Option<LaxRedefinition> {
        self.layout.numcheck.lax(item)
    }

    fn numcheck_removed(&self, item: usize, pos: Pos) -> bool {
        self.layout.numcheck.removed(item, pos)
    }
}

impl<'p> Machine<'p, '_, '_> {
    /// Holds no borrow of the machine, so a call can take it beside `self.unit`.
    pub(super) fn facts(&self) -> Facts<'p> {
        Facts { layout: self.layout, collating: self.collating, options: self.options, page: self.page, decimal_point: self.decimal_point() }
    }
}

/// An INSPECT phrase, its literals read as the inspected item's characters when the statement runs.
pub(super) fn inspect_phrase(p: &InspectPhrase) -> text::InspectPhrase<&Ref, &Operand> {
    let by = p.by.as_ref().map(|op| Replacement::Chars(chars(op)));
    text::InspectPhrase { mode: p.mode, pattern: p.pattern.as_ref().map(chars), by, counter: p.counter.as_ref(), bounds: bounds(&p.bounds) }
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

    fn taint(&mut self) -> Option<&mut rt::taint::Taint> {
        self.unit.taint.as_mut()
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

impl<'w> UnitHost<'w> for Machine<'_, '_, 'w> {
    type Program = Rc<Compiled>;
    type Loader = crate::unit::Library;

    fn unit(&mut self) -> &mut RunUnit<'w> {
        self.unit
    }
}

impl<'a, 'w> Arguments<'w, &'a Ref, &'a Operand> for Machine<'_, '_, 'w> {
    fn item(&self, operand: &&'a Operand) -> Option<&'a Ref> {
        match *operand {
            Operand::Ref(r) => Some(r),
            _ => None,
        }
    }

    fn length_of(&self, operand: &&'a Operand) -> bool {
        matches!(operand, Operand::LengthOf(_))
    }

    fn content_item(&mut self, place: &'a Ref) -> R<Loc> {
        let loc = self.locate(place)?;
        self.numcheck(loc, false, place.pos)?;
        Ok(loc)
    }
}

/// An integer the file verbs have the walker evaluate: a LINAGE value or an ADVANCING count.
#[derive(Clone, Copy)]
pub(super) enum Int<'a> {
    Linage(&'a LinageValue),
    Count(&'a Expr),
}

impl<'p> Machine<'p, '_, '_> {
    /// File k's SELECT and FD as `rt::fileio` takes them.
    pub(super) fn file_desc(&self, k: usize) -> fileio::File<'p, &'p Ref, Int<'p>> {
        let (program, layout) = (self.program, self.layout);
        let decl = &program.files[k];
        let counter = layout.linage_counters.get(k).copied().flatten().map(|i| {
            let item = &layout.items[i];
            Loc { offset: self.base + item.offset as usize, len: item.size as usize, kind: item.kind, item: i }
        });
        fileio::File {
            index: k,
            name: &decl.name,
            assign: &decl.assign,
            organization: match decl.organization {
                Organization::Sequential => lir::Organization::Sequential,
                Organization::LineSequential => lir::Organization::LineSequential,
                Organization::Indexed => lir::Organization::Indexed,
                Organization::Relative => lir::Organization::Relative,
            },
            access: match decl.access {
                Access::Sequential => lir::Access::Sequential,
                Access::Random => lir::Access::Random,
                Access::Dynamic => lir::Access::Dynamic,
            },
            optional: decl.optional,
            format: self.described_format(k),
            status: decl.status.as_ref(),
            relative: decl.relative_key.as_ref(),
            linage: decl.linage.as_ref().map(|l| fileio::Linage {
                lines: Int::Linage(&l.lines),
                footing: l.footing.as_ref().map(Int::Linage),
                top: l.top.as_ref().map(Int::Linage),
                bottom: l.bottom.as_ref().map(Int::Linage),
                counter,
            }),
            carriage: self.carriage[k].map(|c| lir::Carriage { machine: c.machine, reserved: c.reserved }),
            area: self.area(k),
            read_lengths: {
                let (shortest, longest) = compile::read_lengths(decl, layout, k, self.options.vlr);
                (shortest as usize, longest as usize)
            },
            depending: decl.record_depending.as_ref().map(|item| {
                let (shortest, longest) = compile::varying_lengths(decl, layout, k);
                fileio::Depending { item, lengths: (shortest as usize, longest as usize) }
            }),
        }
    }
}

pub(super) fn spacing(space: Space) -> lir::Spacing {
    match space {
        Space::Lines(n) => lir::Spacing::Lines(n),
        Space::Channel(c) => lir::Spacing::Channel(c),
        Space::PageMode => lir::Spacing::PageMode,
    }
}

pub(super) fn advance(a: &Advancing) -> Advance<'_, Int<'_>> {
    match a {
        Advancing::Lines { before, count } => Advance::Lines { before: *before, count: Int::Count(count) },
        Advancing::Page { before } => Advance::Page { before: *before },
        Advancing::Mnemonic { before, name, environment } => Advance::Mnemonic { before: *before, space: printer::mnemonic_space(environment).map(spacing), name, environment },
    }
}

impl<'a> Files<&'a Ref, Int<'a>> for Machine<'_, '_, '_> {
    fn slot(&mut self, k: usize) -> &mut Option<Open> {
        self.unit.file(self.me, k)
    }

    fn locked(&mut self, k: usize) -> &mut bool {
        self.unit.locked(self.me, k)
    }

    fn dd(&self, assign: &str) -> Option<Dd> {
        self.unit.dds.get(assign)
    }

    fn notify(&mut self, event: Event<'_>) {
        self.unit.notify(event);
    }

    fn int(&mut self, value: Int<'a>, pos: Pos) -> R<i64> {
        match value {
            Int::Linage(v) => self.linage_value(v, pos),
            Int::Count(e) => self.integer(e, pos),
        }
    }

    fn keying(&mut self, k: usize, pos: Pos) -> R<Keying> {
        Machine::keying(self, k, pos)
    }

    fn key_value(&mut self, k: usize, keying: &Keying, key: &'a Ref, partial: bool, pos: Pos) -> R<(usize, Vec<u8>)> {
        self.key_named(k, keying, key, partial, pos)
    }
}
