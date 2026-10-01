//! A CALL that reaches a Language Environment callable service: the walker builds the argument
//! addresses and runs the CALL's phrases, and `rt::le` runs the service.

use super::*;
use crate::le::{self, LeHost};
use std::rc::Rc;

impl<'p> Machine<'p, '_, '_> {
    pub(super) fn le_call(&mut self, c: &'p Call, name: &str) -> R<Flow> {
        let mark = self.unit.mem.len();
        let outcome = self.le_arguments(c).and_then(|args| self.le_service(name, &args, c.pos));
        self.unit.release_temporaries(mark);
        outcome?;
        match &c.not_on_exception {
            Some(body) => self.run_block(body),
            None => Ok(Flow::Next),
        }
    }

    /// Each argument's address in run-unit memory, as a CALL to a program passes it.
    fn le_arguments(&mut self, c: &Call) -> R<Vec<Option<usize>>> {
        let mut addresses = Vec::new();
        for arg in &c.using {
            let at = match (arg.mode, &arg.value) {
                (_, None) => None,
                (ArgMode::Reference, Some(Operand::Ref(r))) => Some(self.locate(r)?.offset),
                (ArgMode::Value, Some(op)) => {
                    let bytes = self.value_argument(op, c.pos)?;
                    Some(self.unit.push_temporary(&bytes))
                }
                (_, Some(op)) => {
                    let bytes = self.content_argument(op, c.pos)?;
                    Some(self.unit.push_temporary(&bytes))
                }
            };
            addresses.push(at);
        }
        Ok(addresses)
    }

    fn le_service(&mut self, name: &str, args: &[Option<usize>], pos: Pos) -> R<()> {
        match le::service(name) {
            Some(service) => le::call(self, service, args, pos),
            None => Err(Abend::ironwork(format!("CALL {name}: not a service ironwork provides"), pos)),
        }
    }
}

impl<'w> LeHost<'w> for Machine<'_, '_, 'w> {
    type Program = Rc<Compiled>;
    type Loader = crate::unit::Library;

    fn unit(&mut self) -> &mut RunUnit<'w> {
        self.unit
    }

    fn page(&self) -> &'static CodePage {
        self.page
    }

    fn method_name(program: &Rc<Compiled>) -> Option<String> {
        program.program.oo.as_deref().and_then(|o| o.method()).map(|m| format!("{}.{}", m.class, m.name))
    }
}
