//! The report writer at compile time. Enterprise COBOL runs Report Writer only as the Report
//! Writer Precompiler's generated COBOL ([`numeric::assumptions::REPORT_WRITER_PRECOMPILER`]),
//! and this follows it: each report's lines and fields are placed; its report control area
//! (PAGE-COUNTER, LINE-COUNTER, the writer's own state and saved controls), its printed fields and
//! its SUM totals become WORKING-STORAGE ([`numeric::assumptions::REPORT_CONTROL_AREA`]); and every
//! name is resolved. `rt::report` runs the result.

use crate::layout::{self, Layout};
use crate::picture::{self, Category};
use syntax::ast::*;
use syntax::report::{self as rw, ColumnNumber, ControlName, Footing, GroupType, LineNumber, NextGroup, ReportStmt};
use syntax::{Error, Pos};

pub use rt::report::{Adding, GroupKind, Page, Sum, generate_target, span, state};

pub type Writer = rt::report::Writer<Expr, Ref, Literal>;
pub type Report = rt::report::Report<Expr, Ref, Literal>;
pub type Control = rt::report::Control<Ref>;
pub type Group = rt::report::Group<Expr, Literal>;
pub type Line = rt::report::Line<Expr, Literal>;
pub type Field = rt::report::Field<Expr, Literal>;
pub type FieldContent = rt::report::FieldContent<Expr, Literal>;
pub type Origin = rt::report::Origin<Expr, Literal>;
pub type Subtotal = rt::report::Subtotal<Expr>;

/// A report's geometry and where its storage went, between synthesis and resolution.
pub(crate) struct Draft {
    file: Option<usize>,
    width: usize,
    /// How many items the report's 01-level item holds.
    children: usize,
    groups: Vec<DraftGroup>,
    controls: Vec<(usize, usize)>,
}

struct DraftGroup {
    lines: Vec<(LineNumber, Vec<DraftField>)>,
    unprinted: Vec<DraftField>,
    /// Entry index and the child holding its SUM total.
    totals: Vec<(usize, usize)>,
}

struct DraftField {
    entry: usize,
    child: usize,
    column: usize,
    size: usize,
    blank_when_zero: bool,
    group_indicate: bool,
    category: Category,
}

pub(crate) fn entry(level: u8, name: Option<String>, picture: Option<String>, usage: Option<Usage>, pos: Pos) -> DataEntry {
    DataEntry {
        level,
        name,
        spelled: None,
        picture,
        usage,
        value: None,
        redefines: None,
        occurs: None,
        occurs_min: None,
        depending_on: None,
        sign: None,
        justified: false,
        sync: false,
        blank_when_zero: false,
        indexed_by: Vec::new(),
        keys: Vec::new(),
        condition_values: Vec::new(),
        false_value: None,
        renames: None,
        object_class: None,
        pos,
    }
}

/// The PICTURE of an entry that has none but a VALUE: X(n) for a nonnumeric literal, S9(n) for a
/// numeric one.
fn implied_picture(lit: &Literal) -> Option<String> {
    Some(match lit {
        Literal::Alnum(s) if !s.is_empty() => format!("X({})", s.chars().count()),
        Literal::Hex(b) if !b.is_empty() => format!("X({})", b.len()),
        Literal::National(s) if !s.is_empty() => format!("N({})", s.chars().count()),
        Literal::Number(t) => {
            let body = t.trim_start_matches(['+', '-']);
            let (int, frac) = body.split_once('.').unwrap_or((body, ""));
            let int = int.len().max(1);
            if frac.is_empty() { format!("S9({int})") } else { format!("S9({int})V9({})", frac.len()) }
        }
        _ => return None,
    })
}

fn entry_picture(e: &rw::Entry) -> Option<String> {
    e.picture.clone().or_else(|| match &e.content {
        Some(rw::Content::Value(lit)) => implied_picture(lit),
        _ => None,
    })
}

/// Integer and decimal places of a numeric or numeric-edited PICTURE.
fn places(picture: &str, notation: crate::picture::Notation) -> Option<(u32, u32)> {
    let p = picture::analyse_with(picture, notation).ok()?;
    matches!(p.category, Category::Numeric | Category::NumericEdited).then_some(((p.digits + p.scaling).saturating_sub(p.scale), p.scale))
}

/// The entry a SUM operand names, when it names a REPORT SECTION entry: report, group, entry.
fn report_entry(reports: &[rw::Report], current: usize, operand: &Ref) -> Option<(usize, usize, usize)> {
    let report = match operand.qualifiers.as_slice() {
        [] => current,
        [q] => reports.iter().position(|r| r.name == *q)?,
        _ => return None,
    };
    if !operand.subscripts.is_empty() || operand.refmod.is_some() {
        return None;
    }
    reports[report].groups.iter().enumerate().find_map(|(g, group)| {
        group.entries.iter().position(|e| e.name.as_deref() == Some(operand.name.as_str()) && entry_picture(e).is_some()).map(|e| (report, g, e))
    })
}

