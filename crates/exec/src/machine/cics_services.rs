//! EXEC CICS services: time, ASSIGN, storage, terminal text and the temporary-storage and
//! transient-data queues.

use super::*;
use super::cics::{EIBDATE, EIBTIME, has};
use crate::cics::{self, FormatValue};

/// The largest GETMAIN the harness will grant.
const GETMAIN_LIMIT: usize = 1 << 28;

/// FORMATTIME options that are not outputs.
const FORMAT_CONTROLS: &[&str] = &["ABSTIME", "DATESEP", "TIMESEP", "RESP", "RESP2", "NOHANDLE"];

impl<'p> Machine<'p, '_, '_> {
    pub(super) fn cics_service(&mut self, block: &'p ExecBlock) -> R<Flow> {
        match block.command.as_str() {
            "ASKTIME" => self.cics_asktime(block),
            "FORMATTIME" => self.cics_formattime(block),
            "ASSIGN" => self.cics_assign(block),
            "GETMAIN" => self.cics_getmain(block),
            "FREEMAIN" | "ENQ" | "DEQ" | "DELAY" => self.cics_ok(block),
            "SYNCPOINT" => self.cics_syncpoint(block),
            "ADDRESS" => self.cics_address(block),
            "SEND TEXT" => self.cics_send_text(block),
            "WRITE" => self.cics_write_operator(block),
            "WRITEQ TS" => self.cics_writeq_ts(block),
            "READQ TS" => self.cics_readq_ts(block),
            "DELETEQ TS" => self.cics_deleteq_ts(block),
            "WRITEQ TD" => self.cics_writeq_td(block),
            "READQ TD" => self.cics_readq_td(block),
            "DELETEQ TD" => self.cics_deleteq_td(block),
            _ => Err(Abend::ironwork(format!("EXEC CICS {} is not supported yet", block.command), block.pos)),
        }
    }

    /// The task; `cics` has already refused to dispatch outside one.
    fn task(&mut self) -> &mut cics::Task {
        self.unit.cics.as_mut().expect("EXEC CICS dispatch checked for a task")
    }

    fn encoded(&self, text: &str) -> Vec<u8> {
        text.chars().map(|c| self.page.encode_char(c).unwrap_or(ebcdic::SPACE)).collect()
    }

    fn cics_asktime(&mut self, block: &ExecBlock) -> R<Flow> {
        let (seconds, hundredths) = self.unit.now();
        let abstime = cics::abstime(seconds, hundredths);
        self.eib_packed(EIBDATE, cics::eib_date(abstime));
        self.eib_packed(EIBTIME, cics::eib_time(abstime));
        self.store_int(block, "ABSTIME", abstime)?;
        self.cics_ok(block)
    }

    /// A separator option: its first character, or `default` when it is given without an argument.
    fn separator(&mut self, block: &ExecBlock, name: &str, default: char) -> R<Option<char>> {
        if !has(block, name) {
            return Ok(None);
        }
        Ok(Some(self.arg_text(block, name)?.and_then(|t| t.chars().next()).unwrap_or(default)))
    }

    fn cics_formattime(&mut self, block: &ExecBlock) -> R<Flow> {
        let Some(abstime) = self.arg_int(block, "ABSTIME")? else {
            return Err(Abend::ironwork("EXEC CICS FORMATTIME needs ABSTIME", block.pos));
        };
        let datesep = self.separator(block, "DATESEP", '/')?;
        let timesep = self.separator(block, "TIMESEP", ':')?;
        for (name, _) in block.options.iter().filter(|(n, _)| !FORMAT_CONTROLS.contains(&n.as_str())) {
            match cics::format_time(abstime, name, datesep, timesep) {
                Some(FormatValue::Text(t)) => {
                    let bytes = self.encoded(&t);
                    self.store_bytes(block, name, &bytes)?;
                }
                Some(FormatValue::Number(n)) => self.store_int(block, name, n)?,
                None => return Err(Abend::ironwork(format!("EXEC CICS FORMATTIME {name} is not supported"), block.pos)),
            }
        }
        self.cics_ok(block)
    }

    fn cics_assign(&mut self, block: &ExecBlock) -> R<Flow> {
        let (applid, sysid, userid, termid) = match self.unit.cics.as_ref() {
            Some(t) => (t.applid.clone(), t.sysid.clone(), t.userid.clone(), t.termid.clone()),
            None => Default::default(),
        };
        let program = self.program.id.clone();
        let texts = [
            ("APPLID", applid),
            ("SYSID", sysid),
            ("USERID", userid),
            ("NETNAME", termid.clone()),
            ("FACILITY", termid),
            ("STARTCODE", "TD".to_owned()),
            ("ABCODE", String::new()),
            ("PROGRAM", program),
        ];
        for (name, text) in texts {
            if has(block, name) {
                let bytes = self.encoded(&text);
                self.store_bytes(block, name, &bytes)?;
            }
        }
        self.store_int(block, "CWALENG", 0)?;
        self.store_int(block, "TWALENG", 0)?;
        self.cics_ok(block)
    }

