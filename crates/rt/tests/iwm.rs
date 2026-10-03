//! Whole load modules: written, read back, and refused when damaged.

mod common;

use common::payroll;
use ironwork_rt::bms::{Attrb, Field, Initial, Intensity, Map, Mapset, Mode, Protection};
use ironwork_rt::lir::{Program, SqlEntry, SqlStatement};
use ironwork_rt::module::{
    DirectoryEntry, Module, ModuleError, ModuleWriter, Section, Version, read, write, write_with,
};
use ironwork_rt::sql::fingerprint;
use numeric::Trunc;

fn report() -> Program {
    let mut program = payroll();
    program.symbols[0] = "REPORT".into();
    program.options.options.trunc = Trunc::Bin;
    let first = program.symbols.len() as u32;
    program.symbols.extend(["COMMIT", "ROLLBACK"].map(String::from));
    let entry = |ordinal, verb, statement, text: &str| SqlEntry {
        ordinal,
        verb,
        statement,
        text: verb,
        fingerprint: fingerprint(text),
        with_hold: false,
    };
    program.sql = vec![entry(1, first, SqlStatement::Commit, "COMMIT"), entry(2, first + 1, SqlStatement::Rollback, "ROLLBACK")];
    program
}

fn two() -> Vec<Program> {
    vec![payroll(), report()]
}

fn damaged(bytes: &[u8], at: usize) -> Vec<u8> {
    let mut copy = bytes.to_vec();
    copy[at] ^= 0x01;
    copy
}

#[test]
fn two_programs_round_trip() {
    let programs = two();
    let loaded = read(&write(&programs)).unwrap();
    assert_eq!(loaded.programs, programs);
    let ids: Vec<_> = loaded.directory.iter().map(|d| d.id.as_str()).collect();
    assert_eq!(ids, ["PAYROLL", "REPORT"]);
    assert!(loaded.directory.iter().all(|d| d.parent.is_none() && !d.common && d.dynamic && d.entries.is_empty()));
}

#[test]
fn a_caller_s_directory_is_kept() {
    let programs = two();
    let mut directory: Vec<_> = programs.iter().map(DirectoryEntry::top_level).collect();
    directory[1].parent = Some(0);
    directory[1].common = true;
    directory[1].entries = vec![("ALT".into(), 1)];
    directory[1].params = vec![true];
    directory[1].external = Some("Report".into());
    let loaded = read(&write_with(&programs, &directory, &[]).unwrap()).unwrap();
    assert_eq!(loaded.directory, directory);
    assert_eq!(loaded.directory[1].load_name(), "Report");
    assert_eq!(loaded.directory[0].load_name(), "PAYROLL");
}

#[test]
fn a_directory_that_disagrees_with_the_programs_is_refused_by_the_writer() {
    let programs = two();
    let base: Vec<_> = programs.iter().map(DirectoryEntry::top_level).collect();
    let refused = |directory: &[DirectoryEntry]| match write_with(&programs, directory, &[]) {
        Err(ModuleError::Malformed { section: "DIRECTORY", .. }) => (),
        other => panic!("{other:?}"),
    };
    refused(&base[..1]);
    refused(&[DirectoryEntry { id: "OTHER".into(), ..base[0].clone() }, base[1].clone()]);
    refused(&[DirectoryEntry { parent: Some(0), ..base[0].clone() }, base[1].clone()]);
    refused(&[base[0].clone(), DirectoryEntry { entries: vec![("X".into(), 99)], ..base[1].clone() }]);
}

#[test]
fn the_same_programs_give_the_same_bytes() {
    let first = write(&two());
    assert_eq!(first, write(&two()));
    let loaded = read(&first).unwrap();
    assert_eq!(write(&loaded.programs), first);
}

#[test]
fn an_empty_module_round_trips() {
    let bytes = write(&[]);
    let loaded = read(&bytes).unwrap();
    assert!(loaded.programs.is_empty() && loaded.directory.is_empty());
    assert_eq!(Module::read(&bytes).unwrap().sections().len(), Section::ALL.len());
}

#[test]
fn every_section_is_written_in_order() {
    let bytes = write(&two());
    let ids: Vec<_> = Module::read(&bytes).unwrap().sections().iter().map(|e| e.id).collect();
    assert_eq!(ids, [1, 2, 3, 4, 5, 6, 7, 8]);
}

