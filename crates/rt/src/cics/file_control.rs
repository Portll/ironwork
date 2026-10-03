//! File control over the task's VSAM files: READ, WRITE, REWRITE, DELETE and UNLOCK, and browsing
//! with STARTBR, READNEXT, READPREV, RESETBR and ENDBR. A file opens on first use from its --file
//! definition and is written back when the task ends.

use super::command::{Datum, FileControl, FileOptions};
use super::run::{At, CicsHost, EIBFN, EIBRSRCE, Flow, R};
use super::run::{bytes, deliver, eib_bytes, eib_text, int, ok, page, raise, sent, store_bytes, store_int};
use super::{Browse, Condition};
use crate::abend::{Abend, FileStatus};
use crate::files::{self, Keyed, Keying};
use crate::vocab::OpenMode;

/// The first record at or after `key`: matching it exactly, or (GENERIC) starting with it, or
/// (GTEQ) any record from there on.
fn find(keyed: &Keyed, key: &[u8], generic: bool, gteq: bool) -> Option<(Vec<u8>, Vec<u8>)> {
    let (k, r) = keyed.seek(key, true, false)?;
    let matches = if generic { k.starts_with(key) } else { k == key };
    (matches || gteq).then_some((k, r))
}

/// A keyed store's insert or replace.
type Put = fn(&mut Keyed, Vec<u8>, Vec<u8>) -> Result<bool, FileStatus>;

pub(super) fn run<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, verb: FileControl, file: Option<&Datum<P, O, S>>, o: &FileOptions<P, O, S>) -> R<Flow> {
    let Some(file) = super::run::text(x, file, at.pos)?.map(|f| f.to_ascii_uppercase()) else {
        return Err(Abend::ironwork(format!("EXEC CICS {} needs FILE", at.name), at.pos));
    };
    let page = page(x);
    eib_text(x.unit(), page, EIBRSRCE, 8, &file);
    match verb {
        FileControl::Read => read(x, at, &file, o),
        FileControl::Write => write(x, at, &file, o),
        FileControl::Rewrite => rewrite(x, at, &file, o),
        FileControl::Delete => delete(x, at, &file, o),
        FileControl::Unlock => {
            release_hold(x, &file);
            ok(x, at)
        }
        FileControl::Startbr => startbr(x, at, &file, o, false),
        FileControl::Resetbr => startbr(x, at, &file, o, true),
        FileControl::Readnext => browse(x, at, &file, o, false),
        FileControl::Readprev => browse(x, at, &file, o, true),
        FileControl::Endbr => {
            let reqid = reqid(x, at, o)?;
            match x.unit().cics.as_mut().and_then(|t| t.browses.remove(&(file, reqid))) {
                Some(_) => ok(x, at),
                None => raise(x, at, Condition::INVREQ, 0),
            }
        }
    }
}

/// Runs `op` on a file's store, opening it on first use. Err names the condition to raise:
/// FILENOTFOUND when no --file defines it, NOTOPEN when its data set cannot be read.
fn on_file<'w, P: Copy, O, S, X: CicsHost<'w, P, O, S>, T>(x: &mut X, file: &str, op: impl FnOnce(&mut X, &mut Keyed) -> R<T>) -> R<Result<T, Condition>> {
    let page = page(x);
    let unit = x.unit();
    let mut open = match unit.cics_files.remove(file) {
        Some(open) => open,
        None => {
            let Some(def) = unit.cics.as_ref().and_then(|t| t.files.get(file)).cloned() else { return Ok(Err(Condition::FILENOTFOUND)) };
            match files::open_keyed(Some(&def.dd), OpenMode::InputOutput, def.format(), def.keying(), def.record_len, page) {
                Ok(open) => open,
                Err(_) => return Ok(Err(Condition::NOTOPEN)),
            }
        }
    };
    let result = match open.keyed() {
        Some(keyed) => op(x, keyed).map(Ok),
        None => Ok(Err(Condition::INVREQ)),
    };
    x.unit().cics_files.insert(file.to_owned(), open);
    result
}

