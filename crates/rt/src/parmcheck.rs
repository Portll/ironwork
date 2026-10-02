//! PARMCHECK around a CALL: the buffer after the caller's WORKING-STORAGE is set to X'AA' before
//! the CALL and tested after it, a changed byte being a warning (MSG) or the end of the run (ABD)
//! (Programming Guide SC27-8714-03, p. 397).

use crate::abend::{Abend, AbendCode};
use crate::unit::{Loader, RunUnit};
use crate::vocab::Pos;

type R<T> = Result<T, Abend>;

/// Sets the buffer, at `buffer`'s offset from the caller's slab at `base`, to X'AA'.
pub fn set(mem: &mut [u8], base: usize, buffer: Option<(u32, u32)>) {
    if let Some((offset, len)) = buffer {
        let at = base + offset as usize;
        mem[at..at + len as usize].fill(0xAA);
    }
}

/// After a CALL that returned: a change to the buffer is reported with the called program's name,
/// which `called` works out only then, the CALL's line, the calling program and the parameter the
/// change is put down to, of `arguments`, each data item's address and its name as written; as a
/// warning on standard error under MSG, and as the end of the run with U4038 under ABD (assumption
/// [`numeric::assumptions::PARMCHECK_MESSAGE`]).
#[allow(clippy::too_many_arguments)]
pub fn test<'a, H, L: Loader<H>>(
    unit: &mut RunUnit<'_, H, L>,
    base: usize,
    buffer: Option<(u32, u32)>,
    arguments: impl IntoIterator<Item = (usize, &'a str)>,
    called: impl FnOnce(&RunUnit<'_, H, L>) -> String,
    caller: &str,
    abd: bool,
    pos: Pos,
) -> R<()> {
    let Some((offset, len)) = buffer else { return Ok(()) };
    let at = base + offset as usize;
    if unit.mem[at..at + len as usize].iter().all(|&b| b == 0xAA) {
        return Ok(());
    }
    // The argument starting nearest the buffer; max_by_key keeps the later of two at one byte.
    let parameter = arguments.into_iter().filter(|&(a, _)| (base..at).contains(&a)).max_by_key(|&(a, _)| a);
    let beyond = parameter.map_or(String::new(), |(_, name)| format!(", beyond parameter {name}"));
    let message = format!("PARMCHECK: {}, called at line {} of program {caller}, wrote past the end of WORKING-STORAGE{beyond}", called(unit), pos.line);
    if abd {
        return Err(Abend { code: AbendCode::user(4038), message, pos, file: None });
    }
    let _ = writeln!(unit.err, "ironwork: {pos}: {message}");
    Ok(())
}