/// Places every report's lines and fields and adds its storage to WORKING-STORAGE; gives each
/// report file a record length when its FD has none ([`numeric::assumptions::REPORT_RECORD_LENGTH`]).
/// Under NOADV (`adv` false) a report record's first byte is its printer control character;
/// `qualify` is how the CONTROL names resolve.
pub(crate) fn prepare(program: &mut Program, adv: bool, qualify: numeric::Qualify, errors: &mut Vec<Error>) -> Vec<Draft> {
    let reports = program.report_writer.reports.clone();
    if reports.is_empty() {
        return Vec::new();
    }
    let control_sizes = measure_controls(program, &reports, qualify);
    let taken: std::collections::HashSet<String> = program
        .working_storage
        .iter()
        .chain(program.files.iter().flat_map(|f| &f.records))
        .chain(&program.linkage)
        .chain(&program.local_storage)
        .filter_map(|e| e.name.clone())
        .collect();
    let mut drafts = Vec::new();
    let mut added = Vec::new();
    for (ri, r) in reports.iter().enumerate() {
        let holders: Vec<usize> = program.files.iter().enumerate().filter(|(_, f)| f.reports.contains(&r.name)).map(|(k, _)| k).collect();
        let file = match holders.as_slice() {
            [k] => Some(*k),
            [] => {
                errors.push(Error::at(r.pos, format!("report {} is named in no FD's REPORT clause", r.name)));
                None
            }
            _ => {
                errors.push(Error::at(r.pos, format!("report {} in more than one FD (INITIATE ... UPON) is not supported yet", r.name)));
                None
            }
        };
        let mut children = vec![
            entry(5, Some("PAGE-COUNTER".into()), Some("S9(9)".into()), Some(Usage::Binary), r.pos),
            entry(5, Some("LINE-COUNTER".into()), Some("S9(9)".into()), Some(Usage::Binary), r.pos),
            entry(5, None, None, None, r.pos),
        ];
        let mut groups = Vec::new();
        for g in &r.groups {
            groups.push(draft_group(&reports, ri, g, &taken, &mut children, crate::picture::Notation::of(&program.environment), errors));
        }
        let mut controls = Vec::new();
        let mut cursor = state::FLAGS + r.groups.len();
        for &len in &control_sizes[ri] {
            controls.push((cursor, len));
            cursor += len;
        }
        children[2].picture = Some(format!("X({cursor})"));
        added.push(entry(1, Some(r.name.clone()), None, None, r.pos));
        let count = children.len();
        added.extend(children);
        drafts.push(Draft { file, width: 0, children: count, groups, controls });
    }
    for (k, f) in program.files.iter_mut().enumerate() {
        let mine: Vec<usize> = drafts.iter().enumerate().filter(|(_, d)| d.file == Some(k)).map(|(i, _)| i).collect();
        if mine.is_empty() {
            continue;
        }
        let reserved = usize::from(crate::printer::reserves_first_byte(f, adv));
        let code = |ri: usize| reserved + code_bytes(&reports[ri].code);
        let longest = mine.iter().map(|&ri| line_end(&drafts[ri])).max().unwrap_or(0);
        let record = match f.record_max {
            Some(n) => n as usize,
            None => {
                let n = (longest.div_ceil(4) * 4).max(4) + mine.iter().map(|&ri| code(ri)).max().unwrap_or(0);
                f.record_max = Some(n as u32);
                if f.recording != Some('V') {
                    f.record_min.get_or_insert(n as u32);
                }
                n
            }
        };
        for &ri in &mine {
            let width = record.saturating_sub(code(ri));
            drafts[ri].width = width;
            let end = line_end(&drafts[ri]);
            if end > width {
                errors.push(Error::at(reports[ri].pos, format!("report {}: a line reaches column {end}, beyond the {width} bytes of the report file's record", reports[ri].name)));
            }
            if let Some(limit) = reports[ri].line_limit.filter(|&l| end > l as usize) {
                errors.push(Error::at(reports[ri].pos, format!("report {}: a line reaches column {end}, beyond LINE LIMIT {limit}", reports[ri].name)));
            }
        }
    }
    if !taken.contains("PRINT-SWITCH") {
        added.push(entry(1, Some("PRINT-SWITCH".into()), Some("S9(9)".into()), Some(Usage::Binary), Pos::default()));
    }
    program.working_storage.extend(added);
    drafts
}

fn code_bytes(code: &Option<Literal>) -> usize {
    match code {
        Some(Literal::Alnum(s)) => s.chars().count(),
        Some(Literal::Hex(b)) => b.len(),
        _ => 0,
    }
}

/// The last column any printed field of the report reaches.
fn line_end(d: &Draft) -> usize {
    d.groups.iter().flat_map(|g| &g.lines).flat_map(|l| &l.1).map(|f| f.column + f.size).max().unwrap_or(0)
}

