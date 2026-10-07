//! CALL within the run unit (lir.md §9.3), as `Machine::call` and `call_nested` run it: the program
//! found through the run unit's loader, then `rt::callee`'s arguments and run around a new
//! activation run by Rust recursion, RETURNING, and what an observer is told. A name no program has
//! may be an LE callable service or a job for the virtual printer.

use super::{Code, Halt, Lowered, R, Spare, Stop, Vm, check_storage, not_yet};
use crate::abend::{Abend, Ending};
use crate::callee::{self, Arguments, Bindings, By, Callee};
use crate::cics;
use crate::le::{self, LeHost};
use crate::lir::{Base, CallArg, CallPlan, CallTarget, Chars, LeService, Operand, PlaceId, SenderCheck, Step};
use crate::loc;
use crate::parmcheck;
use crate::set;
use crate::storage::{Kind, Loc, Val};
use crate::store;
use crate::unit::{Event, LoadError, Loader, OS_COMMAND_ROUTINES, RETURN_CODE, RunUnit, UnitHost};
use crate::virtual_printer::{self, Job};
use crate::vocab::Pos;
use numeric::{LeServices, ProgramScope, Switched};
use numeric::precision::{Fixed, Places};
use std::borrow::Cow;
use std::rc::Rc;
use zarch::ebcdic::CodePage;

impl<'p, L: Loader<Rc<Code>>> Vm<'p, '_, '_, L> {
    pub(super) fn call(&mut self, plan: &'p CallPlan, pos: Pos) -> R<Step> {
        let dynam = self.p.options.options.dynam;
        let (name, variable, dynamic) = match &plan.target {
            CallTarget::Pointer(pointer) => {
                if self.entry_in(*pointer)?.is_some() {
                    return Err(not_yet("a CALL through a JNI function-pointer SET TO ENTRY, whose arguments the LIR keeps by value"));
                }
                return self.call_through_pointer(plan, *pointer, pos);
            }
            CallTarget::Entry(pointer) => match self.entry_in(*pointer)? {
                Some(entry) => (Cow::Owned(entry.name), false, entry.dynamic),
                None if plan.args.iter().all(|a| matches!(a, CallArg::Value(_) | CallArg::Omitted)) => return self.call_through_pointer(plan, *pointer, pos),
                None => return Err(not_yet("a CALL BY REFERENCE or BY CONTENT through a pointer that holds no entry, which the walker calls as a JNI service")),
            },
            CallTarget::Named { name, .. } => (Cow::Borrowed(self.sym(*name)), false, dynam),
            CallTarget::Dynamic(o) => (Cow::Owned(self.program_name(*o, pos)?), true, true),
        };
        if self.unit.observed() {
            if variable {
                self.sink("dynamic-program-load", pos, &name);
            }
            if OS_COMMAND_ROUTINES.contains(&&*name) {
                let text = callee::arguments_text(self, &plan.args, pos);
                match self.settle(text).map_err(Stop::halt) {
                    Ok(text) => self.sink("os-command", pos, &text),
                    Err(Halt::Unimplemented(what)) => return Err(Halt::Unimplemented(what).into()),
                    Err(Halt::Abend(_)) => {}
                }
            }
        }
        let options = self.p.options.options;
        if options.le_services == LeServices::Bind
            && let Some(service) = le::service(&name)
        {
            return self.le_call(plan, service, &name, pos);
        }
        let strict = options.program_scope == ProgramScope::Strict;
        let scope = &self.p.services.scope;
        let (hidden, callable) = (callee::names(scope.hidden.iter().map(|&s| self.sym(s)), &name), callee::names(scope.callable.iter().map(|&s| self.sym(s)), &name));
        let found = if strict && hidden { Err(LoadError::NotFound) } else { self.unit.load_entry(&name, callee::entry_copy(dynamic, options.dialect_of(Switched::EntryCalls))) };
        let found = found.and_then(|(index, entry)| {
            let contained = self.unit.programs[index].compiled.as_deref().and_then(|c| c.lowered.as_ref().ok()).is_some_and(|l| !l.program.services.scope.containers.is_empty());
            if strict && contained && !callable { Err(LoadError::NotFound) } else { Ok((index, entry)) }
        });
        let (index, entry) = match found {
            Ok(found) => found,
            Err(LoadError::NotFound) => {
                if let Some(service) = le::service(&name) {
                    return self.le_call(plan, service, &name, pos);
                }
                if let Some(step) = self.virtual_print(plan, &name, pos)? {
                    return Ok(step);
                }
                if plan.on_exception {
                    return Ok(Step::Arm(1));
                }
                return Err(le::not_found(&format!("CALL {name}"), &name, dynamic, pos).into());
            }
            Err(LoadError::Compile(message)) => return Err(Abend::ironwork(format!("CALL {name}: {message}"), pos).into()),
        };
        let held = self.unit.programs[index].compiled.clone();
        let lowered = match held.as_deref() {
            Some(code) => code.lowered.as_ref().map_err(|why| not_yet(format!("CALL of a program that does not lower ({why})")))?,
            None => self.first.ok_or_else(|| Abend::ironwork(format!("CALL {name}: the run unit's first program cannot be CALLed from a function or a method"), pos))?,
        };
        self.unit.programs[index].dynamic |= dynamic;
        let p = &lowered.program;
        if self.unit.programs[index].active && !p.recursive {
            let unit = p.services.scope.containers.last().unwrap_or(&p.id);
            return Err(callee::recursive_call(&p.symbols[p.id as usize], &p.symbols[*unit as usize], pos).into());
        }
        self.unit.enter(pos)?;
        // A dynamic CALL suspends the caller's handlers, as CBLPSHPOP(ON) does (C234).
        let suspends = dynamic && lowered.program.services.scope.containers.is_empty();
        let result = self.call_nested(plan, index, entry, lowered, suspends, pos);
        self.unit.depth = self.unit.depth.saturating_sub(1);
        result
    }

