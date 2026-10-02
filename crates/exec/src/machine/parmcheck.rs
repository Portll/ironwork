//! PARMCHECK around a CALL, as `rt::parmcheck` runs it over this program's slab.

use super::*;

impl<'p> Machine<'p, '_, '_> {
    /// Sets PARMCHECK's buffer to X'AA' before a CALL.
    pub(super) fn parmcheck_set(&mut self) {
        rt::parmcheck::set(&mut self.unit.mem, self.base, self.layout.parmcheck);
    }

    /// After a CALL that returned, its arguments at `addresses`: `rt::parmcheck::test`, the called
    /// program's name worked out by `called` only when the buffer changed.
    pub(super) fn parmcheck_test(&mut self, c: &Call, addresses: &[Option<usize>], called: impl FnOnce(&RunUnit) -> String) -> R<()> {
        let arguments = c.using.iter().zip(addresses).filter_map(|(arg, address)| match (&arg.value, *address) {
            (Some(Operand::Ref(r)), Some(a)) => Some((a, r.name.as_str())),
            _ => None,
        });
        let abd = self.options.parmcheck.is_some_and(|p| p.abd);
        rt::parmcheck::test(self.unit, self.base, self.layout.parmcheck, arguments, called, &self.program.id, abd, c.pos)
    }
}
