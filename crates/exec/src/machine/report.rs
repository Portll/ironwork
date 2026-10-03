//! INITIATE, GENERATE, TERMINATE and SUPPRESS PRINTING, whose semantics are `rt::report`: the
//! reports and DETAIL groups they name, and the SOURCE, SUM and CONTROL operands the walker
//! evaluates for it.

use super::*;
use crate::files::Format;
use rt::lir::Spacing;
use rt::report::{ReportFile, ReportHost, ReportOp, UseEnd};
use std::io::Write;
use syntax::report::ReportStmt;

impl<'p> Machine<'p, '_, '_> {
    pub(super) fn report_statement(&mut self, s: &'p ReportStmt) -> R<Flow> {
        let writer = self.report_writer;
        let run = |m: &mut Self, op, pos| rt::report::run(m, writer, op, pos);
        let ended = match s {
            ReportStmt::Initiate { reports, pos } | ReportStmt::Terminate { reports, pos } => {
                let mut ended = None;
                for n in reports {
                    let ri = self.report_named(n, *pos)? as u32;
                    let op = if matches!(s, ReportStmt::Initiate { .. }) { ReportOp::Initiate(ri) } else { ReportOp::Terminate(ri) };
                    ended = run(self, op, *pos)?;
                    if ended.is_some() {
                        break;
                    }
                }
                ended
            }
            ReportStmt::Generate { name, qualifier, pos } => {
                let (ri, detail) = crate::report::generate_target(&writer.reports, name, qualifier.as_deref())
                    .ok_or_else(|| Abend::ironwork(format!("GENERATE {name}: no such DETAIL group or report"), *pos))?;
                run(self, ReportOp::Generate { report: ri as u32, detail: detail.map(|d| d as u32) }, *pos)?
            }
            ReportStmt::Suppress { pos } => run(self, ReportOp::Suppress, *pos)?,
        };
        Ok(match ended {
            Some(e) => Flow::End(e),
            None => Flow::Next,
        })
    }

    fn report_named(&self, name: &str, pos: Pos) -> R<usize> {
        self.report_writer.reports.iter().position(|r| r.name == name).ok_or_else(|| Abend::ironwork(format!("{name} is not a report of this program"), pos))
    }
}

impl<'p> ReportHost<'p, Expr, Ref, Literal, crate::report::Section> for Machine<'p, '_, '_> {
    fn value(&mut self, expr: &Expr, pos: Pos) -> R<Val> {
        self.expr_value(expr, pos)
    }

    fn operand(&mut self, expr: &Expr, pos: Pos) -> Option<R<(Val, Option<Loc>)>> {
        match expr {
            Expr::Operand(op) => Some(self.operand_with_loc(op, pos)),
            _ => None,
        }
    }

    fn literal(&mut self, value: &Literal, pos: Pos) -> R<Val> {
        self.literal_value(value, pos)
    }

    fn item(&self, item: usize) -> Loc {
        let it = &self.layout.items[item];
        Loc { offset: self.base + it.offset as usize, len: it.size as usize, kind: it.kind, item }
    }

    fn store_value(&mut self, dest: Loc, value: Val, rounded: bool, keep_on_size_error: bool, pos: Pos) -> R<bool> {
        store::store_value(&self.facts(), self.unit, dest, value, rounded, keep_on_size_error, pos)
    }

    fn store_checked(&mut self, dest: Loc, value: &Fixed, pos: Pos) -> R<bool> {
        store::store_fixed_checked(&self.facts(), self.unit, dest, value, false, true, pos)
    }

    fn report_file(&mut self, k: usize) -> ReportFile {
        ReportFile {
            area: self.area(k),
            variable: self.unit.programs[self.me].files[k].as_ref().is_some_and(|f| f.format == Format::Variable),
            reserved: self.carriage[k].is_some_and(|c| c.reserved),
            record_min: self.program.files[k].record_min,
        }
    }

    fn write_line(&mut self, k: usize, loc: Loc, space: Spacing, pos: Pos) -> R<()> {
        self.write_stream(k, loc, false, space, pos)
    }

    fn use_before_reporting(&mut self, &(first, last): &crate::report::Section, pos: Pos) -> R<UseEnd> {
        self.unit.enter(pos)?;
        self.uses.arrival = declaratives::Arrival::Use;
        let flow = self.run_paragraphs(first, last);
        self.unit.depth -= 1;
        Ok(match flow? {
            Flow::End(e) => UseEnd::End(e),
            Flow::GoTo(_) => UseEnd::GoTo,
            leaving @ (Flow::Resume(..) | Flow::Return(_)) => {
                self.uses.leaving = Some(leaving);
                UseEnd::Left
            }
            _ => UseEnd::Completed,
        })
    }

    fn err(&mut self) -> &mut dyn Write {
        &mut *self.unit.err
    }
}