    fn call_nested(&mut self, plan: &CallPlan, index: usize, entry: Option<usize>, lowered: &Lowered, suspends: bool, pos: Pos) -> R<Step> {
        let mark = self.unit.mem.len();
        let mut addresses = std::mem::take(&mut self.spare.addresses);
        addresses.clear();
        let mut lengths = Vec::with_capacity(plan.args.len());
        let filled = callee::addresses_into(self, &plan.args, pos, &mut addresses, &mut lengths);
        self.settle(filled)?;
        self.parmcheck_set();
        let program = &lowered.program;
        let containers = self.containers_of(program);
        let by = By::Call { initial: program.initial };
        let (ending, returned) = callee::run(self, &Callee { index, by, mark: Some(mark), pos, lengths: &lengths }, |caller| {
            let spare = &mut caller.spare;
            let tables = Spare { linkage: std::mem::take(&mut spare.linkage), armed: std::mem::take(&mut spare.armed), frames: std::mem::take(&mut spare.frames), ..Spare::default() };
            // Built where it runs and initialized there: returning the activation would copy it.
            check_storage(lowered)?;
            let (base, fresh) = caller.unit.activate(index, program.initial);
            let mut vm = Vm::over_reusing(lowered, index, base, &mut *caller.unit, false, containers, tables);
            vm.start_storage(fresh)?;
            let entry = entry.and_then(|k| program.services.entries.get(k));
            let mut using = std::mem::take(&mut caller.spare.using);
            using.clear();
            using.extend(entry.map_or(&program.storage.using, |e| &e.using).iter().map(|&o| Some(usize::from(o))));
            let returning = program.storage.returning.map(|o| (usize::from(o), program.storage.linkage[usize::from(o)] as usize));
            let bindings = Bindings { records: &[], using, addresses: &addresses, returning };
            bindings.bind(vm.unit, &mut vm.linkage);
            caller.spare.using = bindings.using;
            (vm.cics_handlers, vm.first) = (caller.cics_handlers.lend(suspends), caller.first);
            vm.spare = std::mem::take(&mut caller.spare);
            let ran = vm.run_called(entry.map(|e| (e.paragraph, e.block)));
            caller.spare = std::mem::take(&mut vm.spare);
            let ending = match ran.map_err(Stop::halt) {
                Err(Halt::Unimplemented(what)) => return Err(Stop::from(Halt::Unimplemented(what))),
                Err(Halt::Abend(a)) => Err(a),
                Ok(e) => Ok(e),
            };
            caller.cics_handlers.take_back(&mut vm.cics_handlers, suspends && ending.is_ok());
            let returned = match (program.storage.returning, &ending) {
                (Some(ordinal), Ok(_)) => Some(vm.returned(ordinal, pos)?),
                _ => None,
            };
            (caller.spare.linkage, caller.spare.armed, caller.spare.frames) = (std::mem::take(&mut vm.linkage), std::mem::take(&mut vm.returns.armed), std::mem::take(&mut vm.returns.frames));
            Ok((ending, returned))
        })?;
        if ending? == Ending::StopRun {
            return Ok(Step::End(Ending::StopRun));
        }
        if cics::level_ended(self.unit) {
            return Ok(Step::End(Ending::Goback));
        }
        self.parmcheck_test(plan, &addresses, |unit| unit.programs[index].name.clone(), pos)?;
        self.spare.addresses = addresses;
        if let (Some(target), Some(val)) = (plan.returning, returned) {
            let dest = self.loc_written(target)?;
            store::assign(&self.facts(), self.unit, dest, val, None, pos)?;
        }
        Ok(if plan.on_exception || plan.not_on_exception { Step::Arm(0) } else { Step::Next })
    }

