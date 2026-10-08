//! GnuCOBOL's MOVE of a procedure-pointer or function-pointer under `--compliance extended`: the
//! SET Enterprise COBOL writes for it.

use crate::layout::{Kind, Layout, Resolved};
use syntax::ast::*;
use syntax::Error;

/// Each MOVE whose sender and receivers are all procedure-pointers or function-pointers becomes
/// SET receivers TO sender, with IWX0080-W.
pub(crate) fn rewrite(program: &mut Program, layout: &Layout, errors: &mut Vec<Error>) {
    for p in &mut program.paragraphs {
        statements(&mut p.statements, layout, errors);
    }
}

fn statements(stmts: &mut [Stmt], layout: &Layout, errors: &mut Vec<Error>) {
    let pointer = |r: &Ref| matches!(layout.resolve(&r.name, &r.qualifiers, r.pos), Ok(Resolved::Item(i)) if layout.items[i].kind == Kind::ProgramPointer);
    for s in stmts.iter_mut() {
        for body in crate::oo::bodies_mut(s) {
            statements(body, layout, errors);
        }
        let Stmt::Move { from: Operand::Ref(from), to, pos, .. } = s else { continue };
        if !pointer(from) || to.is_empty() || !to.iter().all(pointer) {
            continue;
        }
        errors.push(syntax::messages::IWX0080.at(*pos, format!("MOVE {} of a procedure-pointer or function-pointer (GnuCOBOL and Micro Focus; Enterprise COBOL writes SET): it is read as SET ... TO {}", from.name, from.name)));
        *s = Stmt::Set { set: SetStmt::To { targets: std::mem::take(to), value: Operand::Ref(from.clone()) }, pos: *pos };
    }
}
