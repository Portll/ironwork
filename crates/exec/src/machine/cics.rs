//! EXEC CICS commands, run against the harness's task: dispatch, arguments, the EXEC interface
//! block, exception conditions and program control. Services are in cics_services.rs, file control
//! in cics_files.rs.

use super::*;
use crate::cics;

#[derive(Clone, Debug, Default)]
pub(super) struct Handlers {
    pub conditions: HashMap<String, Handler>,
    pub stack: Vec<HashMap<String, Handler>>,
    pub abend: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Handler {
    Label(usize),
    Ignore,
}

pub(super) const EIBTIME: usize = 0x00;
pub(super) const EIBDATE: usize = 0x04;
pub(super) const EIBTRNID: usize = 0x08;
pub(super) const EIBTASKN: usize = 0x0C;
pub(super) const EIBTRMID: usize = 0x10;
pub(super) const EIBCPOSN: usize = 0x16;
pub(super) const EIBCALEN: usize = 0x18;
pub(super) const EIBAID: usize = 0x1A;
pub(super) const EIBFN: usize = 0x1B;
pub(super) const EIBRSRCE: usize = 0x33;
pub(super) const EIBRESP: usize = 0x4C;
pub(super) const EIBRESP2: usize = 0x50;

const FILE_CONTROL: &[&str] = &["READ", "WRITE", "REWRITE", "DELETE", "UNLOCK", "STARTBR", "READNEXT", "READPREV", "RESETBR", "ENDBR"];

/// Options that are not conditions in HANDLE CONDITION and IGNORE CONDITION.
const NOT_CONDITIONS: &[&str] = &["RESP", "RESP2", "NOHANDLE"];

pub(super) fn option<'b>(block: &'b ExecBlock, name: &str) -> Option<&'b Option<ExecArg>> {
    block.options.iter().find(|(n, _)| n == name).map(|(_, a)| a)
}

pub(super) fn has(block: &ExecBlock, name: &str) -> bool {
    option(block, name).is_some()
}

pub(super) fn operand<'b>(block: &'b ExecBlock, name: &str) -> Option<&'b Operand> {
    match option(block, name) {
        Some(Some(ExecArg::Operand(op))) => Some(op),
        _ => None,
    }
}

/// The transaction abend CICS issues when a condition is raised and nothing handles it.
fn default_abend(condition: &str) -> &'static str {
    match condition {
        "NOTFND" => "AEIM",
        "DUPREC" => "AEIN",
        "DUPKEY" => "AEIO",
        "IOERR" => "AEIQ",
        "NOSPACE" => "AEIR",
        "NOTOPEN" => "AEIS",
        "ENDFILE" => "AEIT",
        "ILLOGIC" => "AEIU",
        "LENGERR" => "AEIV",
        "QZERO" => "AEIW",
        "ITEMERR" => "AEIZ",
        "PGMIDERR" => "AEI0",
        "TRANSIDERR" => "AEI1",
        "ENDDATA" => "AEI2",
        "INVTSREQ" => "AEI3",
        "EXPIRED" => "AEI4",
        "TSIOERR" => "AEI8",
        "MAPFAIL" => "AEI9",
        "ERROR" => "AEIA",
        "EOF" => "AEID",
        "EODS" => "AEIE",
        "INBFMH" => "AEIG",
        "ENDINPT" => "AEIH",
        "NONVAL" => "AEII",
        "NOSTART" => "AEIJ",
        "TERMIDERR" => "AEIK",
        "FILENOTFOUND" => "AEIL",
        "DISABLED" => "AEXL",
        "ROLLEDBACK" => "AEXJ",
        "LOCKED" => "AEX8",
        "RECORDBUSY" => "AEX9",
        "QIDERR" => "AEYH",
        "SYSIDERR" => "AEYQ",
        "NOTAUTH" => "AEY7",
        "USERIDERR" => "AEYX",
        "CONTAINERERR" => "AEZJ",
        "CHANNELERR" => "AEZV",
        _ => "AEIP",
    }
}

