//! Evaluating a data reference to a `Loc`, over integers the executor has already evaluated: the
//! base, subscripts, OCCURS DEPENDING ON, reference modification and the run-unit bound, each
//! with its SSRANGE check and message (lir.md §5.4 and §5.5).

use crate::abend::{Abend, AbendCode};
use crate::unit::ADDRESS_BASE;
use crate::vocab::Pos;

type R<T> = Result<T, Abend>;

/// An SSRANGE failure: LE's condition, which nothing handles, ends the run with U4038 under the
/// default ABTERMENC(ABEND); the message starts with IBM's message id where one is known (L19).
fn out_of_range(message: String, pos: Pos) -> Abend {
    Abend { code: AbendCode::user(4038), message, pos, file: None }
}

/// A LINKAGE record's address; S0C4 while no argument or SET ADDRESS OF has given it one.
pub fn linkage_base(address: Option<usize>, name: &str, pos: Pos) -> R<usize> {
    address.ok_or_else(|| Abend {
        code: AbendCode::Protection,
        message: format!("{name} is a LINKAGE item with no address: no argument was passed for it, and no SET ADDRESS OF gave it one"),
        pos,
        file: None,
    })
}

/// What subscript `value` adds to the offset; `check` is the occurrence count under SSRANGE.
pub fn subscript(value: i64, stride: u32, check: Option<u32>, name: &str, pos: Pos) -> R<i64> {
    if let Some(count) = check
        && (value < 1 || value > count as i64)
    {
        return Err(out_of_range(format!("IGZ0006S subscript {value} of {name} is out of range 1 to {count} (SSRANGE)"), pos));
    }
    Ok((value - 1) * stride as i64)
}

/// The current count of an OCCURS DEPENDING ON table from its object's value, kept within the
/// declared maximum so that a bad count never reaches past the table's storage.
pub fn occurrences(count: i64, max: u32, check: bool, object: &str, pos: Pos) -> R<u32> {
    if check && !(0..=max as i64).contains(&count) {
        return Err(out_of_range(format!("{object} = {count} is outside the OCCURS DEPENDING ON range 0 to {max} (SSRANGE)"), pos));
    }
    Ok(count.clamp(0, max as i64) as u32)
}

/// A group's length with its table at `current` of `max` occurrences of `element` bytes.
pub fn odo_len(len: i64, max: u32, current: u32, element: u32) -> i64 {
    len - (max - current) as i64 * element as i64
}

/// Reference modification of an item of `len` bytes: what it adds to the offset, and the length.
/// Without a length, it runs to the item's end.
pub fn refmod(len: i64, start: i64, length: Option<i64>, check: bool, name: &str, pos: Pos) -> R<(i64, i64)> {
    let length = length.unwrap_or(len - start + 1);
    if check {
        let id = if start < 1 || start > len {
            "IGZ0072S"
        } else if length < 1 {
            "IGZ0073S"
        } else if start + length - 1 > len {
            "IGZ0074S"
        } else {
            return Ok((start - 1, length));
        };
        return Err(out_of_range(format!("{id} reference modification ({start}:{length}) of {name} is out of range (SSRANGE)"), pos));
    }
    Ok((start - 1, length))
}

/// The offset and length, refused when they reach outside run-unit memory of `mem_len` bytes.
pub fn within(offset: i64, len: i64, mem_len: usize, name: &str, pos: Pos) -> R<(usize, usize)> {
    if offset < 0 || len < 0 || offset + len > mem_len as i64 {
        return Err(Abend::ironwork(format!("{name} reaches outside the run unit's storage"), pos));
    }
    Ok((offset as usize, len as usize))
}

/// Where occurrence `k` of an item lies from its first, counting the last dimension fastest;
/// `dims` are the stride and count of each OCCURS, outermost first.
pub fn occurrence_offset(dims: &[(u32, u32)], mut k: u32) -> usize {
    let mut offset = 0usize;
    for &(stride, count) in dims.iter().rev() {
        offset += (k % count) as usize * stride as usize;
        k /= count;
    }
    offset
}

/// An address back to an offset in run-unit memory of `mem_len` bytes: None for NULL, and an
/// abend for one outside it.
pub fn offset_of(address: u32, mem_len: usize, pos: Pos) -> R<Option<usize>> {
    if address == 0 {
        return Ok(None);
    }
    let offset = address.checked_sub(ADDRESS_BASE).map(|o| o as usize).filter(|&o| o < mem_len);
    offset.map(Some).ok_or_else(|| Abend { code: AbendCode::Protection, message: format!("address {address:08X} is outside the run unit's storage"), pos, file: None })
}
