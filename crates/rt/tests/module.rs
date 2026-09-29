//! The codec macros from outside `rt`, the §4.7 worked example, and seeded pseudo-random round trips.

use std::collections::BTreeMap;

use ironwork_rt::module::codec::{Decode, Encode, Writer, decode_all};
use ironwork_rt::module::{Module, ModuleError, ModuleWriter, Section, StringTable};
use ironwork_rt::{codec_enum, codec_struct};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arith {
    Compat,
    Extend,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Trunc {
    Std,
    Opt,
    Bin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Numproc {
    Nopfd,
    Pfd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TruncCheck {
    Report,
    Silent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Options {
    arith: Arith,
    trunc: Trunc,
    numproc: Numproc,
    codepage: u16,
    trunc_check: TruncCheck,
}

codec_enum!(Arith { Compat = 0, Extend = 1 });
codec_enum!(Trunc { Std = 0, Opt = 1, Bin = 2 });
codec_enum!(Numproc { Nopfd = 0, Pfd = 1 });
codec_enum!(TruncCheck { Report = 0, Silent = 1 });
codec_struct!(Options { arith, trunc, numproc, codepage, trunc_check } check options_valid);

fn options_valid(options: &Options) -> Result<(), String> {
    match options.codepage {
        37 | 273 | 500 | 1047 | 1140 => Ok(()),
        other => Err(format!("CODEPAGE({other}) is not a page the tables carry")),
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Kind {
    Group,
    Alnum { justified: bool },
    Zoned { digits: u8, scale: i32, signed: bool },
    Float(u8),
    Pair(u16, char),
}

codec_enum!(Kind {
    Group = 0,
    Alnum { justified } = 1,
    Zoned { digits, scale, signed } = 3,
    Float(precision) = 6,
    Pair(width, fill) = 7,
});

#[derive(Clone, Debug, PartialEq)]
struct Entry {
    id: String,
    parent: Option<u32>,
    common: bool,
    entries: Vec<(String, u32)>,
    params: Vec<bool>,
    kind: Kind,
    offset: i64,
    image: Vec<u8>,
    keys: [u16; 2],
    notes: BTreeMap<u32, String>,
}

codec_struct!(Entry { id, parent, common, entries, params, kind, offset, image, keys, notes });

fn encoded<T: Encode>(value: &T) -> (Vec<u8>, StringTable) {
    let mut w = Writer::new();
    value.encode(&mut w);
    (w.take(), w.strings().clone())
}

fn payroll() -> Entry {
    Entry {
        id: "PAYROLL".into(),
        parent: None,
        common: false,
        entries: vec![("PAYROLL-ALT".into(), 3), ("PAYROLL".into(), 0)],
        params: vec![false, true],
        kind: Kind::Zoned { digits: 9, scale: -2, signed: true },
        offset: -1,
        image: vec![0xF0, 0x40, 0xC1],
        keys: [4, 65_535],
        notes: BTreeMap::from([(2, "B".into()), (1, "".into())]),
    }
}

#[test]
fn the_worked_example_encodes_as_documented() {
    let options = Options {
        arith: Arith::Extend,
        trunc: Trunc::Opt,
        numproc: Numproc::Nopfd,
        codepage: 1140,
        trunc_check: TruncCheck::Report,
    };
    let (bytes, strings) = encoded(&options);
    assert_eq!(bytes, [0x01, 0x01, 0x00, 0xF4, 0x08, 0x00]);
    assert_eq!(decode_all::<Options>("OPTIONS", &bytes, &strings), Ok(options));
}

#[test]
fn a_struct_declared_through_the_macro_round_trips() {
    let entry = payroll();
    let (bytes, strings) = encoded(&entry);
    assert_eq!(strings.iter().collect::<Vec<_>>(), ["PAYROLL", "PAYROLL-ALT", "", "B"]);
    assert_eq!(bytes[..4], [0, 0, 0, 2], "id, no parent, not common, two entries");
    assert_eq!(decode_all::<Entry>("DIRECTORY", &bytes, &strings), Ok(entry));
}

#[test]
fn a_failed_check_is_malformed_at_the_value() {
    let bytes = [0x07, 0x00, 0x01, 0x00, 0xE7, 0x07, 0x01];
    let err = decode_all::<(u8, Options)>("OPTIONS", &bytes, &StringTable::default());
    let reason = "CODEPAGE(999) is not a page the tables carry".to_owned();
    assert_eq!(err, Err(ModuleError::Malformed { section: "OPTIONS", offset: 1, reason }));
}

#[test]
fn an_unknown_tag_names_the_type_and_the_tag() {
    let strings = StringTable::default();
    let err = decode_all::<Kind>("LAYOUT", &[2], &strings);
    assert_eq!(err, Err(ModuleError::Malformed { section: "LAYOUT", offset: 0, reason: "Kind has no tag 2".into() }));
    let err = decode_all::<Trunc>("OPTIONS", &[0x83, 0x01], &strings).unwrap_err();
    assert_eq!(err.to_string(), "OPTIONS is malformed at byte 0: Trunc has no tag 131");
}

#[test]
fn enum_variants_are_their_tag_then_their_fields() {
    assert_eq!(encoded(&Kind::Group).0, [0]);
    assert_eq!(encoded(&Kind::Alnum { justified: true }).0, [1, 1]);
    assert_eq!(encoded(&Kind::Zoned { digits: 5, scale: -1, signed: false }).0, [3, 5, 1, 0]);
    assert_eq!(encoded(&Kind::Float(2)).0, [6, 2]);
    assert_eq!(encoded(&Kind::Pair(300, 'A')).0, [7, 0xAC, 0x02, 0x41]);
}

/// Knuth's MMIX LCG; callers take the high half, whose period is long.
struct Lcg(u64);

impl Lcg {
    fn step(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 32
    }

    fn below(&mut self, n: u64) -> u64 {
        self.step() % n
    }

    fn chance(&mut self, one_in: u64) -> bool {
        self.below(one_in) == 0
    }

    /// A value of a random bit width, so that every LEB128 length occurs.
    fn wide(&mut self) -> u64 {
        let word = (self.step() << 32) | self.step();
        word >> self.below(64)
    }

    fn text(&mut self) -> String {
        const POOL: [&str; 6] = ["", "A", "PAYROLL", "WS-TOTAL", "é€", "\u{10FFFF}"];
        if self.chance(2) {
            return POOL[self.below(6) as usize].to_owned();
        }
        (0..self.below(8)).filter_map(|_| char::from_u32(self.below(0x11_0000) as u32)).collect()
    }

    fn kind(&mut self) -> Kind {
        match self.below(5) {
            0 => Kind::Group,
            1 => Kind::Alnum { justified: self.chance(2) },
            2 => Kind::Zoned { digits: self.step() as u8, scale: self.wide() as i32, signed: self.chance(2) },
            3 => Kind::Float(self.step() as u8),
            _ => Kind::Pair(self.wide() as u16, char::from_u32(self.below(0xD800) as u32).unwrap_or('?')),
        }
    }

    fn entry(&mut self) -> Entry {
        Entry {
            id: self.text(),
            parent: (!self.chance(3)).then(|| self.wide() as u32),
            common: self.chance(2),
            entries: (0..self.below(4)).map(|_| (self.text(), self.wide() as u32)).collect(),
            params: (0..self.below(5)).map(|_| self.chance(2)).collect(),
            kind: self.kind(),
            offset: self.wide() as i64,
            image: (0..self.below(12)).map(|_| self.step() as u8).collect(),
            keys: [self.wide() as u16, self.wide() as u16],
            notes: (0..self.below(4)).map(|_| (self.wide() as u32, self.text())).collect(),
        }
    }
}

#[test]
fn pseudo_random_values_round_trip_and_encode_the_same_again() {
    let mut rng = Lcg(0x1BAD_C0B0);
    for _ in 0..2_000 {
        let entries: Vec<Entry> = (0..rng.below(4)).map(|_| rng.entry()).collect();
        let (bytes, strings) = encoded(&entries);
        let decoded = decode_all::<Vec<Entry>>("DIRECTORY", &bytes, &strings).unwrap();
        assert_eq!(decoded, entries);
        assert_eq!(encoded(&decoded), (bytes, strings));

        let numbers = ((rng.wide(), rng.wide() as i64), rng.wide() as i32, rng.wide() as u16, rng.wide() as i16);
        let (bytes, strings) = encoded(&numbers);
        assert_eq!(decode_all("TEST", &bytes, &strings), Ok(numbers));
    }
}

type Stringless = ((u64, i32, Option<bool>, Vec<u16>), ([i16; 2], char, BTreeMap<u8, u8>, Kind));

#[test]
fn random_bytes_decode_to_their_one_encoding_or_are_refused() {
    let mut rng = Lcg(7);
    let none = StringTable::default();
    let mut accepted = 0;
    for _ in 0..20_000 {
        let bytes: Vec<u8> = (0..rng.below(24))
            .map(|_| if rng.chance(3) { 0x80 | rng.step() as u8 } else { rng.below(4) as u8 })
            .collect();
        let _ = decode_all::<Vec<Entry>>("TEST", &bytes, &none);
        if let Ok(value) = decode_all::<Stringless>("TEST", &bytes, &none) {
            assert_eq!(encoded(&value).0, bytes);
            accepted += 1;
        }
    }
    assert!(accepted > 0);
}

fn module(entries: &[Entry], options: &Options) -> Vec<u8> {
    let mut w = ModuleWriter::new();
    w.section(Section::DIRECTORY, |w| entries.to_vec().encode(w));
    w.section(Section::OPTIONS, |w| options.encode(w));
    for section in &Section::ALL[3..] {
        w.section(*section, |w| w.count(0));
    }
    w.finish()
}

fn read_all(bytes: &[u8]) -> Result<(Vec<Entry>, Options), ModuleError> {
    let module = Module::read(bytes)?;
    for entry in module.sections() {
        module.body(entry.id)?;
    }
    let strings = module.strings()?;
    let mut directory = module.reader(Section::DIRECTORY, &strings)?;
    let entries = Vec::<Entry>::decode(&mut directory)?;
    directory.finish()?;
    let options = decode_all(Section::OPTIONS.name, module.body(Section::OPTIONS.id)?, &strings)?;
    Ok((entries, options))
}

fn options() -> Options {
    Options {
        arith: Arith::Compat,
        trunc: Trunc::Bin,
        numproc: Numproc::Pfd,
        codepage: 37,
        trunc_check: TruncCheck::Silent,
    }
}

#[test]
fn a_module_read_and_written_again_is_byte_identical() {
    let mut rng = Lcg(42);
    for _ in 0..200 {
        let entries: Vec<Entry> = (0..rng.below(5)).map(|_| rng.entry()).collect();
        let bytes = module(&entries, &options());
        let (decoded, options) = read_all(&bytes).unwrap();
        assert_eq!(decoded, entries);
        assert_eq!(module(&decoded, &options), bytes);
    }
}

#[test]
fn every_changed_byte_of_a_module_is_refused() {
    let mut rng = Lcg(99);
    let bytes = module(&[payroll(), rng.entry()], &options());
    assert!(read_all(&bytes).is_ok());
    for at in 0..bytes.len() {
        let mut changed = bytes.clone();
        changed[at] ^= 1 + rng.below(255) as u8;
        assert!(read_all(&changed).is_err(), "byte {at}");
    }
    for len in 0..bytes.len() {
        assert!(matches!(read_all(&bytes[..len]), Err(ModuleError::Truncated { .. })), "{len}");
    }
}

#[test]
fn corrupt_modules_are_refused_without_panicking() {
    let mut rng = Lcg(2026);
    let bytes = module(&[payroll(), payroll()], &options());
    for _ in 0..5_000 {
        let mut changed = bytes.clone();
        for _ in 0..1 + rng.below(4) {
            let at = rng.below(changed.len() as u64) as usize;
            changed[at] = rng.step() as u8;
        }
        if rng.chance(4) {
            changed.truncate(rng.below(changed.len() as u64) as usize);
        }
        let _ = read_all(&changed);
    }
}
