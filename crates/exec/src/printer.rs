//! The printer control character ([`numeric::assumptions::PRINT_CONTROL_CHARACTER`]). A
//! sequential file that a WRITE ... ADVANCING of the program names, whose FD has LINAGE, or that
//! holds a report is a print file: every record written to it carries a control character, an ASA
//! character when every WRITE ... ADVANCING of the file says AFTER, a machine code when one says
//! BEFORE. Under ADV the character is a byte before the record; under NOADV it is the record's
//! first byte.

use crate::files::Move;
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

const ASA_LINES: [u8; 4] = [0x4E, 0x40, 0xF0, 0x60];
const ASA_CHANNELS: [u8; 12] = [0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8, 0xF9, 0xC1, 0xC2, 0xC3];
const PAGE_MODE: u8 = 0x5A;
const PRINT_THEN_SPACE: [u8; 4] = [0x01, 0x09, 0x11, 0x19];
const SPACE_NOW: [u8; 4] = [0x01, 0x0B, 0x13, 0x1B];

fn print_then_skip(channel: u8) -> u8 {
    0x89 + 8 * (channel.clamp(1, 12) - 1)
}

fn skip_now(channel: u8) -> u8 {
    0x8B + 8 * (channel.clamp(1, 12) - 1)
}

/// Records written only to move the paper: `full` of them with `byte`, then one with `rest`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Spacing {
    pub byte: u8,
    pub full: u64,
    pub rest: Option<u8>,
}

impl Spacing {
    /// Machine-code spacing of `lines` lines without printing, three at a time.
    fn machine(lines: u64) -> Self {
        let rest = (lines % 3) as usize;
        Self { byte: SPACE_NOW[3], full: lines / 3, rest: (rest > 0).then(|| SPACE_NOW[rest]) }
    }

    fn bytes(self) -> impl Iterator<Item = u8> {
        std::iter::repeat_n(self.byte, self.full as usize).chain(self.rest)
    }
}

/// The records one WRITE puts out, in order: spacing records, the record holding the line with
/// `data` as its control character, spacing records
/// ([`numeric::assumptions::PRINT_SPACING_RECORDS`] where one character cannot say the movement).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Controls {
    pub lead: Spacing,
    pub data: u8,
    pub trail: Spacing,
}

impl Controls {
    /// Each record's control character, and whether it is the record holding the line.
    pub fn records(self) -> impl Iterator<Item = (u8, bool)> {
        self.lead.bytes().map(|b| (b, false)).chain([(self.data, true)]).chain(self.trail.bytes().map(|b| (b, false)))
    }
}

/// The control characters of a WRITE BEFORE (`before`) or AFTER ADVANCING `space`. An ASA file
/// has no WRITE BEFORE, since one makes the file's characters machine codes.
pub fn controls(machine: bool, before: bool, space: Space) -> Controls {
    let only = |data| Controls { lead: Spacing::default(), data, trail: Spacing::default() };
    match (machine, before, space) {
        (_, _, Space::PageMode) => only(PAGE_MODE),
        (false, _, Space::Lines(n)) => {
            let full = n.saturating_sub(1) / 3;
            Controls { lead: Spacing { byte: ASA_LINES[3], full, rest: None }, data: ASA_LINES[(n - 3 * full) as usize], trail: Spacing::default() }
        }
        (false, _, Space::Channel(c)) => only(ASA_CHANNELS[usize::from(c.clamp(1, 12) - 1)]),
        (true, true, Space::Lines(n)) => Controls { lead: Spacing::default(), data: PRINT_THEN_SPACE[n.min(3) as usize], trail: Spacing::machine(n.saturating_sub(3)) },
        (true, true, Space::Channel(c)) => only(print_then_skip(c)),
        (true, false, Space::Lines(n)) => Controls { lead: Spacing::machine(n), data: PRINT_THEN_SPACE[0], trail: Spacing::default() },
        (true, false, Space::Channel(c)) => Controls { lead: Spacing { byte: skip_now(c), full: 1, rest: None }, data: PRINT_THEN_SPACE[0], trail: Spacing::default() },
    }
}

