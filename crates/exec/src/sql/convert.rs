//! Host variables to values and back, by Db2's assignment rules: a number that does not fit its
//! host variable is an error rather than a truncation, and a string that does not fit is cut and
//! reported.

use super::{BAD_LENGTH, HostType, NOT_ASSIGNABLE, OUT_OF_RANGE, SqlError, UNCONVERTIBLE, Value};
use crate::codec;
use numeric::Numproc;
use syntax::ast::{SignClause, SignPosition};
use zarch::check::ProgramCheck;
use zarch::decimal::{self, Decimal};
use zarch::ebcdic::CodePage;
use zarch::hfp::{Hfp, Precision};

/// Why an input host variable could not be read: its storage failed as the machine's own read
/// would (S0C7), or Db2 refuses it with an SQLCODE.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadError {
    Check(ProgramCheck),
    Sql(SqlError),
}

/// What storing an output value did besides store it: the length a cut string had, for SQLWARN1
/// and the indicator.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Written {
    pub truncated_from: Option<usize>,
}

/// The value an input host variable sends. `bytes` is the host variable's storage.
pub fn read(bytes: &[u8], ty: &HostType, page: &CodePage, numproc: Numproc) -> Result<Value, ReadError> {
    let as_value = |d: Decimal, scale: u32| {
        let magnitude = d.magnitude as i128;
        Value::Decimal { value: if d.negative { -magnitude } else { magnitude }, scale }
    };
    Ok(match *ty {
        HostType::SmallInt { signed } | HostType::Integer { signed } | HostType::BigInt { signed } => {
            let v = integer(bytes, signed);
            i64::try_from(v).map_or(Value::Decimal { value: v, scale: 0 }, Value::Int)
        }
        HostType::Decimal { scale, signed, .. } => as_value(codec::packed(bytes, signed, numproc).map_err(ReadError::Check)?, scale),
        HostType::Zoned { digits, scale, signed, sign } => as_value(codec::zoned(bytes, digits, signed, sign, numproc).map_err(ReadError::Check)?, scale),
        HostType::Real => Value::Double(Hfp::from_bytes(Precision::Short, bytes).approx()),
        HostType::Double => Value::Double(Hfp::from_bytes(Precision::Long, bytes).approx()),
        HostType::Char(_) => Value::Char(page.decode(bytes)),
        HostType::VarChar(max) => {
            let len = i16::from_be_bytes([bytes[0], bytes[1]]);
            if len < 0 || len as u32 > max {
                return Err(ReadError::Sql(BAD_LENGTH));
            }
            Value::Char(page.decode(&bytes[2..2 + len as usize]))
        }
        HostType::Structure(_) => unreachable!("a host structure is read member by member"),
    })
}

/// Stores a non-null output value into a host variable's storage, as Db2 assigns it.
pub fn write(value: &Value, bytes: &mut [u8], ty: &HostType, page: &CodePage) -> Result<Written, SqlError> {
    match *ty {
        HostType::SmallInt { signed } | HostType::Integer { signed } | HostType::BigInt { signed } => store_integer(whole(value)?, bytes, signed)?,
        HostType::Decimal { digits, scale, signed } => {
            let v = fitted(scaled(value, scale)?, digits, signed)?;
            decimal::encode(bytes, Decimal { negative: v < 0, magnitude: v.unsigned_abs() }).expect("a packed host variable is 1 to 16 bytes");
            if !signed {
                *bytes.last_mut().unwrap() |= 0x0F;
            }
        }
        HostType::Zoned { digits, scale, signed, sign } => store_zoned(fitted(scaled(value, scale)?, digits, signed)?, bytes, digits, signed, sign),
        HostType::Real => bytes.copy_from_slice(&hfp_image(float(value)?, Precision::Short).ok_or(OUT_OF_RANGE)?),
        HostType::Double => bytes.copy_from_slice(&hfp_image(float(value)?, Precision::Long).ok_or(OUT_OF_RANGE)?),
        HostType::Char(_) => return store_text(&text(value, page)?, bytes, page),
        HostType::VarChar(max) => {
            let t = text(value, page)?;
            let n = t.len().min(max as usize);
            bytes[..2].copy_from_slice(&(n as u16).to_be_bytes());
            bytes[2..2 + n].copy_from_slice(&t[..n]);
            return Ok(Written { truncated_from: (t.len() > n).then_some(t.len()) });
        }
        HostType::Structure(_) => unreachable!("a host structure is written member by member"),
    }
    Ok(Written::default())
}