impl<'p> Machine<'p, '_, '_> {
    pub(super) fn cics(&mut self, block: &'p ExecBlock) -> R<Flow> {
        if self.unit.cics.is_none() {
            return Err(Abend::ironwork(format!("EXEC CICS {} was reached outside a CICS task: run the program with `ironwork cics`", block.command), block.pos));
        }
        self.eib_fullword(EIBRESP, 0);
        self.eib_fullword(EIBRESP2, 0);
        match block.command.as_str() {
            "WRITE" if has(block, "OPERATOR") => self.cics_service(block),
            c if FILE_CONTROL.contains(&c) => self.cics_file(block),
            "RETURN" => self.cics_return(block),
            "LINK" => self.cics_link(block, false),
            "XCTL" => self.cics_link(block, true),
            "ABEND" => self.cics_abend(block),
            "HANDLE CONDITION" => self.handle_condition(block),
            "IGNORE CONDITION" => {
                for (name, _) in block.options.iter().filter(|(n, _)| !NOT_CONDITIONS.contains(&n.as_str())) {
                    self.cics_handlers.conditions.insert(name.clone(), Handler::Ignore);
                }
                self.cics_ok(block)
            }
            "PUSH HANDLE" => {
                let saved = std::mem::take(&mut self.cics_handlers.conditions);
                self.cics_handlers.stack.push(saved);
                self.cics_ok(block)
            }
            "POP HANDLE" => match self.cics_handlers.stack.pop() {
                Some(saved) => {
                    self.cics_handlers.conditions = saved;
                    self.cics_ok(block)
                }
                None => self.raise(block, "INVREQ", 0),
            },
            "HANDLE ABEND" => self.handle_abend(block),
            "HANDLE AID" => self.cics_ok(block),
            "SEND" | "SEND MAP" if has(block, "MAP") || block.command == "SEND MAP" => self.send_map(block),
            "RECEIVE" | "RECEIVE MAP" if has(block, "MAP") || block.command == "RECEIVE MAP" => self.receive_map(block),
            "SEND CONTROL" => self.send_control(block),
            "RECEIVE" => self.receive_raw(block),
            _ => self.cics_service(block),
        }
    }

    /// An argument's bytes: a data item's storage, or a literal's image.
    pub(super) fn arg_bytes(&mut self, block: &ExecBlock, name: &str) -> R<Option<Vec<u8>>> {
        Ok(match operand(block, name) {
            None => None,
            Some(Operand::Ref(r)) => {
                let loc = self.locate(r)?;
                Some(self.bytes(loc).to_vec())
            }
            Some(op) => Some(self.content_argument(op, block.pos)?),
        })
    }

    /// An argument's bytes, cut to a length option when that is shorter.
    pub(super) fn arg_bytes_cut(&mut self, block: &ExecBlock, data: &str, length: &str) -> R<Option<Vec<u8>>> {
        let Some(mut bytes) = self.arg_bytes(block, data)? else { return Ok(None) };
        if let Some(n) = self.arg_int(block, length)? {
            bytes.truncate(n.max(0) as usize);
        }
        Ok(Some(bytes))
    }

    pub(super) fn arg_int(&mut self, block: &ExecBlock, name: &str) -> R<Option<i64>> {
        match operand(block, name) {
            Some(op) => Ok(Some(self.integer(&Expr::Operand(op.clone()), block.pos)?)),
            None => Ok(None),
        }
    }

    /// A resource name (PROGRAM, TRANSID, QUEUE, FILE, ABCODE) with trailing spaces removed.
    pub(super) fn arg_text(&mut self, block: &ExecBlock, name: &str) -> R<Option<String>> {
        if let Some(Some(ExecArg::Text(t))) = option(block, name) {
            return Ok(Some(t.trim().trim_matches(|c| c == '\'' || c == '"').to_owned()));
        }
        Ok(self.arg_bytes(block, name)?.map(|b| self.page.decode(&b).trim_end().to_owned()))
    }

    /// The first of several alternative options (FILE or DATASET) that is given, as `arg_text`.
    pub(super) fn arg_text_any(&mut self, block: &ExecBlock, names: &[&str]) -> R<Option<String>> {
        for name in names {
            if let Some(text) = self.arg_text(block, name)? {
                return Ok(Some(text));
            }
        }
        Ok(None)
    }

    /// MOVEs bytes into the data item an option names, as INTO and the like receive them.
    pub(super) fn store_bytes(&mut self, block: &ExecBlock, name: &str, bytes: &[u8]) -> R<()> {
        match operand(block, name) {
            None => Ok(()),
            Some(Operand::Ref(r)) => {
                let loc = self.locate(r)?;
                self.assign(loc, Val::Bytes(bytes.to_vec()), None, block.pos)
            }
            Some(_) => Err(Abend::ironwork(format!("EXEC CICS {}: {name} must name a data item", block.command), block.pos)),
        }
    }

