//! User-defined functions (FUNCTION-ID): the rules a definition or prototype keeps, the
//! prototypes an invocation is checked against (Language Reference, the USING phrase's conformance
//! of parameters for user-defined functions), and the formal parameters and result the
//! interpreter invokes one with.

use crate::corresponding::is_alphabetic;
use crate::layout::{self, Kind, Layout, Resolved};
use crate::picture::Notation;
use numeric::Qualify;
use rt::picture::Sym;
use syntax::ast::*;
use syntax::{Error, Pos};

/// A user-defined function as an invocation sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Udf {
    pub name: String,
    pub external: String,
    pub params: Vec<Formal>,
    pub result: Formal,
    pub pos: Pos,
}

/// A formal parameter or the RETURNING item: what an argument conforms to, and the shape of the
/// temporary that holds a literal or expression argument.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Formal {
    pub name: String,
    pub by_value: bool,
    pub kind: Kind,
    pub size: u32,
    pub scaling: u32,
    pub alphabetic: bool,
    /// An edited item's PICTURE symbols and the currency string its currency symbol stands for.
    pub edit: Option<(Vec<Sym>, String)>,
    pub decimal_point_comma: bool,
}

impl Udf {
    /// Whether the function is alphanumeric or national, so its value can be reference-modified.
    pub fn character_valued(&self) -> bool {
        matches!(self.result.kind, Kind::Group | Kind::Alnum { .. } | Kind::AlnumEdited { .. } | Kind::National)
    }
}

/// The function as its prototype's LINKAGE SECTION and PROCEDURE DIVISION header describe it.
pub fn signature(p: &Prototype, qualify: Qualify) -> Result<Udf, Error> {
    let layout = layout::build(&[], &[], &[], &p.linkage, &[], Notation::of(&p.environment), qualify, None)?;
    let alphabetic: Vec<Pos> = p.linkage.iter().filter(|e| e.picture.as_deref().is_some_and(is_alphabetic)).map(|e| e.pos).collect();
    let formal = |name: &str, by_value: bool| -> Result<Formal, Error> {
        let root = layout.linkage_roots.iter().copied().find(|&i| layout.items[i].name.as_deref() == Some(name));
        let item = &layout.items[root.ok_or_else(|| syntax::messages::IWC0016.at(p.pos, format!("FUNCTION-ID {}: {name} is not an 01 or 77 item of the LINKAGE SECTION", p.name)))?];
        let edit = match item.kind {
            Kind::NumericEdited { edit, .. } | Kind::AlnumEdited { edit } => Some((layout.edits[edit as usize].clone(), layout.currencies[edit as usize].clone())),
            _ => None,
        };
        Ok(Formal {
            name: name.to_owned(),
            by_value,
            kind: item.kind,
            size: item.size,
            scaling: item.scaling,
            alphabetic: alphabetic.contains(&item.pos),
            edit,
            decimal_point_comma: p.environment.decimal_point_comma,
        })
    };
    let returning = p.returning.as_deref().ok_or_else(|| syntax::messages::IWC0017.at(p.pos, format!("FUNCTION-ID {}: a user-defined function needs PROCEDURE DIVISION RETURNING", p.name)))?;
    let params = p.using.iter().map(|u| formal(&u.name, u.by_value)).collect::<Result<_, _>>()?;
    Ok(Udf { name: p.name.clone(), external: p.external.clone(), params, result: formal(returning, false)?, pos: p.pos })
}

/// The functions a program may invoke, each laid out once. A prototype that does not lay out is
/// left out here: compiling it reports why, and an invocation of it is then unknown. One named as an
/// intrinsic function is left out unless the REPOSITORY paragraph names it, as the name then
/// invokes the intrinsic function (assumption C271).
pub fn functions(program: &Program, qualify: Qualify, errors: &mut Vec<Error>) -> Vec<Udf> {
    let own = program.function.as_ref().map(|f| f.pos);
    let mut out = Vec::new();
    for p in &program.prototypes {
        match signature(p, qualify) {
            Ok(udf) => out.push(udf),
            Err(e) if Some(p.pos) == own => errors.push(e),
            Err(_) => {}
        }
    }
    if let Some(own) = own {
        definition_rules(program, &out, own, errors);
    }
    facilities(program, errors);
    out.retain(|u| !program.intrinsic(&u.name));
    out
}

