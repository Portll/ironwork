//! CALL within the run unit (lir.md §9.3), as `Machine::call` and `call_nested` run it: the program
//! found through the run unit's loader, its arguments' addresses, a new activation run by Rust
//! recursion, RETURNING, and what an observer is told.

use super::{Code, Halt, Lowered, R, Vm, not_yet};
use crate::abend::{Abend, AbendCode, Ending};
use crate::lir::{Base, CallArg, CallPlan, CallTarget, Chars, Operand, Step};
use crate::le;
use crate::virtual_printer;
use crate::storage::{Kind, Loc, Val};
use crate::store::{self, ProgramFacts};
use crate::unit::{Event, LoadError, Loader, OS_COMMAND_ROUTINES};
use crate::vocab::Pos;
use std::rc::Rc;

impl<'p, L: Loader<Rc<Code>>> Vm<'p, '_, '_, L> {
    pub(super) fn call(&mut self, plan: &'p CallPlan, pos: Pos) -> R<Step> {
        let (name, variable) = match &plan.target {
            CallTarget::Pointer(_) => return Err(not_yet("CALL through a FUNCTION-POINTER or PROCEDURE-POINTER")),
            CallTarget::Named { name, .. } => (self.sym(*name).to_owned(), false),
            CallTarget::Dynamic(o) => (self.program_name(*o, pos)?, true),
        };
        if self.unit.observed() {
            if variable {
                self.sink("dynamic-program-load", pos, &name);
            }
            if OS_COMMAND_ROUTINES.contains(&name.as_str()) {
                match self.arguments_text(plan) {
                    Ok(text) => self.sink("os-command", pos, &text),
                    Err(Halt::Unimplemented(what)) => return Err(Halt::Unimplemented(what)),
                    Err(Halt::Abend(_)) => {}
                }
            }
        }
        let dynamic = self.p.options.options.dynam || variable;
        let (index, entry) = match self.unit.load_entry(&name, dynamic) {
            Ok(found) => found,
            Err(LoadError::NotFound) if le::provides(&name) => return Err(not_yet("Language Environment callable services")),
            Err(LoadError::NotFound) if virtual_printer::ROUTINES.contains(&name.as_str()) && self.unit.dds.get(virtual_printer::DD).is_some() => {
                return Err(not_yet("the virtual printer"));
            }
            Err(LoadError::NotFound) if plan.on_exception => return Ok(Step::Arm(1)),
            Err(LoadError::NotFound) => return Err(Abend { code: AbendCode::ModuleNotFound, message: le::missing(&name), pos, file: None }.into()),
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

    fn call_nested(&mut self, plan: &CallPlan, index: usize, entry: Option<usize>, callee: &Lowered, pos: Pos) -> R<Step> {
        let mark = self.unit.mem.len();
        let mut addresses = Vec::with_capacity(plan.args.len());
        for arg in &plan.args {
            let at = match arg {
                CallArg::Omitted => {
                    addresses.push(None);
                    continue;
                }
                CallArg::Reference(place) => self.loc(*place)?.offset,
                CallArg::Value(o) => {
                    let bytes = self.value_argument(*o, pos)?;
                    self.unit.push_temporary(&bytes)
                }
                CallArg::Content(chars) => {
                    let bytes = self.content(chars)?;
                    self.unit.push_temporary(&bytes)
                }
            };
            addresses.push(Some(at));
        }
        let program = &callee.program;
        let (ending, returned) = {
            let mut vm = Vm::activation(callee, index, &mut *self.unit, false)?;
            let entry = entry.and_then(|k| program.services.entries.get(k));
            let using = entry.map_or(&program.storage.using, |e| &e.using);
            for (&ordinal, address) in using.iter().zip(&addresses) {
                if let Some(slot) = vm.linkage.get_mut(usize::from(ordinal)) {
                    *slot = *address;
                }
            }
            if let Some(ordinal) = program.storage.returning {
                let size = program.storage.linkage[usize::from(ordinal)] as usize;
                vm.linkage[usize::from(ordinal)] = Some(vm.unit.push_temporary(&vec![0; size]));
            }
            let ending = match vm.run_from(entry.map(|e| (e.paragraph, e.block))) {
                Err(Halt::Unimplemented(what)) => return Err(Halt::Unimplemented(what)),
                Err(Halt::Abend(a)) => Err(a),
                Ok(e) => Ok(e),
            };
            let returned = match (program.storage.returning, &ending) {
                (Some(ordinal), Ok(_)) => Some(vm.returned(ordinal, pos)?),
                _ => None,
            };
            (ending, returned)
        };
        self.unit.programs[index].active = false;
        if program.initial {
            self.cancel_program(index, pos)?;
        }
        self.unit.release_temporaries(mark);
        let ending = ending.map_err(|a| self.in_loaded(index, callee, a))?;
        if ending == Ending::StopRun {
            return Ok(Step::End(Ending::StopRun));
        }
        if let (Some(target), Some(val)) = (plan.returning, returned) {
            let dest = self.loc(target)?;
            store::assign(&self.facts(), self.unit, dest, val, None, pos)?;
        }
        Ok(if plan.on_exception || plan.not_on_exception { Step::Arm(0) } else { Step::Next })
    }

    /// `Machine::content_argument`: a copy of a data item, a literal as its own item would hold it,
    /// or another operand's value as bytes.
    fn content(&mut self, chars: &Chars) -> R<Vec<u8>> {
        match chars {
            Chars::Literal(bytes) => Ok(bytes.clone()),
            Chars::Place(place) => {
                let loc = self.loc(*place)?;
                Ok(store::bytes(&self.unit.mem, loc).to_vec())
            }
            Chars::Value(o) => self.content_of(*o),
        }
    }

    /// `Machine::arguments_text`: each argument's bytes, as the code page reads them.
    fn arguments_text(&mut self, plan: &CallPlan) -> R<String> {
        let mut text = String::new();
        for arg in &plan.args {
            let bytes = match arg {
                CallArg::Omitted => continue,
                CallArg::Reference(place) | CallArg::Content(Chars::Place(place)) | CallArg::Value(Operand::Load(place)) => {
                    let loc = self.loc(*place)?;
                    store::bytes(&self.unit.mem, loc).to_vec()
                }
                CallArg::Content(chars) => self.content(chars)?,
                CallArg::Value(o) => self.content_of(*o)?,
            };
            text.push_str(&self.facts().page().decode(&bytes));
        }
        Ok(text)
    }

    /// The callee's RETURNING item, located by a place naming the whole record and read as its
    /// kind once the callee has returned.
    fn returned(&mut self, ordinal: u16, pos: Pos) -> R<Val> {
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
        self.read(loc, pos)
    }

    /// `Machine::in_loaded`: an abend in a program CALL loaded from a library names that program's
    /// files.
    fn in_loaded(&self, index: usize, callee: &Lowered, mut abend: Abend) -> Abend {
        if abend.file.is_none()
            && let Some(source) = &self.unit.programs[index].source
        {
            let program = &callee.program;
            abend.file = Some(match abend.pos.file {
                0 => source.display().to_string(),
                i => program.debug.sources.get(usize::from(i)).map(|&s| program.symbols[s as usize].clone()).unwrap_or_default(),
            });
        }
        abend
    }

    /// `Machine::cancel`: a program a dynamic CALL entered, or a contained one, is closed and starts
    /// afresh at its next CALL; one only ever called statically is left as it is.
    pub(super) fn cancel(&mut self, name: Operand, pos: Pos) -> R<()> {
        let name = self.program_name(name, pos)?;
        let Some(index) = self.unit.find(&name) else { return Ok(()) };
        let target = &self.unit.programs[index].name;
        let contained = self.unit.programs.iter().any(|p| p.compiled.as_ref().is_some_and(|c| c.nested.contains(target)));
        if !self.unit.programs[index].dynamic && !contained {
            return Ok(());
        }
        if self.unit.programs[index].active {
            return Err(Abend::ironwork(format!("CANCEL {name}: the program is active"), pos).into());
        }
        self.cancel_program(index, pos)
    }

    /// `Machine::cancel_program`: the files of program `index` and of the programs it contains
    /// closed, each to start in its initial state.
    fn cancel_program(&mut self, index: usize, pos: Pos) -> R<()> {
        let files: Vec<_> = self.unit.programs[index].files.iter_mut().filter_map(Option::take).collect();
        for f in files {
            f.close().map_err(|e| Abend::ironwork(format!("CANCEL {}: {e}", self.unit.programs[index].name), pos))?;
        }
        self.unit.programs[index].initialized = false;
        let nested = self.unit.programs[index].compiled.as_ref().map(|c| c.nested.clone()).unwrap_or_default();
        for name in nested {
            if let Some(contained) = self.unit.find(&name) {
                self.cancel_program(contained, pos)?;
            }
        }
        Ok(())
    }

    /// `Machine::sink`: tells the observer an operation an input could steer, and its operand.
    pub(super) fn sink(&mut self, kind: &'static str, pos: Pos, operand: &str) {
        let file = self.event_file(pos);
        self.unit.notify(Event::Sink { kind, file: &file, line: pos.line, operand });
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
