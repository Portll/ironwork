//! Invoking a user-defined function: the arguments evaluated here, the function run as a callee
//! through `rt::callee`, and the value of its RETURNING item the invocation's value.

use super::*;
use compile::function::Udf;
use rt::callee::Bound;
use std::rc::Rc;

impl<'p> Machine<'p, '_, '_> {
    pub(super) fn user_function(&self, name: &str) -> Option<&'p Udf> {
        self.functions.iter().find(|u| u.name == name)
    }

    /// The function's value, the function run when its operand is evaluated; STOP RUN in it ends
    /// the run from the statement that invoked it (assumption C274).
    pub(super) fn invoke_function(&mut self, udf: &'p Udf, f: &FunctionCall) -> R<Val> {
        let pos = f.pos;
        let index = match self.unit.load_entry(&udf.external, false) {
            Ok((index, _)) => index,
            Err(LoadError::NotFound) => {
                let message = format!("FUNCTION {}: its definition, {}, is in neither the source nor the program libraries", udf.name, udf.external);
                return Err(Abend { code: AbendCode::ModuleNotFound, message, pos, file: None });
            }
            Err(LoadError::Compile(message)) => return Err(Abend::ironwork(format!("FUNCTION {}: {message}", udf.name), pos)),
        };
        let compiled = match self.unit.programs[index].compiled.clone() {
            Some(c) if c.program.function.as_ref().is_some_and(|g| !g.prototype) => c,
            _ => return Err(Abend::ironwork(format!("FUNCTION {}: {} is a program, not a user-defined function", udf.name, udf.external), pos)),
        };
        let mut bound = Vec::with_capacity(f.args.len());
        for (arg, formal) in f.args.iter().zip(&udf.params) {
            bound.push(match arg {
                Expr::Operand(Operand::Ref(r)) if !formal.by_value => Bound::At(self.locate(r)?.offset),
                _ => Bound::Value(self.expr_value(arg, pos)?),
            });
        }
        self.nest(pos)?;
        let mark = self.unit.mem.len();
        let read_before = self.unit.pending();
        let (outcome, ()) = callee::run(self, &Callee { index, by: By::Function, mark: Some(mark), pos }, |m| {
            let outcome = run(&compiled, index, &mut *m.unit, &bound, pos);
            m.unit.resume_statement(read_before);
            Ok::<_, Abend>((outcome, ()))
        })?;
        self.unit.depth -= 1;
        match outcome? {
            (Ending::StopRun, _) => Err(Abend { code: AbendCode::Signal(Signal::StopRun), message: String::new(), pos, file: None }),
            (_, value) => Ok(value),
        }
    }
}

/// One activation of the function: each formal parameter given its argument's address or a
/// temporary its MOVE or COMPUTE fills, then the procedure run, then its RETURNING item read.
fn run(compiled: &Rc<Compiled>, index: usize, unit: &mut RunUnit<'_>, bound: &[Bound], pos: Pos) -> R<(Ending, Val)> {
    let mut callee = Machine::activation(compiled, index, unit, false)?;
    let using = &compiled.program.using;
    let addresses = rt::callee::bound_addresses(callee.unit, bound, using.iter().map(|param| record_size(&compiled.layout, &param.name)));
    callee.bind_linkage(&[], using, &addresses, false);
    for (param, b) in using.iter().zip(bound) {
        if let Bound::Value(value) = b {
            let dest = callee.parameter(&param.name, pos)?;
            callee.assign(dest, value.clone(), None, pos)?;
        }
    }
    callee.bind_linkage(&[], &[], &[], true);
    let ending = callee.run_from(None)?;
    let returning = compiled.program.returning.as_deref().unwrap_or_default();
    Ok((ending, callee.returned(returning, pos)?))
}

impl Machine<'_, '_, '_> {
    /// Where a LINKAGE record named in the PROCEDURE DIVISION header is, once it has an address.
    fn parameter(&mut self, name: &str, pos: Pos) -> R<Loc> {
        self.locate(&Ref { name: name.to_owned(), qualifiers: Vec::new(), subscripts: Vec::new(), refmod: None, pos })
    }
}

fn record_size(layout: &Layout, name: &str) -> usize {
    let root = layout.linkage_roots.iter().find(|&&i| layout.items[i].name.as_deref() == Some(name));
    root.map_or(0, |&i| layout.items[i].size as usize)
}
