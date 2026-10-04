//! The SQL runtime is `rt::sql`, with the host type of a declared item from `compile::sql`.

pub use compile::sql::{host_array, host_type};
pub use rt::sql::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Execute;
    use crate::layout::Resolved;

    fn types(data: &str, names: &[&str]) -> Vec<Result<HostType, String>> {
        let source = format!(
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n{data}       PROCEDURE DIVISION.\n           GOBACK.\n"
        );
        let compiled = crate::compile(syntax::parse(&source).expect("parses"), &[]).expect("compiles");
        let layout = &compiled.layout;
        names
            .iter()
            .map(|n| match layout.resolve(n, &[], syntax::Pos::default()) {
                Ok(Resolved::Item(i)) => host_type(layout, i),
                other => panic!("{n}: {other:?}"),
            })
            .collect()
    }

    #[test]
    fn cobol_declarations_as_db2_reads_them() {
        let got = types(
            concat!(
                "       01 H PIC S9(4) COMP.\n",
                "       01 I PIC S9(9) COMP-5.\n",
                "       01 B PIC S9(18) BINARY.\n",
                "       01 D PIC S9(5)V99 COMP-3.\n",
                "       01 Z PIC S9(5)V99 SIGN LEADING SEPARATE.\n",
                "       01 C PIC X(10).\n",
                "       01 F COMP-2.\n",
                "       01 V.\n",
                "          49 V-LEN PIC S9(4) COMP.\n",
                "          49 V-TEXT PIC X(30).\n",
                "       01 S.\n",
                "          05 S-ID PIC S9(9) COMP.\n",
                "          05 S-NAME PIC X(20).\n",
            ),
            &["H", "I", "B", "D", "Z", "C", "F", "V", "S"],
        );
        assert_eq!(got[0], Ok(HostType::SmallInt { signed: true }));
        assert_eq!(got[1], Ok(HostType::Integer { signed: true }));
        assert_eq!(got[2], Ok(HostType::BigInt { signed: true }));
        assert_eq!(got[3], Ok(HostType::Decimal { digits: 7, scale: 2, signed: true }));
        assert!(matches!(got[4], Ok(HostType::Zoned { digits: 7, scale: 2, signed: true, sign: Some(_) })));
        assert_eq!(got[5], Ok(HostType::Char(10)));
        assert_eq!(got[6], Ok(HostType::Double));
        assert_eq!(got[7], Ok(HostType::VarChar(30)));
        let Ok(HostType::Structure(members)) = &got[8] else { panic!("{:?}", got[8]) };
        assert_eq!(members.iter().map(|(_, t)| t.clone()).collect::<Vec<_>>(), [HostType::Integer { signed: true }, HostType::Char(20)]);
    }

    #[test]
    fn dbcs_items_bind_as_graphic_and_vargraphic() {
        let data = "       01 G PIC G(5) DISPLAY-1.\n       01 V.\n          49 V-LEN PIC S9(4) COMP.\n          49 V-TEXT PIC G(30) DISPLAY-1.\n       01 E PIC GBG DISPLAY-1.\n";
        let got = types(data, &["G", "V", "E"]);
        assert_eq!((&got[0], &got[1]), (&Ok(HostType::Graphic(5)), &Ok(HostType::VarGraphic(30))));
        assert!(got[2].is_err(), "a DBCS PICTURE with B has no SQL type");
    }

    #[test]
    fn declarations_with_no_sql_type() {
        let got = types("       01 E PIC ZZ9.99.\n       01 P USAGE POINTER.\n       01 BS PIC S9(3)V9 COMP.\n", &["E", "P", "BS"]);
        assert!(got.iter().all(Result::is_err), "{got:?}");
    }

    /// Against a live server named by IRONWORK_PG_URL, which `tools/pg-test.sh` starts in a
    /// container; without it the test passes without running.
    mod live {
        use super::*;
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
            setup.load_script(SCHEMA).expect("the schema loads");
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

        /// Lines that put `text` in STMT, thirty characters at a time.
        fn set(text: &str) -> String {
            let mut lines = String::from("           MOVE SPACES TO STMT-TEXT.\n");
            for (k, chunk) in text.as_bytes().chunks(30).enumerate() {
                let chunk = std::str::from_utf8(chunk).expect("ASCII");
                lines += &format!("           MOVE '{chunk}' TO STMT-TEXT({}:{}).\n", 30 * k + 1, chunk.len());
            }
            lines + &format!("           MOVE {} TO STMT-LEN.\n", text.len())
        }

        fn shown(sql: &str, label: &str) -> String {
            format!("           EXEC SQL {sql} END-EXEC.\n           MOVE SQLCODE TO E-CODE.\n           DISPLAY '{label} ' E-CODE.\n")
        }

        fn dynamic_program() -> String {
            [
                concat!(
                    "       IDENTIFICATION DIVISION.\n",
                    "       PROGRAM-ID. PGDYN.\n",
                    "       DATA DIVISION.\n",
                    "       WORKING-STORAGE SECTION.\n",
                    "           EXEC SQL INCLUDE SQLCA END-EXEC.\n",
                    "           EXEC SQL INCLUDE SQLDA END-EXEC.\n",
                    "       01 E-NUM    PIC -9(4).\n",
                    "       01 STMT.\n",
                    "          49 STMT-LEN  PIC S9(4) COMP.\n",
                    "          49 STMT-TEXT PIC X(120).\n",
                    "       01 WS-ID    PIC S9(9) COMP.\n",
                    "       01 WS-NAME  PIC X(10).\n",
                    "       01 E-CODE   PIC -9(3).\n",
                    "           EXEC SQL DECLARE C2 CURSOR FOR SEL END-EXEC.\n",
                    "       PROCEDURE DIVISION.\n",
                ),
                &set("CREATE TABLE dyn (id integer PRIMARY KEY, name char(10))"),
                &shown("EXECUTE IMMEDIATE :STMT", "CREATE"),
                &set("INSERT INTO dyn VALUES (?, ?)"),
                &shown("PREPARE INS FROM :STMT", "PREPARE"),
                "           MOVE 1 TO WS-ID. MOVE 'ADAMS' TO WS-NAME.\n",
                &shown("EXECUTE INS USING :WS-ID, :WS-NAME", "INSERT"),
                "           MOVE 2 TO WS-ID. MOVE 'BAKER' TO WS-NAME.\n",
                &shown("EXECUTE INS USING :WS-ID, :WS-NAME", "INSERT"),
                &shown("EXECUTE INS USING :WS-ID, :WS-NAME", "DUPLICATE"),
                &set("SELEC name FROM dyn"),
                &shown("PREPARE BAD FROM :STMT", "SYNTAX"),
                &set("SELECT name FROM nosuch"),
                &shown("PREPARE BAD FROM :STMT", "NO TABLE"),
                &set("SELECT name FROM dyn WHERE id >= ? ORDER BY id"),
                &shown("PREPARE SEL FROM :STMT", "PREPARE"),
                "           MOVE 1 TO WS-ID.\n",
                &shown("OPEN C2 USING :WS-ID", "OPEN"),
                concat!(
                    "           PERFORM UNTIL SQLCODE NOT = 0\n",
                    "               EXEC SQL FETCH C2 INTO :WS-NAME END-EXEC\n",
                    "               IF SQLCODE = 0\n",
                    "                   DISPLAY 'ROW ' WS-NAME\n",
                    "               END-IF\n",
                    "           END-PERFORM.\n",
                ),
                &shown("CLOSE C2", "CLOSE"),
                "           MOVE 2 TO SQLN.\n",
                &set("SELECT id, name FROM dyn"),
                &shown("PREPARE S3 INTO :SQLDA FROM :STMT", "DESCRIBE"),
                concat!(
                    "           MOVE SQLD TO E-NUM.\n",
                    "           DISPLAY 'SQLD ' E-NUM.\n",
                    "           MOVE SQLTYPE(1) TO E-NUM.\n",
                    "           DISPLAY 'ID ' E-NUM ' ' SQLNAMEC(1)(1:SQLNAMEL(1)).\n",
                    "           MOVE SQLTYPE(2) TO E-NUM.\n",
                    "           DISPLAY 'NAME ' E-NUM ' ' SQLLEN(2) ' '\n",
                    "                   SQLNAMEC(2)(1:SQLNAMEL(2)).\n",
                ),
                &set("DROP TABLE dyn"),
                &shown("EXECUTE IMMEDIATE :STMT", "DROP"),
                "           GOBACK.\n",
            ]
            .concat()
        }

        const DYNAMIC_EXPECTED: &str = concat!(
            "CREATE  000\n",
            "PREPARE  000\n",
            "INSERT  000\n",
            "INSERT  000\n",
            "DUPLICATE -803\n",
            "SYNTAX -104\n",
            "NO TABLE -204\n",
            "PREPARE  000\n",
            "OPEN  000\n",
            "ROW ADAMS     \n",
            "ROW BAKER     \n",
            "CLOSE  000\n",
            "DESCRIBE  000\n",
            "SQLD  0002\n",
            "ID  0496 ID\n",
            "NAME  0453 0010 NAME\n",
            "DROP  000\n",
        );

        fn run_dynamic(mut database: Box<dyn Database + '_>) -> Result<String, String> {
            let compiled = crate::compile(syntax::parse(&dynamic_program()).expect("parses"), &[]).expect("compiles");
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let ran = compiled.execute_with(crate::unit::Library::default(), crate::files::Dds::default(), None, crate::unit::Clock::System, Some(database.as_mut()), &mut out, &mut err);
            ran.map(|_| String::from_utf8(out).expect("DISPLAY writes text")).map_err(|a| format!("{}: {}", a.code, a.message))
        }

        #[test]
        fn dynamic_statements_run_against_postgresql_and_replay_identically() {
            let Some(url) = url() else { return };
            let mut setup = Postgres::connect(&url, None).expect("connects");
            setup.load_script("DROP TABLE IF EXISTS dyn").expect("the table is dropped");
            let postgres = Postgres::connect(&url, None).expect("connects");
            let source = postgres.source().to_owned();
            let recording = Rc::new(RefCell::new(Vec::new()));
            let recorder = Recorder::new(Box::new(postgres), Box::new(Sink(recording.clone())), &source).expect("records");
            assert_eq!(run_dynamic(Box::new(recorder)).as_deref(), Ok(DYNAMIC_EXPECTED));
            let text = String::from_utf8(recording.borrow().clone()).expect("a recording is text");
            assert!(text.contains(" PREPARE INS\n") && text.contains(" CREATE\n"), "{text}");
            let replayed = run_dynamic(Box::new(Replay::parse(&text, false).expect("the recording parses")));
            assert_eq!(replayed.as_deref(), Ok(DYNAMIC_EXPECTED), "{text}");
        }

        const ROWS_PROGRAM: &str = concat!(
            "       IDENTIFICATION DIVISION.\n",
            "       PROGRAM-ID. PGROWS.\n",
            "       DATA DIVISION.\n",
            "       WORKING-STORAGE SECTION.\n",
            "           EXEC SQL INCLUDE SQLCA END-EXEC.\n",
            "       01 IDS.\n",
            "          05 R-ID   PIC S9(9) COMP OCCURS 4.\n",
            "       01 NAMES.\n",
            "          05 R-NAME PIC X(3) OCCURS 4.\n",
            "       01 OUT-IDS.\n",
            "          05 O-ID   PIC 9 OCCURS 3.\n",
            "       01 OUT-NAMES.\n",
            "          05 O-NAME PIC X(3) OCCURS 3.\n",
            "       01 E-CODE   PIC -9(3).\n",
            "       01 E-ROWS   PIC 9.\n",
            "           EXEC SQL DECLARE C3 CURSOR WITH ROWSET POSITIONING FOR\n",
            "                SELECT ID, NAME FROM RWS ORDER BY ID\n",
            "           END-EXEC.\n",
            "       PROCEDURE DIVISION.\n",
            "           MOVE ZEROS TO OUT-IDS. MOVE SPACES TO OUT-NAMES.\n",
            "           MOVE 1 TO R-ID(1). MOVE 2 TO R-ID(2).\n",
            "           MOVE 3 TO R-ID(3). MOVE 1 TO R-ID(4).\n",
            "           MOVE 'AAA' TO R-NAME(1). MOVE 'BBB' TO R-NAME(2).\n",
            "           MOVE 'CCC' TO R-NAME(3). MOVE 'DUP' TO R-NAME(4).\n",
            "           EXEC SQL INSERT INTO RWS (ID, NAME)\n",
            "                VALUES (:R-ID, :R-NAME) FOR 4 ROWS\n",
            "           END-EXEC.\n",
            "           PERFORM SHOW-CODE.\n",
            "           EXEC SQL INSERT INTO RWS (ID, NAME)\n",
            "                VALUES (:R-ID, :R-NAME) FOR 4 ROWS\n",
            "                NOT ATOMIC CONTINUE ON SQLEXCEPTION\n",
            "           END-EXEC.\n",
            "           PERFORM SHOW-CODE.\n",
            "           EXEC SQL OPEN C3 END-EXEC.\n",
            "           EXEC SQL FETCH NEXT ROWSET FROM C3 FOR 2 ROWS\n",
            "                INTO :O-ID, :O-NAME\n",
            "           END-EXEC.\n",
            "           PERFORM SHOW-CODE.\n",
            "           EXEC SQL FETCH NEXT ROWSET FROM C3\n",
            "                INTO :O-ID, :O-NAME\n",
            "           END-EXEC.\n",
            "           PERFORM SHOW-CODE.\n",
            "           EXEC SQL CLOSE C3 END-EXEC.\n",
            "           GOBACK.\n",
            "       SHOW-CODE.\n",
            "           MOVE SQLCODE TO E-CODE.\n",
            "           MOVE SQLERRD(3) TO E-ROWS.\n",
            "           DISPLAY E-CODE ' ' E-ROWS ' ' O-ID(1) O-NAME(1)\n",
            "                   O-ID(2) O-NAME(2).\n",
        );

        fn run_rows(program: &str, mut database: Box<dyn Database + '_>) -> Result<String, String> {
            let compiled = crate::compile(syntax::parse(program).expect("parses"), &[]).expect("compiles");
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let ran = compiled.execute_with(crate::unit::Library::default(), crate::files::Dds::default(), None, crate::unit::Clock::System, Some(database.as_mut()), &mut out, &mut err);
            ran.map(|_| String::from_utf8(out).expect("DISPLAY writes text")).map_err(|a| format!("{}: {}", a.code, a.message))
        }

        #[test]
        fn multiple_row_inserts_and_rowsets_run_against_postgresql_and_replay_identically() {
            let Some(url) = url() else { return };
            let mut setup = Postgres::connect(&url, None).expect("connects");
            setup.load_script("DROP TABLE IF EXISTS rws; CREATE TABLE rws (id integer PRIMARY KEY, name char(3))").expect("the table is made");
            let postgres = Postgres::connect(&url, None).expect("connects");
            let source = postgres.source().to_owned();
            let recording = Rc::new(RefCell::new(Vec::new()));
            let recorder = Recorder::new(Box::new(postgres), Box::new(Sink(recording.clone())), &source).expect("records");
            let expected = "-803 0 0   0   \n-253 3 0   0   \n 000 2 1AAA2BBB\n 100 1 3CCC2BBB\n";
            assert_eq!(run_rows(ROWS_PROGRAM, Box::new(recorder)).as_deref(), Ok(expected));
            let text = String::from_utf8(recording.borrow().clone()).expect("a recording is text");
            assert!(text.contains("> int:3 | char:\"CCC\"\n> int:1 | char:\"DUP\"\n"), "{text}");
            let replayed = run_rows(ROWS_PROGRAM, Box::new(Replay::parse(&text, false).expect("the recording parses")));
            assert_eq!(replayed.as_deref(), Ok(expected), "{text}");
            let call = ROWS_PROGRAM.replace("           EXEC SQL OPEN C3 END-EXEC.\n", "           EXEC SQL CALL NOPROC (:E-ROWS) END-EXEC.\n");
            let refused = run_rows(&call, Box::new(Postgres::connect(&url, None).expect("connects")));
            assert!(refused.as_ref().is_err_and(|e| e.starts_with("EXEC: EXEC SQL CALL NOPROC was reached: this database runs no stored procedures")), "{refused:?}");
        }
    }
}