/// The size of each CONTROL item, from a layout of the program as written.
fn measure_controls(program: &Program, reports: &[rw::Report], qualify: numeric::Qualify) -> Vec<Vec<usize>> {
    let files: Vec<(&[DataEntry], Option<u32>)> = program.files.iter().map(|f| (f.records.as_slice(), f.record_max)).collect();
    let built = layout::build(&program.working_storage, &files, &[], &program.linkage, &program.local_storage, crate::picture::Notation::of(&program.environment), qualify).ok();
    reports
        .iter()
        .map(|r| {
            r.controls
                .iter()
                .map(|c| match built.as_ref().map(|l| l.resolve(&c.name, &c.qualifiers, c.pos)) {
                    Some(Ok(layout::Resolved::Item(i))) => built.as_ref().map_or(0, |l| l.items[i].size as usize),
                    _ => 0,
                })
                .collect()
        })
        .collect()
}

/// Places one group's lines and fields, adding a storage child for each field and SUM total
/// ([`numeric::assumptions::REPORT_SUM_OVERFLOW`] for a total's PICTURE).
fn draft_group(
    reports: &[rw::Report],
    ri: usize,
    g: &rw::Group,
    taken: &std::collections::HashSet<String>,
    children: &mut Vec<DataEntry>,
    notation: crate::picture::Notation,
    errors: &mut Vec<Error>,
) -> DraftGroup {
    let mut d = DraftGroup { lines: Vec::new(), unprinted: Vec::new(), totals: Vec::new() };
    let mut inherited: Vec<(u8, bool, bool, bool)> = Vec::new();
    let mut last_column = 0usize;
    for (ei, e) in g.entries.iter().enumerate() {
        while inherited.last().is_some_and(|&(level, ..)| level >= e.level) {
            inherited.pop();
        }
        let (gi, bwz, just) = inherited.last().map_or((false, false, false), |&(_, a, b, c)| (a, b, c));
        let (gi, bwz, just) = (gi || e.group_indicate, bwz || e.blank_when_zero, just || e.justified);
        if let Some(number) = e.line {
            let merged = matches!((d.lines.last().map(|l| l.0), number), (Some(LineNumber::Line(a)), LineNumber::Line(b)) if a == b);
            if !merged {
                d.lines.push((number, Vec::new()));
                last_column = 0;
            }
        }
        let elementary = g.entries.get(ei + 1).is_none_or(|next| next.level <= e.level);
        if !elementary {
            if e.column.is_some() || e.picture.is_some() || e.content.is_some() {
                errors.push(Error::at(e.pos, "a group entry in a report group cannot have COLUMN, PICTURE, SOURCE, VALUE or SUM"));
            }
            inherited.push((e.level, gi, bwz, just));
            continue;
        }
        let Some(picture) = entry_picture(e) else {
            match (&e.content, e.column) {
                (Some(rw::Content::Sum(_)), _) => errors.push(Error::at(e.pos, "a SUM entry needs a PICTURE")),
                (Some(rw::Content::Value(_)), _) => errors.push(Error::at(e.pos, "a VALUE entry with a figurative constant or ALL needs a PICTURE")),
                (Some(_), Some(_)) => errors.push(Error::at(e.pos, "a printed SOURCE entry needs a PICTURE")),
                (None, Some(column)) => {
                    let start = column_start(column, last_column, 1, e.pos, errors);
                    last_column = start;
                }
                _ => {}
            }
            continue;
        };
        let analysed = match picture::analyse_with(&picture, notation) {
            Ok(p) => p,
            Err(m) => {
                errors.push(Error::at(e.pos, m));
                continue;
            }
        };
        if e.content.is_none() && e.name.is_none() {
            errors.push(Error::at(e.pos, "a report entry with no SOURCE, VALUE or SUM needs a data-name for the program to fill it"));
            continue;
        }
        let size = (analysed.size as usize).max(1);
        let numeric_edited = analysed.category == Category::NumericEdited;
        let numeric = numeric_edited || analysed.category == Category::Numeric;
        if e.blank_when_zero && !numeric {
            errors.push(Error::at(e.pos, "BLANK WHEN ZERO needs a numeric PICTURE"));
        }
        let mut field = entry(5, None, Some(picture.clone()), Some(Usage::Display), e.pos);
        field.sign = e.sign;
        field.justified = just && analysed.category == Category::Alphanumeric;
        field.blank_when_zero = bwz && numeric_edited;
        if let Some(rw::Content::Sum(clauses)) = &e.content {
            let Some((mut int, mut dec)) = places(&picture, notation) else {
                errors.push(Error::at(e.pos, "a SUM entry needs a numeric PICTURE"));
                continue;
            };
            for operand in clauses.iter().flat_map(|c| &c.operands) {
                if let Some((r, og, oe)) = report_entry(reports, ri, operand)
                    && let Some((i, s)) = entry_picture(&reports[r].groups[og].entries[oe]).as_deref().and_then(|p| places(p, notation))
                {
                    int = int.max(i);
                    dec = dec.max(s);
                }
            }
            let int = int.max(1).min(31 - dec.min(30));
            let total_picture = if dec == 0 { format!("S9({int})") } else { format!("S9({int})V9({dec})") };
            let usage = if int + dec <= 18 { Usage::Binary } else { Usage::Packed };
            d.totals.push((ei, children.len()));
            children.push(entry(5, e.name.clone(), Some(total_picture), Some(usage), e.pos));
        } else if e.name.as_ref().is_some_and(|n| e.content.is_none() || !taken.contains(n)) {
            field.name = e.name.clone();
        }
        let child = children.len();
        let draft = DraftField { entry: ei, child, column: 0, size, blank_when_zero: bwz && numeric && !numeric_edited, group_indicate: gi, category: analysed.category };
        children.push(field);
        match e.column {
            Some(column) => {
                let start = column_start(column, last_column, size, e.pos, errors);
                last_column = start + size - 1;
                match d.lines.last_mut() {
                    Some((_, fields)) => fields.push(DraftField { column: start.saturating_sub(1), ..draft }),
                    None => errors.push(Error::at(e.pos, "a COLUMN with no LINE above it")),
                }
            }
            None => d.unprinted.push(draft),
        }
    }
    d
}

