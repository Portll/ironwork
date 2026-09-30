//! The runtime a program compiled by ironwork links, and none of the compiler: it may depend on
//! `numeric` and `zarch` only. It is AGPL-3.0-or-later with the ironwork runtime exception
//! (RUNTIME-EXCEPTION.md), so a compiled program that links it is not bound by the AGPL.
//! [`module`] reads and writes the load modules such a program is shipped as.

pub mod abend;
pub mod bms;
pub mod calendar;
pub mod cics;
pub mod cics_tables;
pub mod codec;
pub mod digest;
pub mod edit;
pub mod evidence;
pub mod files;
pub mod fixed;
pub mod intrinsic;
pub mod json;
pub mod xml;
pub mod le;
pub mod linage;
pub mod loc;
pub mod lir;
pub mod module;
pub mod oo;
pub mod picture;
pub mod reserved_words;
pub mod sql;
pub mod storage;
pub mod store;
pub mod strings;
pub mod terminal;
pub mod tn3270;
pub mod unit;
pub mod vocab;
