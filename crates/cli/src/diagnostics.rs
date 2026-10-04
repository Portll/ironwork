//! `--diagnostics text|json`: how a compile's messages reach standard error.

use exec::evidence::{Value, canonical, fields};
use std::sync::atomic::{AtomicBool, Ordering};

static JSON: AtomicBool = AtomicBool::new(false);

/// Sets the form the rest of the process writes compiler messages in, once the flags are read.
pub fn follow(json: bool) {
    JSON.store(json, Ordering::Relaxed);
}

/// One message as `--diagnostics` asks: `file:line:col: message`, or a JSON object whose `file` is
/// `main`, the program as it was named, and whose `member` is the COPY member the position is in.
pub fn line(m: &syntax::Error, main: &str) -> String {
    if !JSON.load(Ordering::Relaxed) {
        return m.place(main);
    }
    let at = |n: u32| if m.pos.line == 0 { Value::Null } else { Value::Int(i64::from(n)) };
    canonical(&Value::Obj(fields([
        ("file", main.into()),
        ("member", m.file.clone().map_or(Value::Null, Value::Str)),
        ("line", at(m.pos.line)),
        ("col", at(m.pos.col)),
        ("id", m.id.map_or(Value::Null, Value::from)),
        ("severity", m.severity.letter().to_string().into()),
        ("message", m.message.clone().into()),
    ])))
}
