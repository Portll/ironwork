//! A numeric item's storage read as generated code reads it. The interpreter and the SQL runtime
//! share it, so a host variable sends exactly the value a COMPUTE would read.

use numeric::Numproc;
use crate::vocab::{SignClause, SignPosition};
use zarch::check::ProgramCheck;
use zarch::decimal::{self, Decimal};

/// The longest packed field DECIMAL instructions take, and the most digits a PACK of one gives.
const PACKED_MAX: usize = 16;
const ZONED_MAX: usize = 2 * PACKED_MAX - 1;
/// The most digits, each at most 15 where the data is bad, whose value fits a `u64`.
const U64_DIGITS: usize = 18;

/// A packed field. Under NUMPROC(NOPFD) an unsigned field's sign nibble is forced to F first.
#[inline]
pub fn packed(bytes: &[u8], signed: bool, numproc: Numproc) -> Result<Decimal, ProgramCheck> {
    if signed || numproc != Numproc::Nopfd {
        return decimal::decode(bytes);
    }
    let mut held = [0u8; PACKED_MAX];
    let mut grown = Vec::new();
    let p = match held.get_mut(..bytes.len()) {
        Some(p) => p,
        None => {
            grown.extend_from_slice(bytes);
            &mut grown[..]
        }
    };
    p.copy_from_slice(bytes);
    *p.last_mut().unwrap() |= 0x0F;
    decimal::decode(p)
}

/// A zoned field, entering through PACK, which keeps only the sign's zone.
#[inline]
pub fn zoned(bytes: &[u8], signed: bool, sign: Option<SignClause>, numproc: Numproc) -> Result<Decimal, ProgramCheck> {
    if matches!(sign, None | Some(SignClause { separate: false, position: SignPosition::Trailing })) && (1..=ZONED_MAX).contains(&bytes.len()) {
        return packed_zoned(bytes, signed, numproc);
    }
    let mut held = [0u8; ZONED_MAX + 1];
    let mut grown = Vec::new();
    let mut zoned = match held.get_mut(..bytes.len()) {
        Some(z) => z,
        None => {
            grown.extend_from_slice(bytes);
            &mut grown[..]
        }
    };
    zoned.copy_from_slice(bytes);
    let mut separate_negative = None;
    match sign {
        Some(SignClause { separate: true, position }) => {
            let whole = std::mem::take(&mut zoned);
            let (s, rest) = if position == SignPosition::Leading { whole.split_first_mut().unwrap() } else { whole.split_last_mut().unwrap() };
            separate_negative = Some(match *s {
                0x60 => true,
                0x4E => false,
                _ => return Err(ProgramCheck::Data),
            });
            zoned = rest;
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
    let value = if (1..=ZONED_MAX).contains(&zoned.len()) { packed_zoned(zoned, signed, numproc)? } else { packed(&numeric::zoned::pack(zoned)?, signed, numproc)? };
    Ok(match separate_negative {
        Some(negative) => Decimal { negative, ..value },
        None => value,
    })
}

/// What `packed` gives for PACK of 1 to 31 zoned bytes: their digit nibbles, after a zero nibble
/// when there are evenly many, then the last zone as the sign.
#[inline]
fn packed_zoned(zoned: &[u8], signed: bool, numproc: Numproc) -> Result<Decimal, ProgramCheck> {
    let mut digit_bad = false;
    let mut digit = |b: &u8| {
        digit_bad |= b & 0x0F > 9;
        b & 0x0F
    };
    let magnitude = if zoned.len() <= U64_DIGITS {
        u128::from(zoned.iter().fold(0u64, |m, b| m * 10 + u64::from(digit(b))))
    } else {
        zoned.iter().fold(0u128, |m, b| m * 10 + u128::from(digit(b)))
    };
    let sign = if !signed && numproc == Numproc::Nopfd { 0x0F } else { zoned[zoned.len() - 1] >> 4 };
    if digit_bad || !decimal::is_sign(sign) {
        return Err(ProgramCheck::Data);
    }
    Ok(Decimal { negative: decimal::is_minus(sign), magnitude })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packed_by_copy(bytes: &[u8], signed: bool, numproc: Numproc) -> Result<Decimal, ProgramCheck> {
        let mut p = bytes.to_vec();
        if !signed && numproc == Numproc::Nopfd {
            *p.last_mut().unwrap() |= 0x0F;
        }
        decimal::decode(&p)
    }

    fn zoned_by_pack(bytes: &[u8], signed: bool, sign: Option<SignClause>, numproc: Numproc) -> Result<Decimal, ProgramCheck> {
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
        let value = packed_by_copy(&p, signed, numproc)?;
        Ok(match separate_negative {
            Some(negative) => Decimal { negative, ..value },
            None => value,
        })
    }

    /// Bytes mostly of valid digits and signs, with every kind of fault mixed in, from xorshift64*.
    fn fields(count: usize) -> Vec<Vec<u8>> {
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            state.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 32
        };
        const ZONES: [u8; 8] = [0xF0, 0xF0, 0xF0, 0xC0, 0xD0, 0xB0, 0x40, 0x00];
        const SEPARATE: [u8; 3] = [0x60, 0x4E, 0xF1];
        (0..count)
            .map(|_| {
                let len = 1 + (next() % 36) as usize;
                let mut field: Vec<u8> = (0..len).map(|_| if next() % 20 == 0 { next() as u8 } else { 0xF0 | (next() % 10) as u8 }).collect();
                let at = if next() % 2 == 0 { 0 } else { len - 1 };
                field[at] = ZONES[(next() % 8) as usize] | (field[at] & 0x0F);
                if next() % 3 == 0 {
                    field[at] = SEPARATE[(next() % 3) as usize];
                }
                field
            })
            .collect()
    }

    #[test]
    fn zoned_and_packed_read_as_pack_then_decode_reads_them() {
        let signs = [
            None,
            Some(SignClause { position: SignPosition::Trailing, separate: false }),
            Some(SignClause { position: SignPosition::Leading, separate: false }),
            Some(SignClause { position: SignPosition::Leading, separate: true }),
            Some(SignClause { position: SignPosition::Trailing, separate: true }),
        ];
        let mut read = 0;
        for field in fields(4000) {
            for numproc in [Numproc::Nopfd, Numproc::Pfd] {
                for signed in [false, true] {
                    assert_eq!(packed(&field, signed, numproc), packed_by_copy(&field, signed, numproc), "packed {field:02X?}");
                    for sign in signs {
                        if sign.is_some_and(|s| s.separate) && field.len() < 2 {
                            continue;
                        }
                        let value = zoned(&field, signed, sign, numproc);
                        assert_eq!(value, zoned_by_pack(&field, signed, sign, numproc), "zoned {field:02X?} {sign:?}");
                        read += usize::from(value.is_ok());
                    }
                }
            }
        }
        assert!(read > 10_000, "only {read} fields were valid");
    }
}
