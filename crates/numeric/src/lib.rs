//! What IBM Enterprise COBOL does with numeric data, on top of what the machine does (`zarch`).
//! Where the manuals leave the answer to the generated code, the choice made here is an entry in
//! [`assumptions::ASSUMPTIONS`] until an oracle run settles it.

pub mod assumptions;
pub mod binary;
pub mod float;
pub mod options;
pub mod precision;
pub mod sign;

pub use options::{Arith, Numproc, Options, Trunc, TruncCheck};