/// RIDFLD as a key: record-number bytes for a relative file, else its bytes cut to KEYLENGTH (or
/// to the key's length). True when the search is GENERIC.
fn ridfld<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, o: &FileOptions<P, O, S>, keying: &Keying) -> R<(Vec<u8>, bool)> {
    if o.rrn || *keying == Keying::Relative {
        let n = int(x, o.ridfld.as_ref(), at.pos)?.unwrap_or(0);
        return Ok((files::record_number(n.max(0) as u64), false));
    }
    let mut key = bytes(x, o.ridfld.as_ref(), at.pos)?.unwrap_or_default();
    match (int(x, o.keylength.as_ref(), at.pos)?, keying) {
        (Some(n), _) => key.truncate(n.max(0) as usize),
        (None, Keying::Indexed { prime, .. }) => key.truncate(prime.len),
        _ => {}
    }
    Ok((key, o.generic))
}

/// Puts a record's key back into RIDFLD, as CICS does after a GENERIC, GTEQ or browse read.
fn set_ridfld<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, o: &FileOptions<P, O, S>, key: &[u8], relative: bool) -> R<()> {
    if relative {
        store_int(x, o.ridfld.as_ref(), files::number_of(key) as i64, at.pos)
    } else {
        store_bytes(x, at, o.ridfld.as_ref(), "RIDFLD", key)
    }
}

/// Releases the key a READ UPDATE held on a file.
fn release_hold<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, file: &str) -> Option<Vec<u8>> {
    x.unit().cics.as_mut().and_then(|t| t.held.remove(file))
}

fn reqid<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, o: &FileOptions<P, O, S>) -> R<i64> {
    Ok(int(x, o.reqid.as_ref(), at.pos)?.unwrap_or(0))
}

fn read<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, file: &str, o: &FileOptions<P, O, S>) -> R<Flow> {
    eib_bytes(x.unit(), EIBFN, &[0x06, 0x02]);
    let gteq = o.gteq;
    let found = on_file(x, file, |x, keyed| {
        let (key, generic) = ridfld(x, at, o, &keyed.keying)?;
        Ok((find(keyed, &key, generic, gteq), keyed.keying == Keying::Relative))
    })?;
    let (key, record, relative) = match found {
        Err(c) => return raise(x, at, c, 0),
        Ok((None, _)) => return raise(x, at, Condition::NOTFND, 0),
        Ok((Some((key, record)), relative)) => (key, record, relative),
    };
    x.unit().take_input();
    if o.update
        && let Some(task) = x.unit().cics.as_mut()
    {
        task.held.insert(file.to_owned(), key.clone());
    }
    if o.generic || gteq {
        set_ridfld(x, at, o, &key, relative)?;
    }
    deliver(x, at, &o.record, &record)
}

/// Stores `record` under the key `key_of` yields, by `put`; for a KSDS the record's own key must match it.
fn store_record<'w, P: Copy, O, S, X: CicsHost<'w, P, O, S>>(
    x: &mut X,
    at: &At<P, O, S>,
    file: &str,
    record: Vec<u8>,
    key_of: impl FnOnce(&mut X, &Keyed) -> R<Vec<u8>>,
    put: Put,
) -> R<Flow> {
    let outcome = on_file(x, file, |x, keyed| {
        let key = key_of(x, keyed)?;
        Ok(match keyed.prime_key(&record) {
            Some(own) if own != key => Err(Condition::INVREQ),
            _ => put(keyed, key, record).map(|_| ()).map_err(FileStatus::cics_condition),
        })
    })?;
    match outcome {
        Err(c) | Ok(Err(c)) => raise(x, at, c, 0),
        Ok(Ok(())) => ok(x, at),
    }
}

/// WRITE adds a record at RIDFLD. For a KSDS the record's own key must be RIDFLD's.
fn write<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, file: &str, o: &FileOptions<P, O, S>) -> R<Flow> {
    eib_bytes(x.unit(), EIBFN, &[0x06, 0x04]);
    let record = sent(x, at, o.from.as_ref(), o.record.length.as_ref(), "FROM")?;
    store_record(x, at, file, record, |x, keyed| Ok(ridfld(x, at, o, &keyed.keying)?.0), Keyed::insert)
}

/// REWRITE replaces the record a READ UPDATE holds; its key may not change.
fn rewrite<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, file: &str, o: &FileOptions<P, O, S>) -> R<Flow> {
    let Some(key) = release_hold(x, file) else { return raise(x, at, Condition::INVREQ, 0) };
    let record = sent(x, at, o.from.as_ref(), o.record.length.as_ref(), "FROM")?;
    store_record(x, at, file, record, |_, _| Ok(key), Keyed::replace)
}

