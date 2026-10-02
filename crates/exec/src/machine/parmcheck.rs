//! PARMCHECK around a CALL: the buffer after the program's WORKING-STORAGE is set to X'AA' before
//! the CALL and checked after it, a changed byte being a warning (MSG) or the end of the run (ABD)
//! (Programming Guide SC27-8714-03, p. 397).

use super::*;

impl<'p> Machine<'p, '_, '_> {
    /// Sets PARMCHECK's buffer to X'AA' before a CALL.
    pub(super) fn parmcheck_set(&mut self) {
        if let Some((offset, len)) = self.layout.parmcheck {
            let at = self.base + offset as usize;
            self.unit.mem[at..at + len as usize].fill(0xAA);
        }
    }

    /// After a CALL that returned, its arguments at `addresses`: a change to PARMCHECK's buffer is
    /// reported with the called program's name, which `called` works out only then, the CALL's
    /// line, this program and the parameter the change is put down to; as a warning on standard
    /// error under MSG, and as the end of the run with U4038 under ABD (assumption
    /// [`numeric::assumptions::PARMCHECK_MESSAGE`]).
    pub(super) fn parmcheck_test(&mut self, c: &Call, addresses: &[Option<usize>], called: impl FnOnce(&Self) -> String) -> R<()> {
        let Some((offset, len)) = self.layout.parmcheck else { return Ok(()) };
        let at = self.base + offset as usize;
        if self.unit.mem[at..at + len as usize].iter().all(|&b| b == 0xAA) {
            return Ok(());
        }
        // The argument starting nearest the buffer; max_by_key keeps the later of two at one byte.
        let parameter = c
            .using
            .iter()
            .zip(addresses)
            .filter_map(|(arg, address)| match (&arg.value, *address) {
                (Some(Operand::Ref(r)), Some(a)) if (self.base..at).contains(&a) => Some((a, r.name.as_str())),
                _ => None,
            })
            .max_by_key(|&(a, _)| a);
        let beyond = parameter.map_or(String::new(), |(_, name)| format!(", beyond parameter {name}"));
        let message = format!("PARMCHECK: {}, called at line {} of program {}, wrote past the end of WORKING-STORAGE{beyond}", called(self), c.pos.line, self.program.id);
        if self.options.parmcheck.is_some_and(|p| p.abd) {
            return Err(Abend { code: AbendCode::user(4038), message, pos: c.pos, file: None });
        }
        let _ = writeln!(self.unit.err, "ironwork: {}: {message}", c.pos);
        Ok(())
    }
}