    pub(super) fn store_int(&mut self, block: &ExecBlock, name: &str, value: i64) -> R<()> {
        if let Some(Operand::Ref(r)) = operand(block, name) {
            self.set_integer(r, value, block.pos)?;
        }
        Ok(())
    }

    /// Stores an address, or NULL for None, into the POINTER an option names.
    pub(super) fn store_pointer(&mut self, block: &ExecBlock, name: &str, offset: Option<usize>) -> R<()> {
        if let Some(Operand::Ref(r)) = operand(block, name) {
            let loc = self.locate(r)?;
            let address = offset.map_or(0, |o| ADDRESS_BASE + o as u32);
            self.assign(loc, Val::Address(address), None, block.pos)?;
        }
        Ok(())
    }

    pub(super) fn eib_bytes(&mut self, offset: usize, bytes: &[u8]) {
        let at = self.unit.eib + offset;
        self.unit.mem[at..at + bytes.len()].copy_from_slice(bytes);
    }

    pub(super) fn eib_halfword(&mut self, offset: usize, value: i16) {
        self.eib_bytes(offset, &value.to_be_bytes());
    }

    pub(super) fn eib_fullword(&mut self, offset: usize, value: i32) {
        self.eib_bytes(offset, &value.to_be_bytes());
    }

    /// A 4-byte packed EIB field: seven digits and a sign.
    pub(super) fn eib_packed(&mut self, offset: usize, value: i64) {
        let mut field = [0u8; 4];
        let _ = decimal::encode(&mut field, Decimal { negative: value < 0, magnitude: value.unsigned_abs() as u128 });
        self.eib_bytes(offset, &field);
    }

    /// An EIB text field, in EBCDIC, padded with spaces or cut to `len`.
    pub(super) fn eib_text(&mut self, offset: usize, len: usize, text: &str) {
        let mut bytes: Vec<u8> = text.chars().map(|c| self.page.encode_char(c).unwrap_or(ebcdic::SPACE)).collect();
        bytes.resize(len, ebcdic::SPACE);
        self.eib_bytes(offset, &bytes);
    }

    fn eib_calen(&self) -> i16 {
        let at = self.unit.eib + EIBCALEN;
        i16::from_be_bytes([self.unit.mem[at], self.unit.mem[at + 1]])
    }

    /// Gives a record to INTO or SET and its length to LENGTH. A record longer than LENGTH (or the
    /// INTO item) arrives cut short, with LENGERR.
    pub(super) fn deliver_record(&mut self, block: &ExecBlock, record: &[u8]) -> R<Flow> {
        let limit = match self.arg_int(block, "LENGTH")? {
            Some(n) => n.max(0) as usize,
            None => match operand(block, "INTO") {
                Some(Operand::Ref(r)) => self.locate(r)?.len,
                _ => record.len(),
            },
        };
        if has(block, "SET") {
            let at = self.unit.push_temporary(record);
            self.store_pointer(block, "SET", Some(at))?;
        } else {
            self.store_bytes(block, "INTO", &record[..record.len().min(limit)])?;
        }
        self.store_int(block, "LENGTH", record.len() as i64)?;
        if record.len() > limit && !has(block, "SET") {
            return self.raise(block, "LENGERR", 0);
        }
        self.cics_ok(block)
    }

    /// A command that succeeded: RESP and RESP2, when asked for, are zero.
    pub(super) fn cics_ok(&mut self, block: &ExecBlock) -> R<Flow> {
        self.store_int(block, "RESP", 0)?;
        self.store_int(block, "RESP2", 0)?;
        Ok(Flow::Next)
    }

    /// Raises a condition. RESP or NOHANDLE take it; otherwise HANDLE CONDITION (the condition's
    /// own entry, else ERROR) or IGNORE CONDITION decides; otherwise HANDLE ABEND, if active, or
    /// the task abends with the condition's AEIx code.
    pub(super) fn raise(&mut self, block: &ExecBlock, name: &str, resp2: i32) -> R<Flow> {
        let resp = syntax::system::resp_code(name).unwrap_or(1);
        self.eib_fullword(EIBRESP, resp);
        self.eib_fullword(EIBRESP2, resp2);
        if has(block, "RESP") {
            self.store_int(block, "RESP", i64::from(resp))?;
            self.store_int(block, "RESP2", i64::from(resp2))?;
            return Ok(Flow::Next);
        }
        if has(block, "NOHANDLE") {
            return Ok(Flow::Next);
        }
        let handlers = &self.cics_handlers.conditions;
        match handlers.get(name).or_else(|| handlers.get("ERROR")).copied() {
            Some(Handler::Ignore) => Ok(Flow::Next),
            Some(Handler::Label(p)) => Ok(Flow::GoTo(p)),
            None => match self.cics_handlers.abend.take() {
                Some(p) => Ok(Flow::GoTo(p)),
                None => Err(Abend {
                    code: default_abend(name).into(),
                    message: format!("EXEC CICS {}: {name} was raised with no RESP, HANDLE CONDITION or IGNORE CONDITION", block.command),
                    pos: block.pos,
                }),
            },
        }
    }

