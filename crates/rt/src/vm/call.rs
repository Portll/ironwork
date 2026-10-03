//! CALL within the run unit (lir.md §9.3), as `Machine::call` and `call_nested` run it: the program
//! found through the run unit's loader, then `rt::callee`'s arguments and run around a new
//! activation run by Rust recursion, RETURNING, and what an observer is told. A name no program has
//! may be an LE callable service or a job for the virtual printer.

use super::{Code, Halt, Lowered, R, Vm, not_yet};
use crate::abend::{Abend, AbendCode, Ending};
use crate::callee::{self, Arguments, Bindings, By, Callee};
use crate::host::Host;
use crate::le::{self, LeHost};
use crate::lir::{Base, CallPlan, CallTarget, LeService, Operand, PlaceId, Step};
use crate::storage::{Kind, Loc, Val};
use crate::store;
use crate::unit::{Event, LoadError, Loader, OS_COMMAND_ROUTINES, RETURN_CODE, RunUnit, UnitHost};
use crate::virtual_printer::{self, Job};
use crate::vocab::Pos;
use numeric::precision::{Fixed, Places};
use std::rc::Rc;
use zarch::ebcdic::CodePage;

impl<'p, L: Loader<Rc<Code>>> Vm<'p, '_, '_, L> {
    pub(super) fn call(&mut self, plan: &'p CallPlan, pos: Pos) -> R<Step> {
        let (name, variable) = match &plan.target {
            CallTarget::Pointer(pointer) => return self.call_through_pointer(plan, *pointer, pos),
            CallTarget::Named { name, .. } => (self.sym(*name).to_owned(), false),
            CallTarget::Dynamic(o) => (self.program_name(*o, pos)?, true),
        };
        if self.unit.observed() {
            if variable {
                self.sink("dynamic-program-load", pos, &name);
            }
            if OS_COMMAND_ROUTINES.contains(&name.as_str()) {
                let text = callee::arguments_text(self, &plan.args, pos);
                match self.settle(text) {
                    Ok(text) => self.sink("os-command", pos, &text),
                    Err(Halt::Unimplemented(what)) => return Err(Halt::Unimplemented(what)),
                    Err(Halt::Abend(_)) => {}
                }
            }
        }
        let dynamic = self.p.options.options.dynam || variable;
        let (index, entry) = match self.unit.load_entry(&name, dynamic) {
            Ok(found) => found,
            Err(LoadError::NotFound) => {
                if let Some(service) = le::service(&name) {
                    return self.le_call(plan, service, pos);
                }
                if let Some(step) = self.virtual_print(plan, &name, pos)? {
                    return Ok(step);
                }
                if plan.on_exception {
                    return Ok(Step::Arm(1));
                }
                return Err(Abend { code: AbendCode::ModuleNotFound, message: le::missing(&name), pos, file: None }.into());
            }
            Err(LoadError::Compile(message)) => return Err(Abend::ironwork(format!("CALL {name}: {message}"), pos).into()),
        };
        let Some(code) = self.unit.programs[index].compiled.clone() else {
            return Err(Abend::ironwork(format!("CALL {name}: the first program of the run unit is already active"), pos).into());
        };
        self.unit.programs[index].dynamic |= dynamic;
        let lowered = code.lowered.as_ref().map_err(|why| not_yet(format!("CALL of a program that does not lower ({why})")))?;
        if self.unit.programs[index].active && !lowered.program.recursive {
            return Err(Abend::ironwork(format!("CALL {name}: the program is already active and is not RECURSIVE"), pos).into());
        }
        self.unit.enter(pos)?;
        let result = self.call_nested(plan, index, entry, lowered, pos);
        self.unit.depth = self.unit.depth.saturating_sub(1);
        result
    }

