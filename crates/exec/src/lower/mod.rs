//! Lowering (docs/lir.md): a compiled program with every name resolved, every category decided and
//! every transfer of control made explicit, as the LIR a VM runs and a load module holds.
//!
//! Lowered so far: storage, places, expressions and conditions, the arithmetic verbs, MOVE, IF,
//! EVALUATE, DISPLAY, INITIALIZE, PERFORM, GO TO, GO TO DEPENDING ON, ALTER, EXIT, STOP RUN, GOBACK,
//! CALL, CANCEL, ENTRY, INVOKE, SET, STRING, UNSTRING, INSPECT, SEARCH, ACCEPT, the file
//! statements, intrinsic functions, independent segments, class definitions, USE AFTER
//! EXCEPTION/ERROR, USE FOR DEBUGGING, JSON and XML GENERATE and PARSE, the EXEC blocks, SORT,
//! MERGE, RELEASE and RETURN, and the Report Writer.
//! Anything else is [`LowerError::Unsupported`], naming the construct.

mod call;
mod cics;
mod class;
mod cond;
mod data;
mod file;
mod flow;
mod function;
mod markup;
mod plans;
mod report;
mod search;
mod set;
mod sort;
mod sql;
mod text;
mod verify;

#[cfg(test)]
mod tests;

pub use verify::verify;

use crate::Compiled;
use crate::layout::{Layout, Resolved};
use crate::machine::Machine;
use crate::unit::{AddProgram, Clock, Library, RunUnit};
use rt::abend::AbendCode;
use rt::lir::{self, AbendId, BlockId, ConstId, DebugId, PlaceId, RangeId, SymId};
use std::collections::{BTreeSet, HashMap};
use std::fmt;
use syntax::Pos;
use syntax::ast;
use zarch::ebcdic::CodePage;

/// Why a program does not lower.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LowerError {
    /// A construct this slice does not lower yet, and where the program uses it.
    Unsupported(&'static str, Pos),
    /// A table larger than the LIR's ids can index.
    Exceeds(&'static str, Pos),
    /// The lowered program fails [`verify`], which is a fault in lowering.
    Invalid(String),
}

impl LowerError {
    pub fn pos(&self) -> Pos {
        match self {
            Self::Unsupported(_, pos) | Self::Exceeds(_, pos) => *pos,
            Self::Invalid(_) => Pos::default(),
        }
    }
}

impl fmt::Display for LowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(what, _) => write!(f, "lowering: {what} is not lowered yet"),
            Self::Exceeds(what, _) => write!(f, "lowering: {what} exceeds the LIR's limit"),
            Self::Invalid(why) => write!(f, "lowering: the lowered program is invalid: {why}"),
        }
    }
}

impl From<LowerError> for syntax::Error {
    fn from(e: LowerError) -> Self {
        syntax::Error::at(e.pos(), e.to_string())
    }
}

type R<T> = Result<T, LowerError>;

fn unsupported<T>(what: &'static str, pos: Pos) -> R<T> {
    Err(LowerError::Unsupported(what, pos))
}

/// The next index of a table, refused past `u32`.
fn next_id<T>(table: &[T], what: &'static str) -> R<u32> {
    u32::try_from(table.len()).map_err(|_| LowerError::Exceeds(what, Pos::default()))
}

fn push<T>(table: &mut Vec<T>, value: T, what: &'static str) -> R<u32> {
    let id = next_id(table, what)?;
    table.push(value);
    Ok(id)
}

/// Lowers one compiled program, or a class definition with its data and methods. One that passes
/// Check and uses only the constructs lowered so far lowers.
pub fn lower(compiled: &Compiled) -> Result<lir::Program, LowerError> {
    let mut l = Lower::new(compiled);
    let id = l.sym(&compiled.program.id);
    let sources = compiled.program.sources.iter().map(|s| l.sym(s)).collect();
    let storage = l.storage()?;
    let items = l.items()?;
    l.services.files = l.files()?;
    l.services.declaratives = l.declaratives()?;
    l.services.report = l.report_writer()?;
    (l.sql, l.services.sqlca) = l.sql_table()?;
    let paragraphs = l.procedure()?;
    l.services.entries = l.entry_points()?;
    l.services.class = l.class_definition()?;
    let procedure_start = compiled.program.report_writer.procedure_start.min(compiled.program.paragraphs.len());
    let (blocks, ops) = l.blocks.finish()?;
    let program = lir::Program {
        id,
        options: lir::ProgramOptions {
            options: compiled.options,
            ssrange: compiled.ssrange,
            cards: compiled.program.options.clone(),
            collating: collating(&compiled.collating),
            decimal_point_comma: compiled.program.environment.decimal_point_comma,
            numval_currency: crate::machine::numval_currency(&compiled.program.environment.currency),
            when_compiled: l.plans.function.iter().any(|f| f.func == lir::Func::WhenCompiled).then_some(compiled.when_compiled),
        },
        initial: compiled.program.initial,
        recursive: compiled.program.recursive,
        storage,
        items,
        paragraphs,
        procedure_start: procedure_start as u32,
        ranges: l.ranges,
        blocks,
        places: l.places,
        exprs: l.exprs,
        conds: l.conds,
        consts: l.consts,
        plans: l.plans,
        services: l.services,
        sql: l.sql,
        abends: l.abends,
        edits: edits(&compiled.layout)?,
        symbols: l.symbols,
        debug: lir::Debug { sources, positions: l.positions, ops },
    };
    if cfg!(debug_assertions) {
        verify(&program).map_err(LowerError::Invalid)?;
    }
    Ok(program)
}

