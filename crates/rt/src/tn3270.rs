//! A `Terminal` over a TCP connection speaking TN3270 (RFC 1576): telnet negotiation of the
//! terminal type, EOR and BINARY, then 3270 data streams as records ended by IAC EOR.

use crate::terminal::Terminal;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::rc::Rc;
use std::time::Duration;

const IAC: u8 = 255;
const DONT: u8 = 254;
const DO: u8 = 253;
const WONT: u8 = 252;
const WILL: u8 = 251;
const SB: u8 = 250;
const SE: u8 = 240;
const EOR_COMMAND: u8 = 239;

const BINARY: u8 = 0;
const TERMINAL_TYPE: u8 = 24;
const EOR: u8 = 25;

const TT_IS: u8 = 0;
const TT_SEND: u8 = 1;

/// How long a client may take over the negotiation before the connection is given up.
const NEGOTIATION_TIMEOUT: Duration = Duration::from_secs(30);

/// One unit of the telnet stream.
enum Item {
    Data(u8),
    Verb(u8, u8),
    Sub(Vec<u8>),
    Eor,
    Other,
}

/// What the client has agreed to so far.
#[derive(Default)]
struct Agreed {
    terminal_type: bool,
    eor_in: bool,
    eor_out: bool,
    binary_in: bool,
    binary_out: bool,
}

impl Agreed {
    fn options(&self) -> bool {
        self.eor_in && self.eor_out && self.binary_in && self.binary_out
    }
}

/// A connected 3270 terminal.
#[derive(Debug)]
pub struct Tn3270 {
    stream: TcpStream,
    buffer: Vec<u8>,
    at: usize,
    rows: usize,
    columns: usize,
    pub terminal_type: String,
    pending: VecDeque<Vec<u8>>,
}

/// The screen size a terminal type names: IBM-327x-N with model N, an -E suffix changing nothing.
pub fn screen_size(terminal_type: &str) -> (usize, usize) {
    let upper = terminal_type.to_ascii_uppercase();
    let model = upper.strip_prefix("IBM-3278-").or_else(|| upper.strip_prefix("IBM-3279-")).and_then(|m| m.chars().next());
    match model {
        Some('3') => (32, 80),
        Some('4') => (43, 80),
        Some('5') => (27, 132),
        _ => (24, 80),
    }
}

/// Negotiates the terminal type, EOR and BINARY with a freshly connected client.
pub fn negotiate(stream: TcpStream) -> Result<Tn3270, String> {
    let io = |e: std::io::Error| e.to_string();
    stream.set_nodelay(true).map_err(io)?;
    stream.set_read_timeout(Some(NEGOTIATION_TIMEOUT)).map_err(io)?;
    let mut t = Tn3270 { stream, buffer: Vec::new(), at: 0, rows: 24, columns: 80, terminal_type: String::new(), pending: VecDeque::new() };
    let mut agreed = Agreed::default();

    t.write(&[IAC, DO, TERMINAL_TYPE])?;
    while !agreed.terminal_type {
        match t.expect_item()? {
            Item::Verb(WONT, TERMINAL_TYPE) => return Err("the client refused TERMINAL-TYPE: this is not a 3270 emulator".into()),
            Item::Verb(verb, option) => t.note(verb, option, &mut agreed)?,
            _ => {}
        }
    }

    t.write(&[IAC, SB, TERMINAL_TYPE, TT_SEND, IAC, SE])?;
    loop {
        match t.expect_item()? {
            Item::Sub(payload) if payload.starts_with(&[TERMINAL_TYPE, TT_IS]) => {
                t.terminal_type = String::from_utf8_lossy(&payload[2..]).trim().to_ascii_uppercase();
                break;
            }
            Item::Verb(verb, option) => t.note(verb, option, &mut agreed)?,
            _ => {}
        }
    }
    (t.rows, t.columns) = screen_size(&t.terminal_type);

    t.write(&[IAC, DO, EOR, IAC, WILL, EOR, IAC, DO, BINARY, IAC, WILL, BINARY])?;
    while !agreed.options() {
        if let Item::Verb(verb, option) = t.expect_item()? {
            t.note(verb, option, &mut agreed)?;
        }
    }
    t.stream.set_read_timeout(None).map_err(io)?;
    Ok(t)
}

