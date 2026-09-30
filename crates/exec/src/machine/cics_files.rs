//! EXEC CICS file control over the task's VSAM files: READ, WRITE, REWRITE, DELETE and UNLOCK, and
//! browsing with STARTBR, READNEXT, READPREV, RESETBR and ENDBR. A file opens on first use from its
//! --file definition and is written back when the task ends.

use super::cics::{EIBFN, has};
use super::*;
use crate::cics::{Browse, Condition};
use crate::files::{self, FileStatus, Keyed, Keying};

/// The first record at or after `key`: matching it exactly, or (GENERIC) starting with it, or
/// (GTEQ) any record from there on.
fn find(keyed: &Keyed, key: &[u8], generic: bool, gteq: bool) -> Option<(Vec<u8>, Vec<u8>)> {
    let (k, r) = keyed.seek(key, true, false)?;
    let matches = if generic { k.starts_with(key) } else { k == key };
    (matches || gteq).then_some((k, r))
}

/// A keyed store's insert or replace.
type Put = fn(&mut Keyed, Vec<u8>, Vec<u8>) -> Result<bool, FileStatus>;

impl<'p> Machine<'p, '_, '_> {
    pub(super) fn cics_file(&mut self, block: &'p ExecBlock) -> R<Flow> {
        let named = self.arg_text_any(block, &["FILE", "DATASET"])?;
        let Some(file) = named.map(|f| f.to_ascii_uppercase()) else {
            return Err(Abend::ironwork(format!("EXEC CICS {} needs FILE", block.command), block.pos));
        };
        self.eib_text(super::cics::EIBRSRCE, 8, &file);
        match block.command.as_str() {
            "READ" => self.file_read(block, &file),
            "WRITE" => self.file_write(block, &file),
            "REWRITE" => self.file_rewrite(block, &file),
            "DELETE" => self.file_delete(block, &file),
            "UNLOCK" => {
                self.release_hold(&file);
                self.cics_ok(block)
            }
            "STARTBR" => self.file_startbr(block, &file, false),
            "RESETBR" => self.file_startbr(block, &file, true),
            "READNEXT" => self.file_browse(block, &file, false),
            "READPREV" => self.file_browse(block, &file, true),
            "ENDBR" => {
                let reqid = self.reqid(block)?;
                match self.unit.cics.as_mut().and_then(|t| t.browses.remove(&(file, reqid))) {
                    Some(_) => self.cics_ok(block),
                    None => self.raise(block, Condition::INVREQ, 0),
                }
            }
            other => Err(Abend::ironwork(format!("EXEC CICS {other} is not supported yet"), block.pos)),
        }
    }

    /// Runs `op` on a file's store, opening it on first use. Err names the condition to raise:
    /// FILENOTFOUND when no --file defines it, NOTOPEN when its data set cannot be read.
    fn on_file<T>(&mut self, file: &str, op: impl FnOnce(&mut Self, &mut Keyed) -> R<T>) -> R<Result<T, Condition>> {
        let mut open = match self.unit.cics_files.remove(file) {
            Some(open) => open,
            None => {
                let Some(def) = self.unit.cics.as_ref().and_then(|t| t.files.get(file)).cloned() else { return Ok(Err(Condition::FILENOTFOUND)) };
                match files::open_keyed(Some(&def.dd), OpenMode::InputOutput, def.format(), def.keying(), def.record_len, self.page) {
                    Ok(open) => open,
                    Err(_) => return Ok(Err(Condition::NOTOPEN)),
                }
            }
        };
        let result = match open.keyed() {
            Some(keyed) => op(self, keyed).map(Ok),
            None => Ok(Err(Condition::INVREQ)),
        };
        self.unit.cics_files.insert(file.to_owned(), open);
        result
    }

