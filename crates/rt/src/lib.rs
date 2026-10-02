//! The runtime a program compiled by ironwork links, and none of the compiler: it may depend on
//! `numeric` and `zarch` only. It is AGPL-3.0-or-later with the ironwork runtime exception
//! (RUNTIME-EXCEPTION.md), so a compiled program that links it is not bound by the AGPL.
//! [`module`] reads and writes the load modules such a program is shipped as.

pub mod abend;
pub mod accept;
pub mod arith;
pub mod bms;
pub mod calendar;
pub mod callee;
pub mod cics;
pub mod cics_tables;
pub mod codec;
pub mod digest;
pub mod display;
pub mod edit;
pub mod evidence;
pub mod feedback;
pub mod files;
pub mod fileio;
pub mod fixed;
pub mod host;
pub mod intrinsic;
pub mod jni;
pub mod json;
pub mod xml;
pub mod le;
pub mod linage;
pub mod loc;
pub mod lir;
pub mod module;
pub mod oo;
pub mod parmcheck;
pub mod picture;
pub mod printer;
pub mod report;
pub mod reserved_words;
pub mod set;
pub mod sort;
pub mod sql;
pub mod storage;
pub mod store;
pub mod strings;
pub mod terminal;
pub mod text;
pub mod tn3270;
pub mod unit;
pub mod virtual_printer;
pub mod vm;
pub mod vocab;