impl Tn3270 {
    /// Queues a record that the next `receive` returns before reading the connection.
    pub fn push_back(&mut self, record: Vec<u8>) {
        self.pending.push_back(record);
    }

    /// Drops queued input no task read: a task's terminal input ends with the task.
    pub fn discard_pending(&mut self) {
        self.pending.clear();
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.stream.write_all(bytes).and_then(|()| self.stream.flush()).map_err(|e| format!("writing to the terminal: {e}"))
    }

    /// The next byte of the connection, None at end of stream.
    fn byte(&mut self) -> Result<Option<u8>, String> {
        if self.at == self.buffer.len() {
            self.buffer.resize(4096, 0);
            self.at = 0;
            let n = loop {
                match self.stream.read(&mut self.buffer) {
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(e) => {
                        self.buffer.clear();
                        return Err(format!("reading from the terminal: {e}"));
                    }
                    Ok(n) => break n,
                }
            };
            self.buffer.truncate(n);
            if n == 0 {
                return Ok(None);
            }
        }
        self.at += 1;
        Ok(Some(self.buffer[self.at - 1]))
    }

    fn need(&mut self) -> Result<u8, String> {
        self.byte()?.ok_or_else(|| "the terminal closed the connection inside a telnet command".to_string())
    }

    /// The next data byte or telnet command; None at end of stream between items.
    fn item(&mut self) -> Result<Option<Item>, String> {
        let Some(b) = self.byte()? else { return Ok(None) };
        if b != IAC {
            return Ok(Some(Item::Data(b)));
        }
        Ok(Some(match self.need()? {
            IAC => Item::Data(IAC),
            EOR_COMMAND => Item::Eor,
            verb @ (DO | DONT | WILL | WONT) => Item::Verb(verb, self.need()?),
            SB => {
                let mut payload = Vec::new();
                loop {
                    match self.need()? {
                        IAC => match self.need()? {
                            SE => break,
                            IAC => payload.push(IAC),
                            _ => {}
                        },
                        b => payload.push(b),
                    }
                }
                Item::Sub(payload)
            }
            _ => Item::Other,
        }))
    }

    fn expect_item(&mut self) -> Result<Item, String> {
        self.item()?.ok_or_else(|| "the terminal closed the connection during negotiation".to_string())
    }

    /// Records the client's word on an option, and refuses any option we do not use.
    fn note(&mut self, verb: u8, option: u8, agreed: &mut Agreed) -> Result<(), String> {
        let flag = match (option, verb) {
            (TERMINAL_TYPE, WILL) => Some(&mut agreed.terminal_type),
            (EOR, WILL) => Some(&mut agreed.eor_in),
            (EOR, DO) => Some(&mut agreed.eor_out),
            (BINARY, WILL) => Some(&mut agreed.binary_in),
            (BINARY, DO) => Some(&mut agreed.binary_out),
            (EOR | BINARY, WONT | DONT) => return Err(format!("the client refused telnet option {option}: this is not a 3270 emulator")),
            _ => None,
        };
        match flag {
            Some(flag) => *flag = true,
            None => self.refuse(verb, option)?,
        }
        Ok(())
    }

    /// Answers a request for an option we do not use.
    fn refuse(&mut self, verb: u8, option: u8) -> Result<(), String> {
        match verb {
            DO => self.write(&[IAC, WONT, option]),
            WILL => self.write(&[IAC, DONT, option]),
            _ => Ok(()),
        }
    }
}

impl Terminal for Tn3270 {
    fn size(&self) -> (usize, usize) {
        (self.rows, self.columns)
    }

