//! GnuCOBOL's COB-CRT-STATUS under `--compliance extended`: the code of the key that ended the last
//! screen ACCEPT.

use syntax::ast::*;
use syntax::parser::CRT_STATUS;
use syntax::Pos;

/// After each screen ACCEPT, and first in its exception phrases, COB-CRT-STATUS takes the code of
/// the key that ended it (assumption C489), where the program or one containing it declares it.
pub(crate) fn rewrite(program: &mut Program, inherited: &[DataEntry]) {
    let entries = program.working_storage.iter().chain(&program.local_storage).chain(&program.linkage).chain(inherited);
    if !entries.into_iter().any(|e| e.name.as_deref() == Some(CRT_STATUS)) {
        return;
    }
    for p in &mut program.paragraphs {
        statements(&mut p.statements);
    }
}

fn statements(stmts: &mut Vec<Stmt>) {
    let mut out = Vec::with_capacity(stmts.len());
    for mut s in std::mem::take(stmts) {
        for body in crate::oo::bodies_mut(&mut s) {
            statements(body);
        }
        let after = match &mut s {
            Stmt::Accept { screen: Some(_), exception, pos, .. } => {
                for body in [&mut exception.on, &mut exception.not_on].into_iter().flatten() {
                    body.insert(0, store(*pos));
                }
                Some(store(*pos))
            }
            _ => None,
        };
        out.push(s);
        out.extend(after);
    }
    *stmts = out;
}

/// COMPUTE COB-CRT-STATUS = the last screen key's code.
fn store(pos: Pos) -> Stmt {
    let key = FunctionCall { name: "CRT STATUS".into(), args: Vec::new(), modifier: None, refmod: None, all_subscripts: Vec::new(), pos };
    let target = Ref { name: CRT_STATUS.into(), qualifiers: Vec::new(), subscripts: Vec::new(), refmod: None, pos };
    Stmt::Compute { targets: vec![Target { r: target, rounded: false }], expr: Expr::Operand(Operand::Function(key)), size_error: None, pos }
}
