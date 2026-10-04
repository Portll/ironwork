//! EXEC SQL (lir.md §9.7): the program's statement table, which the SQL runtime reads. WHENEVER is
//! not in it: lowering emits it as `Cond::Sql` branches after the op.

use super::{AbendId, PlaceId, SymId};
use crate::sql::{HostType, fingerprint};
use crate::{codec_enum, codec_struct};

/// One EXEC SQL block, declarative ones included; `Program.sql[k − 1]` has ordinal k. `P` and `S`
/// are the executor's handles to a data item and to text: the LIR's ids by default, the walker's
/// own references in the interpreter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlEntry<P = PlaceId, S = SymId> {
    pub ordinal: u32,
    /// The command word the call record and the EXEC messages name.
    pub verb: S,
    pub statement: SqlStatement<P, S>,
    /// What a `Database` call receives, `?` for each input; empty for a declaration. A dynamic
    /// statement's call sends its statement string instead, and this names the statement.
    pub text: S,
    pub fingerprint: u32,
    /// Set on the OPEN of a cursor declared WITH HOLD, and on no other entry.
    pub with_hold: bool,
}

/// The typed statement with its host variables resolved and each OPEN given its DECLARE's inputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SqlStatement<P = PlaceId, S = SymId> {
    Query { inputs: Vec<HostPlace<P>>, into: Vec<HostPlace<P>> },
    Change { delete: bool, inputs: Vec<HostPlace<P>>, current_of: Option<S> },
    Open { cursor: S, inputs: Vec<HostPlace<P>> },
    Fetch { cursor: S, into: Vec<HostPlace<P>> },
    Close { cursor: S },
    Commit,
    Rollback,
    /// PREPARE of the statement `name` from the statement string in `source`, which must be one
    /// varying-length character or graphic string.
    Prepare { name: S, source: Vec<HostPlace<P>> },
    ExecuteImmediate { source: Vec<HostPlace<P>> },
    /// EXECUTE of a prepared statement, `inputs` replacing its parameter markers.
    Execute { name: S, inputs: Vec<HostPlace<P>> },
    /// OPEN of a cursor declared for the prepared statement `statement`.
    OpenPrepared { cursor: S, statement: S, inputs: Vec<HostPlace<P>> },
    /// DESCRIBE [OUTPUT] of a prepared statement into the SQLDA at `descriptor`.
    Describe { name: S, descriptor: P, names: SqlNames },
    /// PREPARE ... INTO: PREPARE, then DESCRIBE of the statement it made.
    PrepareInto { name: S, source: Vec<HostPlace<P>>, descriptor: P, names: SqlNames },
    ExecuteDescriptor { name: S, descriptor: P },
    OpenDescriptor { cursor: S, statement: S, descriptor: P },
    FetchDescriptor { cursor: S, descriptor: P },
    /// WHENEVER, DECLARE CURSOR, INCLUDE and the other declarations: no op.
    Declaration,
    /// Abends EXEC, naming it, when reached.
    Unsupported(S),
    /// CONNECT or SET CONNECTION: the host variable naming the location, if any, goes to the
    /// input trace, then the statement abends EXEC as an unsupported one does.
    Connect { what: S, location: Vec<HostPlace<P>> },
}

/// What DESCRIBE puts in each SQLNAME: the column's name, its label, or its label and else its name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlNames {
    Names,
    Labels,
    Any,
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
    Connect { what, location } = 9,
    Prepare { name, source } = 10,
    ExecuteImmediate { source } = 11,
    Execute { name, inputs } = 12,
    OpenPrepared { cursor, statement, inputs } = 13,
    Describe { name, descriptor, names } = 14,
    PrepareInto { name, source, descriptor, names } = 15,
    ExecuteDescriptor { name, descriptor } = 16,
    OpenDescriptor { cursor, statement, descriptor } = 17,
    FetchDescriptor { cursor, descriptor } = 18,
});
codec_enum!(SqlNames {
    Names = 0,
    Labels = 1,
    Any = 2,
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
            SqlStatement::Open { cursor, .. } | SqlStatement::OpenPrepared { cursor, .. } | SqlStatement::OpenDescriptor { cursor, .. } => text.starts_with(&format!("DECLARE {} CURSOR WITH HOLD FOR ", symbol(*cursor)?)),
            _ => false,
        };
        if entry.with_hold != held {
            return Err(format!("SQL entry {k} has WITH HOLD {} for its text", if entry.with_hold { "set" } else { "clear" }));
        }
    }
    Ok(())
}
