//! DFSORT's record selection and reformatting over records as bytes: INCLUDE and OMIT conditions,
//! and the BUILD and OVERLAY items of INREC, OUTREC and OUTFIL. Fields compare as SORT FIELDS
//! orders them (rt::sort), so a condition reads a zoned, packed or binary field as a key does.

use jcl::sort::{Area, Condition, Constant, Edit, Format, Item, Operand, Piece, Relation, Selection};
use rt::sort::{KeyValue, decimal, order};
use std::cmp::Ordering;
use zarch::ebcdic::CodePage;

const BLANK: u8 = 0x40;

/// Whether `selection` keeps `record`.
pub fn keeps(selection: &Selection, record: &[u8], page: &CodePage) -> Result<bool, String> {
    Ok(holds(&selection.condition, record, page)? == selection.include)
}

fn holds(condition: &Condition, record: &[u8], page: &CodePage) -> Result<bool, String> {
    Ok(match condition {
        Condition::Always(value) => *value,
        Condition::And(a, b) => holds(a, record, page)? && holds(b, record, page)?,
        Condition::Or(a, b) => holds(a, record, page)? || holds(b, record, page)?,
        Condition::Compare { left, relation, right } => {
            let o = compare(left, right, record, page)?;
            match relation {
                Relation::Eq => o == Ordering::Equal,
                Relation::Ne => o != Ordering::Equal,
                Relation::Gt => o == Ordering::Greater,
                Relation::Ge => o != Ordering::Less,
                Relation::Lt => o == Ordering::Less,
                Relation::Le => o != Ordering::Greater,
            }
        }
    })
}

fn bytes<'r>(record: &'r [u8], area: &Area) -> Result<&'r [u8], String> {
    record.get(area.position - 1..area.position - 1 + area.length).ok_or_else(|| format!("a record of {} bytes ends inside the field at {},{}", record.len(), area.position, area.length))
}

/// A zoned or packed field's sign and digits, with the digits widened to `width` by zeros on the
/// left or narrowed to their last `width`.
fn digits(sign_and_digits: (bool, Vec<u8>), width: usize) -> KeyValue {
    let (negative, d) = sign_and_digits;
    let digits = if d.len() >= width { d[d.len() - width..].to_vec() } else { std::iter::repeat_n(0, width - d.len()).chain(d).collect() };
    KeyValue::Decimal { negative, digits }
}

fn digit_count(area: &Area) -> usize {
    if area.format == Format::Pd { 2 * area.length - 1 } else { area.length }
}

fn decimal_of(field: &[u8], format: Format) -> (bool, Vec<u8>) {
    let format = if format == Format::Pd { rt::sort::Format::Pd } else { rt::sort::Format::Zd };
    decimal(field, format).expect("a zoned or packed field of at least one byte")
}

/// FI bytes in the order their values take: the sign bit inverted.
fn signed(bytes: &[u8]) -> KeyValue {
    KeyValue::Collated(bytes.iter().enumerate().map(|(i, &b)| if i == 0 { b ^ 0x80 } else { b }).collect())
}

/// `bytes` padded with `fill` or truncated on the right to `length`.
fn fitted(mut bytes: Vec<u8>, length: usize, fill: u8) -> Vec<u8> {
    bytes.resize(length, fill);
    bytes
}

/// The last `length` bytes of a big-endian number, or the number widened on the left by `fill`.
fn low_bytes(be: [u8; 16], length: usize, fill: u8) -> Vec<u8> {
    if length <= 16 { be[16 - length..].to_vec() } else { std::iter::repeat_n(fill, length - 16).chain(be).collect() }
}

