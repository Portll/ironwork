//! The order of records under their keys, as SORT and MERGE give it: plain records and keys
//! described as data, with no program behind them, so a sort utility can use it alone. A record's
//! keys are read once into [`KeyValue`]s, which compare exactly, so the order is total and a stable
//! sort keeps records with equal keys in the order they came.

use crate::codec;
use crate::fixed::{compare_fixed, fixed};
use crate::lir;
use crate::storage::{Kind, Val};
use crate::store::compare_national;
use crate::vocab::{SignClause, SignPosition};
use numeric::Numproc;
use numeric::precision::Places;
use std::cmp::Ordering;
use std::fmt;
use std::rc::Rc;
use zarch::check::ProgramCheck;
use zarch::ebcdic::{self, CodePage, Collation};
use zarch::hfp::Hfp;
use zarch::wide::U256;

/// A key's format, as DFSORT's SORT FIELDS names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// Characters, in the order of the sort's collating sequence.
    Ch,
    /// Characters, in ASCII's order.
    Ac,
    /// Zoned decimal, signed in the last byte's zone.
    Zd,
    /// Zoned decimal, signed in the first byte's zone.
    Clo,
    /// Zoned decimal with a separate leading sign character.
    Csl,
    /// Zoned decimal with a separate trailing sign character.
    Cst,
    Pd,
    /// Unsigned binary.
    Bi,
    /// Signed binary, in two's complement.
    Fi,
}

impl Format {
    /// The decimal format DFSORT reads a zoned or packed item's storage as; None for other items.
    pub fn of_decimal(kind: Kind) -> Option<Format> {
        Some(match kind {
            Kind::Packed { .. } => Format::Pd,
            Kind::Zoned { sign: Some(SignClause { separate: true, position: SignPosition::Leading }), .. } => Format::Csl,
            Kind::Zoned { sign: Some(SignClause { separate: true, position: SignPosition::Trailing }), .. } => Format::Cst,
            Kind::Zoned { sign: Some(SignClause { separate: false, position: SignPosition::Leading }), .. } => Format::Clo,
            Kind::Zoned { .. } => Format::Zd,
            _ => return None,
        })
    }

    fn sign(self) -> Option<SignClause> {
        match self {
            Format::Clo => Some(SignClause { position: SignPosition::Leading, separate: false }),
            Format::Csl => Some(SignClause { position: SignPosition::Leading, separate: true }),
            Format::Cst => Some(SignClause { position: SignPosition::Trailing, separate: true }),
            _ => None,
        }
    }
}

/// One key of a record: `length` bytes from byte `position`, counted from 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    pub position: usize,
    pub length: usize,
    pub format: Format,
    pub ascending: bool,
}

/// The order characters collate in: EBCDIC's, or each byte's position in a sequence, where bytes
/// that collate equal share a position.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Collating {
    #[default]
    Ebcdic,
    Positions(Rc<[u8; 256]>),
}

impl Collating {
    /// 7-bit ASCII's order, as ALPHABET IS STANDARD-1 gives it: the code page's character for each
    /// ASCII code in turn, then every other byte in EBCDIC order.
    pub fn ascii(page: &CodePage) -> Self {
        let given: Vec<u8> = (0..0x80u8).filter_map(|c| page.encode_char(c as char)).collect();
        let mut taken = [false; 256];
        given.iter().for_each(|&b| taken[usize::from(b)] = true);
        let order = given.into_iter().chain((0..=255u8).filter(|&b| !taken[usize::from(b)]));
        let mut positions = [0u8; 256];
        for (at, b) in order.enumerate() {
            positions[usize::from(b)] = at as u8;
        }
        Collating::Positions(Rc::new(positions))
    }

    /// A program's collating sequence.
    pub fn of(collating: &lir::Collating) -> Self {
        match collating {
            lir::Collating::Native => Collating::Ebcdic,
            lir::Collating::Sequence(s) => Collating::Positions(Rc::new(*s.positions)),
        }
    }

    pub fn is_ebcdic(&self) -> bool {
        matches!(self, Collating::Ebcdic)
    }

    /// Each character's position.
    pub fn collate(&self, bytes: &[u8]) -> Vec<u8> {
        match self {
            Collating::Ebcdic => bytes.to_vec(),
            Collating::Positions(p) => bytes.iter().map(|&b| p[usize::from(b)]).collect(),
        }
    }
}

