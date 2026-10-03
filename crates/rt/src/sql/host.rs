//! Host variables as a statement reaches them: the values its inputs send, a row assigned to its
//! INTO list with each indicator set, and the SQLCA filled after it.

use super::run::SqlHost;
use super::{HostType, NULL_WITHOUT_INDICATOR, Outcome, ReadError, SqlError, Value, read, write};
use crate::abend::Abend;
use crate::lir::{HostPlace, Sqlca, SqlcaField};
use crate::store::ProgramFacts;

type R<T> = Result<T, Abend>;

/// SQLWARN0 to SQLWARNA, as the runtime sets them.
pub(super) type Warnings = [bool; 11];
pub(super) const TRUNCATED: usize = 1;
pub(super) const COLUMN_COUNT: usize = 3;

/// A host variable's storage, its type, and its indicator's storage when it has one.
struct Target<'a> {
    offset: usize,
    len: usize,
    ty: &'a HostType,
    indicator: Option<usize>,
}

/// Every place's storage, the whole list located before any is read or written. A host
/// structure's first member is at its start, so its later members take the structure's one locate.
fn targets<'a, 'w, P: Copy, S>(x: &mut impl SqlHost<'w, P, S>, places: &'a [HostPlace<P>]) -> R<Vec<Target<'a>>> {
    let mut out = Vec::with_capacity(places.len());
    let mut located = None;
    for place in places {
        let ((offset, len), indicator) = match (place.member, located) {
            (Some((at, _)), Some(structure)) if at != 0 => structure,
            _ => {
                let loc = x.locate(place.var, false)?;
                let indicator = match place.indicator {
                    Some((p, _)) => Some(x.locate_indicator(p)?.offset),
                    None => None,
                };
                located = Some(((loc.offset, loc.len), indicator));
                ((loc.offset, loc.len), indicator)
            }
        };
        let ty = match &place.ty {
            Ok(ty) => ty,
            Err(abend) => return Err(x.untyped(*abend)),
        };
        let (offset, len) = match place.member {
            Some((at, size)) => (offset + at as usize, size as usize),
            None => (offset, len),
        };
        let indicator = indicator.zip(place.indicator).map(|(at, (_, element))| at + element as usize);
        out.push(Target { offset, len, ty, indicator });
    }
    Ok(out)
}

/// The values the input host variables send: NULL where the indicator is negative. Invalid data
/// is a program check at the first host variable.
pub(super) fn inputs<'w, P: Copy, S>(x: &mut impl SqlHost<'w, P, S>, places: &[HostPlace<P>]) -> R<Result<Vec<Value>, SqlError>> {
    let targets = targets(x, places)?;
    let facts = x.facts();
    let (page, numproc) = (facts.page(), facts.options().numproc);
    let mut values = Vec::with_capacity(targets.len());
    for t in targets {
        let mem = x.mem();
        if let Some(at) = t.indicator
            && i16::from_be_bytes([mem[at], mem[at + 1]]) < 0
        {
            values.push(Value::Null);
            continue;
        }
        match read(&mem[t.offset..t.offset + t.len], t.ty, page, numproc) {
            Ok(v) => values.push(v),
            Err(ReadError::Check(c)) => return Err(Abend::check(c, x.place_pos(places[0].var))),
            Err(ReadError::Sql(e)) => return Ok(Err(e)),
        }
    }
    Ok(Ok(values))
}

/// Assigns a row to the INTO host variables, setting each indicator: -1 for NULL, a cut string's
/// original length, and 0 otherwise.
pub(super) fn assign<'w, P: Copy, S>(x: &mut impl SqlHost<'w, P, S>, into: &[HostPlace<P>], row: &[Value], warnings: &mut Warnings) -> R<Result<(), SqlError>> {
    let targets = targets(x, into)?;
    if targets.len() != row.len() {
        warnings[COLUMN_COUNT] = true;
    }
    let page = x.facts().page();
    let mem = x.mem();
    for (t, value) in targets.iter().zip(row) {
        let indicator = match value {
            Value::Null => match t.indicator {
                Some(_) => -1,
                None => return Ok(Err(NULL_WITHOUT_INDICATOR)),
            },
            value => match write(value, &mut mem[t.offset..t.offset + t.len], t.ty, page) {
                Err(e) => return Ok(Err(e)),
                Ok(written) => {
                    warnings[TRUNCATED] |= written.truncated_from.is_some();
                    written.truncated_from.map_or(0, |n| n.min(i16::MAX as usize) as i16)
                }
            },
        };
        if let Some(at) = t.indicator {
            mem[at..at + 2].copy_from_slice(&indicator.to_be_bytes());
        }
    }
    if let Some(taint) = x.taint() {
        for t in &targets {
            taint.set(t.offset, t.len, true);
            if let Some(at) = t.indicator {
                taint.set(at, 2, true);
            }
        }
    }
    Ok(Ok(()))
}

/// Writes the SQLCA fields the program declares, or its standalone SQLCODE and SQLSTATE. The SQLCA
/// is the program's own declaration: a field that cannot be located or cannot hold its value keeps
/// what it held.
pub(super) fn sqlca<'w, P: Copy, S>(x: &mut impl SqlHost<'w, P, S>, sqlca: &Sqlca<P>, o: &Outcome, warnings: &Warnings) {
    let page = x.facts().page();
    for (field, place, ty) in &sqlca.fields {
        let value = match *field {
            SqlcaField::CaId => Value::Char("SQLCA".into()),
            SqlcaField::CaBc => Value::Int(136),
            SqlcaField::Code => Value::Int(o.sqlcode.into()),
            SqlcaField::ErrMl => Value::Int(o.tokens.len().min(70) as i64),
            SqlcaField::ErrMc => Value::Char(o.tokens.clone()),
            SqlcaField::ErrP => Value::Char(String::new()),
            SqlcaField::State => Value::Char(o.sqlstate.clone()),
            SqlcaField::ErrD(n) => Value::Int(if n == 3 { o.affected } else { 0 }),
            SqlcaField::Warn(n) => Value::Char(if warnings[n as usize] { "W" } else { " " }.into()),
        };
        let Ok(loc) = x.locate(*place, false) else { continue };
        let _ = write(&value, &mut x.mem()[loc.offset..loc.offset + loc.len], ty, page);
        // The database's answer, SQLERRMC's tokens among it, is input.
        if let Some(t) = x.taint() {
            t.set(loc.offset, loc.len, true);
        }
    }
}
