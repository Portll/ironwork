//! `ironwork-ddl FILE.sql`: writes the Db2 for z/OS DDL in FILE as PostgreSQL DDL for the SQL
//! backend's tests: tables, keys and indexes, Db2 types mapped as the conversion layer reads them,
//! NOT NULL WITH DEFAULT given its Db2 default, and every clause PostgreSQL has no use for (IN,
//! CCSID, BUFFERPOOL, ...) kept as a comment.
//!
//! `cargo run -p ironwork-ddl -- FILE.sql`

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [file] = args.as_slice() else {
        eprintln!("usage: ironwork-ddl FILE.sql");
        return ExitCode::from(2);
    };
    match ironwork_ddl::convert_file(std::path::Path::new(file)) {
        Ok(sql) => {
            print!("{sql}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("ironwork-ddl: {file}: {e}");
            ExitCode::from(2)
        }
    }
}
