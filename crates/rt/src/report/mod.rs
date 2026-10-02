//! The report writer (lir.md §9.6): the reports `compile::report` resolves, as Enterprise COBOL's
//! Report Writer Precompiler lays them out, and INITIATE, GENERATE, TERMINATE and SUPPRESS PRINTING
//! over them ([`run`]). The model is generic over the executor's handles: `X` a SOURCE or SUM
//! operand's expression, `C` a CONTROL item, `V` a VALUE or CODE literal, `U` a USE BEFORE
//! REPORTING procedure: the LIR's comparands and ids by default, the walker's own AST and paragraph
//! spans in the interpreter.

mod run;

pub use run::{ReportFile, ReportHost, UseEnd, run};

use crate::lir::{Comparand, ConstId, PlaceId, RangeId};
use crate::vocab::Pos;
use crate::{codec_enum, codec_struct};

/// Offsets in a report's state item. The item starts as X'00', so every flag starts false.
pub mod state {
    pub const INITIATED: usize = 0;
    pub const GENERATED: usize = 1;
    /// A page has begun: output has gone to the file since INITIATE.
    pub const STARTED: usize = 2;
    /// The PAGE HEADING is still to come on the current page, below a REPORT HEADING.
    pub const HEADING_DUE: usize = 3;
    pub const BODY_ON_PAGE: usize = 4;
    /// The current page holds the REPORT HEADING alone.
    pub const HEADING_ONLY: usize = 5;
    /// The line the file is at, 0 before the page's first line: a fullword.
    pub const VERTICAL: usize = 8;
    /// NEXT GROUP's absolute line, held until the next page: a fullword.
    pub const SAVED_NEXT_GROUP: usize = 12;
    /// One GROUP INDICATE flag per report group, then each control's value at the last GENERATE.
    pub const FLAGS: usize = 16;
}