/// The control characters of a WRITE to a LINAGE file, which moves the paper `ahead` lines before
/// its line and `behind` lines after it. An ASA file has no WRITE BEFORE, so moves nothing after.
pub fn moving(machine: bool, ahead: u64, behind: u64) -> Controls {
    if !machine {
        return controls(false, false, Space::Lines(ahead));
    }
    Controls { lead: Spacing::machine(ahead), data: PRINT_THEN_SPACE[behind.min(3) as usize], trail: Spacing::machine(behind.saturating_sub(3)) }
}

/// How a text DD moves the paper before and after a record's line: as the control characters of
/// a print file say, and for any other file a line of its own
/// ([`numeric::assumptions::TEXT_PRINT_LINES`]).
pub fn text_motion(before: bool, space: Space) -> (Option<Move>, Option<Move>) {
    let movement = match space {
        Space::Lines(n) => Move::Lines(n),
        Space::Channel(1) => Move::Page,
        Space::Channel(_) | Space::PageMode => Move::Lines(1),
    };
    if before { (None, Some(movement)) } else { (Some(movement), None) }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(machine: bool, before: bool, space: Space) -> Vec<(u8, bool)> {
        controls(machine, before, space).records().collect()
    }

    #[test]
    fn asa_characters_for_after_advancing() {
        let data = |n| bytes(false, false, Space::Lines(n));
        assert_eq!([data(0), data(1), data(2), data(3)], [[(0x4E, true)], [(0x40, true)], [(0xF0, true)], [(0x60, true)]]);
        assert_eq!(data(5), [(0x60, false), (0xF0, true)]);
        assert_eq!(data(7), [(0x60, false), (0x60, false), (0x40, true)]);
        assert_eq!(bytes(false, false, Space::Channel(1)), [(0xF1, true)]);
        assert_eq!(bytes(false, false, Space::Channel(10)), [(0xC1, true)]);
        assert_eq!(bytes(false, false, Space::Channel(12)), [(0xC3, true)]);
        assert_eq!(bytes(false, false, Space::PageMode), [(0x5A, true)]);
    }

    #[test]
    fn machine_codes_print_then_act_or_act_first() {
        let before = |n| bytes(true, true, Space::Lines(n));
        assert_eq!([before(0), before(1), before(2), before(3)], [[(0x01, true)], [(0x09, true)], [(0x11, true)], [(0x19, true)]]);
        assert_eq!(before(5), [(0x19, true), (0x13, false)]);
        assert_eq!(before(7), [(0x19, true), (0x1B, false), (0x0B, false)]);
        assert_eq!(bytes(true, true, Space::Channel(1)), [(0x89, true)]);
        assert_eq!(bytes(true, true, Space::Channel(12)), [(0xE1, true)]);
        assert_eq!(bytes(true, false, Space::Lines(0)), [(0x01, true)]);
        assert_eq!(bytes(true, false, Space::Lines(1)), [(0x0B, false), (0x01, true)]);
        assert_eq!(bytes(true, false, Space::Lines(4)), [(0x1B, false), (0x0B, false), (0x01, true)]);
        assert_eq!(bytes(true, false, Space::Channel(1)), [(0x8B, false), (0x01, true)]);
        assert_eq!(bytes(true, false, Space::Channel(2)), [(0x93, false), (0x01, true)]);
    }

    #[test]
    fn a_linage_movement_spaces_before_and_after_the_line() {
        let records = |machine, ahead, behind| moving(machine, ahead, behind).records().collect::<Vec<_>>();
        assert_eq!(records(false, 5, 0), [(0x60, false), (0xF0, true)]);
        assert_eq!(records(false, 0, 0), [(0x4E, true)]);
        assert_eq!(records(true, 0, 5), [(0x19, true), (0x13, false)]);
        assert_eq!(records(true, 4, 1), [(0x1B, false), (0x0B, false), (0x09, true)]);
        assert_eq!(records(true, 2, 0), [(0x13, false), (0x01, true)]);
    }

    #[test]
    fn mnemonic_environment_names() {
        assert_eq!(mnemonic_space("C01"), Some(Space::Channel(1)));
        assert_eq!(mnemonic_space("C12"), Some(Space::Channel(12)));
        assert_eq!(mnemonic_space("CSP"), Some(Space::Lines(0)));
        assert_eq!(mnemonic_space("AFP-5A"), Some(Space::PageMode));
        assert_eq!(mnemonic_space("S01"), None);
        assert_eq!(mnemonic_space("C13"), None);
    }
}
