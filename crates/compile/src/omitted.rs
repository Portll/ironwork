//! GnuCOBOL's CALL ... RETURNING OMITTED, NOTHING and NULL, under which the caller's RETURN-CODE
//! is as it was before the CALL.

use syntax::ast::*;
use syntax::messages::{IWC0300, IWX0024};
use syntax::{Error, Pos};

/// The item the caller's RETURN-CODE is kept in across such a CALL: the space keeps any source
/// from naming it.
const SAVED_RETURN_CODE: &str = "RETURN-CODE SAVED";

/// Refuses each such CALL under strict. Under extended, rewrites it to keep the caller's
/// RETURN-CODE before the CALL and put it back when the CALL succeeds, with IWX0024-W.
pub(crate) fn rewrite(program: &mut Program, inherited: &[DataEntry], extended: bool, errors: &mut Vec<Error>) {
    let names = program.working_storage.iter().chain(&program.local_storage).chain(&program.linkage).chain(inherited).chain(program.files.iter().flat_map(|f| &f.records));
    // NOTHING is not reserved in Enterprise COBOL, so an item of that name stays a RETURNING item.
    let nothing = !names.into_iter().any(|e| e.name.as_deref() == Some("NOTHING") || e.indexed_by.iter().any(|i| i == "NOTHING"));
    let mut rewriter = Rewriter { nothing, extended, errors, rewritten: false };
    for p in &mut program.paragraphs {
        rewriter.statements(&mut p.statements);
    }
    if rewriter.rewritten {
        let saved = crate::report::entry(1, Some(SAVED_RETURN_CODE.into()), Some("S9(4)".into()), Some(Usage::Binary), Pos::default());
        if program.recursive { program.local_storage.push(saved) } else { program.working_storage.push(saved) }
    }
}

struct Rewriter<'e> {
    nothing: bool,
    extended: bool,
    errors: &'e mut Vec<Error>,
    rewritten: bool,
}

impl Rewriter<'_> {
    fn word(&self, r: &Ref) -> Option<&'static str> {
        if !r.qualifiers.is_empty() || !r.subscripts.is_empty() || r.refmod.is_some() {
            return None;
        }
        match r.name.as_str() {
            "OMITTED" => Some("OMITTED"),
            "NULL" => Some("NULL"),
            "NOTHING" if self.nothing => Some("NOTHING"),
            _ => None,
        }
    }

    fn statements(&mut self, stmts: &mut Vec<Stmt>) {
        let mut out = Vec::with_capacity(stmts.len());
        for mut s in std::mem::take(stmts) {
            for body in crate::oo::bodies_mut(&mut s) {
                self.statements(body);
            }
            if let Stmt::Call(c) = &mut s
                && let Some(word) = c.returning.as_ref().and_then(|r| self.word(r))
            {
                let at = c.returning.take().map_or(c.pos, |r| r.pos);
                if !self.extended {
                    self.errors.push(IWC0300.at(at, format!("CALL ... RETURNING {word}: GnuCOBOL's, not Enterprise COBOL's, whose RETURNING names a data item; --compliance extended reads it")));
                } else {
                    self.errors.push(IWX0024.at(at, format!("CALL ... RETURNING {word} (GnuCOBOL; Enterprise COBOL's RETURNING names a data item): the CALL leaves the caller's RETURN-CODE as it was")));
                    self.rewritten = true;
                    out.push(move_to(c.pos, "RETURN-CODE", SAVED_RETURN_CODE));
                    c.not_on_exception.get_or_insert_default().insert(0, move_to(c.pos, SAVED_RETURN_CODE, "RETURN-CODE"));
                }
            }
            out.push(s);
        }
        *stmts = out;
    }
}

fn move_to(pos: Pos, from: &str, to: &str) -> Stmt {
    let item = |name: &str| Ref { name: name.into(), qualifiers: Vec::new(), subscripts: Vec::new(), refmod: None, pos };
    Stmt::Move { from: Operand::Ref(item(from)), to: vec![item(to)], pos }
}
