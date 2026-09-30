//! EXEC CICS terminal control through BMS: SEND MAP, RECEIVE MAP, SEND CONTROL and RECEIVE, over
//! the task's terminal. A map's screen comes from its BMS source; the program's symbolic map is
//! read and written at the offsets `bms::slots` gives.

use super::cics::{EIBAID, EIBCPOSN, has, operand};
use super::*;
use crate::cics::Condition;
use crate::terminal::{self, attribute_byte};
use syntax::bms::{self, Initial, Intensity, Map, Protection};

/// The attribute byte ATTRB describes.
fn attribute_of(attrb: &bms::Attrb) -> u8 {
    let mut bits = match attrb.protection {
        Protection::Askip => terminal::PROTECTED | terminal::NUMERIC,
        Protection::Prot => terminal::PROTECTED,
        Protection::Unprot => 0,
    };
    if attrb.numeric {
        bits |= terminal::NUMERIC;
    }
    bits |= match attrb.intensity {
        Intensity::Norm => 0,
        Intensity::Brt => terminal::INTENSIFIED,
        Intensity::Drk => terminal::NON_DISPLAY,
    };
    if attrb.detectable && attrb.intensity != Intensity::Drk {
        bits |= 0x04;
    }
    if attrb.fset {
        bits |= terminal::MDT;
    }
    attribute_byte(bits)
}

impl<'p> Machine<'p, '_, '_> {
    /// The map MAP names in MAPSET (or the mapset of the map's name), read once per task from the
    /// copy libraries.
    fn bms_map(&mut self, block: &ExecBlock) -> R<Result<Map, Condition>> {
        let Some(map_name) = self.arg_text(block, "MAP")?.map(|m| m.to_ascii_uppercase()) else {
            return Err(Abend::ironwork(format!("EXEC CICS {} needs MAP", block.command), block.pos));
        };
        let set_name = self.arg_text(block, "MAPSET")?.map_or_else(|| map_name.clone(), |m| m.to_ascii_uppercase());
        let cached = self.unit.cics.as_ref().and_then(|t| t.mapsets.get(&set_name)).cloned();
        let mapset = match cached {
            Some(m) => m,
            None => match bms::find_mapset(self.unit.copy_libraries(), &set_name) {
                None => return Ok(Err(Condition::PGMIDERR)),
                Some(Err(e)) => return Err(Abend::ironwork(format!("EXEC CICS {} MAPSET({set_name}): {}", block.command, e.message), block.pos)),
                Some(Ok(m)) => {
                    if let Some(task) = self.unit.cics.as_mut() {
                        task.mapsets.insert(set_name.clone(), m.clone());
                    }
                    m
                }
            },
        };
        Ok(mapset.maps.into_iter().find(|m| m.name == map_name).ok_or(Condition::INVREQ))
    }

    /// The bytes of the area an option names, else of the data item `default`, else nothing.
    fn area_bytes(&mut self, block: &ExecBlock, option: &str, default: &str) -> R<Option<Vec<u8>>> {
        if let Some(bytes) = self.arg_bytes(block, option)? {
            return Ok(Some(bytes));
        }
        let r = Ref { name: default.to_owned(), qualifiers: Vec::new(), subscripts: Vec::new(), refmod: None, pos: block.pos };
        match self.resolve(&r) {
            Ok(Resolved::Item(_)) => {
                let loc = self.locate(&r)?;
                Ok(Some(self.bytes(loc).to_vec()))
            }
            _ => Ok(None),
        }
    }

    fn with_terminal<T>(&mut self, block: &ExecBlock, op: impl FnOnce(&mut dyn crate::cics::Terminal) -> Result<T, String>) -> R<T> {
        let Some(term) = self.unit.cics.as_mut().and_then(|t| t.terminal.as_mut()) else {
            return Err(Abend::ironwork(format!("EXEC CICS {} needs a terminal: run with --screens, or through the TN3270 server", block.command), block.pos));
        };
        op(term.as_mut()).map_err(|m| Abend::ironwork(format!("EXEC CICS {}: {m}", block.command), block.pos))
    }

