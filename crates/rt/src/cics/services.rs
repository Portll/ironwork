//! Services: time, ASSIGN, storage, terminal text, the temporary-storage and transient-data
//! queues, and SYNCPOINT.

use super::command::{Assign, Datum, Record};
use super::run::{At, CicsHost, EIBDATE, EIBTIME, Flow, R};
use super::run::{bytes, deliver, eib_packed, encoded, int, ok, page, raise, sent, store_bytes, store_int, store_pointer, task, text};
use super::{Condition, FormatValue, abstime, eib_date, eib_time, format_time};
use crate::abend::Abend;
use crate::vocab::Pos;

/// An option that may be absent, as a service takes it.
type Arg<'c, P, O, S> = Option<&'c Datum<P, O, S>>;

/// The largest GETMAIN the harness will grant.
const GETMAIN_LIMIT: usize = 1 << 28;

pub(super) fn asktime<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, abstime_to: Option<&Datum<P, O, S>>) -> R<Flow> {
    let (seconds, hundredths) = x.unit().now();
    let now = abstime(seconds, hundredths);
    eib_packed(x.unit(), EIBDATE, eib_date(now));
    eib_packed(x.unit(), EIBTIME, eib_time(now));
    store_int(x, abstime_to, now, at.pos)?;
    ok(x, at)
}

/// A separator option: its first character, or `default` when it is given without an argument.
fn separator<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, d: Option<&Datum<P, O, S>>, default: char, pos: Pos) -> R<Option<char>> {
    if d.is_none() {
        return Ok(None);
    }
    Ok(Some(text(x, d, pos)?.and_then(|t| t.chars().next()).unwrap_or(default)))
}

pub(super) fn formattime<'w, P: Copy, O, S>(
    x: &mut impl CicsHost<'w, P, O, S>,
    at: &At<P, O, S>,
    abstime: Option<&Datum<P, O, S>>,
    datesep: Option<&Datum<P, O, S>>,
    timesep: Option<&Datum<P, O, S>>,
    outputs: &[(S, Datum<P, O, S>)],
) -> R<Flow> {
    let Some(abstime) = int(x, abstime, at.pos)? else {
        return Err(Abend::ironwork("EXEC CICS FORMATTIME needs ABSTIME", at.pos));
    };
    let datesep = separator(x, datesep, '/', at.pos)?;
    let timesep = separator(x, timesep, ':', at.pos)?;
    for (name, output) in outputs {
        let name = x.text(name);
        match format_time(abstime, &name, datesep, timesep) {
            Some(FormatValue::Text(t)) => {
                let bytes = encoded(page(x), &t);
                store_bytes(x, at, Some(output), &name, &bytes)?;
            }
            Some(FormatValue::Number(n)) => store_int(x, Some(output), n, at.pos)?,
            None => return Err(Abend::ironwork(format!("EXEC CICS FORMATTIME {name} is not supported"), at.pos)),
        }
    }
    ok(x, at)
}

pub(super) fn assign<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, a: &Assign<P, O, S>) -> R<Flow> {
    let (applid, sysid, userid, termid, abcode) = match x.unit().cics.as_ref() {
        Some(t) => (t.applid.clone(), t.sysid.clone(), t.userid.clone(), t.termid.clone(), t.abcode.clone().unwrap_or_default()),
        None => Default::default(),
    };
    let texts = [
        ("APPLID", &a.applid, applid),
        ("SYSID", &a.sysid, sysid),
        ("USERID", &a.userid, userid),
        ("NETNAME", &a.netname, termid.clone()),
        ("FACILITY", &a.facility, termid),
        ("STARTCODE", &a.startcode, "TD".to_owned()),
        ("ABCODE", &a.abcode, abcode),
        ("PROGRAM", &a.program, x.program_id()),
    ];
    for (name, option, text) in texts {
        if option.is_some() {
            let bytes = encoded(page(x), &text);
            store_bytes(x, at, option.as_ref(), name, &bytes)?;
        }
    }
    store_int(x, a.cwaleng.as_ref(), 0, at.pos)?;
    store_int(x, a.twaleng.as_ref(), 0, at.pos)?;
    ok(x, at)
}

pub(super) fn getmain<'w, P: Copy, O, S>(
    x: &mut impl CicsHost<'w, P, O, S>,
    at: &At<P, O, S>,
    flength: Option<&Datum<P, O, S>>,
    length: Option<&Datum<P, O, S>>,
    initimg: Option<&Datum<P, O, S>>,
    set: Option<&Datum<P, O, S>>,
) -> R<Flow> {
    let length = match int(x, flength, at.pos)? {
        Some(n) => n,
        None => int(x, length, at.pos)?.ok_or_else(|| Abend::ironwork("EXEC CICS GETMAIN needs FLENGTH or LENGTH", at.pos))?,
    };
    if length < 0 || length as usize > GETMAIN_LIMIT {
        return raise(x, at, super::Condition::LENGERR, 0);
    }
    let fill = bytes(x, initimg, at.pos)?.and_then(|b| b.first().copied()).unwrap_or(0);
    let area = x.unit().push_temporary(&vec![fill; length as usize]);
    store_pointer(x, set, Some(area), at.pos)?;
    ok(x, at)
}

pub(super) fn address<'w, P: Copy, O, S>(
    x: &mut impl CicsHost<'w, P, O, S>,
    at: &At<P, O, S>,
    eib: Option<&Datum<P, O, S>>,
    commarea: Option<&Datum<P, O, S>>,
    cwa: Option<&Datum<P, O, S>>,
    twa: Option<&Datum<P, O, S>>,
) -> R<Flow> {
    if eib.is_some() {
        let block = x.unit().eib;
        store_pointer(x, eib, Some(block), at.pos)?;
    }
    if commarea.is_some() {
        let address = x.commarea();
        store_pointer(x, commarea, address, at.pos)?;
    }
    store_pointer(x, cwa, None, at.pos)?;
    store_pointer(x, twa, None, at.pos)?;
    ok(x, at)
}