    fn send(&mut self, stream: &[u8]) -> Result<(), String> {
        let mut wire = Vec::with_capacity(stream.len() + 2);
        for &b in stream {
            wire.push(b);
            if b == IAC {
                wire.push(IAC);
            }
        }
        wire.extend_from_slice(&[IAC, EOR_COMMAND]);
        self.write(&wire)
    }

    fn receive(&mut self) -> Result<Option<Vec<u8>>, String> {
        if let Some(record) = self.pending.pop_front() {
            return Ok(Some(record));
        }
        let mut record = Vec::new();
        loop {
            match self.item()? {
                None if record.is_empty() => return Ok(None),
                None => return Err("the terminal closed the connection inside a record".into()),
                Some(Item::Data(b)) => record.push(b),
                Some(Item::Eor) => return Ok(Some(record)),
                Some(Item::Verb(verb, option)) if !matches!(option, EOR | BINARY) => self.refuse(verb, option)?,
                Some(_) => {}
            }
        }
    }
}

/// A terminal shared between the serving loop and the task that is using it.
#[derive(Clone, Debug)]
pub struct Shared(pub Rc<RefCell<Tn3270>>);

impl Terminal for Shared {
    fn size(&self) -> (usize, usize) {
        self.0.borrow().size()
    }

    fn send(&mut self, stream: &[u8]) -> Result<(), String> {
        self.0.borrow_mut().send(stream)
    }

