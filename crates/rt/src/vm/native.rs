//! What a program's generated code runs its blocks through (codegen-runtime.md §14 step 6). The
//! code `ironwork compile --native` generates for a program is a [`Native`] function: the VM's
//! dispatch loop over that program's blocks, written out, calling the VM's own op and terminator
//! code through [`Machine`] for everything it does not run itself. A run anything watches, which
//! taint, statement tracing, the run's limits, an observer or NUMCHECK make, is the VM's alone.

use super::flow::{Exit, Next, stops_run};
use super::{Code, R, Stop, Vm};
use crate::abend::Ending;
use crate::fast::Storage;
use crate::lir::{BlockId, Step};
use crate::unit::Loader;
use std::rc::Rc;

/// A program's generated code: its blocks from `block` until the frame at `floor` completes or is
/// left, or the run ends, as the VM's dispatch runs them; None, having run nothing, where the run is
/// watched.
pub type Native = fn(&mut dyn Machine, BlockId, usize) -> Option<Result<Exit, Stop>>;

/// A program's generated code: `run` as the VM's dispatch hands it control, and `direct`, where the
/// generator wrote one, running the program from its start without an activation of the VM, which
/// a CALL that needs nothing else of one runs (`call_nested`).
#[derive(Clone, Copy)]
pub struct NativeProgram {
    pub run: Native,
    pub direct: Option<crate::fast::Direct>,
}

/// The VM's activation as generated code drives it.
pub trait Machine {
    /// Whether anything watches the run, so that only the VM's own dispatch may run it.
    fn watched(&self) -> bool;
    /// Control has reached `block`: at a paragraph's entry, what the VM does there.
    fn entered(&mut self, block: BlockId);
    /// Op `k` of `block`, run as the VM runs it, with its arm taken; Some where it transfers control
    /// or ends the run, with where control goes.
    fn run_op(&mut self, block: BlockId, k: usize, arm: &mut Option<u8>, floor: usize) -> R<Option<Next>>;
    /// `block`'s terminator, with the arm its ops left, as the VM ends a block.
    fn end(&mut self, block: BlockId, arm: Option<u8>, floor: usize) -> R<Next>;
    /// The activation's storage, for generated code to read and store in itself.
    fn storage(&mut self) -> Storage<'_>;
}

impl<L: Loader<Rc<Code>>> Machine for Vm<'_, '_, '_, L> {
    fn watched(&self) -> bool {
        self.unit.statements.is_some() || self.unit.taint.is_some() || self.unit.limited() || self.unit.observer.is_some() || self.p.options.options.numcheck.is_some()
    }

    fn entered(&mut self, block: BlockId) {
        if self.code.entry_of[block as usize].is_some() {
            self.paragraph_reached();
        }
    }

    fn run_op(&mut self, block: BlockId, k: usize, arm: &mut Option<u8>, floor: usize) -> R<Option<Next>> {
        let p = self.p;
        let step = match self.op(&p.blocks[block as usize].ops[k], p.debug.ops[block as usize][k]) {
            Ok(Step::Next) => return Ok(None),
            Ok(Step::Arm(a)) => {
                *arm = Some(a);
                return Ok(None);
            }
            Ok(step) => step,
            Err(halt) if stops_run(&halt) => Step::End(Ending::StopRun),
            Err(halt) => return Err(halt),
        };
        self.transfer(step, floor).map(Some)
    }

    fn end(&mut self, block: BlockId, arm: Option<u8>, floor: usize) -> R<Next> {
        let p = self.p;
        let b = &p.blocks[block as usize];
        match self.terminator(&b.end, p.debug.ops[block as usize][b.ops.len()], arm, floor) {
            Err(halt) if stops_run(&halt) => Ok(Next::Exit(Exit::End(Ending::StopRun))),
            next => next,
        }
    }

    fn storage(&mut self) -> Storage<'_> {
        Storage { mem: &mut self.unit.mem, program: self.base, local: self.local_base, linkage: &self.linkage, options: &self.p.options.options }
    }
}
