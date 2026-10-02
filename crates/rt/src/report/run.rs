//! The report writer at run time: INITIATE, GENERATE, TERMINATE and SUPPRESS PRINTING over a
//! report control area in WORKING-STORAGE. Each line is written with WRITE ... AFTER ADVANCING
//! through the report file, as the Report Writer Precompiler's generated code writes it, so each
//! record carries a printer control character
//! ([`numeric::assumptions::REPORT_LINE_WRITES`], [`numeric::assumptions::REPORT_CARRIAGE_CONTROL`]).

use super::{Adding, Field, FieldContent, Group, GroupKind, Line, LineNumber, NextGroup, Origin, Report, ReportOp, Writer, span, state};
use crate::abend::{Abend, AbendCode, Ending, Signal};
use crate::fixed::{align, places_of};
use crate::host::{self, Host};
use crate::lir::Spacing;
use crate::storage::{Kind, Loc, Val};
use crate::store::{self, ProgramFacts};
use crate::vocab::{Figurative, Pos};
use numeric::float;
use numeric::precision::{Fixed, Places};
use std::io::Write;
use zarch::ebcdic;

type R<T> = Result<T, Abend>;

/// How a USE BEFORE REPORTING procedure ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UseEnd {
    Completed,
    /// STOP RUN or GOBACK.
    End(Ending),
    GoTo,
    /// Control left for an active PERFORM's return point or resumed a statement; the executor
    /// carries that out once the report statement is abandoned.
    Left,
}

/// A report file as the report writer writes it.
#[derive(Clone, Copy, Debug)]
pub struct ReportFile {
    /// The record area in run-unit memory, and its length.
    pub area: (usize, usize),
    /// Open with variable-length records.
    pub variable: bool,
    /// NOADV: the record's first byte is the printer control character's.
    pub reserved: bool,
    pub record_min: Option<u32>,
}

/// What the report writer asks of the executor beyond [`Host`]: the values of its SOURCE, SUM and
/// VALUE operands, its data items, the stores of a SUM and a SOURCE, the report file, and running
/// a USE BEFORE REPORTING procedure.
pub trait ReportHost<'w, X: 'w, C: 'w, V: 'w, U: 'w>: Host<&'w C> {
    fn value(&mut self, expr: &X, pos: Pos) -> R<Val>;
    /// A lone operand's value and its storage, when it has any; None for any other expression.
    fn operand(&mut self, expr: &X, pos: Pos) -> Option<R<(Val, Option<Loc>)>>;
    fn literal(&mut self, value: &V, pos: Pos) -> R<Val>;
    fn item(&self, item: usize) -> Loc;
    /// Stores an arithmetic result; true when it was a size error.
    fn store_value(&mut self, dest: Loc, value: Val, rounded: bool, keep_on_size_error: bool, pos: Pos) -> R<bool>;
    /// A numeric store, not ROUNDED, that keeps the receiver on a size error; true when it was one.
    fn store_checked(&mut self, dest: Loc, value: &Fixed, pos: Pos) -> R<bool>;
    fn report_file(&mut self, k: usize) -> ReportFile;
    /// WRITE ... AFTER ADVANCING `space` of the record at `loc` to file `k`.
    fn write_line(&mut self, k: usize, loc: Loc, space: Spacing, pos: Pos) -> R<()>;
    /// Performs a USE BEFORE REPORTING procedure.
    fn use_before_reporting(&mut self, procedure: &U, pos: Pos) -> R<UseEnd>;
    fn err(&mut self) -> &mut dyn Write;
}

/// One report statement. A STOP RUN or GOBACK in a USE BEFORE REPORTING procedure ends the run.
pub fn run<'w, X, C, V, U, H: ReportHost<'w, X, C, V, U>>(x: &mut H, writer: &'w Writer<X, C, V, U>, op: ReportOp, pos: Pos) -> R<Option<Ending>> {
    let mut r = Reporting { x, w: writer };
    let done = match op {
        ReportOp::Initiate(ri) => r.initiate(ri as usize, pos),
        ReportOp::Terminate(ri) => r.terminate(ri as usize, pos),
        ReportOp::Generate { report, detail } => r.generate(report as usize, detail.map(|d| d as usize), pos),
        ReportOp::Suppress => r.set_print_switch(1, pos),
    };
    match done {
        Ok(()) => Ok(None),
        Err(Abend { code: AbendCode::Signal(Signal::StopRun), .. }) => Ok(Some(Ending::StopRun)),
        Err(Abend { code: AbendCode::Signal(Signal::GoBack), .. }) => Ok(Some(Ending::Goback)),
        Err(a) => Err(a),
    }
}