    /// `Machine::le_call`: a callable service run with its arguments' addresses, which are released
    /// however it ends, PARMCHECK's buffer set around it. No depth is counted and ON EXCEPTION
    /// never runs.
    fn le_call(&mut self, plan: &CallPlan, service: LeService, name: &str, pos: Pos) -> R<Step> {
        let mark = self.unit.mem.len();
        let addresses = callee::addresses(self, &plan.args, pos);
        let ran = match self.settle(addresses) {
            Ok(args) => {
                self.parmcheck_set();
                le::call(self, service, &args, pos).map(|()| args).map_err(Stop::from)
            }
            Err(halt) => Err(halt),
        };
        self.unit.release_temporaries(mark);
        self.parmcheck_test(plan, &ran?, |_| name.to_owned(), pos)?;
        Ok(if plan.on_exception || plan.not_on_exception { Step::Arm(0) } else { Step::Next })
    }

    /// `Machine::parmcheck_set`: PARMCHECK's buffer set to X'AA' before a CALL.
    pub(super) fn parmcheck_set(&mut self) {
        parmcheck::set(self.unit, self.base, self.p.storage.parmcheck);
    }

    /// `Machine::parmcheck_test`: after a CALL that returned, its data item arguments at
    /// `addresses`, the called program's name worked out by `called` only when the buffer changed.
    pub(super) fn parmcheck_test(&mut self, plan: &CallPlan, addresses: &[Option<usize>], called: impl FnOnce(&RunUnit<'_, Rc<Code>, L>) -> String, pos: Pos) -> R<()> {
        let p = self.p;
        let arguments = plan.args.iter().zip(addresses).filter_map(|(arg, &address)| match (arg, address) {
            (CallArg::Reference(q) | CallArg::Content(Chars::Place(q)) | CallArg::Value(Operand::Load(q)), Some(a)) => Some((a, p.symbols[p.places[*q as usize].name as usize].as_str())),
            _ => None,
        });
        let abd = p.options.options.parmcheck.is_some_and(|c| c.abd);
        Ok(parmcheck::test(self.unit, self.base, p.storage.parmcheck, arguments, called, &p.symbols[p.id as usize], abd, pos)?)
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
        let job = match self.settle(text).map_err(Stop::halt) {
            Ok(text) => Job::parse(&text),
            Err(Halt::Abend(_)) => None,
            Err(halt) => return Err(halt.into()),
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

    /// Where RETURNING record `ordinal` is once the callee has returned; with no address, the
    /// abend the walker's locate of it gives at `pos`, the statement that ran the callee.
    pub(super) fn returning_address(&self, ordinal: u16, pos: Pos) -> Result<usize, Abend> {
        let record = self.p.items.iter().find(|i| i.linkage == Some(ordinal) && i.parent.is_none());
        let name = record.and_then(|i| i.name).map_or("", |n| self.sym(n));
        loc::linkage_base(self.linkage[usize::from(ordinal)], name, pos)
    }

    /// The callee's RETURNING item, located by a place naming the whole record and read as its
    /// kind once the callee has returned.
    pub(super) fn returned(&mut self, ordinal: u16, pos: Pos) -> R<Val> {
        let p = self.p;
        let offset = self.returning_address(ordinal, pos)?;
        let len = p.storage.linkage[usize::from(ordinal)];
        let odo = p.items.iter().any(|i| i.linkage == Some(ordinal) && i.depending_on.is_some());
        let whole = |q: &crate::lir::Place| {
            q.base == Base::Linkage(ordinal) && q.offset == 0 && q.len == len && q.subscripts.is_empty() && q.refmod.is_none() && (!q.odo.is_empty() || !odo)
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

    /// `Machine::entry_pointer`: the entry SET TO ENTRY gave the pointer, once it is located.
    fn entry_in(&mut self, pointer: PlaceId) -> R<Option<set::Entry>> {
        let loc = self.loc(pointer)?;
        let Ok(value) = <[u8; 4]>::try_from(store::bytes(&self.unit.mem, loc)).map(u32::from_be_bytes) else { return Ok(None) };
        Ok(set::entry_of(&self.unit.entries, value).cloned())
    }

    /// SET TO ENTRY (`Machine::entry_named`): the entry's program loaded when the SET runs (C140),
    /// a name no program and no LE service has ending as `le::not_found` says, then each receiver given the value
    /// naming the entry.
    pub(super) fn set_entry(&mut self, entry: Operand, targets: &[PlaceId], pos: Pos) -> R<()> {
        let name = self.program_name(entry, pos)?;
        let variable = !matches!(entry, Operand::Const(_));
        if variable && self.unit.observed() {
            self.sink("dynamic-program-load", pos, &name);
        }
        let dynamic = self.p.options.options.dynam || variable;
        match self.unit.load_entry(&name, callee::entry_copy(dynamic, self.p.options.options.dialect_of(Switched::EntryCalls))) {
            Ok(_) => {}
            Err(LoadError::NotFound) if le::provides(&name) => {}
            Err(LoadError::NotFound) => return Err(le::not_found(&format!("SET TO ENTRY {name}"), &name, dynamic, pos).into()),
            Err(LoadError::Compile(message)) => return Err(Abend::ironwork(format!("SET TO ENTRY {name}: {message}"), pos).into()),
        }
        let value = set::entry(&mut self.unit.entries, &name, dynamic, pos)?;
        for &target in targets {
            let dest = self.loc_written(target)?;
            store::assign(&self.facts(), self.unit, dest, Val::Address(value), None, pos)?;
        }
        Ok(())
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

    fn content_item(&mut self, place: PlaceId) -> Result<Loc, Abend> {
        let pos = self.pos(self.p.places[place as usize].at);
        let tested = self.loc(place).and_then(|loc| self.numcheck(loc, SenderCheck::Item, pos).map(|()| loc));
        self.lift(tested, pos)
    }
}
