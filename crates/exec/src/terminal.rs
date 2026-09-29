//! A 3270 display: its buffer of fields, the outbound data stream that writes it, the inbound Read
//! Modified stream that reports what the operator changed, and a text rendering. The byte values
//! are those of the 3270 Data Stream Programmer's Reference (GA23-0059).

use zarch::ebcdic::CodePage;

/// The task's terminal: SEND writes a 3270 data stream to it, and RECEIVE reads the stream the
/// operator's next AID key sends back.
pub trait Terminal: std::fmt::Debug {
    fn size(&self) -> (usize, usize);
    fn send(&mut self, stream: &[u8]) -> Result<(), String>;
    /// None when the operator has nothing more to send.
    fn receive(&mut self) -> Result<Option<Vec<u8>>, String>;
}

pub const WRITE: u8 = 0xF1;
pub const ERASE_WRITE: u8 = 0xF5;
pub const ERASE_WRITE_ALTERNATE: u8 = 0x7E;
pub const ERASE_ALL_UNPROTECTED: u8 = 0x6F;

pub const WCC_ALARM: u8 = 0x04;
pub const WCC_RESTORE: u8 = 0x02;
pub const WCC_RESET_MDT: u8 = 0x01;

pub const SF: u8 = 0x1D;
pub const SFE: u8 = 0x29;
pub const SBA: u8 = 0x11;
pub const SA: u8 = 0x28;
pub const MF: u8 = 0x2C;
pub const IC: u8 = 0x13;
pub const PT: u8 = 0x05;
pub const RA: u8 = 0x3C;
pub const EUA: u8 = 0x12;

pub const PROTECTED: u8 = 0x20;
pub const NUMERIC: u8 = 0x10;
pub const DISPLAY: u8 = 0x0C;
pub const INTENSIFIED: u8 = 0x08;
pub const NON_DISPLAY: u8 = 0x0C;
pub const MDT: u8 = 0x01;

pub const AID_ENTER: u8 = 0x7D;
pub const AID_CLEAR: u8 = 0x6D;
pub const AID_PA1: u8 = 0x6C;
pub const AID_PA2: u8 = 0x6E;
pub const AID_PA3: u8 = 0x6B;
pub const AID_NONE: u8 = 0x60;

/// The 6-bit code table: a 12-bit address's halves, and an attribute byte's graphic form.
const CODES: [u8; 64] = [
    0x40, 0xC1, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0x4A, 0x4B, 0x4C, 0x4D, 0x4E, 0x4F, 0x50, 0xD1, 0xD2, 0xD3, 0xD4, 0xD5,
    0xD6, 0xD7, 0xD8, 0xD9, 0x5A, 0x5B, 0x5C, 0x5D, 0x5E, 0x5F, 0x60, 0x61, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0x6A, 0x6B,
    0x6C, 0x6D, 0x6E, 0x6F, 0xF0, 0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8, 0xF9, 0x7A, 0x7B, 0x7C, 0x7D, 0x7E, 0x7F,
];

/// A buffer address as the data stream carries it: 12-bit coded below 4096, else 14-bit binary.
pub fn encode_address(address: usize) -> [u8; 2] {
    if address < 4096 {
        [CODES[address >> 6], CODES[address & 0x3F]]
    } else {
        [((address >> 8) & 0x3F) as u8, (address & 0xFF) as u8]
    }
}

pub fn decode_address(bytes: [u8; 2]) -> usize {
    if bytes[0] & 0xC0 == 0 {
        (usize::from(bytes[0] & 0x3F) << 8) | usize::from(bytes[1])
    } else {
        (usize::from(bytes[0] & 0x3F) << 6) | usize::from(bytes[1] & 0x3F)
    }
}

/// An attribute's graphic form: its low six bits through the code table.
pub fn attribute_byte(bits: u8) -> u8 {
    CODES[usize::from(bits & 0x3F)]
}

