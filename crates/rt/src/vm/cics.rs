//! EXEC CICS (lir.md §9.5): a command run by `rt::cics::run` over the activation as its
//! `CicsHost`, with the program level's handler table; a logical level whose HANDLE ABEND exit an
//! abend reaches; and LINK and XCTL as a new activation at a level of its own.

use super::{Code, Halt, R, Vm, not_yet};
use crate::abend::{Abend, Ending};
use crate::arith;
use crate::bms::Mapset;
use crate::callee;
use crate::cics::{self, Cics, CicsCommand, CicsHost, Datum, ExitTarget, Handlers};
use crate::lir::{Chars, CicsId, Operand, PlaceId, Step, SymId};
use crate::storage::{Loc, Val};
use crate::store::{self, ProgramFacts};
use crate::unit::{Loader, RunUnit};
use crate::vocab::Pos;
use std::rc::Rc;

/// Runs program `me` of the run unit, `code`, as the first program of the CICS task the run unit
/// holds: the EXEC interface block filled, and DFHEIBLK and the task's COMMAREA, `length` bytes at
/// `commarea`, its USING items.
pub fn run_task<L: Loader<Rc<Code>>>(code: &Code, me: usize, unit: &mut RunUnit<'_, Rc<Code>, L>, commarea: Option<usize>, length: usize) -> Result<Ending, Halt> {
    let lowered = code.lowered.as_ref().map_err(|why| not_yet(format!("a program that does not lower ({why})")))?;
    let mut vm = Vm::activation(lowered, me, unit, true)?;
    cics::begin_task(vm.unit, lowered.program.options.options.code_page(), length);
    let eib = vm.unit.eib;
    vm.bind_level(Some(eib), commarea);
    vm.run_level()
}

impl<'p, L: Loader<Rc<Code>>> Vm<'p, '_, '_, L> {
    pub(super) fn cics(&mut self, id: CicsId, pos: Pos) -> R<Step> {
        let p = self.p;
        let command = &p.services.cics[id as usize];
        cics::in_task(self.unit, self.sym(command.name), pos)?;
        if self.unit.observed() {
            self.cics_sinks(command, pos)?;
        }
        let flow = cics::run(self, command, pos);
        Ok(self.settle(flow)?.into())
    }

