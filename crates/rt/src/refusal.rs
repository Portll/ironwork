//! The run-time half of ironwork's message catalogue: each construct a run reaches that ironwork
//! does not run, refused by name. syntax::messages holds the compile-time half; docs/messages.md
//! lists both, and a test there keeps their ids apart.

use crate::abend::{Abend, AbendCode};
use crate::vocab::Pos;
use std::fmt::Display;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub id: &'static str,
    pub text: &'static str,
}

impl Refusal {
    /// The message as a run shows it: the id with severity S, then the site's words.
    pub fn message(&self, text: impl Display) -> String {
        format!("{}-S {text}", self.id)
    }

    /// The run's end with an IRONWORK abend at `pos`.
    pub fn abend(&self, text: impl Display, pos: Pos) -> Abend {
        self.ending(AbendCode::Ironwork, text, pos)
    }

    /// The run's end with `code`: EXEC for an EXEC statement, JAVA for a Java class or JNI service.
    pub fn ending(&self, code: AbendCode, text: impl Display, pos: Pos) -> Abend {
        Abend { code, message: self.message(text), pos, file: None }
    }
}

macro_rules! catalogue {
    ($($id:ident $text:literal;)*) => {
        $(pub const $id: Refusal = Refusal { id: stringify!($id), text: $text };)*
        /// Every run-time refusal, in id order.
        pub const RUNTIME: &[Refusal] = &[$($id),*];
    };
}

catalogue! {
    IWR0058 "EXEC CICS {command} is not supported yet";
    IWR0059 "EXEC CICS FORMATTIME {option} is not supported";
    IWR0060 "EXEC {kind} {command} was reached: ironwork for COBOL checks EXEC statements but does not run them yet";
    IWR0061 "EXEC SQL {verb} was reached: ironwork for COBOL does not run {statement}";
    IWR0062 "{what} was reached: {class} is a Java class, and ironwork for COBOL checks Java classes but has no JVM to run them";
    IWR0063 "CALL {name} was reached: {service} is a JNI service, and ironwork for COBOL has no JVM to run it";
    IWR0064 "FUNCTION {name} is not supported yet";
    IWR0065 "FUNCTION LENGTH of this argument is not supported yet";
    IWR0066 "DISPLAY of a floating-point value is not supported yet";
    IWR0067 "DISPLAY of a pointer, index or object reference is not supported";
    IWR0068 "ADVANCING {name} on {file}, whose FD has LINAGE, is not supported yet";
    IWR0070 "this BY VALUE argument is not supported";
    IWR0071 "this INVOKE argument is not supported";
    IWR0072 "{statement}: {name} is a Language Environment callable service that ironwork for COBOL does not provide yet";
    IWR0073 "the VM does not run {construct} yet; run it with --interpret";
    IWR0074 "{file}, a GLOBAL file of {declarer}, is written as a print file in one of {declarer} and {program} and not the other, which is not supported yet";
}
