//! The host type of a declared item, which EXEC SQL binds by: what `rt::sql` needs from the layout.

use crate::layout::Layout;
use rt::sql::HostType;
use rt::storage::Kind;
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
