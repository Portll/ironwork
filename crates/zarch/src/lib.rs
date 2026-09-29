//! What the machine does with the bytes of a COBOL program, independent of any compiler's choices.
//! The normative source is *z/Architecture Principles of Operation* (SA22-7832); code pages come
//! from IBM's tables as ICU publishes them.

pub mod check;
pub mod decimal;
pub mod ebcdic;
pub mod hfp;
pub mod wide;

pub use check::{Cc, ProgramCheck, ProgramMask};
