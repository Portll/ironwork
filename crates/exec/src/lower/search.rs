//! SEARCH and SEARCH ALL (lir.md §9.1) as `Machine::search` runs them: the table's current count,
//! its first index or the VARYING item, then a serial loop of blocks or one binary search op. A
//! serial SEARCH of an OCCURS DEPENDING ON table holds its count from the statement's start.

use super::flow::Ctx;
use super::{Lower, R, unsupported};
use crate::layout::Resolved;
use crate::machine::{flatten_and, key_term};
use rt::lir::{self, Count, IntExpr, Op, PlaceId, SearchAllPlan, SearchKey, Terminator};
use rt::storage::Kind;
use syntax::Pos;
use syntax::ast::{BinOp, Expr, Literal, Operand, Ref, Search, Stmt};

impl Lower<'_> {
    pub(super) fn search(&mut self, se: &Search, pos: Pos, ctx: &Ctx) -> R<()> {
        let layout = self.layout;
        let t = match layout.resolve(&se.table.name, &se.table.qualifiers, se.table.pos) {
            Ok(Resolved::Item(t)) => t,
            Ok(Resolved::Condition(_)) => {
                let abend = self.ironwork(&format!("SEARCH {}: not a table", se.table.name))?;
                return self.end(Terminator::Abend(abend), pos);
            }
            Err(e) => {
                let abend = self.ironwork(&e.message)?;
                return self.end(Terminator::Abend(abend), se.table.pos);
            }
        };
        let table = &layout.items[t];
        let index = match (&se.varying, table.index_names.first()) {
            (_, Some(name)) => Some(Ref { name: name.clone(), qualifiers: Vec::new(), subscripts: Vec::new(), refmod: None, pos }),
            (Some(v), None) => Some(v.clone()),
            (None, None) => None,
        };
        let count = match self.count(t, pos)? {
            Count::Odo(odo) if !se.all || index.is_none() => {
                let temp = self.temp(pos)?;
                self.op(Op::SetCount(temp, odo), pos)?;
                Count::Temp(temp)
            }
            count => count,
        };
        let Some(index) = index else {
            let abend = self.ironwork(&format!("SEARCH {}: the table has no INDEXED BY", se.table.name))?;
            return self.end(Terminator::Abend(abend), pos);
        };
        let index_place = self.place(&index, false)?;
        let at_end = se.at_end.as_deref().unwrap_or_default();
        let join = self.new_block()?;
        if se.all {
            let Some((cond, body)) = se.whens.first() else { return unsupported("SEARCH ALL without a WHEN phrase", pos) };
            let mut terms = Vec::new();
            flatten_and(cond, &mut terms);
            let mut keys = Vec::new();
            for (ascending, key) in &table.keys {
                let Some((subject, value)) = key_term(&terms, &key.name) else { continue };
                let (key, value, how) = self.comparison(subject, value, pos)?;
                keys.push(SearchKey { ascending: *ascending, key, value, how });
            }
            let store = self.store_plan(self.kind_of(index_place), self.place_items[index_place as usize])?;
            let plan = SearchAllPlan { index: index_place, store, count, keys };
            let id = super::push(&mut self.plans.search_all, plan, "SEARCH ALL plans")?;
            self.op(Op::SearchAll(id), pos)?;
            let (found, run, end) = (self.new_block()?, self.new_block()?, self.new_block()?);
            self.end(Terminator::Select(vec![found, end]), pos)?;
            self.switch(found)?;
            let test = self.test(cond, pos)?;
            self.branch(test, run, end, pos)?;
            self.body(run, body, join, pos, ctx)?;
            self.body(end, at_end, join, pos, ctx)?;
            return self.switch(join);
        }
        let varying = se.varying.as_ref().filter(|v| v.name != index.name);
        let mut stepped = vec![(index_place, &index)];
        if let Some(v) = varying {
            stepped.push((self.place(v, false)?, v));
        }
        for &(place, r) in &stepped {
            if !matches!(self.kind_of(place), Kind::Index | Kind::Zoned { scale: 0, .. } | Kind::Packed { scale: 0, .. } | Kind::Binary { scale: 0, .. }) {
                return unsupported("SEARCH VARYING an item that is not an index or an integer", r.pos);
            }
        }
        // The loop reads the index twice at each step, where the walker reads it once.
        if self.read_tested(index_place) {
            return unsupported("NUMCHECK of a serial SEARCH's index", pos);
        }
        let steps = stepped.iter().map(|&(place, r)| Ok((place, self.plus_one(r, pos)?))).collect::<R<Vec<(PlaceId, IntExpr)>>>()?;
        let (head, end) = (self.new_block()?, self.new_block()?);
        self.jump(head, pos)?;
        self.switch(head)?;
        let within = self.cond(lir::Cond::InTable { index: index_place, count })?;
        let first = self.new_block()?;
        self.end(Terminator::Branch { cond: within, then: first, otherwise: end }, pos)?;
        self.switch(first)?;
        for (cond, body) in &se.whens {
            let (run, next) = (self.new_block()?, self.new_block()?);
            let test = self.test(cond, pos)?;
            self.branch(test, run, next, pos)?;
            self.body(run, body, join, pos, ctx)?;
            self.switch(next)?;
        }
        for (target, value) in steps {
            self.op(Op::SetInt { target, value }, pos)?;
        }
        self.jump(head, pos)?;
        self.body(end, at_end, join, pos, ctx)?;
        self.switch(join)
    }

    fn body(&mut self, block: lir::BlockId, stmts: &[Stmt], join: lir::BlockId, pos: Pos, ctx: &Ctx) -> R<()> {
        self.switch(block)?;
        self.statements(stmts, ctx)?;
        self.jump(join, pos)
    }

    /// `occurrences`: the table's OCCURS, or its DEPENDING ON object's value kept within it.
    fn count(&mut self, t: usize, pos: Pos) -> R<Count> {
        let table = &self.layout.items[t];
        let Some(object) = &table.depending_on else { return Ok(Count::Fixed(table.occurs)) };
        let value = self.int_expr(&Expr::Operand(Operand::Ref(object.clone())), pos)?;
        Ok(Count::Odo(lir::Odo { object: value, max: table.occurs, element: table.size, check: self.c.ssrange }))
    }

    /// `r` + 1, as the walker's `integer(r) + 1` for an index or integer item.
    fn plus_one(&mut self, r: &Ref, pos: Pos) -> R<IntExpr> {
        let one = Expr::Operand(Operand::Literal(Literal::Number("1".into())));
        self.int_expr(&Expr::Bin(Box::new(Expr::Operand(Operand::Ref(r.clone()))), BinOp::Add, Box::new(one)), pos)
    }
}