/// The 1-based first column of a field of `size` columns after one ending at `last`.
fn column_start(column: ColumnNumber, last: usize, size: usize, pos: Pos, errors: &mut Vec<Error>) -> usize {
    let start = match column {
        ColumnNumber::Left(n) => n as i64,
        ColumnNumber::Plus(n) => last as i64 + n as i64,
        ColumnNumber::Right(n) => n as i64 - size as i64 + 1,
        ColumnNumber::Center(n) => n as i64 - (size as i64 - 1) / 2,
    };
    if start < 1 {
        errors.push(Error::at(pos, "a report field that starts left of column 1"));
        return 1;
    }
    start as usize
}

fn control_level(controls: &[Ref], name: &ControlName) -> Option<usize> {
    match name {
        ControlName::Final => Some(0),
        ControlName::Item(r) => controls.iter().position(|c| c.name == r.name && c.qualifiers == r.qualifiers && c.subscripts.is_empty()).map(|i| i + 1),
    }
}

/// PAGE-COUNTER and LINE-COUNTER in a report group mean the group's own report's.
fn qualified(e: &Expr, report: &str) -> Expr {
    match e {
        Expr::Operand(op) => Expr::Operand(qualified_operand(op, report)),
        Expr::Neg(x) => Expr::Neg(Box::new(qualified(x, report))),
        Expr::Bin(a, op, b) => Expr::Bin(Box::new(qualified(a, report)), *op, Box::new(qualified(b, report))),
    }
}

fn qualified_operand(op: &Operand, report: &str) -> Operand {
    match op {
        Operand::Ref(r) => Operand::Ref(qualified_ref(r, report)),
        Operand::LengthOf(r) => Operand::LengthOf(qualified_ref(r, report)),
        Operand::AddressOf(r) => Operand::AddressOf(qualified_ref(r, report)),
        Operand::Function(f) => {
            let mut f = f.clone();
            f.args = f.args.iter().map(|a| qualified(a, report)).collect();
            Operand::Function(f)
        }
        Operand::Literal(_) => op.clone(),
    }
}

fn qualified_ref(r: &Ref, report: &str) -> Ref {
    let mut r = r.clone();
    if matches!(r.name.as_str(), "PAGE-COUNTER" | "LINE-COUNTER") && r.qualifiers.is_empty() {
        r.qualifiers.push(report.to_owned());
    }
    r.subscripts = r.subscripts.iter().map(|s| qualified(s, report)).collect();
    r
}

/// A reference as text, positions aside, for SOURCE SUM correlation.
fn ref_text(r: &Ref) -> String {
    let mut s = r.name.clone();
    for q in &r.qualifiers {
        s.push_str(" OF ");
        s.push_str(q);
    }
    for e in &r.subscripts {
        s.push(' ');
        s.push_str(&expr_text(e));
    }
    s
}

fn expr_text(e: &Expr) -> String {
    match e {
        Expr::Operand(Operand::Ref(r)) => ref_text(r),
        Expr::Operand(Operand::Literal(l)) => format!("{l:?}"),
        Expr::Operand(other) => format!("{other:?}"),
        Expr::Neg(x) => format!("-({})", expr_text(x)),
        Expr::Bin(a, op, b) => format!("({} {op:?} {})", expr_text(a), expr_text(b)),
    }
}

