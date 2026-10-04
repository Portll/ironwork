//! LINAGE: a print file's logical page ([`numeric::assumptions::LINAGE_COUNTER`]). OPEN OUTPUT or
//! EXTEND takes the page body, first footing line and margins from the FD's integers and data
//! items, and each new page takes the data items' values again. LINAGE-COUNTER is the line of the
//! page body the printer is at; WRITE moves the paper past the footing area and the margins in
//! lines ([`numeric::assumptions::LINAGE_PAGE_MOVEMENT`]).

use crate::layout::{self, Kind, Layout, Resolved};
use syntax::ast::*;
use syntax::{Error, Pos};

pub use rt::linage::{Geometry, Motion, Page, Step};

/// Adds each LINAGE file's LINAGE-COUNTER to WORKING-STORAGE: with the PICTURE and USAGE of the
/// page body's data item, or binary with as many digits as its integer (Language Reference
/// SC27-8713-03, p. 23), its data-name resolved as `qualify` says. Returns each file's entry in
/// WORKING-STORAGE.
pub(crate) fn add_counters(program: &mut Program, qualify: numeric::Qualify) -> Vec<Option<usize>> {
    let mut counters = vec![None; program.files.len()];
    let by_data = |f: &FileDecl| matches!(f.linage.as_ref().map(|l| &l.lines), Some(LinageValue::Data(_)));
    let built = program.files.iter().any(by_data).then(|| {
        let files: Vec<(&[DataEntry], Option<u32>)> = program.files.iter().map(|f| (f.records.as_slice(), f.record_max)).collect();
        layout::build(&program.working_storage, &files, &[], &program.linkage, &program.local_storage, crate::picture::Notation::of(&program.environment), qualify, None).ok()
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
pub(crate) fn unsigned_integer(layout: &Layout, r: &Ref) -> Option<(u32, Usage)> {
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
        errors.push(syntax::messages::IWC0108.at(pos, format!("WRITE ... END-OF-PAGE: the FD of {} has no LINAGE clause", f.name)));
    }
    if let (Some(Advancing::Mnemonic { name, .. }), Some(_)) = (advancing, &f.linage) {
        errors.push(syntax::messages::IWR0014.at(pos, format!("WRITE ... ADVANCING {name} on {}, whose FD has LINAGE, is not supported yet", f.name)));
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
        Stmt::Set { set: SetStmt::To { targets, .. } | SetStmt::Entry { targets, .. } | SetStmt::AddressOf { targets, .. } | SetStmt::UpDown { targets, .. }, .. } => out.extend(targets),
        Stmt::Accept { target, .. } => out.push(target),
        Stmt::Read(r) => out.extend(&r.into),
        Stmt::String(st) => out.extend(std::iter::once(&st.into).chain(&st.pointer)),
        Stmt::Unstring(u) => {
            out.extend(u.into.iter().flat_map(|i| std::iter::once(&i.target).chain(&i.delimiter_in).chain(&i.count_in)));
            out.extend(u.pointer.iter().chain(&u.tallying));
        }
        Stmt::Inspect(i) => {
            if let Operand::Ref(r) = &i.target
                && (!i.replacing.is_empty() || i.converting.is_some())
            {
                out.push(r);
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
            errors.push(syntax::messages::IWC0109.at(r.pos, "LINAGE-COUNTER can be read, but no statement can change it"));
        }
    }
}
