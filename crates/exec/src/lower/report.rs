//! Report Writer (lir.md §9.6): the reports `compile::report` resolved, held in `Services.report`
//! with each SOURCE and SUM operand a comparand, each CONTROL a place, each VALUE and CODE a
//! constant, each data item a static place's id and each USE BEFORE REPORTING section a range; and
//! INITIATE, GENERATE, TERMINATE and SUPPRESS PRINTING as `Op::Report` naming them by index.

use super::{Lower, R, unsupported};
use crate::report::{self, generate_target};
use rt::lir::{self, ConstId, Op, RangeKind, ReportOp, Terminator};
use rt::report::{Control, Field, FieldContent, Group, Line, Origin, Report, Subtotal, Sum, Writer};
use syntax::Pos;
use syntax::ast::{Expr, Literal};
use syntax::report::ReportStmt;

impl Lower<'_> {
    pub(super) fn report_writer(&mut self) -> R<lir::ReportWriter> {
        let c = self.c;
        let writer = &c.report_writer;
        let mut reports = Vec::with_capacity(writer.reports.len());
        for r in &writer.reports {
            reports.push(self.report(r)?);
        }
        let print_switch = writer.print_switch.map(|i| self.report_item(i)).transpose()?;
        Ok(Writer { reports, print_switch })
    }

    fn report(&mut self, r: &report::Report) -> R<Report> {
        let code = r.code.as_ref().map(|lit| self.report_literal(lit, Pos::default())).transpose()?;
        let mut controls = Vec::with_capacity(r.controls.len());
        for c in &r.controls {
            controls.push(Control { reference: self.place(&c.reference, false)?, saved: c.saved, len: c.len });
        }
        let mut groups = Vec::with_capacity(r.groups.len());
        for g in &r.groups {
            groups.push(self.report_group(g)?);
        }
        let mut sums = Vec::with_capacity(r.sums.len());
        for s in &r.sums {
            sums.push(Sum { total: self.report_item(s.total)?, reset: s.reset });
        }
        let mut subtotals = Vec::with_capacity(r.subtotals.len());
        for st in &r.subtotals {
            subtotals.push(Subtotal { sum: st.sum, operand: self.report_operand(&st.operand, Pos::default())?, adding: st.adding.clone() });
        }
        Ok(Report {
            name: r.name.clone(),
            file: r.file,
            code,
            width: r.width,
            page: r.page,
            controls,
            groups,
            sums,
            subtotals,
            page_counter: self.report_item(r.page_counter)?,
            line_counter: self.report_item(r.line_counter)?,
            state: self.report_item(r.state)?,
            report_heading: r.report_heading,
            page_heading: r.page_heading,
            page_footing: r.page_footing,
            report_footing: r.report_footing,
            control_headings: r.control_headings.clone(),
            control_footings: r.control_footings.clone(),
            first_detail_written: r.first_detail_written,
        })
    }

    fn report_group(&mut self, g: &report::Group) -> R<Group> {
        let mut lines = Vec::with_capacity(g.lines.len());
        for l in &g.lines {
            let fields = l.fields.iter().map(|f| self.report_field(f)).collect::<R<_>>()?;
            lines.push(Line { number: l.number, fields });
        }
        let unprinted = g.unprinted.iter().map(|f| self.report_field(f)).collect::<R<_>>()?;
        let cross = g.cross.iter().map(|(s, o)| Ok((*s, self.origin(o)?))).collect::<R<_>>()?;
        let rolls = g.rolls.iter().map(|(s, o)| Ok((*s, self.origin(o)?))).collect::<R<_>>()?;
        let declarative = g.declarative.map(|s| self.span_range(s, RangeKind::UseBeforeReporting)).transpose()?;
        Ok(Group {
            name: g.name.clone(),
            kind: g.kind,
            level: g.level,
            next_group: g.next_group,
            lines,
            unprinted,
            cross,
            rolls,
            totals: g.totals.clone(),
            indicate: g.indicate,
            declarative,
        })
    }

    fn report_field(&mut self, f: &report::Field) -> R<Field> {
        let content = match &f.content {
            FieldContent::Source(e) => FieldContent::Source(self.report_operand(e, f.pos)?),
            FieldContent::Value(lit) => FieldContent::Value(self.report_literal(lit, f.pos)?),
            FieldContent::Sum(s) => FieldContent::Sum(*s),
            FieldContent::Program => FieldContent::Program,
        };
        Ok(Field { item: self.report_item(f.item)?, column: f.column, content, group_indicate: f.group_indicate, blank_when_zero: f.blank_when_zero, rounded: f.rounded, pos: f.pos })
    }

    fn origin(&mut self, o: &report::Origin) -> R<Origin> {
        Ok(match o {
            Origin::Source(e) => Origin::Source(self.report_operand(e, Pos::default())?),
            Origin::Value(lit) => Origin::Value(self.report_literal(lit, Pos::default())?),
            Origin::Total(t) => Origin::Total(*t),
        })
    }

    /// A SOURCE or SUM operand: an operand the writer reads with its storage, or an expression it
    /// evaluates as `expr_value` does.
    fn report_operand(&mut self, e: &Expr, pos: Pos) -> R<lir::Comparand> {
        Ok(self.comparand(e, pos)?.0)
    }

    fn report_literal(&mut self, lit: &Literal, pos: Pos) -> R<ConstId> {
        Ok(self.encoded_const(lit, pos)?.0)
    }

    /// The report writer's `item`: a data item at its offset in the slab, whole.
    fn report_item(&mut self, i: usize) -> R<usize> {
        let key = format!("report item {i}");
        if let Some(&id) = self.place_ids.get(&key) {
            return Ok(id as usize);
        }
        let layout = self.layout;
        let item = &layout.items[i];
        if item.linkage.is_some() || item.local {
            return unsupported("a report writer item outside WORKING-STORAGE", item.pos);
        }
        let name = self.sym(item.name.as_deref().unwrap_or("FILLER"));
        let place = lir::Place {
            base: lir::Base::Program,
            offset: item.offset,
            len: item.size,
            kind: item.kind,
            scaling: item.scaling,
            moved: Vec::new(),
            subscripts: Vec::new(),
            odo: Vec::new(),
            refmod: None,
            name,
            at: self.at(item.pos),
            numcheck: self.place_numcheck(i, item.pos),
        };
        let id = self.push_place(place, Some(i))?;
        self.place_ids.insert(key, id);
        Ok(id as usize)
    }

    /// One `Op::Report` per report an INITIATE or TERMINATE names, in turn, or the walker's abend
    /// for a name it cannot find, after the ops before it.
    pub(super) fn report_statement(&mut self, s: &ReportStmt, pos: Pos) -> R<()> {
        let c = self.c;
        let reports = &c.report_writer.reports;
        let refused = |l: &mut Self, message: String| {
            let abend = l.ironwork(&message)?;
            l.end(Terminator::Abend(abend), pos)
        };
        match s {
            ReportStmt::Initiate { reports: names, pos: _ } | ReportStmt::Terminate { reports: names, pos: _ } => {
                for n in names {
                    let Some(ri) = reports.iter().position(|r| r.name == *n) else { return refused(self, format!("{n} is not a report of this program")) };
                    let ri = ri as u32;
                    let op = if matches!(s, ReportStmt::Initiate { .. }) { ReportOp::Initiate(ri) } else { ReportOp::Terminate(ri) };
                    self.op(Op::Report(op), pos)?;
                }
                Ok(())
            }
            ReportStmt::Generate { name, qualifier, pos: _ } => match generate_target(reports, name, qualifier.as_deref()) {
                Some((ri, detail)) => self.op(Op::Report(ReportOp::Generate { report: ri as u32, detail: detail.map(|d| d as u32) }), pos),
                None => refused(self, format!("GENERATE {name}: no such DETAIL group or report")),
            },
            ReportStmt::Suppress { pos: _ } => self.op(Op::Report(ReportOp::Suppress), pos),
        }
    }
}
