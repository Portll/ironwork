//! The LIR of docs/lir.md: a program lowered once, which the VM runs and a load module holds.

mod arith;
mod call;
mod class;
mod codec;
mod collating;
mod debug;
mod file;
mod flow;
mod payload;
mod place;
mod sql;
mod text;
mod value;

pub use arith::{ArithPlan, ArithStep, Mode, RemainderPlan, StepPlan, StorePlan, UpDown};
pub use call::{CallArg, CallPlan, CallTarget, EntryPoint, LeService};
pub use class::{Class, ClassPart, Method};
pub use collating::{Collating, Sequence};
pub use debug::Debug;
pub use file::{
    Access, Advance, Carriage, FileDesc, FileOp, FileVerb, FromMove, IndexKeys, Linage, Organization, Phrase, RecordSpan,
    RelativeKey, Spacing, StartKey, StartRel,
};
pub use flow::{Declaratives, Frame, FrameKind, Op, Range, RangeKind, Resume, ReturnPoint, Returns, Step, Terminator};
pub use crate::cics::CicsCommand;
pub use payload::{
    DisplayItem, DisplayPlan, FloatFrom, Func, FunctionPlan, Image, InitField, InitPlan,
    InvokePlan, MethodName, MovePlan, NationalFrom, NumericFrom, Receiver, ReleasePlan, ReportOp, ReturnPlan,
    SearchAllPlan, SearchKey, SortPlan, TrimSide,
};
pub use place::{Base, Odo, Place, RefMod, Subscript};
pub use sql::{HostPlace, SqlEntry, SqlStatement, Sqlca, SqlcaField};
pub use text::{
    Bound, Chars, ConvertTable, Converting, DelimiterIn, InspectPhrase, InspectPlan, Replacement, StringPlan,
    StringSource, UnstringInto, UnstringPlan,
};
pub use value::{ByteClass, Compare, Comparand, Cond, Const, Count, Expr, IntExpr, Operand, SignTest, SqlTest};

use crate::abend::AbendCode;
use crate::codec_struct;
use crate::picture::Sym;
use crate::storage::Kind;

pub type BlockId = u32;
pub type ParaId = u32;
pub type RangeId = u32;
pub type PlaceId = u32;
pub type ExprId = u32;
pub type CondId = u32;
pub type ConstId = u32;
pub type SymId = u32;
pub type DebugId = u32;
pub type AbendId = u32;
/// A PERFORM TIMES counter, held in the frame the statement runs under (`Frame.temps`).
pub type TempId = u16;
pub type SqlId = u32;
pub type ArithId = u32;
pub type InitId = u32;
pub type DisplayId = u32;
pub type InspectId = u32;
pub type StringId = u32;
pub type UnstringId = u32;
pub type SearchAllId = u32;
pub type FunctionId = u32;
pub type FileOpId = u32;
pub type CallId = u32;
pub type SortId = u32;
pub type ReleaseId = u32;
pub type ReturnId = u32;
pub type InvokeId = u32;
pub type CicsId = u32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub id: SymId,
    pub options: ProgramOptions,
    pub initial: bool,
    pub recursive: bool,
    pub storage: Storage,
    pub items: Vec<Item>,
    pub paragraphs: Vec<Paragraph>,
    /// The first paragraph after DECLARATIVES, where a run starts.
    pub procedure_start: ParaId,
    pub ranges: Vec<Range>,
    pub blocks: Vec<Block>,
    pub places: Vec<Place>,
    pub exprs: Vec<Expr>,
    pub conds: Vec<Cond>,
    pub consts: Vec<Const>,
    pub plans: Plans,
    pub services: Services,
    pub sql: Vec<SqlEntry>,
    pub abends: Vec<AbendText>,
    pub edits: Vec<Vec<Sym>>,
    pub symbols: Vec<String>,
    pub debug: Debug,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramOptions {
    pub options: numeric::Options,
    pub ssrange: bool,
    /// The CBL and PROCESS cards as written.
    pub cards: Vec<String>,
    pub collating: Collating,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Storage {
    pub size: u32,
    /// The slab and LOCAL-STORAGE as VALUE clauses leave them.
    pub image: Vec<u8>,
    pub local_image: Vec<u8>,
    /// What VALUE initialization prints (TRUNC(OPT) reports), and its abend.
    pub init_reports: Vec<SymId>,
    pub init_abend: Option<AbendId>,
    /// Each LINKAGE record's size; USING and RETURNING as record ordinals.
    pub linkage: Vec<u32>,
    pub using: Vec<u16>,
    pub returning: Option<u16>,
    /// Offset and size of each file's record area in the slab.
    pub file_areas: Vec<(u32, u32)>,
}

/// A data item for `dump` and a debugger; no executor reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub name: Option<SymId>,
    pub level: u8,
    pub parent: Option<u32>,
    pub offset: u32,
    pub size: u32,
    pub occurs: u32,
    /// Stride and count of each OCCURS on the item and its ancestors, outermost first.
    pub dims: Vec<(u32, u32)>,
    pub kind: Kind,
    pub local: bool,
    pub linkage: Option<u16>,
    pub redefines: Option<SymId>,
    /// The OCCURS DEPENDING ON object's item index.
    pub depending_on: Option<u32>,
    /// Each key's item index, true for ASCENDING.
    pub keys: Vec<(bool, u32)>,
    pub at: DebugId,
}

