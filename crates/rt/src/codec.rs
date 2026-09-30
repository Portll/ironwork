//! A numeric item's storage read as generated code reads it. The interpreter and the SQL runtime
//! share it, so a host variable sends exactly the value a COMPUTE would read.

use numeric::Numproc;
use crate::vocab::{SignClause, SignPosition};
use zarch::check::ProgramCheck;
use zarch::decimal::{self, Decimal};

/// A packed field. Under NUMPROC(NOPFD) an unsigned field's sign nibble is forced to F first.
pub fn packed(bytes: &[u8], signed: bool, numproc: Numproc) -> Result<Decimal, ProgramCheck> {
    let mut p = bytes.to_vec();
    if !signed && numproc == Numproc::Nopfd {
        *p.last_mut().unwrap() |= 0x0F;
    }
    decimal::decode(&p)
}

/// A zoned field, entering through PACK, which keeps only the sign's zone.
pub fn zoned(bytes: &[u8], signed: bool, sign: Option<SignClause>, numproc: Numproc) -> Result<Decimal, ProgramCheck> {
    let mut zoned = bytes.to_vec();
    let mut separate_negative = None;
    match sign {
        Some(SignClause { separate: true, position }) => {
            let s = if position == SignPosition::Leading { zoned.remove(0) } else { zoned.pop().unwrap() };
            separate_negative = Some(match s {
                0x60 => true,
                0x4E => false,
                _ => return Err(ProgramCheck::Data),
            });
            *zoned.last_mut().unwrap() |= 0xF0;
        }
        Some(SignClause { separate: false, position: SignPosition::Leading }) => {
            let zone = zoned[0] & 0xF0;
            zoned[0] |= 0xF0;
            let last = zoned.len() - 1;
            zoned[last] = zone | (zoned[last] & 0x0F);
        }
        _ => {}
    }
    let p = numeric::zoned::pack(&zoned)?;
    let value = packed(&p, signed, numproc)?;
    Ok(match separate_negative {
        Some(negative) => Decimal { negative, ..value },
        None => value,
    })
}
