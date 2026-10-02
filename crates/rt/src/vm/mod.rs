//! The VM (codegen-runtime.md §14, step 3): runs a lowered program, and the programs it CALLs, over
//! the run unit, calling the semantics library the interpreter calls with the `Loc`s and values the
//! interpreter would pass, so the two agree by construction. Control follows lir.md §8: frames and
//! return points per activation (assumption C99), a dispatch loop over blocks, and Rust recursion
//! for CALL and for a procedure a statement runs, bounded by `MAX_DEPTH` as the interpreter is.
//! What this slice does not run stops the run as [`Halt::Unimplemented`], never as an abend.

mod arith;
mod call;
mod cics;
mod cond;
mod files;
mod flow;
mod markup;
mod ops;
mod place;
mod report;
mod sort;
mod sql;
mod value;

use crate::abend::{Abend, Ending};
use crate::cics::Handlers;
use crate::lir::{AbendId, Block, Collating, DebugId, Frame, FrameKind, MovePlan, Op, PlaceId, Program, Returns, StorePlan, SymId, UpDown};
use crate::picture::Sym;
use crate::sql::Ran;
use crate::store::ProgramFacts;
use crate::unit::{Loader, RunUnit};
use crate::vocab::{Figurative, Pos};
use numeric::Options;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use zarch::ebcdic::{self, CodePage, Collation};

pub use cics::run_task;
pub(crate) use flow::Arrival;

type R<T> = Result<T, Halt>;

/// Why a VM run stopped other than by ending.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Halt {
    Abend(Abend),
    /// A construct this slice of the VM does not run yet, named.
    Unimplemented(String),
}

impl From<Abend> for Halt {
    fn from(a: Abend) -> Self {
        Self::Abend(a)
    }
}

fn not_yet(what: impl Into<String>) -> Halt {
    Halt::Unimplemented(what.into())
}

/// A program as the VM's run unit holds it: its LIR, or why it did not lower, with its ENTRY names,
/// the file count and storage size the run unit gives it whether or not it lowered, and the
/// PROGRAM-IDs of the programs it contains, which a CANCEL of it reaches.
pub struct Code {
    lowered: Result<Lowered, String>,
    entries: Vec<String>,
    files: usize,
    size: usize,
    nested: Vec<String>,
}

/// A lowered program with what the VM works out from it once: its collating sequence as the
/// semantics library takes it, the paragraph each entry block begins, and the name a TRUNC(OPT)
/// report gives each binary receiver.
struct Lowered {
    program: Program,
    collation: Collation,
    ordinals: Vec<u8>,
    high_value: u8,
    low_value: u8,
    entry_of: Vec<Option<u32>>,
    receivers: HashMap<PlaceId, SymId>,
}

impl Code {
    pub fn new(program: Result<Program, String>, entries: Vec<String>, files: usize, size: usize, nested: Vec<String>) -> Self {
        Self { lowered: program.map(Lowered::new), entries, files, size, nested }
    }

    pub fn program(&self) -> Option<&Program> {
        self.lowered.as_ref().ok().map(|l| &l.program)
    }

    /// Which ENTRY statement, in source order, has this name.
    pub fn entry(&self, name: &str) -> Option<usize> {
        self.entries.iter().position(|e| e == name)
    }

    pub fn shape(&self) -> (usize, usize) {
        (self.files, self.size)
    }

    pub fn nested(&self) -> &[String] {
        &self.nested
    }
}

impl Lowered {
    fn new(program: Program) -> Self {
        let (collation, ordinals, high_value, low_value) = match &program.options.collating {
            Collating::Native => (Collation::Native, (0..=255).collect(), ebcdic::HIGH_VALUE, ebcdic::LOW_VALUE),
            Collating::Sequence(s) => {
                let weights = Box::new(std::array::from_fn(|b| u16::from(s.positions[b])));
                (Collation::Weights(weights), s.characters.clone(), s.high_value, s.low_value)
            }
        };
        let mut entry_of = vec![None; program.blocks.len()];
        for (i, paragraph) in program.paragraphs.iter().enumerate() {
            if let Some(slot) = entry_of.get_mut(paragraph.entry as usize) {
                *slot = Some(i as u32);
            }
        }
        let receivers = receivers(&program);
        Self { program, collation, ordinals, high_value, low_value, entry_of, receivers }
    }
}

