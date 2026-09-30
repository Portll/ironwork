//! The PostgreSQL backend. Each call runs under a savepoint inside the unit of work, because Db2
//! undoes a failed statement and keeps the rest of the unit, where PostgreSQL would abort it all.

mod dialect;
mod scram;
mod wire;

use super::{Abandoned, Answer, Call, Database, Outcome, Value};
use std::collections::HashMap;
pub use wire::{Stream, Tls};
use wire::{Connection, Described, Failure, Target};

pub struct Postgres {
    conn: Connection,
    /// Prepared statements by their PostgreSQL text: the statement's name and its types.
    prepared: HashMap<String, (String, Described)>,
    source: String,
}

fn abandon(f: Failure) -> Abandoned {
    let message = match f {
        Failure::Refused { state, message } => format!("PostgreSQL refused a request of ironwork's own ({state}): {message}"),
        Failure::Broken(m) => m,
    };
    Abandoned { code: "SQL", message }
}

impl Postgres {
    /// `tls` is None in ironwork's own build, which then connects without TLS.
    pub fn connect(url: &str, tls: Option<&dyn Tls>) -> Result<Self, String> {
        let target = Target::parse(url)?;
        let conn = Connection::open(&target, tls)?;
        let over = if conn.encrypted { " over TLS" } else { "" };
        let source = format!("PostgreSQL {} at {}:{}/{}{over}", conn.server_version, target.host, target.port, target.database);
        Ok(Self { conn, prepared: HashMap::new(), source })
    }

    /// The server and database, as a recording's header names them.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Runs a call as one statement, returning at most `max_rows` rows (0 for all).
    fn run(&mut self, call: &Call, max_rows: i32) -> Answer {
        let sql = dialect::rewrite(call.text, call.cursor);
        if self.conn.status == b'I' {
            self.conn.simple("BEGIN").map_err(abandon)?;
        }
        self.conn.simple("SAVEPOINT ironwork").map_err(abandon)?;
        match self.statement(&sql, call, max_rows) {
            Ok(outcome) => {
                self.conn.simple("RELEASE SAVEPOINT ironwork").map_err(abandon)?;
                Ok(outcome)
            }
            Err(Failure::Refused { state, message }) => {
                let db2 = dialect::db2_error(&state);
                // -911 rolls back the whole unit of work, as Db2 does after a deadlock or timeout.
                let undo = if db2.is_some_and(|(code, _)| code == -911) { "ROLLBACK" } else { "ROLLBACK TO SAVEPOINT ironwork; RELEASE SAVEPOINT ironwork" };
                self.conn.simple(undo).map_err(abandon)?;
                match db2 {
                    Some((code, state)) => Ok(Outcome::error(code, state)),
                    None => Err(Abandoned {
                        code: "SQL",
                        message: format!("PostgreSQL refused {} {sql} with SQLSTATE {state}, which ironwork's table gives no Db2 SQLCODE: {message}", call.verb),
                    }),
                }
            }
            Err(broken) => Err(abandon(broken)),
        }
    }

    fn statement(&mut self, sql: &str, call: &Call, max_rows: i32) -> Result<Outcome, Failure> {
        let (name, described) = match self.prepared.get(sql) {
            Some(p) => p.clone(),
            None => {
                let name = format!("ironwork{}", self.prepared.len() + 1);
                let described = self.conn.prepare(&name, sql)?;
                self.prepared.insert(sql.to_owned(), (name.clone(), described.clone()));
                (name, described)
            }
        };
        if described.parameters.len() != call.inputs.len() {
            return Err(Failure::Broken(format!("PostgreSQL reads {} parameters in {sql}, and the program sends {}", described.parameters.len(), call.inputs.len())));
        }
        let parameters: Vec<Option<String>> = call.inputs.iter().zip(&described.parameters).map(|(v, &oid)| dialect::text(v, oid)).collect();
        let executed = self.conn.execute(&name, &parameters, max_rows)?;
        let mut rows = Vec::new();
        for row in &executed.rows {
            let values = row.iter().zip(&described.columns).map(|(column, &oid)| column.as_deref().map_or(Ok(Value::Null), |text| dialect::value(oid, text)));
            rows.push(values.collect::<Result<Vec<_>, _>>().map_err(Failure::Broken)?);
        }
        let changed = matches!(executed.tag.split(' ').next(), Some("INSERT" | "UPDATE" | "DELETE" | "MERGE"));
        let affected = if changed { executed.tag.rsplit(' ').next().and_then(|n| n.parse().ok()).unwrap_or(0) } else { 0 };
        Ok(Outcome { affected, rows, ..Outcome::ok() })
    }

