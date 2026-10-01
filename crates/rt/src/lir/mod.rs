//! The LIR of docs/lir.md: a program lowered once, which the VM runs and a load module holds.

mod arith;
mod call;
mod class;
mod codec;
mod collating;
mod debug;
mod file;
mod flow;
mod markup;
mod payload;
mod place;
mod sort;
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
pub use markup::{
    Ccsid, Convert, Flag, Indicator, JsonGenerate, JsonLeaf, JsonNode, JsonParse, JsonValue, Marker, Markup, Named, NumberInto, ParseLeaf, ParseNode, ParseValue,
    SetTo, XmlForm, XmlGenerate, XmlNode, XmlParse, XmlRegister, XmlValue,
};
pub use flow::{Declaratives, Frame, FrameKind, Op, Range, RangeKind, Resume, ReturnPoint, Returns, Step, Terminator};
pub use crate::cics::CicsCommand;
pub use payload::{
    Argument, DisplayItem, DisplayPlan, FloatFrom, Func, FunctionPlan, Image, InitField, InitPlan,
    InvokePlan, MethodName, MovePlan, NationalFrom, NumericFrom, Receiver, ReportOp, SearchAllPlan, SearchKey, TrimSide,
};
pub use place::{Base, Odo, Place, RefMod, Subscript};
pub use sort::{FileSort, ReleasePlan, ReturnPlan, SortIo, SortKey, SortKeys, SortPlan, TableSort};
pub use sql::{HostPlace, SqlEntry, SqlStatement, Sqlca, SqlcaField};
pub use text::{
    Bound, Chars, ConvertTable, Converting, DelimiterIn, InspectPhrase, InspectPlan, Replacement, StringPlan,
    StringSource, UnstringInto, UnstringPlan,
};
pub use value::{ByteClass, Compare, Comparand, Cond, Const, Count, Expr, IntExpr, Operand, SignTest, SqlTest};

use crate::abend::AbendCode;
use crate::{codec_enum, codec_struct};
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
pub type MarkupId = u32;

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
    pub edits: Vec<Edit>,
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
    /// DECIMAL-POINT IS COMMA: numeric editing shows a comma for the decimal point, and the NUMVAL
    /// and TEST-NUMVAL functions read one.
    pub decimal_point_comma: bool,
    /// The cs NUMVAL-C and TEST-NUMVAL-C take without argument-2 (assumption C102).
    pub numval_currency: String,
    /// What FUNCTION WHEN-COMPILED gives; None in a program that does not use it, so its module
    /// does not depend on when it was compiled.
    pub when_compiled: Option<CompileTime>,
}

/// An edited PICTURE's symbols, and the currency sign value its currency symbol stands for, empty
/// when it has none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub syms: Vec<Sym>,
    pub currency: String,
}

/// When the program was compiled, which FUNCTION WHEN-COMPILED gives: seconds since
/// 1970-01-01T00:00:00Z and hundredths, and where the time came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompileTime {
    pub seconds: i64,
    pub hundredths: u32,
    pub source: TimeSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeSource {
    /// The build's SOURCE_DATE_EPOCH, whole seconds (reproducible-builds.org/specs/source-date-epoch).
    SourceDateEpoch,
    Clock,
}

impl CompileTime {
    /// 9999-12-31T23:59:59Z, the last second WHEN-COMPILED's four-digit year can show.
    pub const LATEST: i64 = 253_402_300_799;
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
    /// JSON GENERATE, JSON PARSE, XML GENERATE and XML PARSE, by `Op::Markup`.
    pub markup: Vec<Markup>,
}

codec_struct!(Program {
    id, options, initial, recursive, storage, items, paragraphs, procedure_start, ranges, blocks, places, exprs,
    conds, consts, plans, services, sql, abends, edits, symbols, debug,
} check program_valid);
codec_struct!(ProgramOptions { options, ssrange, cards, collating, decimal_point_comma, numval_currency, when_compiled });
codec_struct!(Edit { syms, currency });
codec_struct!(CompileTime { seconds, hundredths, source } check compile_time_valid);
codec_enum!(TimeSource { SourceDateEpoch = 0, Clock = 1 });
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
codec_struct!(Services { file_ops, files, calls, sorts, releases, returns, invokes, cics, sqlca, entries, class, declaratives, markup });

pub(crate) fn program_valid(program: &Program) -> Result<(), String> {
    sql::table_valid(&program.sql, &program.symbols)
}

fn compile_time_valid(t: &CompileTime) -> Result<(), String> {
    let whole = t.source == TimeSource::SourceDateEpoch && t.hundredths != 0;
    if !(0..=CompileTime::LATEST).contains(&t.seconds) || t.hundredths > 99 || whole {
        return Err(format!("a compile time of {} seconds and {} hundredths from {:?}", t.seconds, t.hundredths, t.source));
    }
    Ok(())
}

fn storage_valid(storage: &Storage) -> Result<(), String> {
    if u32::try_from(storage.image.len()) == Ok(storage.size) {
        Ok(())
    } else {
        Err(format!("an image of {} bytes for a slab of {}", storage.image.len(), storage.size))
    }
}