fn compare(left: &Area, right: &Operand, record: &[u8], page: &CodePage) -> Result<Ordering, String> {
    let field = bytes(record, left)?;
    let (a, b) = match right {
        Operand::Field(other) => {
            let theirs = bytes(record, other)?;
            match left.format {
                Format::Zd | Format::Pd => {
                    let width = digit_count(left).max(digit_count(other));
                    (digits(decimal_of(field, left.format), width), digits(decimal_of(theirs, other.format), width))
                }
                Format::Fi => (signed(field), signed(theirs)),
                _ => (KeyValue::Collated(field.to_vec()), KeyValue::Collated(theirs.to_vec())),
            }
        }
        Operand::Constant(c) => match (c, left.format) {
            (Constant::Chars(text), _) => (KeyValue::Collated(field.to_vec()), KeyValue::Collated(fitted(page.encode_lossy(text), left.length, BLANK))),
            (Constant::Hex(hex), _) => (KeyValue::Collated(field.to_vec()), KeyValue::Collated(fitted(hex.clone(), left.length, 0))),
            (Constant::Decimal(n), Format::Zd | Format::Pd) => {
                let width = digit_count(left);
                let constant = (*n < 0, n.unsigned_abs().to_string().bytes().map(|b| b - b'0').collect());
                (digits(decimal_of(field, left.format), width), digits(constant, width))
            }
            (Constant::Decimal(n), Format::Fi) => (signed(field), signed(&low_bytes(n.to_be_bytes(), left.length, if *n < 0 { 0xFF } else { 0 }))),
            (Constant::Decimal(n), _) => (KeyValue::Collated(field.to_vec()), KeyValue::Collated(low_bytes(n.to_be_bytes(), left.length, 0))),
        },
    };
    Ok(order(&[a], &[b], &[true]))
}

/// The bytes one item puts in the record, read from `record`.
fn piece_bytes(piece: &Piece, record: &[u8], variable: bool, page: &CodePage) -> Result<Vec<u8>, String> {
    Ok(match piece {
        Piece::Field { position, length: Some(length) } => record.get(position - 1..position - 1 + length).ok_or_else(|| format!("a record of {} bytes ends inside the field at {position},{length}", record.len()))?.to_vec(),
        Piece::Field { position, length: None } if variable => record.get(position - 1..).unwrap_or_default().to_vec(),
        Piece::Field { position, length: None } => return Err(format!("{position} without a length, the rest of a variable-length record, on fixed-length records")),
        Piece::Blanks(n) => vec![BLANK; *n],
        Piece::Zeros(n) => vec![0; *n],
        Piece::Chars(text) => page.encode_lossy(text),
        Piece::Hex(hex) => hex.clone(),
    })
}