    /// `Machine::cics_sinks`: each operand an input could steer, told to the observer before the
    /// command runs, from the options the command keeps. A command kept as `Unsupported`, whose
    /// options lowering drops, stops the VM, and so does a WRITE with no FILE, whose FROM a
    /// JOURNALNAME the command does not keep may make a sink.
    fn cics_sinks(&mut self, command: &'p CicsCommand, pos: Pos) -> R<()> {
        let name = self.sym(command.name);
        let unkept = || not_yet(format!("the operands EXEC CICS {name} tells an observer, which its lowering does not keep"));
        let queue = matches!(name, "WRITEQ" | "READQ" | "DELETEQ") || ["WRITEQ ", "READQ ", "DELETEQ "].iter().any(|w| name.starts_with(w));
        let mut sinks: Vec<(Option<&Datum>, &'static str)> = Vec::new();
        match (name, &command.command) {
            (_, Cics::Unsupported) => return Err(unkept()),
            ("LINK" | "XCTL", Cics::Link(t) | Cics::Xctl(t)) => sinks.push((t.program.as_ref(), "cics-dynamic-transfer")),
            ("READ" | "STARTBR" | "RESETBR", Cics::File { options, .. }) => sinks.push((options.ridfld.as_ref(), "record-key")),
            ("DELETE", Cics::File { options, .. }) => sinks.push((options.ridfld.as_ref(), "record-update")),
            ("WRITEQ TD", Cics::WriteqTd { from, .. }) => sinks.push((from.as_ref(), "log")),
            ("WRITE", Cics::WriteOperator { text, .. }) => sinks.push((text.as_ref(), "log")),
            ("WRITE", Cics::File { file: None, options, .. }) if matches!(options.from, Some(Datum::Place(_))) => return Err(unkept()),
            ("SEND TEXT" | "SEND MAP" | "SEND", Cics::SendText { from, .. } | Cics::SendMap { from, .. }) => sinks.push((from.as_ref(), "screen")),
            _ => {}
        }
        if queue {
            let held = match &command.command {
                Cics::WriteqTs { queue, .. } | Cics::ReadqTs { queue, .. } | Cics::DeleteqTs { queue } => queue,
                Cics::WriteqTd { queue, .. } | Cics::ReadqTd { queue, .. } | Cics::DeleteqTd { queue } => queue,
                _ => return Err(unkept()),
            };
            sinks.push((held.as_ref(), "queue-name"));
        }
        if let Cics::Assign(assign) = &command.command {
            sinks.push((assign.sysid.as_ref(), "cics-sysid"));
        }
        for (datum, kind) in sinks {
            let Some(&Datum::Place(place)) = datum else { continue };
            match self.loc(place) {
                Ok(loc) => {
                    let text = self.facts().page().decode(store::bytes(&self.unit.mem, loc));
                    self.sink(kind, pos, &text);
                }
                Err(Halt::Abend(_)) => {}
                Err(stopped) => return Err(stopped),
            }
        }
        Ok(())
    }

    /// `Machine::run_level`: this activation as a logical level of the task. An abend that reaches
    /// it while its HANDLE ABEND exit is active goes to the exit (C142): a LABEL as a GO TO from
    /// the procedure's start, a PROGRAM in place of the rest of this level.
    pub(super) fn run_level(&mut self) -> R<Ending> {
        let mut start = None;
        loop {
            let abend = match self.run_from(start) {
                Err(Halt::Abend(abend)) => abend,
                done => return done,
            };
            let me = self.activation();
            match cics::abend_exit(self.unit, &mut self.cics_handlers, &abend, me, true)? {
                None => return Err(abend.into()),
                Some(ExitTarget::Label { paragraph: p, .. }) => {
                    self.unwind();
                    start = Some((p, self.p.paragraphs[p as usize].entry));
                }
                Some(ExitTarget::Program { name, commarea }) => {
                    let ending = cics::enter_exit_program(self, &name, commarea, abend.pos);
                    let ending = self.settle(ending)?;
                    return Ok(if ending == Ending::StopRun { ending } else { Ending::Goback });
                }
            }
        }
    }

    /// The frames an abend left, gone as the walker's Rust calls are: the points they armed stay
    /// armed, and the depth is the activation's.
    fn unwind(&mut self) {
        self.returns.frames.truncate(1);
        if let Some(main) = self.returns.frames.first_mut() {
            main.temps.clear();
            self.unit.depth = main.depth as usize;
        }
    }

    /// DFHEIBLK and DFHCOMMAREA, the USING items the translator gives a CICS program.
    fn bind_level(&mut self, eib: Option<usize>, commarea: Option<usize>) {
        let p = self.p;
        for (&record, address) in p.storage.using.iter().zip([eib, commarea]) {
            self.linkage[usize::from(record)] = address;
        }
    }

    /// A LINKed or XCTLed program, `code`, as program `index` at a logical level of its own.
    fn level(&mut self, code: &Code, index: usize, commarea: Option<usize>, xctl: bool) -> R<Ending> {
        let lowered = code.lowered.as_ref().map_err(|why| not_yet(format!("EXEC CICS LINK or XCTL of a program that does not lower ({why})")))?;
        let eib = self.unit.eib;
        let mut callee = Vm::activation(lowered, index, &mut *self.unit, self.main && xctl)?;
        callee.bind_level(Some(eib), commarea);
        callee.run_level()
    }

    /// `Machine::integer` of an operand that is not a data item: its value's whole part.
    fn integer_value(&mut self, o: Operand, pos: Pos) -> R<i64> {
        let val = self.value(o)?;
        if matches!(val, Val::Float(_)) {
            return Err(not_yet("a floating-point EXEC CICS option, whose scale the LIR does not keep"));
        }
        let v = arith::fixed_operand(val, 0, pos)?;
        Ok(super::place::whole(&v, pos)?)
    }
}

impl<'w, L: Loader<Rc<Code>>> CicsHost<'w, PlaceId, Operand, SymId> for Vm<'_, '_, 'w, L> {
    fn handlers(&mut self) -> &mut Handlers {
        &mut self.cics_handlers
    }

    fn content(&mut self, operand: &Operand, pos: Pos) -> Result<Vec<u8>, Abend> {
        callee::content(self, &Chars::Value(*operand), pos)
    }

    fn integer_of(&mut self, operand: &Operand, pos: Pos) -> Result<i64, Abend> {
        let n = self.integer_value(*operand, pos);
        self.lift(n, pos)
    }

    fn text(&self, text: &SymId) -> String {
        self.sym(*text).to_owned()
    }

    fn main(&self) -> bool {
        self.main
    }

    // Lowering refuses HANDLE ABEND, so no exit names a VM activation.
    fn activation(&self) -> u64 {
        0
    }

    fn program_id(&self) -> String {
        self.sym(self.p.id).to_owned()
    }

    fn commarea(&self) -> Option<usize> {
        let records = self.p.items.iter().filter(|i| i.parent.is_none());
        let record = records.filter_map(|i| Some((i.name?, i.linkage?))).find(|&(name, _)| self.sym(name) == "DFHCOMMAREA")?.1;
        self.linkage.get(usize::from(record)).copied().flatten()
    }

    fn mapset(&mut self, name: &str) -> Option<Result<Mapset, String>> {
        self.unit.library.mapset(name)
    }

    fn item_named(&mut self, _name: &str, pos: Pos) -> Result<Option<Loc>, Abend> {
        let found = Err(not_yet("SEND MAP with no FROM, whose symbolic map the LIR has no place for"));
        self.lift(found, pos)
    }

    fn locate_named(&mut self, _name: &str, pos: Pos) -> Result<Loc, Abend> {
        let found = Err(not_yet("RECEIVE MAP with no INTO or SET, whose symbolic map the LIR has no place for"));
        self.lift(found, pos)
    }

    fn run_program(&mut self, program: Rc<Code>, index: usize, commarea: Option<usize>, xctl: bool) -> Result<Ending, Abend> {
        let ending = self.level(&program, index, commarea, xctl);
        self.lift(ending, Pos::default())
    }
}
