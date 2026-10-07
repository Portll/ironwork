//! The CICS region a harness run stands in for: one task, its files, its temporary-storage and
//! transient-data queues, and the time services; and the commands a program runs against it, as
//! `CicsCommand`s the executor builds and `run` carries out.

mod command;
mod file_control;
mod maps;
mod program;
mod run;
mod services;

pub use command::{Assign, Cics, CicsCommand, Control, Datum, FileControl, FileOptions, Handles, Opt, Record, Resp, Sink, Transfer};
pub use run::{
    At, CicsHost, EIBAID, EIBCALEN, EIBCPOSN, EIBDATE, EIBFN, EIBRESP, EIBRESP2, EIBRSRCE, EIBTASKN, EIBTIME, EIBTRMID,
    EIBTRNID, Flow, Handler, Handlers, begin_command, begin_task, bytes, in_task, ok, raise, run, unsupported,
};
pub use run::{AbendExit, ExitTarget};
pub use program::{abend_exit, enter_exit_program, level_ended};

use crate::files::{Dd, Format, KeySpan, Keying};
use crate::calendar::{civil, EPOCH_1900_TO_1970_MILLIS, EPOCH_1900_TO_1970_SECONDS, SECONDS_PER_DAY};
pub use crate::terminal::Terminal;
use std::collections::{BTreeMap, HashMap};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

/// The EXEC interface block's length: EIBRLDBK, its last field, is at X'54'.
pub const EIB_LEN: usize = 85;

macro_rules! conditions {
    ($($name:ident)*) => {
        /// A CICS exception condition, one variant per name DFHRESP knows, spelt as IBM spells it.
        #[allow(clippy::upper_case_acronyms)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Condition {
            $($name,)*
        }

        impl Condition {
            pub const ALL: &[Self] = &[$(Self::$name,)*];

            pub fn name(self) -> &'static str {
                match self {
                    $(Self::$name => stringify!($name),)*
                }
            }
        }
    };
}

conditions! {
    NORMAL ERROR RDATT WRBRK EOF EODS EOC INBFMH ENDINPT NONVAL NOSTART TERMIDERR FILENOTFOUND
    NOTFND DUPREC DUPKEY INVREQ IOERR NOSPACE NOTOPEN ENDFILE ILLOGIC LENGERR QZERO SIGNAL QBUSY
    ITEMERR PGMIDERR TRANSIDERR ENDDATA INVTSREQ EXPIRED RETPAGE RTEFAIL RTESOME TSIOERR MAPFAIL
    INVERRTERM INVMPSZ IGREQID OVERFLOW INVLDC NOSTG JIDERR QIDERR NOJBUFSP DSSTAT SELNERR FUNCERR
    UNEXPIN NOPASSBKRD NOPASSBKWR SEGIDERR SYSIDERR ISCINVREQ ENQBUSY ENVDEFERR IGREQCD SESSIONERR
    SYSBUSY SESSBUSY NOTALLOC CBIDERR INVEXITREQ INVPARTNSET INVPARTN PARTNFAIL USERIDERR NOTAUTH
    VOLIDERR SUPPRESSED RESIDERR NOSPOOL TERMERR ROLLEDBACK END DISABLED ALLOCERR STRELERR OPENERR
    SPOLBUSY SPOLERR NODEIDERR TASKIDERR TCIDERR DSNNOTFOUND LOADING MODELIDERR OUTDESCRERR
    PARTNERIDERR PROFILEIDERR NETNAMEIDERR LOCKED RECORDBUSY UOWNOTFOUND UOWLNOTFOUND LINKABEND
    CHANGED PROCESSBUSY ACTIVITYBUSY PROCESSERR ACTIVITYERR CONTAINERERR EVENTERR TOKENERR
    NOTFINISHED POOLERR TIMERERR SYMBOLERR TEMPLATERR NOTSUPERUSER CSDERR DUPRES RESUNAVAIL
    CHANNELERR CCSIDERR TIMEDOUT CODEPAGEERR INCOMPLETE APPNOTFOUND BUSY
}

