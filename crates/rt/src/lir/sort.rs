//! SORT, MERGE, RELEASE and RETURN (lir.md §9.6), which `rt::sort` runs. `FileSort` is generic
//! over the handles the executor resolves as the statement runs: the LIR's by default, the walker's
//! own references in the interpreter.

use super::{Count, FromMove, MovePlan, PlaceId, RangeId, SymId};
use crate::storage::Kind;
use crate::{codec_enum, codec_struct};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SortPlan {
    File(FileSort),
    Table(TableSort),
}

/// A SORT or MERGE of SD `sd`. `sort_return` and `sort_control` are SORT-RETURN and SORT-CONTROL;
/// an input or output that is None fails the statement when the sort reaches it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileSort<R = PlaceId, Q = RangeId, K = SortKeys, F = u16> {
    pub sd: u16,
    pub merge: bool,
    pub keys: K,
    pub input: Option<SortIo<Q, F>>,
    pub output: Option<SortIo<Q, F>>,
    pub sort_return: R,
    pub sort_control: R,
}

/// USING or GIVING files, or an INPUT or OUTPUT PROCEDURE.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SortIo<Q = RangeId, F = u16> {
    Files(Vec<F>),
    Procedure(Q),
}

/// The keys, most significant first, and the collating sequence of those marked `collated` when it
/// is not EBCDIC: each byte's position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SortKeys {
    pub keys: Vec<SortKey>,
    pub collating: Option<Box<[u8; 256]>>,
}

/// A key at `offset` in the record or table element, read as item `item` of `kind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SortKey {
    pub ascending: bool,
    pub offset: u32,
    pub len: u32,
    pub kind: Kind,
    pub item: u32,
    pub collated: bool,
}

/// A table SORT: `first` is the table's first element and `count` how many it holds now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableSort {
    pub first: PlaceId,
    pub count: Count,
    pub stride: u32,
    pub keys: SortKeys,
    pub name: SymId,
}

/// RELEASE of `record`, a record of file `file`, located after FROM has moved into `FromMove.to`,
/// the record as a receiving item. `name` is the record as written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReleasePlan {
    pub record: PlaceId,
    pub file: Option<u16>,
    pub from: Option<FromMove>,
    pub sort_return: PlaceId,
    pub name: SymId,
}

/// RETURN of file `file`, named `name` as written, and INTO's MOVE of the record's bytes. The op
/// returns Arm(0) at end and Arm(1) when a record came.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReturnPlan {
    pub file: Option<u16>,
    pub into: Option<(PlaceId, MovePlan)>,
    pub sort_return: PlaceId,
    pub name: SymId,
}

codec_enum!(SortPlan { File(sort) = 0, Table(sort) = 1 });
codec_struct!(FileSort { sd, merge, keys, input, output, sort_return, sort_control });
codec_enum!(SortIo { Files(files) = 0, Procedure(range) = 1 });
codec_struct!(SortKeys { keys, collating });
codec_struct!(SortKey { ascending, offset, len, kind, item, collated });
codec_struct!(TableSort { first, count, stride, keys, name });
codec_struct!(ReleasePlan { record, file, from, sort_return, name });
codec_struct!(ReturnPlan { file, into, sort_return, name });
