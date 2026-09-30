//! LINAGE: a print file's logical page ([`numeric::assumptions::LINAGE_COUNTER`]). OPEN OUTPUT or
//! EXTEND takes the page body, first footing line and margins from the FD's integers and data
//! items, and each new page takes the data items' values again. LINAGE-COUNTER is the line of the
//! page body the printer is at; WRITE moves the paper past the footing area and the margins in
//! lines ([`numeric::assumptions::LINAGE_PAGE_MOVEMENT`]).

use crate::layout::{self, Kind, Layout, Resolved};
use syntax::ast::*;
use syntax::{Error, Pos};

/// One logical page, in lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Geometry {
    pub body: u64,
    /// The first line of the footing area. Without FOOTING only a page overflow raises the
    /// end-of-page condition (Language Reference SC27-8713-03, p. 475).
    pub footing: Option<u64>,
    pub top: u64,
    pub bottom: u64,
}

impl Geometry {
    /// The page the clause's values give, or why they give none: the page body is at least a line
    /// and the footing starts within it (Language Reference SC27-8713-03, p. 189).
    pub fn new(body: i64, footing: Option<i64>, top: i64, bottom: i64) -> Result<Self, String> {
        if body < 1 {
            return Err(format!("LINAGE gives a page body of {body} lines, and it needs at least 1"));
        }
        if let Some(f) = footing.filter(|f| !(1..=body).contains(f)) {
            return Err(format!("LINAGE puts the footing at line {f}, outside the page body of {body} lines"));
        }
        if top < 0 || bottom < 0 {
            return Err(format!("LINAGE gives margins of {top} and {bottom} lines"));
        }
        Ok(Self { body: body as u64, footing: footing.map(|f| f as u64), top: top as u64, bottom: bottom as u64 })
    }
}

/// Where the printer is on a LINAGE file's current page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Page {
    pub geometry: Geometry,
    /// LINAGE-COUNTER: the line of the page body the printer is at.
    pub counter: u64,
    /// Lines the paper has yet to move to reach that line: the first page's top margin, until the
    /// first WRITE.
    owed: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Lines(u64),
    Page,
}

/// What one WRITE does: the lines the paper moves before the record's line and after it, and
/// whether the end-of-page condition holds once it is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    pub ahead: u64,
    pub behind: u64,
    pub end_of_page: bool,
}

impl Page {
    pub fn opened(geometry: Geometry) -> Self {
        Self { geometry, counter: 1, owed: geometry.top }
    }

    /// A WRITE BEFORE (`before`) or AFTER ADVANCING `motion`. A WRITE that would pass the page
    /// body, or that advances a page, goes to the first line of the next page, whose geometry
    /// `next` gives (Language Reference SC27-8713-03, pp. 474-475).
    pub fn write<E>(&mut self, before: bool, motion: Motion, next: impl FnOnce() -> Result<Geometry, E>) -> Result<Step, E> {
        let current = self.geometry;
        let (moved, overflow) = match motion {
            Motion::Lines(n) if self.counter.saturating_add(n) <= current.body => {
                self.counter += n;
                (n, false)
            }
            _ => {
                let following = next()?;
                let rest = current.body.saturating_sub(self.counter).saturating_add(current.bottom);
                self.geometry = following;
                self.counter = 1;
                (rest.saturating_add(following.top).saturating_add(1), motion != Motion::Page)
            }
        };
        let end_of_page = overflow || self.geometry.footing.is_some_and(|f| self.counter >= f);
        let owed = std::mem::take(&mut self.owed);
        Ok(if before { Step { ahead: owed, behind: moved, end_of_page } } else { Step { ahead: owed.saturating_add(moved), behind: 0, end_of_page } })
    }
}

