//! A user-defined function's invocation (lir.md §9.15), as `Machine::invoke_function` runs it: the
//! definition found through the run unit's loader, the arguments evaluated, then `rt::callee`'s run
//! around a new activation that binds the formal parameters, runs the procedure and reads the
//! RETURNING item.

use super::{Code, Halt, Lowered, R, Stop, Vm, not_yet};
use crate::abend::{Abend, AbendCode, Ending, Signal};
use crate::callee::{self, Bindings, Bound, By, Callee};
use crate::lir::{FunctionDefinition, UserArgument, UserFunctionId};
use crate::storage::Val;
use crate::store;
use crate::unit::{LoadError, Loader};
use crate::vocab::Pos;
use std::rc::Rc;

impl<'p, L: Loader<Rc<Code>>> Vm<'p, '_, '_, L> {
    /// The function's value, the function run when its operand is evaluated; STOP RUN in it ends
    /// the run from the statement that invoked it (assumption C274).
    pub(super) fn user_function(&mut self, id: UserFunctionId) -> R<Val> {
        let plan = &self.p.services.user_functions[id as usize];
        let pos = self.pos(plan.at);
        let (name, external) = (self.sym(plan.name), self.sym(plan.external));
        let index = match self.unit.load_entry(external, false) {
            Ok((index, _)) => index,
            Err(LoadError::NotFound) => {
                let message = format!("FUNCTION {name}: its definition, {external}, is in neither the source nor the program libraries");
                return Err(Abend { code: AbendCode::ModuleNotFound, message, pos, file: None }.into());
            }
            Err(LoadError::Compile(message)) => return Err(Abend::ironwork(format!("FUNCTION {name}: {message}"), pos).into()),
        };
        let not_a_function = || Stop::from(Abend::ironwork(format!("FUNCTION {name}: {external} is a program, not a user-defined function"), pos));
        let code = self.unit.programs[index].compiled.clone().ok_or_else(not_a_function)?;
        let lowered = code.lowered.as_ref().map_err(|why| not_yet(format!("a user-defined function that does not lower ({why})")))?;
        let definition = lowered.program.services.function.as_ref().ok_or_else(not_a_function)?;
        let mut bound = Vec::with_capacity(plan.args.len());
        for arg in &plan.args {
            bound.push(match arg {
                UserArgument::Reference(q) => Bound::At(self.loc(*q)?.offset),
                UserArgument::Value(c) => Bound::Value(self.comparand(c, pos)?),
            });
        }
        self.unit.enter(pos)?;
        let mark = self.unit.mem.len();
        let read_before = self.unit.pending();
        let ran = callee::run(self, &Callee { index, by: By::Function, mark: Some(mark), pos }, |caller| {
            let outcome = caller.function_activation(lowered, definition, index, &bound, pos);
            caller.unit.resume_statement(read_before);
            match outcome.map_err(Stop::halt) {
                Err(Halt::Unimplemented(what)) => Err(Stop::from(Halt::Unimplemented(what))),
                Err(Halt::Abend(a)) => Ok((Err(a), ())),
                Ok(returned) => Ok((Ok(returned), ())),
            }
        });
        self.unit.depth = self.unit.depth.saturating_sub(1);
        let (outcome, ()) = ran?;
        match outcome? {
            (Ending::StopRun, _) => Err(Abend { code: AbendCode::Signal(Signal::StopRun), message: String::new(), pos, file: None }.into()),
            (_, value) => self.refmodded(value, plan.refmod.as_ref(), pos),
        }
    }

    /// One activation of the function: each formal parameter given its argument's address or a
    /// temporary its value is moved into, the RETURNING record given storage, the procedure run,
    /// and the RETURNING item read.
    fn function_activation(&mut self, lowered: &Lowered, definition: &FunctionDefinition, index: usize, bound: &[Bound], pos: Pos) -> R<(Ending, Val)> {
        let storage = &lowered.program.storage;
        let record = |ordinal: u16| (usize::from(ordinal), storage.linkage[usize::from(ordinal)] as usize);
        let mut vm = Vm::activation(lowered, index, &mut *self.unit, false)?;
        let addresses = callee::bound_addresses(vm.unit, bound, storage.using.iter().map(|&o| record(o).1));
        let using = storage.using.iter().map(|&o| Some(record(o).0)).collect();
        Bindings { records: &[], using, addresses: &addresses, returning: None }.bind(vm.unit, &mut vm.linkage);
        for (&q, b) in definition.params.iter().zip(bound) {
            if let Bound::Value(value) = b {
                let dest = vm.loc(q)?;
                store::assign(&vm.facts(), vm.unit, dest, value.clone(), None, pos)?;
            }
        }
        Bindings { records: &[], using: Vec::new(), addresses: &[], returning: storage.returning.map(record) }.bind(vm.unit, &mut vm.linkage);
        let ending = vm.run_from(None)?;
        if let Some(ordinal) = storage.returning {
            vm.returning_address(ordinal, pos)?;
        }
        let loc = vm.loc(definition.returning)?;
        Ok((ending, vm.read(loc, pos)?))
    }
}
