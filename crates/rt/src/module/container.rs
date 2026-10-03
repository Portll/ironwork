use super::ModuleError;
use super::codec::{Reader, Writer};
use super::crc::{crc32, extend};
use super::strings::StringTable;

pub const MAGIC: [u8; 8] = [0x89, b'I', b'W', b'M', 0x0D, 0x0A, 0x1A, 0x0A];

/// Flag bit 0 of a section entry: a reader that does not know the section skips it.
pub const OPTIONAL: u32 = 1;

/// The first id of the extension sections, which are always written optional.
pub const EXTENSIONS: u32 = 0x8000;

const HEADER: usize = 32;
const ENTRY: usize = 28;
const HEADER_CRC: usize = 28;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u16,
    pub minor: u16,
}

impl Version {
    pub const CURRENT: Self = Self { major: 0, minor: 5 };

    /// Whether this reader reads `found`: the same major, and before 1.0 the same minor (§8.1).
    pub const fn reads(self, found: Self) -> bool {
        found.major == self.major && (self.major != 0 || found.minor == self.minor)
    }
}

/// A section this version knows (load-module.md §3.4). Every one is required.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Section {
    pub id: u32,
    pub name: &'static str,
}

impl Section {
    pub const STRINGS: Self = Self { id: 1, name: "STRINGS" };
    pub const DIRECTORY: Self = Self { id: 2, name: "DIRECTORY" };
    pub const OPTIONS: Self = Self { id: 3, name: "OPTIONS" };
    pub const LAYOUT: Self = Self { id: 4, name: "LAYOUT" };
    pub const LIR: Self = Self { id: 5, name: "LIR" };
    pub const SQL: Self = Self { id: 6, name: "SQL" };
    pub const BMS: Self = Self { id: 7, name: "BMS" };
    pub const DEBUG: Self = Self { id: 8, name: "DEBUG" };

    pub const ALL: [Self; 8] =
        [Self::STRINGS, Self::DIRECTORY, Self::OPTIONS, Self::LAYOUT, Self::LIR, Self::SQL, Self::BMS, Self::DEBUG];

    pub fn by_id(id: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.id == id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SectionEntry {
    pub id: u32,
    pub flags: u32,
    pub offset: u64,
    pub length: u64,
    pub crc: u32,
}

impl SectionEntry {
    pub fn name(&self) -> Option<&'static str> {
        Section::by_id(self.id).map(|s| s.name)
    }

    pub fn optional(&self) -> bool {
        self.flags & OPTIONAL != 0
    }
}

fn le<const N: usize>(bytes: &[u8], at: usize) -> Option<[u8; N]> {
    bytes.get(at..)?.first_chunk().copied()
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    le(bytes, at).map(u16::from_le_bytes)
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    le(bytes, at).map(u32::from_le_bytes)
}

fn u64_at(bytes: &[u8], at: usize) -> Option<u64> {
    le(bytes, at).map(u64::from_le_bytes)
}

/// Lays sections out as given, with their table, checksums and header.
fn assemble(version: Version, features: u32, sections: &[(u32, u32, &[u8])]) -> Vec<u8> {
    let table_end = HEADER + sections.len() * ENTRY;
    let file_len = table_end + sections.iter().map(|s| s.2.len()).sum::<usize>();
    let mut out = Vec::with_capacity(file_len);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&version.major.to_le_bytes());
    out.extend_from_slice(&version.minor.to_le_bytes());
    out.extend_from_slice(&features.to_le_bytes());
    out.extend_from_slice(&(sections.len() as u32).to_le_bytes());
    out.extend_from_slice(&(file_len as u64).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    let mut offset = table_end as u64;
    for &(id, flags, body) in sections {
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(&(body.len() as u64).to_le_bytes());
        out.extend_from_slice(&crc32(body).to_le_bytes());
        offset += body.len() as u64;
    }
    let crc = extend(crc32(&out[..HEADER_CRC]), &out[HEADER..]);
    out[HEADER_CRC..HEADER].copy_from_slice(&crc.to_le_bytes());
    for &(_, _, body) in sections {
        out.extend_from_slice(body);
    }
    out
}

/// Builds section bodies in id order; `finish` puts the string table first and adds the checksums.
#[derive(Debug, Default)]
pub struct ModuleWriter {
    writer: Writer,
    sections: Vec<(u32, u32, Vec<u8>)>,
}

impl ModuleWriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Panics if `section` does not follow the last one written, or is `STRINGS`, which `finish` writes.
    pub fn section(&mut self, section: Section, build: impl FnOnce(&mut Writer)) {
        self.push(section.id, 0, build);
    }