/// A field on the screen: where its attribute byte is, the attribute, and where its data runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldView {
    pub attribute_at: usize,
    pub attribute: u8,
    pub start: usize,
    pub len: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Screen {
    pub rows: usize,
    pub columns: usize,
    data: Vec<u8>,
    attributes: Vec<Option<u8>>,
    pub cursor: usize,
    pub alarm: bool,
    pub keyboard_locked: bool,
}

impl Screen {
    pub fn new(rows: usize, columns: usize) -> Self {
        let size = rows * columns;
        Self { rows, columns, data: vec![0; size], attributes: vec![None; size], cursor: 0, alarm: false, keyboard_locked: true }
    }

    pub fn size(&self) -> usize {
        self.rows * self.columns
    }

    /// Row and column (both from 1) of a buffer address.
    pub fn position(&self, address: usize) -> (usize, usize) {
        (address / self.columns + 1, address % self.columns + 1)
    }

    pub fn address(&self, row: usize, column: usize) -> usize {
        ((row - 1) * self.columns + column - 1) % self.size()
    }

    fn erase(&mut self) {
        self.data.fill(0);
        self.attributes.fill(None);
        self.cursor = 0;
    }

    /// Every field, in buffer order; empty when the screen is unformatted.
    pub fn fields(&self) -> Vec<FieldView> {
        let size = self.size();
        let starts: Vec<usize> = (0..size).filter(|&a| self.attributes[a].is_some()).collect();
        starts
            .iter()
            .enumerate()
            .map(|(i, &at)| {
                let next = starts[(i + 1) % starts.len()];
                let len = (next + size - at - 1) % size;
                FieldView { attribute_at: at, attribute: self.attributes[at].unwrap_or(0), start: (at + 1) % size, len: if starts.len() == 1 { size - 1 } else { len } }
            })
            .collect()
    }

    /// The field a buffer address falls in.
    pub fn field_at(&self, address: usize) -> Option<FieldView> {
        let size = self.size();
        self.fields().into_iter().find(|f| f.attribute_at == address || (address + size - f.start) % size < f.len)
    }

    pub fn field_data(&self, field: &FieldView) -> Vec<u8> {
        (0..field.len).map(|i| self.data[(field.start + i) % self.size()]).collect()
    }

    fn set_attribute(&mut self, address: usize, attribute: u8) {
        self.attributes[address] = Some(attribute);
        self.data[address] = 0;
    }

    fn put(&mut self, address: &mut usize, byte: u8) {
        self.attributes[*address] = None;
        self.data[*address] = byte;
        *address = (*address + 1) % self.size();
    }

    fn next_unprotected(&self, from: usize) -> usize {
        let size = self.size();
        (1..=size)
            .map(|i| (from + i) % size)
            .find(|&a| self.attributes[a].is_some_and(|attr| attr & PROTECTED == 0))
            .map_or(0, |a| (a + 1) % size)
    }