/// Each place a binary store plan names with the item the TRUNC(OPT) report gives it, which differs
/// from the place's own name where a condition-name reaches its conditional variable.
fn receivers(p: &Program) -> HashMap<PlaceId, SymId> {
    let mut names = HashMap::new();
    let mut store = |place: PlaceId, plan: &StorePlan| {
        if let StorePlan::Binary { name, .. } = plan {
            names.entry(place).or_insert(*name);
        }
    };
    let moved = |plan: &MovePlan| match plan {
        MovePlan::Numeric { store, .. } => Some(*store),
        _ => None,
    };
    for a in &p.plans.arith {
        a.steps.iter().for_each(|s| store(s.target, &s.store));
        a.remainder.iter().for_each(|r| store(r.target, &r.store));
    }
    for Block { ops, .. } in &p.blocks {
        for op in ops {
            match op {
                Op::Move { to, plan, .. } | Op::Set { to, plan, .. } | Op::Accept { target: to, plan, .. } => moved(plan).iter().for_each(|s| store(*to, s)),
                Op::Step { var, plan, .. } => store(*var, &plan.store),
                Op::SetUpDown { targets, .. } => targets.iter().for_each(|(place, how)| {
                    if let UpDown::Number(plan) = how {
                        store(*place, &plan.store);
                    }
                }),
                _ => {}
            }
        }
    }
    for s in &p.plans.string {
        s.pointer.iter().for_each(|(place, plan)| store(*place, plan));
    }
    for u in &p.plans.unstring {
        u.pointer.iter().for_each(|(place, plan)| store(*place, plan));
        u.tallying.iter().for_each(|(place, plan)| store(*place, &plan.store));
        for into in &u.into {
            moved(&into.plan).iter().for_each(|s| store(into.target, s));
            into.count.iter().for_each(|(place, plan)| store(*place, plan));
        }
    }
    for i in &p.plans.inspect {
        i.tallying.iter().filter_map(|t| t.counter.as_ref()).for_each(|(place, plan)| store(*place, &plan.store));
    }
    for s in &p.plans.search_all {
        store(s.index, &s.store);
    }
    names
}

/// Runs program `me` of the run unit, `code`, as its first program, its PROCEDURE DIVISION USING
/// items given `arguments`' addresses.
pub fn run<L: Loader<Rc<Code>>>(code: &Code, me: usize, unit: &mut RunUnit<'_, Rc<Code>, L>, arguments: &[Option<usize>]) -> Result<Ending, Halt> {
    let lowered = code.lowered.as_ref().map_err(|why| not_yet(format!("a program that does not lower ({why})")))?;
    let mut vm = Vm::activation(lowered, me, unit, true)?;
    for (&record, &address) in lowered.program.storage.using.iter().zip(arguments) {
        vm.linkage[usize::from(record)] = address;
    }
    vm.run_from(None)
}

/// What the semantics library reads of the running program, answered from its LIR. A `Loc`'s
/// item is the place it was evaluated from.
#[derive(Clone, Copy)]
struct Facts<'p> {
    code: &'p Lowered,
}

impl ProgramFacts for Facts<'_> {
    fn options(&self) -> Options {
        self.code.program.options.options
    }

    fn page(&self) -> &'static CodePage {
        self.code.program.options.options.code_page()
    }

    fn figurative(&self, f: Figurative) -> u8 {
        match f {
            Figurative::HighValue => self.code.high_value,
            Figurative::LowValue => self.code.low_value,
            Figurative::Space => ebcdic::SPACE,
            Figurative::Zero => ebcdic::ZERO,
            Figurative::Quote => self.options().quote.byte(),
            Figurative::Null => 0,
        }
    }

    fn collation(&self) -> &Collation {
        &self.code.collation
    }

    fn ordinal(&self, byte: u8) -> u16 {
        self.code.collation.weight(byte) + 1
    }

    fn character(&self, ordinal: i64) -> Option<u8> {
        usize::try_from(ordinal).ok().and_then(|n| n.checked_sub(1)).and_then(|i| self.code.ordinals.get(i)).copied()
    }

    fn characters(&self) -> usize {
        self.code.ordinals.len()
    }

    fn decimal_point(&self) -> char {
        if self.code.program.options.decimal_point_comma { ',' } else { '.' }
    }

    fn edit(&self, edit: u32) -> (&[Sym], &str) {
        let e = &self.code.program.edits[edit as usize];
        (&e.syms, &e.currency)
    }

    fn scaling(&self, item: usize) -> u32 {
        self.code.program.places.get(item).map_or(0, |p| p.scaling)
    }

    fn item_name(&self, item: usize) -> String {
        let p = &self.code.program;
        let name = match (u32::try_from(item).ok().and_then(|i| self.code.receivers.get(&i)), p.places.get(item)) {
            (Some(&name), _) => name,
            (None, Some(place)) => place.name,
            (None, None) => return "RETURN-CODE".into(),
        };
        p.symbols[name as usize].clone()
    }
}

