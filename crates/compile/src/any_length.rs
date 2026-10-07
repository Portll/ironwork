//! GnuCOBOL's and Micro Focus's ANY LENGTH parameter under `--compliance extended`: an
//! alphanumeric 01 or 77 item of the LINKAGE SECTION as long as the argument each CALL passes.

use crate::layout::MAX_STORAGE;
use syntax::ast::*;
use syntax::messages::{IWR0076, IWX0035};
use syntax::{Error, Pos};

/// The function that gives the length of the argument in a USING position of the running
/// activation. Its space keeps any source from naming it.
pub const ARGUMENT_LENGTH: &str = "ARGUMENT LENGTH";

/// Rewrites each ANY LENGTH parameter as a group holding a table of single characters whose count
/// is a counter of its own, and moves the argument's length into the counter where the procedure
/// starts (assumption C481). Refuses each ANY LENGTH entry it does not read so.
pub(crate) fn rewrite(program: &mut Program, errors: &mut Vec<Error>) {
    let elsewhere = program.working_storage.iter().chain(&program.local_storage).chain(program.files.iter().flat_map(|f| &f.records));
    for e in elsewhere.filter(|e| e.any_length) {
        errors.push(refusal(e, "only a parameter, an item of the LINKAGE SECTION, is as long as its argument"));
    }
    if !program.linkage.iter().any(|e| e.any_length) {
        return;
    }
    let mut entry_using = Vec::new();
    for p in &program.paragraphs {
        crate::oo::each(&p.statements, &mut |s| {
            if let Stmt::Entry { using, .. } = s {
                entry_using.extend(using.iter().map(|u| u.name.clone()));
            }
        });
    }
    let mut counters = Vec::new();
    let mut moves = Vec::new();
    let mut at = 0;
    while at < program.linkage.len() {
        if !program.linkage[at].any_length {
            at += 1;
            continue;
        }
        program.linkage[at].any_length = false;
        let e = &program.linkage[at];
        let name = e.name.clone().unwrap_or_default();
        let group = program.linkage.get(at + 1).is_some_and(|next| next.level > e.level && next.level != 88 && e.level != 77);
        let position = program.using.iter().position(|u| u.name == name);
        let why = if !matches!(e.level, 1 | 77) || e.name.is_none() {
            Some("it is not an 01 or 77 entry with a name")
        } else if group || !e.picture.as_deref().is_none_or(alphanumeric) || e.usage.is_some_and(|u| u != Usage::Display) {
            Some("it is not an alphanumeric item")
        } else if e.occurs.is_some() || e.redefines.is_some() {
            Some("it has OCCURS or REDEFINES")
        } else if program.returning.as_deref() == Some(name.as_str()) {
            Some("it is the RETURNING item, whose length the program would choose")
        } else if entry_using.contains(&name) {
            Some("an ENTRY statement names it, and the program is entered there with no length")
        } else if position.is_none() {
            Some("PROCEDURE DIVISION USING does not name it")
        } else {
            None
        };
        if let Some(why) = why {
            errors.push(refusal(e, why));
            at += 1;
            continue;
        }
        let pos = e.pos;
        errors.push(IWX0035.at(pos, format!("ANY LENGTH (GnuCOBOL and Micro Focus; Enterprise COBOL's parameters have the length their entries give): {name} is as long as the argument each CALL passes for it")));
        let counter = format!("ANY LENGTH {name}");
        let record = &mut program.linkage[at];
        record.level = 1;
        record.picture = None;
        record.usage = None;
        let mut chars = crate::report::entry(2, Some(format!("{counter} CHARS")), Some("X".into()), None, pos);
        chars.occurs = Some(MAX_STORAGE);
        chars.occurs_min = Some(0);
        chars.depending_on = Some(reference(&counter, pos));
        program.linkage.insert(at + 1, chars);
        moves.push(argument_length(position.unwrap_or_default() + 1, &counter, pos));
        counters.push(crate::report::entry(1, Some(counter), Some("S9(9)".into()), Some(Usage::Binary), pos));
        at += 2;
    }
    if program.recursive || program.function.is_some() { program.local_storage.extend(counters) } else { program.working_storage.extend(counters) }
    if let Some(first) = program.paragraphs.get_mut(program.report_writer.procedure_start) {
        first.statements.splice(0..0, moves);
    }
}

fn refusal(e: &DataEntry, why: &str) -> Error {
    let name = e.name.as_deref().unwrap_or("FILLER");
    IWR0076.at(e.pos, format!("ANY LENGTH on {name}: ironwork reads it on an alphanumeric 01 or 77 parameter, and {why}"))
}

/// A PICTURE of X alone, as GnuCOBOL's ANY LENGTH items are written.
fn alphanumeric(picture: &str) -> bool {
    let p = picture.to_ascii_uppercase();
    let symbols = p.split(['(', ')']).enumerate().filter(|(k, _)| k % 2 == 0).map(|(_, s)| s).collect::<String>();
    !symbols.is_empty() && symbols.chars().all(|c| c == 'X')
}

fn reference(name: &str, pos: Pos) -> Ref {
    Ref { name: name.into(), qualifiers: Vec::new(), subscripts: Vec::new(), refmod: None, pos }
}

/// COMPUTE `counter` = the length of the argument in USING position `position`.
fn argument_length(position: usize, counter: &str, pos: Pos) -> Stmt {
    let length = FunctionCall { name: ARGUMENT_LENGTH.into(), args: vec![Expr::Operand(Operand::Literal(Literal::Number(position.to_string())))], modifier: None, refmod: None, all_subscripts: Vec::new(), pos };
    Stmt::Compute { targets: vec![Target { r: reference(counter, pos), rounded: false }], expr: Expr::Operand(Operand::Function(length)), size_error: None, pos }
}

/// Whether LINKAGE record `name` is an ANY LENGTH parameter as `rewrite` left it.
pub fn is_parameter(program: &Program, name: &str) -> bool {
    let chars = format!("ANY LENGTH {name} CHARS");
    program.linkage.iter().any(|e| e.name.as_deref() == Some(chars.as_str()))
}
