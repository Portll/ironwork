//! EXEC SQL (lir.md §9.7): the program's statement table, which the SQL runtime reads. WHENEVER is
//! not in it: lowering emits it as `Cond::Sql` branches after the op.

use super::{AbendId, PlaceId, SymId};
use crate::sql::{HostType, fingerprint};
use crate::{codec_enum, codec_struct};

/// One EXEC SQL block, declarative ones included; `Program.sql[k − 1]` has ordinal k.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlEntry<P = PlaceId> {
    pub ordinal: u32,
    /// The command word the call record and the EXEC messages name.
    pub verb: SymId,
    pub statement: SqlStatement<P>,
    /// What a `Database` call receives, `?` for each input; empty for a declaration.
    pub text: SymId,
    pub fingerprint: u32,
    /// Set on the OPEN of a cursor declared WITH HOLD, and on no other entry.
    pub with_hold: bool,
}

/// The typed statement with its host variables resolved and each OPEN given its DECLARE's inputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SqlStatement<P = PlaceId> {
    Query { inputs: Vec<HostPlace<P>>, into: Vec<HostPlace<P>> },
    Change { delete: bool, inputs: Vec<HostPlace<P>>, current_of: Option<SymId> },
    Open { cursor: SymId, inputs: Vec<HostPlace<P>> },
    Fetch { cursor: SymId, into: Vec<HostPlace<P>> },
    Close { cursor: SymId },
    Commit,
    Rollback,
    /// WHENEVER, DECLARE CURSOR, INCLUDE and the other declarations: no op.
    Declaration,
    /// Abends EXEC, naming it, when reached.
    Unsupported(SymId),
}

/// A host variable, or one member of a host structure at `member`'s offset and length. The
/// indicator comes with the offset of this member's element in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostPlace<P = PlaceId> {
    pub var: P,
    pub member: Option<(u32, u32)>,
    /// Never a structure. An item with no SQL type keeps the walker's EXEC abend.
    pub ty: Result<HostType, AbendId>,
    pub indicator: Option<(P, u32)>,
}

/// The SQLCA fields the program declares, or its own SQLCODE and SQLSTATE, in the order filled.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sqlca<P = PlaceId> {
    pub fields: Vec<(SqlcaField, P, HostType)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlcaField {
    CaId,
    CaBc,
    Code,
    ErrMl,
    ErrMc,
    ErrP,
    State,
    /// SQLERRD(n), n from 1 to 6.
    ErrD(u8),
    /// SQLWARN0 to SQLWARNA as 0 to 10.
    Warn(u8),
}

codec_struct!(SqlEntry { ordinal, verb, statement, text, fingerprint, with_hold });
codec_enum!(SqlStatement {
    Query { inputs, into } = 0,
    Change { delete, inputs, current_of } = 1,
    Open { cursor, inputs } = 2,
    Fetch { cursor, into } = 3,
    Close { cursor } = 4,
    Commit = 5,
    Rollback = 6,
    Declaration = 7,
    Unsupported(what) = 8,
});
codec_struct!(HostPlace { var, member, ty, indicator } check host_place_valid);
codec_struct!(Sqlca { fields } check sqlca_valid);
codec_enum!(SqlcaField {
    CaId = 0,
    CaBc = 1,
    Code = 2,
    ErrMl = 3,
    ErrMc = 4,
    ErrP = 5,
    State = 6,
    ErrD(n) = 7,
    Warn(n) = 8,
});

/// `crate::sql::read` and `write` take a structure's members one by one, never the structure.
fn not_a_structure(ty: &HostType) -> Result<(), String> {
    match ty {
        HostType::Structure(_) => Err("a host structure where lowering gives its members".into()),
        _ => Ok(()),
    }
}

fn host_place_valid(place: &HostPlace) -> Result<(), String> {
    place.ty.as_ref().map_or(Ok(()), not_a_structure)
}

fn sqlca_valid(sqlca: &Sqlca) -> Result<(), String> {
    for (field, _, ty) in &sqlca.fields {
        match *field {
            SqlcaField::ErrD(n) if !(1..=6).contains(&n) => return Err(format!("SQLERRD({n}) is not an SQLCA field")),
            SqlcaField::Warn(n) if n > 10 => return Err(format!("SQLWARN{n} is not an SQLCA field")),
            _ => not_a_structure(ty)?,
        }
    }
    Ok(())
}

/// load-module.md §7: ordinals run from 1, each fingerprint is its text's, and only the OPEN of a
/// cursor declared WITH HOLD says so.
pub(super) fn table_valid(table: &[SqlEntry], symbols: &[String]) -> Result<(), String> {
    let symbol = |id: SymId| symbols.get(id as usize).ok_or_else(|| format!("symbol {id} of a table of {}", symbols.len()));
    for (k, entry) in (1u32..).zip(table) {
        if entry.ordinal != k {
            return Err(format!("SQL entry {k} has ordinal {}", entry.ordinal));
        }
        let text = symbol(entry.text)?;
        if entry.fingerprint != fingerprint(text) {
            return Err(format!("SQL entry {k} has fingerprint {:08X}, not its text's {:08X}", entry.fingerprint, fingerprint(text)));
        }
        let held = match &entry.statement {
            SqlStatement::Open { cursor, .. } => text.starts_with(&format!("DECLARE {} CURSOR WITH HOLD FOR ", symbol(*cursor)?)),
            _ => false,
        };
        if entry.with_hold != held {
            return Err(format!("SQL entry {k} has WITH HOLD {} for its text", if entry.with_hold { "set" } else { "clear" }));
        }
    }
    Ok(())
}
