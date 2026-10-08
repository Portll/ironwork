//! What the first program of a batch run unit is given, and how its end is read: a job step's PARM
//! as Language Environment passes it, a caller's arguments, and control running past the last line
//! of a main program.

use crate::abend::{Abend, AbendCode, Ending};
use crate::vocab::Pos;
use zarch::ebcdic::CodePage;

/// What the first program of a batch run unit is given for its PROCEDURE DIVISION USING items.
#[derive(Clone, Copy, Debug, Default)]
pub enum Passed<'a> {
    /// Nothing: a main program no one passes anything.
    #[default]
    Nothing,
    /// A job step's PARM, as Language Environment builds its parameter list.
    Parm(&'a str),
    /// What a caller passes a subprogram, one per USING item: the bytes of the item passed, or
    /// None for OMITTED.
    Arguments(&'a [Option<Vec<u8>>]),
}

impl Passed<'_> {
    /// The addresses the USING items are bound to, each argument pushed as input.
    pub fn addresses<H: Clone, L: crate::unit::Loader<H>>(self, run_unit: &mut crate::unit::RunUnit<'_, H, L>, page: &CodePage) -> Vec<Option<usize>> {
        match self {
            Passed::Nothing => Vec::new(),
            Passed::Parm(parm) => vec![Some(push_parm(run_unit, page, parm))],
            Passed::Arguments(arguments) => arguments.iter().map(|a| a.as_deref().map(|bytes| push_input(run_unit, bytes))).collect(),
        }
    }

    /// Whether the program runs as a run unit's main program, where EXIT PROGRAM does nothing,
    /// rather than as one a caller passed arguments to.
    pub fn main(self) -> bool {
        !matches!(self, Passed::Arguments(_))
    }

    /// Each argument's bytes at `addresses` in `mem` as the run left them, None for OMITTED; none
    /// for a run given no arguments.
    pub fn returned(self, addresses: &[Option<usize>], mem: &[u8]) -> Vec<Option<Vec<u8>>> {
        let Passed::Arguments(arguments) = self else { return Vec::new() };
        arguments
            .iter()
            .zip(addresses)
            .map(|(a, at)| match (a, at) {
                (Some(a), Some(at)) => mem.get(*at..at + a.len()).map(<[u8]>::to_vec),
                _ => None,
            })
            .collect()
    }

    /// Gives the run unit what a job step's PARM sets: the program arguments, and the UPSI switches
    /// its runtime options give, which are off otherwise; a malformed UPSI is named on standard
    /// error.
    pub fn apply_parm<H: Clone, L: crate::unit::Loader<H>>(self, run_unit: &mut crate::unit::RunUnit<'_, H, L>) {
        let Passed::Parm(parm) = self else { return };
        run_unit.arguments = crate::le::parm::Arguments::of(parm);
        match crate::le::parm::upsi(parm) {
            Some(Ok(on)) => run_unit.set_switches(on),
            Some(Err(m)) => {
                let _ = writeln!(run_unit.err, "ironwork: {m}");
            }
            None => {}
        }
    }
}

/// A main program whose control ran past its last statement: IGZ0037S, a severity-3 condition that
/// ends the run U4038 (assumption C456), placed at the last paragraph, the one control ran out of;
/// compiled for GnuCOBOL (`cobc`), the end of the run, as GOBACK ends it. A program a caller passed
/// arguments to returns there, as an implicit EXIT PROGRAM does.
pub fn past_the_end(ending: Ending, main: bool, program: &str, last_paragraph: Option<Pos>, cobc: bool) -> Result<Ending, Abend> {
    if main && ending == Ending::EndOfProgram && cobc {
        return Ok(Ending::Goback);
    }
    if main && ending == Ending::EndOfProgram {
        let message = format!("IGZ0037S The flow of control in program {} proceeded beyond the last line of the program.", program.to_ascii_uppercase());
        return Err(Abend { code: AbendCode::user(4038), message, pos: last_paragraph.unwrap_or_default(), file: None });
    }
    Ok(ending)
}

/// A job step's PARM as Language Environment passes it, at the end of memory: input.
pub fn push_parm<H: Clone, L: crate::unit::Loader<H>>(run_unit: &mut crate::unit::RunUnit<'_, H, L>, page: &CodePage, parm: &str) -> usize {
    let area = crate::le::parm::parameter_area(crate::le::parm::program_arguments(parm), page);
    push_input(run_unit, &area)
}

/// `bytes` at the end of memory, marked as input.
fn push_input<H: Clone, L: crate::unit::Loader<H>>(run_unit: &mut crate::unit::RunUnit<'_, H, L>, bytes: &[u8]) -> usize {
    let at = run_unit.push_temporary(bytes);
    run_unit.mark_input(at, bytes.len(), true);
    at
}