    fn receive(&mut self) -> Result<Option<Vec<u8>>, String> {
        self.0.borrow_mut().receive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread::JoinHandle;

    fn read_exact(stream: &mut TcpStream, n: usize) -> Vec<u8> {
        let mut bytes = vec![0; n];
        stream.read_exact(&mut bytes).unwrap();
        bytes
    }

    /// The client's side of the negotiation, reporting `terminal_type`; None for WONT TERMINAL-TYPE.
    fn client_negotiates(stream: &mut TcpStream, terminal_type: Option<&str>) {
        assert_eq!(read_exact(stream, 3), [IAC, DO, TERMINAL_TYPE]);
        let Some(name) = terminal_type else {
            stream.write_all(&[IAC, WONT, TERMINAL_TYPE]).unwrap();
            return;
        };
        stream.write_all(&[IAC, WILL, TERMINAL_TYPE]).unwrap();
        assert_eq!(read_exact(stream, 6), [IAC, SB, TERMINAL_TYPE, TT_SEND, IAC, SE]);
        let mut reply = vec![IAC, SB, TERMINAL_TYPE, TT_IS];
        reply.extend_from_slice(name.as_bytes());
        reply.extend_from_slice(&[IAC, SE]);
        stream.write_all(&reply).unwrap();
        assert_eq!(read_exact(stream, 12), [IAC, DO, EOR, IAC, WILL, EOR, IAC, DO, BINARY, IAC, WILL, BINARY]);
        stream.write_all(&[IAC, WILL, BINARY, IAC, DO, BINARY, IAC, WILL, EOR, IAC, DO, EOR]).unwrap();
    }

    /// A connected pair: the server side negotiated, and the client thread run by `client`.
    fn pair(terminal_type: &str, client: impl FnOnce(TcpStream) + Send + 'static) -> (Tn3270, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let name = terminal_type.to_string();
        let handle = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(address).unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
            client_negotiates(&mut stream, Some(&name));
            client(stream);
        });
        let (server, _) = listener.accept().unwrap();
        (negotiate(server).unwrap(), handle)
    }

    #[test]
    fn negotiation_learns_the_terminal_type_and_size() {
        let (t, client) = pair("IBM-3278-4", |_| {});
        assert_eq!(t.terminal_type, "IBM-3278-4");
        assert_eq!(t.size(), (43, 80));
        client.join().unwrap();
    }

    #[test]
    fn screen_sizes_follow_the_model_number() {
        assert_eq!(screen_size("IBM-3278-2"), (24, 80));
        assert_eq!(screen_size("IBM-3278-3"), (32, 80));
        assert_eq!(screen_size("IBM-3278-5-E"), (27, 132));
        assert_eq!(screen_size("IBM-3279-4-E"), (43, 80));
        assert_eq!(screen_size("ibm-3279-3"), (32, 80));
        assert_eq!(screen_size("IBM-DYNAMIC"), (24, 80));
        assert_eq!(screen_size("VT100"), (24, 80));
    }

    #[test]
    fn a_client_that_refuses_terminal_type_is_not_a_3270() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let client = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(address).unwrap();
            client_negotiates(&mut stream, None);
        });
        let (server, _) = listener.accept().unwrap();
        let error = negotiate(server).unwrap_err();
        assert!(error.contains("not a 3270 emulator"), "{error}");
        client.join().unwrap();
    }

    #[test]
    fn negotiation_refuses_options_the_client_offers_unasked() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let client = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(address).unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
            stream.write_all(&[IAC, WILL, 31]).unwrap();
            assert_eq!(read_exact(&mut stream, 3), [IAC, DO, TERMINAL_TYPE]);
            assert_eq!(read_exact(&mut stream, 3), [IAC, DONT, 31]);
            stream.write_all(&[IAC, DO, 1, IAC, WILL, TERMINAL_TYPE]).unwrap();
            assert_eq!(read_exact(&mut stream, 3), [IAC, WONT, 1]);
            assert_eq!(read_exact(&mut stream, 6), [IAC, SB, TERMINAL_TYPE, TT_SEND, IAC, SE]);
            stream.write_all(&[IAC, SB, TERMINAL_TYPE, TT_IS, b'I', b'B', b'M', b'-', b'3', b'2', b'7', b'8', b'-', b'2', IAC, SE]).unwrap();
            assert_eq!(read_exact(&mut stream, 12).len(), 12);
            stream.write_all(&[IAC, DO, EOR, IAC, WILL, EOR, IAC, DO, BINARY, IAC, WILL, BINARY]).unwrap();
        });
        let (server, _) = listener.accept().unwrap();
        let t = negotiate(server).unwrap();
        assert_eq!(t.size(), (24, 80));
        client.join().unwrap();
    }

    #[test]
    fn send_doubles_iac_and_ends_the_record() {
        let (mut t, client) = pair("IBM-3278-2", |mut stream| {
            assert_eq!(read_exact(&mut stream, 7), [0xF5, 0xC2, IAC, IAC, 0x01, IAC, EOR_COMMAND]);
        });
        t.send(&[0xF5, 0xC2, 0xFF, 0x01]).unwrap();
        client.join().unwrap();
    }

    #[test]
    fn receive_undoes_doubled_iac_and_skips_telnet_commands() {
        let (mut t, client) = pair("IBM-3278-2", |mut stream| {
            let mut wire = vec![IAC, DO, 99, IAC, SB, TERMINAL_TYPE, TT_SEND, IAC, IAC, IAC, SE, 0x7D, IAC, IAC, 0x40, IAC, 241];
            wire.extend_from_slice(&[0x41, IAC, EOR_COMMAND]);
            stream.write_all(&wire).unwrap();
            assert_eq!(read_exact(&mut stream, 3), [IAC, WONT, 99]);
        });
        assert_eq!(t.receive().unwrap(), Some(vec![0x7D, IAC, 0x40, 0x41]));
        assert_eq!(t.receive().unwrap(), None);
        client.join().unwrap();
    }

    #[test]
    fn a_pushed_back_record_comes_before_the_connection() {
        let (mut t, client) = pair("IBM-3278-2", |mut stream| {
            stream.write_all(&[0x7D, 0x01, IAC, EOR_COMMAND]).unwrap();
        });
        t.push_back(vec![0xF3]);
        t.push_back(vec![0xF4]);
        assert_eq!(t.receive().unwrap(), Some(vec![0xF3]));
        assert_eq!(t.receive().unwrap(), Some(vec![0xF4]));
        assert_eq!(t.receive().unwrap(), Some(vec![0x7D, 0x01]));
        assert_eq!(t.receive().unwrap(), None);
        client.join().unwrap();
    }
}
