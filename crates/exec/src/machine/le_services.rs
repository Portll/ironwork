//! A CALL that reaches a Language Environment callable service. The service gets its arguments as
//! addresses and reads or writes each at its documented length, whatever the argument's own; its
//! feedback code goes to fc, or, with fc OMITTED, a failure ends the run as an unhandled condition.

use super::*;
use crate::le::{self, Condition, Stamp};
use std::io::Write;

/// The largest CEEGTST request the run grants, as GETMAIN's limit is for CICS.
const HEAP_LIMIT: usize = 1 << 28;
/// MSGFILE's default ddname, where CEEMOUT writes.
const MSGFILE: &str = "SYSOUT";
/// The transient data queue that takes a CICS task's LE messages and dumps in place of any DD.
const CESE: &str = "CESE";

type Outcome = R<Option<Condition>>;

impl<'p> Machine<'p, '_, '_> {
    pub(super) fn le_call(&mut self, c: &'p Call, name: &str) -> R<Flow> {
        let mark = self.unit.mem.len();
        let outcome = self.le_arguments(c).and_then(|args| self.le_service(name, &args, c.pos));
        self.unit.release_temporaries(mark);
        outcome?;
        match &c.not_on_exception {
            Some(body) => self.run_block(body),
            None => Ok(Flow::Next),
        }
    }

    /// Each argument's address in run-unit memory, as a CALL to a program passes it.
    fn le_arguments(&mut self, c: &Call) -> R<Vec<Option<usize>>> {
        let mut addresses = Vec::new();
        for arg in &c.using {
            let at = match (arg.mode, &arg.value) {
                (_, None) => None,
                (ArgMode::Reference, Some(Operand::Ref(r))) => Some(self.locate(r)?.offset),
                (ArgMode::Value, Some(op)) => {
                    let bytes = self.value_argument(op, c.pos)?;
                    Some(self.unit.push_temporary(&bytes))
                }
                (_, Some(op)) => {
                    let bytes = self.content_argument(op, c.pos)?;
                    Some(self.unit.push_temporary(&bytes))
                }
            };
            addresses.push(at);
        }
        Ok(addresses)
    }

    fn le_service(&mut self, name: &str, args: &[Option<usize>], pos: Pos) -> R<()> {
        let params = le::parameters(name).unwrap_or_default();
        let required = params.iter().filter(|p| **p != "fc").count();
        if args.len() < params.len() {
            let passed = match args.len() {
                1 => "1 argument".to_owned(),
                n => format!("{n} arguments"),
            };
            return Err(Abend::ironwork(format!("CALL {name} passes {passed}; {name} takes {}, and with fewer z/OS is unpredictable", params.join(", ")), pos));
        }
        let call = LeCall { name, args, pos };
        let failed = match name {
            "CEE3ABD" => return Err(self.cee3abd(&call)),
            "CEE3DMP" => self.cee3dmp(&call)?,
            "CEEDATE" => self.ceedate(&call)?,
            "CEEDATM" => self.ceedatm(&call)?,
            "CEEDAYS" => self.ceedays_or_secs(&call, le::Reading::Days)?,
            "CEEDYWK" => self.ceedywk(&call)?,
            "CEEFRST" => self.ceefrst(&call)?,
            "CEEGMT" | "CEEUTC" => self.ceegmt(&call)?,
            "CEEGMTO" => self.ceegmto(&call)?,
            "CEEGTST" => self.ceegtst(&call)?,
            "CEELOCT" => self.ceeloct(&call)?,
            "CEEMOUT" => self.ceemout(&call)?,
            "CEESECS" => self.ceedays_or_secs(&call, le::Reading::Seconds)?,
            _ => return Err(Abend::ironwork(format!("CALL {name}: not a service ironwork provides"), pos)),
        };
        match (args.get(required).copied().flatten(), failed) {
            (Some(fc), failed) => {
                self.le_store(fc, &failed.map_or([0; 12], Condition::token));
                Ok(())
            }
            (None, Some(c)) if c.severity >= 2 => Err(Abend {
                code: AbendCode::user(4038),
                message: format!("CALL {name}: {} {} With its feedback code omitted the condition was signaled, and nothing handled it", c.symbol(), c.text()),
                pos,
            }),
            (None, _) => Ok(()),
        }
    }

    /// The address of parameter `i`; an OMITTED one is a null address the service stores through.
    fn le_at(&self, call: &LeCall, i: usize) -> R<usize> {
        call.args.get(i).copied().flatten().ok_or_else(|| {
            let param = le::parameters(call.name).and_then(|p| p.get(i)).copied().unwrap_or("argument");
            Abend { code: AbendCode::Protection, message: format!("CALL {}: {param} is OMITTED, and the service addresses it", call.name), pos: call.pos }
        })
    }