fn integer(bytes: &[u8], signed: bool) -> i128 {
    let unsigned = bytes.iter().fold(0u128, |acc, &b| acc << 8 | u128::from(b)) as i128;
    if signed && bytes[0] & 0x80 != 0 { unsigned - (1i128 << (8 * bytes.len())) } else { unsigned }
}

fn pow10(n: u32) -> Option<i128> {
    10i128.checked_pow(n)
}

/// The value as a whole number, fractional digits dropped.
fn whole(value: &Value) -> Result<i128, SqlError> {
    match *value {
        Value::Int(v) => Ok(v.into()),
        Value::Decimal { value, scale } => Ok(value / pow10(scale).ok_or(OUT_OF_RANGE)?),
        Value::Double(f) if f.is_finite() && f.abs() < 1e38 => Ok(f.trunc() as i128),
        Value::Double(_) => Err(OUT_OF_RANGE),
        _ => Err(NOT_ASSIGNABLE),
    }
}

/// The value unscaled at `scale` places, extra places dropped.
fn scaled(value: &Value, scale: u32) -> Result<i128, SqlError> {
    match *value {
        Value::Int(v) => i128::from(v).checked_mul(pow10(scale).ok_or(OUT_OF_RANGE)?).ok_or(OUT_OF_RANGE),
        Value::Decimal { value, scale: from } if from >= scale => Ok(value / pow10(from - scale).ok_or(OUT_OF_RANGE)?),
        Value::Decimal { value, scale: from } => value.checked_mul(pow10(scale - from).ok_or(OUT_OF_RANGE)?).ok_or(OUT_OF_RANGE),
        Value::Double(f) => {
            let v = (f * 10f64.powi(scale as i32)).trunc();
            if v.is_finite() && v.abs() < 1e38 { Ok(v as i128) } else { Err(OUT_OF_RANGE) }
        }
        _ => Err(NOT_ASSIGNABLE),
    }
}

fn fitted(v: i128, digits: u32, signed: bool) -> Result<i128, SqlError> {
    if (!signed && v < 0) || v.unsigned_abs() >= pow10(digits).ok_or(OUT_OF_RANGE)?.unsigned_abs() { Err(OUT_OF_RANGE) } else { Ok(v) }
}

fn store_integer(v: i128, bytes: &mut [u8], signed: bool) -> Result<(), SqlError> {
    let bits = 8 * bytes.len() as u32;
    let (low, high) = if signed { (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1) } else { (0, (1i128 << bits) - 1) };
    if !(low..=high).contains(&v) {
        return Err(OUT_OF_RANGE);
    }
    let image = v as u128;
    for (i, b) in bytes.iter_mut().rev().enumerate() {
        *b = (image >> (8 * i)) as u8;
    }
    Ok(())
}

/// Zoned digits through UNPK, then the sign where the SIGN clause puts it.
fn store_zoned(v: i128, bytes: &mut [u8], digits: u32, signed: bool, sign: Option<SignClause>) {
    let negative = v < 0;
    let mut packed = vec![0u8; digits as usize / 2 + 1];
    decimal::encode(&mut packed, Decimal { negative, magnitude: v.unsigned_abs() }).expect("the packed image holds the item's digits");
    let unpack = |zoned: &mut [u8]| decimal::unpk(zoned, &packed).expect("a zoned host variable is 1 to 16 bytes");
    match sign {
        Some(SignClause { separate: true, position }) => {
            let n = bytes.len();
            let (sign_at, digits_at) = if position == SignPosition::Leading { (0, 1..n) } else { (n - 1, 0..n - 1) };
            unpack(&mut bytes[digits_at.clone()]);
            bytes[digits_at.end - 1] |= 0xF0;
            bytes[sign_at] = if negative { 0x60 } else { 0x4E };
        }
        Some(SignClause { separate: false, position: SignPosition::Leading }) => {
            unpack(bytes);
            let last = bytes.len() - 1;
            let zone = bytes[last] & 0xF0;
            bytes[last] |= 0xF0;
            bytes[0] = zone | (bytes[0] & 0x0F);
        }
        _ => {
            unpack(bytes);
            if !signed {
                *bytes.last_mut().unwrap() |= 0xF0;
            }
        }
    }
}

fn float(value: &Value) -> Result<f64, SqlError> {
    match *value {
        Value::Int(v) => Ok(v as f64),
        Value::Decimal { value, scale } => Ok(value as f64 / 10f64.powi(scale as i32)),
        Value::Double(f) => Ok(f),
        _ => Err(NOT_ASSIGNABLE),
    }
}

