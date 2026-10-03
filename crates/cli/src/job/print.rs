//! IDCAMS PRINT: a data set's records listed in CHARACTER, HEX or DUMP format, from SKIP or
//! FROMKEY to COUNT or TOKEY, laid out as the Access Method Services samples show them (z/OS 3.1
//! DFSMS Access Method Services for Catalogs, idai200: da6i2245, dgt3i239 to dgt3i241, da6i2249).

use jcl::idcams::{First, Key, Keys, Last, Print, PrintFormat};
use std::fmt::Write;
use std::ops::Range;
use zarch::ebcdic::CodePage;

/// What PRINT reads: the data set's name as the listing shows it, and its records as EBCDIC
/// bytes in the order the data set holds them (a cluster's in key or record-number order).
pub(super) struct Input<'a> {
    pub name: &'a str,
    pub records: Vec<Vec<u8>>,
    pub kind: Kind,
}

/// How the listing identifies each record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    /// A key-sequenced cluster or an alternate index, by its key.
    Keyed(Keys),
    /// An entry-sequenced cluster, by relative byte address.
    Entry,
    /// A relative record cluster, by relative record number.
    Numbered,
    /// A sequential data set, by its place in the data set.
    Nonvsam,
}

/// Characters on a line of CHARACTER format, and hexadecimal digits on a line of HEX format.
const WIDTH: usize = 120;

/// Bytes on a line of DUMP format, in eight groups of four.
const DUMP_BYTES: usize = 32;

/// The last byte of a generic FROMKEY or TOKEY, an asterisk.
const GENERIC: u8 = 0x5C;

/// The graphics of the PN print chain, PL/I's 60-character set, which IDCAMS prints by unless
/// PARM GRAPHICS names another chain or a table; any other byte prints as a period.
const GRAPHICS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 =+-*/(),.'%;:¬&|><_?$@#";

/// The listing's lines go into `out` and the messages into `messages`; the condition code is
/// returned.
pub(super) fn print(command: &Print, input: &Input<'_>, page: &CodePage, out: &mut Vec<String>, messages: &mut Vec<String>) -> u16 {
    if input.records.is_empty() && input.kind != Kind::Nonvsam {
        messages.push(format!("IDC3300I ERROR OPENING {}", input.name));
        messages.push("IDC3351I ** VSAM OPEN RETURN CODE IS 160".into());
        return 12;
    }
    let records = located(input);
    let range = match range(command, input, &records, page) {
        Ok(r) => r,
        Err(ended) => {
            messages.extend(ended);
            return 12;
        }
    };
    out.push(format!("LISTING OF DATA SET -{}", input.name));
    let listed = &records[range];
    for (i, (place, record)) in listed.iter().enumerate() {
        if i > 0 {
            out.push(String::new());
        }
        out.push(identity(input.kind, *place, record, command.format, page));
        match command.format {
            PrintFormat::Character => {
                out.push(String::new());
                out.extend(record.chunks(WIDTH).map(|c| characters(c, page)));
            }
            PrintFormat::Hex => out.extend(record.chunks(WIDTH / 2).map(hex)),
            PrintFormat::Dump => out.extend(record.chunks(DUMP_BYTES).enumerate().map(|(n, c)| dump_line(n * DUMP_BYTES, c, page))),
        }
    }
    messages.push(format!("IDC0005I NUMBER OF RECORDS PROCESSED WAS {}", listed.len()));
    if listed.is_empty() { 4 } else { 0 }
}

/// Each record PRINT can list, with its place: its relative byte address in an entry-sequenced
/// cluster, the relative record number of a relative record cluster's filled slot, or the record's
/// number in the data set. An empty slot is a record of no bytes or of zero bytes only.
fn located<'r>(input: &'r Input<'_>) -> Vec<(usize, &'r [u8])> {
    let mut rba = 0;
    input
        .records
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            let place = match input.kind {
                Kind::Entry => rba,
                _ => i + 1,
            };
            rba += r.len();
            let empty_slot = input.kind == Kind::Numbered && r.iter().all(|&b| b == 0);
            (!empty_slot).then_some((place, r.as_slice()))
        })
        .collect()
}

