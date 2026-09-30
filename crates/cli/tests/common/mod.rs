//! A TN3270 client for `ironwork cics --serve`: it starts the server, negotiates as an IBM-3278-2,
//! presses ENTER once, and collects what the tasks displayed.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

const IAC: u8 = 255;
const DO: u8 = 253;
const WILL: u8 = 251;
const SB: u8 = 250;
const SE: u8 = 240;
const EOR_COMMAND: u8 = 239;
const BINARY: u8 = 0;
const TERMINAL_TYPE: u8 = 24;
const EOR: u8 = 25;
const ENTER: u8 = 0x7D;

const WAIT: Duration = Duration::from_secs(60);

/// The server process, killed when dropped so a failing test leaves nothing listening.
pub struct Server {
    child: Child,
    address: String,
    stdout: Receiver<String>,
    stderr: Receiver<String>,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn lines(stream: impl Read + Send + 'static) -> Receiver<String> {
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            if send.send(line.trim_end_matches('\r').to_owned()).is_err() {
                return;
            }
        }
    });
    receive
}

pub fn serve(program: &Path, args: &[&str]) -> Server {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ironwork"))
        .args(["cics", program.to_str().expect("a UTF-8 path"), "--serve", "127.0.0.1:0"])
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the ironwork binary starts");
    let (stdout, stderr) = (lines(child.stdout.take().expect("piped")), lines(child.stderr.take().expect("piped")));
    let mut server = Server { child, address: String::new(), stdout, stderr };
    while server.address.is_empty() {
        let line = server.stderr.recv_timeout(WAIT).expect("the server reports its address");
        if let Some(address) = line.strip_prefix("ironwork: serving TN3270 on ") {
            server.address = address.to_owned();
        }
    }
    server
}

fn read_exact(stream: &mut TcpStream, n: usize) -> Vec<u8> {
    let mut bytes = vec![0; n];
    stream.read_exact(&mut bytes).expect("the server negotiates");
    bytes
}

/// Connects as an IBM-3278-2, presses ENTER once the first task has returned TRANSID, disconnects
/// once the second has ended, and returns the lines the tasks displayed and how many tasks ran.
pub fn converse(server: &Server) -> (Vec<String>, usize) {
    let mut stream = TcpStream::connect(&server.address).expect("the server accepts");
    stream.set_read_timeout(Some(WAIT)).unwrap();
    assert_eq!(read_exact(&mut stream, 3), [IAC, DO, TERMINAL_TYPE]);
    stream.write_all(&[IAC, WILL, TERMINAL_TYPE]).unwrap();
    assert_eq!(read_exact(&mut stream, 6), [IAC, SB, TERMINAL_TYPE, 1, IAC, SE]);
    let mut reply = vec![IAC, SB, TERMINAL_TYPE, 0];
    reply.extend_from_slice(b"IBM-3278-2");
    reply.extend_from_slice(&[IAC, SE]);
    stream.write_all(&reply).unwrap();
    assert_eq!(read_exact(&mut stream, 12).len(), 12);
    stream.write_all(&[IAC, WILL, BINARY, IAC, DO, BINARY, IAC, WILL, EOR, IAC, DO, EOR]).unwrap();
    let first = server.stdout.recv_timeout(WAIT).expect("the first task displays");
    stream.write_all(&[ENTER, 0x40, 0x40, IAC, EOR_COMMAND]).unwrap();
    let second = server.stdout.recv_timeout(WAIT).expect("the second task displays");
    drop(stream);
    let mut tasks = 0;
    loop {
        let line = server.stderr.recv_timeout(WAIT).expect("the server ends the connection");
        eprintln!("{line}");
        tasks += usize::from(line.contains(" runs "));
        if line.ends_with(" disconnected") {
            return (vec![first, second], tasks);
        }
    }
}