/// Resolves every name of every report against the layout, and places the page regions.
pub(crate) fn resolve(program: &Program, layout: &Layout, drafts: Vec<Draft>, errors: &mut Vec<Error>) -> Writer {
    let reports = &program.report_writer.reports;
    let mut writer = Writer::default();
    if reports.is_empty() {
        for u in &program.report_writer.uses {
            errors.push(Error::at(u.pos, format!("USE BEFORE REPORTING {}: the program has no REPORT SECTION", u.group)));
        }
        return writer;
    }
    writer.print_switch = (0..layout.items.len()).rev().find(|&i| layout.items[i].parent.is_none() && layout.items[i].name.as_deref() == Some("PRINT-SWITCH"));
    for (ri, (r, draft)) in reports.iter().zip(drafts).enumerate() {
        match resolve_report(program, layout, ri, r, draft, errors) {
            Some(report) => writer.reports.push(report),
            None => return Writer::default(),
        }
    }
    for u in &program.report_writer.uses {
        let found: Vec<(usize, usize)> = writer
            .reports
            .iter()
            .enumerate()
            .filter(|(_, r)| u.qualifier.as_ref().is_none_or(|q| r.name == *q))
            .flat_map(|(ri, r)| r.groups.iter().enumerate().filter(|(_, g)| g.name.as_deref() == Some(u.group.as_str())).map(move |(gi, _)| (ri, gi)))
            .collect();
        match found.as_slice() {
            [(ri, gi)] => writer.reports[*ri].groups[*gi].declarative = Some((u.section, crate::section_end(program, u.section))),
            [] => errors.push(Error::at(u.pos, format!("USE BEFORE REPORTING {}: no report group has that name", u.group))),
            _ => errors.push(Error::at(u.pos, format!("USE BEFORE REPORTING {}: more than one report group has that name; qualify it with IN", u.group))),
        }
    }
    writer
}

fn field_content(r: &rw::Report, e: &rw::Entry, f: &DraftField, total: Option<usize>, check: &mut crate::Check) -> FieldContent {
    match &e.content {
        None => FieldContent::Program,
        Some(rw::Content::Value(lit)) => FieldContent::Value(lit.clone()),
        Some(rw::Content::Source(x)) => {
            let x = qualified(x, &r.name);
            check.expr(&x);
            if (!matches!(x, Expr::Operand(_)) || e.rounded) && !matches!(f.category, Category::Numeric | Category::NumericEdited) {
                check.errors.push(Error::at(e.pos, "an arithmetic SOURCE, or ROUNDED, needs a numeric PICTURE"));
            }
            FieldContent::Source(x)
        }
        Some(rw::Content::Sum(_)) => total.map_or(FieldContent::Program, FieldContent::Sum),
    }
}

fn group_kind(kind: &GroupType) -> (GroupKind, Option<Option<ControlName>>) {
    match kind {
        GroupType::ReportHeading => (GroupKind::ReportHeading, None),
        GroupType::PageHeading => (GroupKind::PageHeading, None),
        GroupType::ControlHeading(c) => (GroupKind::ControlHeading, Some(c.clone())),
        GroupType::Detail => (GroupKind::Detail, None),
        GroupType::ControlFooting(c) => (GroupKind::ControlFooting, Some(c.clone())),
        GroupType::PageFooting => (GroupKind::PageFooting, None),
        GroupType::ReportFooting => (GroupKind::ReportFooting, None),
    }
}

/// The LINE clauses of a group must suit its type and the report.
fn check_lines(lines: &[(LineNumber, Vec<DraftField>)], kind: GroupKind, paged: bool, pos: Pos, errors: &mut Vec<Error>) {
    let relative = matches!(lines.first().map(|l| l.0), Some(LineNumber::Plus(_)));
    let mut previous: Option<u32> = None;
    for (li, (number, _)) in lines.iter().enumerate() {
        match *number {
            LineNumber::Line(n) | LineNumber::NextPage(Some(n)) => {
                if !paged {
                    errors.push(Error::at(pos, "an absolute LINE needs a PAGE LIMIT"));
                }
                if relative {
                    errors.push(Error::at(pos, "a report group whose first LINE is relative must have only relative LINEs"));
                }
                if previous.is_some_and(|p| n <= p) {
                    errors.push(Error::at(pos, "absolute LINE numbers in a report group must increase"));
                }
                previous = Some(n);
            }
            LineNumber::Plus(k) => previous = previous.map(|p| p + k),
            LineNumber::NextPage(None) => {}
        }
        if matches!(number, LineNumber::NextPage(_)) {
            if !paged {
                errors.push(Error::at(pos, "NEXT PAGE needs a PAGE LIMIT"));
            }
            if li > 0 {
                errors.push(Error::at(pos, "NEXT PAGE on a LINE other than a group's first (MULTIPLE PAGE) is not supported yet"));
            }
            if matches!(kind, GroupKind::PageHeading | GroupKind::PageFooting) {
                errors.push(Error::at(pos, "a PAGE HEADING or PAGE FOOTING cannot begin on the NEXT PAGE"));
            }
        }
    }
}