/// The tables a program lowers into, each interned where equal entries may be shared.
struct Lower<'c> {
    c: &'c Compiled,
    layout: &'c Layout,
    program: &'c ast::Program,
    page: &'static CodePage,
    symbols: Vec<String>,
    symbol_ids: HashMap<String, SymId>,
    positions: Vec<Pos>,
    position_ids: HashMap<(u16, u32, u32), DebugId>,
    places: Vec<lir::Place>,
    /// The layout item each place names, None for RETURN-CODE the program does not declare.
    place_items: Vec<Option<usize>>,
    place_ids: HashMap<String, PlaceId>,
    exprs: Vec<lir::Expr>,
    conds: Vec<lir::Cond>,
    consts: Vec<lir::Const>,
    const_ids: HashMap<String, ConstId>,
    abends: Vec<lir::AbendText>,
    abend_ids: HashMap<String, AbendId>,
    plans: lir::Plans,
    services: lir::Services,
    /// Every EXEC SQL block's entry, by ordinal.
    sql: Vec<lir::SqlEntry>,
    ranges: Vec<lir::Range>,
    range_ids: HashMap<(u32, u32, lir::RangeKind), RangeId>,
    blocks: flow::Blocks,
    temps: u16,
    /// Each paragraph's entry block.
    entries: Vec<BlockId>,
    /// The block that starts statement k of paragraph p, after a separator period.
    sentences: HashMap<(usize, usize), BlockId>,
    /// The block that starts statement k of paragraph p, after an ENTRY statement.
    entry_blocks: HashMap<(usize, usize), BlockId>,
    /// The paragraphs an ALTER names.
    altered: BTreeSet<usize>,
    /// Whether an ALTER names a paragraph of an independent segment, which makes the segment
    /// control reaches observable.
    segments: bool,
    /// Under the DEBUG option, whether a debugging section serves a paragraph.
    debugging: bool,
}

impl<'c> Lower<'c> {
    fn new(c: &'c Compiled) -> Self {
        Self {
            c,
            layout: &c.layout,
            program: &c.program,
            page: c.options.code_page(),
            symbols: Vec::new(),
            symbol_ids: HashMap::new(),
            positions: Vec::new(),
            position_ids: HashMap::new(),
            places: Vec::new(),
            place_items: Vec::new(),
            place_ids: HashMap::new(),
            exprs: Vec::new(),
            conds: Vec::new(),
            consts: Vec::new(),
            const_ids: HashMap::new(),
            abends: Vec::new(),
            abend_ids: HashMap::new(),
            plans: lir::Plans::default(),
            services: lir::Services::default(),
            sql: Vec::new(),
            ranges: Vec::new(),
            range_ids: HashMap::new(),
            blocks: flow::Blocks::default(),
            temps: 0,
            entries: Vec::new(),
            sentences: HashMap::new(),
            entry_blocks: HashMap::new(),
            altered: BTreeSet::new(),
            segments: false,
            debugging: !c.declaratives.triggers.is_empty(),
        }
    }

    fn sym(&mut self, text: &str) -> SymId {
        if let Some(&id) = self.symbol_ids.get(text) {
            return id;
        }
        let id = self.symbols.len() as SymId;
        self.symbols.push(text.to_owned());
        self.symbol_ids.insert(text.to_owned(), id);
        id
    }

    fn at(&mut self, pos: Pos) -> DebugId {
        let key = (pos.file, pos.line, pos.col);
        if let Some(&id) = self.position_ids.get(&key) {
            return id;
        }
        let id = self.positions.len() as DebugId;
        self.positions.push(pos);
        self.position_ids.insert(key, id);
        id
    }

    /// An abend's text, with a position of its own only where no op gives one.
    fn abend(&mut self, code: AbendCode, message: &str, pos: Option<Pos>) -> R<AbendId> {
        let at = pos.map(|p| self.at(p));
        let key = format!("{code:?}{at:?}{message}");
        if let Some(&id) = self.abend_ids.get(&key) {
            return Ok(id);
        }
        let text = lir::AbendText { code, message: self.sym(message), at };
        let id = push(&mut self.abends, text, "abend messages")?;
        self.abend_ids.insert(key, id);
        Ok(id)
    }

    fn ironwork(&mut self, message: &str) -> R<AbendId> {
        self.abend(AbendCode::Ironwork, message, None)
    }

