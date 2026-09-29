//! PostgreSQL's frontend/backend protocol, version 3: startup and authentication, the simple query
//! cycle, and the extended one (Parse, Describe, Bind, Execute, Sync) with text values.

use super::scram::{Scram, nonce};
use std::io::{BufReader, BufWriter, Read, Write};

/// Where and as whom to connect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: Option<String>,
    pub database: String,
}

impl Target {
    /// `postgres://user[:password]@host[:port]/database[?host=/socket/directory]`. A host that
    /// starts with `/` is a Unix socket's directory; the password may come from PGPASSWORD instead.
    pub fn parse(url: &str) -> Result<Self, String> {
        let rest = url.strip_prefix("postgres://").or_else(|| url.strip_prefix("postgresql://")).ok_or("a database URL starts postgres://")?;
        let (rest, query) = rest.split_once('?').unwrap_or((rest, ""));
        let (authority, database) = rest.split_once('/').unwrap_or((rest, ""));
        let (userinfo, hostport) = authority.rsplit_once('@').map_or((None, authority), |(u, h)| (Some(u), h));
        let (user, password) = match userinfo.map(|u| u.split_once(':').map_or((u, None), |(u, p)| (u, Some(p)))) {
            Some((u, p)) => (Some(decode(u)?), p.map(decode).transpose()?),
            None => (None, None),
        };
        let (mut host, port) = match hostport.rsplit_once(':') {
            Some((h, p)) => (h.to_owned(), p.parse().map_err(|_| format!("{p} is not a port"))?),
            None => (hostport.to_owned(), 5432),
        };
        for pair in query.split('&').filter(|p| !p.is_empty()) {
            match pair.split_once('=') {
                Some(("host", h)) => host = decode(h)?,
                _ => return Err(format!("{pair} is not a URL option ironwork reads")),
            }
        }
        let user = match user {
            Some(u) => u,
            None => std::env::var("USER").map_err(|_| "the URL names no user, and USER is not set")?,
        };
        Ok(Self {
            host: if host.is_empty() { "localhost".into() } else { decode(&host)? },
            port,
            database: if database.is_empty() { user.clone() } else { decode(database)? },
            password: password.or_else(|| std::env::var("PGPASSWORD").ok()),
            user,
        })
    }
}

fn decode(text: &str) -> Result<String, String> {
    let bytes = text.as_bytes();
    let (mut out, mut i) = (Vec::new(), 0);
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = text.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()).ok_or("the URL has a bad % escape")?;
            out.push(hex);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "the URL is not UTF-8 once decoded".to_owned())
}

/// Why a request failed: the server refused it, naming an SQLSTATE, or the connection broke.
#[derive(Debug, PartialEq, Eq)]
pub enum Failure {
    Refused { state: String, message: String },
    Broken(String),
}

impl From<std::io::Error> for Failure {
    fn from(e: std::io::Error) -> Self {
        Failure::Broken(format!("the connection to PostgreSQL failed: {e}"))
    }
}

/// What Execute returned: each row's columns as text, and the command tag.
#[derive(Debug, Default)]
pub struct Executed {
    pub rows: Vec<Vec<Option<String>>>,
    pub tag: String,
}

/// A prepared statement's parameter and column types.
#[derive(Clone, Debug, Default)]
pub struct Described {
    pub parameters: Vec<u32>,
    pub columns: Vec<u32>,
}

pub struct Connection {
    reader: BufReader<Box<dyn Read>>,
    writer: BufWriter<Box<dyn Write>>,
    pub server_version: String,
    /// ReadyForQuery's transaction status: `I` idle, `T` in a transaction, `E` in a failed one.
    pub status: u8,
}