fn resolve_report(program: &Program, layout: &Layout, ri: usize, r: &rw::Report, draft: Draft, errors: &mut Vec<Error>) -> Option<Report> {
    let reports = &program.report_writer.reports;
    let file = draft.file?;
    let root = (0..layout.items.len()).rev().find(|&i| {
        let it = &layout.items[i];
        it.parent.is_none() && it.file.is_none() && it.linkage.is_none() && !it.local && it.name.as_deref() == Some(r.name.as_str())
    });
    let Some(root) = root.filter(|&i| layout.items[i].children.len() == draft.children) else {
        errors.push(Error::at(r.pos, format!("report {}: its report control area could not be laid out", r.name)));
        return None;
    };
    let children = &layout.items[root].children;
    let child = |ordinal: usize| children[ordinal];
    let paged = r.page.is_some();
    let mut check = crate::Check { layout, program, errors, debugging: false, max_digits: 31, inline_performs: 0 };
    let mut controls = Vec::new();
    for (c, &(saved, len)) in r.controls.iter().zip(&draft.controls) {
        check.reference(c);
        if !c.subscripts.is_empty() || c.refmod.is_some() {
            check.errors.push(Error::at(c.pos, format!("CONTROL {}: a subscripted or reference-modified control is not supported yet", c.name)));
        }
        controls.push(Control { reference: c.clone(), saved, len });
    }
    let levels = r.controls.len();
    let mut control_headings = vec![None; levels + 1];
    let mut control_footings = vec![None; levels + 1];
    let (mut report_heading, mut page_heading, mut page_footing, mut report_footing) = (None, None, None, None);
    let mut sums: Vec<Sum> = Vec::new();
    let mut total_of: std::collections::HashMap<(usize, usize), usize> = std::collections::HashMap::new();
    let mut groups: Vec<Group> = Vec::new();
    for (gi, g) in r.groups.iter().enumerate() {
        let (kind, control) = group_kind(&g.kind);
        let level = match control {
            None => 0,
            Some(None) if levels <= 1 => levels,
            Some(None) => {
                check.errors.push(Error::at(g.pos, "a CONTROL HEADING or FOOTING must name its control when the report has several"));
                0
            }
            Some(Some(name)) => control_level(&r.controls, &name).unwrap_or_else(|| {
                check.errors.push(Error::at(g.pos, "a CONTROL HEADING or FOOTING names a control that is not in the report's CONTROL clause"));
                0
            }),
        };
        let slot = match kind {
            GroupKind::ReportHeading => Some(&mut report_heading),
            GroupKind::PageHeading => Some(&mut page_heading),
            GroupKind::PageFooting => Some(&mut page_footing),
            GroupKind::ReportFooting => Some(&mut report_footing),
            GroupKind::ControlHeading => control_headings.get_mut(level),
            GroupKind::ControlFooting => control_footings.get_mut(level),
            GroupKind::Detail => None,
        };
        if let Some(slot) = slot {
            if slot.is_some() {
                check.errors.push(Error::at(g.pos, format!("report {} has two {kind:?} groups for the same level", r.name)));
            }
            *slot = Some(gi);
        }
        if !paged && matches!(kind, GroupKind::PageHeading | GroupKind::PageFooting) {
            check.errors.push(Error::at(g.pos, "a PAGE HEADING or PAGE FOOTING needs a PAGE LIMIT"));
        }
        if let Some(ng) = g.next_group {
            if !paged && !matches!(ng, NextGroup::Plus(_)) {
                check.errors.push(Error::at(g.pos, "NEXT GROUP with a line or NEXT PAGE needs a PAGE LIMIT"));
            }
            if matches!(kind, GroupKind::PageHeading | GroupKind::ReportFooting) {
                check.errors.push(Error::at(g.pos, "NEXT GROUP is not allowed in a PAGE HEADING or REPORT FOOTING"));
            }
        }
        let dg = &draft.groups[gi];
        check_lines(&dg.lines, kind, paged, g.pos, check.errors);
        let mut totals = Vec::new();
        for &(ei, ordinal) in &dg.totals {
            total_of.insert((gi, ei), sums.len());
            totals.push(sums.len());
            sums.push(Sum { total: child(ordinal), reset: None });
        }
        let make = |f: &DraftField, column: usize, check: &mut crate::Check| {
            let e = &g.entries[f.entry];
            let content = field_content(r, e, f, total_of.get(&(gi, f.entry)).copied(), check);
            Field { item: child(f.child), column, content, group_indicate: f.group_indicate, blank_when_zero: f.blank_when_zero, rounded: e.rounded, pos: e.pos }
        };
        let lines: Vec<Line> = dg.lines.iter().map(|(number, fields)| Line { number: *number, fields: fields.iter().map(|f| make(f, f.column, &mut check)).collect() }).collect();
        let unprinted: Vec<Field> = dg.unprinted.iter().map(|f| make(f, 0, &mut check)).collect();
        let indicate = lines.iter().flat_map(|l| &l.fields).any(|f| f.group_indicate).then_some(gi);
        if indicate.is_some() && kind != GroupKind::Detail {
            check.errors.push(Error::at(g.pos, "GROUP INDICATE outside a DETAIL group is not supported yet"));
        }
        groups.push(Group { name: g.name.clone(), kind, level, next_group: g.next_group, lines, unprinted, cross: Vec::new(), rolls: Vec::new(), totals, indicate, declarative: None });
    }
    if !groups.iter().any(|g| g.kind.is_body()) {
        check.errors.push(Error::at(r.pos, format!("report {} has no CONTROL HEADING, DETAIL or CONTROL FOOTING group", r.name)));
    }
    let details: Vec<usize> = groups.iter().enumerate().filter(|(_, g)| g.kind == GroupKind::Detail).map(|(i, _)| i).collect();
    let mut subtotals = Vec::new();
    for (gi, g) in r.groups.iter().enumerate() {
        for (ei, e) in g.entries.iter().enumerate() {
            let Some(rw::Content::Sum(clauses)) = &e.content else { continue };
            let Some(&s) = total_of.get(&(gi, ei)) else { continue };
            for clause in clauses {
                if let Some(reset) = &clause.reset {
                    match control_level(&r.controls, reset) {
                        Some(level) => sums[s].reset = Some(level),
                        None => check.errors.push(Error::at(e.pos, "RESET ON names a control that is not in the report's CONTROL clause")),
                    }
                }
                let mut upon = Vec::new();
                for u in &clause.upon {
                    match details.iter().copied().find(|&d| groups[d].name.as_deref() == Some(u.name.as_str())) {
                        Some(d) => upon.push(d),
                        None => check.errors.push(Error::at(u.pos, format!("SUM ... UPON {}: not a DETAIL group of report {}", u.name, r.name))),
                    }
                }
                for operand in &clause.operands {
                    if let Some((or, og, oe)) = report_entry(reports, ri, operand) {
                        if or != ri {
                            check.errors.push(Error::at(operand.pos, "a SUM of an entry in another report is not supported yet"));
                            continue;
                        }
                        let source = &r.groups[og].entries[oe];
                        if entry_picture(source).as_deref().and_then(|p| places(p, crate::picture::Notation::of(&program.environment))).is_none() {
                            check.errors.push(Error::at(operand.pos, format!("SUM {}: the entry summed must be numeric", operand.name)));
                            continue;
                        }
                        let origin = match &source.content {
                            Some(rw::Content::Source(x)) => Origin::Source(qualified(x, &r.name)),
                            Some(rw::Content::Value(lit)) => Origin::Value(lit.clone()),
                            Some(rw::Content::Sum(_)) => match total_of.get(&(og, oe)) {
                                Some(&t) => Origin::Total(t),
                                None => continue,
                            },
                            None => {
                                check.errors.push(Error::at(operand.pos, format!("SUM {}: an entry the program fills itself cannot be summed", operand.name)));
                                continue;
                            }
                        };
                        if og == gi { groups[gi].cross.push((s, origin)) } else { groups[og].rolls.push((s, origin)) }
                        continue;
                    }
                    check.reference(operand);
                    if let Some(i) = check.item(operand)
                        && !layout.items[i].kind.is_numeric()
                    {
                        check.errors.push(Error::at(operand.pos, format!("SUM {}: not a numeric data item", operand.name)));
                    }
                    let text = ref_text(operand);
                    let correlated: Vec<usize> = details
                        .iter()
                        .copied()
                        .filter(|&d| r.groups[d].entries.iter().any(|x| matches!(&x.content, Some(rw::Content::Source(Expr::Operand(Operand::Ref(s)))) if ref_text(s) == text)))
                        .collect();
                    let adding = if !upon.is_empty() {
                        Adding::Upon(upon.clone())
                    } else if !correlated.is_empty() {
                        Adding::Correlated(correlated)
                    } else {
                        Adding::EveryGenerate
                    };
                    subtotals.push(Subtotal { sum: s, operand: Expr::Operand(Operand::Ref(operand.clone())), adding });
                }
            }
        }
    }
    for (gi, g) in groups.iter_mut().enumerate() {
        match order_cross(std::mem::take(&mut g.cross)) {
            Ok(ordered) => g.cross = ordered,
            Err(()) => check.errors.push(Error::at(r.groups[gi].pos, "SUM entries of a report group total each other in a circle")),
        }
    }
    if !paged && (r.heading.is_some() || r.first_detail.is_some() || r.last_detail.is_some() || r.footing.is_some()) {
        check.errors.push(Error::at(r.pos, "HEADING, FIRST DETAIL, LAST DETAIL and FOOTING need a PAGE LIMIT"));
    }
    let page = r.page.map(|limit| regions(r, limit, page_footing.map(|p| &groups[p]), check.errors));
    Some(Report {
        name: r.name.clone(),
        file,
        code: r.code.clone(),
        width: draft.width,
        page,
        controls,
        groups,
        sums,
        subtotals,
        page_counter: child(0),
        line_counter: child(1),
        state: child(2),
        report_heading,
        page_heading,
        page_footing,
        report_footing,
        control_headings,
        control_footings,
        first_detail_written: r.first_detail.map(i64::from),
    })
}