/// A key's value as the comparison sees it.
#[derive(Clone, Debug)]
pub enum KeyValue {
    /// The value as a program reads its item.
    Read(Val),
    /// A zoned or packed key as DFSORT reads a ZD, PD, CLO, CSL or CST field: its sign, and its
    /// digit nibbles as they stand.
    Decimal { negative: bool, digits: Vec<u8> },
    /// Bytes compared unsigned: characters as their positions in the collating sequence, binary
    /// as it stands.
    Collated(Vec<u8>),
}

/// The order of two records by their key values, most significant key first, with each key's
/// direction in `ascending`. Every comparison is exact, so the order is total.
pub fn order(a: &[KeyValue], b: &[KeyValue], ascending: &[bool]) -> Ordering {
    for ((x, y), &up) in a.iter().zip(b).zip(ascending) {
        let o = match (x, y) {
            (KeyValue::Read(Val::Num(x)), KeyValue::Read(Val::Num(y))) => compare_fixed(x, y),
            (KeyValue::Read(Val::Float(x)), KeyValue::Read(Val::Float(y))) => float_order(*x, *y),
            (KeyValue::Read(Val::National(x)), KeyValue::Read(Val::National(y))) => compare_national(x, y),
            (KeyValue::Read(Val::Bytes(x)), KeyValue::Read(Val::Bytes(y))) => ebcdic::compare_alphanumeric(x, y, &Collation::Native),
            (KeyValue::Collated(x), KeyValue::Collated(y)) => x.cmp(y),
            (KeyValue::Decimal { negative: false, digits: x }, KeyValue::Decimal { negative: false, digits: y }) => x.cmp(y),
            (KeyValue::Decimal { negative: true, digits: x }, KeyValue::Decimal { negative: true, digits: y }) => y.cmp(x),
            (KeyValue::Decimal { negative, .. }, KeyValue::Decimal { .. }) => if *negative { Ordering::Less } else { Ordering::Greater },
            _ => Ordering::Equal,
        };
        let o = if up { o } else { o.reverse() };
        if o != Ordering::Equal {
            return o;
        }
    }
    Ordering::Equal
}

/// Floating-point keys in numeric order, by exact value: normalized, the characteristic then the
/// fraction.
pub fn float_order(a: Hfp, b: Hfp) -> Ordering {
    let exact = |h: Hfp| {
        if h.fraction == 0 {
            return (0, 0, 0);
        }
        let top = 4 * h.precision.digits() - 4;
        let (mut exponent, mut fraction) = (h.characteristic as i32, h.fraction);
        while fraction >> top == 0 {
            fraction <<= 4;
            exponent -= 1;
        }
        (if h.negative { -1 } else { 1 }, exponent, fraction)
    };
    let ((sa, ea, fa), (sb, eb, fb)) = (exact(a), exact(b));
    match sa.cmp(&sb) {
        Ordering::Equal if sa < 0 => (eb, fb).cmp(&(ea, fa)),
        Ordering::Equal => (ea, fa).cmp(&(eb, fb)),
        other => other,
    }
}

/// A decimal field's sign and digit nibbles as DFSORT reads them, or None for a format that is not
/// decimal or a field too short to hold one. See SORT_DECIMAL_KEYS, SORT_KEY_INVALID_DIGITS and
/// SORT_NEGATIVE_ZERO in numeric::assumptions.
pub fn decimal(bytes: &[u8], format: Format) -> Option<(bool, Vec<u8>)> {
    let negative = |sign: u8| sign % 2 == 1 && sign != 0xF;
    let low = |b: &[u8]| b.iter().map(|b| b & 0x0F).collect();
    match format {
        Format::Pd => {
            let (last, body) = bytes.split_last()?;
            let mut digits: Vec<u8> = body.iter().flat_map(|b| [b >> 4, b & 0x0F]).collect();
            digits.push(last >> 4);
            Some((negative(last & 0x0F), digits))
        }
        Format::Csl | Format::Cst => {
            let (sign, body) = if format == Format::Csl { bytes.split_first()? } else { bytes.split_last()? };
            Some((*sign == 0x60, low(body)))
        }
        Format::Clo => Some((negative(bytes.first()? >> 4), low(bytes))),
        Format::Zd => Some((negative(bytes.last()? >> 4), low(bytes))),
        _ => None,
    }
}

/// Why a record's keys could not be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyError {
    /// Record `record` ends inside a key.
    Short { record: usize, length: usize },
    /// Under strict reading, key `key` of record `record` is not valid data for its format.
    Data { record: usize, key: usize, check: ProgramCheck },
}

impl fmt::Display for KeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyError::Short { length, .. } => write!(f, "a record of {length} bytes ends inside a key"),
            KeyError::Data { key, check, .. } => write!(f, "key {} is not valid data for its format: {check}", key + 1),
        }
    }
}

