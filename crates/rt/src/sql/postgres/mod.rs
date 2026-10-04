//! The PostgreSQL backend. Each call runs under a savepoint inside the unit of work, because Db2
//! undoes a failed statement and keeps the rest of the unit, where PostgreSQL would abort it all.

mod dialect;
mod scram;
mod wire;

use super::{Abandoned, Answer, Call, Column, Database, Outcome, Value};
use std::collections::HashMap;
pub use wire::{Stream, Tls};
use wire::{Connection, Described, Failure, Target};

pub struct Postgres {
    conn: Connection,
    /// Prepared statements by their PostgreSQL text: the statement's name and its types.
    prepared: HashMap<String, (String, Described)>,
    source: String,
}

/// The statements a connection keeps prepared. Past it they are all deallocated and each prepared
/// again when next run, so dynamic statements that carry their values in their text do not grow
/// the server's memory without end.
const PREPARED_LIMIT: usize = 1024;

fn abandon(f: Failure) -> Abandoned {
    let message = match f {
        Failure::Refused { state, message } => format!("PostgreSQL refused a request of ironwork's own ({state}): {message}"),
        Failure::Broken(m) => m,
    };
    Abandoned { code: "SQL", message }
}

impl Postgres {
    /// Runs `script` outside any unit of work, to prepare a server for a test.
    #[doc(hidden)]
    pub fn load_script(&mut self, script: &str) -> Result<(), String> {
        self.conn.simple(script).map_err(|f| abandon(f).message)
    }

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
        self.guarded(call, |pg, sql| pg.statement(sql, call, max_rows))
    }

    /// `work` on the call's PostgreSQL text under a savepoint, its refusal mapped to a Db2 SQLCODE.
    fn guarded(&mut self, call: &Call, work: impl FnOnce(&mut Self, &str) -> Result<Outcome, Failure>) -> Answer {
        let sql = dialect::rewrite(call.text, call.cursor);
        if self.conn.status == b'I' {
            self.conn.simple("BEGIN").map_err(abandon)?;
        }
        self.conn.simple("SAVEPOINT ironwork").map_err(abandon)?;
        match work(self, &sql) {
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

    /// The PostgreSQL statement for `sql`, prepared on first use and kept while fewer than
    /// [`PREPARED_LIMIT`] are.
    fn parsed(&mut self, sql: &str) -> Result<(String, Described), Failure> {
        if let Some(p) = self.prepared.get(sql) {
            return Ok(p.clone());
        }
        if self.prepared.len() >= PREPARED_LIMIT {
            self.conn.simple("DEALLOCATE ALL")?;
            self.prepared.clear();
        }
        let name = format!("ironwork{}", self.prepared.len() + 1);
        let described = self.conn.prepare(&name, sql)?;
        self.prepared.insert(sql.to_owned(), (name.clone(), described.clone()));
        Ok((name, described))
    }

    fn statement(&mut self, sql: &str, call: &Call, max_rows: i32) -> Result<Outcome, Failure> {
        let (name, described) = self.parsed(sql)?;
        if described.parameters.len() != call.inputs.len() {
            return Err(Failure::Broken(format!("PostgreSQL reads {} parameters in {sql}, and the program sends {}", described.parameters.len(), call.inputs.len())));
        }
        let parameters: Vec<Option<String>> = call.inputs.iter().zip(&described.parameters).map(|(v, &oid)| dialect::text(v, oid)).collect();
        let executed = self.conn.execute(&name, &parameters, max_rows)?;
        let mut rows = Vec::new();
        for row in &executed.rows {
            let values = row.iter().zip(&described.columns).map(|(column, field)| column.as_deref().map_or(Ok(Value::Null), |text| dialect::value(field.oid, text)));
            rows.push(values.collect::<Result<Vec<_>, _>>().map_err(Failure::Broken)?);
        }
        let changed = matches!(executed.tag.split(' ').next(), Some("INSERT" | "UPDATE" | "DELETE" | "MERGE"));
        let affected = if changed { executed.tag.rsplit(' ').next().and_then(|n| n.parse().ok()).unwrap_or(0) } else { 0 };
        Ok(Outcome { affected, rows, ..Outcome::ok() })
    }

    /// Each result column with its Db2 type, its name upper-cased and NULL allowed unless it is a
    /// table's column declared NOT NULL (assumption C403).
    fn columns(&mut self, described: &Described) -> Result<Vec<Column>, Failure> {
        let from_tables: Vec<String> = described.columns.iter().filter(|f| f.table != 0).map(|f| format!("({}, {})", f.table, f.attnum)).collect();
        let mut not_null = Vec::new();
        if !from_tables.is_empty() {
            let sql = format!("SELECT attrelid, attnum FROM pg_attribute WHERE attnotnull AND (attrelid, attnum) IN ({})", from_tables.join(", "));
            let (name, _) = self.parsed(&sql)?;
            for row in self.conn.execute(&name, &[], 0)?.rows {
                if let [Some(table), Some(attnum)] = row.as_slice() {
                    not_null.push((table.clone(), attnum.clone()));
                }
            }
        }
        Ok(described
            .columns
            .iter()
            .map(|f| Column {
                name: f.name.to_uppercase(),
                ty: dialect::column_type(f.oid, f.typmod),
                nullable: !not_null.contains(&(f.table.to_string(), f.attnum.to_string())),
            })
            .collect())
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
    /// PostgreSQL parses the statement string as Db2's PREPARE does, so its errors come at PREPARE,
    /// and describes its result columns.
    fn prepare(&mut self, call: &Call) -> Answer {
        self.guarded(call, |pg, sql| {
            let (_, described) = pg.parsed(sql)?;
            Ok(Outcome { columns: pg.columns(&described)?, ..Outcome::ok() })
        })
    }
    fn open(&mut self, call: &Call) -> Answer {
        self.run(call, 0)
    }
    fn fetch(&mut self, call: &Call) -> Answer {
        self.run(call, 0)
    }
    fn fetch_rows(&mut self, call: &Call, rows: u32) -> Answer {
        let text = format!("FETCH FORWARD {rows} FROM {}", call.cursor.unwrap_or_default());
        self.run(&Call { text: &text, inputs: &[], ..*call }, 0)
    }
    /// ATOMIC inserts every row under the call's one savepoint, so a failure undoes them all; NOT
    /// ATOMIC gives each row its own, as Db2 keeps the rows that went in
    /// ([`numeric::assumptions::NOT_ATOMIC_SUMMARY`]).
    fn insert_rows(&mut self, call: &Call, rows: &[Vec<Value>], atomic: bool) -> Answer {
        if atomic {
            return self.guarded(call, |pg, sql| {
                let mut affected = 0;
                for row in rows {
                    affected += pg.statement(sql, &Call { inputs: row, ..*call }, 0)?.affected;
                }
                Ok(Outcome { affected, ..Outcome::ok() })
            });
        }
        let (mut inserted, mut failed) = (0, 0);
        for row in rows {
            let one = Call { inputs: row, ..*call };
            let outcome = self.guarded(&one, |pg, sql| pg.statement(sql, &one, 0))?;
            if outcome.sqlcode < 0 {
                failed += 1;
            } else {
                inserted += outcome.affected;
            }
        }
        Ok(match (failed, inserted) {
            (0, _) => Outcome { affected: inserted, ..Outcome::ok() },
            (_, 0) => Outcome::error(-254, "22530"),
            _ => Outcome { affected: inserted, ..Outcome::error(-253, "22529") },
        })
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
/// without it this test passes without running. The recorded run of a whole program is in exec.
#[cfg(test)]
mod tests {
    use super::*;

    fn url() -> Option<String> {
        std::env::var("IRONWORK_PG_URL").ok()
    }

    #[test]
    fn a_wrong_password_is_refused() {
        let Some(url) = url() else { return };
        let Some((head, tail)) = url.split_once('@') else { return };
        let wrong = format!("{}:wrong@{tail}", head.rsplit_once(':').map_or(head, |(user, _)| user));
        let refused = Postgres::connect(&wrong, None).err().expect("refused");
        assert!(refused.contains("28P01"), "{refused}");
    }

    #[test]
    fn distinct_statements_do_not_pile_up_on_the_server() {
        let Some(url) = url() else { return };
        let mut pg = Postgres::connect(&url, None).expect("connects");
        fn call(text: &str) -> Call<'_> {
            Call { program: "P", ordinal: 1, verb: "SELECT", cursor: None, text, inputs: &[] }
        }
        for n in 0..PREPARED_LIMIT as i64 + 8 {
            let text = format!("VALUES {n}");
            assert_eq!(pg.execute(&call(&text)).expect("answers").rows, [[Value::Int(n)]]);
        }
        let rows = pg.execute(&call("SELECT COUNT(*) FROM PG_PREPARED_STATEMENTS")).expect("answers").rows;
        assert!(matches!(rows.as_slice(), [row] if matches!(row.as_slice(), [Value::Int(k)] if *k as usize <= PREPARED_LIMIT)), "{rows:?}");
        assert!(pg.prepared.len() <= PREPARED_LIMIT);
    }
}
