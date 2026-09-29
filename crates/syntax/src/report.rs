//! The REPORT SECTION as written: report descriptions with their report groups, the INITIATE,
//! GENERATE, TERMINATE and SUPPRESS statements, and USE BEFORE REPORTING procedures.

use crate::Pos;
use crate::ast::{Expr, Literal, Ref, SignClause};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReportWriter {
    pub reports: Vec<Report>,
    pub uses: Vec<UseBeforeReporting>,
    /// Paragraphs before this index are the DECLARATIVES, which run only when invoked.
    pub procedure_start: usize,
}

/// An RD entry and the report groups that follow it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub name: String,
    /// CODE literal: prefixed to every record the report writes.
    pub code: Option<Literal>,
    /// CONTROL identifiers, major to minor. FINAL is always the highest level, written or not.
    pub controls: Vec<Ref>,
    pub page: Option<u32>,
    pub heading: Option<u32>,
    pub first_detail: Option<u32>,
    pub last_detail: Option<u32>,
    pub footing: Option<Footing>,
    pub line_limit: Option<u32>,
    pub groups: Vec<Group>,
    pub pos: Pos,
}

/// FOOTING, or LAST CONTROL FOOTING: a line, or lines below LAST DETAIL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Footing {
    Line(u32),
    Plus(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControlName {
    Final,
    Item(Ref),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupType {
    ReportHeading,
    PageHeading,
    /// CONTROL HEADING, for the control named or the only one there is.
    ControlHeading(Option<ControlName>),
    Detail,
    ControlFooting(Option<ControlName>),
    PageFooting,
    ReportFooting,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SumClause {
    pub operands: Vec<Ref>,
    /// UPON: the DETAIL groups whose GENERATE adds the operands.
    pub upon: Vec<Ref>,
    pub reset: Option<ControlName>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Content {
    Source(Expr),
    Value(Literal),
    Sum(Vec<SumClause>),
}

/// A report group: its 01-level entry, then every entry below it, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub name: Option<String>,
    pub kind: GroupType,
    pub next_group: Option<NextGroup>,
    pub entries: Vec<Entry>,
    pub pos: Pos,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub level: u8,
    pub name: Option<String>,
    pub line: Option<LineNumber>,
    pub column: Option<ColumnNumber>,
    pub picture: Option<String>,
    pub content: Option<Content>,
    pub rounded: bool,
    pub group_indicate: bool,
    pub blank_when_zero: bool,
    pub justified: bool,
    pub sign: Option<SignClause>,
    pub pos: Pos,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReportStmt {
    Initiate { reports: Vec<String>, pos: Pos },
    /// GENERATE a DETAIL group, or a report for summary reporting; `qualifier` is IN report-name.
    Generate { name: String, qualifier: Option<String>, pos: Pos },
    Terminate { reports: Vec<String>, pos: Pos },
    Suppress { pos: Pos },
}

/// A DECLARATIVES section that runs just before a report group is produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UseBeforeReporting {
    /// The section's header among the program's paragraphs.
    pub section: usize,
    pub group: String,
    pub qualifier: Option<String>,
    pub pos: Pos,
}