/// An IEEE double as a hexadecimal floating-point storage image, low-order bits that do not fit
/// the short form dropped (assumption SQ9). None when the exponent is outside HFP's range.
fn hfp_image(f: f64, precision: Precision) -> Option<Vec<u8>> {
    let len = precision.bytes();
    if f == 0.0 {
        return Some(vec![0; len]);
    }
    if !f.is_finite() {
        return None;
    }
    let bits = f.to_bits();
    let raw_exponent = ((bits >> 52) & 0x7FF) as i32;
    let (mantissa, exponent) = match raw_exponent {
        0 => (bits & ((1 << 52) - 1), -1074),
        e => ((bits & ((1 << 52) - 1)) | 1 << 52, e - 1075),
    };
    let top = exponent + (64 - mantissa.leading_zeros() as i32);
    let hex = top.div_euclid(4) + i32::from(top.rem_euclid(4) != 0);
    let characteristic = hex + 64;
    if !(0..=127).contains(&characteristic) {
        return None;
    }
    let fraction_bits = if precision == Precision::Short { 24 } else { 56 };
    let shift = exponent + fraction_bits - 4 * hex;
    let fraction = if shift >= 0 { mantissa << shift } else { mantissa >> -shift };
    let lead = u64::from(f.is_sign_negative()) << 7 | characteristic as u64;
    Some(match precision {
        Precision::Short => ((lead << 24 | fraction) as u32).to_be_bytes().to_vec(),
        _ => (lead << 56 | fraction).to_be_bytes().to_vec(),
    })
}

fn text(value: &Value, page: &CodePage) -> Result<Vec<u8>, SqlError> {
    match value {
        Value::Char(s) => page.encode(s).map_err(|_| UNCONVERTIBLE),
        Value::Binary(b) => Ok(b.clone()),
        _ => Err(NOT_ASSIGNABLE),
    }
}