    /// The WCC SEND's options and the map's CTRL ask for.
    fn wcc(block: &ExecBlock, ctrl: &[String]) -> u8 {
        let on = |name: &str| has(block, name) || ctrl.iter().any(|c| c == name);
        let mut wcc = 0;
        if on("FREEKB") {
            wcc |= terminal::WCC_RESTORE;
        }
        if on("ALARM") {
            wcc |= terminal::WCC_ALARM;
        }
        if on("FRSET") {
            wcc |= terminal::WCC_RESET_MDT;
        }
        wcc
    }

    /// SEND MAP: each field's attribute (ATTRB, or the symbolic map's A byte when it is not null)
    /// and data (the symbolic map's, when its first byte is not null, else INITIAL). MAPONLY sends
    /// the map alone; DATAONLY sends only the program's data. CURSOR(n) places the cursor; bare
    /// CURSOR puts it on the first field whose L is -1; otherwise an ATTRB=IC field has it.
    pub(super) fn send_map(&mut self, block: &ExecBlock) -> R<Flow> {
        let map = match self.bms_map(block)? {
            Ok(m) => m,
            Err(c) => return self.raise(block, c, 0),
        };
        let (rows, columns) = self.with_terminal(block, |t| Ok(t.size()))?;
        let (maponly, dataonly) = (has(block, "MAPONLY"), has(block, "DATAONLY"));
        let area = if maponly { None } else { self.area_bytes(block, "FROM", &format!("{}O", map.name))? };
        let ctrl: Vec<String> = map.ctrl.clone();
        let slots = bms::slots(&map, false);
        let command = if has(block, "ERASE") { terminal::ERASE_WRITE } else { terminal::WRITE };
        let mut stream = vec![command, Self::wcc(block, &ctrl)];
        let (mut cursor, mut symbolic_cursor) = (None, None);
        for (i, field) in map.fields.iter().enumerate() {
            let copies = if field.group.is_none() { field.occurs.max(1) } else { 1 };
            for occurrence in 0..copies {
                let row = usize::from(map.line) + usize::from(field.line) - 1;
                let column = usize::from(map.column) + usize::from(field.column) - 1 + usize::from(occurrence) * (usize::from(field.length) + 1);
                if row > rows || column > columns {
                    return self.raise(block, Condition::INVMPSZ, 0);
                }
                let at = (row - 1) * columns + column - 1;
                let slot = slots.iter().find(|s| s.field == i && s.occurrence == occurrence);
                let symbolic = |offset: usize, len: usize| area.as_ref().and_then(|a| a.get(offset..offset + len)).filter(|b| b.first().is_some_and(|&x| x != 0));
                let attribute = slot.and_then(|s| s.attribute_at()).and_then(|o| symbolic(o, 1)).map(|b| b[0]);
                let data = slot.and_then(|s| symbolic(s.data, s.size.min(usize::from(field.length))).map(<[u8]>::to_vec));
                if slot.and_then(|s| s.length_at()).and_then(|o| area.as_ref().and_then(|a| a.get(o..o + 2))) == Some(&[0xFF, 0xFF][..]) {
                    symbolic_cursor.get_or_insert(at + 1);
                }
                let initial = match &field.initial {
                    Some(Initial::Text(t)) => Some(t.chars().map(|c| self.page.encode_char(c).unwrap_or(ebcdic::SPACE)).collect::<Vec<u8>>()),
                    Some(Initial::Bytes(b)) => Some(b.clone()),
                    None => None,
                };
                if dataonly && attribute.is_none() && data.is_none() {
                    continue;
                }
                stream.push(terminal::SBA);
                stream.extend(terminal::encode_address(at));
                stream.extend([terminal::SF, attribute.unwrap_or_else(|| attribute_of(&field.attrb))]);
                if let Some(bytes) = data.or(if dataonly { None } else { initial }) {
                    stream.extend(bytes.iter().take(usize::from(field.length)));
                }
                if field.attrb.cursor && cursor.is_none() {
                    cursor = Some(at + 1);
                }
            }
        }
        if has(block, "CURSOR") {
            cursor = match self.arg_int(block, "CURSOR")? {
                Some(n) => Some(n.max(0) as usize),
                None => symbolic_cursor.or(cursor),
            };
        }
        if let Some(at) = cursor {
            stream.push(terminal::SBA);
            stream.extend(terminal::encode_address(at));
            stream.push(terminal::IC);
        }
        self.with_terminal(block, |t| t.send(&stream))?;
        self.cics_ok(block)
    }

