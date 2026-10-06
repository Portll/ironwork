//! The screen Micro Focus's and GnuCOBOL's positioned DISPLAY and ACCEPT write and read under
//! `--compliance extended` (docs/compliance.md IWX0020, assumption C462): a grid of characters with
//! a cursor, and an operator played from a screen script, as `--screens` plays one for a CICS task.

use crate::abend::Abend;
use crate::intrinsic::numval::{self, Form};
use crate::storage::{Kind, Loc, Val, literal_fixed};
use crate::store::{self, ProgramFacts};
use crate::terminal::{AID_ENTER, Action};
use crate::unit::{Loader, RunUnit};
use crate::vocab::Pos;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

/// The rows and columns of the screen a run gets.
pub const ROWS: usize = 24;
pub const COLUMNS: usize = 80;

/// The line and column an AT phrase's number gives: four digits as LLCC, six as LLLCCC.
pub fn line_column(at: u64) -> (usize, usize) {
    if at > 9999 { ((at / 1000) as usize, (at % 1000) as usize) } else { ((at / 100) as usize, (at % 100) as usize) }
}

/// The run unit's screen, a blank one with no operator when the run has none yet.
fn screen<H, L: Loader<H>>(unit: &mut RunUnit<'_, H, L>) -> Rc<RefCell<Crt>> {
    unit.crt.get_or_insert_with(|| Rc::new(RefCell::new(Crt::new(ROWS, COLUMNS, Vec::new())))).clone()
}

/// A positioned DISPLAY: `text` written on the run unit's screen at `at`, or the cursor.
pub fn display<H, L: Loader<H>>(unit: &mut RunUnit<'_, H, L>, at: Option<(usize, usize)>, text: &str, clearing: Clearing) {
    screen(unit).borrow_mut().display(at, text, clearing);
}

/// A positioned ACCEPT into `dest`, whose field `shown` is as DISPLAY shows the item: the operator's
/// entry moved into it, alphanumeric as typed and numeric as NUMVAL reads it, zero where it is not
/// a number, and the field then showing the item as stored. True when a key other than ENTER
/// ended it, which ON EXCEPTION takes.
#[allow(clippy::too_many_arguments)]
pub fn accept<H, L: Loader<H>>(facts: &dyn ProgramFacts, unit: &mut RunUnit<'_, H, L>, dest: Loc, shown: &str, at: Option<(usize, usize)>, update: bool, secure: bool, pos: Pos) -> Result<bool, Abend> {
    let crt = screen(unit);
    let (row, column) = at.unwrap_or_else(|| crt.borrow().cursor_position());
    let field = Field { row, column, len: shown.chars().count().max(1), secure };
    let Some(entry) = crt.borrow_mut().accept(field, update.then_some(shown)) else {
        return Err(Abend::ironwork("ACCEPT: the screen has no more operator input", pos));
    };
    let numeric = matches!(dest.kind, Kind::Zoned { .. } | Kind::Packed { .. } | Kind::Binary { .. } | Kind::NumericEdited { .. } | Kind::Float(_));
    let val = if numeric {
        let typed = entry.text.trim();
        let number = numval::parse(typed, Form::Numval, 31, facts.decimal_point() == ',').ok().and_then(|n| {
            let digits = n.digits.to_string();
            let (int, frac) = digits.split_at(digits.len().saturating_sub(n.decimals as usize));
            literal_fixed(&format!("{}{}.{frac}", if n.negative { "-" } else { "" }, if int.is_empty() { "0" } else { int }))
        });
        Val::Num(number.unwrap_or_else(|| literal_fixed("0").expect("zero")))
    } else {
        let page = facts.page();
        let unknown = page.encode_char('?').unwrap_or(0x6F);
        Val::Bytes(entry.text.chars().map(|c| page.encode_char(c).unwrap_or(unknown)).collect())
    };
    store::assign(facts, unit, dest, val, None, pos)?;
    unit.mark_input(dest.offset, dest.len, true);
    let stored = crate::display::place(facts, &unit.mem, dest, pos, false)?;
    crt.borrow_mut().show(field, &stored);
    Ok(entry.key != AID_ENTER)
}

/// What a positioned DISPLAY clears before it writes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clearing {
    /// BLANK SCREEN or ERASE SCREEN: the whole screen.
    pub screen: bool,
    /// BLANK LINE: the line written on.
    pub line: bool,
    /// ERASE EOL: from the position to the end of its line.
    pub to_line_end: bool,
    /// ERASE EOS or ERASE: from the position to the end of the screen.
    pub to_screen_end: bool,
}