    /// Applies an outbound data stream: a command, its WCC, then orders and data.
    pub fn apply(&mut self, stream: &[u8]) -> Result<(), String> {
        let (&command, rest) = stream.split_first().ok_or("an empty data stream")?;
        match command {
            ERASE_WRITE | ERASE_WRITE_ALTERNATE | 0x05 | 0x0D => self.erase(),
            WRITE | 0x01 => {}
            ERASE_ALL_UNPROTECTED | 0x0F => {
                let size = self.size();
                for f in self.fields().iter().filter(|f| f.attribute & PROTECTED == 0) {
                    for i in 0..f.len {
                        self.data[(f.start + i) % size] = 0;
                    }
                    self.attributes[f.attribute_at] = Some(f.attribute & !MDT);
                }
                self.keyboard_locked = false;
                return Ok(());
            }
            other => return Err(format!("3270 command X'{other:02X}' is not supported")),
        }
        let Some((&wcc, mut orders)) = rest.split_first() else { return Ok(()) };
        if wcc & WCC_RESET_MDT != 0 {
            for a in 0..self.size() {
                if let Some(attr) = self.attributes[a] {
                    self.attributes[a] = Some(attr & !MDT);
                }
            }
        }
        let mut at = self.cursor;
        let size = self.size();
        let address = |bytes: &[u8]| -> Result<usize, String> {
            let [a, b] = bytes.get(..2).and_then(|s| s.try_into().ok()).ok_or("a buffer address cut short")?;
            let address = decode_address([a, b]);
            if address >= size { Err(format!("buffer address {address} is past the screen ({size})")) } else { Ok(address) }
        };
        while let Some((&byte, rest)) = orders.split_first() {
            orders = rest;
            match byte {
                SBA => {
                    at = address(orders)?;
                    orders = &orders[2..];
                }
                SF => {
                    let (&attr, rest) = orders.split_first().ok_or("SF without an attribute")?;
                    self.set_attribute(at, attr);
                    at = (at + 1) % size;
                    orders = rest;
                }
                SFE => {
                    let (&pairs, rest) = orders.split_first().ok_or("SFE without a count")?;
                    let pairs = usize::from(pairs);
                    let list = rest.get(..pairs * 2).ok_or("SFE cut short")?;
                    let attr = list.chunks(2).find(|p| p[0] == 0xC0).map_or(0x40, |p| p[1]);
                    self.set_attribute(at, attr);
                    at = (at + 1) % size;
                    orders = &rest[pairs * 2..];
                }
                SA => orders = orders.get(2..).ok_or("SA cut short")?,
                MF => {
                    let (&pairs, rest) = orders.split_first().ok_or("MF without a count")?;
                    orders = rest.get(usize::from(pairs) * 2..).ok_or("MF cut short")?;
                }
                IC => self.cursor = at,
                PT => at = self.next_unprotected(at),
                RA => {
                    let stop = address(orders)?;
                    let fill = *orders.get(2).ok_or("RA without a character")?;
                    orders = &orders[3..];
                    loop {
                        self.put(&mut at, fill);
                        if at == stop {
                            break;
                        }
                    }
                }
                EUA => {
                    let stop = address(orders)?;
                    orders = &orders[2..];
                    while at != stop {
                        if self.field_at(at).is_some_and(|f| f.attribute & PROTECTED == 0) && self.attributes[at].is_none() {
                            self.data[at] = 0;
                        }
                        at = (at + 1) % size;
                    }
                }
                data => self.put(&mut at, data),
            }
        }
        self.alarm = wcc & WCC_ALARM != 0;
        if wcc & WCC_RESTORE != 0 {
            self.keyboard_locked = false;
        }
        Ok(())
    }

    /// The operator typing `text` (EBCDIC) at an address: it must be in an unprotected field, and
    /// it sets the field's modified bit. Typing stops at the end of the field.
    pub fn type_at(&mut self, address: usize, text: &[u8]) -> Result<(), String> {
        let field = self.field_at(address).ok_or("the screen has no fields")?;
        if field.attribute & PROTECTED != 0 || field.attribute_at == address {
            return Err(format!("row {} column {} is protected", self.position(address).0, self.position(address).1));
        }
        let size = self.size();
        let offset = (address + size - field.start) % size;
        for (i, &b) in text.iter().take(field.len - offset).enumerate() {
            self.data[(address + i) % size] = b;
        }
        self.attributes[field.attribute_at] = Some(field.attribute | MDT);
        self.cursor = (address + text.len().min(field.len - offset)) % size;
        Ok(())
    }

    /// ERASE EOF: clears the rest of the field at an address and sets its modified bit.
    pub fn erase_eof(&mut self, address: usize) -> Result<(), String> {
        let field = self.field_at(address).ok_or("the screen has no fields")?;
        if field.attribute & PROTECTED != 0 {
            return Err("ERASE EOF in a protected field".into());
        }
        let size = self.size();
        let offset = (address + size - field.start) % size;
        for i in offset..field.len {
            self.data[(field.start + i) % size] = 0;
        }
        self.attributes[field.attribute_at] = Some(field.attribute | MDT);
        Ok(())
    }