/// Names a condition also goes by: DSIDERR is FILENOTFOUND's older name.
const ALIASES: &[(&str, Condition)] = &[("DSIDERR", Condition::FILENOTFOUND)];

/// The transaction abend CICS issues when a condition is raised and nothing handles it; a
/// condition not listed abends AEIP.
const DEFAULT_ABENDS: &[(Condition, &str)] = &[
    (Condition::NOTFND, "AEIM"),
    (Condition::INVMPSZ, "AEYB"),
    (Condition::DUPREC, "AEIN"),
    (Condition::DUPKEY, "AEIO"),
    (Condition::IOERR, "AEIQ"),
    (Condition::NOSPACE, "AEIR"),
    (Condition::NOTOPEN, "AEIS"),
    (Condition::ENDFILE, "AEIT"),
    (Condition::ILLOGIC, "AEIU"),
    (Condition::LENGERR, "AEIV"),
    (Condition::QZERO, "AEIW"),
    (Condition::ITEMERR, "AEIZ"),
    (Condition::PGMIDERR, "AEI0"),
    (Condition::TRANSIDERR, "AEI1"),
    (Condition::ENDDATA, "AEI2"),
    (Condition::INVTSREQ, "AEI3"),
    (Condition::EXPIRED, "AEI4"),
    (Condition::TSIOERR, "AEI8"),
    (Condition::MAPFAIL, "AEI9"),
    (Condition::ERROR, "AEIA"),
    (Condition::EOF, "AEID"),
    (Condition::EODS, "AEIE"),
    (Condition::INBFMH, "AEIG"),
    (Condition::ENDINPT, "AEIH"),
    (Condition::NONVAL, "AEII"),
    (Condition::NOSTART, "AEIJ"),
    (Condition::TERMIDERR, "AEIK"),
    (Condition::FILENOTFOUND, "AEIL"),
    (Condition::DISABLED, "AEXL"),
    (Condition::ROLLEDBACK, "AEXJ"),
    (Condition::LOCKED, "AEX8"),
    (Condition::RECORDBUSY, "AEX9"),
    (Condition::QIDERR, "AEYH"),
    (Condition::SYSIDERR, "AEYQ"),
    (Condition::NOTAUTH, "AEY7"),
    (Condition::USERIDERR, "AEYX"),
    (Condition::CONTAINERERR, "AEZJ"),
    (Condition::CHANNELERR, "AEZV"),
];

impl Condition {
    /// The condition DFHRESP(name) names, by either of its names, in any case.
    pub fn from_name(name: &str) -> Option<Self> {
        let alias = ALIASES.iter().find(|(a, _)| a.eq_ignore_ascii_case(name)).map(|&(_, c)| c);
        alias.or_else(|| Self::ALL.iter().copied().find(|c| c.name().eq_ignore_ascii_case(name)))
    }

    /// EIBRESP's value for the condition, from IBM's DFHRESP table.
    pub fn resp(self) -> i32 {
        crate::cics_tables::resp(self.name()).unwrap_or_else(|| panic!("DFHRESP has no {}", self.name()))
    }

    pub fn default_abend(self) -> &'static str {
        DEFAULT_ABENDS.iter().find(|&&(c, _)| c == self).map_or("AEIP", |&(_, abend)| abend)
    }
}

/// What kind of VSAM data set a CICS file is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataSet {
    Ksds { key: KeySpan },
    Rrds,
}

/// A file-control table entry: a CICS FILE name's data set, given with --file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDef {
    pub dd: Dd,
    pub data_set: DataSet,
    pub record_len: usize,
}

impl FileDef {
    /// How files::open_keyed keys the data set.
    pub fn keying(&self) -> Keying {
        match &self.data_set {
            DataSet::Ksds { key } => Keying::Indexed { prime: *key, alternates: vec![] },
            DataSet::Rrds => Keying::Relative,
        }
    }

    pub fn format(&self) -> Format {
        self.dd.format.unwrap_or(Format::Fixed)
    }
}