    fn end(&mut self, verb: &str) -> Answer {
        if self.conn.status == b'I' {
            return Ok(Outcome::ok());
        }
        match self.conn.simple(verb) {
            Ok(()) => Ok(Outcome::ok()),
            Err(Failure::Refused { state, message }) => match dialect::db2_error(&state) {
                Some((code, state)) => Ok(Outcome::error(code, state)),
                None => Err(Abandoned { code: "SQL", message: format!("PostgreSQL refused {verb} with SQLSTATE {state}: {message}") }),
            },
            Err(broken) => Err(abandon(broken)),
        }
    }
}

impl Database for Postgres {
    /// Two rows are enough to tell a SELECT INTO's one row from its too many.
    fn execute(&mut self, call: &Call) -> Answer {
        self.run(call, 2)
    }
    fn open(&mut self, call: &Call) -> Answer {
        self.run(call, 0)
    }
    fn fetch(&mut self, call: &Call) -> Answer {
        self.run(call, 0)
    }
    fn close(&mut self, call: &Call) -> Answer {
        self.run(call, 0)
    }
    fn commit(&mut self, _: &Call) -> Answer {
        self.end("COMMIT")
    }
    fn rollback(&mut self, _: &Call) -> Answer {
        self.end("ROLLBACK")
    }
    fn close_all(&mut self) -> Result<(), Abandoned> {
        self.conn.simple("CLOSE ALL").map_err(abandon)
    }
}

