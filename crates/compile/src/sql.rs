//! The host type of a declared item, which EXEC SQL binds by: what `rt::sql` needs from the layout.
//! The SQLCA's fields by name, and what DESCRIBE puts in each SQLNAME, as `rt::lir` has them.

use crate::layout::Layout;
use rt::lir::{Dimension, SqlNames, SqlcaField};
use rt::sql::HostType;
use rt::storage::Kind;
use syntax::Pos;
use syntax::ast::{Expr, Literal, Operand, Ref};
use syntax::sql::Names;
use zarch::hfp::Precision;

/// The host type of layout item `item`, or why it cannot be a host variable.
pub fn host_type(layout: &Layout, item: usize) -> Result<HostType, String> {
    let it = &layout.items[item];
    let name = it.name.as_deref().unwrap_or("FILLER");
    Ok(match it.kind {
        Kind::Binary { scale: 0, signed, .. } => match it.size {
            2 => HostType::SmallInt { signed },
            4 => HostType::Integer { signed },
            8 => HostType::BigInt { signed },
            n => return Err(format!("{name}: a binary item of {n} bytes has no SQL type")),
        },
        Kind::Binary { .. } => return Err(format!("{name}: a binary item with decimal places has no SQL type")),
        Kind::Packed { digits, scale, signed } if digits <= 31 => HostType::Decimal { digits, scale, signed },
        Kind::Zoned { digits, scale, signed, sign } if digits <= 31 => HostType::Zoned { digits, scale, signed, sign },
        Kind::Packed { .. } | Kind::Zoned { .. } => return Err(format!("{name}: more than 31 digits has no SQL type")),
        Kind::Float(Precision::Short) => HostType::Real,
        Kind::Float(Precision::Long) => HostType::Double,
        Kind::Alnum { .. } => HostType::Char(it.size),
        Kind::Dbcs { edit: None, .. } => HostType::Graphic(it.size / 2),
        Kind::Group => structure(layout, item, name)?,
        _ => return Err(format!("{name}: this USAGE or PICTURE has no SQL type")),
    })
}

/// How a multiple-row statement takes layout item `var`, named with or without subscripts, and its
/// indicator: a host-variable array, an item of one OCCURS named without subscripts, with its
/// dimension; None for one host variable; or why it is neither (Db2 13 for z/OS, Host-variable
/// arrays in COBOL).
pub fn host_array(layout: &Layout, var: usize, subscripted: bool, indicator: Option<(usize, bool)>) -> Result<Option<Dimension>, String> {
    let it = &layout.items[var];
    let name = it.name.as_deref().unwrap_or("FILLER");
    match it.dims.len() {
        _ if subscripted => return Ok(None),
        0 => return Ok(None),
        1 => {}
        _ => return Err(format!("{name} is a table of more than one dimension, which no host-variable array is")),
    }
    if matches!(host_type(layout, var), Ok(HostType::Structure(_))) {
        return Err(format!("{name} is a host-structure array, which Db2 for z/OS does not take in COBOL"));
    }
    let (stride, count) = it.dims[0];
    match indicator {
        None => Ok(Some(Dimension { stride, count, indicator_stride: 0 })),
        Some((i, false)) if layout.items[i].dims.len() == 1 => {
            let (indicator_stride, indicators) = layout.items[i].dims[0];
            Ok(Some(Dimension { stride, count: count.min(indicators), indicator_stride }))
        }
        Some(_) => Err(format!("{name}'s indicator is not an indicator array, as a host-variable array's must be")),
    }
}

/// A group is VARCHAR when it is a 49-level length halfword and a 49-level text, and otherwise a
/// host structure of its members.
fn structure(layout: &Layout, item: usize, name: &str) -> Result<HostType, String> {
    let members: Vec<usize> = layout.items[item].children.iter().copied().filter(|&c| layout.items[c].redefines.is_none()).collect();
    if let [length, text] = members[..] {
        let (l, t) = (&layout.items[length], &layout.items[text]);
        if l.level == 49 && t.level == 49 && l.size == 2 && matches!(l.kind, Kind::Binary { scale: 0, .. }) {
            match t.kind {
                Kind::Alnum { .. } => return Ok(HostType::VarChar(t.size)),
                Kind::Dbcs { edit: None, .. } => return Ok(HostType::VarGraphic(t.size / 2)),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    for m in members {
        if layout.items[m].table {
            return Err(format!("{name}: a host structure holding a table"));
        }
        match host_type(layout, m)? {
            HostType::Structure(_) => return Err(format!("{name}: a host structure nested in another")),
            t => out.push((m, t)),
        }
    }
    if out.is_empty() {
        return Err(format!("{name}: a group with no members"));
    }
    Ok(HostType::Structure(out))
}

pub fn sql_names(names: Names) -> SqlNames {
    match names {
        Names::Names => SqlNames::Names,
        Names::Labels => SqlNames::Labels,
        Names::Any => SqlNames::Any,
    }
}

/// The SQLCA's fields by name, in the order they are filled.
pub fn sqlca_fields(pos: Pos) -> Vec<(SqlcaField, Ref)> {
    let named = |name: &str, subscript: Option<u8>| {
        let subscripts = subscript.map(|n| vec![Expr::Operand(Operand::Literal(Literal::Number(n.to_string())))]).unwrap_or_default();
        Ref { name: name.into(), qualifiers: Vec::new(), subscripts, refmod: None, pos }
    };
    let mut fields = vec![
        (SqlcaField::CaId, named("SQLCAID", None)),
        (SqlcaField::CaBc, named("SQLCABC", None)),
        (SqlcaField::Code, named("SQLCODE", None)),
        (SqlcaField::ErrMl, named("SQLERRML", None)),
        (SqlcaField::ErrMc, named("SQLERRMC", None)),
        (SqlcaField::ErrP, named("SQLERRP", None)),
        (SqlcaField::State, named("SQLSTATE", None)),
    ];
    fields.extend((1..=6).map(|n| (SqlcaField::ErrD(n), named("SQLERRD", Some(n)))));
    let warnings = ["SQLWARN0", "SQLWARN1", "SQLWARN2", "SQLWARN3", "SQLWARN4", "SQLWARN5", "SQLWARN6", "SQLWARN7", "SQLWARN8", "SQLWARN9", "SQLWARNA"];
    fields.extend((0..).zip(warnings).map(|(n, name)| (SqlcaField::Warn(n), named(name, None))));
    fields
}
