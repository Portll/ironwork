//! The interpreter's run unit: `rt::unit`'s, holding each loaded program as its `Compiled` and
//! calling through the [`Library`].

pub use crate::loader::{AddProgram, Library};
pub use rt::unit::{ADDRESS_BASE, Clock, Event, LoadError, LoadedProgram, Loader, MAX_DEPTH, Observer, RETURN_CODE};

use crate::Compiled;
use std::rc::Rc;

pub type RunUnit<'w> = rt::unit::RunUnit<'w, Rc<Compiled>, Library>;
pub type Loaded = rt::unit::Loaded<Rc<Compiled>>;
