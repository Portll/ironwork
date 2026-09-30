//! The runtime a program compiled by ironwork links, and none of the compiler: it may depend on
//! `numeric` and `zarch` only. It is AGPL-3.0-or-later with the ironwork runtime exception
//! (RUNTIME-EXCEPTION.md), so a compiled program that links it is not bound by the AGPL.
//! [`module`] reads and writes the load modules such a program is shipped as.

pub mod calendar;
pub mod cics_tables;
pub mod codec;
pub mod digest;
pub mod edit;
pub mod evidence;
pub mod module;
pub mod picture;
pub mod reserved_words;
pub mod sql;
pub mod storage;
pub mod strings;
pub mod vocab;