    pub(super) fn send_control(&mut self, block: &ExecBlock) -> R<Flow> {
        let command = if has(block, "ERASE") { terminal::ERASE_WRITE } else { terminal::WRITE };
        let mut stream = vec![command, Self::wcc(block, &[])];
        if let Some(n) = self.arg_int(block, "CURSOR")? {
            stream.push(terminal::SBA);
            stream.extend(terminal::encode_address(n.max(0) as usize));
            stream.push(terminal::IC);
        }
        self.with_terminal(block, |t| t.send(&stream))?;
        self.cics_ok(block)
    }

    /// The operator's next AID key: EIBAID and EIBCPOSN are set from it.
    fn next_inbound(&mut self, block: &ExecBlock) -> R<terminal::Inbound> {
        let Some(stream) = self.with_terminal(block, |t| t.receive())? else {
            return Err(Abend::ironwork(format!("EXEC CICS {}: the terminal has no more input", block.command), block.pos));
        };
        let read = terminal::parse_inbound(&stream).map_err(|m| Abend::ironwork(format!("EXEC CICS {}: {m}", block.command), block.pos))?;
        self.eib_bytes(EIBAID, &[read.aid]);
        self.eib_halfword(EIBCPOSN, read.cursor.unwrap_or(0) as i16);
        Ok(read)
    }

    /// RECEIVE MAP: the input map starts as nulls; each field the operator modified gets its length
    /// in L and its data in I, justified and filled as JUSTIFY says, and a field erased to empty
    /// gets F = X'80'. MAPFAIL when no field was modified (CLEAR and the PA keys included).
    pub(super) fn receive_map(&mut self, block: &ExecBlock) -> R<Flow> {
        let map = match self.bms_map(block)? {
            Ok(m) => m,
            Err(c) => return self.raise(block, c, 0),
        };
        let (_, columns) = self.with_terminal(block, |t| Ok(t.size()))?;
        let read = self.next_inbound(block)?;
        if read.fields.is_empty() {
            return self.raise(block, Condition::MAPFAIL, 0);
        }
        let slots = bms::slots(&map, true);
        let size = slots.last().map_or(0, |s| s.data + s.size);
        let mut area = vec![0u8; size];
        for slot in &slots {
            let field = &map.fields[slot.field];
            let row = usize::from(map.line) + usize::from(field.line) - 1;
            let column = usize::from(map.column) + usize::from(field.column) - 1 + usize::from(slot.occurrence) * (usize::from(field.length) + 1);
            let start = (row - 1) * columns + column;
            let Some((_, data)) = read.fields.iter().find(|(a, _)| *a == start) else { continue };
            let data = &data[..data.len().min(slot.size)];
            if let Some(l) = slot.length_at() {
                area[l..l + 2].copy_from_slice(&(data.len() as u16).to_be_bytes());
                if data.is_empty() {
                    area[l + 2] = 0x80;
                }
            }
            if data.is_empty() {
                continue;
            }
            let fill = if field.fill_zero { 0xF0 } else { ebcdic::SPACE };
            let target = &mut area[slot.data..slot.data + slot.size];
            target.fill(fill);
            let offset = if field.justify_right { slot.size - data.len() } else { 0 };
            target[offset..offset + data.len()].copy_from_slice(data);
        }
        if has(block, "SET") {
            let at = self.unit.push_temporary(&area);
            self.store_pointer(block, "SET", Some(at))?;
        } else if operand(block, "INTO").is_some() {
            self.store_bytes(block, "INTO", &area)?;
        } else {
            let r = Ref { name: format!("{}I", map.name), qualifiers: Vec::new(), subscripts: Vec::new(), refmod: None, pos: block.pos };
            let loc = self.locate(&r)?;
            self.assign(loc, Val::Bytes(area), None, block.pos)?;
        }
        self.cics_ok(block)
    }

    /// RECEIVE without MAP: the inbound stream as it came (AID, cursor address, then each modified
    /// field behind its SBA) into INTO or SET, its length in LENGTH.
    pub(super) fn receive_raw(&mut self, block: &ExecBlock) -> R<Flow> {
        let Some(stream) = self.with_terminal(block, |t| t.receive())? else {
            return Err(Abend::ironwork("EXEC CICS RECEIVE: the terminal has no more input", block.pos));
        };
        if let Some(&aid) = stream.first() {
            self.eib_bytes(EIBAID, &[aid]);
        }
        self.deliver_record(block, &stream)
    }
}
