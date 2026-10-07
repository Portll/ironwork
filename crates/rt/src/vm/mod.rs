//! The VM (codegen-runtime.md §14, step 3): runs a lowered program, and the programs and functions
//! it calls, over the run unit, calling the semantics library the interpreter calls with the `Loc`s
//! and values the interpreter would pass, so the two agree by construction. Control follows lir.md
//! §8: frames and return points per activation (assumption C99), a dispatch loop over blocks, and
//! Rust recursion for CALL, a user-defined function and a procedure a statement runs, bounded by
//! `MAX_DEPTH` as the interpreter is.
//! What this slice does not run stops the run as [`Halt::Unimplemented`], never as an abend.

mod arith;
mod call;
mod cics;
mod cond;
mod files;
mod flow;
mod function;
mod markup;
mod oo;
mod ops;
mod place;
mod report;
mod scope;
mod sort;
mod sql;
mod value;

use crate::abend::{Abend, Ending};
use crate::cics::Handlers;
use crate::lir::{AbendId, Base, Block, Collating, DebugId, Frame, FrameKind, MovePlan, Op, Place, PlaceId, Program, ReturnPoint, Returns, StorePlan, SymId, UpDown};
use crate::oo::Running;
use crate::picture::Sym;
use crate::sql::Ran;
use crate::storage::Val;
use crate::store::{LaxRedefinition, ProgramFacts};
use crate::unit::{Loader, RunUnit};
use crate::vocab::{Figurative, Pos};
use numeric::Options;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use zarch::ebcdic::{self, CodePage, Collation};

pub use cics::run_task;
pub(crate) use flow::Arrival;

type R<T> = Result<T, Stop>;

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

/// A `Halt` in a box, as the VM's own results carry it: an `R` of a small value is then small.
#[derive(Debug)]
struct Stop(Box<Halt>);

impl Stop {
    fn halt(self) -> Halt {
        *self.0
    }
}

impl From<Halt> for Stop {
    fn from(halt: Halt) -> Self {
        Self(Box::new(halt))
    }
}

impl From<Abend> for Stop {
    fn from(a: Abend) -> Self {
        Self(Box::new(Halt::Abend(a)))
    }
}

impl From<Stop> for Halt {
    fn from(stop: Stop) -> Self {
        stop.halt()
    }
}

fn not_yet(what: impl Into<String>) -> Stop {
    Halt::Unimplemented(what.into()).into()
}

/// A program as the VM's run unit holds it: its LIR, or why it did not lower, with its ENTRY names,
/// the file count and storage size the run unit gives it whether or not it lowered, the
/// PROGRAM-IDs of the programs it contains, which a CANCEL of it reaches, for a method the
/// `Class.method` a dump lists it by, and the statement kinds, usages and options it holds.
pub struct Code {
    lowered: Result<Lowered, String>,
    entries: Vec<String>,
    files: usize,
    size: usize,
    nested: Vec<String>,
    method: Option<String>,
    facts: numeric::governs::Facts,
}

/// A lowered program with what the VM works out from it once: its collating sequence as the
/// semantics library takes it, the paragraph each entry block begins, the name a TRUNC(OPT)
/// report gives each binary receiver, and under NUMCHECK the name its message gives each
/// conditional variable (`cond::conditional_variables`).
struct Lowered {
    program: Program,
    collation: Collation,
    ordinals: Vec<u8>,
    high_value: u8,
    low_value: u8,
    entry_of: Vec<Option<u32>>,
    receivers: HashMap<PlaceId, SymId>,
    variables: HashMap<PlaceId, Option<String>>,
    pure: Vec<bool>,
    quick: Vec<Option<place::Quick>>,
    /// Each place `loc_with` takes straight from its base (`place::direct`).
    direct: Vec<bool>,
    /// Each place `static_number` reads: a static one of a numeric kind.
    numbers: Vec<bool>,
    /// Each constant's value as `operand_number` takes it where it is a number that fits an `i64`.
    literals: Vec<Option<(i64, numeric::precision::Places)>>,
}

impl Code {
    pub fn new(program: Result<Program, String>, entries: Vec<String>, files: usize, size: usize, nested: Vec<String>, method: Option<String>, facts: numeric::governs::Facts) -> Self {
        Self { lowered: program.map(Lowered::new), entries, files, size, nested, method, facts }
    }