    /// An optional section a reader that does not know `id` skips. Panics if `id` is below `EXTENSIONS`.
    pub fn extension(&mut self, id: u32, build: impl FnOnce(&mut Writer)) {
        assert!(id >= EXTENSIONS, "extension section {id:#x} is below {EXTENSIONS:#x}");
        self.push(id, OPTIONAL, build);
    }

    fn push(&mut self, id: u32, flags: u32, build: impl FnOnce(&mut Writer)) {
        let last = self.sections.last().map_or(Section::STRINGS.id, |s| s.0);
        assert!(id > last, "section {id:#x} written after section {last:#x}");
        build(&mut self.writer);
        let body = self.writer.take();
        self.sections.push((id, flags, body));
    }

    /// Panics if a required section was not written.
    pub fn finish(self) -> Vec<u8> {
        for required in &Section::ALL[1..] {
            assert!(self.sections.iter().any(|s| s.0 == required.id), "section {} was not written", required.name);
        }
        let strings = self.writer.strings().encode();
        let mut sections = vec![(Section::STRINGS.id, 0, strings.as_slice())];
        sections.extend(self.sections.iter().map(|(id, flags, body)| (*id, *flags, body.as_slice())));
        assemble(Version::CURRENT, 0, &sections)
    }
}

/// A module whose header and table are checked; a section's checksum is checked when it is read.
#[derive(Clone, Debug)]
pub struct Module<'a> {
    bytes: &'a [u8],
    version: Version,
    sections: Vec<SectionEntry>,
}

impl<'a> Module<'a> {
    /// Checks magic, version, length, header checksum, features, then the table, in that order.
    pub fn read(bytes: &'a [u8]) -> Result<Self, ModuleError> {
        let actual = bytes.len() as u64;
        let shown = bytes.len().min(MAGIC.len());
        if bytes[..shown] != MAGIC[..shown] {
            return Err(ModuleError::NotAModule);
        }
        let truncated = ModuleError::Truncated { expected: HEADER as u64, actual };
        let header = (
            u16_at(bytes, 8),
            u16_at(bytes, 10),
            u32_at(bytes, 12),
            u32_at(bytes, 16),
            u64_at(bytes, 20),
            u32_at(bytes, HEADER_CRC),
        );
        let (Some(major), Some(minor), Some(features), Some(count), Some(file_len), Some(stored)) = header else {
            return Err(truncated);
        };
        let version = Version { major, minor };
        if !Version::CURRENT.reads(version) {
            return Err(ModuleError::Version(version));
        }
        if actual < file_len {
            return Err(ModuleError::Truncated { expected: file_len, actual });
        }
        if actual > file_len {
            return Err(ModuleError::TrailingBytes { expected: file_len, actual });
        }
        let table_end = HEADER as u64 + u64::from(count) * ENTRY as u64;
        let table = bytes.get(HEADER..table_end.min(actual) as usize).unwrap_or_default();
        let computed = extend(crc32(&bytes[..HEADER_CRC]), table);
        if computed != stored {
            return Err(ModuleError::HeaderChecksum { computed, stored });
        }
        if features != 0 {
            return Err(ModuleError::Feature(features));
        }
        let malformed = |offset, reason| ModuleError::Malformed { section: "section table", offset, reason };
        if table_end > file_len {
            return Err(malformed(
                16,
                format!("{count} sections need a table to byte {table_end}, past the end at {file_len}"),
            ));
        }

        let mut sections: Vec<SectionEntry> = Vec::with_capacity(count as usize);
        let mut next = table_end;
        for at in (HEADER..table_end as usize).step_by(ENTRY) {
            let fields = (
                u32_at(bytes, at),
                u32_at(bytes, at + 4),
                u64_at(bytes, at + 8),
                u64_at(bytes, at + 16),
                u32_at(bytes, at + 24),
            );
            let (Some(id), Some(flags), Some(offset), Some(length), Some(crc)) = fields else {
                return Err(truncated);
            };
            let entry = SectionEntry { id, flags, offset, length, crc };
            if let Some(last) = sections.last()
                && last.id >= id
            {
                return Err(malformed(at, format!("section {id:#x} follows section {:#x}", last.id)));
            }
            if offset != next {
                return Err(malformed(at, format!("section {id:#x} begins at byte {offset}, not {next}")));
            }
            next = match offset.checked_add(length) {
                Some(end) if end <= file_len => end,
                _ => return Err(malformed(at, format!("section {id:#x} of {length} bytes runs past the end"))),
            };
            if flags & !OPTIONAL != 0 {
                return Err(malformed(at, format!("section {id:#x} has flags {flags:#x}")));
            }
            match (Section::by_id(id), entry.optional()) {
                (Some(known), true) => {
                    return Err(malformed(at, format!("required section {} is flagged optional", known.name)));
                }
                (None, false) => return Err(ModuleError::UnknownSection(id)),
                _ => {}
            }
            sections.push(entry);
        }
        if next != file_len {
            return Err(malformed(HEADER, format!("the sections end at byte {next}, before the end at {file_len}")));
        }
        if let Some(missing) = Section::ALL.iter().find(|s| !sections.iter().any(|e| e.id == s.id)) {
            return Err(ModuleError::MissingSection(missing.id));
        }
        Ok(Self { bytes, version, sections })
    }

