//! EXEC SQL (lir.md §9.7): the statement of `Op::Sql`'s ordinal run by `rt::sql::run` over the
//! activation as its `SqlHost`, its SQLCODE and SQLWARN0 kept for the WHENEVER tests after it.

use super::{Code, R, Vm, not_yet};
use crate::abend::Abend;
use crate::lir::{AbendId, PlaceId, SqlTest, Step, SymId};
use crate::sql::{self, Ran, Session, SqlHost};
use crate::unit::Loader;
use crate::vocab::Pos;
use std::rc::Rc;

impl<L: Loader<Rc<Code>>> Vm<'_, '_, '_, L> {
    pub(super) fn sql(&mut self, ordinal: u32, pos: Pos) -> R<Step> {
        let p = self.p;
        let Some(entry) = (ordinal as usize).checked_sub(1).and_then(|k| p.sql.get(k)) else { return Err(not_yet("an EXEC SQL ordinal outside the program's table")) };
        let ran = sql::run(self, entry, &p.services.sqlca, pos);
        self.whenever = self.settle(ran)?;
        Ok(Step::Next)
    }

    /// The WHENEVER class of the last statement's outcome, as the walker's `whenever` tests it:
    /// SQLERROR, then NOT FOUND, then SQLWARNING.
    pub(super) fn sql_test(&self, test: SqlTest) -> R<bool> {
        let Some(Ran { sqlcode, warned }) = self.whenever else { return Err(not_yet("a WHENEVER test before an EXEC SQL statement ran")) };
        Ok(match test {
            SqlTest::Error => sqlcode < 0,
            SqlTest::NotFound => sqlcode == 100,
            SqlTest::Warning => sqlcode >= 0 && sqlcode != 100 && (warned || sqlcode > 0),
        })
    }
}

impl<'w, L: Loader<Rc<Code>>> SqlHost<'w, PlaceId, SymId> for Vm<'_, '_, 'w, L> {
    fn session(&mut self) -> Option<&mut Session<'w>> {
        self.unit.sql.as_mut()
    }

    fn in_task(&self) -> bool {
        self.unit.cics.is_some()
    }

    fn program_id(&self) -> String {
        self.sym(self.p.id).to_owned()
    }

    fn text(&self, text: &SymId) -> String {
        self.sym(*text).to_owned()
    }

    fn place_pos(&self, place: PlaceId) -> Pos {
        self.pos(self.p.places[place as usize].at)
    }

    fn untyped(&mut self, abend: AbendId) -> Abend {
        self.abend(abend, None)
    }

    fn sink(&mut self, kind: &'static str, pos: Pos, operand: &str) {
        Vm::sink(self, kind, pos, operand);
    }
}
