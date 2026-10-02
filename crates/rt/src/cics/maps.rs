//! Terminal control through BMS: SEND MAP, RECEIVE MAP, SEND CONTROL and RECEIVE, over the task's
//! terminal. A map's screen comes from its BMS source; the program's symbolic map is read and
//! written at the offsets `bms::slots` gives.

use super::Condition;
use super::command::{Control, Datum, Record};
use super::run::{At, CicsHost, EIBAID, EIBCPOSN, Flow, R};
use super::run::{bytes, deliver, eib_bytes, eib_halfword, encoded, int, ok, page, raise, store_bytes, store_pointer, text};
use crate::abend::Abend;
use crate::bms::{self, Initial, Intensity, Map, Protection};
use crate::storage::Val;
use crate::store;
use crate::terminal::{self, attribute_byte};

/// MAP and MAPSET.
pub(super) struct MapNames<'c, P, O, S> {
    pub map: Option<&'c Datum<P, O, S>>,
    pub mapset: Option<&'c Datum<P, O, S>>,
}

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

/// The map MAP names in MAPSET (or the mapset of the map's name), read once per task from the copy
/// libraries.
fn bms_map<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, names: &MapNames<P, O, S>) -> R<Result<Map, Condition>> {
    let Some(map_name) = text(x, names.map, at.pos)?.map(|m| m.to_ascii_uppercase()) else {
        return Err(Abend::ironwork(format!("EXEC CICS {} needs MAP", at.name), at.pos));
    };
    let set_name = text(x, names.mapset, at.pos)?.map_or_else(|| map_name.clone(), |m| m.to_ascii_uppercase());
    let cached = x.unit().cics.as_ref().and_then(|t| t.mapsets.get(&set_name)).cloned();
    let mapset = match cached {
        Some(m) => m,
        None => match x.mapset(&set_name) {
            None => return Ok(Err(Condition::PGMIDERR)),
            Some(Err(m)) => return Err(Abend::ironwork(format!("EXEC CICS {} MAPSET({set_name}): {m}", at.name), at.pos)),
            Some(Ok(m)) => {
                if let Some(task) = x.unit().cics.as_mut() {
                    task.mapsets.insert(set_name.clone(), m.clone());
                }
                m
            }
        },
    };
    Ok(mapset.maps.into_iter().find(|m| m.name == map_name).ok_or(Condition::INVREQ))
}

/// The bytes of the area an option names, else of the data item named `default`, else nothing.
fn area_bytes<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, option: Option<&Datum<P, O, S>>, default: &str) -> R<Option<Vec<u8>>> {
    if let Some(bytes) = bytes(x, option, at.pos)? {
        return Ok(Some(bytes));
    }
    Ok(x.item_named(default, at.pos)?.map(|loc| store::bytes(x.mem(), loc).to_vec()))
}

fn with_terminal<'w, P: Copy, O, S, T>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, op: impl FnOnce(&mut dyn terminal::Terminal) -> Result<T, String>) -> R<T> {
    let Some(term) = x.unit().cics.as_mut().and_then(|t| t.terminal.as_mut()) else {
        return Err(Abend::ironwork(format!("EXEC CICS {} needs a terminal: run with --screens, or through the TN3270 server", at.name), at.pos));
    };
    op(term.as_mut()).map_err(|m| Abend::ironwork(format!("EXEC CICS {}: {m}", at.name), at.pos))
}

/// The WCC SEND's options and the map's CTRL ask for.
fn wcc(control: Control, ctrl: &[String]) -> u8 {
    let on = |given: bool, name: &str| given || ctrl.iter().any(|c| c == name);
    let mut wcc = 0;
    if on(control.freekb, "FREEKB") {
        wcc |= terminal::WCC_RESTORE;
    }
    if on(control.alarm, "ALARM") {
        wcc |= terminal::WCC_ALARM;
    }
    if on(control.frset, "FRSET") {
        wcc |= terminal::WCC_RESET_MDT;
    }
    wcc
}

