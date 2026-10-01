//! A class definition (lir.md §9.8): its FACTORY and OBJECT data and each method, compiled as
//! `load_class` compiles them when a run first reaches the class, and each lowered as a program.

use super::{Lower, LowerError, R, lower};
use numeric::options::{FastsrtAdvPrint, Warnings};
use numeric::{Options, SortKeys, TruncCheck};
use rt::lir;

impl Lower<'_> {
    /// The class this program defines, or None when it defines none.
    pub(super) fn class_definition(&mut self) -> R<Option<Box<lir::Class>>> {
        let program = self.program;
        if program.oo.as_deref().and_then(|o| o.class()).is_none() {
            return Ok(None);
        }
        let (code, _) = crate::oo::class_code(program, &flags(&self.c.options), self.c.when_compiled).map_err(|errors| {
            let first = syntax::most_severe(&errors).map(|e| e.message.clone()).unwrap_or_default();
            LowerError::Invalid(format!("the class definition does not compile again: {first}"))
        })?;
        let options = self.c.options;
        let same = |compiled: &crate::Compiled| if compiled.options == options { Ok(()) } else { Err(LowerError::Invalid("a method or class data compiled with other options than its class".into())) };
        let part = |p: Option<&crate::oo::Part>| -> R<Option<lir::ClassPart>> {
            let Some(p) = p else { return Ok(None) };
            same(&p.data)?;
            Ok(Some(lir::ClassPart { data: lower(&p.data)?, records: p.records.clone() }))
        };
        let (factory, object) = (part(code.factory.as_ref())?, part(code.object.as_ref())?);
        let mut methods = Vec::with_capacity(code.methods.len());
        for m in &code.methods {
            same(&m.code)?;
            let own_records = u16::try_from(m.own_records).map_err(|_| LowerError::Exceeds("LINKAGE records", syntax::Pos::default()))?;
            methods.push(lir::Method {
                name: self.sym(&m.name),
                factory: m.factory,
                params: m.params.iter().map(|p| self.sym(p)).collect(),
                returns: m.returns.as_deref().map(|r| self.sym(r)),
                own_records,
                code: lower(&m.code)?,
            });
        }
        let external = crate::oo::defined_class(program).unwrap_or_default();
        Ok(Some(Box::new(lir::Class { external: self.sym(&external), parent: self.sym(&code.parent), factory, object, methods })))
    }
}

/// The compiler's flags that leave these options: cards set everything else, and a flag, applied
/// after the cards, sets only the field it names.
fn flags(options: &Options) -> Vec<String> {
    let mut flags = Vec::new();
    if options.trunc_check == TruncCheck::Silent {
        flags.push("-silent");
    }
    if options.sort_keys == SortKeys::Strict {
        flags.push("-strict-sort-keys");
    }
    flags.push(match options.fastsrt_adv_print {
        FastsrtAdvPrint::Exclude => "--fastsrt-adv-print=exclude",
        FastsrtAdvPrint::Include => "--fastsrt-adv-print=include",
    });
    if options.warnings == Warnings::Block {
        flags.push("-warnings-block");
    }
    if options.debug {
        flags.push("-debug");
    }
    flags.into_iter().map(String::from).collect()
}