    /// The slab and LOCAL-STORAGE as the walker's own VALUE initialization leaves them, run once
    /// in a run unit of their own, with what it reported and any abend.
    fn storage(&mut self) -> R<lir::Storage> {
        let layout = self.layout;
        let (size, local) = (layout.size as usize, layout.local_size as usize);
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let (image, local_image, abend) = {
            let mut unit = RunUnit::new(Library::default(), crate::files::Dds::default(), None, Clock::Fixed(0, 0), &mut out, &mut err);
            let me = unit.add(None, self.program, size);
            let abend = Machine::activation(self.c, me, &mut unit, true).err();
            let base = unit.programs[me].base;
            let image = unit.mem[base..base + size].to_vec();
            let local_image = if local > 0 { unit.mem[unit.mem.len() - local..].to_vec() } else { Vec::new() };
            (image, local_image, abend)
        };
        let init_reports = String::from_utf8_lossy(&err).lines().map(|l| self.sym(l)).collect();
        let init_abend = match abend {
            Some(a) => Some(self.abend(a.code.clone(), &a.message, Some(a.pos))?),
            None => None,
        };
        let root = |name: &str| layout.linkage_roots.iter().position(|&i| layout.items[i].name.as_deref() == Some(name));
        let mut using = Vec::new();
        for param in &self.program.using {
            match root(&param.name).map(u16::try_from) {
                Some(Ok(ordinal)) => using.push(ordinal),
                Some(Err(_)) => return Err(LowerError::Exceeds("LINKAGE records", Pos::default())),
                None => return unsupported("PROCEDURE DIVISION USING an item that is not a LINKAGE record", Pos::default()),
            }
        }
        let returning = match &self.program.returning {
            None => None,
            Some(name) => match root(name).map(u16::try_from) {
                Some(Ok(ordinal)) => Some(ordinal),
                Some(Err(_)) => return Err(LowerError::Exceeds("LINKAGE records", Pos::default())),
                None => return unsupported("RETURNING an item that is not a LINKAGE record", Pos::default()),
            },
        };
        Ok(lir::Storage {
            size: layout.size,
            image,
            local_image,
            init_reports,
            init_abend,
            linkage: layout.linkage_roots.iter().map(|&i| layout.items[i].size).collect(),
            using,
            returning,
            file_areas: layout.file_areas.clone(),
        })
    }

    /// The layout's items for `dump`, with DEPENDING ON objects and keys as item indices.
    fn items(&mut self) -> R<Vec<lir::Item>> {
        let layout = self.layout;
        let mut items = Vec::with_capacity(layout.items.len());
        for item in &layout.items {
            let item_of = |r: &ast::Ref| match layout.resolve(&r.name, &r.qualifiers, r.pos) {
                Ok(Resolved::Item(i)) => Ok(i as u32),
                _ => unsupported("an OCCURS DEPENDING ON or KEY that names no data item", r.pos),
            };
            let depending_on = item.depending_on.as_ref().map(item_of).transpose()?;
            let keys = item.keys.iter().map(|(ascending, r)| item_of(r).map(|i| (*ascending, i))).collect::<R<_>>()?;
            items.push(lir::Item {
                name: item.name.as_deref().map(|n| self.sym(n)),
                level: item.level,
                parent: item.parent.map(|p| p as u32),
                offset: item.offset,
                size: item.size,
                occurs: item.occurs,
                dims: item.dims.clone(),
                kind: item.kind,
                local: item.local,
                linkage: item.linkage,
                redefines: item.redefines.as_deref().map(|n| self.sym(n)),
                depending_on,
                keys,
                at: self.at(item.pos),
            });
        }
        Ok(items)
    }
}

/// The program's sequence, from what `collating::Sequence` exposes: each byte's position, the
/// character FUNCTION CHAR gives for each position, and HIGH-VALUE and LOW-VALUE.
fn collating(sequence: &crate::collating::Sequence) -> lir::Collating {
    if sequence.is_native() {
        return lir::Collating::Native;
    }
    let characters = (1..=sequence.count() as i64).filter_map(|k| sequence.character(k)).collect();
    lir::Collating::Sequence(lir::Sequence {
        positions: Box::new(sequence.positions()),
        characters,
        high_value: sequence.high_value,
        low_value: sequence.low_value,
    })
}

/// Each edited PICTURE with the currency sign value it shows.
fn edits(layout: &Layout) -> R<Vec<lir::Edit>> {
    if layout.edits.len() != layout.currencies.len() {
        return Err(LowerError::Invalid(format!("{} edited PICTUREs with {} currency values", layout.edits.len(), layout.currencies.len())));
    }
    Ok(layout.edits.iter().zip(&layout.currencies).map(|(syms, currency)| lir::Edit { syms: syms.clone(), currency: currency.clone() }).collect())
}

/// Whether the place cannot abend when evaluated: a slab, LOCAL-STORAGE or RETURN-CODE base and a
/// constant offset and length.
fn is_static(place: &lir::Place) -> bool {
    matches!(place.base, lir::Base::Program | lir::Base::Local | lir::Base::ReturnCode) && place.subscripts.is_empty() && place.odo.is_none() && place.refmod.is_none()
}