/// The records the command lists, or the messages that end it: a key on a data set without
/// keys, a key longer than the data set's, or a start past the last record.
fn range(command: &Print, input: &Input<'_>, records: &[(usize, &[u8])], page: &CodePage) -> Result<Range<usize>, Vec<String>> {
    let action = |second: &str| vec![format!("IDC3302I ACTION ERROR ON {}", input.name), second.to_string()];
    let bound = |k: &Key| match input.kind {
        Kind::Keyed(keys) => {
            let b = generic(k, page);
            if b.len() > keys.length { Err(action("IDC3310I ** KEY SUPPLIED IS LONGER THAN KEY LENGTH OF DATA SET")) } else { Ok((keys, b)) }
        }
        _ => Err(action("IDC3311I ** TYPE OF POSITIONING NOT SUPPORTED")),
    };
    let from = match &command.first {
        First::Key(k) => Some(bound(k)?),
        _ => None,
    };
    let to = match &command.last {
        Last::Key(k) => Some(bound(k)?),
        _ => None,
    };
    let start = match (&command.first, from) {
        (_, Some((keys, b))) => records.iter().position(|(_, r)| leading(r, keys, b.len()) >= b.as_slice()),
        (First::Skip(n), _) => Some(*n).filter(|&n| n <= records.len()),
        _ => Some(0),
    };
    let Some(start) = start else { return Err(vec!["IDC3006I FUNCTION TERMINATED DUE TO BEGINNING POSITIONING ERROR".into()]) };
    let end = match (&command.last, to) {
        (_, Some((keys, b))) => start + records[start..].iter().take_while(|(_, r)| leading(r, keys, b.len()) <= b.as_slice()).count(),
        (Last::Count(n), _) => start.saturating_add(*n).min(records.len()),
        _ => records.len(),
    };
    Ok(start..end)
}

/// A FROMKEY or TOKEY as bytes, its characters through the code page, without the asterisk
/// that makes it generic. A key shorter than the data set's matches the keys it begins.
fn generic(key: &Key, page: &CodePage) -> Vec<u8> {
    let mut bytes = match key {
        Key::Chars(text) => page.encode_lossy(text),
        Key::Bytes(b) => b.clone(),
    };
    if bytes.last() == Some(&GENERIC) {
        bytes.pop();
    }
    bytes
}

/// The first `length` bytes of a record's key, fewer where the record ends first.
fn leading(record: &[u8], keys: Keys, length: usize) -> &[u8] {
    let key = record.get(keys.offset..).unwrap_or_default();
    &key[..key.len().min(length).min(keys.length)]
}

/// The line before a record's data: its key, in characters for CHARACTER format and in
/// hexadecimal otherwise, or its relative byte address, relative record number or sequence number.
fn identity(kind: Kind, place: usize, record: &[u8], format: PrintFormat, page: &CodePage) -> String {
    match kind {
        Kind::Keyed(keys) => {
            let key = leading(record, keys, keys.length);
            format!("KEY OF RECORD - {}", if format == PrintFormat::Character { characters(key, page) } else { hex(key) })
        }
        Kind::Entry => format!("RBA OF RECORD - {place}"),
        Kind::Numbered => format!("RELATIVE RECORD NUMBER - {place}"),
        Kind::Nonvsam => format!("RECORD SEQUENCE NUMBER - {place}"),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
        let _ = write!(s, "{b:02X}");
        s
    })
}

fn characters(bytes: &[u8], page: &CodePage) -> String {
    bytes
        .iter()
        .map(|&b| match page.decode_byte(b) {
            c if GRAPHICS.contains(c) => c,
            _ => '.',
        })
        .collect()
}

/// One line of DUMP format: the offset in the record, up to 32 bytes in hexadecimal in groups of
/// four with a wider gap after the fourth, and the same bytes as characters between asterisks.
fn dump_line(offset: usize, bytes: &[u8], page: &CodePage) -> String {
    let mut digits = String::with_capacity(72);
    for slot in 0..DUMP_BYTES {
        match bytes.get(slot) {
            Some(b) => {
                let _ = write!(digits, "{b:02X}");
            }
            None => digits.push_str("  "),
        }
        if slot % 4 == 3 && slot + 1 < DUMP_BYTES {
            digits.push_str(if slot + 1 == DUMP_BYTES / 2 { "  " } else { " " });
        }
    }
    format!("{offset:04X}   {digits}   *{:<32}*", characters(bytes, page))
}