/// Adds each LINAGE file's LINAGE-COUNTER to WORKING-STORAGE: with the PICTURE and USAGE of the
/// page body's data item, or binary with as many digits as its integer (Language Reference
/// SC27-8713-03, p. 23). Returns each file's entry in WORKING-STORAGE.
pub(crate) fn add_counters(program: &mut Program) -> Vec<Option<usize>> {
    let mut counters = vec![None; program.files.len()];
    let by_data = |f: &FileDecl| matches!(f.linage.as_ref().map(|l| &l.lines), Some(LinageValue::Data(_)));
    let built = program.files.iter().any(by_data).then(|| {
        let files: Vec<(&[DataEntry], Option<u32>)> = program.files.iter().map(|f| (f.records.as_slice(), f.record_max)).collect();
        layout::build(&program.working_storage, &files, &[], &program.linkage, &program.local_storage, program.environment.decimal_point_comma).ok()
    });
    let mut added = Vec::new();
    for (k, f) in program.files.iter().enumerate() {
        let Some(linage) = &f.linage else { continue };
        let described = match &linage.lines {
            LinageValue::Integer(n) => Some((format!("9({})", n.len()), Usage::Binary)),
            LinageValue::Data(r) => built.as_ref().and_then(Option::as_ref).and_then(|l| unsigned_integer(l, r)).map(|(digits, usage)| (format!("9({digits})"), usage)),
        };
        let (picture, usage) = described.unwrap_or_else(|| ("9(9)".into(), Usage::Binary));
        counters[k] = Some(program.working_storage.len() + added.len());
        added.push(crate::report::entry(1, Some("LINAGE-COUNTER".into()), Some(picture), Some(usage), f.pos));
    }
    program.working_storage.extend(added);
    counters
}

/// The digits and USAGE of an unsigned integer data item, as LINAGE wants its data-names.
fn unsigned_integer(layout: &Layout, r: &Ref) -> Option<(u32, Usage)> {
    let Ok(Resolved::Item(i)) = layout.resolve(&r.name, &r.qualifiers, r.pos) else { return None };
    let item = &layout.items[i];
    if !item.dims.is_empty() {
        return None;
    }
    match item.kind {
        Kind::Zoned { digits, scale: 0, signed: false, .. } => Some((digits, Usage::Display)),
        Kind::Packed { digits, scale: 0, signed: false } => Some((digits, Usage::Packed)),
        Kind::Binary { digits, scale: 0, signed: false, native } => Some((digits, if native { Usage::NativeBinary } else { Usage::Binary })),
        _ => None,
    }
}

/// The compiler limit on a LINAGE integer (Language Reference SC27-8713-03, p. 747).
const MOST: u64 = 99_999_999;

/// What the Language Reference asks of an FD's LINAGE clause (SC27-8713-03, pp. 179-190).
pub(crate) fn check_file(program: &Program, layout: &Layout, k: usize, errors: &mut Vec<Error>) {
    let f = &program.files[k];
    let Some(linage) = &f.linage else { return };
    let fail = |errors: &mut Vec<Error>, pos: Pos, m: String| errors.push(Error::at(pos, m));
    match f.organization {
        Organization::Sequential => {}
        Organization::LineSequential => fail(errors, f.pos, format!("{}: LINAGE is for a sequential file, not a line-sequential one", f.name)),
        Organization::Indexed | Organization::Relative => fail(errors, f.pos, format!("{}: LINAGE is for a sequential file, not an indexed or relative one", f.name)),
    }
    if !f.reports.is_empty() {
        fail(errors, f.pos, format!("{}: LINAGE on a report file is not supported yet", f.name));
    }
    let phrases = [("LINAGE", Some(&linage.lines)), ("FOOTING", linage.footing.as_ref()), ("TOP", linage.top.as_ref()), ("BOTTOM", linage.bottom.as_ref())];
    let mut integers = [None; 4];
    for (slot, (phrase, value)) in phrases.into_iter().enumerate() {
        match value {
            None => {}
            Some(LinageValue::Integer(n)) => match n.parse::<u64>().ok().filter(|&v| v <= MOST) {
                Some(v) => integers[slot] = Some(v),
                None => fail(errors, f.pos, format!("{}: {phrase} {n} is more than the {MOST} lines LINAGE allows", f.name)),
            },
            Some(LinageValue::Data(r)) if unsigned_integer(layout, r).is_none() => match layout.resolve(&r.name, &r.qualifiers, r.pos) {
                Err(e) => errors.push(e),
                Ok(_) => fail(errors, r.pos, format!("{}: {phrase} {} is not an unsigned integer data item", f.name, r.name)),
            },
            Some(LinageValue::Data(_)) => {}
        }
    }
    if integers[0] == Some(0) {
        fail(errors, f.pos, format!("{}: LINAGE 0: the page body needs at least one line", f.name));
    }
    match (integers[0], integers[1]) {
        (_, Some(0)) => fail(errors, f.pos, format!("{}: FOOTING 0: the footing starts at line 1 or later", f.name)),
        (Some(body), Some(footing)) if footing > body => fail(errors, f.pos, format!("{}: FOOTING {footing} is past the page body of {body} lines", f.name)),
        _ => {}
    }
}