    /// The inbound stream an AID key sends: Read Modified, or the short read of CLEAR and the PA
    /// keys (the AID alone). Nulls in a field's data are not sent.
    pub fn read_modified(&self, aid: u8) -> Vec<u8> {
        if matches!(aid, AID_CLEAR | AID_PA1 | AID_PA2 | AID_PA3) {
            return vec![aid];
        }
        let mut out = vec![aid];
        out.extend(encode_address(self.cursor));
        for f in self.fields().iter().filter(|f| f.attribute & MDT != 0) {
            out.push(SBA);
            out.extend(encode_address(f.start));
            out.extend(self.field_data(f).into_iter().filter(|&b| b != 0));
        }
        out
    }

    /// The screen as text: attribute positions and nulls as spaces, non-display fields blank.
    pub fn render(&self, page: &CodePage) -> String {
        let mut shown: Vec<char> = self.data.iter().map(|&b| if b == 0 { ' ' } else { page.decode_byte(b) }).collect();
        for f in self.fields() {
            shown[f.attribute_at] = ' ';
            if f.attribute & DISPLAY == NON_DISPLAY {
                for i in 0..f.len {
                    shown[(f.start + i) % self.size()] = ' ';
                }
            }
        }
        shown.chunks(self.columns).map(|row| row.iter().collect::<String>().trim_end().to_owned()).collect::<Vec<_>>().join("\n")
    }
}

/// What an AID key sent: the key, the cursor (absent in a short read), and each modified field's
/// data with the address it starts at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inbound {
    pub aid: u8,
    pub cursor: Option<usize>,
    pub fields: Vec<(usize, Vec<u8>)>,
}

pub fn parse_inbound(stream: &[u8]) -> Result<Inbound, String> {
    let (&aid, rest) = stream.split_first().ok_or("an empty inbound stream")?;
    if rest.is_empty() {
        return Ok(Inbound { aid, cursor: None, fields: Vec::new() });
    }
    let cursor = decode_address(rest.get(..2).and_then(|s| s.try_into().ok()).ok_or("the cursor address is cut short")?);
    let mut fields = Vec::new();
    let mut i = 2;
    while i < rest.len() {
        if rest[i] != SBA {
            return Err(format!("expected SBA at byte {}", i + 1));
        }
        let at = decode_address(rest.get(i + 1..i + 3).and_then(|s| s.try_into().ok()).ok_or("an SBA address is cut short")?);
        let end = rest[i + 3..].iter().position(|&b| b == SBA).map_or(rest.len(), |p| i + 3 + p);
        fields.push((at, rest[i + 3..end].to_vec()));
        i = end;
    }
    Ok(Inbound { aid, cursor: Some(cursor), fields })
}

/// The AID byte a key sends: ENTER, CLEAR, PA1-PA3, PF1-PF24.
pub fn aid_of(key: &str) -> Option<u8> {
    let key = key.to_ascii_uppercase();
    let pf = |n: u8| -> u8 {
        match n {
            1..=9 => 0xF0 + n,
            10..=12 => 0x7A + n - 10,
            13..=21 => 0xC1 + n - 13,
            _ => 0x4A + n - 22,
        }
    };
    Some(match key.as_str() {
        "ENTER" => AID_ENTER,
        "CLEAR" => AID_CLEAR,
        "PA1" => AID_PA1,
        "PA2" => AID_PA2,
        "PA3" => AID_PA3,
        k => match k.strip_prefix("PF").and_then(|n| n.parse::<u8>().ok()) {
            Some(n @ 1..=24) => pf(n),
            _ => return None,
        },
    })
}

/// One thing the operator does at the keyboard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Type { row: usize, column: usize, text: String },
    EraseEof { row: usize, column: usize },
    Cursor { row: usize, column: usize },
    Key(u8),
}

