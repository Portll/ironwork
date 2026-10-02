//! Files (lir.md §9.4): each file's SELECT and FD, and each file statement with its phrases. The
//! statements stay `rt::files` calls; these name the places, plans and procedures the walker finds
//! by name on each execution.

use super::{IntExpr, MovePlan, Operand, PlaceId, RangeId, SenderCheck, StorePlan, SymId};
use crate::files::Format;
use crate::vocab::{Closing, OpenMode};
use crate::{codec_enum, codec_struct};

/// A file as SELECT and FD declare it. `format` is how its records are held when its DD does not
/// say; `status` is FILE STATUS with the MOVE its two characters take.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDesc {
    pub name: SymId,
    /// The DD name ASSIGN gives.
    pub assign: SymId,
    pub organization: Organization,
    pub access: Access,
    pub optional: bool,
    pub format: Format,
    /// The shortest and longest variable-length record a READ takes without a record length
    /// conflict (status 04), as the VLR option measures them (Programming Guide SC27-8714-03,
    /// pp. 422-424).
    pub read_lengths: (u32, u32),
    /// No RECORDING MODE V, and its smallest record as long as its largest.
    pub fixed: bool,
    /// The RECORD clause's smallest record.
    pub record_min: Option<u32>,
    pub depending: Option<RecordDepending>,
    pub status: Option<(PlaceId, MovePlan)>,
    /// An indexed file's keys, as spans of its record area.
    pub keys: Option<IndexKeys>,
    pub relative: Option<RelativeKey>,
    pub linage: Option<Linage>,
    /// A print file's control character.
    pub carriage: Option<Carriage>,
    /// Described by SD.
    pub sort: bool,
    /// Its own EXCEPTION/ERROR procedure, which comes before one for its open mode.
    pub error: Option<RangeId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Organization {
    Sequential,
    LineSequential,
    Indexed,
    Relative,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Sequential,
    Random,
    Dynamic,
}

/// RECORD IS VARYING DEPENDING ON ([`crate::fileio::Depending`]): the item a successful READ or
/// RETURN stores the record's length in and WRITE, REWRITE and RELEASE take it from, read as an
/// integer and stored as `set_integer` stores; and the shortest and longest record the clause allows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordDepending {
    pub item: PlaceId,
    pub lengths: (u32, u32),
}

/// Bytes of a record: from `offset` in the file's record area, `len` long.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordSpan {
    pub offset: u32,
    pub len: u32,
}

/// RECORD KEY, then each ALTERNATE RECORD KEY with WITH DUPLICATES; a key of reference numbers
/// them from 0 in this order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexKeys {
    pub prime: RecordSpan,
    pub alternates: Vec<(RecordSpan, bool)>,
}

/// The RELATIVE KEY: read as `value`, stored by `store` when a sequential READ or WRITE sets it,
/// and `digits` the integer digits that bound the record numbers it can hold (None: no bound).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelativeKey {
    pub place: PlaceId,
    pub value: IntExpr,
    pub store: StorePlan,
    pub digits: Option<u32>,
}

/// LINAGE: each value evaluated, in this order, whenever the page's geometry is taken (at OPEN
/// OUTPUT or EXTEND and at each new page), and LINAGE-COUNTER with its store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Linage {
    pub lines: IntExpr,
    pub footing: Option<IntExpr>,
    pub top: Option<IntExpr>,
    pub bottom: Option<IntExpr>,
    pub counter: Option<(PlaceId, StorePlan)>,
}

/// `machine`: machine codes rather than ASA characters; `reserved`: the character is the record's
/// own first byte (NOADV) rather than one added before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Carriage {
    pub machine: bool,
    pub reserved: bool,
}

/// One file statement on one file. `phrase` is AT END or INVALID KEY and its NOT phrase, as
/// written; `end_of_page` WRITE's END-OF-PAGE and NOT END-OF-PAGE. With neither the op returns
/// Next; otherwise it returns Arm(1) for the ON phrase, Arm(2) for NOT ON, Arm(3) for END-OF-PAGE,
/// Arm(4) for NOT END-OF-PAGE, and Arm(0) when no phrase written runs, and a `Select` of
/// [`FileOp::arms`] blocks follows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileOp {
    pub file: u16,
    pub verb: FileVerb,
    pub phrase: Option<Phrase>,
    pub end_of_page: Option<Phrase>,
}