/// Why a report group is being produced, for NEXT GROUP and subtotalling.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Trigger {
    Detail,
    /// A CONTROL FOOTING of a control break at this level (0 at TERMINATE).
    Footing(usize),
    Other,
}

fn lines(n: i64) -> Spacing {
    Spacing::Lines(n.max(0) as u64)
}

fn signal(signal: Signal, pos: Pos) -> Abend {
    Abend { code: AbendCode::Signal(signal), message: String::new(), pos, file: None }
}

struct Reporting<'w, 'h, H, X, C, V, U> {
    x: &'h mut H,
    w: &'w Writer<X, C, V, U>,
}

impl<'w, X, C, V, U, H: ReportHost<'w, X, C, V, U>> Reporting<'w, '_, H, X, C, V, U> {
    fn report(&self, ri: usize) -> &'w Report<X, C, V, U> {
        let w = self.w;
        &w.reports[ri]
    }

    fn read(&mut self, loc: Loc, pos: Pos) -> R<Val> {
        host::read::<&'w C>(self.x, loc, pos)
    }

    fn state_offset(&self, ri: usize, at: usize) -> usize {
        self.x.item(self.report(ri).state).offset + at
    }

    fn flag(&mut self, ri: usize, at: usize) -> bool {
        let offset = self.state_offset(ri, at);
        self.x.mem()[offset] != 0
    }

    fn set_flag(&mut self, ri: usize, at: usize, on: bool) {
        let offset = self.state_offset(ri, at);
        self.x.mem()[offset] = on as u8;
    }

    fn fullword(&mut self, ri: usize, at: usize) -> i64 {
        let offset = self.state_offset(ri, at);
        i32::from_be_bytes(self.x.mem()[offset..offset + 4].try_into().unwrap()) as i64
    }

    fn set_fullword(&mut self, ri: usize, at: usize, value: i64) {
        let offset = self.state_offset(ri, at);
        self.x.mem()[offset..offset + 4].copy_from_slice(&(value.clamp(i32::MIN as i64, i32::MAX as i64) as i32).to_be_bytes());
    }

    fn counter(&mut self, item: usize, pos: Pos) -> R<i64> {
        Ok(match self.read(self.x.item(item), pos)? {
            Val::Num(f) => align(&f, 0, false).and_then(|m| m.to_u128()).map_or(0, |m| if f.negative { -(m as i64) } else { m as i64 }),
            _ => 0,
        })
    }

    fn set_counter(&mut self, item: usize, value: i64, pos: Pos) -> R<()> {
        let loc = self.x.item(item);
        self.x.store_fixed(loc, &Fixed::new(value as i128, Places::new(18, 0)), pos)
    }

    fn line_counter(&mut self, ri: usize, pos: Pos) -> R<i64> {
        self.counter(self.report(ri).line_counter, pos)
    }

    fn set_line_counter(&mut self, ri: usize, value: i64, pos: Pos) -> R<()> {
        self.set_counter(self.report(ri).line_counter, value, pos)
    }

    fn set_print_switch(&mut self, value: i64, pos: Pos) -> R<()> {
        match self.w.print_switch {
            Some(item) => self.set_counter(item, value, pos),
            None => Ok(()),
        }
    }

    fn arm_indicators(&mut self, ri: usize) {
        for g in &self.report(ri).groups {
            if let Some(flag) = g.indicate {
                self.set_flag(ri, state::FLAGS + flag, true);
            }
        }
    }

    fn zero_total(&mut self, ri: usize, sum: usize, pos: Pos) -> R<()> {
        let loc = self.x.item(self.report(ri).sums[sum].total);
        self.x.store_fixed(loc, &Fixed::new(0, Places::new(1, 0)), pos)
    }

    fn initiate(&mut self, ri: usize, pos: Pos) -> R<()> {
        let r = self.report(ri);
        let at = self.state_offset(ri, 0);
        let len = self.x.item(r.state).len;
        self.x.mem()[at..at + len].fill(0);
        self.set_counter(r.page_counter, 1, pos)?;
        self.set_counter(r.line_counter, 0, pos)?;
        for s in 0..r.sums.len() {
            self.zero_total(ri, s, pos)?;
        }
        self.set_flag(ri, state::INITIATED, true);
        self.arm_indicators(ri);
        Ok(())
    }

    /// See [`numeric::assumptions::REPORT_OUT_OF_ORDER`] for a GENERATE before INITIATE.
    fn generate(&mut self, ri: usize, detail: Option<usize>, pos: Pos) -> R<()> {
        let r = self.report(ri);
        if !self.flag(ri, state::INITIATED) {
            let _ = writeln!(self.x.err(), "ironwork: {pos}: report writer run-time error 14: GENERATE for report {} before its INITIATE; it is initiated now", r.name);
            self.initiate(ri, pos)?;
        }
        if !self.flag(ri, state::GENERATED) {
            self.set_flag(ri, state::GENERATED, true);
            if let Some(g) = r.report_heading {
                self.produce(ri, g, Trigger::Other, pos)?;
            }
            for g in r.control_headings.iter().flatten() {
                self.produce(ri, *g, Trigger::Other, pos)?;
            }
        } else if let Some(level) = self.control_break(ri)? {
            self.break_at(ri, level, pos)?;
        }
        match detail {
            Some(g) => self.produce(ri, g, Trigger::Detail, pos)?,
            None => self.subtotal(ri, None, pos)?,
        }
        self.save_controls(ri)
    }

    fn terminate(&mut self, ri: usize, pos: Pos) -> R<()> {
        let r = self.report(ri);
        if !self.flag(ri, state::INITIATED) {
            return Ok(());
        }
        if self.flag(ri, state::GENERATED) {
            let current = self.swap_in_saved_controls(ri)?;
            for level in (0..r.control_footings.len()).rev() {
                if let Some(g) = r.control_footings[level] {
                    self.produce(ri, g, Trigger::Footing(0), pos)?;
                }
                if level > 0 {
                    self.reset_totals_on(ri, level, pos)?;
                }
            }
            self.restore_controls(&current);
            if r.page.is_some() && self.flag(ri, state::STARTED) && !self.flag(ri, state::HEADING_ONLY)
                && let Some(g) = r.page_footing
            {
                self.produce(ri, g, Trigger::Other, pos)?;
            }
            if let Some(g) = r.report_footing {
                self.produce(ri, g, Trigger::Other, pos)?;
            }
        }
        self.set_flag(ri, state::INITIATED, false);
        Ok(())
    }

    fn control_values(&mut self, ri: usize) -> R<Vec<(Loc, Vec<u8>)>> {
        let mut out = Vec::new();
        for c in &self.report(ri).controls {
            let loc = self.x.locate(&c.reference, false)?;
            out.push((loc, store::bytes(self.x.mem(), loc).to_vec()));
        }
        Ok(out)
    }

    /// The most major control whose value differs from the last GENERATE's, by its level.
    fn control_break(&mut self, ri: usize) -> R<Option<usize>> {
        let values = self.control_values(ri)?;
        for (i, ((_, value), c)) in values.iter().zip(&self.report(ri).controls).enumerate() {
            let saved = self.state_offset(ri, c.saved);
            if self.x.mem()[saved..saved + c.len.min(value.len())] != value[..c.len.min(value.len())] {
                return Ok(Some(i + 1));
            }
        }
        Ok(None)
    }

    fn save_controls(&mut self, ri: usize) -> R<()> {
        let values = self.control_values(ri)?;
        for ((_, value), c) in values.iter().zip(&self.report(ri).controls) {
            let saved = self.state_offset(ri, c.saved);
            let n = c.len.min(value.len());
            self.x.mem()[saved..saved + n].copy_from_slice(&value[..n]);
        }
        Ok(())
    }

    /// Puts each control's value at the last GENERATE back in the control, for CONTROL FOOTING
    /// time; returns the values it replaced.
    fn swap_in_saved_controls(&mut self, ri: usize) -> R<Vec<(Loc, Vec<u8>)>> {
        let current = self.control_values(ri)?;
        for ((loc, value), c) in current.iter().zip(&self.report(ri).controls) {
            let saved = self.state_offset(ri, c.saved);
            let n = c.len.min(value.len());
            let mem = self.x.mem();
            let before = mem[saved..saved + n].to_vec();
            mem[loc.offset..loc.offset + n].copy_from_slice(&before);
        }
        Ok(current)
    }

    fn restore_controls(&mut self, values: &[(Loc, Vec<u8>)]) {
        for (loc, value) in values {
            store::write(self.x.mem(), *loc, value);
        }
    }

    /// CONTROL FOOTINGs minor to major up to the break's level under the values before the break,
    /// then CONTROL HEADINGs major to minor under the new ones.
    fn break_at(&mut self, ri: usize, level: usize, pos: Pos) -> R<()> {
        let r = self.report(ri);
        let current = self.swap_in_saved_controls(ri)?;
        for l in (level..r.control_footings.len()).rev() {
            if let Some(g) = r.control_footings[l] {
                self.produce(ri, g, Trigger::Footing(level), pos)?;
            }
            self.reset_totals_on(ri, l, pos)?;
        }
        self.restore_controls(&current);
        self.arm_indicators(ri);
        for l in level..r.control_headings.len() {
            if let Some(g) = r.control_headings[l] {
                self.produce(ri, g, Trigger::Other, pos)?;
            }
        }
        Ok(())
    }

    fn reset_totals_on(&mut self, ri: usize, level: usize, pos: Pos) -> R<()> {
        for (s, sum) in self.report(ri).sums.iter().enumerate() {
            if sum.reset == Some(level) {
                self.zero_total(ri, s, pos)?;
            }
        }
        Ok(())
    }

    /// Adds SUM operands from outside the REPORT SECTION: on a GENERATE of `detail`, or of the
    /// report alone for summary reporting ([`numeric::assumptions::REPORT_SOURCE_SUM_CORRELATION`]).
    fn subtotal(&mut self, ri: usize, detail: Option<usize>, pos: Pos) -> R<()> {
        for st in &self.report(ri).subtotals {
            let times = match (&st.adding, detail) {
                (Adding::EveryGenerate, _) => 1,
                (Adding::Upon(ds) | Adding::Correlated(ds), Some(d)) => ds.contains(&d) as usize,
                (Adding::Upon(_), None) => 0,
                (Adding::Correlated(ds), None) => ds.len(),
            };
            for _ in 0..times {
                let value = self.x.value(&st.operand, pos)?;
                self.add_to_total(ri, st.sum, value, pos)?;
            }
        }
        Ok(())
    }

    fn accumulate(&mut self, ri: usize, sum: usize, origin: &Origin<X, V>, pos: Pos) -> R<()> {
        let value = match origin {
            Origin::Source(e) => match self.x.value(e, pos) {
                Err(a) if a.code.zero_divisor() => {
                    let _ = writeln!(self.x.err(), "ironwork: {pos}: report writer run-time error 10: a SOURCE expression divided by zero; nothing was added to the total");
                    return Ok(());
                }
                other => other?,
            },
            Origin::Value(lit) => self.x.literal(lit, pos)?,
            Origin::Total(t) => self.read(self.x.item(self.report(ri).sums[*t].total), pos)?,
        };
        self.add_to_total(ri, sum, value, pos)
    }

    /// ADD with ON SIZE ERROR, as the precompiler generates it: a total that would overflow is
    /// left as it was, and the run-time error is logged ([`numeric::assumptions::REPORT_SUM_OVERFLOW`]).
    fn add_to_total(&mut self, ri: usize, sum: usize, value: Val, pos: Pos) -> R<()> {
        let loc = self.x.item(self.report(ri).sums[sum].total);
        let addend = match value {
            Val::Num(f) => f,
            Val::Float(h) => float::to_receiver(h, places_of(loc.kind)).0,
            Val::Fig(Figurative::Zero) => Fixed::new(0, Places::new(1, 0)),
            _ => return Err(Abend::ironwork("a SUM operand that is not numeric", pos)),
        };
        let Val::Num(current) = self.read(loc, pos)? else { return Err(Abend::ironwork("a SUM total that is not numeric", pos)) };
        let dmax = places_of(loc.kind).dec.max(addend.places.dec);
        let total = current.add(addend, dmax, self.x.facts().options().arith).map_err(|_| Abend::ironwork("a SUM wider than 256 bits", pos))?;
        if self.x.store_checked(loc, &total, pos)? {
            let name = &self.report(ri).name;
            let _ = writeln!(self.x.err(), "ironwork: {pos}: report writer run-time error 11: a SUM total of report {name} overflowed; the value was not added");
        }
        Ok(())
    }

    /// One report group, as the precompiler's GENERATE processing cycle produces it: totals first
    /// ([`numeric::assumptions::REPORT_TOTALS_BEFORE_PAGE_FIT`]), then USE BEFORE REPORTING, then
    /// the lines unless it suppressed them ([`numeric::assumptions::REPORT_SUPPRESS_PRINTING`]).
    fn produce(&mut self, ri: usize, gi: usize, trigger: Trigger, pos: Pos) -> R<()> {
        let r = self.report(ri);
        let g = &r.groups[gi];
        for (s, origin) in &g.cross {
            self.accumulate(ri, *s, origin, pos)?;
        }
        if trigger == Trigger::Detail {
            self.subtotal(ri, Some(gi), pos)?;
        }
        for (s, origin) in &g.rolls {
            self.accumulate(ri, *s, origin, pos)?;
        }
        let suppressed = match &g.declarative {
            Some(procedure) => self.use_before_reporting(procedure, pos)?,
            None => false,
        };
        if !suppressed {
            for f in &g.unprinted {
                self.fill_field(ri, f)?;
            }
            if !g.lines.is_empty() {
                match g.kind {
                    GroupKind::ReportHeading => self.place_report_heading(ri, g, pos)?,
                    GroupKind::PageHeading => self.place_page_heading(ri, g, pos)?,
                    GroupKind::PageFooting => self.place_page_footing(ri, g, pos)?,
                    GroupKind::ReportFooting => self.place_report_footing(ri, g, pos)?,
                    _ => self.place_body(ri, g, pos)?,
                }
            }
            self.next_group(ri, g, trigger, pos)?;
        }
        for &s in &g.totals {
            if r.sums[s].reset.is_none() {
                self.zero_total(ri, s, pos)?;
            }
        }
        if !suppressed && let Some(flag) = g.indicate {
            self.set_flag(ri, state::FLAGS + flag, false);
        }
        Ok(())
    }

    /// Performs a USE BEFORE REPORTING section; true when it suppressed the group's printing.
    fn use_before_reporting(&mut self, procedure: &U, pos: Pos) -> R<bool> {
        match self.x.use_before_reporting(procedure, pos)? {
            UseEnd::End(Ending::StopRun) => return Err(signal(Signal::StopRun, pos)),
            UseEnd::End(_) => return Err(signal(Signal::GoBack, pos)),
            UseEnd::GoTo => return Err(Abend::ironwork("GO TO out of a USE BEFORE REPORTING procedure", pos)),
            UseEnd::Left => return Err(signal(Signal::DeclarativeExit, pos)),
            UseEnd::Completed => {}
        }
        let Some(item) = self.w.print_switch else { return Ok(false) };
        let suppressed = self.counter(item, pos)? != 0;
        self.set_counter(item, 0, pos)?;
        Ok(suppressed)
    }

    fn first_increment(g: &Group<X, V, U>) -> i64 {
        match g.lines.first().map(|l| l.number) {
            Some(LineNumber::Plus(k)) => k as i64,
            _ => 1,
        }
    }

    /// Prints a group's lines, the first on line `first` and each other where its LINE puts it.
    fn print_lines(&mut self, ri: usize, g: &'w Group<X, V, U>, first: i64, pos: Pos) -> R<()> {
        let mut target = first;
        for (i, line) in g.lines.iter().enumerate() {
            if i > 0 {
                target = match line.number {
                    LineNumber::Line(n) => n as i64,
                    LineNumber::Plus(k) => target + k as i64,
                    LineNumber::NextPage(_) => target + 1,
                };
            }
            self.print_line(ri, g, line, target, pos)?;
        }
        Ok(())
    }

    fn place_body(&mut self, ri: usize, g: &'w Group<X, V, U>, pos: Pos) -> R<()> {
        let r = self.report(ri);
        let Some(page) = r.page else {
            let lc = self.line_counter(ri, pos)?;
            return self.print_lines(ri, g, lc + Self::first_increment(g), pos);
        };
        self.open_page(ri, pos)?;
        let limit = if g.kind == GroupKind::ControlFooting { page.footing } else { page.last_detail };
        let lc = self.line_counter(ri, pos)?;
        let body = self.flag(ri, state::BODY_ON_PAGE);
        let fits = match g.lines[0].number {
            LineNumber::NextPage(_) => !body,
            LineNumber::Line(n) => lc < n as i64,
            LineNumber::Plus(k) => {
                let start = if body { lc + k as i64 } else { page.first_detail.max(lc + 1) };
                start + span(g) - k as i64 <= limit
            }
        };
        if !fits {
            self.advance_page(ri, pos)?;
        }
        let lc = self.line_counter(ri, pos)?;
        let first = match g.lines[0].number {
            LineNumber::Line(n) | LineNumber::NextPage(Some(n)) => n as i64,
            LineNumber::Plus(k) if self.flag(ri, state::BODY_ON_PAGE) => lc + k as i64,
            _ => page.first_detail.max(lc + 1),
        };
        self.print_lines(ri, g, first, pos)?;
        self.set_flag(ri, state::BODY_ON_PAGE, true);
        Ok(())
    }

    /// Before a body group: the first page begins, or the PAGE HEADING below a REPORT HEADING
    /// comes out, or a REPORT HEADING alone on its page gives way to the next page.
    fn open_page(&mut self, ri: usize, pos: Pos) -> R<()> {
        if self.flag(ri, state::HEADING_ONLY) {
            return self.advance_page(ri, pos);
        }
        if !self.flag(ri, state::STARTED) {
            self.set_flag(ri, state::STARTED, true);
            self.set_fullword(ri, state::VERTICAL, 0);
            self.set_flag(ri, state::HEADING_DUE, true);
        }
        if self.flag(ri, state::HEADING_DUE) {
            self.set_flag(ri, state::HEADING_DUE, false);
            if let Some(g) = self.report(ri).page_heading {
                self.produce(ri, g, Trigger::Other, pos)?;
            }
        }
        Ok(())
    }

    /// PAGE FOOTING, the next page, PAGE HEADING.
    fn advance_page(&mut self, ri: usize, pos: Pos) -> R<()> {
        let r = self.report(ri);
        if self.flag(ri, state::STARTED) && !self.flag(ri, state::HEADING_ONLY)
            && let Some(g) = r.page_footing
        {
            self.produce(ri, g, Trigger::Other, pos)?;
        }
        self.new_page(ri, pos)?;
        self.arm_indicators(ri);
        if let Some(g) = r.page_heading {
            self.produce(ri, g, Trigger::Other, pos)?;
        }
        let saved = self.fullword(ri, state::SAVED_NEXT_GROUP);
        if saved != 0 {
            self.set_line_counter(ri, saved, pos)?;
            self.set_fullword(ri, state::SAVED_NEXT_GROUP, 0);
        }
        Ok(())
    }

    fn new_page(&mut self, ri: usize, pos: Pos) -> R<()> {
        let r = self.report(ri);
        let page = self.counter(r.page_counter, pos)?;
        self.set_counter(r.page_counter, page + 1, pos)?;
        self.set_fullword(ri, state::VERTICAL, 0);
        self.set_line_counter(ri, 0, pos)?;
        for at in [state::HEADING_DUE, state::BODY_ON_PAGE, state::HEADING_ONLY] {
            self.set_flag(ri, at, false);
        }
        self.set_flag(ri, state::STARTED, true);
        Ok(())
    }

    fn place_report_heading(&mut self, ri: usize, g: &'w Group<X, V, U>, pos: Pos) -> R<()> {
        let r = self.report(ri);
        let Some(page) = r.page else {
            let lc = self.line_counter(ri, pos)?;
            return self.print_lines(ri, g, lc + Self::first_increment(g), pos);
        };
        self.set_flag(ri, state::STARTED, true);
        self.set_fullword(ri, state::VERTICAL, 0);
        self.set_flag(ri, state::HEADING_DUE, true);
        let first = match g.lines[0].number {
            LineNumber::Line(n) | LineNumber::NextPage(Some(n)) => n as i64,
            LineNumber::Plus(k) => page.heading - 1 + k as i64,
            LineNumber::NextPage(None) => page.heading,
        };
        self.print_lines(ri, g, first, pos)?;
        let lc = self.line_counter(ri, pos)?;
        let alone = matches!(g.next_group, Some(NextGroup::NextPage))
            || match r.page_heading.map(|h| &r.groups[h]) {
                Some(h) => match h.lines[0].number {
                    LineNumber::Line(n) => lc >= n as i64,
                    _ => r.first_detail_written.is_some_and(|fd| lc + span(h) >= fd),
                },
                None => false,
            };
        self.set_flag(ri, state::HEADING_ONLY, alone);
        Ok(())
    }

    fn place_page_heading(&mut self, ri: usize, g: &'w Group<X, V, U>, pos: Pos) -> R<()> {
        let Some(page) = self.report(ri).page else { return Ok(()) };
        let lc = self.line_counter(ri, pos)?;
        let first = match g.lines[0].number {
            LineNumber::Line(n) | LineNumber::NextPage(Some(n)) => n as i64,
            LineNumber::Plus(k) => (if lc > 0 { lc } else { page.heading - 1 }) + k as i64,
            LineNumber::NextPage(None) => page.heading,
        };
        self.print_lines(ri, g, first, pos)
    }

    fn place_page_footing(&mut self, ri: usize, g: &'w Group<X, V, U>, pos: Pos) -> R<()> {
        let Some(page) = self.report(ri).page else { return Ok(()) };
        let first = match g.lines[0].number {
            LineNumber::Line(n) | LineNumber::NextPage(Some(n)) => n as i64,
            LineNumber::Plus(k) => page.footing + k as i64,
            LineNumber::NextPage(None) => page.footing + 1,
        };
        self.print_lines(ri, g, first, pos)
    }

    /// The REPORT FOOTING below the last PAGE FOOTING, or on a page of its own when it will
    /// not fit there or says NEXT PAGE ([`numeric::assumptions::REPORT_NEW_PAGES`]).
    fn place_report_footing(&mut self, ri: usize, g: &'w Group<X, V, U>, pos: Pos) -> R<()> {
        let r = self.report(ri);
        let lc = self.line_counter(ri, pos)?;
        let Some(page) = r.page else {
            return self.print_lines(ri, g, lc + Self::first_increment(g), pos);
        };
        let started = self.flag(ri, state::STARTED);
        if !started {
            self.set_flag(ri, state::STARTED, true);
            self.set_fullword(ri, state::VERTICAL, 0);
        }
        let own_page = |m: &mut Self| if started { m.new_page(ri, pos) } else { Ok(()) };
        let first = match g.lines[0].number {
            LineNumber::NextPage(n) => {
                own_page(self)?;
                n.map_or(page.heading, |n| n as i64)
            }
            LineNumber::Line(n) => {
                if started && n as i64 <= lc {
                    own_page(self)?;
                }
                n as i64
            }
            LineNumber::Plus(k) => {
                let base = if !started { page.heading - 1 } else if r.page_footing.is_some() { lc } else { page.footing };
                if started && base + span(g) > page.limit {
                    own_page(self)?;
                    page.heading - 1 + k as i64
                } else {
                    base + k as i64
                }
            }
        };
        self.print_lines(ri, g, first, pos)
    }

    fn next_group(&mut self, ri: usize, g: &'w Group<X, V, U>, trigger: Trigger, pos: Pos) -> R<()> {
        let Some(next) = g.next_group else { return Ok(()) };
        if let (GroupKind::ControlFooting, Trigger::Footing(level)) = (g.kind, trigger)
            && g.level != level
        {
            return Ok(());
        }
        let lc = self.line_counter(ri, pos)?;
        let Some(page) = self.report(ri).page else {
            if let NextGroup::Plus(n) = next {
                self.set_line_counter(ri, lc + n as i64, pos)?;
            }
            return Ok(());
        };
        match (g.kind.is_body(), next) {
            (true, NextGroup::Plus(n)) => self.set_line_counter(ri, (lc + n as i64).min(page.footing), pos),
            (true, NextGroup::Line(n)) if lc < n as i64 => self.set_line_counter(ri, n as i64, pos),
            (true, NextGroup::Line(n)) => {
                self.set_fullword(ri, state::SAVED_NEXT_GROUP, n as i64);
                self.set_line_counter(ri, page.footing, pos)
            }
            (true, NextGroup::NextPage) => self.set_line_counter(ri, page.footing, pos),
            (false, NextGroup::Plus(n)) => self.set_line_counter(ri, lc + n as i64, pos),
            (false, NextGroup::Line(n)) if lc < n as i64 => self.set_line_counter(ri, n as i64, pos),
            (false, _) => Ok(()),
        }
    }

    /// Sets LINE-COUNTER to the line, fills its fields and writes it.
    fn print_line(&mut self, ri: usize, g: &'w Group<X, V, U>, line: &'w Line<X, V>, target: i64, pos: Pos) -> R<()> {
        let r = self.report(ri);
        self.set_line_counter(ri, target, pos)?;
        let indicate = g.indicate.is_none_or(|flag| self.flag(ri, state::FLAGS + flag));
        let mut text = vec![ebcdic::SPACE; r.width];
        let mut end = 0;
        for f in &line.fields {
            if f.group_indicate && !indicate {
                continue;
            }
            self.fill_field(ri, f)?;
            let loc = self.x.item(f.item);
            let n = loc.len.min(r.width.saturating_sub(f.column));
            text[f.column..f.column + n].copy_from_slice(&self.x.mem()[loc.offset..loc.offset + n]);
            end = end.max(f.column + n);
        }
        self.write_report_line(ri, &text, end, target, pos)
    }

    fn blank(&mut self, loc: Loc) {
        store::write(self.x.mem(), loc, &vec![ebcdic::SPACE; loc.len]);
    }

    /// Stores a field's SOURCE, VALUE or SUM, as MOVE (or COMPUTE, for an expression or ROUNDED)
    /// would ([`numeric::assumptions::REPORT_SOURCE_OVERFLOW`]).
    fn fill_field(&mut self, ri: usize, f: &'w Field<X, V>) -> R<()> {
        let dest = self.x.item(f.item);
        let pos = f.pos;
        match &f.content {
            FieldContent::Program => return Ok(()),
            FieldContent::Value(lit) => {
                let v = self.x.literal(lit, pos)?;
                self.x.assign(dest, v, None, pos)?;
            }
            FieldContent::Source(e) => match self.x.operand(e, pos) {
                Some(read) if !f.rounded => {
                    let (v, src) = read?;
                    self.x.assign(dest, v, src, pos)?;
                }
                Some(read) => {
                    let (v, _) = read?;
                    self.x.store_value(dest, v, true, false, pos)?;
                }
                None => {
                    let overflow = match self.x.value(e, pos) {
                        Err(a) if a.code.zero_divisor() => true,
                        Err(a) => return Err(a),
                        Ok(v) => self.x.store_value(dest, v, f.rounded, true, pos)?,
                    };
                    if overflow {
                        self.blank(dest);
                        let _ = writeln!(self.x.err(), "ironwork: {pos}: report writer run-time error 10: a SOURCE expression overflowed or divided by zero; the field is left blank");
                    }
                }
            },
            FieldContent::Sum(s) => {
                let total = self.x.item(self.report(ri).sums[*s].total);
                let v = self.read(total, pos)?;
                if f.rounded {
                    self.x.store_value(dest, v, true, false, pos)?;
                } else {
                    self.x.assign(dest, v, Some(total), pos)?;
                }
            }
        }
        if f.blank_when_zero && matches!(self.read(dest, pos)?, Val::Num(v) if v.magnitude.is_zero()) {
            self.blank(dest);
        }
        Ok(())
    }

    /// The WRITEs for one line: at the top of a page, a line 1 goes out AFTER ADVANCING PAGE, and
    /// a lower line after a blank record written AFTER ADVANCING PAGE.
    fn write_report_line(&mut self, ri: usize, text: &[u8], end: usize, target: i64, pos: Pos) -> R<()> {
        let paged = self.report(ri).page.is_some();
        let vertical = self.fullword(ri, state::VERTICAL);
        if paged && vertical == 0 {
            if target > 1 {
                self.write_report_record(ri, None, Spacing::Channel(1), pos)?;
                self.write_report_record(ri, Some((text, end)), lines(target - 1), pos)?;
            } else {
                self.write_report_record(ri, Some((text, end)), Spacing::Channel(1), pos)?;
            }
        } else {
            self.write_report_record(ri, Some((text, end)), lines(target - vertical), pos)?;
        }
        self.set_fullword(ri, state::VERTICAL, target.max(1));
        Ok(())
    }

    /// One record through the report file's record area: the CODE, then the line; all spaces for
    /// the blank record at the top of a page. Under NOADV both follow the control character's byte.
    /// A variable-length record ends after its last field.
    fn write_report_record(&mut self, ri: usize, line: Option<(&[u8], usize)>, space: Spacing, pos: Pos) -> R<()> {
        let r = self.report(ri);
        let k = r.file;
        let file = self.x.report_file(k);
        let (offset, size) = file.area;
        let mut record = vec![ebcdic::SPACE; size];
        let reserved = usize::from(file.reserved).min(size);
        let shortest = (file.record_min.unwrap_or(1) as usize).max(reserved);
        let mut len = size;
        if let Some((text, end)) = line {
            let code = match &r.code {
                Some(lit) => match self.x.literal(lit, pos)? {
                    Val::Bytes(b) => b,
                    _ => Vec::new(),
                },
                None => Vec::new(),
            };
            let c = reserved + code.len().min(size - reserved);
            record[reserved..c].copy_from_slice(&code[..c - reserved]);
            let n = text.len().min(size - c);
            record[c..c + n].copy_from_slice(&text[..n]);
            if file.variable {
                len = (c + end.min(n)).max(shortest).min(size);
            }
        } else if file.variable {
            len = shortest.min(size);
        }
        self.x.mem()[offset..offset + size].copy_from_slice(&record);
        let loc = Loc { offset, len, kind: Kind::Alnum { justified: false }, item: usize::MAX };
        self.x.write_line(k, loc, space, pos)
    }
}