/// SQL and CICS cannot be used with user-defined functions (Programming Guide, Structuring
/// user-defined functions): not in a function, nor in a program its source defines or prototypes
/// one before.
fn facilities(program: &Program, errors: &mut Vec<Error>) {
    if program.function.is_none() && program.prototypes.is_empty() {
        return;
    }
    let mut all = Vec::new();
    program.paragraphs.iter().for_each(|p| crate::inner_statements(&p.statements, &mut all));
    let statements = all.into_iter().filter_map(|s| match s {
        Stmt::Exec(b) => Some(b.as_ref()),
        _ => None,
    });
    if let Some(b) = program.exec_declarations.iter().chain(statements).find(|b| matches!(b.kind, ExecKind::Sql | ExecKind::Cics)) {
        let kind = if b.kind == ExecKind::Sql { "SQL" } else { "CICS" };
        errors.push(syntax::messages::IWC0018.at(b.pos, format!("EXEC {kind}: SQL and CICS cannot be used with user-defined functions, so neither in one nor in a program after one in its source (assumption C273)")));
    }
}

/// What a function definition or prototype keeps of the rules: BY VALUE parameters of the kinds
/// that can be passed by value, and agreement with every prototype of its name before it.
fn definition_rules(program: &Program, functions: &[Udf], own: Pos, errors: &mut Vec<Error>) {
    let Some(this) = functions.iter().find(|u| u.pos == own) else { return };
    for f in this.params.iter().filter(|f| f.by_value) {
        let one_character = matches!(f.kind, Kind::Alnum { .. }) && f.size == 1 || f.kind == Kind::National && f.size == 2;
        if !(one_character || matches!(f.kind, Kind::Binary { .. } | Kind::Float(_) | Kind::Pointer | Kind::ProgramPointer)) {
            errors.push(syntax::messages::IWC0019.at(own, format!("PROCEDURE DIVISION USING BY VALUE {}: a function's BY VALUE parameter is binary, floating-point, a pointer, or one alphanumeric or national character", f.name)));
        }
    }
    for other in functions.iter().filter(|u| u.name == this.name && u.pos != own) {
        if let Some(why) = disagreement(this, other) {
            errors.push(syntax::messages::IWC0020.at(own, format!("FUNCTION-ID {}: {why} from the prototype at line {}", program.id, other.pos.line)));
        }
    }
}

fn disagreement(a: &Udf, b: &Udf) -> Option<String> {
    if a.external != b.external {
        return Some(format!("the external name {} differs", a.external));
    }
    if a.params.len() != b.params.len() {
        return Some(format!("{} parameters differ in number", a.params.len()));
    }
    let same = |x: &Formal, y: &Formal| (x.by_value, x.kind, x.size, x.scaling, x.alphabetic, &x.edit) == (y.by_value, y.kind, y.size, y.scaling, y.alphabetic, &y.edit);
    if let Some(k) = (0..a.params.len()).find(|&k| !same(&a.params[k], &b.params[k])) {
        return Some(format!("parameter {} ({}) differs", k + 1, a.params[k].name));
    }
    (!same(&a.result, &b.result)).then(|| format!("the RETURNING item {} differs", a.result.name))
}