/// A sort's keys, most significant first, and how their bytes are read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keys {
    pub keys: Vec<Key>,
    /// The order of CH keys.
    pub collating: Collating,
    /// The order of AC keys.
    pub ascii: Collating,
    /// None reads decimal keys as DFSORT does; Some reads them as a COBOL program under this
    /// NUMPROC reads a signed item, refusing invalid data (ironwork's sort-keys strict).
    pub strict: Option<Numproc>,
}

impl Keys {
    /// CH keys in EBCDIC's order, AC keys in ASCII's by `page`, decimal keys as DFSORT reads them.
    pub fn new(keys: Vec<Key>, page: &CodePage) -> Self {
        Keys { keys, collating: Collating::Ebcdic, ascii: Collating::ascii(page), strict: None }
    }

    /// Each key's direction, as [`order`] takes them.
    pub fn ascending(&self) -> Vec<bool> {
        self.keys.iter().map(|k| k.ascending).collect()
    }

    /// The values of `record`'s keys; `index` is the record's place, which an error names.
    pub fn values(&self, record: &[u8], index: usize) -> Result<Vec<KeyValue>, KeyError> {
        let mut out = Vec::with_capacity(self.keys.len());
        for (n, k) in self.keys.iter().enumerate() {
            let bytes = record.get(k.position..k.position + k.length).ok_or(KeyError::Short { record: index, length: record.len() })?;
            out.push(match k.format {
                Format::Ch => KeyValue::Collated(self.collating.collate(bytes)),
                Format::Ac => KeyValue::Collated(self.ascii.collate(bytes)),
                Format::Bi => KeyValue::Collated(bytes.to_vec()),
                Format::Fi => KeyValue::Collated(bytes.iter().enumerate().map(|(i, &b)| if i == 0 { b ^ 0x80 } else { b }).collect()),
                decimal_format => match self.strict {
                    Some(numproc) => {
                        let read = strict(bytes, decimal_format, numproc).map_err(|check| KeyError::Data { record: index, key: n, check })?;
                        KeyValue::Read(Val::Num(read))
                    }
                    None => match decimal(bytes, decimal_format) {
                        Some((negative, digits)) => KeyValue::Decimal { negative, digits },
                        None => KeyValue::Collated(Vec::new()),
                    },
                },
            });
        }
        Ok(out)
    }

    pub fn compare(&self, a: &[u8], b: &[u8]) -> Result<Ordering, KeyError> {
        Ok(order(&self.values(a, 0)?, &self.values(b, 1)?, &self.ascending()))
    }

    /// The records in key order; records with equal keys keep the order they came in, as DFSORT's
    /// EQUALS keeps them. With no keys, the records as they came (SORT FIELDS=COPY).
    pub fn sort(&self, records: Vec<Vec<u8>>) -> Result<Vec<Vec<u8>>, KeyError> {
        let mut entries = Vec::with_capacity(records.len());
        for (i, record) in records.into_iter().enumerate() {
            let values = self.values(&record, i)?;
            entries.push((record, values));
        }
        let ascending = self.ascending();
        entries.sort_by(|a, b| order(&a.1, &b.1, &ascending));
        Ok(entries.into_iter().map(|(record, _)| record).collect())
    }

    /// The first record that sorts before the one ahead of it, which a MERGE's input may not hold.
    pub fn out_of_order(&self, records: &[Vec<u8>]) -> Result<Option<usize>, KeyError> {
        let ascending = self.ascending();
        let mut last: Option<Vec<KeyValue>> = None;
        for (i, record) in records.iter().enumerate() {
            let values = self.values(record, i)?;
            if last.as_ref().is_some_and(|l| order(l, &values, &ascending) == Ordering::Greater) {
                return Ok(Some(i));
            }
            last = Some(values);
        }
        Ok(None)
    }
}

/// A decimal key read as a signed item of its format is, as a COBOL program reads it.
fn strict(bytes: &[u8], format: Format, numproc: Numproc) -> Result<numeric::precision::Fixed, ProgramCheck> {
    let shortest = if matches!(format, Format::Csl | Format::Cst) { 2 } else { 1 };
    if bytes.len() < shortest {
        return Err(ProgramCheck::Data);
    }
    let d = match format {
        Format::Pd => codec::packed(bytes, true, numproc)?,
        _ => codec::zoned(bytes, true, format.sign(), numproc)?,
    };
    Ok(fixed(d.negative, U256::from_u128(d.magnitude), Places::new(31, 0)))
}