    fn cics_getmain(&mut self, block: &ExecBlock) -> R<Flow> {
        let length = match self.arg_int(block, "FLENGTH")? {
            Some(n) => n,
            None => self.arg_int(block, "LENGTH")?.ok_or_else(|| Abend::ironwork("EXEC CICS GETMAIN needs FLENGTH or LENGTH", block.pos))?,
        };
        if length < 0 || length as usize > GETMAIN_LIMIT {
            return self.raise(block, "LENGERR", 0);
        }
        let fill = self.arg_bytes(block, "INITIMG")?.and_then(|b| b.first().copied()).unwrap_or(0);
        let at = self.unit.push_temporary(&vec![fill; length as usize]);
        self.store_pointer(block, "SET", Some(at))?;
        self.cics_ok(block)
    }

    fn cics_address(&mut self, block: &ExecBlock) -> R<Flow> {
        if has(block, "EIB") {
            let eib = self.unit.eib;
            self.store_pointer(block, "EIB", Some(eib))?;
        }
        if has(block, "COMMAREA") {
            let ordinal = self.layout.linkage_roots.iter().position(|&i| self.layout.items[i].name.as_deref() == Some("DFHCOMMAREA"));
            let address = ordinal.and_then(|o| self.linkage[o]);
            self.store_pointer(block, "COMMAREA", address)?;
        }
        self.store_pointer(block, "CWA", None)?;
        self.store_pointer(block, "TWA", None)?;
        self.cics_ok(block)
    }

    /// FROM's bytes cut to LENGTH (or TEXT and TEXTLENGTH for WRITE OPERATOR).
    pub(super) fn sent_bytes(&mut self, block: &ExecBlock, from: &str, length: &str) -> R<Vec<u8>> {
        let Some(mut bytes) = self.arg_bytes(block, from)? else {
            return Err(Abend::ironwork(format!("EXEC CICS {} needs {from}", block.command), block.pos));
        };
        if let Some(n) = self.arg_int(block, length)? {
            bytes.truncate(n.max(0) as usize);
        }
        Ok(bytes)
    }

    fn cics_send_text(&mut self, block: &ExecBlock) -> R<Flow> {
        let bytes = self.sent_bytes(block, "FROM", "LENGTH")?;
        let text = self.page.decode(&bytes);
        let _ = writeln!(self.unit.out, "{text}");
        self.cics_ok(block)
    }

    fn cics_write_operator(&mut self, block: &ExecBlock) -> R<Flow> {
        let bytes = self.sent_bytes(block, "TEXT", "TEXTLENGTH")?;
        let text = self.page.decode(&bytes);
        let _ = writeln!(self.unit.err, "{text}");
        self.cics_ok(block)
    }

    fn queue_name(&mut self, block: &ExecBlock) -> R<String> {
        let name = match self.arg_text(block, "QUEUE")? {
            Some(n) => Some(n),
            None => self.arg_text(block, "QNAME")?,
        };
        name.ok_or_else(|| Abend::ironwork(format!("EXEC CICS {} needs QUEUE", block.command), block.pos))
    }

    fn cics_writeq_ts(&mut self, block: &ExecBlock) -> R<Flow> {
        let queue = self.queue_name(block)?;
        let data = self.sent_bytes(block, "FROM", "LENGTH")?;
        let rewrite = if has(block, "REWRITE") { Some(self.arg_int(block, "ITEM")?.unwrap_or(0).max(0) as usize) } else { None };
        let written = self.task().writeq_ts(&queue, rewrite, &data);
        match written {
            Ok(item) => {
                let count = self.task().ts.get(queue.trim_end()).map_or(0, |q| q.items.len());
                if rewrite.is_none() {
                    self.store_int(block, "ITEM", item as i64)?;
                }
                self.store_int(block, "NUMITEMS", count as i64)?;
                self.cics_ok(block)
            }
            Err(condition) => self.raise(block, condition, 0),
        }
    }

    fn cics_readq_ts(&mut self, block: &ExecBlock) -> R<Flow> {
        let queue = self.queue_name(block)?;
        let item = if has(block, "NEXT") { None } else { self.arg_int(block, "ITEM")?.map(|n| n.max(0) as usize) };
        let read = self.task().readq_ts(&queue, item);
        match read {
            Ok((data, count)) => {
                self.store_int(block, "NUMITEMS", count as i64)?;
                self.deliver_record(block, &data)
            }
            Err(condition) => self.raise(block, condition, 0),
        }
    }

    fn cics_deleteq_ts(&mut self, block: &ExecBlock) -> R<Flow> {
        let queue = self.queue_name(block)?;
        let deleted = self.task().deleteq_ts(&queue);
        match deleted {
            Ok(()) => self.cics_ok(block),
            Err(condition) => self.raise(block, condition, 0),
        }
    }

    fn cics_writeq_td(&mut self, block: &ExecBlock) -> R<Flow> {
        let queue = self.queue_name(block)?;
        let data = self.sent_bytes(block, "FROM", "LENGTH")?;
        self.task().writeq_td(&queue, &data);
        self.cics_ok(block)
    }

    fn cics_readq_td(&mut self, block: &ExecBlock) -> R<Flow> {
        let queue = self.queue_name(block)?;
        let read = self.task().readq_td(&queue);
        match read {
            Ok(data) => self.deliver_record(block, &data),
            Err(condition) => self.raise(block, condition, 0),
        }
    }

    fn cics_deleteq_td(&mut self, block: &ExecBlock) -> R<Flow> {
        let queue = self.queue_name(block)?;
        self.task().deleteq_td(&queue);
        self.cics_ok(block)
    }
}