impl FileOp {
    /// The blocks the `Select` after the op has: none, 3, or 5 with END-OF-PAGE.
    pub fn arms(&self) -> usize {
        match (self.phrase, self.end_of_page) {
            (None, None) => 0,
            (_, None) => 3,
            (_, Some(_)) => 5,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Phrase {
    pub on: bool,
    pub not_on: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileVerb {
    Open(OpenMode),
    Close,
    CloseWith(Closing),
    /// `sequential`: READ NEXT or PREVIOUS, or a file read in sequence, whose phrase is AT END;
    /// otherwise the phrase is INVALID KEY and `key` the key of reference of an indexed file.
    Read { sequential: bool, previous: bool, into: Option<(PlaceId, MovePlan)>, key: u8 },
    /// `record` is located after FROM has moved into it.
    Write { record: PlaceId, from: Option<FromMove>, advancing: Option<Advance> },
    Rewrite { record: PlaceId, from: Option<FromMove> },
    Delete,
    Start { rel: StartRel, key: StartKey },
}

/// WRITE, REWRITE or RELEASE FROM: `to` is the record as a receiving item, and `check` NUMCHECK's
/// test of `from` before it is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FromMove {
    pub from: Operand,
    pub to: PlaceId,
    pub plan: MovePlan,
    pub check: SenderCheck,
}

/// WRITE's ADVANCING phrase. A count below zero moves as zero; PAGE is channel 1, or the next
/// page of a LINAGE file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Advance {
    Lines { before: bool, count: IntExpr },
    Page { before: bool },
    /// A mnemonic-name of SPECIAL-NAMES, as the movement its environment-name gives.
    Mnemonic { before: bool, space: Spacing },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spacing {
    /// CSP: no spacing.
    Lines(u64),
    Channel(u8),
    /// AFP-5A: page mode data.
    PageMode,
}

/// START's relation: KEY =, KEY >, and KEY NOT < or >=.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartRel {
    Equal,
    Greater,
    NotLess,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartKey {
    /// An indexed file without KEY: the prime key's bytes in the record area.
    Prime,
    /// KEY names key `key`, or a leading part of it: the bytes of `span`.
    Named { key: u8, span: RecordSpan },
    /// A relative file's KEY, read as an integer.
    Relative(IntExpr),
    /// A relative file without KEY: its RELATIVE KEY.
    RelativeKey,
}

codec_struct!(FileDesc {
    name, assign, organization, access, optional, format, read_lengths, fixed, record_min, depending, status, keys, relative, linage, carriage, sort, error,
} check file_valid);
codec_struct!(RecordDepending { item, lengths });
codec_enum!(Organization { Sequential = 0, LineSequential = 1, Indexed = 2, Relative = 3 });
codec_enum!(Access { Sequential = 0, Random = 1, Dynamic = 2 });
codec_struct!(RecordSpan { offset, len });
codec_struct!(IndexKeys { prime, alternates });
codec_struct!(RelativeKey { place, value, store, digits });
codec_struct!(Linage { lines, footing, top, bottom, counter });
codec_struct!(Carriage { machine, reserved });
codec_struct!(FileOp { file, verb, phrase, end_of_page });
codec_struct!(Phrase { on, not_on });
codec_enum!(FileVerb {
    Open(mode) = 0,
    Close = 1,
    Read { sequential, previous, into, key } = 2,
    Write { record, from, advancing } = 3,
    Rewrite { record, from } = 4,
    Delete = 5,
    Start { rel, key } = 6,
    CloseWith(closing) = 7,
});
codec_struct!(FromMove { from, to, plan, check });
codec_enum!(Advance { Lines { before, count } = 0, Page { before } = 1, Mnemonic { before, space } = 2 });
codec_enum!(Spacing { Lines(n) = 0, Channel(c) = 1, PageMode = 2 });
codec_enum!(StartRel { Equal = 0, Greater = 1, NotLess = 2 });
codec_enum!(StartKey { Prime = 0, Named { key, span } = 1, Relative(value) = 2, RelativeKey = 3 });

/// Only an indexed file has keys.
fn file_valid(file: &FileDesc) -> Result<(), String> {
    if file.keys.is_some() == (file.organization == Organization::Indexed) {
        Ok(())
    } else {
        Err("keys on a file that is not indexed, or an indexed file without them".into())
    }
}