    pub fn facts(&self) -> numeric::governs::Facts {
        self.facts
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
        let variables = if program.options.options.numcheck.is_some() { cond::conditional_variables(&program) } else { HashMap::new() };
        let pure = place::pure_places(&program);
        let quick = place::quick_places(&program);
        let literals = program.consts.iter().map(value::const_number).collect();
        let numbers = program.places.iter().map(place::number_item).collect();
        let direct = program.places.iter().map(place::direct).collect();
        Self { program, collation, ordinals, high_value, low_value, entry_of, receivers, variables, pure, quick, direct, numbers, literals }
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
/// items given `arguments`' addresses; `main` as a run unit's main program, where EXIT PROGRAM
/// does nothing, or else as a subprogram a caller passed them to.
pub fn run<L: Loader<Rc<Code>>>(code: &Code, me: usize, unit: &mut RunUnit<'_, Rc<Code>, L>, arguments: &[Option<usize>], main: bool) -> Result<Ending, Halt> {
    let lowered = code.lowered.as_ref().map_err(|why| not_yet(format!("a program that does not lower ({why})")))?;
    let mut vm = Vm::activation(lowered, me, unit, main)?;
    for (&record, &address) in lowered.program.storage.using.iter().zip(arguments) {
        vm.linkage[usize::from(record)] = address;
    }
    Ok(vm.run_from(None)?)
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

    /// The walker names an item by its `Loc`'s item, which an XML register's has none of.
    fn item_name(&self, item: usize) -> String {
        let p = &self.code.program;
        let name = match (u32::try_from(item).ok().and_then(|i| self.code.receivers.get(&i)), p.places.get(item)) {
            (Some(&name), _) => name,
            (None, Some(Place { base: Base::Xml(_), .. }) | None) => return "RETURN-CODE".into(),
            (None, Some(place)) => match self.code.variables.get(&(item as u32)) {
                Some(Some(name)) => return name.clone(),
                _ => place.name,
            },
        };
        p.symbols[name as usize].clone()
    }

    fn lax_redefinition(&self, item: usize) -> Option<LaxRedefinition> {
        self.code.program.places.get(item).and_then(|p| p.numcheck.lax)
    }

    fn numcheck_removed(&self, item: usize, _pos: Pos) -> bool {
        self.code.program.places.get(item).is_some_and(|p| p.numcheck.removed)
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
    /// The logical level's HANDLE CONDITION, IGNORE CONDITION and HANDLE ABEND, which the
    /// activation running holds (C234).
    cics_handlers: Handlers,
    /// This activation's number in the CICS task, which owns the HANDLE labels it sets.
    serial: u64,
    /// `Machine::first`: the run unit's first program, for a CALL, LINK or XCTL of it.
    first: Option<&'p Lowered>,
    /// SQLCODE and SQLWARN0 of the last EXEC SQL statement, which WHENEVER tests.
    whenever: Option<Ran>,
    /// The method this activation runs, if it is one: its class and SELF.
    method: Option<Running>,
    /// The programs containing this one, innermost first, as they are running.
    containers: Vec<scope::Container<'p>>,
    memo: Option<place::Memo>,
    /// What the activations this one CALLs leave, for the next.
    spare: Spare,
    unit: &'u mut RunUnit<'w, Rc<Code>, L>,
}

/// What an activation cannot lay out yet, refused before the program is activated.
fn check_storage(code: &Lowered) -> R<()> {
    let storage = &code.program.storage;
    if !storage.local_image.is_empty() && (storage.init_abend.is_some() || !storage.init_reports.is_empty()) {
        return Err(not_yet("VALUE initialization that reports or abends in a program with LOCAL-STORAGE"));
    }
    Ok(())
}

/// The tables an activation the VM CALLs takes, a CALL's argument lists and a FUNCTION's arguments,
/// kept from one to the next.
#[derive(Default)]
struct Spare {
    linkage: Vec<Option<usize>>,
    armed: Vec<Option<ReturnPoint>>,
    frames: Vec<Frame>,
    using: Vec<Option<usize>>,
    addresses: Vec<Option<usize>>,
    args: Vec<Val>,
}

impl<'p, 'u, 'w, L: Loader<Rc<Code>>> Vm<'p, 'u, 'w, L> {
    /// An activation of loaded program `me`, its storage as `Machine::activation` leaves it: fresh
    /// on its first activation, after a CANCEL, and on every activation of an INITIAL program.
    fn activation(code: &'p Lowered, me: usize, unit: &'u mut RunUnit<'w, Rc<Code>, L>, main: bool) -> R<Self> {
        Self::activation_within(code, me, unit, main, Vec::new(), Spare::default())
    }

    /// An activation of a contained program, called with the programs containing it running, its
    /// tables taken from `spare`.
    fn activation_within(code: &'p Lowered, me: usize, unit: &'u mut RunUnit<'w, Rc<Code>, L>, main: bool, containers: Vec<scope::Container<'p>>, spare: Spare) -> R<Self> {
        check_storage(code)?;
        let (base, fresh) = unit.activate(me, code.program.initial);
        let mut vm = Self::over_reusing(code, me, base, unit, main, containers, spare);
        vm.start_storage(fresh)?;
        Ok(vm)
    }

    /// Binds the shared records and lays out storage as an activation starts: LOCAL-STORAGE each
    /// time, and on a `fresh` activation the program's initial image, with what it reported.
    fn start_storage(&mut self, fresh: bool) -> R<()> {
        let storage = &self.p.storage;
        self.bind_shared()?;
        if !storage.local_image.is_empty() {
            self.local_base = self.unit.push_temporary(&storage.local_image);
            self.unit.mark_input(self.local_base, storage.local_image.len(), false);
        }
        if fresh {
            let base = self.base;
            self.unit.mem[base..base + storage.image.len()].copy_from_slice(&storage.image);
            self.unit.mark_input(base, storage.image.len(), false);
            for &report in &storage.init_reports {
                let _ = writeln!(self.unit.err, "{}", self.sym(report));
            }
            if let Some(abend) = storage.init_abend {
                return Err(self.abend(abend, None).into());
            }
            self.unit.initialized(self.me);
        }
        Ok(())
    }

    /// Program `me` over its storage at `base`, with nothing bound or initialized.
    fn over(code: &'p Lowered, me: usize, base: usize, unit: &'u mut RunUnit<'w, Rc<Code>, L>, main: bool, containers: Vec<scope::Container<'p>>) -> Self {
        Self::over_reusing(code, me, base, unit, main, containers, Spare::default())
    }

    fn over_reusing(code: &'p Lowered, me: usize, base: usize, unit: &'u mut RunUnit<'w, Rc<Code>, L>, main: bool, containers: Vec<scope::Container<'p>>, spare: Spare) -> Self {
        let p = &code.program;
        let serial = unit.cics.as_mut().map_or(0, crate::cics::Task::next_activation);
        let first = unit.programs[me].compiled.is_none().then_some(code);
        let main_frame = Frame { id: 0, kind: FrameKind::Main, displaced: None, segment: 0, depth: unit.depth as u32, temps: Vec::new() };
        let Spare { mut linkage, mut armed, mut frames, .. } = spare;
        linkage.clear();
        linkage.resize(p.storage.linkage.len(), None);
        armed.clear();
        armed.resize(p.paragraphs.len(), None);
        frames.clear();
        frames.push(main_frame);
        Self {
            code,
            p,
            me,
            base,
            local_base: 0,
            linkage,
            main,
            returns: Returns { armed, saved: BTreeMap::new(), frames, next_frame: 1 },
            segment: 0,
            line: 0,
            arrival: Arrival::Start,
            debugging: false,
            locating: 0,
            pending: None,
            markup: markup::State::default(),
            io: files::State::default(),
            cics_handlers: Handlers::default(),
            serial,
            first,
            whenever: None,
            method: None,
            containers,
            memo: None,
            spare: Spare::default(),
            unit,
        }
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
        result.map_err(|stop| match stop.halt() {
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
            (_, Some(what)) => Err(Halt::Unimplemented(what).into()),
            (result, None) => result.map_err(Stop::from),
        }
    }
}