/// SEND MAP: each field's attribute (ATTRB, or the symbolic map's A byte when it is not null) and
/// data (the symbolic map's, when its first byte is not null, else INITIAL). MAPONLY sends the map
/// alone; DATAONLY sends only the program's data. CURSOR(n) places the cursor; bare CURSOR puts it
/// on the first field whose L is -1; otherwise an ATTRB=IC field has it.
pub(super) fn send_map<'w, P: Copy, O, S>(
    x: &mut impl CicsHost<'w, P, O, S>,
    at: &At<P, O, S>,
    names: MapNames<P, O, S>,
    from: Option<&Datum<P, O, S>>,
    (maponly, dataonly): (bool, bool),
    cursor_option: Option<&Datum<P, O, S>>,
    control: Control,
) -> R<Flow> {
    let map = match bms_map(x, at, &names)? {
        Ok(m) => m,
        Err(c) => return raise(x, at, c, 0),
    };
    let (rows, columns) = with_terminal(x, at, |t| Ok(t.size()))?;
    let area = if maponly { None } else { area_bytes(x, at, from, &format!("{}O", map.name))? };
    let page = page(x);
    let slots = bms::slots(&map, false);
    let command = if control.erase { terminal::ERASE_WRITE } else { terminal::WRITE };
    let mut stream = vec![command, wcc(control, &map.ctrl)];
    let (mut cursor, mut symbolic_cursor) = (None, None);
    for (i, field) in map.fields.iter().enumerate() {
        let copies = if field.group.is_none() { field.occurs.max(1) } else { 1 };
        for occurrence in 0..copies {
            let row = usize::from(map.line) + usize::from(field.line) - 1;
            let column = usize::from(map.column) + usize::from(field.column) - 1 + usize::from(occurrence) * (usize::from(field.length) + 1);
            if row > rows || column > columns {
                return raise(x, at, Condition::INVMPSZ, 0);
            }
            let address = (row - 1) * columns + column - 1;
            let slot = slots.iter().find(|s| s.field == i && s.occurrence == occurrence);
            let symbolic = |offset: usize, len: usize| area.as_ref().and_then(|a| a.get(offset..offset + len)).filter(|b| b.first().is_some_and(|&x| x != 0));
            let attribute = slot.and_then(|s| s.attribute_at()).and_then(|o| symbolic(o, 1)).map(|b| b[0]);
            let data = slot.and_then(|s| symbolic(s.data, s.size.min(usize::from(field.length))).map(<[u8]>::to_vec));
            if slot.and_then(|s| s.length_at()).and_then(|o| area.as_ref().and_then(|a| a.get(o..o + 2))) == Some(&[0xFF, 0xFF][..]) {
                symbolic_cursor.get_or_insert(address + 1);
            }
            let initial = match &field.initial {
                Some(Initial::Text(t)) => Some(encoded(page, t)),
                Some(Initial::Bytes(b)) => Some(b.clone()),
                None => None,
            };
            if dataonly && attribute.is_none() && data.is_none() {
                continue;
            }
            stream.push(terminal::SBA);
            stream.extend(terminal::encode_address(address));
            stream.extend([terminal::SF, attribute.unwrap_or_else(|| attribute_of(&field.attrb))]);
            if let Some(bytes) = data.or(if dataonly { None } else { initial }) {
                stream.extend(bytes.iter().take(usize::from(field.length)));
            }
            if field.attrb.cursor && cursor.is_none() {
                cursor = Some(address + 1);
            }
        }
    }
    if cursor_option.is_some() {
        cursor = match int(x, cursor_option, at.pos)? {
            Some(n) => Some(n.max(0) as usize),
            None => symbolic_cursor.or(cursor),
        };
    }
    if let Some(address) = cursor {
        stream.push(terminal::SBA);
        stream.extend(terminal::encode_address(address));
        stream.push(terminal::IC);
    }
    with_terminal(x, at, |t| t.send(&stream))?;
    ok(x, at)
}