struct Body<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Body<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Failure> {
        let got = self.bytes.get(self.at..self.at + n).ok_or_else(|| Failure::Broken("PostgreSQL sent a short message".into()))?;
        self.at += n;
        Ok(got)
    }
    fn i16(&mut self) -> Result<i16, Failure> {
        self.take(2).map(|b| i16::from_be_bytes([b[0], b[1]]))
    }
    fn i32(&mut self) -> Result<i32, Failure> {
        self.take(4).map(|b| i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn cstr(&mut self) -> Result<String, Failure> {
        let end = self.bytes[self.at..].iter().position(|&b| b == 0).ok_or_else(|| Failure::Broken("PostgreSQL sent an unterminated string".into()))?;
        let s = String::from_utf8_lossy(&self.bytes[self.at..self.at + end]).into_owned();
        self.at += end + 1;
        Ok(s)
    }
    fn rest(&mut self) -> &'a [u8] {
        let r = &self.bytes[self.at..];
        self.at = self.bytes.len();
        r
    }
}

fn cstr(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(s.as_bytes());
    out.push(0);
}

/// An ErrorResponse's SQLSTATE and message.
fn refusal(body: &[u8]) -> Failure {
    let mut b = Body { bytes: body, at: 0 };
    let (mut state, mut message) = (String::new(), String::new());
    while let Ok(field) = b.take(1) {
        if field[0] == 0 {
            break;
        }
        let Ok(value) = b.cstr() else { break };
        match field[0] {
            b'C' => state = value,
            b'M' => message = value,
            _ => {}
        }
    }
    Failure::Refused { state, message }
}

impl Connection {
    pub fn open(target: &Target) -> Result<Self, String> {
        let broken = |e: std::io::Error| format!("cannot reach PostgreSQL at {}:{}: {e}", target.host, target.port);
        let (reader, writer): (Box<dyn Read>, Box<dyn Write>) = if target.host.starts_with('/') {
            unix_socket(target).map_err(broken)?
        } else {
            let stream = std::net::TcpStream::connect((target.host.as_str(), target.port)).map_err(broken)?;
            stream.set_nodelay(true).map_err(broken)?;
            (Box::new(stream.try_clone().map_err(broken)?), Box::new(stream))
        };
        let mut conn = Self { reader: BufReader::new(reader), writer: BufWriter::new(writer), server_version: String::new(), status: b'I' };
        conn.start(target).map_err(|f| match f {
            Failure::Refused { state, message } => format!("PostgreSQL refused the connection ({state}): {message}"),
            Failure::Broken(m) => m,
        })?;
        Ok(conn)
    }

