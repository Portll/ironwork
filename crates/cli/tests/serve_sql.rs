//! `ironwork cics --serve` with a database: a TN3270 client drives a two-task conversation whose
//! tasks run EXEC SQL, each its own unit of work on the one database the server holds. With
//! IRONWORK_PG_URL set (tools/pg-test.sh), the conversation is also recorded against PostgreSQL and
//! replayed from that recording.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
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

/// A held cursor left open at the end of the first task, and opened again by the second.
const PROGRAM: &str = concat!(
    "       IDENTIFICATION DIVISION.\n",
    "       PROGRAM-ID. SRVQ.\n",
    "       DATA DIVISION.\n",
    "       WORKING-STORAGE SECTION.\n",
    "           EXEC SQL INCLUDE SQLCA END-EXEC.\n",
    "       01 WS-NAME  PIC X(8).\n",
    "       01 WS-CODE  PIC -9(3).\n",
    "       01 WS-TURN  PIC X(4) VALUE 'NEXT'.\n",
    "       PROCEDURE DIVISION.\n",
    "           EXEC SQL DECLARE C1 CURSOR WITH HOLD FOR\n",
    "                    SELECT NAME FROM SERVED ORDER BY ID END-EXEC.\n",
    "           EXEC SQL OPEN C1 END-EXEC.\n",
    "           EXEC SQL FETCH C1 INTO :WS-NAME END-EXEC.\n",
    "           MOVE SQLCODE TO WS-CODE.\n",
    "           IF EIBCALEN = 0\n",
    "               DISPLAY 'FIRST ' WS-NAME WS-CODE\n",
    "               EXEC CICS RETURN TRANSID('T1') COMMAREA(WS-TURN)\n",
    "                    LENGTH(4) END-EXEC\n",
    "           END-IF.\n",
    "           DISPLAY 'SECOND ' WS-NAME WS-CODE.\n",
    "           EXEC CICS RETURN END-EXEC.\n",
);

const SHOWN: [&str; 2] = ["FIRST ADAMS    000", "SECOND ADAMS    000"];

/// The server process, killed when dropped so a failing test leaves nothing listening.
struct Server {
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

fn serve(program: &Path, database: &[&str]) -> Server {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ironwork"))
        .args(["cics", program.to_str().expect("a UTF-8 path"), "--transid", "T1", "--serve", "127.0.0.1:0"])
        .args(database)
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
fn converse(server: &Server) -> (Vec<String>, usize) {
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
        tasks += usize::from(line.ends_with(" runs SRVQ"));
        if line.ends_with(" disconnected") {
            return (vec![first, second], tasks);
        }
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ironwork-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("srvq.cbl"), PROGRAM).unwrap();
    dir
}

#[test]
fn a_served_conversation_runs_exec_sql_from_a_recording() {
    let hash = syntax::sql::fingerprint;
    let (declare, fetch, commit) = (hash("DECLARE C1 CURSOR WITH HOLD FOR SELECT NAME FROM SERVED ORDER BY ID"), hash("FETCH C1"), hash("COMMIT"));
    let mut recording = String::from("# ironwork sql recording 1\n");
    for task in 0..2 {
        let n = task * 3;
        recording += &format!("@ {} SRVQ:2:{declare:08x} OPEN C1\n< 0 00000 rows=0\n", n + 1);
        recording += &format!("@ {} SRVQ:3:{fetch:08x} FETCH C1\n< 0 00000 rows=0\n= char:\"ADAMS\"\n", n + 2);
        recording += &format!("@ {} SRVQ:0:{commit:08x} COMMIT\n< 0 00000 rows=0\n", n + 3);
    }
    let dir = scratch("serve-replay");
    std::fs::write(dir.join("served.sql"), recording).unwrap();
    let server = serve(&dir.join("srvq.cbl"), &["--sql-replay", dir.join("served.sql").to_str().unwrap()]);
    assert_eq!(converse(&server), (SHOWN.map(String::from).to_vec(), 2));
    drop(server);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_served_conversation_recorded_against_postgresql_replays() {
    use exec::sql::{Call, Database};
    let Ok(url) = std::env::var("IRONWORK_PG_URL") else { return };
    let mut postgres = exec::sql::Postgres::connect(&url, None).expect("PostgreSQL answers");
    for text in ["DROP TABLE IF EXISTS served", "CREATE TABLE served (id integer PRIMARY KEY, name char(8) NOT NULL)", "INSERT INTO served VALUES (1, 'ADAMS'), (2, 'BAKER')"] {
        let call = Call { program: "SETUP", ordinal: 1, verb: "SETUP", cursor: None, text, inputs: &[] };
        assert_eq!(postgres.execute(&call).expect("the setup runs").sqlcode, 0, "{text}");
    }
    let end = Call { program: "SETUP", ordinal: 0, verb: "COMMIT", cursor: None, text: "COMMIT", inputs: &[] };
    postgres.commit(&end).expect("the setup commits");
    drop(postgres);

    let dir = scratch("serve-pg");
    let recording = dir.join("served.sql");
    let live = serve(&dir.join("srvq.cbl"), &["--sql-db", &url, "--sql-record", recording.to_str().unwrap()]);
    assert_eq!(converse(&live), (SHOWN.map(String::from).to_vec(), 2));
    drop(live);
    let recorded = std::fs::read_to_string(&recording).unwrap();
    assert_eq!(recorded.lines().filter(|l| l.starts_with('@')).count(), 6, "{recorded}");
    let replayed = serve(&dir.join("srvq.cbl"), &["--sql-replay", recording.to_str().unwrap()]);
    assert_eq!(converse(&replayed), (SHOWN.map(String::from).to_vec(), 2));
    drop(replayed);
    std::fs::remove_dir_all(dir).unwrap();
}