    /// RIDFLD as a key: record-number bytes for a relative file, else its bytes cut to KEYLENGTH
    /// (or to the key's length). True when the search is GENERIC.
    fn ridfld(&mut self, block: &ExecBlock, keying: &Keying) -> R<(Vec<u8>, bool)> {
        if has(block, "RRN") || *keying == Keying::Relative {
            let n = self.arg_int(block, "RIDFLD")?.unwrap_or(0);
            return Ok((files::record_number(n.max(0) as u64), false));
        }
        let mut key = self.arg_bytes(block, "RIDFLD")?.unwrap_or_default();
        match (self.arg_int(block, "KEYLENGTH")?, keying) {
            (Some(n), _) => key.truncate(n.max(0) as usize),
            (None, Keying::Indexed { prime, .. }) => key.truncate(prime.len),
            _ => {}
        }
        Ok((key, has(block, "GENERIC")))
    }

    /// Puts a record's key back into RIDFLD, as CICS does after a GENERIC, GTEQ or browse read.
    fn set_ridfld(&mut self, block: &ExecBlock, key: &[u8], relative: bool) -> R<()> {
        if relative {
            self.store_int(block, "RIDFLD", files::number_of(key) as i64)
        } else {
            self.store_bytes(block, "RIDFLD", key)
        }
    }

    /// Releases the key a READ UPDATE held on a file.
    fn release_hold(&mut self, file: &str) -> Option<Vec<u8>> {
        self.unit.cics.as_mut().and_then(|t| t.held.remove(file))
    }

    fn reqid(&mut self, block: &ExecBlock) -> R<i64> {
        Ok(self.arg_int(block, "REQID")?.unwrap_or(0))
    }

    fn file_read(&mut self, block: &ExecBlock, file: &str) -> R<Flow> {
        self.eib_bytes(EIBFN, &[0x06, 0x02]);
        let gteq = has(block, "GTEQ");
        let found = self.on_file(file, |m, keyed| {
            let (key, generic) = m.ridfld(block, &keyed.keying)?;
            Ok((find(keyed, &key, generic, gteq), keyed.keying == Keying::Relative))
        })?;
        let (key, record, relative) = match found {
            Err(c) => return self.raise(block, c, 0),
            Ok((None, _)) => return self.raise(block, Condition::NOTFND, 0),
            Ok((Some((key, record)), relative)) => (key, record, relative),
        };
        if has(block, "UPDATE")
            && let Some(task) = self.unit.cics.as_mut()
        {
            task.held.insert(file.to_owned(), key.clone());
        }
        if has(block, "GENERIC") || gteq {
            self.set_ridfld(block, &key, relative)?;
        }
        self.deliver_record(block, &record)
    }

    /// Stores `record` under the key `key_of` yields, by `put`; for a KSDS the record's own key must match it.
    fn store_record(
        &mut self,
        block: &ExecBlock,
        file: &str,
        record: Vec<u8>,
        key_of: impl FnOnce(&mut Self, &Keyed) -> R<Vec<u8>>,
        put: Put,
    ) -> R<Flow> {
        let outcome = self.on_file(file, |m, keyed| {
            let key = key_of(m, keyed)?;
            Ok(match keyed.prime_key(&record) {
                Some(own) if own != key => Err(Condition::INVREQ),
                _ => put(keyed, key, record).map(|_| ()).map_err(FileStatus::cics_condition),
            })
        })?;
        match outcome {
            Err(c) | Ok(Err(c)) => self.raise(block, c, 0),
            Ok(Ok(())) => self.cics_ok(block),
        }
    }

    /// WRITE adds a record at RIDFLD. For a KSDS the record's own key must be RIDFLD's.
    fn file_write(&mut self, block: &ExecBlock, file: &str) -> R<Flow> {
        self.eib_bytes(EIBFN, &[0x06, 0x04]);
        let record = self.sent_bytes(block, "FROM", "LENGTH")?;
        self.store_record(block, file, record, |m, keyed| Ok(m.ridfld(block, &keyed.keying)?.0), Keyed::insert)
    }

    /// REWRITE replaces the record a READ UPDATE holds; its key may not change.
    fn file_rewrite(&mut self, block: &ExecBlock, file: &str) -> R<Flow> {
        let Some(key) = self.release_hold(file) else { return self.raise(block, Condition::INVREQ, 0) };
        let record = self.sent_bytes(block, "FROM", "LENGTH")?;
        self.store_record(block, file, record, |_, _| Ok(key), Keyed::replace)
    }

