//! The special registers the compiler declares for a program that names them: WHEN-COMPILED,
//! PIC X(16), the compile time as MM/DD/YYhh.mm.ss (Language Reference SC27-8713-03, p. 23).
//! A program that does not name it gets no item, so its module does not depend on when it was
//! compiled.

use rt::calendar::civil;
use rt::lir::CompileTime;
use syntax::Pos;
use syntax::ast::Program;

pub(crate) fn with_when_compiled(mut program: Program, at: CompileTime) -> Program {
    let named = program.registers.iter().any(|r| r == "WHEN-COMPILED");
    let declared = program.working_storage.iter().chain(&program.local_storage).chain(&program.linkage).chain(program.files.iter().flat_map(|f| &f.records)).any(|e| e.name.as_deref() == Some("WHEN-COMPILED"));
    if !named || declared {
        return program;
    }
    let c = civil(at.seconds);
    let text = format!(
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. REGISTERS.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01  WHEN-COMPILED PIC X(16) VALUE '{:02}/{:02}/{:02}{:02}.{:02}.{:02}'.\n",
        c.month,
        c.day,
        c.year.rem_euclid(100),
        c.hour,
        c.minute,
        c.second
    );
    if let Ok(registers) = syntax::parse(&text) {
        for mut entry in registers.working_storage {
            entry.pos = Pos::default();
            program.working_storage.push(entry);
        }
    }
    program
}