/// A scripted operator: `type ROW COL text`, `eof ROW COL`, `cursor ROW COL`, and an AID key
/// (`ENTER`, `CLEAR`, `PA1`..`PA3`, `PF1`..`PF24`) that ends each turn. `#` starts a comment.
pub fn parse_script(text: &str) -> Result<Vec<Action>, String> {
    let mut actions = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let bad = |why: &str| format!("screen script line {}: {why}: {raw}", n + 1);
        let mut words = line.splitn(4, ' ');
        let verb = words.next().unwrap_or("").to_ascii_lowercase();
        let at = |words: &mut std::str::SplitN<'_, char>| -> Result<(usize, usize), String> {
            let row = words.next().and_then(|w| w.parse().ok()).filter(|&r: &usize| r >= 1).ok_or_else(|| bad("expected a row from 1"))?;
            let column = words.next().and_then(|w| w.parse().ok()).filter(|&c: &usize| c >= 1).ok_or_else(|| bad("expected a column from 1"))?;
            Ok((row, column))
        };
        actions.push(match verb.as_str() {
            "type" => {
                let (row, column) = at(&mut words)?;
                Action::Type { row, column, text: words.next().unwrap_or("").to_owned() }
            }
            "eof" => {
                let (row, column) = at(&mut words)?;
                Action::EraseEof { row, column }
            }
            "cursor" => {
                let (row, column) = at(&mut words)?;
                Action::Cursor { row, column }
            }
            key => Action::Key(aid_of(key).ok_or_else(|| bad("not type, eof, cursor or an AID key"))?),
        });
    }
    Ok(actions)
}

/// A terminal driven by a screen script: each SEND is applied to the screen and its rendering kept
/// in `shown`; each RECEIVE plays the script up to its next AID key.
pub struct Scripted {
    pub screen: Screen,
    actions: std::collections::VecDeque<Action>,
    page: &'static CodePage,
    pub shown: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
}

impl std::fmt::Debug for Scripted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Scripted({}x{}, {} actions left)", self.screen.rows, self.screen.columns, self.actions.len())
    }
}

impl Scripted {
    pub fn new(rows: usize, columns: usize, actions: Vec<Action>, page: &'static CodePage) -> Self {
        Self { screen: Screen::new(rows, columns), actions: actions.into(), page, shown: Default::default() }
    }
}

impl Terminal for Scripted {
    fn size(&self) -> (usize, usize) {
        (self.screen.rows, self.screen.columns)
    }

    fn send(&mut self, stream: &[u8]) -> Result<(), String> {
        self.screen.apply(stream)?;
        self.shown.borrow_mut().push(self.screen.render(self.page));
        Ok(())
    }