    /// DELETE removes the record at RIDFLD (every record starting with it, when GENERIC, counted
    /// in NUMREC), or without RIDFLD the record a READ UPDATE holds.
    fn file_delete(&mut self, block: &ExecBlock, file: &str) -> R<Flow> {
        let held = if has(block, "RIDFLD") { None } else { Some(self.release_hold(file)) };
        if let Some(None) = held {
            return self.raise(block, Condition::INVREQ, 0);
        }
        let outcome = self.on_file(file, |m, keyed| {
            if let Some(Some(key)) = held {
                return Ok(keyed.remove(&key).map_or(0, |_| 1));
            }
            let (key, generic) = m.ridfld(block, &keyed.keying)?;
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
            Err(c) => self.raise(block, c, 0),
            Ok(0) => self.raise(block, Condition::NOTFND, 0),
            Ok(count) => {
                self.store_int(block, "NUMREC", count)?;
                self.cics_ok(block)
            }
        }
    }

    /// STARTBR (or RESETBR, for a browse already open) positions a browse at the first record
    /// at or after RIDFLD, or (EQUAL) at RIDFLD itself. RIDFLD of all X'FF' positions after the
    /// last record, so READPREV reads backward from the end.
    fn file_startbr(&mut self, block: &ExecBlock, file: &str, reset: bool) -> R<Flow> {
        let reqid = self.reqid(block)?;
        let exists = self.unit.cics.as_ref().is_some_and(|t| t.browses.contains_key(&(file.to_owned(), reqid)));
        if reset != exists {
            return self.raise(block, Condition::INVREQ, 0);
        }
        let gteq = !has(block, "EQUAL");
        let found = self.on_file(file, |m, keyed| {
            let (key, generic) = m.ridfld(block, &keyed.keying)?;
            if !key.is_empty() && key.iter().all(|&b| b == 0xFF) {
                return Ok(Some(Browse { at: key, inclusive: false }));
            }
            Ok(find(keyed, &key, generic, gteq).map(|(k, _)| Browse { at: k, inclusive: true }))
        })?;
        match found {
            Err(c) => self.raise(block, c, 0),
            Ok(None) => self.raise(block, Condition::NOTFND, 0),
            Ok(Some(browse)) => {
                if let Some(task) = self.unit.cics.as_mut() {
                    task.browses.insert((file.to_owned(), reqid), browse);
                }
                self.cics_ok(block)
            }
        }
    }

    /// READNEXT or READPREV: the next record in the browse's direction. A program that changed
    /// RIDFLD to a key the browse is not at skips forward to it, as CICS allows.
    fn file_browse(&mut self, block: &ExecBlock, file: &str, backward: bool) -> R<Flow> {
        let reqid = self.reqid(block)?;
        let Some(browse) = self.unit.cics.as_ref().and_then(|t| t.browses.get(&(file.to_owned(), reqid))).cloned() else {
            return self.raise(block, Condition::INVREQ, 0);
        };
        let found = self.on_file(file, |m, keyed| {
            let relative = keyed.keying == Keying::Relative;
            let (rid, _) = m.ridfld(block, &keyed.keying)?;
            let skip = !backward && !browse.inclusive && !rid.is_empty() && !browse.at.starts_with(&rid);
            let (from, inclusive) = if skip { (rid, true) } else { (browse.at.clone(), browse.inclusive) };
            Ok((keyed.seek(&from, inclusive, backward), relative))
        })?;
        let (key, record, relative) = match found {
            Err(c) => return self.raise(block, c, 0),
            Ok((None, _)) => return self.raise(block, Condition::ENDFILE, 0),
            Ok((Some((key, record)), relative)) => (key, record, relative),
        };
        if let Some(task) = self.unit.cics.as_mut() {
            task.browses.insert((file.to_owned(), reqid), Browse { at: key.clone(), inclusive: false });
        }
        self.set_ridfld(block, &key, relative)?;
        self.deliver_record(block, &record)
    }
}
