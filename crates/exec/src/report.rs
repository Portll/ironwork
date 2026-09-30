//! The report writer's run-time lookups over what `compile::report` built; `machine::report` runs
//! the reports.

pub use compile::report::*;

/// The report and DETAIL group a GENERATE names: a report alone for summary reporting.
pub fn generate_target(reports: &[Report], name: &str, qualifier: Option<&str>) -> Option<(usize, Option<usize>)> {
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
