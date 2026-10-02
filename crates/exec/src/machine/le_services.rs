//! A CALL that reaches a Language Environment callable service: `rt::callee` builds the argument
//! addresses, the walker runs the CALL's phrases, and `rt::le` runs the service.

use super::*;
use crate::le::{self, LeHost};
use std::rc::Rc;

impl<'p> Machine<'p, '_, '_> {
    pub(super) fn le_call(&mut self, c: &'p Call, name: &str) -> R<Flow> {
        let mark = self.unit.mem.len();
        let outcome = callee::addresses(self, &call_args(&c.using), c.pos).and_then(|args| {
            self.parmcheck_set();
            self.le_service(name, &args, c.pos).map(|()| args)
        });
        self.unit.release_temporaries(mark);
        self.parmcheck_test(c, &outcome?, |_| name.to_owned())?;
        match &c.not_on_exception {
            Some(body) => self.run_block(body),
            None => Ok(Flow::Next),
        }
    }

    fn le_service(&mut self, name: &str, args: &[Option<usize>], pos: Pos) -> R<()> {
        match le::service(name) {
            Some(service) => le::call(self, service, args, pos),
            None => Err(Abend::ironwork(format!("CALL {name}: not a service ironwork provides"), pos)),
        }
    }
}

impl<'w> LeHost<'w> for Machine<'_, '_, 'w> {
    fn page(&self) -> &'static CodePage {
        self.page
    }

    fn method_name(program: &Rc<Compiled>) -> Option<String> {
        program.program.oo.as_deref().and_then(|o| o.method()).map(|m| format!("{}.{}", m.class, m.name))
    }
}