/// Parses `NAME=path,KSDS,key=OFFSET:LENGTH,len=RECLEN[,text|,fixed|,variable]` or
/// `NAME=path,RRDS,len=RECLEN[,text|,fixed|,variable]`.
pub fn parse_file(spec: &str) -> Result<(String, FileDef), String> {
    let parts: Vec<&str> = spec.split(',').collect();
    if parts.len() < 3 {
        return Err(format!("{spec}: expected NAME=path,KIND,…"));
    }

    let (name, path) = parts[0]
        .split_once('=')
        .ok_or_else(|| format!("{spec}: expected NAME=path"))?;
    let name = name.to_ascii_uppercase();
    let kind = parts[1].to_ascii_uppercase();

    let mut key: Option<KeySpan> = None;
    let mut record_len: Option<usize> = None;
    let mut format: Option<Format> = None;

    for part in &parts[2..] {
        let part = part.trim();
        if let Some(val) = part.strip_prefix("key=") {
            let (off, len) = val
                .split_once(':')
                .ok_or_else(|| format!("{spec}: bad key spec {val}"))?;
            let off: usize = off.parse().map_err(|_| format!("{spec}: bad key offset {off}"))?;
            let len: usize = len.parse().map_err(|_| format!("{spec}: bad key length {len}"))?;
            key = Some(KeySpan { offset: off, len });
        } else if let Some(val) = part.strip_prefix("len=") {
            record_len = Some(
                val.parse()
                    .map_err(|_| format!("{spec}: bad record length {val}"))?,
            );
        } else if let Some(f) = Format::from_keyword(part) {
            format = Some(f);
        } else {
            return Err(format!("{spec}: unknown field {part}"));
        }
    }

    let data_set = match kind.as_str() {
        "KSDS" => {
            let key = key.ok_or_else(|| format!("{spec}: KSDS requires key=OFFSET:LENGTH"))?;
            DataSet::Ksds { key }
        }
        "RRDS" => DataSet::Rrds,
        _ => return Err(format!("{spec}: unknown data set kind {kind}")),
    };

    let record_len = record_len.ok_or_else(|| format!("{spec}: missing len=RECLEN"))?;
    let dd = Dd { path: PathBuf::from(path), format, append: false };
    Ok((name, FileDef { dd, data_set, record_len }))
}

/// A temporary-storage queue: its items (item numbers start at 1) and where READQ TS NEXT is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TsQueue {
    pub items: Vec<Vec<u8>>,
    pub next: usize,
}

/// A browse's position: the key it is at, and whether the record there is itself next (after
/// STARTBR or RESETBR) or was the last returned.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Browse {
    pub at: Vec<u8>,
    pub inclusive: bool,
}

/// The task: who and what started it, and the resources it has touched.
#[derive(Debug, Default)]
pub struct Task {
    pub transid: String,
    pub termid: String,
    pub userid: String,
    pub applid: String,
    pub sysid: String,
    pub number: u32,
    /// The COMMAREA the task starts with; EIBCALEN is its length.
    pub commarea: Option<Vec<u8>>,
    pub files: HashMap<String, FileDef>,
    pub ts: BTreeMap<String, TsQueue>,
    pub td: BTreeMap<String, Vec<Vec<u8>>>,
    /// TD queues whose items are appended to a host file as text lines when the task ends.
    pub td_files: HashMap<String, PathBuf>,
    /// The key each file's READ UPDATE holds, for REWRITE, DELETE and UNLOCK.
    pub held: HashMap<String, Vec<u8>>,
    /// Open browses by file and REQID.
    pub browses: HashMap<(String, i64), Browse>,
    pub terminal: Option<Box<dyn Terminal>>,
    /// The AID key whose input started the task, as EIBAID shows it before any RECEIVE.
    pub initial_aid: Option<u8>,
    /// Mapsets already read from the copy libraries, by name.
    pub mapsets: HashMap<String, crate::bms::Mapset>,
    /// RETURN TRANSID and COMMAREA, when the task ended that way.
    pub next_transid: Option<String>,
    pub returned_commarea: Option<Vec<u8>>,
    /// The code of the abend a HANDLE ABEND exit was given, which ASSIGN ABCODE returns.
    pub abcode: Option<String>,
    /// An ABEND CANCEL is ending the task, which no HANDLE ABEND exit intercepts.
    pub cancelling: bool,
    /// The program activations the task has started, which number each one.
    pub activations: u64,
    /// The LINKs and HANDLE ABEND PROGRAM exits running, each a logical level below the task's
    /// first; RETURN TRANSID and COMMAREA belong to that first level.
    pub links: u32,
    /// RETURN or XCTL has ended the logical level running: each program CALLed at it ends as its
    /// CALL comes back (C233).
    pub ending_level: bool,
}