pub(super) fn send_text<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, from: Option<&Datum<P, O, S>>, length: Option<&Datum<P, O, S>>) -> R<Flow> {
    let bytes = sent(x, at, from, length, "FROM")?;
    let text = page(x).decode(&bytes);
    let _ = writeln!(x.unit().out, "{text}");
    ok(x, at)
}

pub(super) fn write_operator<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, text: Option<&Datum<P, O, S>>, textlength: Option<&Datum<P, O, S>>) -> R<Flow> {
    let bytes = sent(x, at, text, textlength, "TEXT")?;
    let text = page(x).decode(&bytes);
    let _ = writeln!(x.unit().err, "{text}");
    ok(x, at)
}

fn queue_name<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, queue: Option<&Datum<P, O, S>>) -> R<String> {
    text(x, queue, at.pos)?.ok_or_else(|| Abend::ironwork(format!("EXEC CICS {} needs QUEUE", at.name), at.pos))
}

/// `from` is FROM and LENGTH.
pub(super) fn writeq_ts<'w, P: Copy, O, S>(
    x: &mut impl CicsHost<'w, P, O, S>,
    at: &At<P, O, S>,
    queue: Option<&Datum<P, O, S>>,
    from: (Arg<P, O, S>, Arg<P, O, S>),
    rewrite: bool,
    item: Option<&Datum<P, O, S>>,
    numitems: Option<&Datum<P, O, S>>,
) -> R<Flow> {
    let queue = queue_name(x, at, queue)?;
    let data = sent(x, at, from.0, from.1, "FROM")?;
    let rewrite = if rewrite { Some(int(x, item, at.pos)?.unwrap_or(0).max(0) as usize) } else { None };
    match task(x).writeq_ts(&queue, rewrite, &data) {
        Ok(written) => {
            let count = task(x).ts.get(queue.trim_end()).map_or(0, |q| q.items.len());
            if rewrite.is_none() {
                store_int(x, item, written as i64, at.pos)?;
            }
            store_int(x, numitems, count as i64, at.pos)?;
            ok(x, at)
        }
        Err(condition) => raise(x, at, condition, 0),
    }
}

pub(super) fn readq_ts<'w, P: Copy, O, S>(
    x: &mut impl CicsHost<'w, P, O, S>,
    at: &At<P, O, S>,
    queue: Option<&Datum<P, O, S>>,
    next: bool,
    item: Option<&Datum<P, O, S>>,
    numitems: Option<&Datum<P, O, S>>,
    record: &Record<P, O, S>,
) -> R<Flow> {
    let queue = queue_name(x, at, queue)?;
    let item = if next { None } else { int(x, item, at.pos)?.map(|n| n.max(0) as usize) };
    match task(x).readq_ts(&queue, item) {
        Ok((data, count)) => {
            x.unit().take_input();
            store_int(x, numitems, count as i64, at.pos)?;
            deliver(x, at, record, &data)
        }
        Err(condition) => raise(x, at, condition, 0),
    }
}

pub(super) fn deleteq_ts<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, queue: Option<&Datum<P, O, S>>) -> R<Flow> {
    let queue = queue_name(x, at, queue)?;
    match task(x).deleteq_ts(&queue) {
        Ok(()) => ok(x, at),
        Err(condition) => raise(x, at, condition, 0),
    }
}

pub(super) fn writeq_td<'w, P: Copy, O, S>(
    x: &mut impl CicsHost<'w, P, O, S>,
    at: &At<P, O, S>,
    queue: Option<&Datum<P, O, S>>,
    from: Option<&Datum<P, O, S>>,
    length: Option<&Datum<P, O, S>>,
) -> R<Flow> {
    let queue = queue_name(x, at, queue)?;
    let data = sent(x, at, from, length, "FROM")?;
    task(x).writeq_td(&queue, &data);
    ok(x, at)
}

pub(super) fn readq_td<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, queue: Option<&Datum<P, O, S>>, record: &Record<P, O, S>) -> R<Flow> {
    let queue = queue_name(x, at, queue)?;
    match task(x).readq_td(&queue) {
        Ok(data) => {
            x.unit().take_input();
            deliver(x, at, record, &data)
        }
        Err(condition) => raise(x, at, condition, 0),
    }
}

pub(super) fn deleteq_td<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, queue: Option<&Datum<P, O, S>>) -> R<Flow> {
    let queue = queue_name(x, at, queue)?;
    task(x).deleteq_td(&queue);
    ok(x, at)
}

/// SYNCPOINT commits the task's unit of work, and SYNCPOINT ROLLBACK backs it out. A commit the
/// database refuses leaves the work backed out and raises ROLLEDBACK.
pub(super) fn syncpoint<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, rollback: bool) -> R<Flow> {
    let program = x.program_id();
    if let Some(session) = x.unit().sql.as_mut() {
        let answer = session.settle(&program, !rollback).map_err(|a| Abend { code: a.code.into(), message: a.message, pos: at.pos, file: None })?;
        if answer.sqlcode < 0 && !rollback {
            return raise(x, at, Condition::ROLLEDBACK, 0);
        }
        if answer.sqlcode < 0 {
            let message = format!("SYNCPOINT ROLLBACK: the database refused to roll back with SQLCODE {}", answer.sqlcode);
            return Err(Abend { code: "SQL".into(), message, pos: at.pos, file: None });
        }
    }
    ok(x, at)
}