    fn label(&self, block: &ExecBlock, text: &str) -> R<usize> {
        let p = ProcName { name: text.trim().to_ascii_uppercase(), section: None };
        crate::procedure(self.program, &p).map(|(start, _)| start).map_err(|m| Abend::ironwork(format!("EXEC CICS {}: {m}", block.command), block.pos))
    }

    fn handle_condition(&mut self, block: &ExecBlock) -> R<Flow> {
        for (name, arg) in block.options.iter().filter(|(n, _)| !NOT_CONDITIONS.contains(&n.as_str())) {
            match arg {
                Some(ExecArg::Text(t)) => {
                    let p = self.label(block, t)?;
                    self.cics_handlers.conditions.insert(name.clone(), Handler::Label(p));
                }
                _ => {
                    self.cics_handlers.conditions.remove(name);
                }
            }
        }
        self.cics_ok(block)
    }

    fn handle_abend(&mut self, block: &ExecBlock) -> R<Flow> {
        if has(block, "PROGRAM") {
            return Err(Abend::ironwork("EXEC CICS HANDLE ABEND PROGRAM is not supported yet; use LABEL", block.pos));
        }
        if let Some(Some(ExecArg::Text(t))) = option(block, "LABEL") {
            let p = self.label(block, t)?;
            self.cics_handlers.abend = Some(p);
        } else if has(block, "CANCEL") || has(block, "RESET") {
            self.cics_handlers.abend = None;
        }
        self.cics_ok(block)
    }

    /// COMMAREA's bytes, cut to LENGTH when LENGTH is shorter.
    fn commarea(&mut self, block: &ExecBlock) -> R<Option<Vec<u8>>> {
        self.arg_bytes_cut(block, "COMMAREA", "LENGTH")
    }

    /// RETURN ends this program. At the task's top level TRANSID and COMMAREA name the next task
    /// and what it starts with; in a LINKed program they raise INVREQ.
    fn cics_return(&mut self, block: &ExecBlock) -> R<Flow> {
        self.eib_bytes(EIBFN, &[0x0E, 0x08]);
        let transid = self.arg_text(block, "TRANSID")?;
        let commarea = self.commarea(block)?;
        if !self.main && (transid.is_some() || commarea.is_some()) {
            return self.raise(block, "INVREQ", 0);
        }
        if let Some(task) = self.unit.cics.as_mut() {
            if let Some(t) = transid {
                task.next_transid = Some(t.to_ascii_uppercase());
            }
            if commarea.is_some() {
                task.returned_commarea = commarea;
            }
        }
        Ok(Flow::End(Ending::Goback))
    }