impl Task {
    /// The number of a program activation starting, which owns the HANDLE labels it sets.
    pub fn next_activation(&mut self) -> u64 {
        self.activations += 1;
        self.activations
    }

    /// WRITEQ TS: appends and returns the new item's number; with Some(item) (REWRITE) replaces
    /// that item and returns it.
    pub fn writeq_ts(
        &mut self,
        queue: &str,
        rewrite: Option<usize>,
        data: &[u8],
    ) -> Result<usize, Condition> {
        let queue = queue.trim_end();
        match rewrite {
            None => {
                let q = self
                    .ts
                    .entry(queue.to_string())
                    .or_default();
                q.items.push(data.to_vec());
                Ok(q.items.len())
            }
            Some(item) => {
                let q = self.ts.get_mut(queue).ok_or(Condition::QIDERR)?;
                if item == 0 || item > q.items.len() {
                    return Err(Condition::ITEMERR);
                }
                q.items[item - 1] = data.to_vec();
                Ok(item)
            }
        }
    }

    /// READQ TS: Some(item) reads that item; None reads the next item after the last read.
    pub fn readq_ts(
        &mut self,
        queue: &str,
        item: Option<usize>,
    ) -> Result<(Vec<u8>, usize), Condition> {
        let queue = queue.trim_end();
        let q = self.ts.get_mut(queue).ok_or(Condition::QIDERR)?;
        let item_num = match item {
            Some(n) => {
                if n == 0 || n > q.items.len() {
                    return Err(Condition::ITEMERR);
                }
                n
            }
            None => {
                let n = if q.next == 0 { 1 } else { q.next + 1 };
                if n > q.items.len() {
                    return Err(Condition::ITEMERR);
                }
                n
            }
        };
        let data = q.items[item_num - 1].clone();
        q.next = item_num;
        Ok((data, q.items.len()))
    }

    /// DELETEQ TS.
    pub fn deleteq_ts(&mut self, queue: &str) -> Result<(), Condition> {
        let queue = queue.trim_end();
        self.ts.remove(queue).map(|_| ()).ok_or(Condition::QIDERR)
    }

    /// WRITEQ TD: appends (creating the queue).
    pub fn writeq_td(&mut self, queue: &str, data: &[u8]) {
        let queue = queue.trim_end();
        self.td.entry(queue.to_string()).or_default().push(data.to_vec());
    }

    /// READQ TD: removes and returns the oldest item.
    pub fn readq_td(&mut self, queue: &str) -> Result<Vec<u8>, Condition> {
        let queue = queue.trim_end();
        let items = self.td.get_mut(queue).ok_or(Condition::QZERO)?;
        if items.is_empty() {
            return Err(Condition::QZERO);
        }
        Ok(items.remove(0))
    }

    /// DELETEQ TD: empties the queue (no error when absent).
    pub fn deleteq_td(&mut self, queue: &str) {
        let queue = queue.trim_end();
        if let Some(items) = self.td.get_mut(queue) {
            items.clear();
        }
    }