/// Against a live server named by IRONWORK_PG_URL, which `tools/pg-test.sh` starts in a container;
/// without it these tests pass without running.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql::{Recorder, Replay};
    use std::cell::RefCell;
    use std::rc::Rc;

    fn url() -> Option<String> {
        std::env::var("IRONWORK_PG_URL").ok()
    }

    const SCHEMA: &str = "DROP TABLE IF EXISTS emp; \
        CREATE TABLE emp (id integer PRIMARY KEY, name char(10) NOT NULL, amt numeric(7,2), stamp timestamp); \
        INSERT INTO emp VALUES (1, 'ADAMS', 100.50, '2020-01-02 03:04:05.5'), (2, 'BAKER', NULL, NULL), (3, 'CLARK', 30, NULL); \
        CREATE SCHEMA IF NOT EXISTS sysibm; \
        CREATE OR REPLACE VIEW sysibm.sysdummy1 AS SELECT 'Y'::char(1) AS ibmreqd;";

    const PROGRAM: &str = concat!(
        "       IDENTIFICATION DIVISION.\n",
        "       PROGRAM-ID. PGQ5.\n",
        "       DATA DIVISION.\n",
        "       WORKING-STORAGE SECTION.\n",
        "           EXEC SQL INCLUDE SQLCA END-EXEC.\n",
        "       01 WS-ID    PIC S9(9) COMP VALUE 2.\n",
        "       01 WS-NAME  PIC X(10).\n",
        "       01 WS-AMT   PIC S9(5)V99 COMP-3.\n",
        "       01 WS-IND   PIC S9(4) COMP.\n",
        "       01 WS-STAMP PIC X(26).\n",
        "       01 WS-Y     PIC X.\n",
        "       01 WS-COUNT PIC S9(9) COMP.\n",
        "       01 E-AMT    PIC -9(5).99.\n",
        "       01 E-CODE   PIC -9(3).\n",
        "       01 E-NUM    PIC -9(3).\n",
        "           EXEC SQL DECLARE C1 CURSOR FOR\n",
        "                SELECT NAME, AMT FROM EMP WHERE ID >= :WS-ID\n",
        "                ORDER BY ID\n",
        "           END-EXEC.\n",
        "       PROCEDURE DIVISION.\n",
        "           MOVE 1 TO WS-ID.\n",
        "           EXEC SQL OPEN C1 END-EXEC.\n",
        "           PERFORM UNTIL SQLCODE NOT = 0\n",
        "               EXEC SQL FETCH C1 INTO :WS-NAME, :WS-AMT:WS-IND\n",
        "               END-EXEC\n",
        "               IF SQLCODE = 0\n",
        "                   MOVE WS-AMT TO E-AMT\n",
        "                   MOVE WS-IND TO E-NUM\n",
        "                   DISPLAY WS-NAME E-AMT E-NUM\n",
        "               END-IF\n",
        "           END-PERFORM.\n",
        "           EXEC SQL CLOSE C1 END-EXEC.\n",
        "           EXEC SQL SELECT STAMP INTO :WS-STAMP FROM EMP\n",
        "                    WHERE ID = 1 END-EXEC.\n",
        "           DISPLAY WS-STAMP.\n",
        "           EXEC SQL INSERT INTO EMP (ID, NAME) VALUES (1, 'DUP')\n",
        "           END-EXEC.\n",
        "           MOVE SQLCODE TO E-CODE.\n",
        "           DISPLAY 'INSERT ' E-CODE ' ' SQLSTATE.\n",
        "           EXEC SQL UPDATE EMP SET AMT = AMT + 1 WHERE ID = 99\n",
        "           END-EXEC.\n",
        "           MOVE SQLCODE TO E-CODE.\n",
        "           DISPLAY 'UPDATE ' E-CODE.\n",
        "           EXEC SQL SELECT COUNT(*) INTO :WS-COUNT FROM EMP\n",
        "                    WHERE AMT < 200 WITH UR END-EXEC.\n",
        "           MOVE WS-COUNT TO E-NUM.\n",
        "           DISPLAY 'COUNT ' E-NUM.\n",
        "           EXEC SQL SELECT IBMREQD INTO :WS-Y\n",
        "                    FROM SYSIBM.SYSDUMMY1 END-EXEC.\n",
        "           DISPLAY 'DUMMY ' WS-Y.\n",
        "           EXEC SQL SELECT NAME INTO :WS-NAME FROM EMP END-EXEC.\n",
        "           MOVE SQLCODE TO E-CODE.\n",
        "           DISPLAY 'MANY ' E-CODE.\n",
        "           GOBACK.\n",
    );

    const EXPECTED: &str = concat!(
        "ADAMS      00100.50 000\n",
        "BAKER      00100.50-001\n",
        "CLARK      00030.00 000\n",
        "2020-01-02-03.04.05.500000\n",
        "INSERT -803 23505\n",
        "UPDATE  100\n",
        "COUNT  002\n",
        "DUMMY Y\n",
        "MANY -811\n",
    );

    struct Sink(Rc<RefCell<Vec<u8>>>);

    impl std::io::Write for Sink {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.borrow_mut().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn run(mut database: Box<dyn Database + '_>) -> Result<String, String> {
        let compiled = crate::compile(syntax::parse(PROGRAM).expect("parses"), &[]).expect("compiles");
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let ran = compiled.execute_with(crate::unit::Library::default(), crate::files::Dds::default(), None, crate::unit::Clock::System, Some(database.as_mut()), &mut out, &mut err);
        ran.map(|_| String::from_utf8(out).expect("DISPLAY writes text")).map_err(|a| format!("{}: {}", a.code, a.message))
    }

    #[test]
    fn a_recorded_postgresql_run_replays_identically() {
        let Some(url) = url() else { return };
        let mut setup = Postgres::connect(&url, None).expect("connects");
        setup.conn.simple(SCHEMA).expect("the schema loads");
        let postgres = Postgres::connect(&url, None).expect("connects");
        let source = postgres.source().to_owned();
        assert!(source.starts_with("PostgreSQL 14"), "{source}");
        let recording = Rc::new(RefCell::new(Vec::new()));
        let recorder = Recorder::new(Box::new(postgres), Box::new(Sink(recording.clone())), &source).expect("records");
        assert_eq!(run(Box::new(recorder)).as_deref(), Ok(EXPECTED));
        let text = String::from_utf8(recording.borrow().clone()).expect("a recording is text");
        let replayed = run(Box::new(Replay::parse(&text, false).expect("the recording parses")));
        assert_eq!(replayed.as_deref(), Ok(EXPECTED), "{text}");
    }

    #[test]
    fn a_wrong_password_is_refused() {
        let Some(url) = url() else { return };
        let Some((head, tail)) = url.split_once('@') else { return };
        let wrong = format!("{}:wrong@{tail}", head.rsplit_once(':').map_or(head, |(user, _)| user));
        let refused = Postgres::connect(&wrong, None).err().expect("refused");
        assert!(refused.contains("28P01"), "{refused}");
    }
}
