//! The load module (`.iwm`) of docs/load-module.md: its container, encoding rules and codec, and
//! the modules a run loads programs from.

pub mod codec;
mod container;
pub mod crc;
pub mod leb;
mod library;
mod programs;
mod strings;

use std::fmt;

pub use codec::{Decode, Encode, Reader, Writer};
pub use container::{EXTENSIONS, MAGIC, Module, ModuleWriter, OPTIONAL, Section, SectionEntry, Version};
pub use library::{Check, Modules, member_name};
pub use programs::{DirectoryEntry, LayoutRecord, LirRecord, LoadedModule, OptionRecords, SourceFile, read, write, write_module, write_with};
pub use strings::StringTable;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModuleError {
    /// The file does not start with the magic.
    NotAModule,
    /// A format version this reader does not read (§8.1).
    Version(Version),
    /// Required feature bits this reader does not know.
    Feature(u32),
    /// The file is shorter than its header, or than the length its header gives.
    Truncated {
        expected: u64,
        actual: u64,
    },
    /// The file is longer than the length its header gives.
    TrailingBytes {
        expected: u64,
        actual: u64,
    },
    HeaderChecksum {
        computed: u32,
        stored: u32,
    },
    SectionChecksum {
        id: u32,
        computed: u32,
        stored: u32,
    },
    /// A section id this reader does not know, without the optional flag.
    UnknownSection(u32),
    MissingSection(u32),
    /// `offset` counts from the start of the file in the header and table, else of the section body.
    Malformed {
        section: &'static str,
        offset: usize,
        reason: String,
    },
}

fn section_name(id: u32) -> String {
    Section::by_id(id).map_or_else(|| format!("{id:#x}"), |s| s.name.to_owned())
}

impl fmt::Display for ModuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAModule => write!(f, "not an ironwork load module"),
            Self::Version(found) => {
                let (oldest, current) = (Version::OLDEST_READABLE, Version::CURRENT);
                write!(f, "load module format {}.{}; this ironwork reads ", found.major, found.minor)?;
                match current.major {
                    0 if oldest == current => write!(f, "0.{}", current.minor)?,
                    0 => write!(f, "0.{} to 0.{}", oldest.minor, current.minor)?,
                    major => write!(f, "{major}.x")?,
                }
                write!(f, ". Compile the source again")
            }
            Self::Feature(bits) => write!(f, "load module needs features {bits:#010x}, which this ironwork lacks"),
            Self::Truncated { expected, actual } => write!(f, "truncated: {actual} bytes of {expected}"),
            Self::TrailingBytes { expected, actual } => {
                write!(f, "{} bytes after the end of the module at {expected}", actual.saturating_sub(*expected))
            }
            Self::HeaderChecksum { computed, stored } => {
                write!(f, "header is corrupt (checksum {computed:08X}, expected {stored:08X})")
            }
            Self::SectionChecksum { id, computed, stored } => {
                write!(f, "section {} is corrupt (checksum {computed:08X}, expected {stored:08X})", section_name(*id))
            }
            Self::UnknownSection(id) => write!(f, "required section {id:#x} is unknown to this ironwork"),
            Self::MissingSection(id) => write!(f, "required section {} is missing", section_name(*id)),
            Self::Malformed { section, offset, reason } => {
                write!(f, "{section} is malformed at byte {offset}: {reason}")
            }
        }
    }
}

impl std::error::Error for ModuleError {}