    pub fn version(&self) -> Version {
        self.version
    }

    /// The section table, in id order, with any optional section this reader does not know.
    pub fn sections(&self) -> &[SectionEntry] {
        &self.sections
    }

    /// A section's bytes, once its checksum matches.
    pub fn body(&self, id: u32) -> Result<&'a [u8], ModuleError> {
        let entry = self.sections.iter().find(|e| e.id == id).ok_or(ModuleError::MissingSection(id))?;
        let body = usize::try_from(entry.offset)
            .ok()
            .zip(usize::try_from(entry.length).ok())
            .and_then(|(start, len)| self.bytes.get(start..start.checked_add(len)?))
            .ok_or(ModuleError::Truncated {
                expected: entry.offset.saturating_add(entry.length),
                actual: self.bytes.len() as u64,
            })?;
        let computed = crc32(body);
        if computed != entry.crc {
            return Err(ModuleError::SectionChecksum { id, computed, stored: entry.crc });
        }
        Ok(body)
    }

    pub fn strings(&self) -> Result<StringTable, ModuleError> {
        StringTable::decode(self.body(Section::STRINGS.id)?)
    }

    pub fn reader<'r>(&self, section: Section, strings: &'r StringTable) -> Result<Reader<'r>, ModuleError>
    where
        'a: 'r,
    {
        Ok(Reader::new(section.name, self.body(section.id)?, strings))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module::codec::{Decode, Encode};

    const TABLE_END: usize = HEADER + 8 * ENTRY;

    fn names(texts: &[&str]) -> Vec<String> {
        texts.iter().map(|s| (*s).to_owned()).collect()
    }

    /// A module whose DIRECTORY holds `ids`, and whose other sections each hold a zero count.
    fn sample(ids: &[String]) -> Vec<u8> {
        let mut w = ModuleWriter::new();
        w.section(Section::DIRECTORY, |w| ids.to_vec().encode(w));
        for section in &Section::ALL[2..] {
            w.section(*section, |w| w.count(0));
        }
        w.finish()
    }

    /// Every known section, each with a zero count for its body.
    fn plain() -> Vec<(u32, u32, &'static [u8])> {
        Section::ALL.iter().map(|s| (s.id, 0, &[0u8][..])).collect()
    }

    fn read(bytes: &[u8]) -> Result<Vec<u32>, ModuleError> {
        Module::read(bytes).map(|m| m.sections().iter().map(|e| e.id).collect())
    }

    fn reseal(bytes: &mut [u8]) {
        let count = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
        let end = (HEADER + count * ENTRY).min(bytes.len());
        let crc = extend(crc32(&bytes[..HEADER_CRC]), &bytes[HEADER..end]);
        bytes[HEADER_CRC..HEADER].copy_from_slice(&crc.to_le_bytes());
    }

    fn table_malformed(result: Result<Vec<u32>, ModuleError>) -> String {
        match result {
            Err(ModuleError::Malformed { section: "section table", reason, .. }) => reason,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_module_lists_its_sections_and_decodes_them() {
        let ids = names(&["PAYROLL", "SUB", "PAYROLL"]);
        let bytes = sample(&ids);
        let module = Module::read(&bytes).unwrap();
        assert_eq!(module.version(), Version::CURRENT);
        let listed: Vec<_> = module.sections().iter().map(|e| (e.id, e.name(), e.optional())).collect();
        let expected: Vec<_> = Section::ALL.iter().map(|s| (s.id, Some(s.name), false)).collect();
        assert_eq!(listed, expected);
        let strings = module.strings().unwrap();
        assert_eq!(strings.iter().collect::<Vec<_>>(), ["PAYROLL", "SUB"]);
        let mut r = module.reader(Section::DIRECTORY, &strings).unwrap();
        assert_eq!(Vec::<String>::decode(&mut r), Ok(ids));
        assert_eq!(r.finish(), Ok(()));
        assert_eq!(module.body(Section::DEBUG.id), Ok(&[0u8][..]));
    }

    #[test]
    fn the_header_and_table_have_the_documented_layout() {
        let bytes = sample(&names(&["A"]));
        assert_eq!(bytes[..8], [0x89, 0x49, 0x57, 0x4D, 0x0D, 0x0A, 0x1A, 0x0A]);
        assert_eq!(bytes[8..20], [0, 0, 5, 0, 0, 0, 0, 0, 8, 0, 0, 0]);
        assert_eq!(u64_at(&bytes, 20), Some(bytes.len() as u64));
        assert_eq!(u32_at(&bytes, HEADER_CRC), Some(extend(crc32(&bytes[..28]), &bytes[32..TABLE_END])));
        let strings_body = [1, 1, b'A'];
        assert_eq!(u32_at(&bytes, 32), Some(1));
        assert_eq!(u32_at(&bytes, 36), Some(0));
        assert_eq!(u64_at(&bytes, 40), Some(TABLE_END as u64));
        assert_eq!(u64_at(&bytes, 48), Some(3));
        assert_eq!(u32_at(&bytes, 56), Some(crc32(&strings_body)));
        assert_eq!(bytes[TABLE_END..TABLE_END + 3], strings_body);
        assert_eq!(u64_at(&bytes, 32 + ENTRY + 8), Some(TABLE_END as u64 + 3));
        assert_eq!(bytes.len(), TABLE_END + 3 + 2 + 6);
    }

    #[test]
    fn reading_and_writing_again_gives_the_same_bytes() {
        let bytes = sample(&names(&["MAIN", "", "SUB", "MAIN"]));
        assert_eq!(sample(&names(&["MAIN", "", "SUB", "MAIN"])), bytes);
        let module = Module::read(&bytes).unwrap();
        let strings = module.strings().unwrap();
        let mut r = module.reader(Section::DIRECTORY, &strings).unwrap();
        let decoded = Vec::<String>::decode(&mut r).unwrap();
        assert_eq!(sample(&decoded), bytes);
    }

    #[test]
    fn every_strict_prefix_is_truncated() {
        let bytes = sample(&names(&["PAYROLL"]));
        for len in 0..bytes.len() {
            assert!(matches!(read(&bytes[..len]), Err(ModuleError::Truncated { .. })), "{len}");
        }
        assert_eq!(read(&bytes[..100]), Err(ModuleError::Truncated { expected: bytes.len() as u64, actual: 100 }));
        assert_eq!(read(&bytes[..20]), Err(ModuleError::Truncated { expected: 32, actual: 20 }));
    }

    #[test]
    fn a_bad_magic_is_not_a_module() {
        let bytes = sample(&names(&["A"]));
        let mut changed = bytes.clone();
        changed[0] = 0x09;
        assert_eq!(read(&changed), Err(ModuleError::NotAModule));
        let text_mode: Vec<u8> = [&bytes[..4], &bytes[5..]].concat();
        assert_eq!(read(&text_mode), Err(ModuleError::NotAModule));
        assert_eq!(read(b"IDENTIFICATION DIVISION."), Err(ModuleError::NotAModule));
        assert_eq!(read(b"\x89IX"), Err(ModuleError::NotAModule));
        assert_eq!(ModuleError::NotAModule.to_string(), "not an ironwork load module");
    }

    #[test]
    fn a_longer_file_has_trailing_bytes() {
        let mut bytes = sample(&names(&["A"]));
        let len = bytes.len() as u64;
        bytes.push(0);
        assert_eq!(read(&bytes), Err(ModuleError::TrailingBytes { expected: len, actual: len + 1 }));
    }

    #[test]
    fn another_major_or_before_one_another_minor_is_refused() {
        for version in [Version { major: 1, minor: 0 }, Version { major: 0, minor: 4 }, Version { major: 0, minor: 6 }]
        {
            assert_eq!(read(&assemble(version, 0, &plain())), Err(ModuleError::Version(version)));
        }
        let message = ModuleError::Version(Version { major: 2, minor: 0 }).to_string();
        assert_eq!(message, "load module format 2.0; this ironwork reads 0.5. Compile the source again");
        let reader = Version { major: 1, minor: 2 };
        assert!(reader.reads(Version { major: 1, minor: 0 }));
        assert!(reader.reads(Version { major: 1, minor: 5 }));
        assert!(!reader.reads(Version { major: 2, minor: 0 }));
        assert!(!reader.reads(Version { major: 0, minor: 1 }));
        assert!(Version::CURRENT.reads(Version::CURRENT));
    }

    #[test]
    fn a_changed_header_or_table_byte_is_a_bad_header_checksum() {
        let bytes = sample(&names(&["A"]));
        for at in (12..20).chain(HEADER_CRC..TABLE_END) {
            let mut changed = bytes.clone();
            changed[at] ^= 0x10;
            assert!(matches!(read(&changed), Err(ModuleError::HeaderChecksum { .. })), "byte {at}");
        }
        let mut changed = bytes.clone();
        changed[20] ^= 1;
        assert!(matches!(read(&changed), Err(ModuleError::Truncated { .. } | ModuleError::TrailingBytes { .. })));
    }

    #[test]
    fn a_set_feature_bit_is_refused_by_name() {
        assert_eq!(read(&assemble(Version::CURRENT, 4, &plain())), Err(ModuleError::Feature(4)));
        assert_eq!(
            ModuleError::Feature(4).to_string(),
            "load module needs features 0x00000004, which this ironwork lacks"
        );
    }

    #[test]
    fn a_changed_section_byte_is_found_when_the_section_is_read() {
        let bytes = sample(&names(&["A"]));
        let module = Module::read(&bytes).unwrap();
        let layout = module.sections()[3];
        let mut changed = bytes.clone();
        changed[layout.offset as usize] ^= 0x01;
        let module = Module::read(&changed).unwrap();
        let err = module.body(Section::LAYOUT.id).unwrap_err();
        let computed = crc32(&[1]);
        assert_eq!(err, ModuleError::SectionChecksum { id: 4, computed, stored: layout.crc });
        assert_eq!(
            err.to_string(),
            format!("section LAYOUT is corrupt (checksum {computed:08X}, expected {:08X})", layout.crc)
        );
        assert!(module.body(Section::LIR.id).is_ok());
        assert!(module.strings().is_ok());
    }

    #[test]
    fn an_unknown_section_is_skipped_only_if_optional() {
        let mut sections = plain();
        sections.push((9, 0, &[1, 2]));
        assert_eq!(read(&assemble(Version::CURRENT, 0, &sections)), Err(ModuleError::UnknownSection(9)));
        assert_eq!(ModuleError::UnknownSection(9).to_string(), "required section 0x9 is unknown to this ironwork");

        let mut sections = plain();
        sections.push((9, OPTIONAL, &[1, 2]));
        sections.push((EXTENSIONS + 1, OPTIONAL, &[3]));
        let bytes = assemble(Version::CURRENT, 0, &sections);
        assert_eq!(read(&bytes), Ok(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, EXTENSIONS + 1]));
        let module = Module::read(&bytes).unwrap();
        assert_eq!(module.sections()[8].name(), None);
        assert_eq!(module.body(9), Ok(&[1u8, 2][..]));

        let mut w = ModuleWriter::new();
        for section in &Section::ALL[1..] {
            w.section(*section, |w| w.count(0));
        }
        w.extension(EXTENSIONS, |w| w.string("NOTE"));
        let module_bytes = w.finish();
        let module = Module::read(&module_bytes).unwrap();
        assert!(module.sections()[8].optional());
        assert_eq!(module.strings().unwrap().get(0), Some("NOTE"));
    }

    #[test]
    fn flags_must_match_what_the_reader_knows() {
        let mut sections = plain();
        sections[3].1 = OPTIONAL;
        assert_eq!(
            table_malformed(read(&assemble(Version::CURRENT, 0, &sections))),
            "required section LAYOUT is flagged optional"
        );
        let mut sections = plain();
        sections[2].1 = 2;
        assert_eq!(table_malformed(read(&assemble(Version::CURRENT, 0, &sections))), "section 0x3 has flags 0x2");
    }

    #[test]
    fn a_missing_required_section_is_refused() {
        let sections = &plain()[..7];
        assert_eq!(read(&assemble(Version::CURRENT, 0, sections)), Err(ModuleError::MissingSection(8)));
        assert_eq!(ModuleError::MissingSection(8).to_string(), "required section DEBUG is missing");
        assert_eq!(read(&assemble(Version::CURRENT, 0, &[])), Err(ModuleError::MissingSection(1)));
        let bytes = sample(&[]);
        assert_eq!(Module::read(&bytes).unwrap().body(9), Err(ModuleError::MissingSection(9)));
    }

    #[test]
    fn only_the_canonical_layout_is_read() {
        let mut sections = plain();
        sections.swap(4, 5);
        assert_eq!(table_malformed(read(&assemble(Version::CURRENT, 0, &sections))), "section 0x5 follows section 0x6");
        let mut sections = plain();
        sections.insert(4, (4, 0, &[0]));
        assert_eq!(table_malformed(read(&assemble(Version::CURRENT, 0, &sections))), "section 0x4 follows section 0x4");

        let bytes = assemble(Version::CURRENT, 0, &plain());
        let mut gap = bytes.clone();
        gap[32 + ENTRY + 8] += 1;
        reseal(&mut gap);
        assert_eq!(
            table_malformed(read(&gap)),
            format!("section 0x2 begins at byte {}, not {}", TABLE_END + 2, TABLE_END + 1)
        );
        let mut long = bytes.clone();
        long[32 + 7 * ENTRY + 16] += 1;
        reseal(&mut long);
        assert_eq!(table_malformed(read(&long)), "section 0x8 of 2 bytes runs past the end");
        let mut short = bytes.clone();
        short[32 + 7 * ENTRY + 16] -= 1;
        reseal(&mut short);
        assert_eq!(
            table_malformed(read(&short)),
            format!("the sections end at byte {}, before the end at {}", TABLE_END + 7, TABLE_END + 8)
        );
        let mut many = bytes;
        many[16] = 200;
        reseal(&mut many);
        assert_eq!(
            table_malformed(read(&many)),
            format!("200 sections need a table to byte {}, past the end at {}", HEADER + 200 * ENTRY, TABLE_END + 8)
        );
    }

    #[test]
    #[should_panic(expected = "section 0x3 written after section 0x4")]
    fn the_writer_takes_sections_in_id_order() {
        let mut w = ModuleWriter::new();
        w.section(Section::LAYOUT, |_| {});
        w.section(Section::OPTIONS, |_| {});
    }

    #[test]
    #[should_panic(expected = "section SQL was not written")]
    fn the_writer_writes_every_required_section() {
        let mut w = ModuleWriter::new();
        for section in
            [Section::DIRECTORY, Section::OPTIONS, Section::LAYOUT, Section::LIR, Section::BMS, Section::DEBUG]
        {
            w.section(section, |w| w.count(0));
        }
        w.finish();
    }
}