/// A field an ACCEPT reads: where it starts, 1-based, and how many characters it holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Field {
    pub row: usize,
    pub column: usize,
    pub len: usize,
    /// SECURE: each character typed shows as `*`.
    pub secure: bool,
}

/// What the operator gave a field: its characters as they stand when the key was pressed, and
/// the key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub text: String,
    pub key: u8,
}

pub struct Crt {
    pub rows: usize,
    pub columns: usize,
    cells: Vec<char>,
    cursor: usize,
    actions: VecDeque<Action>,
    /// The screen as each ACCEPT shows it, before the operator types.
    pub shown: Vec<String>,
    /// Whether a positioned DISPLAY or ACCEPT has used the screen.
    pub used: bool,
}

impl std::fmt::Debug for Crt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Crt({}x{}, {} actions left)", self.rows, self.columns, self.actions.len())
    }
}

impl Crt {
    pub fn new(rows: usize, columns: usize, actions: Vec<Action>) -> Self {
        Self { rows, columns, cells: vec![' '; rows * columns], cursor: 0, actions: actions.into(), shown: Vec::new(), used: false }
    }

    /// The cursor's 1-based row and column.
    pub fn cursor_position(&self) -> (usize, usize) {
        (self.cursor / self.columns + 1, self.cursor % self.columns + 1)
    }

    /// The address of a 1-based row and column, or the cursor's where none is given. A position
    /// off the screen is its last cell.
    fn address(&self, at: Option<(usize, usize)>) -> usize {
        match at {
            Some((row, column)) => ((row.max(1) - 1) * self.columns + column.max(1) - 1).min(self.cells.len() - 1),
            None => self.cursor,
        }
    }

    /// Writes `text` at `at`, or at the cursor, after clearing what `clearing` names; text past
    /// the end of a line goes on at the start of the next, and past the last line is lost. The
    /// cursor is left after the last character written.
    pub fn display(&mut self, at: Option<(usize, usize)>, text: &str, clearing: Clearing) {
        self.used = true;
        let start = self.address(at);
        let line = start / self.columns * self.columns;
        if clearing.screen {
            self.cells.fill(' ');
        }
        if clearing.line {
            self.cells[line..line + self.columns].fill(' ');
        }
        if clearing.to_screen_end {
            self.cells[start..].fill(' ');
        } else if clearing.to_line_end {
            self.cells[start..line + self.columns].fill(' ');
        }
        let mut at = start;
        for c in text.chars() {
            if at >= self.cells.len() {
                break;
            }
            self.cells[at] = c;
            at += 1;
        }
        self.cursor = at.min(self.cells.len() - 1);
    }

    /// Shows `field` holding `current`, or spaces, keeps the screen in `shown`, and plays the
    /// script to its next key: text typed at the cursor or at a row and column replaces the field
    /// from there to its end, as far as the text reaches, and `eof` clears the field from where it
    /// is given. None when the script has no key left.
    pub fn accept(&mut self, field: Field, current: Option<&str>) -> Option<Entry> {
        self.used = true;
        let start = self.address(Some((field.row, field.column)));
        let end = (start + field.len).min(self.cells.len());
        let mut value: Vec<char> = current.unwrap_or("").chars().chain(std::iter::repeat(' ')).take(end - start).collect();
        let shown = |value: &[char]| -> Vec<char> { value.iter().map(|&c| if field.secure && c != ' ' { '*' } else { c }).collect() };
        self.cells[start..end].copy_from_slice(&shown(&value));
        self.shown.push(self.render());
        let mut cursor = start;
        let typed = |cursor: &mut usize, text: &str, value: &mut Vec<char>| {
            if (start..end).contains(cursor) {
                value[*cursor - start..].fill(' ');
            }
            for c in text.chars() {
                if (start..end).contains(cursor) {
                    value[*cursor - start] = c;
                }
                *cursor += 1;
            }
        };
        while let Some(action) = self.actions.pop_front() {
            match action {
                Action::Text(text) => typed(&mut cursor, &text, &mut value),
                Action::Type { row, column, text } => {
                    cursor = self.address(Some((row, column)));
                    typed(&mut cursor, &text, &mut value);
                }
                Action::EraseEof { row, column } => {
                    let from = self.address(Some((row, column))).clamp(start, end);
                    value[from - start..].fill(' ');
                    cursor = from;
                }
                Action::Cursor { row, column } => cursor = self.address(Some((row, column))),
                Action::Home | Action::Tab => cursor = start,
                Action::Key(key) => {
                    self.cells[start..end].copy_from_slice(&shown(&value));
                    self.cursor = end.min(self.cells.len() - 1);
                    return Some(Entry { text: value.into_iter().collect(), key });
                }
            }
        }
        None
    }