/// Orders cross-foot additions so that a total is complete before another group entry adds it.
fn order_cross(mut pending: Vec<(usize, Origin)>) -> Result<Vec<(usize, Origin)>, ()> {
    let mut ordered = Vec::new();
    while !pending.is_empty() {
        let ready = |(_, origin): &(usize, Origin), pending: &[(usize, Origin)]| match origin {
            Origin::Total(t) => !pending.iter().any(|(s, _)| s == t),
            _ => true,
        };
        let Some(i) = (0..pending.len()).find(|&i| ready(&pending[i], &pending)) else { return Err(()) };
        ordered.push(pending.remove(i));
    }
    Ok(ordered)
}

/// The page regions with the defaults the precompiler takes, as it is supplied (option OSVS):
/// [`numeric::assumptions::REPORT_PAGE_REGION_DEFAULTS`].
fn regions(r: &rw::Report, limit: u32, page_footing: Option<&Group>, errors: &mut Vec<Error>) -> Page {
    let limit = limit as i64;
    let heading = r.heading.map_or(1, i64::from);
    let before_footing = match page_footing.and_then(|g| g.lines.first().map(|l| (l.number, g))) {
        Some((LineNumber::Line(n), _)) => n as i64 - 1,
        Some((_, g)) => limit - span(g),
        None => limit,
    };
    let footing = match (r.footing, r.last_detail) {
        (Some(Footing::Line(n)), _) => n as i64,
        (Some(Footing::Plus(k)), Some(ld)) => ld as i64 + k as i64,
        (Some(Footing::Plus(_)), None) => before_footing,
        (None, Some(ld)) => ld as i64,
        (None, None) => before_footing,
    };
    let last_detail = match (r.last_detail, r.footing) {
        (Some(ld), _) => ld as i64,
        (None, Some(Footing::Plus(k))) => footing - k as i64,
        (None, _) => footing,
    };
    let first_detail = r.first_detail.map_or(heading, i64::from);
    let limit = limit.max(last_detail).max(footing);
    if !(1 <= heading && heading <= first_detail && first_detail <= last_detail && last_detail <= footing) {
        errors.push(Error::at(r.pos, format!("report {}: the page regions must run HEADING <= FIRST DETAIL <= LAST DETAIL <= FOOTING <= PAGE LIMIT", r.name)));
    }
    Page { limit, heading, first_detail, last_detail, footing }
}

