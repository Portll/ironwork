//! The special registers of JSON GENERATE and the XML statements (Language Reference SC27-8713-03,
//! pp. 21-37): JSON-CODE, JSON-STATUS, XML-CODE, XML-EVENT and XML-INFORMATION are declared for a
//! program that has the statement; XML-TEXT and the others whose length varies are located while it runs.

use crate::layout::Layout;
use crate::oo::bodies;
use syntax::Pos;
use syntax::ast::{DataEntry, Program, Ref, Stmt};

const JSON_REGISTERS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. REGISTERS.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n",
    "       01  JSON-CODE PIC S9(9) BINARY VALUE ZERO.\n",
    "       01  JSON-STATUS PIC S9(9) BINARY VALUE ZERO.\n",
);

const XML_REGISTERS: &str = concat!(
    "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. REGISTERS.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n",
    "       01  XML-CODE PIC S9(9) BINARY VALUE ZERO.\n",
    "       01  XML-EVENT PIC X(30) VALUE SPACE.\n",
    "       01  XML-INFORMATION PIC S9(9) BINARY VALUE ZERO.\n",
);

/// The XML special registers whose length is that of the fragment they hold.
pub const XML_FRAGMENTS: &[&str] = &["XML-TEXT", "XML-NTEXT", "XML-NAMESPACE", "XML-NNAMESPACE", "XML-NAMESPACE-PREFIX", "XML-NNAMESPACE-PREFIX"];

/// Whether `r` names one of [`XML_FRAGMENTS`] that the program does not declare itself.
pub fn xml_register(layout: &Layout, r: &Ref) -> bool {
    r.qualifiers.is_empty() && r.subscripts.is_empty() && XML_FRAGMENTS.contains(&r.name.as_str()) && layout.resolve(&r.name, &[], r.pos).is_err()
}

fn any(stmts: &[Stmt], found: &dyn Fn(&Stmt) -> bool) -> bool {
    stmts.iter().any(|s| found(s) || bodies(s).into_iter().any(|b| any(b, found)))
}

fn declared(program: &Program) -> Vec<String> {
    let entries: Vec<&DataEntry> = program.working_storage.iter().chain(&program.local_storage).chain(&program.linkage).chain(program.files.iter().flat_map(|f| &f.records)).collect();
    entries.into_iter().filter_map(|e| e.name.clone()).collect()
}

pub(crate) fn with_special_registers(mut program: Program) -> Program {
    let uses = |found: &dyn Fn(&Stmt) -> bool| program.paragraphs.iter().any(|p| any(&p.statements, &found));
    let wanted: Vec<&str> = [(uses(&|s| matches!(s, Stmt::JsonGenerate(_) | Stmt::JsonParse(_))), JSON_REGISTERS), (uses(&|s| matches!(s, Stmt::XmlParse(_) | Stmt::XmlGenerate(_))), XML_REGISTERS)]
        .into_iter()
        .filter_map(|(used, text)| used.then_some(text))
        .collect();
    for text in wanted {
        let Ok(registers) = syntax::parse(text) else { continue };
        let declared = declared(&program);
        for mut entry in registers.working_storage {
            if entry.name.as_ref().is_some_and(|n| !declared.contains(n)) {
                entry.pos = Pos::default();
                program.working_storage.push(entry);
            }
        }
    }
    program
}