    /// LINK runs a program and comes back; XCTL runs it in this program's place. A LINKed program
    /// gets the COMMAREA item itself; XCTL passes a copy, since this program's storage goes away.
    /// Each LINK or XCTL starts the program with fresh WORKING-STORAGE, as CICS gives it.
    fn cics_link(&mut self, block: &ExecBlock, xctl: bool) -> R<Flow> {
        self.eib_bytes(EIBFN, if xctl { &[0x0E, 0x04] } else { &[0x0E, 0x02] });
        let Some(name) = self.arg_text(block, "PROGRAM")?.map(|n| n.to_ascii_uppercase()) else {
            return Err(Abend::ironwork(format!("EXEC CICS {} needs PROGRAM", block.command), block.pos));
        };
        self.eib_text(EIBRSRCE, 8, &name);
        let index = match self.unit.load(&name) {
            Ok(i) => i,
            Err(LoadError::NotFound) => return self.raise(block, "PGMIDERR", 0),
            Err(LoadError::Compile(m)) => return Err(Abend::ironwork(format!("EXEC CICS {} PROGRAM({name}): {m}", block.command), block.pos)),
        };
        let Some(compiled) = self.unit.programs[index].compiled.clone() else {
            return Err(Abend::ironwork(format!("EXEC CICS {} PROGRAM({name}): the task's first program is already running", block.command), block.pos));
        };
        let mark = self.unit.mem.len();
        let (area, item_len) = match operand(block, "COMMAREA") {
            None => (None, 0),
            Some(Operand::Ref(r)) if !xctl => {
                let loc = self.locate(r)?;
                (Some(loc.offset), loc.len)
            }
            Some(_) => {
                let bytes = self.commarea(block)?.unwrap_or_default();
                (Some(self.unit.push_temporary(&bytes)), bytes.len())
            }
        };
        let length = self.arg_int(block, "LENGTH")?.map_or(item_len, |n| n.max(0) as usize);
        let saved = self.eib_calen();
        self.eib_halfword(EIBCALEN, length as i16);
        self.unit.programs[index].initialized = false;
        self.nest(block.pos)?;
        let ending = Machine::activation(&compiled, index, &mut *self.unit, self.main && xctl).and_then(|mut callee| {
            let eib = callee.unit.eib;
            callee.bind(&[Some(eib), area]);
            callee.run_procedure()
        });
        self.unit.depth -= 1;
        self.unit.programs[index].active = false;
        self.unit.release_temporaries(mark);
        self.eib_halfword(EIBCALEN, saved);
        match ending? {
            Ending::StopRun => Ok(Flow::End(Ending::StopRun)),
            _ if xctl => Ok(Flow::End(Ending::Goback)),
            _ => self.cics_ok(block),
        }
    }

    /// ABEND ends the task with ABCODE, unless HANDLE ABEND LABEL is active and CANCEL is absent.
    fn cics_abend(&mut self, block: &ExecBlock) -> R<Flow> {
        let code = self.arg_text(block, "ABCODE")?.unwrap_or_else(|| "????".into());
        if !has(block, "CANCEL")
            && let Some(p) = self.cics_handlers.abend.take()
        {
            return Ok(Flow::GoTo(p));
        }
        Err(Abend { code: code.clone(), message: format!("EXEC CICS ABEND ABCODE({code})"), pos: block.pos })
    }

    /// Fills the EXEC interface block for the task's first program and binds DFHEIBLK and
    /// DFHCOMMAREA, the USING items the translator gave it.
    pub(crate) fn begin_task(&mut self, commarea: Option<usize>, length: usize) {
        let (seconds, hundredths) = self.unit.now();
        let abstime = cics::abstime(seconds, hundredths);
        let (transid, termid, number) = self.unit.cics.as_ref().map(|t| (t.transid.clone(), t.termid.clone(), t.number)).unwrap_or_default();
        self.eib_packed(EIBTIME, cics::eib_time(abstime));
        self.eib_packed(EIBDATE, cics::eib_date(abstime));
        self.eib_text(EIBTRNID, 4, &transid);
        self.eib_packed(EIBTASKN, i64::from(number));
        self.eib_text(EIBTRMID, 4, &termid);
        self.eib_halfword(EIBCALEN, length as i16);
        if let Some(aid) = self.unit.cics.as_ref().and_then(|t| t.initial_aid) {
            self.eib_bytes(EIBAID, &[aid]);
        }
        let eib = self.unit.eib;
        self.bind(&[Some(eib), commarea]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eib_offsets_match_the_dfheiblk_layout() {
        let source = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. EIBT.\n       PROCEDURE DIVISION.\n           EXEC CICS RETURN END-EXEC.\n";
        let program = syntax::parse_all_with(source, &syntax::copy::Libraries::default()).unwrap().remove(0);
        let compiled = crate::compile(program, &[]).unwrap_or_else(|e| panic!("{e:?}"));
        let offset = |name: &str| compiled.layout.items.iter().find(|i| i.name.as_deref() == Some(name)).map(|i| i.offset as usize);
        let fields = [
            ("EIBTIME", EIBTIME),
            ("EIBDATE", EIBDATE),
            ("EIBTRNID", EIBTRNID),
            ("EIBTASKN", EIBTASKN),
            ("EIBTRMID", EIBTRMID),
            ("EIBCPOSN", EIBCPOSN),
            ("EIBCALEN", EIBCALEN),
            ("EIBAID", EIBAID),
            ("EIBFN", EIBFN),
            ("EIBRSRCE", EIBRSRCE),
            ("EIBRESP", EIBRESP),
            ("EIBRESP2", EIBRESP2),
        ];
        for (name, at) in fields {
            assert_eq!(offset(name), Some(at), "{name}");
        }
    }
}