/// DELETE removes the record at RIDFLD (every record starting with it, when GENERIC, counted in
/// NUMREC), or without RIDFLD the record a READ UPDATE holds.
fn delete<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, file: &str, o: &FileOptions<P, O, S>) -> R<Flow> {
    let held = if o.ridfld.is_some() { None } else { Some(release_hold(x, file)) };
    if let Some(None) = held {
        return raise(x, at, Condition::INVREQ, 0);
    }
    let outcome = on_file(x, file, |x, keyed| {
        if let Some(Some(key)) = held {
            return Ok(keyed.remove(&key).map_or(0, |_| 1));
        }
        let (key, generic) = ridfld(x, at, o, &keyed.keying)?;
        if !generic {
            return Ok(keyed.remove(&key).map_or(0, |_| 1));
        }
        let mut count = 0;
        while let Some((k, _)) = keyed.seek(&key, true, false).filter(|(k, _)| k.starts_with(&key)) {
            keyed.remove(&k);
            count += 1;
        }
        Ok(count)
    })?;
    match outcome {
        Err(c) => raise(x, at, c, 0),
        Ok(0) => raise(x, at, Condition::NOTFND, 0),
        Ok(count) => {
            store_int(x, o.numrec.as_ref(), count, at.pos)?;
            ok(x, at)
        }
    }
}

/// STARTBR (or RESETBR, for a browse already open) positions a browse at the first record at or
/// after RIDFLD, or (EQUAL) at RIDFLD itself. RIDFLD of all X'FF' positions after the last record,
/// so READPREV reads backward from the end.
fn startbr<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, file: &str, o: &FileOptions<P, O, S>, reset: bool) -> R<Flow> {
    let reqid = reqid(x, at, o)?;
    let exists = x.unit().cics.as_ref().is_some_and(|t| t.browses.contains_key(&(file.to_owned(), reqid)));
    if reset != exists {
        return raise(x, at, Condition::INVREQ, 0);
    }
    let gteq = !o.equal;
    let found = on_file(x, file, |x, keyed| {
        let (key, generic) = ridfld(x, at, o, &keyed.keying)?;
        if !key.is_empty() && key.iter().all(|&b| b == 0xFF) {
            return Ok(Some(Browse { at: key, inclusive: false }));
        }
        Ok(find(keyed, &key, generic, gteq).map(|(k, _)| Browse { at: k, inclusive: true }))
    })?;
    match found {
        Err(c) => raise(x, at, c, 0),
        Ok(None) => raise(x, at, Condition::NOTFND, 0),
        Ok(Some(browse)) => {
            if let Some(task) = x.unit().cics.as_mut() {
                task.browses.insert((file.to_owned(), reqid), browse);
            }
            ok(x, at)
        }
    }
}

/// READNEXT or READPREV: the next record in the browse's direction. A program that changed RIDFLD
/// to a key the browse is not at skips forward to it, as CICS allows.
fn browse<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, file: &str, o: &FileOptions<P, O, S>, backward: bool) -> R<Flow> {
    let reqid = reqid(x, at, o)?;
    let Some(browse) = x.unit().cics.as_ref().and_then(|t| t.browses.get(&(file.to_owned(), reqid))).cloned() else {
        return raise(x, at, Condition::INVREQ, 0);
    };
    let found = on_file(x, file, |x, keyed| {
        let relative = keyed.keying == Keying::Relative;
        let (rid, _) = ridfld(x, at, o, &keyed.keying)?;
        let skip = !backward && !browse.inclusive && !rid.is_empty() && !browse.at.starts_with(&rid);
        let (from, inclusive) = if skip { (rid, true) } else { (browse.at.clone(), browse.inclusive) };
        Ok((keyed.seek(&from, inclusive, backward), relative))
    })?;
    let (key, record, relative) = match found {
        Err(c) => return raise(x, at, c, 0),
        Ok((None, _)) => return raise(x, at, Condition::ENDFILE, 0),
        Ok((Some((key, record)), relative)) => (key, record, relative),
    };
    x.unit().take_input();
    if let Some(task) = x.unit().cics.as_mut() {
        task.browses.insert((file.to_owned(), reqid), Browse { at: key.clone(), inclusive: false });
    }
    set_ridfld(x, at, o, &key, relative)?;
    deliver(x, at, &o.record, &record)
}
