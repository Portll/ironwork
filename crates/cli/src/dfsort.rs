//! DFSORT's record selection and reformatting over records as bytes: INCLUDE and OMIT conditions,
//! the BUILD and OVERLAY items of INREC, OUTREC and OUTFIL with their edited and converted numbers,
//! and IFTHEN clauses. Fields compare as SORT FIELDS orders them (rt::sort), so a condition reads a
//! zoned, packed or binary field as a key does.

use crate::dfsort_number::{self as number, Number};
use jcl::sort::{Area, Clause, Condition, Constant, Edit, Format, Group, Item, Numeric, Operand, Output, Piece, Pushed, Relation, Selection, When};
use rt::sort::{KeyValue, decimal, order};
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::HashMap;
use zarch::ebcdic::CodePage;

const BLANK: u8 = 0x40;

/// Why a record could not be reformatted: something these items do not do with it, or a number
/// whose digits are not valid, which DFSORT ends with a data exception.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    Refused(String),
    DataException(String),
}

impl From<String> for Failure {
    fn from(why: String) -> Self {
        Failure::Refused(why)
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failure::Refused(why) | Failure::DataException(why) => f.write_str(why),
        }
    }
}

/// Whether `selection` keeps `record`.
pub fn keeps(selection: &Selection, record: &[u8], page: &CodePage) -> Result<bool, String> {
    Ok(holds(&selection.condition, record, page, false)? == selection.include)
}