    fn call_nested(&mut self, plan: &CallPlan, index: usize, entry: Option<usize>, lowered: &Lowered, pos: Pos) -> R<Step> {
        let mark = self.unit.mem.len();
        let addresses = callee::addresses(self, &plan.args, pos);
        let addresses = self.settle(addresses)?;
        let program = &lowered.program;
        let by = By::Call { initial: program.initial };
        let (ending, returned) = callee::run(self, &Callee { index, by, mark: Some(mark), pos }, |caller| {
            let mut vm = Vm::activation(lowered, index, &mut *caller.unit, false)?;
            let entry = entry.and_then(|k| program.services.entries.get(k));
            let using = entry.map_or(&program.storage.using, |e| &e.using).iter().map(|&o| Some(usize::from(o))).collect();
            let returning = program.storage.returning.map(|o| (usize::from(o), program.storage.linkage[usize::from(o)] as usize));
            Bindings { records: &[], using, addresses: &addresses, returning }.bind(vm.unit, &mut vm.linkage);
            let ending = match vm.run_from(entry.map(|e| (e.paragraph, e.block))) {
                Err(Halt::Unimplemented(what)) => return Err(Halt::Unimplemented(what)),
                Err(Halt::Abend(a)) => Err(a),
                Ok(e) => Ok(e),
            };
            let returned = match (program.storage.returning, &ending) {
                (Some(ordinal), Ok(_)) => Some(vm.returned(ordinal, pos)?),
                _ => None,
            };
            Ok((ending, returned))
        })?;
        if ending? == Ending::StopRun {
            return Ok(Step::End(Ending::StopRun));
        }
        if let (Some(target), Some(val)) = (plan.returning, returned) {
            let dest = self.loc_written(target)?;
            store::assign(&self.facts(), self.unit, dest, val, None, pos)?;
        }
        Ok(if plan.on_exception || plan.not_on_exception { Step::Arm(0) } else { Step::Next })
    }

    /// `Machine::le_call`: a callable service run with its arguments' addresses, which are released
    /// however it ends. No depth is counted and ON EXCEPTION never runs.
    fn le_call(&mut self, plan: &CallPlan, service: LeService, pos: Pos) -> R<Step> {
        let mark = self.unit.mem.len();
        let addresses = callee::addresses(self, &plan.args, pos);
        let ran = match self.settle(addresses) {
            Ok(args) => le::call(self, service, &args, pos).map_err(Halt::Abend),
            Err(halt) => Err(halt),
        };
        self.unit.release_temporaries(mark);
        ran?;
        Ok(if plan.on_exception || plan.not_on_exception { Step::Arm(0) } else { Step::Next })
    }

    /// `Machine::virtual_print`: SYSTEM or C$SYSTEM with an lp or lpr command, in a run given DD
    /// PRINTER, prints the job and returns lp's status, 0 printed or 1 not, through RETURNING or
    /// else RETURN-CODE; None for a CALL the virtual printer does not serve.
    fn virtual_print(&mut self, plan: &CallPlan, name: &str, pos: Pos) -> R<Option<Step>> {
        if !virtual_printer::ROUTINES.contains(&name) {
            return Ok(None);
        }
        let Some(printer) = self.unit.dds.get(virtual_printer::DD) else { return Ok(None) };
        let text = callee::arguments_text(self, &plan.args, pos);
        let job = match self.settle(text) {
            Ok(text) => Job::parse(&text),
            Err(Halt::Abend(_)) => None,
            Err(halt) => return Err(halt),
        };
        let Some(job) = job else { return Ok(None) };
        let dds = self.unit.dds.clone();
        let status: i16 = match virtual_printer::print(&dds, &printer, &job, &mut |event| self.unit.notify(event)) {
            Ok(()) => 0,
            Err(why) => {
                let _ = writeln!(self.unit.err, "ironwork: {pos}: CALL {name}: the virtual printer printed nothing: {why}");
                1
            }
        };
        match plan.returning {
            Some(target) => {
                let dest = self.loc(target)?;
                store::assign(&self.facts(), self.unit, dest, Val::Num(Fixed::new(i128::from(status), Places::new(9, 0))), None, pos)?;
            }
            None => self.unit.write(RETURN_CODE, &status.to_be_bytes()),
        }
        Ok(Some(if plan.on_exception || plan.not_on_exception { Step::Arm(0) } else { Step::Next }))
    }

