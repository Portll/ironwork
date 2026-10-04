//! Host variables as a statement reaches them: the values its inputs send, a row assigned to its
//! INTO list with each indicator set, and the SQLCA filled after it.

use super::run::SqlHost;
use super::{HostType, NULL_WITHOUT_INDICATOR, Outcome, ReadError, SqlError, Value, write};
use crate::abend::Abend;
use crate::lir::{Dimension, HostArray, HostPlace, Sqlca, SqlcaField};
use crate::store::ProgramFacts;
use crate::vocab::Pos;
use zarch::ebcdic::CodePage;

type R<T> = Result<T, Abend>;

/// SQLWARN0 to SQLWARNA, as the runtime sets them.
pub(super) type Warnings = [bool; 11];
pub(super) const TRUNCATED: usize = 1;
pub(super) const COLUMN_COUNT: usize = 3;
pub(super) const RESULT_SETS: usize = 9;

/// A host variable's storage, its type, and its indicator's storage when it has one.
pub(super) struct Target<'a> {
    pub offset: usize,
    pub len: usize,
    pub ty: &'a HostType,
    pub indicator: Option<usize>,
}

impl Target<'_> {
    /// Where element `k` of an array is, and its indicator; a host variable's own place.
    fn element(&self, k: usize, array: Option<Dimension>) -> (usize, Option<usize>) {
        match array {
            Some(d) => (self.offset + k * d.stride as usize, self.indicator.map(|at| at + k * d.indicator_stride as usize)),
            None => (self.offset, self.indicator),
        }
    }

    /// The value at `offset`: NULL where its indicator is negative.
    fn value(&self, mem: &[u8], (offset, indicator): (usize, Option<usize>), page: &CodePage, numproc: numeric::Numproc) -> Result<Value, ReadError> {
        if let Some(at) = indicator
            && i16::from_be_bytes([mem[at], mem[at + 1]]) < 0
        {
            return Ok(Value::Null);
        }
        super::read(&mem[offset..offset + self.len], self.ty, page, numproc)
    }

    /// Writes `value` at `offset` and sets the indicator: -1 for NULL, a cut string's original
    /// length, and 0 otherwise.
    fn write(&self, mem: &mut [u8], (offset, indicator): (usize, Option<usize>), value: &Value, page: &CodePage, warnings: &mut Warnings) -> Result<(), SqlError> {
        let set = match value {
            Value::Null => match indicator {
                Some(_) => -1,
                None => return Err(NULL_WITHOUT_INDICATOR),
            },
            value => {
                let written = write(value, &mut mem[offset..offset + self.len], self.ty, page)?;
                warnings[TRUNCATED] |= written.truncated_from.is_some();
                written.truncated_from.map_or(0, |n| n.min(i16::MAX as usize) as i16)
            }
        };
        if let Some(at) = indicator {
            mem[at..at + 2].copy_from_slice(&set.to_be_bytes());
        }
        Ok(())
    }
}

/// Every place's storage, the whole list located before any is read or written. A host
/// structure's first member is at its start, so its later members take the structure's one locate.
/// `first` locates a table named without subscripts at its first element, as a multiple-row
/// statement's arrays are.
fn targets<'a, 'w, P: Copy + 'a, S>(x: &mut impl SqlHost<'w, P, S>, places: impl IntoIterator<Item = &'a HostPlace<P>>, first: bool) -> R<Vec<Target<'a>>> {
    let mut out = Vec::new();
    let mut located = None;
    for place in places {
        let ((offset, len), indicator) = match (place.member, located) {
            (Some((at, _)), Some(structure)) if at != 0 => structure,
            _ => {
                let loc = if first { x.locate_first(place.var)? } else { x.locate(place.var, false)? };
                let indicator = match place.indicator {
                    Some((p, _)) => Some(x.locate_first(p)?.offset),
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
    let targets = targets(x, places, false)?;
    let at = places.first().map(|p| x.place_pos(p.var)).unwrap_or_default();
    read_targets(x, &targets, at)
}

/// The values `targets` send, a program check at `at` for invalid data.
pub(super) fn read_targets<'w, P: Copy, S>(x: &mut impl SqlHost<'w, P, S>, targets: &[Target], at: Pos) -> R<Result<Vec<Value>, SqlError>> {
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
        match super::read(&mem[t.offset..t.offset + t.len], t.ty, page, numproc) {
            Ok(v) => values.push(v),
            Err(ReadError::Check(c)) => return Err(Abend::check(c, at)),
            Err(ReadError::Sql(e)) => return Ok(Err(e)),
        }
    }
    Ok(Ok(values))
}

/// The values the host variables send, each read for the input trace as a sink's operand is.
pub(super) fn traced<'w, P: Copy, S>(x: &mut impl SqlHost<'w, P, S>, places: &[HostPlace<P>]) -> R<Result<Vec<Value>, SqlError>> {
    for t in targets(x, places, false)? {
        if let Some(taint) = x.taint() {
            taint.read(t.offset, t.len);
        }
    }
    inputs(x, places)
}

/// Assigns a row to the INTO host variables, setting each indicator: -1 for NULL, a cut string's
/// original length, and 0 otherwise.
pub(super) fn assign<'w, P: Copy, S>(x: &mut impl SqlHost<'w, P, S>, into: &[HostPlace<P>], row: &[Value], warnings: &mut Warnings) -> R<Result<(), SqlError>> {
    let targets = targets(x, into, false)?;
    assign_targets(x, &targets, row, warnings)
}

