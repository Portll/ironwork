//! What `rt::store` asks of the running program, answered from the interpreter's layout.

use super::*;
use crate::collating::Sequence;
use crate::picture::Sym;
use rt::store::ProgramFacts;

#[derive(Clone, Copy)]
pub(super) struct Facts<'p> {
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
}
