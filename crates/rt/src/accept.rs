//! ACCEPT (lir.md §9.1): the date, day, weekday or time of the run unit's clock, or a line of
//! SYSIN, moved into the receiver.

use crate::abend::Abend;
use crate::calendar::civil;
use crate::storage::{Loc, Val, literal_fixed};
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
        AcceptFrom::Sysin => {
            let mut line = String::new();
            let read = match unit.sysin.as_mut() {
                Some(r) => r.read_line(&mut line).map_err(|e| Abend::ironwork(format!("ACCEPT: {e}"), pos))?,
                None => 0,
            };
            if read == 0 {
                let _ = writeln!(unit.err, "ironwork: {pos}: ACCEPT found SYSIN at its end; {name} is unchanged");
                return Ok(());
            }
            let page = facts.page();
            let unknown = page.encode_char('?').unwrap_or(0x6F);
            Val::Bytes(line.trim_end_matches(['\n', '\r']).chars().map(|c| page.encode_char(c).unwrap_or(unknown)).collect())
        }
    };
    store::assign(facts, unit, dest, val, None, pos)
}