    fn start(&mut self, target: &Target) -> Result<(), Failure> {
        let mut body = 196_608i32.to_be_bytes().to_vec();
        let params = [
            ("user", target.user.as_str()),
            ("database", &target.database),
            ("client_encoding", "UTF8"),
            ("DateStyle", "ISO"),
            ("TimeZone", "UTC"),
            ("extra_float_digits", "3"),
            ("application_name", "ironwork"),
        ];
        for (name, value) in params {
            cstr(&mut body, name);
            cstr(&mut body, value);
        }
        body.push(0);
        self.writer.write_all(&(body.len() as i32 + 4).to_be_bytes())?;
        self.writer.write_all(&body)?;
        self.writer.flush()?;
        let password = || target.password.clone().ok_or_else(|| Failure::Broken("PostgreSQL asks for a password: give one in the URL or PGPASSWORD".into()));
        let mut scram = None;
        loop {
            let (kind, body) = self.receive()?;
            let mut b = Body { bytes: &body, at: 0 };
            match kind {
                b'R' => match b.i32()? {
                    0 => {}
                    3 => {
                        let mut reply = Vec::new();
                        cstr(&mut reply, &password()?);
                        self.send(b'p', &reply)?;
                        self.writer.flush()?;
                    }
                    10 => {
                        let mechanisms: Vec<String> = std::iter::from_fn(|| b.cstr().ok().filter(|m| !m.is_empty())).collect();
                        if !mechanisms.iter().any(|m| m == "SCRAM-SHA-256") {
                            return Err(Failure::Broken(format!("PostgreSQL offers SASL {mechanisms:?}, and ironwork speaks SCRAM-SHA-256")));
                        }
                        let mut exchange = Scram::new(&password()?, nonce());
                        let first = exchange.client_first("");
                        let mut reply = Vec::new();
                        cstr(&mut reply, "SCRAM-SHA-256");
                        reply.extend_from_slice(&(first.len() as i32).to_be_bytes());
                        reply.extend_from_slice(first.as_bytes());
                        self.send(b'p', &reply)?;
                        self.writer.flush()?;
                        scram = Some(exchange);
                    }
                    11 => {
                        let exchange = scram.as_mut().ok_or_else(|| Failure::Broken("PostgreSQL continued a SASL exchange that had not begun".into()))?;
                        let reply = exchange.client_final(&String::from_utf8_lossy(b.rest())).map_err(Failure::Broken)?;
                        self.send(b'p', reply.as_bytes())?;
                        self.writer.flush()?;
                    }
                    12 => {
                        let exchange = scram.as_ref().ok_or_else(|| Failure::Broken("PostgreSQL ended a SASL exchange that had not begun".into()))?;
                        exchange.verify(&String::from_utf8_lossy(b.rest())).map_err(Failure::Broken)?;
                    }
                    5 => return Err(Failure::Broken("PostgreSQL asks for MD5 authentication; ironwork speaks SCRAM-SHA-256".into())),
                    other => return Err(Failure::Broken(format!("PostgreSQL asks for authentication method {other}, which ironwork does not speak"))),
                },
                b'Z' => {
                    self.status = body.first().copied().unwrap_or(b'I');
                    return Ok(());
                }
                b'E' => return Err(refusal(&body)),
                _ => self.note(kind, &body),
            }
        }
    }

    fn send(&mut self, kind: u8, body: &[u8]) -> Result<(), Failure> {
        self.writer.write_all(&[kind])?;
        self.writer.write_all(&(body.len() as i32 + 4).to_be_bytes())?;
        self.writer.write_all(body)?;
        Ok(())
    }

    fn receive(&mut self) -> Result<(u8, Vec<u8>), Failure> {
        let mut head = [0u8; 5];
        self.reader.read_exact(&mut head)?;
        let len = i32::from_be_bytes([head[1], head[2], head[3], head[4]]);
        let mut body = vec![0u8; (len.max(4) - 4) as usize];
        self.reader.read_exact(&mut body)?;
        Ok((head[0], body))
    }

    /// Messages that may arrive at any time: ParameterStatus, notices and notifications.
    fn note(&mut self, kind: u8, body: &[u8]) {
        if kind == b'S' {
            let mut b = Body { bytes: body, at: 0 };
            if let (Ok(name), Ok(value)) = (b.cstr(), b.cstr())
                && name == "server_version"
            {
                self.server_version = value;
            }
        }
    }

    /// Reads to ReadyForQuery, handing each message to `each`, and fails with the first refusal.
    fn until_ready(&mut self, mut each: impl FnMut(u8, &[u8]) -> Result<(), Failure>) -> Result<(), Failure> {
        let mut refused = None;
        loop {
            let (kind, body) = self.receive()?;
            match kind {
                b'Z' => {
                    self.status = body.first().copied().unwrap_or(b'I');
                    return refused.map_or(Ok(()), Err);
                }
                b'E' => refused = refused.or(Some(refusal(&body))),
                b'S' | b'N' | b'A' => self.note(kind, &body),
                _ if refused.is_none() => each(kind, &body)?,
                _ => {}
            }
        }
    }

    pub fn simple(&mut self, sql: &str) -> Result<(), Failure> {
        let mut body = Vec::new();
        cstr(&mut body, sql);
        self.send(b'Q', &body)?;
        self.writer.flush()?;
        self.until_ready(|_, _| Ok(()))
    }

