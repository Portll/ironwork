//! The interpreter's run unit: `rt::unit`'s, holding each loaded program as its `Compiled` and
//! calling through the [`Library`].

pub use crate::loader::{AddProgram, Library};
pub use rt::unit::{ADDRESS_BASE, Clock, Event, LoadError, LoadedProgram, Loader, MAX_DEPTH, OS_COMMAND_ROUTINES, Observer, RETURN_CODE, StatementFilter};

use crate::Compiled;
use std::rc::Rc;

pub type RunUnit<'w> = rt::unit::RunUnit<'w, Rc<Compiled>, Library>;
pub type Loaded = rt::unit::Loaded<Rc<Compiled>>;

/// What a run leaves in its run unit: memory, every byte, and each program's name and where its
/// storage starts, which the differential test compares between the interpreter and the VM.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Remains {
    pub mem: Vec<u8>,
    pub programs: Vec<(String, usize)>,
}

impl Remains {
    pub fn of<H: Clone, L: Loader<H>>(unit: &rt::unit::RunUnit<'_, H, L>) -> Self {
        Self { mem: unit.mem.clone(), programs: unit.programs.iter().map(|p| (p.name.clone(), p.base)).collect() }
    }
}