    fn receive(&mut self) -> Result<Option<Vec<u8>>, String> {
        while let Some(action) = self.actions.pop_front() {
            match action {
                Action::Type { row, column, text } => {
                    let bytes = self.page.encode_lossy(&text);
                    self.screen.type_at(self.screen.address(row, column), &bytes)?;
                }
                Action::EraseEof { row, column } => self.screen.erase_eof(self.screen.address(row, column))?,
                Action::Cursor { row, column } => self.screen.cursor = self.screen.address(row, column),
                Action::Key(aid) => return Ok(Some(self.screen.read_modified(aid))),
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> &'static CodePage {
        CodePage::by_ccsid(37).unwrap()
    }

    #[test]
    fn aid_bytes_match_the_dfhaid_copybook() {
        let copybook = syntax::system::member("DFHAID").unwrap();
        let dfh = |name: &str| -> u8 {
            let line = copybook.lines().find(|l| l.split_whitespace().nth(1) == Some(name)).unwrap();
            let hex = line.split("X'").nth(1).unwrap().split('\'').next().unwrap();
            u8::from_str_radix(hex, 16).unwrap()
        };
        for (name, aid) in [("DFHENTER", AID_ENTER), ("DFHCLEAR", AID_CLEAR), ("DFHPA1", AID_PA1), ("DFHPA2", AID_PA2), ("DFHPA3", AID_PA3)] {
            assert_eq!(dfh(name), aid, "{name}");
        }
        for n in 1..=24 {
            assert_eq!(aid_of(&format!("PF{n}")), Some(dfh(&format!("DFHPF{n}"))), "PF{n}");
        }
    }

    #[test]
    fn addresses_round_trip_in_both_encodings() {
        for a in [0, 1, 79, 80, 1919, 4095] {
            assert_eq!(decode_address(encode_address(a)), a);
        }
        assert_eq!(encode_address(80), [0xC1, 0x50]);
        assert_eq!(decode_address(encode_address(5000)), 5000);
        assert_eq!(encode_address(5000)[0] & 0xC0, 0);
    }

    #[test]
    fn a_write_builds_fields_and_read_modified_returns_what_was_typed() {
        let mut s = Screen::new(24, 80);
        let mut stream = vec![ERASE_WRITE, WCC_RESTORE, SBA];
        stream.extend(encode_address(0));
        stream.extend([SF, attribute_byte(PROTECTED | INTENSIFIED)]);
        stream.extend(page().encode("NAME:").unwrap());
        stream.extend([SF, attribute_byte(0), IC]);
        stream.extend([SBA]);
        stream.extend(encode_address(20));
        stream.extend([SF, attribute_byte(PROTECTED)]);
        s.apply(&stream).unwrap();
        assert_eq!(s.fields().len(), 3);
        assert_eq!(s.cursor, 7);
        assert!(!s.keyboard_locked);
        assert!(s.type_at(1, b"x").is_err());
        s.type_at(7, &page().encode("SMITH").unwrap()).unwrap();
        assert_eq!(s.render(page()).lines().next().unwrap(), " NAME: SMITH");
        let inbound = s.read_modified(AID_ENTER);
        let read = parse_inbound(&inbound).unwrap();
        assert_eq!((read.aid, read.cursor), (AID_ENTER, Some(12)));
        assert_eq!(read.fields, vec![(7, page().encode("SMITH").unwrap())]);
        assert_eq!(s.read_modified(AID_CLEAR), vec![AID_CLEAR]);
    }

    #[test]
    fn wcc_resets_modified_bits_and_eau_clears_unprotected_data() {
        let mut s = Screen::new(24, 80);
        let mut stream = vec![ERASE_WRITE, WCC_RESTORE, SF, attribute_byte(0)];
        stream.extend(page().encode("AB").unwrap());
        s.apply(&stream).unwrap();
        s.type_at(1, &page().encode("Z").unwrap()).unwrap();
        assert_eq!(s.fields()[0].attribute & MDT, MDT);
        s.apply(&[WRITE, WCC_RESET_MDT]).unwrap();
        assert_eq!(s.fields()[0].attribute & MDT, 0);
        s.apply(&[ERASE_ALL_UNPROTECTED]).unwrap();
        assert!(s.field_data(&s.fields()[0]).iter().all(|&b| b == 0));
    }

    #[test]
    fn non_display_fields_render_blank_and_bad_streams_are_refused() {
        let mut s = Screen::new(2, 10);
        let mut stream = vec![ERASE_WRITE, 0, SF, attribute_byte(NON_DISPLAY)];
        stream.extend(page().encode("SECRET").unwrap());
        s.apply(&stream).unwrap();
        assert_eq!(s.render(page()), "\n");
        assert!(s.apply(&[0x99]).is_err());
        assert!(s.apply(&[WRITE, 0, SBA, 0x7F]).is_err());
    }

    #[test]
    fn aid_keys_and_screen_scripts() {
        assert_eq!(aid_of("enter"), Some(0x7D));
        assert_eq!(aid_of("PF1"), Some(0xF1));
        assert_eq!(aid_of("PF12"), Some(0x7C));
        assert_eq!(aid_of("PF13"), Some(0xC1));
        assert_eq!(aid_of("PF24"), Some(0x4C));
        assert_eq!(aid_of("PF25"), None);
        let script = parse_script("# order entry\ntype 5 20 SMITH & CO\neof 6 20\ncursor 7 1\nENTER\nPF3\n").unwrap();
        assert_eq!(script[0], Action::Type { row: 5, column: 20, text: "SMITH & CO".into() });
        assert_eq!(script[1], Action::EraseEof { row: 6, column: 20 });
        assert_eq!(script[3], Action::Key(0x7D));
        assert_eq!(script[4], Action::Key(0xF3));
        assert!(parse_script("type 0 1 X").is_err());
        assert!(parse_script("jump 1 1").is_err());
    }
}