/// INITIATE, GENERATE and TERMINATE must name reports and DETAIL groups the program has.
pub(crate) fn check_statement(program: &Program, s: &ReportStmt, errors: &mut Vec<Error>) {
    let reports = &program.report_writer.reports;
    match s {
        ReportStmt::Initiate { reports: names, pos } | ReportStmt::Terminate { reports: names, pos } => {
            for n in names {
                if !reports.iter().any(|r| r.name == *n) {
                    errors.push(Error::at(*pos, format!("{n} is not a report of this program")));
                }
            }
        }
        ReportStmt::Generate { name, qualifier, pos } => {
            if qualifier.is_none()
                && let Some(r) = reports.iter().find(|r| r.name == *name)
            {
                if !r.groups.iter().any(|g| matches!(g.kind, GroupType::ControlHeading(_) | GroupType::ControlFooting(_))) {
                    errors.push(Error::at(*pos, format!("GENERATE {name}: summary reporting needs a CONTROL HEADING or CONTROL FOOTING group")));
                }
                return;
            }
            let found = reports
                .iter()
                .filter(|r| qualifier.as_ref().is_none_or(|q| r.name == *q))
                .flat_map(|r| &r.groups)
                .filter(|g| g.name.as_deref() == Some(name.as_str()))
                .collect::<Vec<_>>();
            match found.as_slice() {
                [g] if g.kind == GroupType::Detail => {}
                [_] => errors.push(Error::at(*pos, format!("GENERATE {name}: not a DETAIL group"))),
                [] => errors.push(Error::at(*pos, format!("GENERATE {name}: no report or DETAIL group of that name"))),
                _ => errors.push(Error::at(*pos, format!("GENERATE {name}: more than one report has a DETAIL group of that name; qualify it with IN"))),
            }
        }
        ReportStmt::Suppress { .. } => {}
    }
}