/// `at` is None where the op or terminator that raises the abend gives its position, and the data
/// entry's for `Storage.init_abend`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AbendText {
    pub code: AbendCode,
    pub message: SymId,
    pub at: Option<DebugId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Paragraph {
    pub name: SymId,
    pub is_section: bool,
    pub entry: BlockId,
    /// The last paragraph of its section.
    pub section_end: ParaId,
    /// Its section's priority-number, 0 for none; 50 or more is an independent segment.
    pub priority: u8,
    pub at: DebugId,
    /// On a paragraph that ends a range: the IRONWORK abend when control passes its end while it
    /// holds the return point of a frame control left that cannot resume (C99).
    pub abandoned: Option<AbendId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub ops: Vec<Op>,
    pub end: Terminator,
}

/// The plan tables ops index; a MOVE's plan sits in its op.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plans {
    pub arith: Vec<ArithPlan>,
    pub init: Vec<InitPlan>,
    pub display: Vec<DisplayPlan>,
    pub inspect: Vec<InspectPlan>,
    pub string: Vec<StringPlan>,
    pub unstring: Vec<UnstringPlan>,
    pub search_all: Vec<SearchAllPlan>,
    pub function: Vec<FunctionPlan>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Services {
    pub file_ops: Vec<FileOp>,
    pub files: Vec<FileDesc>,
    pub calls: Vec<CallPlan>,
    pub sorts: Vec<SortPlan>,
    pub releases: Vec<ReleasePlan>,
    pub returns: Vec<ReturnPlan>,
    pub invokes: Vec<InvokePlan>,
    pub cics: Vec<CicsCommand>,
    pub sqlca: Sqlca,
    /// The ENTRY statements, in source order, which a CALL of their names enters.
    pub entries: Vec<EntryPoint>,
    /// A class definition's data and methods; None for any other program.
    pub class: Option<Box<Class>>,
    pub declaratives: Declaratives,
}

codec_struct!(Program {
    id, options, initial, recursive, storage, items, paragraphs, procedure_start, ranges, blocks, places, exprs,
    conds, consts, plans, services, sql, abends, edits, symbols, debug,
} check program_valid);
codec_struct!(ProgramOptions { options, ssrange, cards, collating });
codec_struct!(Storage {
    size, image, local_image, init_reports, init_abend, linkage, using, returning, file_areas,
} check storage_valid);
codec_struct!(Item {
    name, level, parent, offset, size, occurs, dims, kind, local, linkage, redefines, depending_on, keys, at,
});
codec_struct!(AbendText { code, message, at });
codec_struct!(Paragraph { name, is_section, entry, section_end, priority, at, abandoned });
codec_struct!(Block { ops, end });
codec_struct!(Plans { arith, init, display, inspect, string, unstring, search_all, function });
codec_struct!(Services { file_ops, files, calls, sorts, releases, returns, invokes, cics, sqlca, entries, class, declaratives });

pub(crate) fn program_valid(program: &Program) -> Result<(), String> {
    sql::table_valid(&program.sql, &program.symbols)
}

fn storage_valid(storage: &Storage) -> Result<(), String> {
    if u32::try_from(storage.image.len()) == Ok(storage.size) {
        Ok(())
    } else {
        Err(format!("an image of {} bytes for a slab of {}", storage.image.len(), storage.size))
    }
}