/// Why argument `item` of the caller's `layout` does not conform to `formal`. A literal or an
/// expression has no description to conform: it is moved or computed into a temporary of the
/// formal parameter's (assumption C272).
pub fn conformance(layout: &Layout, item: usize, alphabetic: bool, decimal_point_comma: bool, formal: &Formal) -> Option<String> {
    let it = &layout.items[item];
    if formal.by_value {
        if it.kind == Kind::Group {
            return Some("a group is never passed BY VALUE".into());
        }
        if arithmetic(formal.kind) && !arithmetic(it.kind) {
            return Some(format!("BY VALUE {} is numeric, and takes an argument COMPUTE could send it", formal.name));
        }
        return None;
    }
    if it.kind == Kind::Group || formal.kind == Kind::Group {
        return (it.size < formal.size).then(|| format!("{} bytes cannot be passed BY REFERENCE to the {}-byte {}", it.size, formal.size, formal.name));
    }
    let edit = match it.kind {
        Kind::NumericEdited { edit, .. } | Kind::AlnumEdited { edit } => Some((layout.edits[edit as usize].clone(), layout.currencies[edit as usize].clone())),
        _ => None,
    };
    let conforms = match (it.kind, formal.kind) {
        (Kind::Alnum { justified: a }, Kind::Alnum { justified: b }) => a == b && alphabetic == formal.alphabetic && it.size >= formal.size,
        (Kind::National, Kind::National) => it.size >= formal.size,
        (a, b) => a == b && it.size == formal.size && it.scaling == formal.scaling && edit == formal.edit && (edit.is_none() || decimal_point_comma == formal.decimal_point_comma),
    };
    (!conforms).then(|| format!("{} is passed BY REFERENCE, so its PICTURE, USAGE, SIGN, JUSTIFIED and BLANK WHEN ZERO must be the argument's", formal.name))
}

/// The invocation of `udf` at `f`: its argument count, and each data-item argument against its
/// formal parameter.
pub fn check_invocation(udf: &Udf, f: &FunctionCall, layout: &Layout, alphabetic: &[Pos], decimal_point_comma: bool, errors: &mut Vec<Error>) {
    let name = &f.name;
    if f.args.len() != udf.params.len() {
        errors.push(syntax::messages::IWC0021.at(f.pos, format!("FUNCTION {name} takes {} arguments, not {}", udf.params.len(), f.args.len())));
        return;
    }
    if f.modifier.is_some() || !f.all_subscripts.is_empty() {
        errors.push(syntax::messages::IWC0022.at(f.pos, format!("FUNCTION {name}: a user-defined function's argument is an identifier, a literal or an arithmetic expression")));
    }
    if f.refmod.is_some() && !udf.character_valued() {
        errors.push(syntax::messages::IWC0023.at(f.pos, format!("FUNCTION {name}: only an alphanumeric or national function's value can be reference-modified")));
    }
    for (k, (arg, formal)) in f.args.iter().zip(&udf.params).enumerate() {
        let r = match arg {
            Expr::Operand(Operand::Literal(Literal::Figurative(_) | Literal::All(_))) => {
                errors.push(syntax::messages::IWC0024.at(f.pos, format!("FUNCTION {name} argument {}: a function's argument is not a figurative constant", k + 1)));
                continue;
            }
            Expr::Operand(Operand::Literal(Literal::Alnum(_) | Literal::Hex(_) | Literal::National(_))) if arithmetic(formal.kind) => {
                errors.push(syntax::messages::IWC0025.at(f.pos, format!("FUNCTION {name} argument {}: {} is numeric, and takes an argument COMPUTE could send it (assumption C272)", k + 1, formal.name)));
                continue;
            }
            Expr::Operand(Operand::Ref(r)) if r.refmod.is_none() => r,
            _ => continue,
        };
        if let Ok(Resolved::Item(i)) = layout.resolve(&r.name, &r.qualifiers, r.pos)
            && let Some(why) = conformance(layout, i, alphabetic.contains(&layout.items[i].pos), decimal_point_comma, formal)
        {
            errors.push(syntax::messages::IWC0026.at(r.pos, format!("FUNCTION {name} argument {} ({}): {why}", k + 1, r.name)));
        }
    }
}

/// A numeric item an arithmetic statement can send or receive: not an index or a numeric-edited
/// item.
fn arithmetic(kind: Kind) -> bool {
    matches!(kind, Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. } | Kind::Float(_))
}