/// A fixed-length string: padded with the code page's space, or cut and reported.
fn store_text(t: &[u8], bytes: &mut [u8], page: &CodePage) -> Result<Written, SqlError> {
    let n = t.len().min(bytes.len());
    bytes[..n].copy_from_slice(&t[..n]);
    bytes[n..].fill(page.encode_char(' ').unwrap_or(0x40));
    Ok(Written { truncated_from: (t.len() > bytes.len()).then_some(t.len()) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> &'static CodePage {
        CodePage::by_ccsid(37).expect("CCSID 37 is carried")
    }

    fn get(bytes: &[u8], ty: &HostType) -> Result<Value, ReadError> {
        read(bytes, ty, page(), Numproc::Nopfd)
    }

    fn put(value: Value, len: usize, ty: &HostType) -> (Vec<u8>, Result<Written, SqlError>) {
        let mut bytes = vec![0xEE; len];
        let r = write(&value, &mut bytes, ty, page());
        (bytes, r)
    }

    const S5V2_PACKED: HostType = HostType::Decimal { digits: 7, scale: 2, signed: true };

    #[test]
    fn binary_host_variables() {
        assert_eq!(get(&[0xFF, 0xFE], &HostType::SmallInt { signed: true }), Ok(Value::Int(-2)));
        assert_eq!(get(&[0xFF, 0xFE], &HostType::SmallInt { signed: false }), Ok(Value::Int(65534)));
        assert_eq!(get(&[0, 0, 1, 0], &HostType::Integer { signed: true }), Ok(Value::Int(256)));
        assert_eq!(put(Value::Int(-2), 2, &HostType::SmallInt { signed: true }).0, [0xFF, 0xFE]);
        assert_eq!(put(Value::Int(40_000), 2, &HostType::SmallInt { signed: true }).1, Err(OUT_OF_RANGE));
        assert_eq!(put(Value::Int(-1), 4, &HostType::Integer { signed: false }).1, Err(OUT_OF_RANGE));
        assert_eq!(put(Value::Decimal { value: 12_399, scale: 2 }, 4, &HostType::Integer { signed: true }).0, [0, 0, 0, 123]);
    }

    #[test]
    fn packed_host_variables() {
        assert_eq!(get(&[0x01, 0x23, 0x45, 0x0D], &S5V2_PACKED), Ok(Value::Decimal { value: -123_450, scale: 2 }));
        assert_eq!(get(&[0x01, 0x23, 0x45, 0x0A << 4], &S5V2_PACKED), Err(ReadError::Check(ProgramCheck::Data)));
        assert_eq!(put(Value::Decimal { value: -123_450, scale: 2 }, 4, &S5V2_PACKED).0, [0x01, 0x23, 0x45, 0x0D]);
        assert_eq!(put(Value::Decimal { value: 1_239, scale: 3 }, 4, &S5V2_PACKED).0, [0x00, 0x00, 0x12, 0x3C]);
        assert_eq!(put(Value::Decimal { value: 12_345_600, scale: 2 }, 4, &S5V2_PACKED).1, Err(OUT_OF_RANGE));
        assert_eq!(put(Value::Int(7), 2, &HostType::Decimal { digits: 3, scale: 0, signed: false }).0, [0x00, 0x7F]);
    }

    #[test]
    fn zoned_host_variables() {
        let trailing = HostType::Zoned { digits: 5, scale: 0, signed: true, sign: None };
        assert_eq!(put(Value::Int(-123), 5, &trailing).0, [0xF0, 0xF0, 0xF1, 0xF2, 0xD3]);
        assert_eq!(get(&[0xF0, 0xF0, 0xF1, 0xF2, 0xD3], &trailing), Ok(Value::Decimal { value: -123, scale: 0 }));
        let leading_separate = HostType::Zoned { digits: 5, scale: 0, signed: true, sign: Some(SignClause { position: SignPosition::Leading, separate: true }) };
        assert_eq!(put(Value::Int(-123), 6, &leading_separate).0, [0x60, 0xF0, 0xF0, 0xF1, 0xF2, 0xF3]);
        assert_eq!(get(&[0x60, 0xF0, 0xF0, 0xF1, 0xF2, 0xF3], &leading_separate), Ok(Value::Decimal { value: -123, scale: 0 }));
        let leading = HostType::Zoned { digits: 3, scale: 0, signed: true, sign: Some(SignClause { position: SignPosition::Leading, separate: false }) };
        assert_eq!(put(Value::Int(45), 3, &leading).0, [0xC0, 0xF4, 0xF5]);
    }

    #[test]
    fn strings_pad_cut_and_report() {
        let (bytes, r) = put(Value::Char("ABCDEFGHIJ".into()), 5, &HostType::Char(5));
        assert_eq!((page().decode(&bytes), r), ("ABCDE".into(), Ok(Written { truncated_from: Some(10) })));
        let (bytes, r) = put(Value::Char("AB".into()), 5, &HostType::Char(5));
        assert_eq!((page().decode(&bytes), r), ("AB   ".into(), Ok(Written::default())));
        let (bytes, r) = put(Value::Char("HELLO".into()), 5, &HostType::VarChar(3));
        assert_eq!((bytes[..2].to_vec(), page().decode(&bytes[2..]), r), (vec![0, 3], "HEL".into(), Ok(Written { truncated_from: Some(5) })));
        let mut varchar = vec![0, 2];
        varchar.extend(page().encode("OKXX").unwrap());
        assert_eq!(get(&varchar, &HostType::VarChar(4)), Ok(Value::Char("OK".into())));
        assert_eq!(get(&[0, 9, 0xC1, 0xC1], &HostType::VarChar(2)), Err(ReadError::Sql(BAD_LENGTH)));
        assert_eq!(put(Value::Int(1), 5, &HostType::Char(5)).1, Err(NOT_ASSIGNABLE));
        assert_eq!(put(Value::Char("1".into()), 2, &HostType::SmallInt { signed: true }).1, Err(NOT_ASSIGNABLE));
    }

    #[test]
    fn floats_cross_between_ieee_and_hfp() {
        assert_eq!(put(Value::Double(1.0), 8, &HostType::Double).0, [0x41, 0x10, 0, 0, 0, 0, 0, 0]);
        assert_eq!(put(Value::Double(-2.5), 8, &HostType::Double).0, [0xC1, 0x28, 0, 0, 0, 0, 0, 0]);
        assert_eq!(put(Value::Double(0.0), 4, &HostType::Real).0, [0, 0, 0, 0]);
        assert_eq!(put(Value::Double(0.5), 4, &HostType::Real).0, [0x40, 0x80, 0, 0]);
        for f in [1.0, -2.5, 0.1, 1234.5678, 1e-20, 6.02e23] {
            let (bytes, _) = put(Value::Double(f), 8, &HostType::Double);
            let Ok(Value::Double(back)) = get(&bytes, &HostType::Double) else { panic!() };
            assert!((back - f).abs() <= f.abs() * 1e-15, "{f} came back as {back}");
        }
        assert_eq!(put(Value::Double(1e300), 8, &HostType::Double).1, Err(OUT_OF_RANGE));
    }
}