/// A multiple-row statement's host variables, located, with their arrays' dimensions.
fn array_targets<'a, 'w, P: Copy + 'a, S>(x: &mut impl SqlHost<'w, P, S>, arrays: &'a [HostArray<P>]) -> R<Vec<(Target<'a>, Option<Dimension>)>> {
    let targets = targets(x, arrays.iter().map(|a| &a.place), true)?;
    Ok(targets.into_iter().zip(arrays.iter().map(|a| a.array)).collect())
}

/// The values each of `rows` rows of a multiple-row INSERT sends: row k takes element k of each
/// array, and a host variable's value on every row.
pub(super) fn input_rows<'w, P: Copy, S>(x: &mut impl SqlHost<'w, P, S>, arrays: &[HostArray<P>], rows: usize) -> R<Result<Vec<Vec<Value>>, SqlError>> {
    let targets = array_targets(x, arrays)?;
    let facts = x.facts();
    let (page, numproc) = (facts.page(), facts.options().numproc);
    let mut out = Vec::with_capacity(rows);
    for k in 0..rows {
        let mut row = Vec::with_capacity(targets.len());
        for (t, array) in &targets {
            match t.value(x.mem(), t.element(k, *array), page, numproc) {
                Ok(v) => row.push(v),
                Err(ReadError::Check(c)) => return Err(Abend::check(c, x.place_pos(arrays[0].place.var))),
                Err(ReadError::Sql(e)) => return Ok(Err(e)),
            }
        }
        out.push(row);
    }
    Ok(Ok(out))
}

/// Assigns row k of a rowset to element k of each array until one cannot be assigned: the rows
/// assigned whole, and why the next was not. Values already assigned stay (Db2 13 SQL, FETCH).
pub(super) fn assign_rows<'w, P: Copy, S>(x: &mut impl SqlHost<'w, P, S>, into: &[HostArray<P>], rows: &[Vec<Value>], warnings: &mut Warnings) -> R<(usize, Option<SqlError>)> {
    let targets = array_targets(x, into)?;
    if rows.first().is_some_and(|row| targets.len() < row.len()) {
        warnings[COLUMN_COUNT] = true;
    }
    let page = x.facts().page();
    for (k, row) in rows.iter().enumerate() {
        for ((t, array), value) in targets.iter().zip(row) {
            let at = t.element(k, *array);
            if let Err(e) = t.write(x.mem(), at, value, page, warnings) {
                return Ok((k, Some(e)));
            }
            taint_written(x, t, at);
        }
    }
    Ok((rows.len(), None))
}

/// Assigns each CALL argument the value the procedure returns for it, and leaves one it does not.
pub(super) fn assign_returned<'w, P: Copy, S>(x: &mut impl SqlHost<'w, P, S>, args: &[HostPlace<P>], returned: &[Option<Value>], warnings: &mut Warnings) -> R<Result<(), SqlError>> {
    let targets = targets(x, args, false)?;
    let page = x.facts().page();
    for (t, value) in targets.iter().zip(returned) {
        let Some(value) = value else { continue };
        let at = (t.offset, t.indicator);
        if let Err(e) = t.write(x.mem(), at, value, page, warnings) {
            return Ok(Err(e));
        }
        taint_written(x, t, at);
    }
    Ok(Ok(()))
}

/// The database's answer is input.
fn taint_written<'w, P: Copy, S>(x: &mut impl SqlHost<'w, P, S>, t: &Target, (offset, indicator): (usize, Option<usize>)) {
    if let Some(taint) = x.taint() {
        taint.set(offset, t.len, true);
        if let Some(at) = indicator {
            taint.set(at, 2, true);
        }
    }
}

pub(super) fn assign_targets<'w, P: Copy, S>(x: &mut impl SqlHost<'w, P, S>, targets: &[Target], row: &[Value], warnings: &mut Warnings) -> R<Result<(), SqlError>> {
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
        for t in targets {
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
            SqlcaField::Warn(n) => Value::Char(match warnings[n as usize] {
                false => " ",
                true if n as usize == RESULT_SETS => "Z",
                true => "W",
            }.into()),
        };
        let Ok(loc) = x.locate(*place, false) else { continue };
        let _ = write(&value, &mut x.mem()[loc.offset..loc.offset + loc.len], ty, page);
        // The database's answer, SQLERRMC's tokens among it, is input.
        if let Some(t) = x.taint() {
            t.set(loc.offset, loc.len, true);
        }
    }
}