    /// Appends each TD queue that has a td_files entry to its file, one line per item, each item
    /// decoded through `page` with trailing spaces trimmed; then empties those queues.
    pub fn flush_td(&mut self, page: &zarch::ebcdic::CodePage) -> std::io::Result<()> {
        let queues: Vec<String> = self.td_files.keys().cloned().collect();
        for queue in &queues {
            let Some(path) = self.td_files.get(queue) else { continue };
            let items: Vec<Vec<u8>> = self
                .td
                .get(queue)
                .cloned()
                .unwrap_or_default();
            if items.is_empty() {
                continue;
            }
            let mut file = OpenOptions::new().create(true).append(true).open(path)?;
            for item in &items {
                let text = page.decode(item);
                let line = format!("{}\n", text.trim_end());
                file.write_all(line.as_bytes())?;
            }
            if let Some(q) = self.td.get_mut(queue) {
                q.clear();
            }
        }
        Ok(())
    }
}

/// Milliseconds from 1900-01-01T00:00:00 to a Unix time given as seconds and hundredths.
pub fn abstime(seconds: i64, hundredths: u32) -> i64 {
    EPOCH_1900_TO_1970_MILLIS + seconds * 1000 + hundredths as i64 * 10
}

/// Unix seconds (rounded down) of an ABSTIME.
pub fn unix_seconds(abstime: i64) -> i64 {
    (abstime - EPOCH_1900_TO_1970_MILLIS).div_euclid(1000)
}

/// EIBDATE's value for an ABSTIME: 0CYYDDD as a decimal number.
pub fn eib_date(abstime: i64) -> i64 {
    let secs = unix_seconds(abstime);
    let date = civil(secs);
    let c = if date.year >= 2000 { 1 } else { 0 };
    let yy = date.year % 100;
    c * 100_000 + yy * 1_000 + i64::from(date.day_of_year)
}

/// EIBTIME's value for an ABSTIME: 0HHMMSS as a decimal number.
pub fn eib_time(abstime: i64) -> i64 {
    let secs = unix_seconds(abstime);
    let time = civil(secs);
    i64::from(time.hour) * 10_000 + i64::from(time.minute) * 100 + i64::from(time.second)
}

/// A FORMATTIME output: text for the date and time forms, a number for the counts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormatValue {
    Text(String),
    Number(i64),
}

