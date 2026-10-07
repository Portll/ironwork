//! User-defined functions (lir.md §9.15): an invocation as `Machine::invoke_function` evaluates
//! it, what its value reads as, and what a function's definition gives each invocation.

use super::data::{Side, Within, value_of};
use super::{Lower, R, push, unsupported};
use crate::function::Udf;
use rt::fixed::places_of;
use rt::lir::{self, FunctionDefinition, PlaceId, RefMod, UserArgument, UserFunctionPlan};
use rt::storage::Kind;
use syntax::Pos;
use syntax::ast::{self, Expr, FunctionCall, Operand, Ref};

impl<'c> Lower<'c> {
    /// The user-defined function the program may invoke by this name.
    pub(super) fn user_defined(&self, name: &str) -> Option<&'c Udf> {
        self.c.functions.iter().find(|u| u.name == name)
    }

    /// An invocation of `udf`: each argument as the walker evaluates it, a data item passed BY
    /// REFERENCE located and any other by `expr_value`, apart from any arithmetic around the
    /// invocation; then reference modification of the value.
    pub(super) fn user_function(&mut self, udf: &Udf, f: &FunctionCall) -> R<(lir::Operand, Side)> {
        // The walker's `invoke_function` reads neither a modifier nor ALL subscripts.
        let FunctionCall { name: _, args: _, modifier: _, refmod, all_subscripts: _, pos } = f;
        let pos = *pos;
        let outer = std::mem::replace(&mut self.within, Within::Own);
        let args = self.user_arguments(udf, f);
        self.within = outer;
        let args = args?;
        let refmod = match refmod {
            None => None,
            Some(ast::RefMod { start, length }) => {
                let start = self.int_expr(start, pos)?;
                let length = match length {
                    Some(l) => Some(self.int_expr(l, pos)?),
                    None => None,
                };
                Some(RefMod { start, length, check: false })
            }
        };
        let plan = UserFunctionPlan { name: self.sym(&udf.name), external: self.sym(&udf.external), args, refmod, at: self.at(pos) };
        let id = push(&mut self.services.user_functions, plan, "user-defined function invocations")?;
        Ok((lir::Operand::UserFunction(id), result(udf)))
    }

    fn user_arguments(&mut self, udf: &Udf, f: &FunctionCall) -> R<Vec<UserArgument>> {
        let mut args = Vec::with_capacity(f.args.len());
        for (arg, formal) in f.args.iter().zip(&udf.params) {
            args.push(match arg {
                Expr::Operand(Operand::Ref(r)) if !formal.by_value => UserArgument::Reference(self.place(r, false)?),
                _ => UserArgument::Value(self.comparand(arg, f.pos)?.0),
            });
        }
        Ok(args)
    }

    /// A function definition's formal parameters and RETURNING item, each its whole LINKAGE record
    /// as `Machine::parameter` and `returned` locate it by name at every invocation.
    pub(super) fn function_definition(&mut self) -> R<Option<FunctionDefinition>> {
        let program = self.program;
        let Some(function) = program.function.as_ref().filter(|f| !f.prototype) else { return Ok(None) };
        let pos = function.pos;
        let Some(returning) = program.returning.as_deref() else { return unsupported("a user-defined function without RETURNING", pos) };
        let mut params = Vec::with_capacity(program.using.len());
        for param in &program.using {
            params.push(self.record(&param.name, pos)?);
        }
        let returning = self.record(returning, pos)?;
        Ok(Some(FunctionDefinition { params, returning }))
    }

    /// A LINKAGE record named in the PROCEDURE DIVISION header, located as `locate` finds it by
    /// name. One holding an OCCURS DEPENDING ON table is refused: locating it reads the count and
    /// may abend, and the walker names the invocation's position there, which the definition does
    /// not know.
    fn record(&mut self, name: &str, pos: Pos) -> R<PlaceId> {
        let r = Ref { name: name.to_owned(), qualifiers: Vec::new(), subscripts: Vec::new(), refmod: None, pos };
        let place = self.place(&r, false)?;
        if !self.places[place as usize].odo.is_empty() {
            return unsupported("a user-defined function's parameter or RETURNING record holding an OCCURS DEPENDING ON table", pos);
        }
        Ok(place)
    }
}

/// What the function's value reads as: its RETURNING item read as an item of that description is,
/// a number's digits counting its PICTURE scaling positions.
fn result(udf: &Udf) -> Side {
    let kind = udf.result.kind;
    let digits = match kind {
        Kind::Index => 9,
        Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. } => places_of(kind).total() + udf.result.scaling,
        _ => 0,
    };
    Side { value: value_of(kind), src: None, digits }
}