/// Whether `condition` holds for `record`. With `pad`, as IFTHEN reads a record, bytes past its
/// end are blanks.
fn holds(condition: &Condition, record: &[u8], page: &CodePage, pad: bool) -> Result<bool, String> {
    Ok(match condition {
        Condition::Always(value) => *value,
        Condition::And(a, b) => holds(a, record, page, pad)? && holds(b, record, page, pad)?,
        Condition::Or(a, b) => holds(a, record, page, pad)? || holds(b, record, page, pad)?,
        Condition::Compare { left, relation, right } => {
            let o = compare(left, right, record, page, pad)?;
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

/// `length` bytes from `position`: an error past the record's end, or blanks there with `pad`.
fn field(record: &[u8], position: usize, length: usize, pad: bool) -> Result<Cow<'_, [u8]>, String> {
    let (start, end) = (position - 1, position - 1 + length);
    match record.get(start..end) {
        Some(b) => Ok(Cow::Borrowed(b)),
        None if pad => {
            let mut b: Vec<u8> = record.get(start.min(record.len())..).unwrap_or_default().to_vec();
            b.resize(length, BLANK);
            Ok(Cow::Owned(b))
        }
        None => Err(format!("a record of {} bytes ends inside the field at {position},{length}", record.len())),
    }
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

fn decimal_of(bytes: &[u8], format: Format) -> (bool, Vec<u8>) {
    let format = if format == Format::Pd { rt::sort::Format::Pd } else { rt::sort::Format::Zd };
    decimal(bytes, format).expect("a zoned or packed field of at least one byte")
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

fn compare(left: &Area, right: &Operand, record: &[u8], page: &CodePage, pad: bool) -> Result<Ordering, String> {
    let mine = field(record, left.position, left.length, pad)?;
    let (a, b) = match right {
        Operand::Field(other) => {
            let theirs = field(record, other.position, other.length, pad)?;
            match left.format {
                Format::Zd | Format::Pd => {
                    let width = digit_count(left).max(digit_count(other));
                    (digits(decimal_of(&mine, left.format), width), digits(decimal_of(&theirs, other.format), width))
                }
                Format::Fi => (signed(&mine), signed(&theirs)),
                _ => (KeyValue::Collated(mine.to_vec()), KeyValue::Collated(theirs.to_vec())),
            }
        }
        Operand::Constant(c) => match (c, left.format) {
            (Constant::Chars(text), _) => (KeyValue::Collated(mine.to_vec()), KeyValue::Collated(fitted(page.encode_lossy(text), left.length, BLANK))),
            (Constant::Hex(hex), _) => (KeyValue::Collated(mine.to_vec()), KeyValue::Collated(fitted(hex.clone(), left.length, 0))),
            (Constant::Decimal(n), Format::Zd | Format::Pd) => {
                let width = digit_count(left);
                let constant = (*n < 0, n.unsigned_abs().to_string().bytes().map(|b| b - b'0').collect());
                (digits(decimal_of(&mine, left.format), width), digits(constant, width))
            }
            (Constant::Decimal(n), Format::Fi) => (signed(&mine), signed(&low_bytes(n.to_be_bytes(), left.length, if *n < 0 { 0xFF } else { 0 }))),
            (Constant::Decimal(n), _) => (KeyValue::Collated(mine.to_vec()), KeyValue::Collated(low_bytes(n.to_be_bytes(), left.length, 0))),
        },
    };
    Ok(order(&[a], &[b], &[true]))
}

/// A numeric item's bytes: its value read and edited or converted.
fn number_bytes(value: &Numeric, output: &Output, length: Option<usize>, record: &[u8], pad: bool, page: &CodePage) -> Result<Vec<u8>, Failure> {
    let (n, places) = match *value {
        Numeric::Field { position, length: m, format } => {
            let bytes = field(record, position, m, pad)?;
            let name = format!("{format:?}").to_uppercase();
            let n = number::read(&bytes, format).map_err(|_| Failure::DataException(format!("the {name} field at {position},{m} holds a digit that is not 0-9")))?;
            (n, number::digits_needed(format, m))
        }
        Numeric::Constant(v) => (Number { negative: v < 0, magnitude: v.unsigned_abs() }, number::constant_digits(v)),
    };
    Ok(match output {
        Output::Edit { mask, signs } => number::edit(n, mask, *signs, places, length, page),
        Output::To(f) => number::convert(n, *f, length.unwrap_or_else(|| number::converted_length(*f, places))),
    })
}

/// The bytes one item puts in the record, read from `record`.
fn piece_bytes(piece: &Piece, record: &[u8], variable: bool, pad: bool, page: &CodePage) -> Result<Vec<u8>, Failure> {
    Ok(match piece {
        Piece::Field { position, length: Some(length) } => field(record, *position, *length, pad)?.into_owned(),
        Piece::Field { position, length: None } if variable => record.get(position - 1..).unwrap_or_default().to_vec(),
        Piece::Field { position, length: None } => return Err(Failure::Refused(format!("{position} without a length, the rest of a variable-length record, on fixed-length records"))),
        Piece::Blanks(n) => vec![BLANK; *n],
        Piece::Zeros(n) => vec![0; *n],
        Piece::Chars(text) => page.encode_lossy(text),
        Piece::Hex(hex) => hex.clone(),
        Piece::Number { value, output, length } => number_bytes(value, output, *length, record, pad, page)?,
    })
}

/// The length an item gives, where it does not depend on the record.
fn piece_length(piece: &Piece, page: &CodePage) -> Option<usize> {
    Some(match piece {
        Piece::Field { length, .. } => (*length)?,
        Piece::Blanks(n) | Piece::Zeros(n) => *n,
        Piece::Chars(text) => page.encode_lossy(text).len(),
        Piece::Hex(hex) => hex.len(),
        Piece::Number { value, output, length } => match (length, value) {
            (Some(n), _) => *n,
            (None, Numeric::Field { length: m, format, .. }) => implied_length(output, number::digits_needed(*format, *m), page),
            (None, Numeric::Constant(v)) => implied_length(output, number::constant_digits(*v), page),
        },
    })
}

fn implied_length(output: &Output, places: usize, page: &CodePage) -> usize {
    match output {
        Output::Edit { mask, signs } => number::edit(Number { negative: false, magnitude: 0 }, mask, *signs, places, None, page).len(),
        Output::To(f) => number::converted_length(*f, places),
    }
}

/// A BUILD of `items` from `record`, or an OVERLAY of them onto it. A variable-length record keeps
/// its RDW in bytes 1 to 4: a BUILD begins by copying it, an OVERLAY leaves it alone.
fn build_or_overlay(edit: &Edit, record: &[u8], variable: bool, pad: bool, page: &CodePage) -> Result<Vec<u8>, Failure> {
    Ok(match edit {
        Edit::Build(items) => {
            if variable && !matches!(items.first(), Some(Item { column: None | Some(1), piece: Piece::Field { position: 1, length: None | Some(4..) } })) {
                return Err(Failure::Refused("for variable-length records a BUILD begins with 1,4, the record descriptor word".into()));
            }
            let mut out = Vec::new();
            for item in items {
                if let Some(column) = item.column {
                    if column <= out.len() {
                        return Err(Failure::Refused(format!("column {column} is inside the item before it")));
                    }
                    out.resize(column - 1, BLANK);
                }
                out.extend(piece_bytes(&item.piece, record, variable, pad, page)?);
            }
            out
        }
        Edit::Overlay(items) => {
            let mut out = record.to_vec();
            let mut next = 0;
            for item in items {
                let at = item.column.map_or(next, |c| c - 1);
                if variable && at < 4 {
                    return Err(Failure::Refused(format!("an OVERLAY item at column {} would write over the record descriptor word", at + 1)));
                }
                let bytes = piece_bytes(&item.piece, &out, variable, pad, page)?;
                write_at(&mut out, at, &bytes);
                next = at + bytes.len();
            }
            out
        }
        Edit::IfThen { .. } => unreachable!("an IFTHEN clause holds BUILD or OVERLAY"),
    })
}

/// `bytes` written at `at`, the record widened with blanks where they reach past its end.
fn write_at(record: &mut Vec<u8>, at: usize, bytes: &[u8]) {
    if record.len() < at + bytes.len() {
        record.resize(at + bytes.len(), BLANK);
    }
    record[at..at + bytes.len()].copy_from_slice(bytes);
}

/// `n` as `width` zoned digits, its leftmost digits dropped when it does not fit.
fn zoned(n: u64, width: usize) -> Vec<u8> {
    let text = format!("{n:0>width$}");
    text.bytes().skip(text.len() - width).map(|d| 0xF0 | (d - b'0')).collect()
}

fn set_rdw(record: &mut [u8]) -> Result<(), Failure> {
    let length = u16::try_from(record.len()).map_err(|_| Failure::Refused(format!("a variable-length record of {} bytes is longer than an RDW holds", record.len())))?;
    record[..2].copy_from_slice(&length.to_be_bytes());
    Ok(())
}

/// What a WHEN=GROUP clause carries from record to record.
#[derive(Debug, Default)]
struct GroupState {
    open: bool,
    id: u64,
    seq: u64,
    count: usize,
    /// The group's first record, as the clause read it.
    first: Vec<u8>,
    key: Option<Vec<u8>>,
}

/// One edit applied to records in turn, with what its WHEN=GROUP clauses carry between them.
pub struct Reformatter<'a> {
    edit: &'a Edit,
    variable: bool,
    page: &'a CodePage,
    groups: HashMap<usize, GroupState>,
    /// The length every fixed-length record an IFTHEN edit makes is given, once the first record
    /// shows the input's length.
    fixed: Option<usize>,
}

impl<'a> Reformatter<'a> {
    pub fn new(edit: &'a Edit, variable: bool, page: &'a CodePage) -> Self {
        Reformatter { edit, variable, page, groups: HashMap::new(), fixed: None }
    }

    /// The record the edit makes of `record`.
    pub fn apply(&mut self, record: &[u8]) -> Result<Vec<u8>, Failure> {
        let mut out = match self.edit {
            Edit::IfThen { clauses, length } => {
                if !self.variable && self.fixed.is_none() {
                    self.fixed = Some(length.unwrap_or_else(|| self.implied_length(clauses, record.len())));
                }
                let mut out = self.ifthen(clauses, record)?;
                match (self.variable, self.fixed, length) {
                    (false, Some(n), _) => out.resize(n, BLANK),
                    (true, _, Some(n)) if out.len() > *n => out.truncate(*n),
                    _ => {}
                }
                out
            }
            simple => build_or_overlay(simple, record, self.variable, false, self.page)?,
        };
        if self.variable {
            set_rdw(&mut out)?;
        }
        Ok(out)
    }

    /// The length DFSORT gives fixed-length records an IFTHEN edit makes: the longest of the
    /// input and of what each clause's items produce (assumption C341).
    fn implied_length(&self, clauses: &[Clause], input: usize) -> usize {
        let items_end = |items: &[Item]| -> Option<usize> {
            let (mut at, mut end) = (0, 0);
            for item in items {
                at = item.column.map_or(at, |c| c - 1) + piece_length(&item.piece, self.page)?;
                end = end.max(at);
            }
            Some(end)
        };
        let mut longest = input;
        for clause in clauses {
            let made = match (&clause.when, &clause.edit) {
                (When::Group(g), _) => {
                    let (mut at, mut end) = (0, input);
                    for push in &g.push {
                        at = push.column.map_or(at, |c| c - 1)
                            + match push.value {
                                Pushed::Field { length, .. } => length,
                                Pushed::Id(n) | Pushed::Seq(n) => n,
                            };
                        end = end.max(at);
                    }
                    Some(end)
                }
                (_, Some(Edit::Build(items))) => items_end(items),
                (_, Some(Edit::Overlay(items))) => items_end(items).map(|e| e.max(input)),
                _ => None,
            };
            longest = longest.max(made.unwrap_or(0));
        }
        longest
    }

    /// The clauses in DFSORT's order on one IFTHEN record: WHEN=INIT and WHEN=GROUP, then
    /// WHEN=(cond) and WHEN=ANY until one holds without HIT=NEXT, then WHEN=NONE if no condition
    /// held. Each clause sees what the clauses before it made.
    fn ifthen(&mut self, clauses: &[Clause], record: &[u8]) -> Result<Vec<u8>, Failure> {
        let mut rec = record.to_vec();
        for (k, clause) in clauses.iter().enumerate() {
            match (&clause.when, &clause.edit) {
                (When::Init, Some(edit)) => rec = build_or_overlay(edit, &rec, self.variable, true, self.page)?,
                (When::Group(group), _) => rec = self.group(k, group, rec)?,
                _ => {}
            }
        }
        let (mut any_held, mut held_since_any) = (false, false);
        for clause in clauses {
            let applies = match &clause.when {
                When::Condition(condition) => {
                    let held = holds(condition, &rec, self.page, true)?;
                    any_held |= held;
                    held_since_any |= held;
                    held
                }
                When::Any => std::mem::take(&mut held_since_any),
                _ => continue,
            };
            if applies {
                if let Some(edit) = &clause.edit {
                    rec = build_or_overlay(edit, &rec, self.variable, true, self.page)?;
                }
                if !clause.hit_next {
                    return Ok(rec);
                }
            }
        }
        if !any_held {
            for clause in clauses.iter().filter(|c| matches!(c.when, When::None)) {
                if let Some(edit) = &clause.edit {
                    rec = build_or_overlay(edit, &rec, self.variable, true, self.page)?;
                }
            }
        }
        Ok(rec)
    }

    /// WHEN=GROUP on one record: whether it starts a group, and if it is in one, the group's
    /// fields, identifier and sequence number written into it.
    fn group(&mut self, k: usize, group: &Group, mut rec: Vec<u8>) -> Result<Vec<u8>, Failure> {
        let page = self.page;
        let state = self.groups.entry(k).or_default();
        let begins = match (&group.begin, group.key) {
            (Some(condition), _) => holds(condition, &rec, page, true)?,
            (None, Some((p, m))) => {
                let key = field(&rec, p, m, true)?.into_owned();
                let changed = state.key.as_ref() != Some(&key);
                state.key = Some(key);
                changed
            }
            (None, None) => !state.open,
        };
        if begins {
            *state = GroupState { open: true, id: state.id + 1, seq: 0, count: 0, first: rec.clone(), key: state.key.take() };
        }
        if !state.open {
            return Ok(rec);
        }
        state.count += 1;
        state.seq += 1;
        let ends = match &group.end {
            Some(condition) => holds(condition, &rec, page, true)?,
            None => false,
        } || group.records.is_some_and(|n| state.count >= n);
        let mut at = 0;
        for push in &group.push {
            at = push.column.map_or(at, |c| c - 1);
            if self.variable && at < 4 {
                return Err(Failure::Refused(format!("a PUSH item at column {} would write over the record descriptor word", at + 1)));
            }
            let bytes = match push.value {
                Pushed::Field { position, length } => field(&state.first, position, length, true)?.into_owned(),
                Pushed::Id(n) => zoned(state.id, n),
                Pushed::Seq(n) => zoned(state.seq, n),
            };
            write_at(&mut rec, at, &bytes);
            at += bytes.len();
        }
        if ends {
            state.open = false;
        }
        Ok(rec)
    }
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

    fn reformat(edit: &Edit, record: &[u8], variable: bool) -> Result<Vec<u8>, Failure> {
        Reformatter::new(edit, variable, page()).apply(record)
    }

    /// Each line through the edit in turn, as text with trailing blanks dropped.
    fn through(edit: &Edit, lines: &[&str]) -> Vec<String> {
        let mut r = Reformatter::new(edit, false, page());
        lines.iter().map(|l| page().decode(&r.apply(&page().encode_lossy(l)).unwrap()).trim_end().to_string()).collect()
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
        let built = reformat(c.inrec.as_ref().unwrap(), &rec, false).unwrap();
        assert_eq!(built, [0x40, 0x40, 0x40, 0x40, 0xC1, 0xC2, 0x40, 0x40, 0x5C, 0x00, 0x00, 0x00, 0xC7, 0xC8]);
        let over = reformat(c.outrec.as_ref().unwrap(), &rec, false).unwrap();
        assert_eq!(p.decode(&over), "ZBCDEFGH AZC", "the third item reads the Z the second wrote");
        let short = reformat(&Edit::Build(vec![Item { column: None, piece: Piece::Field { position: 7, length: Some(5) } }]), &rec, false);
        assert_eq!(short.unwrap_err(), Failure::Refused("a record of 8 bytes ends inside the field at 7,5".into()));
    }

    #[test]
    fn a_variable_record_keeps_its_rdw_and_gets_its_new_length() {
        let rec = [0, 7, 0, 0, 0xC1, 0xC2, 0xC3];
        let c = control("OPTION COPY\nOUTREC BUILD=(1,4,C'**',5)");
        assert_eq!(reformat(c.outrec.as_ref().unwrap(), &rec, true).unwrap(), [0, 9, 0, 0, 0x5C, 0x5C, 0xC1, 0xC2, 0xC3]);
        let c = control("OPTION COPY\nOUTREC BUILD=(5,3)");
        assert!(reformat(c.outrec.as_ref().unwrap(), &rec, true).unwrap_err().to_string().contains("begins with 1,4"));
    }

    #[test]
    fn numbers_are_edited_and_converted_in_place() {
        let c = control("OPTION COPY\nOUTREC BUILD=(1,5,ZD,M4,X,6,3,PD,TO=ZD,LENGTH=4,X,+5000,EDIT=(T,TTT))");
        let mut rec = page().encode_lossy("0012J");
        rec.extend([0x01, 0x23, 0x4D]);
        assert_eq!(page().decode(&reformat(c.outrec.as_ref().unwrap(), &rec, false).unwrap()), "  -1.21 123M 5,000", "ZD -121 by M4; PD -1234 to a 4-byte ZD with a D zone");
        let bad = reformat(c.outrec.as_ref().unwrap(), &page().encode_lossy("12*45   "), false);
        assert!(matches!(bad, Err(Failure::DataException(_))), "{bad:?}");
    }

    #[test]
    fn ifthen_applies_its_clauses_in_dfsorts_order() {
        let c = control(concat!(
            "OPTION COPY\n",
            "INREC IFTHEN=(WHEN=INIT,BUILD=(1,6,7:C'----')),\n",
            "  IFTHEN=(WHEN=(1,2,CH,EQ,C'D1'),OVERLAY=(7:C'ONE'),HIT=NEXT),\n",
            "  IFTHEN=(WHEN=(3,1,CH,EQ,C'X'),OVERLAY=(11:C'X')),\n",
            "  IFTHEN=(WHEN=ANY,OVERLAY=(12:C'ANY')),\n",
            "  IFTHEN=(WHEN=(1,2,CH,EQ,C'D2'),OVERLAY=(7:C'TWO')),\n",
            "  IFTHEN=(WHEN=NONE,OVERLAY=(7:C'NONE'))",
        ));
        assert_eq!(through(c.inrec.as_ref().unwrap(), &["D1Xabc", "D1Yabc", "D2Yabc", "Z9abcd"]), ["D1XabcONE-X", "D1YabcONE- ANY", "D2YabcTWO-", "Z9abcdNONE"]);
    }

    #[test]
    fn groups_start_end_and_carry_as_ibm_shows() {
        let groups = |control_text: &str, lines: &[&str]| {
            let c = control(&format!("OPTION COPY\n{control_text}"));
            through(c.outfil[0].edit.as_ref().unwrap(), lines)
        };
        assert_eq!(groups("OUTFIL IFTHEN=(WHEN=GROUP,BEGIN=(1,1,CH,EQ,C'A'),PUSH=(3:ID=1))", &["H", "R", "A", "B", "C", "A", "A", "B"]), ["H", "R", "A 1", "B 1", "C 1", "A 2", "A 3", "B 3"]);
        assert_eq!(groups("OUTFIL IFTHEN=(WHEN=GROUP,KEYBEGIN=(1,1),PUSH=(3:ID=1,SEQ=2))", &["A", "A", "B", "B", "C"]), ["A 101", "A 102", "B 201", "B 202", "C 301"]);
        assert_eq!(groups("OUTFIL IFTHEN=(WHEN=GROUP,END=(1,1,CH,EQ,C'T'),PUSH=(3:ID=1))", &["A", "B", "T", "T", "A", "T", "M"]), ["A 1", "B 1", "T 1", "T 2", "A 3", "T 3", "M 4"]);
        assert_eq!(groups("OUTFIL IFTHEN=(WHEN=GROUP,BEGIN=(1,1,CH,EQ,C'H'),\n END=(1,1,CH,EQ,C'T'),PUSH=(3:ID=1))", &["H", "B", "T", "T", "H", "T", "M", "N", "H", "A"]), ["H 1", "B 1", "T 1", "T", "H 2", "T 2", "M", "N", "H 3", "A 3"]);
        assert_eq!(groups("OUTFIL IFTHEN=(WHEN=GROUP,RECORDS=3,PUSH=(3:ID=1))", &["H", "B", "T", "H", "B", "T", "M", "N"]), ["H 1", "B 1", "T 1", "H 2", "B 2", "T 2", "M 3", "N 3"]);
        assert_eq!(groups("OUTFIL IFTHEN=(WHEN=GROUP,BEGIN=(1,1,CH,EQ,C'H'),RECORDS=3,\n PUSH=(3:ID=1))", &["H", "B", "T", "A", "H", "B", "H", "M", "N", "P"]), ["H 1", "B 1", "T 1", "A", "H 2", "B 2", "H 3", "M 3", "N 3", "P"]);
        assert_eq!(groups("OUTFIL IFTHEN=(WHEN=GROUP,BEGIN=(1,2,CH,EQ,C'HD'),PUSH=(10:3,4))", &["HDjan1", "  item", "HDfeb2", "  item"]), ["HDjan1   jan1", "  item   jan1", "HDfeb2   feb2", "  item   feb2"]);
    }

    #[test]
    fn fixed_records_from_ifthen_take_one_length() {
        let c = control("OPTION COPY\nINREC IFTHEN=(WHEN=(1,1,CH,EQ,C'L'),BUILD=(1,2,C'LONGER')),\n  IFTHEN=(WHEN=NONE,BUILD=(1,2))");
        let mut r = Reformatter::new(c.inrec.as_ref().unwrap(), false, page());
        let lengths: Vec<usize> = ["LX", "SX"].iter().map(|l| r.apply(&page().encode_lossy(l)).unwrap().len()).collect();
        assert_eq!(lengths, [8, 8], "every record takes the longest a clause makes (assumption C341)");
        let c = control("OPTION COPY\nINREC IFOUTLEN=3,IFTHEN=(WHEN=INIT,OVERLAY=(5:C'WIDE'))");
        assert_eq!(Reformatter::new(c.inrec.as_ref().unwrap(), false, page()).apply(&page().encode_lossy("ABCD")).unwrap().len(), 3);
    }
}
