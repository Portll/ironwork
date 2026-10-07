//! What IBM Enterprise COBOL does with numeric data, on top of what the machine does (`zarch`).
//! Where the manuals leave the answer to the generated code, the choice made here is an entry in
//! [`assumptions::ASSUMPTIONS`] until an oracle run settles it.

pub mod assumptions;
pub mod binary;
pub mod float;
pub mod governs;
pub mod options;
pub mod precision;
pub mod sign;
pub mod zoned;

pub use binary::Native;
pub use options::{Arith, Assumed, BinCheck, CicsReturnWarning, Compliance, Currency, Dialect, DispSign, ExtraPlace, FastsrtAdvPrint, Initcheck, IntDate, LeServices, Nsymbol, Numcheck, Numproc, Options, Parmcheck, Pgmname, ProgramScope, Qualify, Quote, SortKeys, SourceFormat, Switched, Trunc, TruncCheck, UnresolvedCalls, Vlr, VsamOpenFs, ZonCheck};
