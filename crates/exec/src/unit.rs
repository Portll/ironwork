//! The interpreter's run unit: `rt::unit`'s, holding each loaded program as its `Compiled` and
//! calling through the [`Library`].

pub use crate::loader::{AddProgram, Library};
pub use rt::unit::{ADDRESS_BASE, Clock, Event, LoadError, LoadedProgram, Loader, OS_COMMAND_ROUTINES, Observer, RETURN_CODE, StatementFilter};

use crate::Compiled;
use std::rc::Rc;

pub type RunUnit<'w> = rt::unit::RunUnit<'w, Rc<Compiled>, Library>;

/// What a run leaves in its run unit: memory, every byte, and each program's name and where its
/// storage starts, which the differential test compares between the interpreter and the VM.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Remains {
    pub mem: Vec<u8>,
    pub programs: Vec<(String, usize)>,
    /// Which bytes may hold input, and the first operation taint did not follow, when the run
    /// traced input.
    pub taint: Option<(Vec<u64>, Option<&'static str>)>,
    /// For a run a caller passed arguments to, each argument's bytes as the run left them, which
    /// the caller sees after its CALL; None for OMITTED.
    pub arguments: Vec<Option<Vec<u8>>>,
}

impl Remains {
    pub fn of<H: Clone, L: Loader<H>>(unit: &rt::unit::RunUnit<'_, H, L>) -> Self {
        let taint = unit.taint.as_ref().map(|t| (t.bits(), t.not_followed()));
        Self { mem: unit.mem.clone(), programs: unit.programs.iter().map(|p| (p.name.clone(), p.base)).collect(), taint, arguments: Vec::new() }
    }
}
