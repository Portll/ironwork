//! What `rt` asks of the running program, answered from its compiled layout, options and
//! collating sequence.

use crate::Compiled;
use crate::collating::Sequence;
use crate::layout::Layout;
use crate::picture::Sym;
use numeric::Options;
use rt::store::{LaxRedefinition, ProgramFacts};
use rt::vocab::{Figurative, Pos};
use zarch::ebcdic::{CodePage, Collation};

#[derive(Clone, Copy)]
pub struct Facts<'p> {
    pub layout: &'p Layout,
    pub collating: &'p Sequence,
    pub options: Options,
    pub page: &'static CodePage,
    pub decimal_point: char,
}

impl<'p> Facts<'p> {
    pub fn of(c: &'p Compiled) -> Self {
        let decimal_point = if c.program.environment.decimal_point_comma { ',' } else { '.' };
        Self { layout: &c.layout, collating: &c.collating, options: c.options, page: c.options.code_page(), decimal_point }
    }
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