/// The record `edit` makes of `record`. A variable-length record keeps its RDW in bytes 1 to 4: a
/// BUILD begins by copying it, an OVERLAY leaves it alone, and its length is set to the new
/// record's.
pub fn reformat(edit: &Edit, record: &[u8], variable: bool, page: &CodePage) -> Result<Vec<u8>, String> {
    let mut out = match edit {
        Edit::Build(items) => {
            if variable && !matches!(items.first(), Some(Item { column: None | Some(1), piece: Piece::Field { position: 1, length: None | Some(4..) } })) {
                return Err("for variable-length records a BUILD begins with 1,4, the record descriptor word".into());
            }
            let mut out = Vec::new();
            for item in items {
                if let Some(column) = item.column {
                    if column <= out.len() {
                        return Err(format!("column {column} is inside the item before it"));
                    }
                    out.resize(column - 1, BLANK);
                }
                out.extend(piece_bytes(&item.piece, record, variable, page)?);
            }
            out
        }
        Edit::Overlay(items) => {
            let mut out = record.to_vec();
            let mut next = 0;
            for item in items {
                let at = item.column.map_or(next, |c| c - 1);
                if variable && at < 4 {
                    return Err(format!("an OVERLAY item at column {} would write over the record descriptor word", at + 1));
                }
                let bytes = piece_bytes(&item.piece, &out, variable, page)?;
                if out.len() < at + bytes.len() {
                    out.resize(at + bytes.len(), BLANK);
                }
                out[at..at + bytes.len()].copy_from_slice(&bytes);
                next = at + bytes.len();
            }
            out
        }
    };
    if variable {
        let length = u16::try_from(out.len()).map_err(|_| format!("a variable-length record of {} bytes is longer than an RDW holds", out.len()))?;
        out[..2].copy_from_slice(&length.to_be_bytes());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jcl::sort::parse;

    fn page() -> &'static CodePage {
        numeric::options::Options::default().code_page()
    }

    fn control(text: &str) -> jcl::sort::Control {
        parse(&text.lines().map(|l| format!(" {l}")).collect::<Vec<_>>()).unwrap()
    }

    fn kept(cond: &str, record: &[u8]) -> bool {
        let c = control(&format!("OPTION COPY\nINCLUDE COND={cond}"));
        keeps(&c.selection.unwrap(), record, page()).unwrap()
    }

    #[test]
    fn constants_are_fitted_to_the_field_as_dfsort_fits_them() {
        let rec = page().encode_lossy("AB  0042");
        assert!(kept("(1,4,CH,EQ,C'AB')", &rec), "a character string is padded with blanks");
        assert!(kept("(1,1,CH,EQ,C'AXYZ')", &rec), "and truncated on the right");
        assert!(kept("(1,2,CH,EQ,X'C1C2')", &rec));
        assert!(kept("(5,4,ZD,EQ,+42)", &rec) && kept("(5,4,ZD,GT,-1)", &rec) && kept("(5,4,ZD,LT,43)", &rec));
        assert!(kept("(7,2,ZD,EQ,1042)", &rec), "a decimal constant is truncated on the left: 1042 becomes 42");
        assert!(kept("((1,1,CH,EQ,C'Z'),OR,(1,1,CH,EQ,C'A'),\n  AND,(2,1,CH,EQ,C'B'))", &rec), "AND before OR");
        assert!(!kept("(((1,1,CH,EQ,C'Z'),OR,(1,1,CH,EQ,C'A')),\n  AND,(2,1,CH,EQ,C'Q'))", &rec));
    }

    #[test]
    fn packed_binary_and_signed_fields() {
        let rec = [0x00u8, 0x12, 0x3D, 0xFF, 0xFE, 0x00, 0x02];
        assert!(kept("(1,3,PD,EQ,-123)", &rec) && kept("(1,3,PD,LT,0)", &rec));
        assert!(kept("(4,2,FI,EQ,-2)", &rec) && kept("(4,2,FI,LT,1)", &rec));
        assert!(kept("(4,2,BI,GT,65533)", &rec) && kept("(6,2,BI,EQ,2)", &rec));
        assert!(kept("(1,3,PD,LT,6,2,ZD)", &[0x00, 0x12, 0x3D, 0, 0, 0xF0, 0xF1]), "-123 is less than zoned 01");
    }

    #[test]
    fn build_places_items_by_column_and_overlay_writes_in_order() {
        let p = page();
        let rec = p.encode_lossy("ABCDEFGH");
        let c = control("OPTION COPY\nINREC BUILD=(5:1,2,2X,C'*',X'00',2Z,7,2)\nOUTREC OVERLAY=(10:1,3,1:C'Z',11:1,1)");
        let built = reformat(c.inrec.as_ref().unwrap(), &rec, false, p).unwrap();
        assert_eq!(built, [0x40, 0x40, 0x40, 0x40, 0xC1, 0xC2, 0x40, 0x40, 0x5C, 0x00, 0x00, 0x00, 0xC7, 0xC8]);
        let over = reformat(c.outrec.as_ref().unwrap(), &rec, false, p).unwrap();
        assert_eq!(p.decode(&over), "ZBCDEFGH AZC", "the third item reads the Z the second wrote");
        let short = reformat(&Edit::Build(vec![Item { column: None, piece: Piece::Field { position: 7, length: Some(5) } }]), &rec, false, p);
        assert_eq!(short.unwrap_err(), "a record of 8 bytes ends inside the field at 7,5");
    }

    #[test]
    fn a_variable_record_keeps_its_rdw_and_gets_its_new_length() {
        let p = page();
        let rec = [0, 7, 0, 0, 0xC1, 0xC2, 0xC3];
        let c = control("OPTION COPY\nOUTREC BUILD=(1,4,C'**',5)");
        assert_eq!(reformat(c.outrec.as_ref().unwrap(), &rec, true, p).unwrap(), [0, 9, 0, 0, 0x5C, 0x5C, 0xC1, 0xC2, 0xC3]);
        let c = control("OPTION COPY\nOUTREC BUILD=(5,3)");
        assert!(reformat(c.outrec.as_ref().unwrap(), &rec, true, p).unwrap_err().contains("begins with 1,4"));
    }
}