#[cfg(test)]
mod tests {
    use super::*;
    use jcl::idcams::Target;

    fn page() -> &'static CodePage {
        CodePage::by_ccsid(1140).unwrap()
    }

    fn command(format: PrintFormat, first: First, last: Last) -> Print {
        Print { from: Target::Dd("IN".into()), format, first, last, out: None }
    }

    fn records(lines: &[&str]) -> Vec<Vec<u8>> {
        lines.iter().map(|l| page().encode_lossy(l)).collect()
    }

    fn run(command: &Print, kind: Kind, records: Vec<Vec<u8>>) -> (u16, Vec<String>) {
        let (mut out, mut messages) = (Vec::new(), Vec::new());
        let code = print(command, &Input { name: "PAY.KSDS", records, kind }, page(), &mut out, &mut messages);
        out.extend(messages);
        (code, out)
    }

    const KEYS: Kind = Kind::Keyed(Keys { length: 4, offset: 0 });

    fn keys_listed(out: &[String]) -> Vec<String> {
        out.iter().filter_map(|l| l.strip_prefix("KEY OF RECORD - ")).map(str::to_string).collect()
    }

    fn staff() -> Vec<Vec<u8>> {
        records(&["AB01SMITH", "AB02JONES", "AC01ADAMS", "BA01BAKER", "BB07CLARK"])
    }

    #[test]
    fn dump_lists_offsets_hexadecimal_groups_and_characters_as_the_sample_does() {
        let record: Vec<u8> = page().encode_lossy("ABCD000000000001ABCDEFGHIJKLMNOPQRST").into_iter().chain([0, 0, 0, 0]).collect();
        let (code, out) = run(&command(PrintFormat::Dump, First::Start, Last::End), Kind::Keyed(Keys { length: 12, offset: 4 }), vec![record]);
        assert_eq!(code, 0);
        assert_eq!(
            out,
            [
                "LISTING OF DATA SET -PAY.KSDS",
                "KEY OF RECORD - F0F0F0F0F0F0F0F0F0F0F0F1",
                "0000   C1C2C3C4 F0F0F0F0 F0F0F0F0 F0F0F0F1  C1C2C3C4 C5C6C7C8 C9D1D2D3 D4D5D6D7   *ABCD000000000001ABCDEFGHIJKLMNOP*",
                format!("0020   D8D9E2E3 00000000{}*QRST....{}*", " ".repeat(58), " ".repeat(24)).as_str(),
                "IDC0005I NUMBER OF RECORDS PROCESSED WAS 1",
            ]
        );
        assert!(out[2..4].iter().all(|l| l.len() == 116), "{out:?}");
        let tail: Vec<u8> = page().encode_lossy("IJKLMNOPQRST").into_iter().chain([0; 4]).collect();
        assert_eq!(dump_line(0x180, &tail, page()), "0180   C9D1D2D3 D4D5D6D7 D8D9E2E3 00000000                                        *IJKLMNOPQRST....                *");
    }

    #[test]
    fn dump_prints_a_byte_outside_the_print_chain_as_a_period_and_ends_a_short_group_early() {
        let line = dump_line(0x40, &page().encode_lossy("Ab1-*"), page());
        assert_eq!(line, format!("0040   C182F160 5C{}*A.1-*{}*", " ".repeat(64), " ".repeat(27)));
    }

    #[test]
    fn hex_runs_sixty_bytes_to_a_line_and_character_has_a_blank_line_then_one_hundred_and_twenty() {
        let record = page().encode_lossy(&"ABCDEFGHIJ".repeat(13));
        let (_, out) = run(&command(PrintFormat::Hex, First::Start, Last::End), Kind::Entry, vec![record.clone()]);
        assert_eq!(out[1], "RBA OF RECORD - 0");
        assert_eq!((out[2].len(), out[3].len(), out[4].len()), (120, 120, 20));
        assert!(out[2].starts_with("C1C2C3C4C5C6C7C8C9D1C1"), "{}", out[2]);
        let (_, out) = run(&command(PrintFormat::Character, First::Start, Last::End), Kind::Entry, vec![record]);
        assert_eq!(out[1..4], ["RBA OF RECORD - 0", "", "ABCDEFGHIJ".repeat(12).as_str()]);
        assert_eq!(out[4], "ABCDEFGHIJ");
    }

    #[test]
    fn a_key_shows_in_characters_for_character_format_and_in_hexadecimal_otherwise() {
        let (_, out) = run(&command(PrintFormat::Character, First::Start, Last::Count(1)), KEYS, staff());
        assert_eq!(out[..4], ["LISTING OF DATA SET -PAY.KSDS", "KEY OF RECORD - AB01", "", "AB01SMITH"]);
        let (_, out) = run(&command(PrintFormat::Hex, First::Start, Last::Count(1)), KEYS, staff());
        assert_eq!(out[1..3], ["KEY OF RECORD - C1C2F0F1", "C1C2F0F1E2D4C9E3C8"]);
    }

    #[test]
    fn records_are_parted_by_a_blank_line_and_counted_at_the_end() {
        let (code, out) = run(&command(PrintFormat::Hex, First::Start, Last::End), Kind::Nonvsam, records(&["A", "B"]));
        assert_eq!(code, 0);
        assert_eq!(out, ["LISTING OF DATA SET -PAY.KSDS", "RECORD SEQUENCE NUMBER - 1", "C1", "", "RECORD SEQUENCE NUMBER - 2", "C2", "IDC0005I NUMBER OF RECORDS PROCESSED WAS 2"]);
    }

    #[test]
    fn skip_and_count_select_by_position() {
        let (code, out) = run(&command(PrintFormat::Character, First::Skip(1), Last::Count(2)), KEYS, staff());
        assert_eq!(code, 0);
        assert_eq!(keys_listed(&out), ["AB02", "AC01"]);
        assert_eq!(out.last().unwrap(), "IDC0005I NUMBER OF RECORDS PROCESSED WAS 2");
        let (_, out) = run(&command(PrintFormat::Character, First::Skip(3), Last::End), KEYS, staff());
        assert_eq!(keys_listed(&out), ["BA01", "BB07"]);
    }

    #[test]
    fn fromkey_starts_at_the_key_or_the_next_higher_and_tokey_stops_at_it_or_the_next_lower() {
        let chars = |s: &str| Key::Chars(s.into());
        let (_, out) = run(&command(PrintFormat::Character, First::Key(chars("AB02")), Last::Key(chars("BA01"))), KEYS, staff());
        assert_eq!(keys_listed(&out), ["AB02", "AC01", "BA01"]);
        let (_, out) = run(&command(PrintFormat::Character, First::Key(chars("AB03")), Last::Key(Key::Bytes(vec![0xC2, 0xC1, 0xF0, 0xF0]))), KEYS, staff());
        assert_eq!(keys_listed(&out), ["AC01"]);
        let (_, out) = run(&command(PrintFormat::Character, First::Key(chars("AC01")), Last::Count(1)), KEYS, staff());
        assert_eq!(keys_listed(&out), ["AC01"]);
    }

    #[test]
    fn a_generic_key_matches_every_key_it_begins() {
        let chars = |s: &str| Key::Chars(s.into());
        let (_, out) = run(&command(PrintFormat::Character, First::Key(chars("AB*")), Last::Key(chars("AC*"))), KEYS, staff());
        assert_eq!(keys_listed(&out), ["AB01", "AB02", "AC01"]);
        let (_, out) = run(&command(PrintFormat::Character, First::Key(chars("B")), Last::Key(chars("B"))), KEYS, staff());
        assert_eq!(keys_listed(&out), ["BA01", "BB07"]);
        let (_, out) = run(&command(PrintFormat::Character, First::Key(Key::Bytes(vec![0xC2, 0x5C])), Last::End), KEYS, staff());
        assert_eq!(keys_listed(&out), ["BA01", "BB07"]);
    }

    #[test]
    fn an_alternate_index_is_keyed_after_its_header() {
        let mut record = vec![0x01, 0x00, 0x01, 0x04, 0x03];
        record.extend(page().encode_lossy("JONAB02"));
        let (_, out) = run(&command(PrintFormat::Dump, First::Key(Key::Chars("JON".into())), Last::End), Kind::Keyed(Keys { length: 3, offset: 5 }), vec![record]);
        assert_eq!(out[1], "KEY OF RECORD - D1D6D5");
    }

    #[test]
    fn an_entry_sequenced_record_is_found_by_its_byte_address_and_a_relative_one_by_its_slot() {
        let (_, out) = run(&command(PrintFormat::Character, First::Skip(1), Last::End), Kind::Entry, records(&["ONE", "TWO22", "THREE"]));
        assert_eq!(out.iter().filter(|l| l.starts_with("RBA")).collect::<Vec<_>>(), ["RBA OF RECORD - 3", "RBA OF RECORD - 8"]);
        let slots = vec![page().encode_lossy("R1"), vec![0, 0], Vec::new(), page().encode_lossy("R4")];
        let (code, out) = run(&command(PrintFormat::Character, First::Start, Last::End), Kind::Numbered, slots);
        assert_eq!(code, 0);
        assert_eq!(out.iter().filter(|l| l.starts_with("RELATIVE")).collect::<Vec<_>>(), ["RELATIVE RECORD NUMBER - 1", "RELATIVE RECORD NUMBER - 4"]);
        assert_eq!(out.last().unwrap(), "IDC0005I NUMBER OF RECORDS PROCESSED WAS 2");
    }

    #[test]
    fn an_empty_cluster_fails_to_open_and_an_empty_sequential_data_set_lists_nothing_with_code_4() {
        let (code, out) = run(&command(PrintFormat::Dump, First::Start, Last::End), KEYS, Vec::new());
        assert_eq!((code, out), (12, vec!["IDC3300I ERROR OPENING PAY.KSDS".to_string(), "IDC3351I ** VSAM OPEN RETURN CODE IS 160".into()]));
        let (code, out) = run(&command(PrintFormat::Dump, First::Start, Last::End), Kind::Nonvsam, Vec::new());
        assert_eq!((code, out), (4, vec!["LISTING OF DATA SET -PAY.KSDS".to_string(), "IDC0005I NUMBER OF RECORDS PROCESSED WAS 0".into()]));
    }

    #[test]
    fn a_range_that_holds_no_record_lists_nothing_with_code_4() {
        let chars = |s: &str| Key::Chars(s.into());
        for c in [command(PrintFormat::Dump, First::Start, Last::Key(chars("AA"))), command(PrintFormat::Dump, First::Start, Last::Count(0)), command(PrintFormat::Dump, First::Skip(5), Last::End)] {
            let (code, out) = run(&c, KEYS, staff());
            assert_eq!((code, out.last().unwrap().as_str()), (4, "IDC0005I NUMBER OF RECORDS PROCESSED WAS 0"), "{c:?}");
        }
    }

    #[test]
    fn a_start_past_the_last_record_is_a_positioning_error() {
        let ended = vec!["IDC3006I FUNCTION TERMINATED DUE TO BEGINNING POSITIONING ERROR".to_string()];
        assert_eq!(run(&command(PrintFormat::Dump, First::Key(Key::Chars("C".into())), Last::End), KEYS, staff()), (12, ended.clone()));
        assert_eq!(run(&command(PrintFormat::Dump, First::Skip(6), Last::End), Kind::Nonvsam, staff()), (12, ended));
    }

    #[test]
    fn a_key_longer_than_the_data_sets_or_on_a_data_set_without_keys_ends_the_command() {
        let long = command(PrintFormat::Dump, First::Start, Last::Key(Key::Chars("AB012".into())));
        assert_eq!(run(&long, KEYS, staff()), (12, vec!["IDC3302I ACTION ERROR ON PAY.KSDS".to_string(), "IDC3310I ** KEY SUPPLIED IS LONGER THAN KEY LENGTH OF DATA SET".into()]));
        let generic = command(PrintFormat::Dump, First::Key(Key::Chars("AB01*".into())), Last::End);
        assert_eq!(keys_listed(&run(&generic, KEYS, staff()).1)[0], "C1C2F0F1");
        for kind in [Kind::Entry, Kind::Numbered, Kind::Nonvsam] {
            let (code, out) = run(&command(PrintFormat::Dump, First::Key(Key::Chars("A".into())), Last::End), kind, staff());
            assert_eq!((code, out[1].as_str()), (12, "IDC3311I ** TYPE OF POSITIONING NOT SUPPORTED"));
        }
    }
}
