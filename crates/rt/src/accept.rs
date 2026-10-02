//! ACCEPT (lir.md §9.1): the date, day, weekday or time of the run unit's clock moved into the
//! receiver, or SYSIN's records transferred into it unconverted.

use crate::abend::Abend;
use crate::calendar::civil;
use crate::storage::{Kind, Loc, Val, literal_fixed};
use crate::store::{self, ProgramFacts};
use crate::unit::{Loader, RunUnit};
use crate::vocab::{AcceptFrom, Pos};

/// `name` is the receiver's, which the message at the end of SYSIN gives.
pub fn accept<H: Clone, L: Loader<H>>(facts: &dyn ProgramFacts, unit: &mut RunUnit<'_, H, L>, dest: Loc, from: AcceptFrom, name: &str, pos: Pos) -> Result<(), Abend> {
    let (seconds, hundredths) = unit.now();
    let c = civil(seconds);
    let (year, month, day, hour, minute, second, yday, wday) = (c.year, c.month, c.day, c.hour, c.minute, c.second, c.day_of_year, c.weekday);
    let digits = |text: String| Val::Num(literal_fixed(&text).expect("digits"));
    let val = match from {
        AcceptFrom::Date { four_digit_year: true } => digits(format!("{year:04}{month:02}{day:02}")),
        AcceptFrom::Date { four_digit_year: false } => digits(format!("{:02}{month:02}{day:02}", year % 100)),
        AcceptFrom::Day { four_digit_year: true } => digits(format!("{year:04}{yday:03}")),
        AcceptFrom::Day { four_digit_year: false } => digits(format!("{:02}{yday:03}", year % 100)),
        AcceptFrom::DayOfWeek => digits(format!("{wday}")),
        AcceptFrom::Time => digits(format!("{hour:02}{minute:02}{second:02}{hundredths:02}")),
        AcceptFrom::Sysin if dest.kind == Kind::National => match sysin_record(facts, unit, pos)? {
            Some(record) => Val::Bytes(record),
            None => {
                at_end(unit, name, pos);
                return Ok(());
            }
        },
        AcceptFrom::Sysin => {
            let space = facts.page().encode_char(' ').unwrap_or(0x40);
            let mut area = Vec::with_capacity(dest.len);
            while area.len() < dest.len {
                let Some(mut record) = sysin_record(facts, unit, pos)? else { break };
                if record.len() < CARD {
                    record.resize(CARD, space);
                }
                record.truncate(dest.len - area.len());
                area.extend(record);
            }
            if area.is_empty() {
                at_end(unit, name, pos);
                return Ok(());
            }
            area.resize(dest.len, space);
            store::write(&mut unit.mem, dest, &area);
            return Ok(());
        }
    };
    store::assign(facts, unit, dest, val, None, pos)
}

/// An in-stream SYSIN record: a card of 80 bytes (assumption [`numeric::assumptions::SYSIN_CARD_IMAGES`]).
const CARD: usize = 80;

/// The next line of SYSIN in the program's code page, or None at its end.
fn sysin_record<H: Clone, L: Loader<H>>(facts: &dyn ProgramFacts, unit: &mut RunUnit<'_, H, L>, pos: Pos) -> Result<Option<Vec<u8>>, Abend> {
    let mut line = String::new();
    let read = match unit.sysin.as_mut() {
        Some(r) => r.read_line(&mut line).map_err(|e| Abend::ironwork(format!("ACCEPT: {e}"), pos))?,
        None => 0,
    };
    if read == 0 {
        return Ok(None);
    }
    let page = facts.page();
    let unknown = page.encode_char('?').unwrap_or(0x6F);
    Ok(Some(line.trim_end_matches(['\n', '\r']).chars().map(|c| page.encode_char(c).unwrap_or(unknown)).collect()))
}

fn at_end<H: Clone, L: Loader<H>>(unit: &mut RunUnit<'_, H, L>, name: &str, pos: Pos) {
    let _ = writeln!(unit.err, "ironwork: {pos}: ACCEPT found SYSIN at its end; {name} is unchanged");
}
