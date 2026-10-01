//! `rt::sort::Keys`: plain records ordered under keys described as data, as a sort utility with no
//! program behind it uses them.

use ironwork_rt::sort::{Collating, Format, Key, KeyError, Keys};
use numeric::Numproc;
use std::rc::Rc;
use zarch::check::ProgramCheck;
use zarch::ebcdic::CodePage;

fn page() -> &'static CodePage {
    CodePage::by_ccsid(1140).unwrap()
}

fn key(position: usize, length: usize, format: Format, ascending: bool) -> Key {
    Key { position, length, format, ascending }
}

fn sorted(keys: &Keys, records: &[&[u8]]) -> Vec<Vec<u8>> {
    keys.sort(records.iter().map(|r| r.to_vec()).collect()).unwrap()
}

#[test]
fn equal_keys_keep_their_order_and_no_keys_copy() {
    let keys = Keys::new(vec![key(0, 1, Format::Ch, true)], page());
    let records: &[&[u8]] = &[b"\xC2\x01", b"\xC1\x02", b"\xC2\x03", b"\xC1\x04"];
    assert_eq!(sorted(&keys, records), [b"\xC1\x02", b"\xC1\x04", b"\xC2\x01", b"\xC2\x03"]);
    let copy = Keys::new(vec![], page());
    assert_eq!(sorted(&copy, records), records);
}

#[test]
fn a_minor_key_breaks_ties_and_descending_reverses() {
    let keys = Keys::new(vec![key(0, 1, Format::Ch, true), key(1, 1, Format::Bi, false)], page());
    let records: &[&[u8]] = &[b"\xC1\x01", b"\xC2\x09", b"\xC1\x05"];
    assert_eq!(sorted(&keys, records), [b"\xC1\x05", b"\xC1\x01", b"\xC2\x09"]);
}

#[test]
fn decimal_keys_sort_as_dfsort_reads_them_with_negative_zero_first() {
    let zd = Keys::new(vec![key(0, 2, Format::Zd, true)], page());
    let records: &[&[u8]] = &[b"\xF1\xC2", b"\xF0\xC0", b"\xF1\xD2", b"\xF0\xD0", b"\xF0\xF0", b"\xF2\xD0"];
    assert_eq!(sorted(&zd, records), [b"\xF2\xD0", b"\xF1\xD2", b"\xF0\xD0", b"\xF0\xC0", b"\xF0\xF0", b"\xF1\xC2"]);
    let pd = Keys::new(vec![key(0, 2, Format::Pd, true)], page());
    let records: &[&[u8]] = &[b"\x01\x2C", b"\x00\x0D", b"\x01\x2D", b"\x00\x0F"];
    assert_eq!(sorted(&pd, records), [b"\x01\x2D", b"\x00\x0D", b"\x00\x0F", b"\x01\x2C"]);
}

#[test]
fn signs_in_the_leading_zone_or_a_separate_character() {
    let clo = Keys::new(vec![key(0, 2, Format::Clo, true)], page());
    assert_eq!(sorted(&clo, &[b"\xC1\xF2", b"\xD1\xF2"]), [b"\xD1\xF2", b"\xC1\xF2"]);
    let csl = Keys::new(vec![key(0, 3, Format::Csl, true)], page());
    assert_eq!(sorted(&csl, &[b"\x4E\xF1\xF2", b"\x60\xF1\xF2", b"\x60\xF0\xF5"]), [b"\x60\xF1\xF2", b"\x60\xF0\xF5", b"\x4E\xF1\xF2"]);
    let cst = Keys::new(vec![key(0, 3, Format::Cst, true)], page());
    assert_eq!(sorted(&cst, &[b"\xF1\xF2\x4E", b"\xF1\xF2\x60"]), [b"\xF1\xF2\x60", b"\xF1\xF2\x4E"]);
}

#[test]
fn binary_keys_are_unsigned_or_twos_complement() {
    let records: &[&[u8]] = &[b"\x80\x00", b"\x7F\xFF", b"\xFF\xFF"];
    let bi = Keys::new(vec![key(0, 2, Format::Bi, true)], page());
    assert_eq!(sorted(&bi, records), [b"\x7F\xFF", b"\x80\x00", b"\xFF\xFF"]);
    let fi = Keys::new(vec![key(0, 2, Format::Fi, true)], page());
    assert_eq!(sorted(&fi, records), [b"\x80\x00", b"\xFF\xFF", b"\x7F\xFF"]);
}

#[test]
fn ch_keys_follow_the_collating_sequence_and_ac_keys_ascii() {
    let p = page();
    let (a, upper, zero) = (p.encode_char('a').unwrap(), p.encode_char('A').unwrap(), p.encode_char('0').unwrap());
    let records = [[zero], [upper], [a]];
    let records: Vec<&[u8]> = records.iter().map(|r| &r[..]).collect();
    let ch = Keys::new(vec![key(0, 1, Format::Ch, true)], p);
    assert_eq!(sorted(&ch, &records), [[a], [upper], [zero]]);
    let ac = Keys::new(vec![key(0, 1, Format::Ac, true)], p);
    assert_eq!(sorted(&ac, &records), [[zero], [upper], [a]]);
    let named = Keys { collating: Collating::ascii(p), ..ch };
    assert_eq!(sorted(&named, &records), [[zero], [upper], [a]]);
    let mut reversed = [0u8; 256];
    reversed.iter_mut().enumerate().for_each(|(b, at)| *at = 255 - b as u8);
    let backwards = Keys { collating: Collating::Positions(Rc::new(reversed)), ..named };
    assert_eq!(sorted(&backwards, &records), [[zero], [upper], [a]]);
}

#[test]
fn strict_reading_refuses_invalid_data_that_dfsort_reads() {
    let mut keys = Keys::new(vec![key(0, 2, Format::Zd, true)], page());
    let bad: &[&[u8]] = &[b"\xF1\x4B", b"\xF1\xC1"];
    assert_eq!(sorted(&keys, bad), [b"\xF1\xC1", b"\xF1\x4B"]);
    keys.strict = Some(Numproc::Nopfd);
    assert_eq!(keys.sort(bad.iter().map(|r| r.to_vec()).collect()), Err(KeyError::Data { record: 0, key: 0, check: ProgramCheck::Data }));
    assert_eq!(sorted(&keys, &[b"\xF0\xD0", b"\xF0\xC0", b"\xF1\xD2"]), [b"\xF1\xD2", b"\xF0\xD0", b"\xF0\xC0"]);
}

#[test]
fn a_record_that_ends_inside_a_key_is_refused() {
    let keys = Keys::new(vec![key(2, 2, Format::Ch, true)], page());
    let short = keys.sort(vec![b"ABCD".to_vec(), b"ABC".to_vec()]);
    assert_eq!(short, Err(KeyError::Short { record: 1, length: 3 }));
    assert_eq!(short.unwrap_err().to_string(), "a record of 3 bytes ends inside a key");
}

#[test]
fn a_merge_input_out_of_order_is_found() {
    let keys = Keys::new(vec![key(0, 1, Format::Bi, true)], page());
    let records = vec![vec![1], vec![2], vec![2], vec![1], vec![3]];
    assert_eq!(keys.out_of_order(&records), Ok(Some(3)));
    assert_eq!(keys.out_of_order(&records[..3]), Ok(None));
    assert_eq!(keys.compare(&[2], &[1]), Ok(std::cmp::Ordering::Greater));
}