/// The file a WRITE's record belongs to.
fn file_of(layout: &Layout, record: &Ref) -> Option<usize> {
    match layout.resolve(&record.name, &record.qualifiers, record.pos) {
        Ok(Resolved::Item(i)) => layout.items[i].file.map(usize::from),
        _ => None,
    }
}

/// END-OF-PAGE needs a LINAGE file (Language Reference SC27-8713-03, p. 475); a mnemonic-name
/// does not move a LINAGE file's paper yet.
pub(crate) fn check_write(program: &Program, layout: &Layout, record: &Ref, advancing: Option<&Advancing>, end_of_page: &Handlers, pos: Pos, errors: &mut Vec<Error>) {
    let Some(f) = file_of(layout, record).map(|k| &program.files[k]) else { return };
    let phrase = end_of_page.on.is_some() || end_of_page.not_on.is_some();
    if phrase && f.linage.is_none() {
        errors.push(Error::at(pos, format!("WRITE ... END-OF-PAGE: the FD of {} has no LINAGE clause", f.name)));
    }
    if let (Some(Advancing::Mnemonic { name, .. }), Some(_)) = (advancing, &f.linage) {
        errors.push(Error::at(pos, format!("WRITE ... ADVANCING {name} on {}, whose FD has LINAGE, is not supported yet", f.name)));
    }
}

/// The items a statement stores into.
fn receivers(s: &Stmt) -> Vec<&Ref> {
    let mut out: Vec<&Ref> = Vec::new();
    match s {
        Stmt::Move { to, .. } => out.extend(to),
        Stmt::Compute { targets, .. } => out.extend(targets.iter().map(|t| &t.r)),
        Stmt::Arith(a) => out.extend(a.computations.iter().map(|(t, _)| &t.r).chain(a.remainder.iter().map(|(t, ..)| &t.r))),
        Stmt::Initialize { targets, .. } => out.extend(targets),
        Stmt::Set { set: SetStmt::To { targets, .. } | SetStmt::AddressOf { targets, .. } | SetStmt::UpDown { targets, .. }, .. } => out.extend(targets),
        Stmt::Accept { target, .. } => out.push(target),
        Stmt::Read(r) => out.extend(&r.into),
        Stmt::String(st) => out.extend(std::iter::once(&st.into).chain(&st.pointer)),
        Stmt::Unstring(u) => {
            out.extend(u.into.iter().flat_map(|i| std::iter::once(&i.target).chain(&i.delimiter_in).chain(&i.count_in)));
            out.extend(u.pointer.iter().chain(&u.tallying));
        }
        Stmt::Inspect(i) => {
            if !i.replacing.is_empty() || i.converting.is_some() {
                out.push(&i.target);
            }
            out.extend(i.tallying.iter().filter_map(|p| p.counter.as_ref()));
        }
        Stmt::PerformInline { repeat: Loop::Varying { varying, .. }, .. } | Stmt::PerformProc { repeat: Loop::Varying { varying, .. }, .. } => out.push(&varying.var),
        Stmt::Search(se) => out.extend(&se.varying),
        Stmt::Sorting(so) => {
            if let Sorting::Return { into: Some(r), .. } = &**so {
                out.push(r);
            }
        }
        _ => {}
    }
    out
}