/// FOOTING, or LAST CONTROL FOOTING: a line, or lines below LAST DETAIL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Footing {
    Line(u32),
    Plus(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NextGroup {
    Line(u32),
    Plus(u32),
    NextPage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineNumber {
    Line(u32),
    Plus(u32),
    /// NEXT PAGE, with the absolute line when one is written.
    NextPage(Option<u32>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnNumber {
    Left(u32),
    Plus(u32),
    Right(u32),
    Center(u32),
}

/// INITIATE, GENERATE, TERMINATE or SUPPRESS PRINTING, with the report and DETAIL group by index.
/// GENERATE of a report alone, for summary reporting, has no `detail`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportOp {
    Initiate(u32),
    Generate { report: u32, detail: Option<u32> },
    Terminate(u32),
    Suppress,
}

/// The reports of one program. Items (`usize`) are data items: by their index in the interpreter,
/// and once lowered by the id of a static place in the slab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Writer<X = Comparand, C = PlaceId, V = ConstId, U = RangeId> {
    pub reports: Vec<Report<X, C, V, U>>,
    /// PRINT-SWITCH, which SUPPRESS PRINTING sets.
    pub print_switch: Option<usize>,
}

impl<X, C, V, U> Default for Writer<X, C, V, U> {
    fn default() -> Self {
        Writer { reports: Vec::new(), print_switch: None }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report<X = Comparand, C = PlaceId, V = ConstId, U = RangeId> {
    pub name: String,
    pub file: usize,
    pub code: Option<V>,
    /// Bytes of a line: the record less the CODE, and under NOADV the control character.
    pub width: usize,
    pub page: Option<Page>,
    /// Level 1 is the most major control; level 0 is FINAL.
    pub controls: Vec<Control<C>>,
    pub groups: Vec<Group<X, V, U>>,
    pub sums: Vec<Sum>,
    /// SUM operands outside the REPORT SECTION, added by GENERATE.
    pub subtotals: Vec<Subtotal<X>>,
    pub page_counter: usize,
    pub line_counter: usize,
    pub state: usize,
    pub report_heading: Option<usize>,
    pub page_heading: Option<usize>,
    pub page_footing: Option<usize>,
    pub report_footing: Option<usize>,
    /// The CONTROL HEADING and CONTROL FOOTING of each level, FINAL first.
    pub control_headings: Vec<Option<usize>>,
    pub control_footings: Vec<Option<usize>>,
    /// FIRST DETAIL when written, so a PAGE HEADING below a REPORT HEADING can be seen not to fit.
    pub first_detail_written: Option<i64>,
}

/// The page regions, with the precompiler's defaults applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Page {
    pub limit: i64,
    pub heading: i64,
    pub first_detail: i64,
    pub last_detail: i64,
    pub footing: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Control<C = PlaceId> {
    pub reference: C,
    /// Where its value at the last GENERATE is kept in the state item.
    pub saved: usize,
    pub len: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupKind {
    ReportHeading,
    PageHeading,
    ControlHeading,
    Detail,
    ControlFooting,
    PageFooting,
    ReportFooting,
}

impl GroupKind {
    pub fn is_body(self) -> bool {
        matches!(self, Self::ControlHeading | Self::Detail | Self::ControlFooting)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group<X = Comparand, V = ConstId, U = RangeId> {
    pub name: Option<String>,
    pub kind: GroupKind,
    /// The control level of a CONTROL HEADING or FOOTING.
    pub level: usize,
    pub next_group: Option<NextGroup>,
    pub lines: Vec<Line<X, V>>,
    /// Fields with no COLUMN, which are set but not printed.
    pub unprinted: Vec<Field<X, V>>,
    /// Cross-footing: SUM entries of this group adding entries of this group, in dependency order.
    pub cross: Vec<(usize, Origin<X, V>)>,
    /// Rolling forward: SUM entries elsewhere adding entries of this group.
    pub rolls: Vec<(usize, Origin<X, V>)>,
    /// The SUM entries defined in this group, reset after it unless RESET defers them.
    pub totals: Vec<usize>,
    pub indicate: Option<usize>,
    /// The USE BEFORE REPORTING section: its first and last paragraph.
    pub declarative: Option<U>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line<X = Comparand, V = ConstId> {
    pub number: LineNumber,
    pub fields: Vec<Field<X, V>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field<X = Comparand, V = ConstId> {
    pub item: usize,
    /// First byte in the line.
    pub column: usize,
    pub content: FieldContent<X, V>,
    pub group_indicate: bool,
    /// BLANK WHEN ZERO on an unedited numeric PICTURE, which the field applies after the MOVE.
    pub blank_when_zero: bool,
    pub rounded: bool,
    pub pos: Pos,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldContent<X = Comparand, V = ConstId> {
    Source(X),
    Value(V),
    Sum(usize),
    /// No SOURCE, VALUE or SUM: the program's own statements fill the field.
    Program,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sum {
    pub total: usize,
    /// RESET ON: the control level whose break resets the total instead.
    pub reset: Option<usize>,
}

/// What an entry adds to a total when its group is produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin<X = Comparand, V = ConstId> {
    Source(X),
    Value(V),
    Total(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subtotal<X = Comparand> {
    pub sum: usize,
    pub operand: X,
    pub adding: Adding,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Adding {
    EveryGenerate,
    /// UPON: only a GENERATE of one of these DETAIL groups adds.
    Upon(Vec<usize>),
    /// SOURCE SUM correlation: the DETAIL groups that have the operand as a SOURCE.
    Correlated(Vec<usize>),
}

/// The rows of a relative group from the line before its first to its last.
pub fn span<X, V, U>(g: &Group<X, V, U>) -> i64 {
    g.lines.iter().map(|l| if let LineNumber::Plus(k) = l.number { k as i64 } else { 0 }).sum()
}

/// The report and DETAIL group a GENERATE names: a report alone for summary reporting.
pub fn generate_target<X, C, V, U>(reports: &[Report<X, C, V, U>], name: &str, qualifier: Option<&str>) -> Option<(usize, Option<usize>)> {
    if qualifier.is_none()
        && let Some(ri) = reports.iter().position(|r| r.name == name)
    {
        return Some((ri, None));
    }
    let mut found = reports
        .iter()
        .enumerate()
        .filter(|(_, r)| qualifier.is_none_or(|q| r.name == q))
        .flat_map(|(ri, r)| r.groups.iter().enumerate().filter(|(_, g)| g.kind == GroupKind::Detail && g.name.as_deref() == Some(name)).map(move |(gi, _)| (ri, Some(gi))));
    let first = found.next()?;
    found.next().is_none().then_some(first)
}

codec_enum!(ReportOp { Initiate(report) = 0, Generate { report, detail } = 1, Terminate(report) = 2, Suppress = 3 });
codec_enum!(NextGroup { Line(line) = 0, Plus(lines) = 1, NextPage = 2 });
codec_enum!(LineNumber { Line(line) = 0, Plus(lines) = 1, NextPage(line) = 2 });
codec_struct!(Writer { reports, print_switch });
codec_struct!(Report {
    name, file, code, width, page, controls, groups, sums, subtotals, page_counter, line_counter, state, report_heading,
    page_heading, page_footing, report_footing, control_headings, control_footings, first_detail_written,
});
codec_struct!(Page { limit, heading, first_detail, last_detail, footing });
codec_struct!(Control { reference, saved, len });
codec_enum!(GroupKind {
    ReportHeading = 0,
    PageHeading = 1,
    ControlHeading = 2,
    Detail = 3,
    ControlFooting = 4,
    PageFooting = 5,
    ReportFooting = 6,
});
codec_struct!(Group { name, kind, level, next_group, lines, unprinted, cross, rolls, totals, indicate, declarative });
codec_struct!(Line { number, fields });
codec_struct!(Field { item, column, content, group_indicate, blank_when_zero, rounded, pos });
codec_enum!(FieldContent { Source(expr) = 0, Value(value) = 1, Sum(sum) = 2, Program = 3 });
codec_struct!(Sum { total, reset });
codec_enum!(Origin { Source(expr) = 0, Value(value) = 1, Total(sum) = 2 });
codec_struct!(Subtotal { sum, operand, adding });
codec_enum!(Adding { EveryGenerate = 0, Upon(details) = 1, Correlated(details) = 2 });
