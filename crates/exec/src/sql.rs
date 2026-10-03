//! The SQL runtime is `rt::sql`, with the host type of a declared item from `compile::sql`.

pub use compile::sql::host_type;
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
    }
}
