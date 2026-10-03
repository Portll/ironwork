//! IDCAMS PRINT: a data set's records listed in CHARACTER, HEX or DUMP format, from SKIP or
//! FROMKEY to COUNT or TOKEY.

use jcl::idcams::{Keys, Print};
use zarch::ebcdic::CodePage;

/// What PRINT reads: the data set's name as the listing shows it, and its records as EBCDIC
/// bytes in the order the data set holds them (a cluster's in key or record-number order).
pub(super) struct Input<'a> {
    pub name: &'a str,
    pub records: Vec<Vec<u8>>,
    pub kind: Kind,
}

/// How the listing identifies each record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    /// A key-sequenced cluster or an alternate index, by its key.
    Keyed(Keys),
    /// An entry-sequenced cluster, by relative byte address.
    Entry,
    /// A relative record cluster, by relative record number.
    Numbered,
    /// A sequential data set, by its place in the data set.
    Nonvsam,
}

/// The listing's lines go into `out`; the condition code is returned.
pub(super) fn print(command: &Print, input: &Input<'_>, page: &CodePage, out: &mut Vec<String>) -> u16 {
    let _ = (command.format, input.kind, page);
    out.push(format!("ironwork: PRINT of {} ({} records) is not built yet", input.name, input.records.len()));
    12
}