#[test]
fn a_corrupt_byte_in_each_section_is_refused_by_that_section_s_checksum() {
    let bytes = write(&two());
    let module = Module::read(&bytes).unwrap();
    for entry in module.sections() {
        assert!(entry.length > 0, "{}", entry.id);
        let middle = (entry.offset + entry.length / 2) as usize;
        match read(&damaged(&bytes, middle)) {
            Err(ModuleError::SectionChecksum { id, .. }) => assert_eq!(id, entry.id),
            other => panic!("section {}: {other:?}", entry.id),
        }
    }
}

#[test]
fn a_corrupt_header_or_table_byte_is_refused_by_the_header_checksum() {
    let bytes = write(&two());
    for at in [16, 32, 40, 60] {
        assert!(matches!(read(&damaged(&bytes, at)), Err(ModuleError::HeaderChecksum { .. })), "{at}");
    }
}

#[test]
fn a_truncated_file_is_refused_at_every_length() {
    let bytes = write(&two());
    for len in 0..bytes.len() {
        assert!(matches!(read(&bytes[..len]), Err(ModuleError::Truncated { .. })), "{len}");
    }
    let mut longer = bytes.clone();
    longer.push(0);
    assert!(matches!(read(&longer), Err(ModuleError::TrailingBytes { .. })));
}

#[test]
fn a_file_without_the_magic_is_not_a_module() {
    assert_eq!(read(b"PK\x03\x04 not a module at all, but longer than the header is"), Err(ModuleError::NotAModule));
}

#[test]
fn another_format_version_is_refused() {
    let bytes = write(&two());
    let mut major = bytes.clone();
    major[8] = 1;
    let error = read(&major).unwrap_err();
    assert_eq!(error, ModuleError::Version(Version { major: 1, minor: 2 }));
    assert_eq!(error.to_string(), "load module format 1.2; this ironwork reads 0.2. Compile the source again");
    let mut minor = bytes;
    minor[10] = 1;
    assert_eq!(read(&minor), Err(ModuleError::Version(Version { major: 0, minor: 1 })));
}

/// A module whose sections each hold a count: the directory's, OPTIONS' and BMS's as given, else zero.
fn with(directory: usize, options: usize, bms: usize) -> Vec<u8> {
    let mut m = ModuleWriter::new();
    let counts = [directory, options, 0, 0, 0, bms, 0];
    for (section, count) in Section::ALL[1..].iter().zip(counts) {
        m.section(*section, |w| w.count(count));
    }
    m.finish()
}

#[test]
fn a_section_that_disagrees_with_the_directory_is_malformed() {
    assert!(read(&with(0, 0, 0)).is_ok());
    match read(&with(0, 1, 0)) {
        Err(ModuleError::Malformed { section: "OPTIONS", .. }) => (),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_mapset_count_past_the_section_s_bytes_is_malformed() {
    assert!(matches!(read(&with(0, 0, 1)), Err(ModuleError::Malformed { section: "BMS", .. })));
}

fn mapset(name: &str) -> Mapset {
    let attrb = Attrb { protection: Protection::Unprot, numeric: true, intensity: Intensity::Brt, detectable: false, cursor: true, fset: false };
    let field = |name: Option<&str>, initial| Field {
        name: name.map(str::to_owned),
        line: 2,
        column: 10,
        length: 5,
        attrb,
        initial,
        picin: Some("9(5)".into()),
        picout: None,
        occurs: 1,
        group: None,
        justify_right: true,
        fill_zero: false,
        color: Some("RED".into()),
        hilight: None,
    };
    let fields = vec![field(Some("AMOUNT"), None), field(None, Some(Initial::Text("Amount:".into()))), field(None, Some(Initial::Bytes(vec![0xC1, 0x00])))];
    let map = Map { name: "MAP1".into(), lines: 24, columns: 80, line: 1, column: 1, ctrl: vec!["FREEKB".into()], tioapfx: true, dsatts: vec!["COLOR".into()], fields };
    Mapset { name: name.into(), mode: Mode::InOut, ctrl: Vec::new(), maps: vec![map] }
}

#[test]
fn mapsets_round_trip_in_name_order_and_are_refused_out_of_it() {
    let programs = two();
    let directory: Vec<_> = programs.iter().map(DirectoryEntry::top_level).collect();
    let mapsets = [mapset("ACCTSET"), mapset("MENUSET")];
    let bytes = write_with(&programs, &directory, &mapsets).unwrap();
    assert_eq!(read(&bytes).unwrap().mapsets, mapsets);
    for wrong in [[mapset("MENUSET"), mapset("ACCTSET")], [mapset("ACCTSET"), mapset("ACCTSET")]] {
        assert!(matches!(write_with(&programs, &directory, &wrong), Err(ModuleError::Malformed { section: "BMS", .. })));
    }
}
