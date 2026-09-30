//! The printer control character ([`numeric::assumptions::PRINT_CONTROL_CHARACTER`]). A
//! sequential file that a WRITE ... ADVANCING of the program names, whose FD has LINAGE, or that
//! holds a report is a print file: every record written to it carries a control character, an ASA
//! character when every WRITE ... ADVANCING of the file says AFTER, a machine code when one says
//! BEFORE. Under ADV the character is a byte before the record; under NOADV it is the record's
//! first byte.

use crate::layout::{Layout, Resolved};
use syntax::Error;
use syntax::ast::*;

/// How a print file's records carry the control character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Carriage {
    /// Machine codes rather than ASA characters.
    pub machine: bool,
    /// The character is the record's own first byte (NOADV), not a byte added before it.
    pub reserved: bool,
}

/// How far a WRITE moves the paper.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    /// Lines; 0 suppresses spacing.
    Lines(u64),
    /// A skip to channel 1 to 12. ADVANCING PAGE is channel 1.
    Channel(u8),
    /// AFP-5A: the record is page mode data for the Print Services Facility.
    PageMode,
}

/// The environment-name a mnemonic-name stands for, as a movement; None for the punch pockets.
pub fn mnemonic_space(environment: &str) -> Option<Space> {
    match environment {
        "CSP" => Some(Space::Lines(0)),
        "AFP-5A" => Some(Space::PageMode),
        c if c.len() == 3 && c.starts_with('C') => c[1..].parse().ok().filter(|n| (1..=12).contains(n)).map(Space::Channel),
        _ => None,
    }
}

/// Each file's control character: None for a file that is not a print file.
pub(crate) fn carriages(program: &Program, layout: &Layout, adv: bool) -> Vec<Option<Carriage>> {
    let mut uses = vec![(false, false); program.files.len()];
    let mut pending: Vec<&[Stmt]> = program.paragraphs.iter().map(|p| p.statements.as_slice()).collect();
    while let Some(stmts) = pending.pop() {
        for s in stmts {
            pending.extend(crate::oo::bodies(s));
            if let Stmt::Write { record, advancing: Some(a), .. } = s
                && let Some(k) = file_of(layout, record)
            {
                uses[k] = (true, uses[k].1 || a.before());
            }
        }
    }
    program
        .files
        .iter()
        .zip(uses)
        .map(|(f, (advancing, before))| {
            let print = f.organization == Organization::Sequential && !f.sort && (advancing || f.linage.is_some() || !f.reports.is_empty());
            print.then_some(Carriage { machine: before, reserved: reserves_first_byte(f, adv) })
        })
        .collect()
}

/// Whether a print file's control character is its records' own first byte: under NOADV, unless
/// LINAGE makes the file ADV.
pub(crate) fn reserves_first_byte(f: &FileDecl, adv: bool) -> bool {
    !adv && f.linage.is_none() && f.organization == Organization::Sequential && !f.sort
}

fn file_of(layout: &Layout, record: &Ref) -> Option<usize> {
    match layout.resolve(&record.name, &record.qualifiers, record.pos) {
        Ok(Resolved::Item(i)) => layout.items[i].file.map(usize::from),
        _ => None,
    }
}

/// What the Language Reference allows of a WRITE's ADVANCING phrase beyond its operand.
pub(crate) fn check_write(program: &Program, layout: &Layout, record: &Ref, advancing: &Advancing, pos: syntax::Pos, errors: &mut Vec<Error>) {
    if let Advancing::Mnemonic { name, environment, .. } = advancing
        && mnemonic_space(environment).is_none()
    {
        errors.push(Error::at(pos, format!("ADVANCING {name}: stacker selection ({environment}) on a card punch is not supported yet")));
    }
    let Some(f) = file_of(layout, record).map(|k| &program.files[k]) else { return };
    match f.organization {
        Organization::Indexed | Organization::Relative => errors.push(Error::at(pos, format!("WRITE ... ADVANCING: {} is not a sequential file", f.name))),
        Organization::LineSequential if advancing.before() || matches!(advancing, Advancing::Mnemonic { .. }) => {
            errors.push(Error::at(pos, format!("WRITE ... BEFORE ADVANCING, or ADVANCING a mnemonic-name, is not allowed for the line-sequential file {}", f.name)));
        }
        _ => {}
    }
}