    pub fn prepare(&mut self, name: &str, sql: &str) -> Result<Described, Failure> {
        let mut parse = Vec::new();
        cstr(&mut parse, name);
        cstr(&mut parse, sql);
        parse.extend_from_slice(&0i16.to_be_bytes());
        self.send(b'P', &parse)?;
        let mut describe = vec![b'S'];
        cstr(&mut describe, name);
        self.send(b'D', &describe)?;
        self.send(b'S', &[])?;
        self.writer.flush()?;
        let mut described = Described::default();
        self.until_ready(|kind, body| {
            let mut b = Body { bytes: body, at: 0 };
            match kind {
                b't' => {
                    let n = b.i16()?;
                    described.parameters = (0..n).map(|_| b.i32().map(|o| o as u32)).collect::<Result<_, _>>()?;
                }
                b'T' => {
                    let n = b.i16()?;
                    for _ in 0..n {
                        b.cstr()?;
                        b.take(6)?;
                        described.columns.push(b.i32()? as u32);
                        b.take(8)?;
                    }
                }
                _ => {}
            }
            Ok(())
        })?;
        Ok(described)
    }

    /// Binds text parameters (None is NULL) to a prepared statement and executes it, returning at
    /// most `max_rows` rows (0 for all).
    pub fn execute(&mut self, name: &str, parameters: &[Option<String>], max_rows: i32) -> Result<Executed, Failure> {
        let mut bind = Vec::new();
        cstr(&mut bind, "");
        cstr(&mut bind, name);
        bind.extend_from_slice(&0i16.to_be_bytes());
        bind.extend_from_slice(&(parameters.len() as i16).to_be_bytes());
        for p in parameters {
            match p {
                None => bind.extend_from_slice(&(-1i32).to_be_bytes()),
                Some(text) => {
                    bind.extend_from_slice(&(text.len() as i32).to_be_bytes());
                    bind.extend_from_slice(text.as_bytes());
                }
            }
        }
        bind.extend_from_slice(&0i16.to_be_bytes());
        self.send(b'B', &bind)?;
        let mut execute = Vec::new();
        cstr(&mut execute, "");
        execute.extend_from_slice(&max_rows.to_be_bytes());
        self.send(b'E', &execute)?;
        self.send(b'S', &[])?;
        self.writer.flush()?;
        let mut executed = Executed::default();
        self.until_ready(|kind, body| {
            let mut b = Body { bytes: body, at: 0 };
            match kind {
                b'D' => {
                    let n = b.i16()?;
                    let mut row = Vec::new();
                    for _ in 0..n {
                        let len = b.i32()?;
                        row.push(if len < 0 { None } else { Some(String::from_utf8_lossy(b.take(len as usize)?).into_owned()) });
                    }
                    executed.rows.push(row);
                }
                b'C' => executed.tag = b.cstr()?,
                _ => {}
            }
            Ok(())
        })?;
        Ok(executed)
    }
}

#[cfg(unix)]
fn unix_socket(target: &Target) -> std::io::Result<(Box<dyn Read>, Box<dyn Write>)> {
    let stream = std::os::unix::net::UnixStream::connect(format!("{}/.s.PGSQL.{}", target.host, target.port))?;
    Ok((Box::new(stream.try_clone()?), Box::new(stream)))
}

#[cfg(not(unix))]
fn unix_socket(_: &Target) -> std::io::Result<(Box<dyn Read>, Box<dyn Write>)> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "Unix sockets need a Unix system"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        let t = Target::parse("postgres://ironwork:p%40ss@db.example:6543/payroll").unwrap();
        assert_eq!(t, Target { host: "db.example".into(), port: 6543, user: "ironwork".into(), password: Some("p@ss".into()), database: "payroll".into() });
        let t = Target::parse("postgres://me@/payroll?host=/var/run/postgresql").unwrap();
        assert_eq!((t.host.as_str(), t.port, t.database.as_str()), ("/var/run/postgresql", 5432, "payroll"));
        assert!(Target::parse("mysql://x").is_err());
        assert!(Target::parse("postgres://me@h:port/db").is_err());
    }
}
