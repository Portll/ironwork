//! The control characters a WRITE to a print file puts out; which files are print files, and how,
//! is `compile::printer`.

pub use compile::printer::*;

use crate::files::Move;

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
