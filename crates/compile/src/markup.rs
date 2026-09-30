//! The special registers of JSON GENERATE: JSON-CODE and JSON-STATUS (Language Reference
//! SC27-8713-03, pp. 21-22), declared for a program that has the statement.

use crate::oo::bodies;
use syntax::Pos;
use syntax::ast::{DataEntry, Program, Stmt};

const JSON_REGISTERS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. REGISTERS.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n",
    "       01  JSON-CODE PIC S9(9) BINARY VALUE ZERO.\n",
    "       01  JSON-STATUS PIC S9(9) BINARY VALUE ZERO.\n",
);

fn any(stmts: &[Stmt], found: &impl Fn(&Stmt) -> bool) -> bool {
    stmts.iter().any(|s| found(s) || bodies(s).into_iter().any(|b| any(b, found)))
}

fn declared(program: &Program) -> Vec<String> {
    let entries: Vec<&DataEntry> = program.working_storage.iter().chain(&program.local_storage).chain(&program.linkage).chain(program.files.iter().flat_map(|f| &f.records)).collect();
    entries.into_iter().filter_map(|e| e.name.clone()).collect()
}

pub(crate) fn with_special_registers(mut program: Program) -> Program {
    let json = program.paragraphs.iter().any(|p| any(&p.statements, &|s| matches!(s, Stmt::JsonGenerate(_))));
    if !json {
        return program;
    }
    let Ok(registers) = syntax::parse(JSON_REGISTERS) else { return program };
    let declared = declared(&program);
    for mut entry in registers.working_storage {
        if entry.name.as_ref().is_some_and(|n| !declared.contains(n)) {
            entry.pos = Pos::default();
            program.working_storage.push(entry);
        }
    }
    program
}