/// One activation of one program: its storage, its LINKAGE addresses, its return points and
/// frames, and the registers of the DEBUG option (lir.md §8.4, §9.10).
struct Vm<'p, 'u, 'w, L: Loader<Rc<Code>>> {
    code: &'p Lowered,
    p: &'p Program,
    me: usize,
    base: usize,
    local_base: usize,
    linkage: Vec<Option<usize>>,
    /// The run unit's first program, where EXIT PROGRAM does nothing.
    main: bool,
    returns: Returns,
    segment: u8,
    /// The line register, which DEBUG-LINE shows.
    line: u32,
    arrival: Arrival,
    /// A debugging section is running.
    debugging: bool,
    /// Places being evaluated, inside which FUNCTION RANDOM is not run.
    locating: u32,
    /// What a callback of the semantics library stopped for, its error being an `Abend`.
    pending: Option<String>,
    /// The JSON walk's subscripts and XML PARSE's fragment registers.
    markup: markup::State,
    io: files::State,
    /// HANDLE CONDITION, IGNORE CONDITION and HANDLE ABEND, which belong to the program level.
    cics_handlers: Handlers,
    /// SQLCODE and SQLWARN0 of the last EXEC SQL statement, which WHENEVER tests.
    whenever: Option<Ran>,
    unit: &'u mut RunUnit<'w, Rc<Code>, L>,
}

impl<'p, 'u, 'w, L: Loader<Rc<Code>>> Vm<'p, 'u, 'w, L> {
    /// An activation of loaded program `me`, its storage as `Machine::activation` leaves it: fresh
    /// on its first activation, after a CANCEL, and on every activation of an INITIAL program.
    fn activation(code: &'p Lowered, me: usize, unit: &'u mut RunUnit<'w, Rc<Code>, L>, main: bool) -> R<Self> {
        let p = &code.program;
        let storage = &p.storage;
        if !storage.local_image.is_empty() && (storage.init_abend.is_some() || !storage.init_reports.is_empty()) {
            return Err(not_yet("VALUE initialization that reports or abends in a program with LOCAL-STORAGE"));
        }
        if p.options.options.numcheck.is_some() {
            return Err(not_yet("NUMCHECK"));
        }
        if p.options.options.parmcheck.is_some() {
            return Err(not_yet("PARMCHECK"));
        }
        let (base, fresh) = unit.activate(me, p.initial);
        let main_frame = Frame { id: 0, kind: FrameKind::Main, displaced: None, segment: 0, depth: unit.depth as u32, temps: Vec::new() };
        let mut vm = Self {
            code,
            p,
            me,
            base,
            local_base: 0,
            linkage: vec![None; storage.linkage.len()],
            main,
            returns: Returns { armed: vec![None; p.paragraphs.len()], saved: BTreeMap::new(), frames: vec![main_frame], next_frame: 1 },
            segment: 0,
            line: 0,
            arrival: Arrival::Start,
            debugging: false,
            locating: 0,
            pending: None,
            markup: markup::State::default(),
            io: files::State::default(),
            cics_handlers: Handlers::default(),
            whenever: None,
            unit,
        };
        if !storage.local_image.is_empty() {
            vm.local_base = vm.unit.push_temporary(&storage.local_image);
            vm.unit.mark_input(vm.local_base, storage.local_image.len(), false);
        }
        if fresh {
            vm.unit.mem[base..base + storage.image.len()].copy_from_slice(&storage.image);
            vm.unit.mark_input(base, storage.image.len(), false);
            for &report in &storage.init_reports {
                let _ = writeln!(vm.unit.err, "{}", vm.sym(report));
            }
            if let Some(abend) = storage.init_abend {
                return Err(vm.abend(abend, None).into());
            }
            vm.unit.initialized(me);
        }
        Ok(vm)
    }

    fn facts(&self) -> Facts<'p> {
        Facts { code: self.code }
    }

    fn sym(&self, id: SymId) -> &'p str {
        &self.p.symbols[id as usize]
    }

    fn pos(&self, at: DebugId) -> Pos {
        self.p.debug.positions[at as usize]
    }

    /// Abend `id` of the program's table, at its own position or else `at`.
    fn abend(&self, id: AbendId, at: Option<DebugId>) -> Abend {
        let text = &self.p.abends[id as usize];
        let pos = text.at.or(at).map(|d| self.pos(d)).unwrap_or_default();
        Abend { code: text.code.clone(), message: self.sym(text.message).to_owned(), pos, file: None }
    }

    /// A callback's result as the semantics library takes it: an `Unimplemented` is held in
    /// `pending` behind an abend that [`Vm::settle`] turns back into it.
    fn lift<T>(&mut self, result: R<T>, pos: Pos) -> Result<T, Abend> {
        result.map_err(|halt| match halt {
            Halt::Abend(a) => a,
            Halt::Unimplemented(what) => {
                self.pending.get_or_insert(what);
                Abend::ironwork("the VM stopped inside a library call", pos)
            }
        })
    }

    /// A library call's result, with what a callback stopped for in place of its abend.
    fn settle<T>(&mut self, result: Result<T, Abend>) -> R<T> {
        match (result, self.pending.take()) {
            (_, Some(what)) => Err(Halt::Unimplemented(what)),
            (result, None) => result.map_err(Halt::Abend),
        }
    }
}