    /// Writes `text` into `field`, as far as the field reaches, a secure field's characters as `*`.
    pub fn show(&mut self, field: Field, text: &str) {
        let start = self.address(Some((field.row, field.column)));
        let end = (start + field.len).min(self.cells.len());
        for (cell, c) in self.cells[start..end].iter_mut().zip(text.chars().chain(std::iter::repeat(' '))) {
            *cell = if field.secure && c != ' ' { '*' } else { c };
        }
    }

    /// The screen as text, each line without its trailing spaces and the blank lines at the
    /// bottom left out.
    pub fn render(&self) -> String {
        let mut lines: Vec<String> = self.cells.chunks(self.columns).map(|row| row.iter().collect::<String>().trim_end().to_owned()).collect();
        while lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::parse_script;

    fn crt(script: &str) -> Crt {
        Crt::new(24, 80, parse_script(script).unwrap())
    }

    #[test]
    fn display_writes_at_its_position_and_leaves_the_cursor_after_the_text() {
        let mut c = crt("");
        c.display(Some((1, 1)), "ROW1COL1", Clearing::default());
        c.display(Some((2, 1)), "XXXXXXXXXX", Clearing::default());
        c.display(Some((2, 5)), "AB", Clearing { to_line_end: true, ..Clearing::default() });
        c.display(None, "CD", Clearing::default());
        assert_eq!(c.render(), "ROW1COL1\nXXXXABCD");
        c.display(Some((1, 79)), "WRAP", Clearing::default());
        assert_eq!(c.render(), "ROW1COL1                                                                      WR\nAPXXABCD");
        c.display(Some((1, 5)), "Z", Clearing { screen: true, ..Clearing::default() });
        assert_eq!(c.render(), "    Z");
    }

    #[test]
    fn erase_eos_clears_to_the_end_of_the_screen_and_text_past_it_is_lost() {
        let mut c = crt("");
        c.display(Some((3, 1)), "KEEP", Clearing::default());
        c.display(Some((5, 1)), "GONE", Clearing::default());
        c.display(Some((4, 1)), "NEW", Clearing { to_screen_end: true, ..Clearing::default() });
        assert_eq!(c.render(), "\n\nKEEP\nNEW");
        c.display(Some((24, 79)), "XYZ", Clearing::default());
        assert!(c.render().ends_with(&format!("{}XY", " ".repeat(78))));
    }

    #[test]
    fn accept_shows_the_screen_then_takes_what_is_typed_into_the_field_up_to_the_key() {
        let mut c = crt("string BOBBY-TOO-LONG\nENTER\ntype 11 3 9\nENTER\n");
        c.display(Some((1, 1)), "NAME:", Clearing::default());
        let name = c.accept(Field { row: 1, column: 7, len: 5, secure: false }, None).unwrap();
        assert_eq!(name, Entry { text: "BOBBY".into(), key: AID_ENTER });
        assert_eq!(c.shown, ["NAME:"]);
        let qty = c.accept(Field { row: 11, column: 1, len: 3, secure: false }, Some("007")).unwrap();
        assert_eq!(qty.text, "009");
        assert_eq!(c.shown[1], "NAME: BOBBY\n\n\n\n\n\n\n\n\n\n007");
        assert_eq!(c.render(), "NAME: BOBBY\n\n\n\n\n\n\n\n\n\n009");
        assert_eq!(c.accept(Field { row: 2, column: 1, len: 1, secure: false }, None), None);
    }

    #[test]
    fn a_secure_field_shows_stars_and_eof_clears_the_rest_of_the_field() {
        let mut c = crt("string PW\nENTER\neof 3 3\nstring Z\nENTER\n");
        let pw = c.accept(Field { row: 2, column: 1, len: 4, secure: true }, None).unwrap();
        assert_eq!((pw.text.as_str(), c.render()), ("PW  ", "\n**".into()));
        let rest = c.accept(Field { row: 3, column: 1, len: 5, secure: false }, Some("ABCDE")).unwrap();
        assert_eq!(rest.text, "ABZ  ");
        let mut c = crt("string 42\nENTER\n");
        assert_eq!(c.accept(Field { row: 1, column: 1, len: 3, secure: false }, Some("007")).unwrap().text, "42 ");
    }
}