pub(super) fn send_control<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, cursor: Option<&Datum<P, O, S>>, control: Control) -> R<Flow> {
    let command = if control.erase { terminal::ERASE_WRITE } else { terminal::WRITE };
    let mut stream = vec![command, wcc(control, &[])];
    if let Some(n) = int(x, cursor, at.pos)? {
        stream.push(terminal::SBA);
        stream.extend(terminal::encode_address(n.max(0) as usize));
        stream.push(terminal::IC);
    }
    with_terminal(x, at, |t| t.send(&stream))?;
    ok(x, at)
}

/// The operator's next AID key: EIBAID and EIBCPOSN are set from it. Data that does not start with
/// an SBA order, as an unformatted screen sends, gives no fields.
fn next_inbound<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>) -> R<terminal::Inbound> {
    let Some(stream) = with_terminal(x, at, |t| t.receive())? else {
        return Err(Abend::ironwork(format!("EXEC CICS {}: the terminal has no more input", at.name), at.pos));
    };
    let formatted = stream.get(3).is_none_or(|&b| b == terminal::SBA);
    let stream = if formatted { &stream[..] } else { &stream[..3] };
    let read = terminal::parse_inbound(stream).map_err(|m| Abend::ironwork(format!("EXEC CICS {}: {m}", at.name), at.pos))?;
    eib_bytes(x.unit(), EIBAID, &[read.aid]);
    eib_halfword(x.unit(), EIBCPOSN, read.cursor.unwrap_or(0) as i16);
    Ok(read)
}

/// RECEIVE MAP: the input map starts as nulls; each field the operator modified gets its length in
/// L and its data in I, justified and filled as JUSTIFY says, and a field erased to empty gets
/// F = X'80'. MAPFAIL when no field was modified (CLEAR and the PA keys included) and when the
/// input holds no SBA order (CICS TS 6.x, RECEIVE MAP, Conditions).
pub(super) fn receive_map<'w, P: Copy, O, S>(
    x: &mut impl CicsHost<'w, P, O, S>,
    at: &At<P, O, S>,
    names: MapNames<P, O, S>,
    into: Option<&Datum<P, O, S>>,
    set: Option<&Datum<P, O, S>>,
) -> R<Flow> {
    let map = match bms_map(x, at, &names)? {
        Ok(m) => m,
        Err(c) => return raise(x, at, c, 0),
    };
    let (_, columns) = with_terminal(x, at, |t| Ok(t.size()))?;
    let read = next_inbound(x, at)?;
    if read.fields.is_empty() {
        return raise(x, at, Condition::MAPFAIL, 0);
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
        let fill = if field.fill_zero { 0xF0 } else { zarch::ebcdic::SPACE };
        let target = &mut area[slot.data..slot.data + slot.size];
        target.fill(fill);
        let offset = if field.justify_right { slot.size - data.len() } else { 0 };
        target[offset..offset + data.len()].copy_from_slice(data);
    }
    if set.is_some() {
        let address = x.unit().push_temporary(&area);
        store_pointer(x, set, Some(address), at.pos)?;
    } else if matches!(into, Some(Datum::Place(_) | Datum::Value(_))) {
        store_bytes(x, at, into, "INTO", &area)?;
    } else {
        let loc = x.locate_named(&format!("{}I", map.name), at.pos)?;
        x.assign(loc, Val::Bytes(area), None, at.pos)?;
    }
    ok(x, at)
}

/// RECEIVE without MAP: the inbound stream as it came (AID, cursor address, then each modified
/// field behind its SBA) into INTO or SET, its length in LENGTH.
pub(super) fn receive_raw<'w, P: Copy, O, S>(x: &mut impl CicsHost<'w, P, O, S>, at: &At<P, O, S>, record: &Record<P, O, S>) -> R<Flow> {
    let Some(stream) = with_terminal(x, at, |t| t.receive())? else {
        return Err(Abend::ironwork("EXEC CICS RECEIVE: the terminal has no more input", at.pos));
    };
    if let Some(&aid) = stream.first() {
        eib_bytes(x.unit(), EIBAID, &[aid]);
    }
    deliver(x, at, record, &stream)
}