    /// The callee's RETURNING item, located by a place naming the whole record and read as its
    /// kind once the callee has returned.
    pub(super) fn returned(&mut self, ordinal: u16, pos: Pos) -> R<Val> {
        let p = self.p;
        let Some(offset) = self.linkage[usize::from(ordinal)] else { return Err(not_yet("a RETURNING item with no storage")) };
        let len = p.storage.linkage[usize::from(ordinal)];
        let odo = p.items.iter().any(|i| i.linkage == Some(ordinal) && i.depending_on.is_some());
        let whole = |q: &crate::lir::Place| {
            q.base == Base::Linkage(ordinal) && q.offset == 0 && q.len == len && q.subscripts.is_empty() && q.refmod.is_none() && (q.odo.is_some() || !odo)
        };
        let loc = match p.places.iter().position(whole) {
            Some(q) => self.loc(q as u32)?,
            None if odo => return Err(not_yet("a RETURNING record holding an OCCURS DEPENDING ON table it never names as a sender")),
            None => {
                let kind = p.items.iter().find(|i| i.linkage == Some(ordinal) && i.parent.is_none()).map_or(Kind::Group, |i| i.kind);
                Loc { offset, len: len as usize, kind, item: usize::MAX }
            }
        };
        self.unit.taint_read(loc);
        self.read(loc, pos)
    }

    pub(super) fn cancel(&mut self, name: Operand, pos: Pos) -> R<()> {
        let name = self.program_name(name, pos)?;
        Ok(callee::cancel(self.unit, &name, pos)?)
    }

    /// `Machine::sink`: tells the observer an operation an input could steer, and its operand.
    pub(super) fn sink(&mut self, kind: &'static str, pos: Pos, operand: &str) {
        let file = self.event_file(pos);
        let input = self.unit.input_at_sink();
        self.unit.notify(Event::Sink { kind, file: &file, line: pos.line, operand, input });
    }

    /// `Machine::event_file`: a library program's own source by its path, a COPY member by the
    /// program's file table, and the first program's own source as empty.
    pub(super) fn event_file(&self, pos: Pos) -> String {
        match (pos.file, &self.unit.programs[self.me].source) {
            (0, Some(path)) => path.to_str().unwrap_or_default().to_owned(),
            (i, _) => self.p.debug.sources.get(usize::from(i)).map_or_else(String::new, |&s| self.sym(s).to_owned()),
        }
    }
}

impl<'w, L: Loader<Rc<Code>>> UnitHost<'w> for Vm<'_, '_, 'w, L> {
    type Program = Rc<Code>;
    type Loader = L;

    fn unit(&mut self) -> &mut RunUnit<'w, Rc<Code>, L> {
        self.unit
    }
}

impl<'w, L: Loader<Rc<Code>>> LeHost<'w> for Vm<'_, '_, 'w, L> {
    fn page(&self) -> &'static CodePage {
        self.p.options.options.code_page()
    }

    fn method_name(program: &Rc<Code>) -> Option<String> {
        program.method.clone()
    }
}

impl<'w, L: Loader<Rc<Code>>> Arguments<'w, PlaceId, Operand> for Vm<'_, '_, 'w, L> {
    fn item(&self, operand: &Operand) -> Option<PlaceId> {
        match *operand {
            Operand::Load(place) => Some(place),
            _ => None,
        }
    }

    fn length_of(&self, operand: &Operand) -> bool {
        matches!(operand, Operand::LengthOf(_))
    }

    /// `Vm::activation` refuses a program compiled with NUMCHECK, so no test is made.
    fn content_item(&mut self, place: PlaceId) -> Result<Loc, Abend> {
        Host::locate(self, place, false)
    }
}
