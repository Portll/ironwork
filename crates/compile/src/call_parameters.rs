//! GnuCOBOL's NUMBER-OF-CALL-PARAMETERS under `--compliance extended`: the number of arguments the
//! running program was called with.

use syntax::ast::*;
use syntax::parser::CALL_PARAMETERS;
use syntax::Pos;

/// Where the parser declared NUMBER-OF-CALL-PARAMETERS for the program (IWX0078), it takes the
/// count where the program is entered: first in the PROCEDURE DIVISION and after each ENTRY.
pub(crate) fn rewrite(program: &mut Program, messages: &[syntax::Error]) {
    if !messages.iter().any(|m| m.id == Some("IWX0078")) {
        return;
    }
    if let Some(first) = program.paragraphs.get_mut(program.report_writer.procedure_start) {
        first.statements.insert(0, store(first.pos));
    }
    for p in &mut program.paragraphs {
        let at: Vec<(usize, Pos)> = p.statements.iter().enumerate().filter_map(|(k, s)| if let Stmt::Entry { pos, .. } = s { Some((k, *pos)) } else { None }).collect();
        for (k, pos) in at.into_iter().rev() {
            p.statements.insert(k + 1, store(pos));
        }
    }
}

/// COMPUTE NUMBER-OF-CALL-PARAMETERS = the running activation's argument count.
fn store(pos: Pos) -> Stmt {
    let count = FunctionCall { name: "CALL PARAMETERS".into(), args: Vec::new(), modifier: None, refmod: None, all_subscripts: Vec::new(), pos };
    let target = Ref { name: CALL_PARAMETERS.into(), qualifiers: Vec::new(), subscripts: Vec::new(), refmod: None, pos };
    Stmt::Compute { targets: vec![Target { r: target, rounded: false }], expr: Expr::Operand(Operand::Function(count)), size_error: None, pos }
}
