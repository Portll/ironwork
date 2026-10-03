//! EXTERNAL and GLOBAL (lir.md §9.16): the records and file connectors a program shares with the
//! run unit and with the programs containing it, bound each time it is activated, and what a
//! program that contains others gives them (assumptions C69, C180 and C181).

use super::{RangeId, SymId};
use crate::{codec_enum, codec_struct};

/// A program's EXTERNAL and GLOBAL storage, files and procedures; empty for a program with none.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scope {
    /// The PROGRAM-IDs of the programs containing this one, innermost first.
    pub containers: Vec<SymId>,
    /// Each LINKAGE record whose storage the run unit or a containing program holds, by ordinal,
    /// in ordinal order.
    pub records: Vec<(u16, Binding)>,
    /// Each file whose connector is not its own, in file order.
    pub files: Vec<SharedFile>,
    /// Each file whose record area is not in the slab, and the LINKAGE record bound to it.
    pub areas: Vec<(u16, u16)>,
    /// A program that contains others: its GLOBAL records and files as they find them.
    pub globals: Vec<Global>,
    /// A program that contains others: its GLOBAL EXCEPTION/ERROR procedures for files, by file,
    /// and for the open modes, in `Declaratives.modes`' order.
    pub global_files: Vec<(u16, RangeId)>,
    pub global_modes: [Option<RangeId>; 4],
}

/// Where a record addressed as a LINKAGE record gets its address when the program is activated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Binding {
    /// The run unit's EXTERNAL record of this name and size, allocated by the first program that
    /// describes it.
    External { name: SymId, size: u32 },
    /// The run unit's record area of EXTERNAL file k, by the file's name and its area's size.
    ExternalFile(u16),
    /// What containing program `program` gives as its GLOBAL `name` of `section`: a record, or for
    /// `Section::File` the record area of the file of that name.
    Global { program: SymId, section: Section, name: SymId },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    WorkingStorage,
    LocalStorage,
    Linkage,
    File,
}

/// File `file`, whose connector is the run unit's EXTERNAL file of its name when `external`, and
/// otherwise file `declared_in` declares GLOBAL, that program's own file of its name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SharedFile {
    pub file: u16,
    pub external: bool,
    pub declared_in: Option<SymId>,
}

/// A GLOBAL record or file of a program that contains others: where the record, or the file's
/// record area, is in an activation of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Global {
    pub section: Section,
    pub name: SymId,
    pub at: GlobalAt,
}

/// An offset in the slab or in LOCAL-STORAGE, or the address LINKAGE record n holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlobalAt {
    Program(u32),
    Local(u32),
    Linkage(u16),
}

codec_struct!(Scope { containers, records, files, areas, globals, global_files, global_modes });
codec_enum!(Binding { External { name, size } = 0, ExternalFile(file) = 1, Global { program, section, name } = 2 });
codec_enum!(Section { WorkingStorage = 0, LocalStorage = 1, Linkage = 2, File = 3 });
codec_struct!(SharedFile { file, external, declared_in });
codec_struct!(Global { section, name, at });
codec_enum!(GlobalAt { Program(offset) = 0, Local(offset) = 1, Linkage(record) = 2 });