/// One FORMATTIME option's value for an ABSTIME, or None for an option this does not produce.
/// `datesep` and `timesep` are the separators when DATESEP / TIMESEP were given.
pub fn format_time(abstime: i64, option: &str, datesep: Option<char>, timesep: Option<char>) -> Option<FormatValue> {
    let secs = unix_seconds(abstime);
    let c = civil(secs);
    let (year, month, day, hour, minute, second, doy) = (c.year, c.month, c.day, c.hour, c.minute, c.second, c.day_of_year);

    let yy = format!("{:02}", year % 100);
    let yyyy = format!("{:04}", year);
    let mm = format!("{:02}", month);
    let dd = format!("{:02}", day);
    let hh = format!("{:02}", hour);
    let mi = format!("{:02}", minute);
    let ss = format!("{:02}", second);
    let ddd = format!("{:03}", doy);

    let join = |parts: &[&str], sep: Option<char>| match sep {
        Some(c) => {
            let mut s = String::new();
            for (i, p) in parts.iter().enumerate() {
                if i > 0 {
                    s.push(c);
                }
                s.push_str(p);
            }
            s
        }
        None => parts.concat(),
    };

    let option = option.to_ascii_uppercase();

    match option.as_str() {
        "YYYYMMDD" => Some(FormatValue::Text(join(&[&yyyy, &mm, &dd], datesep))),
        "YYMMDD" => Some(FormatValue::Text(join(&[&yy, &mm, &dd], datesep))),
        "YYDDMM" => Some(FormatValue::Text(join(&[&yy, &dd, &mm], datesep))),
        "YYYYDDMM" => Some(FormatValue::Text(join(&[&yyyy, &dd, &mm], datesep))),
        "DDMMYY" => Some(FormatValue::Text(join(&[&dd, &mm, &yy], datesep))),
        "DDMMYYYY" => Some(FormatValue::Text(join(&[&dd, &mm, &yyyy], datesep))),
        "MMDDYY" | "DATE" => Some(FormatValue::Text(join(&[&mm, &dd, &yy], datesep))),
        "MMDDYYYY" | "FULLDATE" => Some(FormatValue::Text(join(&[&mm, &dd, &yyyy], datesep))),
        "YYDDD" => Some(FormatValue::Text(join(&[&yy, &ddd], datesep))),
        "YYYYDDD" => Some(FormatValue::Text(join(&[&yyyy, &ddd], datesep))),
        "DATEFORM" => Some(FormatValue::Text("MMDDYY".to_string())),
        "TIME" => Some(FormatValue::Text(join(&[&hh, &mi, &ss], timesep))),
        "DAYCOUNT" => Some(FormatValue::Number((secs + EPOCH_1900_TO_1970_SECONDS) / SECONDS_PER_DAY)),
        "DAYOFWEEK" => Some(FormatValue::Number(c.cics_weekday())),
        "DAYOFMONTH" => Some(FormatValue::Number(day as i64)),
        "MONTHOFYEAR" => Some(FormatValue::Number(month as i64)),
        "YEAR" => Some(FormatValue::Number(year)),
        "MILLISECONDS" => Some(FormatValue::Number(abstime.rem_euclid(1000))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_condition_is_a_dfhresp_row_with_its_number() {
        for &c in Condition::ALL {
            assert_eq!(Some(c.resp()), crate::cics_tables::resp(c.name()), "{c:?}");
            assert_eq!(Condition::from_name(c.name()), Some(c));
        }
        let rows: Vec<&str> = crate::cics_tables::resp_table().iter().map(|r| r.condition).collect();
        let names: Vec<&str> = Condition::ALL.iter().map(|c| c.name()).collect();
        assert_eq!(names, rows, "one variant per DFHRESP row, in its order");
    }

    #[test]
    fn every_default_abend_is_for_a_dfhresp_condition_and_listed_once() {
        for (i, &(c, abend)) in DEFAULT_ABENDS.iter().enumerate() {
            assert!(crate::cics_tables::resp(c.name()).is_some(), "{c:?}");
            assert!(DEFAULT_ABENDS[..i].iter().all(|&(d, _)| d != c), "{c:?} is listed twice");
            assert_eq!(c.default_abend(), abend);
        }
        assert_eq!(DEFAULT_ABENDS.len(), 38);
        let codes = [Condition::QIDERR, Condition::ROLLEDBACK, Condition::FILENOTFOUND, Condition::INVREQ, Condition::INVMPSZ].map(Condition::default_abend);
        assert_eq!(codes, ["AEYH", "AEXJ", "AEIL", "AEIP", "AEYB"]);
    }

    #[test]
    fn dsiderr_is_filenotfound_by_its_older_name() {
        let c = Condition::from_name("dsiderr").unwrap();
        assert_eq!((c, c.name(), c.resp()), (Condition::FILENOTFOUND, "FILENOTFOUND", 12));
        assert_eq!(Condition::from_name("NOSUCH"), None);
    }

    #[test]
    fn parse_file_ksds() {
        let (name, def) = parse_file("MYFILE=/tmp/data,KSDS,key=0:8,len=80").unwrap();
        assert_eq!(name, "MYFILE");
        assert_eq!(def.record_len, 80);
        assert!(matches!(&def.data_set, DataSet::Ksds { key } if key.offset == 0 && key.len == 8));
    }

    #[test]
    fn parse_file_rrds_text() {
        let (name, def) = parse_file("myfile=/tmp/data,rrds,len=120,text").unwrap();
        assert_eq!(name, "MYFILE");
        assert_eq!(def.record_len, 120);
        assert!(matches!(def.data_set, DataSet::Rrds));
        assert_eq!(def.dd.format, Some(Format::Text));
    }

    #[test]
    fn parse_file_errors() {
        assert!(parse_file("F=/tmp/d,KSDS,len=80").is_err());
        assert!(parse_file("F=/tmp/d,RRDS,len=abc").is_err());
        assert!(parse_file("F=/tmp/d,FOO,len=80").is_err());
    }

    #[test]
    fn ts_queue() {
        let mut task = Task::default();
        task.writeq_ts("Q1", None, b"one").unwrap();
        task.writeq_ts("Q1", None, b"two").unwrap();
        task.writeq_ts("Q1", None, b"three").unwrap();

        let (data, count) = task.readq_ts("Q1", Some(2)).unwrap();
        assert_eq!(data, b"two");
        assert_eq!(count, 3);

        let (data, count) = task.readq_ts("Q1", None).unwrap();
        assert_eq!(data, b"three");
        assert_eq!(count, 3);

        assert_eq!(task.readq_ts("Q1", None), Err(Condition::ITEMERR));

        task.writeq_ts("Q1", Some(1), b"ONE").unwrap();
        let (data, _) = task.readq_ts("Q1", Some(1)).unwrap();
        assert_eq!(data, b"ONE");

        assert_eq!(task.writeq_ts("Q1", Some(9), b"x"), Err(Condition::ITEMERR));
        assert_eq!(task.readq_ts("NOPE", None), Err(Condition::QIDERR));

        task.deleteq_ts("Q1").unwrap();
        assert_eq!(task.readq_ts("Q1", None), Err(Condition::QIDERR));
    }

    #[test]
    fn td_queue() {
        let mut task = Task::default();
        task.writeq_td("Q1", b"first");
        task.writeq_td("Q1", b"second");
        task.writeq_td("Q1", b"third");

        assert_eq!(task.readq_td("Q1").unwrap(), b"first");
        assert_eq!(task.readq_td("Q1").unwrap(), b"second");
        assert_eq!(task.readq_td("Q1").unwrap(), b"third");
        assert_eq!(task.readq_td("Q1"), Err(Condition::QZERO));
    }

    #[test]
    fn abstime_and_unix() {
        assert_eq!(abstime(0, 0), 2_208_988_800_000);
        assert_eq!(abstime(1, 50), 2_208_988_801_500);
        assert_eq!(unix_seconds(abstime(1, 50)), 1);
        assert_eq!(unix_seconds(abstime(0, 0)), 0);
    }

    #[test]
    fn eib_date_and_time() {
        // 2026-09-27 13:05:09 UTC
        let at = abstime(1_790_514_309, 0);
        assert_eq!(eib_date(at), 126_270);
        assert_eq!(eib_time(at), 130_509);

        // 1999-12-31 00:00:00 UTC
        let at = abstime(946_598_400, 0);
        assert_eq!(eib_date(at), 99_365);
        assert_eq!(eib_time(at), 0);
    }

    #[test]
    fn format_time_options() {
        let at = abstime(1_790_514_309, 25);

        assert_eq!(format_time(at, "YYYYMMDD", Some('/'), None), Some(FormatValue::Text("2026/09/27".into())));
        assert_eq!(format_time(at, "MMDDYY", None, None), Some(FormatValue::Text("092726".into())));
        assert_eq!(format_time(at, "YYDDD", Some('-'), None), Some(FormatValue::Text("26-270".into())));
        assert_eq!(format_time(at, "TIME", None, Some(':')), Some(FormatValue::Text("13:05:09".into())));
        assert_eq!(format_time(at, "DAYOFWEEK", None, None), Some(FormatValue::Number(0)));
        assert_eq!(format_time(at, "DAYOFMONTH", None, None), Some(FormatValue::Number(27)));
        assert_eq!(format_time(at, "MONTHOFYEAR", None, None), Some(FormatValue::Number(9)));
        assert_eq!(format_time(at, "YEAR", None, None), Some(FormatValue::Number(2026)));
        assert_eq!(format_time(at, "MILLISECONDS", None, None), Some(FormatValue::Number(250)));
        assert_eq!(format_time(at, "DAYCOUNT", None, None), Some(FormatValue::Number(46_290)));
        assert_eq!(format_time(at, "STRINGFORMAT", None, None), None);
    }
}