    fn le_load(&self, at: usize, len: usize, pos: Pos) -> R<Vec<u8>> {
        let bytes = at.checked_add(len).and_then(|end| self.unit.mem.get(at..end));
        bytes.map(<[u8]>::to_vec).ok_or_else(|| Abend { code: AbendCode::Protection, message: "a callable service's argument reaches outside the run unit's storage".into(), pos })
    }

    /// Stores at an argument's address; what would fall past the run unit's storage is not kept.
    fn le_store(&mut self, at: usize, bytes: &[u8]) {
        let end = (at + bytes.len()).min(self.unit.mem.len());
        if at < end {
            self.unit.mem[at..end].copy_from_slice(&bytes[..end - at]);
        }
    }

    fn le_fullword(&self, call: &LeCall, i: usize) -> R<i32> {
        let at = self.le_at(call, i)?;
        let b = self.le_load(at, 4, call.pos)?;
        Ok(i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// A halfword length-prefixed string; a negative length is a null string.
    fn le_vstring(&self, call: &LeCall, i: usize) -> R<Vec<u8>> {
        let at = self.le_at(call, i)?;
        let len = self.le_load(at, 2, call.pos)?;
        let len = i16::from_be_bytes([len[0], len[1]]).max(0) as usize;
        self.le_load(at + 2, len, call.pos)
    }

    fn le_output(&mut self, call: &LeCall, i: usize, bytes: &[u8]) -> R<()> {
        let at = self.le_at(call, i)?;
        self.le_store(at, bytes);
        Ok(())
    }

    fn le_now(&self) -> Stamp {
        let (seconds, hundredths) = self.unit.now();
        Stamp::from_unix(seconds, hundredths)
    }

    fn cee3abd(&mut self, call: &LeCall) -> Abend {
        let code = match self.le_fullword(call, 0) {
            Ok(c) => c,
            Err(a) => return a,
        };
        let clean_up = self.le_fullword(call, 1).ok().filter(|t| (0..=5).contains(t));
        let how = match clean_up {
            Some(1) => "with normal enclave termination",
            Some(2..=5) => "with enclave termination and its dumps as clean-up asks",
            _ => "without clean-up",
        };
        let user = ((code as u32) & 0xFFF) as u16;
        if self.unit.cics.is_some() {
            return Abend { code: AbendCode::Cics(format!("{user:04}")), message: format!("CALL CEE3ABD: transaction abend {user:04}"), pos: call.pos };
        }
        Abend { code: AbendCode::user(user), message: format!("CALL CEE3ABD: user abend {user} {how}"), pos: call.pos }
    }

    fn ceedays_or_secs(&mut self, call: &LeCall, reading: le::Reading) -> Outcome {
        let input = self.le_vstring(call, 0)?;
        let picture = self.le_vstring(call, 1)?;
        let window = self.le_now().fields().year - 80;
        let result = le::read(&input, &picture, reading, window, self.page);
        let failed = result.err();
        let bytes = match (reading, result) {
            (le::Reading::Days, r) => (r.map_or(0, |s| s.lilian) as i32).to_be_bytes().to_vec(),
            (le::Reading::Seconds, r) => r.map_or([0; 8], |s| le::seconds_hfp(s.total_millis())).to_vec(),
        };
        self.le_output(call, 2, &bytes)?;
        Ok(failed)
    }

    fn ceedate(&mut self, call: &LeCall) -> Outcome {
        let lilian = self.le_fullword(call, 0)?;
        let picture = self.le_vstring(call, 1)?;
        let (text, failed) = le::date(i64::from(lilian), &picture, self.page);
        self.le_output(call, 2, &text)?;
        Ok(failed)
    }

    fn ceedatm(&mut self, call: &LeCall) -> Outcome {
        let at = self.le_at(call, 0)?;
        let seconds = self.le_load(at, 8, call.pos)?;
        let picture = self.le_vstring(call, 1)?;
        let (text, failed) = le::timestamp(seconds.try_into().unwrap_or([0; 8]), &picture, self.page);
        self.le_output(call, 2, &text)?;
        Ok(failed)
    }

    fn ceedywk(&mut self, call: &LeCall) -> Outcome {
        let lilian = i64::from(self.le_fullword(call, 0)?);
        let (day, failed) = if (1..=crate::calendar::LAST_LILIAN).contains(&lilian) { (crate::calendar::weekday(lilian) as i32, None) } else { (0, Some(le::LILIAN_RANGE)) };
        self.le_output(call, 1, &day.to_be_bytes())?;
        Ok(failed)
    }

    fn ceegmt(&mut self, call: &LeCall) -> Outcome {
        let now = self.le_now();
        self.le_output(call, 0, &(now.lilian as i32).to_be_bytes())?;
        self.le_output(call, 1, &le::seconds_hfp(now.total_millis()))?;
        Ok(None)
    }

    fn ceeloct(&mut self, call: &LeCall) -> Outcome {
        let now = self.le_now();
        self.le_output(call, 0, &(now.lilian as i32).to_be_bytes())?;
        self.le_output(call, 1, &le::seconds_hfp(now.total_millis()))?;
        let text = self.page.encode(&now.gregorian()).unwrap_or_default();
        self.le_output(call, 2, &text)?;
        Ok(None)
    }

    fn ceegmto(&mut self, call: &LeCall) -> Outcome {
        self.le_output(call, 0, &[0; 4])?;
        self.le_output(call, 1, &[0; 4])?;
        self.le_output(call, 2, &[0; 8])?;
        Ok(None)
    }

    fn ceemout(&mut self, call: &LeCall) -> Outcome {
        let message = self.le_vstring(call, 0)?;
        if self.le_fullword(call, 1)? != 2 {
            return Ok(Some(le::DESTINATION));
        }
        let text = self.page.decode(&message);
        self.le_write(MSGFILE, &[text], call.pos)?;
        Ok(None)
    }

    fn cee3dmp(&mut self, call: &LeCall) -> Outcome {
        let at = self.le_at(call, 0)?;
        let title = self.page.decode(&self.le_load(at, 80, call.pos)?);
        let at = self.le_at(call, 1)?;
        let options = self.page.decode(&self.le_load(at, 255, call.pos)?);
        let (dd, invalid) = le::dump_options(&options);
        let now = self.le_now().fields();
        let unit_name = |p: &crate::unit::Loaded| match p.compiled.as_ref().and_then(|c| c.program.oo.as_deref()).and_then(|o| o.method()) {
            Some(m) => format!("{}.{}", m.class, m.name),
            None => p.name.clone(),
        };
        let active: Vec<String> = self.unit.programs.iter().filter(|p| p.active).map(|p| format!("  {}", unit_name(p))).collect();
        let mut lines = vec![
            format!("CEE3DMP: {:<60}  {:04}-{:02}-{:02} {:02}:{:02}:{:02}", title.chars().take(60).collect::<String>().trim_end(), now.year, now.month, now.day, now.hour, now.minute, now.second),
            format!("Options: {}", options.trim_end()),
            "Active programs, in the order the run unit loaded them:".to_owned(),
        ];
        lines.extend(active);
        self.le_write(&dd, &lines, call.pos)?;
        Ok(invalid.then_some(le::DUMP_OPTIONS))
    }

    fn ceegtst(&mut self, call: &LeCall) -> Outcome {
        let heap = self.le_fullword(call, 0)?;
        let size = self.le_fullword(call, 1)?;
        let address = self.le_at(call, 2)?;
        let failed = match (heap, size) {
            (0, s) if s > 0 && s as usize <= HEAP_LIMIT => None,
            (0, s) if s > 0 => Some(le::HEAP_SHORT),
            (0, _) => Some(le::HEAP_SIZE),
            _ => Some(le::HEAP_ID),
        };
        if failed.is_none() {
            let at = self.unit.push_temporary(&vec![0; size as usize]);
            self.unit.le.heap.push((at, size as usize, false));
            self.le_store(address, &(ADDRESS_BASE + at as u32).to_be_bytes());
        }
        Ok(failed)
    }

    fn ceefrst(&mut self, call: &LeCall) -> Outcome {
        let at = self.le_at(call, 0)?;
        let b = self.le_load(at, 4, call.pos)?;
        let address = u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        let offset = address.checked_sub(ADDRESS_BASE).map(|o| o as usize);
        match self.unit.le.heap.iter_mut().find(|(start, _, freed)| Some(*start) == offset && !*freed) {
            Some(block) => {
                block.2 = true;
                Ok(None)
            }
            None => Ok(Some(le::FREE_ADDRESS)),
        }
    }

    /// Writes lines to a DD as UTF-8 text, the run's first write replacing the file, or to standard
    /// error when the DD is not given; in a CICS task, each line is an item on TD queue CESE.
    fn le_write(&mut self, dd: &str, lines: &[String], pos: Pos) -> R<()> {
        if let Some(task) = self.unit.cics.as_mut() {
            for line in lines {
                task.writeq_td(CESE, &self.page.encode(line).unwrap_or_default());
            }
            return Ok(());
        }
        let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
        let Some(file) = self.unit.dds.get(dd) else {
            return self.unit.err.write_all(text.as_bytes()).map_err(|e| Abend::ironwork(format!("writing to standard error: {e}"), pos));
        };
        let first = !self.unit.le.written.iter().any(|w| w == dd);
        let opened = std::fs::OpenOptions::new().create(true).write(true).append(!first).truncate(first).open(&file.path);
        opened
            .and_then(|mut f| f.write_all(text.as_bytes()))
            .map_err(|e| Abend::ironwork(format!("DD {dd} {}: {e}", file.path.display()), pos))?;
        if first {
            self.unit.le.written.push(dd.to_owned());
        }
        Ok(())
    }
}

struct LeCall<'a> {
    name: &'a str,
    args: &'a [Option<usize>],
    pos: Pos,
}
