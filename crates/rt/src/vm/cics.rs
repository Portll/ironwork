//! EXEC CICS (lir.md §9.5): a command run by `rt::cics::run` over the activation as its
//! `CicsHost`, with the logical level's handler table; an activation whose HANDLE ABEND exit an
//! abend reaches; and LINK as a new activation at a level of its own, XCTL at this one's.

use super::{Code, Halt, R, Stop, Vm, not_yet};
use crate::abend::{Abend, Ending};
use crate::arith;
use crate::bms::Mapset;
use crate::callee;
use crate::cics::{self, CicsCommand, CicsHost, ExitTarget, Handlers};
use crate::fast::Stopped;
use crate::lir::{Base, BlockId, Chars, CicsId, Operand, ParaId, PlaceId, Step, SymId};
use crate::storage::Loc;
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
    Ok(vm.run_level()?)
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

    /// `Machine::cics_sinks`: each data item of the block's options an input could steer, told to
    /// the observer before the command runs; one that cannot be located is left to the command.
    fn cics_sinks(&mut self, command: &'p CicsCommand, pos: Pos) -> R<()> {
        for &(place, sink) in &command.sinks {
            match self.loc(place).map_err(Stop::halt) {
                Ok(loc) => {
                    let text = self.facts().page().decode(store::bytes(&self.unit.mem, loc));
                    self.sink(sink.kind(), pos, &text);
                }
                Err(Halt::Abend(_)) => {}
                Err(stopped) => return Err(stopped.into()),
            }
        }
        Ok(())
    }

    /// `Machine::run_level`: this activation as a logical level of the task. An abend that reaches
    /// it while its HANDLE ABEND exit is active goes to the exit (C142): a LABEL as a GO TO at the
    /// HANDLE ABEND command (C236), a PROGRAM in place of the rest of this level.
    pub(super) fn run_level(&mut self) -> R<Ending> {
        self.run_taking_exits(None, true)
    }

    /// `Machine::run_called`: a CALLed program, at its caller's logical level, takes the level's
    /// exit only when it is a LABEL (C238).
    pub(super) fn run_called(&mut self, at: Option<(ParaId, BlockId)>) -> R<Ending> {
        self.run_taking_exits(at, false)
    }

    /// `run_called` for an activation whose program's generated code ran from the program's start
    /// until it `stopped` where the VM takes it on.
    pub(super) fn run_called_after(&mut self, stopped: Stopped) -> R<Ending> {
        let ending = self.run_after(stopped);
        self.taking_exits(ending, false)
    }

    fn run_taking_exits(&mut self, at: Option<(ParaId, BlockId)>, runs_level: bool) -> R<Ending> {
        let ending = self.run_from(at);
        self.taking_exits(ending, runs_level)
    }

    fn taking_exits(&mut self, mut ending: R<Ending>, runs_level: bool) -> R<Ending> {
        loop {
            let abend = match ending.map_err(Stop::halt) {
                Err(Halt::Abend(abend)) => abend,
                done => return Ok(done?),
            };
            match cics::abend_exit(self.unit, &mut self.cics_handlers, &abend, self.serial, runs_level)? {
                None => return Err(abend.into()),
                Some(ExitTarget::Label { paragraph, at, .. }) => ending = self.go_to(paragraph, at),
                Some(ExitTarget::Program { name, commarea }) => {
                    let ending = cics::enter_exit_program(self, &name, commarea, abend.pos);
                    let ending = self.settle(ending)?;
                    return Ok(if ending == Ending::StopRun { ending } else { Ending::Goback });
                }
            }
        }
    }

    /// DFHEIBLK and DFHCOMMAREA, the USING items the translator gives a CICS program.
    fn bind_level(&mut self, eib: Option<usize>, commarea: Option<usize>) {
        let p = self.p;
        for (&record, address) in p.storage.using.iter().zip([eib, commarea]) {
            self.linkage[usize::from(record)] = address;
        }
    }

    /// A LINKed or XCTLed program, `code`, or the task's first program for None, as program
    /// `index` at a logical level of its own.
    fn level(&mut self, code: Option<&Code>, index: usize, commarea: Option<usize>, xctl: bool) -> R<Ending> {
        let first = self.first;
        let lowered = match code {
            Some(code) => code.lowered.as_ref().map_err(|why| not_yet(format!("EXEC CICS LINK or XCTL of a program that does not lower ({why})")))?,
            None => first.ok_or_else(|| Abend::ironwork("the CICS task's first program cannot be LINKed or XCTLed to from a function or a method", Pos::default()))?,
        };
        let (eib, handlers) = (self.unit.eib, if xctl { self.cics_handlers.xctl() } else { Handlers::default() });
        let mut callee = Vm::activation(lowered, index, &mut *self.unit, self.main && xctl)?;
        callee.bind_level(Some(eib), commarea);
        (callee.cics_handlers, callee.first) = (handlers, first);
        callee.run_level()
    }

    /// `Machine::integer` of an operand that is not a data item: its value's whole part. A value
    /// that is floating point is a function's, whose dmax there is 0.
    fn integer_value(&mut self, o: Operand, pos: Pos) -> R<i64> {
        let val = self.value(o)?;
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

    fn activation(&self) -> u64 {
        self.serial
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
        let found = Err(not_yet("SEND MAP with no FROM, whose MAP is no literal that names the symbolic map of a data item"));
        self.lift(found, pos)
    }

    fn locate_named(&mut self, _name: &str, pos: Pos) -> Result<Loc, Abend> {
        let found = Err(not_yet("RECEIVE MAP with no INTO or SET, whose MAP is no literal that names the symbolic map of a data item"));
        self.lift(found, pos)
    }

    fn unaddressed(&mut self, place: PlaceId) -> Option<usize> {
        let place = &self.p.places[place as usize];
        matches!(place.base, Base::Linkage(record) if self.linkage[usize::from(record)].is_none()).then_some(place.len as usize)
    }

    fn run_program(&mut self, program: Option<Rc<Code>>, index: usize, commarea: Option<usize>, xctl: bool) -> Result<Ending, Abend> {
        let ending = self.level(program.as_deref(), index, commarea, xctl);
        self.lift(ending, Pos::default())
    }
}