/// LINAGE-COUNTER can be read but not changed by a statement (Language Reference SC27-8713-03,
/// p. 24).
pub(crate) fn check_receivers(layout: &Layout, s: &Stmt, errors: &mut Vec<Error>) {
    if layout.linage_counters.iter().all(Option::is_none) {
        return;
    }
    for r in receivers(s) {
        if let Ok(Resolved::Item(i)) = layout.resolve(&r.name, &r.qualifiers, r.pos)
            && layout.linage_counters.contains(&Some(i))
        {
            errors.push(Error::at(r.pos, "LINAGE-COUNTER can be read, but no statement can change it"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(body: i64, footing: Option<i64>, top: i64, bottom: i64) -> Page {
        Page::opened(Geometry::new(body, footing, top, bottom).unwrap())
    }

    fn step(p: &mut Page, before: bool, motion: Motion, next: Geometry) -> (u64, u64, bool, u64) {
        let s = p.write(before, motion, || Ok::<_, ()>(next)).unwrap();
        (s.ahead, s.behind, s.end_of_page, p.counter)
    }

    #[test]
    fn the_first_write_moves_past_the_top_margin_and_counts_from_line_one() {
        let g = Geometry::new(5, Some(4), 2, 3).unwrap();
        let mut p = Page::opened(g);
        assert_eq!(p.counter, 1);
        assert_eq!(step(&mut p, false, Motion::Lines(1), g), (3, 0, false, 2));
        assert_eq!(step(&mut p, false, Motion::Lines(1), g), (1, 0, false, 3));
        assert_eq!(step(&mut p, false, Motion::Lines(1), g), (1, 0, true, 4));
        assert_eq!(step(&mut p, false, Motion::Lines(0), g), (0, 0, true, 4));
        assert_eq!(step(&mut p, false, Motion::Lines(2), g), (1 + 3 + 2 + 1, 0, true, 1));
    }

    #[test]
    fn before_advancing_prints_then_moves_and_overflow_leaves_the_printer_on_the_next_page() {
        let g = Geometry::new(3, Some(3), 1, 1).unwrap();
        let mut p = Page::opened(g);
        assert_eq!(step(&mut p, true, Motion::Lines(1), g), (1, 1, false, 2));
        assert_eq!(step(&mut p, true, Motion::Lines(1), g), (0, 1, true, 3));
        assert_eq!(step(&mut p, true, Motion::Lines(1), g), (0, 1 + 1 + 1, true, 1));
    }

    #[test]
    fn without_footing_only_an_overflow_is_the_end_of_the_page() {
        let g = Geometry::new(2, None, 0, 0).unwrap();
        let mut p = Page::opened(g);
        assert_eq!(step(&mut p, false, Motion::Lines(1), g), (1, 0, false, 2));
        assert_eq!(step(&mut p, false, Motion::Lines(0), g), (0, 0, false, 2));
        assert_eq!(step(&mut p, false, Motion::Lines(1), g), (1, 0, true, 1));
        assert_eq!(step(&mut p, false, Motion::Page, g), (2, 0, false, 1));
    }

    #[test]
    fn advancing_page_takes_the_next_geometry_and_raises_end_of_page_only_at_a_first_line_footing() {
        let mut p = page(10, Some(8), 0, 0);
        let next = Geometry::new(4, Some(2), 3, 0).unwrap();
        assert_eq!(step(&mut p, false, Motion::Lines(5), next), (5, 0, false, 6));
        assert_eq!(step(&mut p, false, Motion::Page, next), (4 + 3 + 1, 0, false, 1));
        assert_eq!(p.geometry, next);
        let at_one = Geometry::new(4, Some(1), 0, 0).unwrap();
        assert_eq!(step(&mut p, true, Motion::Page, at_one), (0, 3 + 1, true, 1));
    }

    #[test]
    fn a_page_the_values_cannot_make_is_refused() {
        assert!(Geometry::new(0, None, 0, 0).is_err());
        assert!(Geometry::new(5, Some(6), 0, 0).is_err());
        assert!(Geometry::new(5, Some(0), 0, 0).is_err());
        assert!(Geometry::new(5, None, -1, 0).is_err());
        assert_eq!(Geometry::new(5, None, 0, 0).unwrap().footing, None);
    }
}
