//! IDCAMS LISTCAT: the catalog's entries, by name or with their attributes.

use super::Runner;
use jcl::idcams::Listcat;

pub(super) fn listcat(runner: &Runner<'_>, command: &Listcat, out: &mut Vec<String>) -> u16 {
    let _ = (runner, command);
    out.push("ironwork: LISTCAT is not built yet".into());
    12
}
