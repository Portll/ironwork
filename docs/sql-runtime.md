# EXEC SQL at run time

A specification for running embedded SQL, turning the plan decided in
[exec-sql-cics.md](exec-sql-cics.md) into work that can be built and checked.

**Status:** all eight steps of §15 are done (2026-09-30), step 8 as far as Db2 for Linux reaches.
It follows that plan's decisions of 2026-09-28:

- no dependencies;
- record and replay first, then the PostgreSQL wire protocol written in-house;
- SQL after CICS in the order of work.

On 2026-09-29 the operator put SQL next after M8, ahead of the VM in
[codegen-runtime.md](codegen-runtime.md), and kept the interpreter as an option beside the VM.

---

## 1. Why this exists

- **Many programs can't run past their SQL.** In the 8,000-program sample, 685 (8.6%) contain EXEC
  SQL. Reaching any executable SQL statement now ends the run with abend `EXEC` (`Machine::exec`,
  `crates/exec/src/machine.rs`), so none of those programs runs past its first query.
- **Migration testing needs SQL answered.** A batch program has to run against the answers
  production gave, and that is record and replay.
- **What exists is too thin to run.** An `ExecBlock` holds the command word, its options and
  `host_variables: Vec<Ref>`, taken by a text scan (`host_variables` in `syntax/src/parser.rs`).
  The scan pairs no indicator variables, gives no direction, and misses subscripted and
  reference-modified host variables. WHENEVER is parsed and ignored. The SQLCA is real: `INCLUDE
  SQLCA` expands the layout in `syntax/src/system.rs`.

## 2. Ubiquitous language

| Term | Meaning |
|---|---|
| Statement | One executable EXEC SQL block, typed: SELECT INTO, INSERT, UPDATE, DELETE (searched or positioned), OPEN, FETCH, CLOSE, COMMIT, ROLLBACK, SET host variable |
| Host variable | A COBOL item named `:NAME` in a statement, with a role: input (sent) or output (INTO) |
| Indicator | The S9(4) binary item that follows a host variable, as `:HV:IND` or `:HV INDICATOR :IND` |
| Value | A typed SQL value at the database boundary: NULL, SMALLINT, INTEGER, BIGINT, DECIMAL(p,s), CHAR, VARCHAR, DATE, TIME, TIMESTAMP, REAL, DOUBLE, or binary bytes |
| Database | The runtime's interface to whatever answers statements |
| Outcome | What a statement returns: SQLCODE, SQLSTATE, rows affected, warning flags, message tokens, and rows |
| Recording | A file of statements, their inputs and their outcomes, in the order a run made them |

## 3. Typed statements (compile time)

- **One type per statement.** `syntax` gains a typed statement for each executable verb the corpus
  uses. These are the verbs in exec-sql-cics.md's table: SELECT INTO, INSERT, UPDATE, DELETE,
  cursors, COMMIT, ROLLBACK, WHENEVER and INCLUDE. `ExecBlock` keeps its text; the typed form sits
  beside it.
- **Host variables.** Each carries its `Ref`, its role, and its paired indicator.
  - Subscripts and reference modification are parsed.
  - A group item names a host structure. It expands to its elementary items in order, and an
    indicator array pairs with them by position. Step 1 cites the Db2 documentation for the rule.
- **Statement text.** The text sent to a backend is the statement with each host variable replaced
  by a positional marker: `$1…` for PostgreSQL, and `?` in a recording's canonical form.
- **WHENEVER applies in listing order.** It is a declaration, not a statement. Each executable
  statement records the SQLERROR, NOT FOUND and SQLWARNING actions in force where it appears in the
  source, and the tests run in a fixed order after it. cobolwork's
  [precompile.md](https://github.com/Portll/cobolwork/blob/main/docs/spec/precompile.md) §3 item 5
  records the same rule and tests:
  - SQLERROR is `SQLCODE < 0`;
  - NOT FOUND is `SQLCODE = 100`;
  - SQLWARNING is `SQLWARN0 = 'W'`, or `SQLCODE > 0` and `NOT = 100`.
- **Cursors.** DECLARE CURSOR records its query, its input host variables, and WITH HOLD and FOR
  UPDATE. OPEN evaluates those inputs at the moment it runs.
  - **A DECLARE precedes every use.** In COBOL, "the DECLARE CURSOR statement must precede all
    statements that explicitly refer to the cursor by name" ([Db2 13 SQL, DECLARE
    CURSOR](https://www.ibm.com/docs/en/db2-for-zos/13.0.0?topic=statements-declare-cursor)). The
    parser reads each OPEN, FETCH, CLOSE and WHERE CURRENT OF against the DECLAREs before it, gives
    OPEN its cursor, and refuses a cursor not yet declared at compile time.
  - **A cursor ironwork does not run** (scrollable, or for a prepared statement) is still declared.
    Reaching its DECLARE does nothing, as for any declaration; its OPEN abends, naming what it is.
- **Refused by name.** DISCONNECT comes from other precompilers, so a program that uses it is
  refused at compile time as not a Db2 for z/OS program. CONNECT and SET CONNECTION, which Db2 for
  z/OS has for DRDA, dynamic SQL (PREPARE, EXECUTE, EXECUTE IMMEDIATE, DESCRIBE) and multi-row
  FETCH are refused at run time by name. A CONNECT or SET CONNECTION that names its location by a
  host variable first gives the input trace a `connection-target` sink with the value
  ([evidence.md](evidence.md) §1.1).

## 4. The Database interface (run time)

The interface lives in `machine/sql.rs` now, beside `machine/cics.rs`, and moves to `rt` with the
split in [codegen-runtime.md](codegen-runtime.md). It is covered by the runtime exception.

    trait Database {
        fn execute(&mut self, s: &Statement, inputs: &[Value]) -> Outcome;   // SELECT INTO, INSERT, UPDATE, DELETE, SET
        fn open(&mut self, c: CursorId, s: &Statement, inputs: &[Value]) -> Outcome;
        fn fetch(&mut self, c: CursorId) -> Outcome;
        fn close(&mut self, c: CursorId) -> Outcome;
        fn commit(&mut self) -> Outcome;
        fn rollback(&mut self) -> Outcome;
    }

- **The first service written as library code.** The SQL runtime (conversion, the SQLCA, cursor
  state, the backends) reaches storage through a narrow interface: a byte range and its item
  `Kind`, read or written. It holds no reference to `Machine`. The interpreter calls it now and the
  VM calls it unchanged later. It is the first piece of the semantics library that
  codegen-runtime.md's step 1 extracts.
- **Cursor identity.** A cursor is identified by program and cursor name, because two programs may
  declare the same name.
- **Cursor state.** OPEN on an open cursor, and FETCH or CLOSE on a closed one, are answered by the
  runtime from its own record of cursor state, before any backend is asked. That keeps the
  answers the same whatever the backend.

## 5. Converting host variables

- **Where conversion happens.** Storage stays EBCDIC in z/OS layout. Conversion happens only at the
  boundary, using the program's CODEPAGE option and the ICU tables assumption C11 already names.
- **The type table.** The COBOL-to-SQL table is transcribed from IBM's table of equivalent SQL and
  COBOL data types (Db2 13 for z/OS Application Programming and SQL Guide), one row at a time. Each
  row cites that table, as `system.rs` cites the SQL Reference for the SQLCA. The rows the corpus
  needs first:

| COBOL | SQL |
|---|---|
| S9(1)–S9(4) BINARY, COMP, COMP-4, COMP-5 | SMALLINT |
| S9(5)–S9(9) BINARY, COMP, COMP-4, COMP-5 | INTEGER |
| S9(10)–S9(18) BINARY, COMP, COMP-4, COMP-5 | BIGINT |
| S9(p-s)V9(s) PACKED-DECIMAL, COMP-3 | DECIMAL(p,s) |
| S9(p-s)V9(s) DISPLAY SIGN LEADING SEPARATE | DECIMAL(p,s) |
| X(n) | CHAR(n) |
| Group of 49-level S9(4) BINARY length and 49-level X(n) | VARCHAR(n) |
| COMP-1, COMP-2 | REAL, DOUBLE |
| X(10), X(8), X(26) holding a date, time or timestamp string | DATE, TIME, TIMESTAMP, in the DATE and TIME option's format |

Output rules:

| Case | Result |
|---|---|
| NULL into a host variable with an indicator | The indicator is -1 and the host variable is left as it was |
| NULL into a host variable with no indicator | SQLCODE -305 |
| A string longer than its host variable | Truncated. SQLWARN1 and SQLWARN0 are set to `W`, and the indicator holds the original length (assumption SQ3) |
| A number outside its host variable's range | SQLCODE -304 |
| More result columns than host variables | SQLWARN3 and SQLWARN0 are set to `W` |
| Floats | Converted between hexadecimal floating point in storage and IEEE at the boundary, with `zarch`'s HFP model |

Input rules:

- **A negative indicator sends NULL.**
- **Invalid packed or zoned data in an input host variable** abends S0C7, as any read of it does.

## 6. Filling the SQLCA

- **After every executable statement**, the runtime writes the SQLCA fields `system.rs` defines:
  - SQLCODE and SQLSTATE;
  - SQLERRML and SQLERRMC, the message tokens, truncated to 70 bytes;
  - SQLERRD(3), the rows an INSERT, UPDATE or DELETE affected;
  - SQLWARN0 to SQLWARNA.
- **No SQLCA.** A program with no SQLCA, but with a standalone SQLCODE and SQLSTATE as STDSQL(YES)
  declares them, gets those two fields.
- **Warnings.** With SQLCODE 0, a truncated string sets SQLSTATE 01004, and an INTO list shorter
  than the select list sets SQLWARN3 and SQLSTATE 01503.
- **Codes the runtime raises itself.** Db2 12.1 for Linux gave each of these that it can (§10); the
  CICS ones wait for CICS:

| Situation | SQLCODE | SQLSTATE |
|---|---|---|
| No row | +100 | 02000 |
| SELECT INTO found more than one row | -811 | 21000 |
| Duplicate key | -803 | 23505 |
| NULL with no indicator | -305 | 22002 |
| Value out of the host variable's range | -304 | 22003 |
| FETCH or CLOSE on a cursor that is not open | -501 | 24501 |
| OPEN on an open cursor | -502 | 24502 |
| UPDATE or DELETE WHERE CURRENT OF a cursor that is not open | -507 | 24501 |
| UPDATE or DELETE WHERE CURRENT OF a cursor not on a row | -508 | 24504 |
| EXEC SQL COMMIT in a CICS task | -925 | 2D521 |
| EXEC SQL ROLLBACK in a CICS task | -926 | 2D521 |
| Deadlock or timeout, rolled back | -911 | 40001 |
| A value whose type cannot be assigned to the host variable | -303 | 42806 |
| A VARCHAR input whose length is negative or over its maximum | -311 | 22501 |
| A character the code page cannot represent | -330 | 22021 |

## 7. Units of work

- **COMMIT** commits and closes every open cursor not declared WITH HOLD, and leaves a held cursor
  before its next row. **ROLLBACK** rolls back and closes every open cursor, as does a -911 answer.
- **Positioning.** A cursor is on a row after a FETCH that returned one, and not after OPEN, a
  FETCH that returned none, a positioned DELETE, or a COMMIT.
- **Batch.** A normal end of the run unit commits; an abend rolls back. This is assumption SQ1,
  basis `Chosen`, until a Db2 run settles it. Only work since the last COMMIT or ROLLBACK is
  committed or rolled back, so a run that ends with its own COMMIT asks nothing more. The call
  names the run unit's first program with ordinal 0.
- **CICS.** `EXEC CICS SYNCPOINT` calls `commit`, and `SYNCPOINT ROLLBACK` calls `rollback`. A task
  that ends normally commits, and a task that abends rolls back, as CICS backs out a failed task's
  work.
  - A held cursor stays open across SYNCPOINT. SYNCPOINT ROLLBACK and the end of the task close
    every cursor ([CICS TS 6, CICS and CURSOR WITH
    HOLD](https://www.ibm.com/docs/en/SSJL4D_6.x/applications/developing/database/dfhtk67.html)).
    At the end of the task the runtime asks the backend's `close_all` for the held cursors a commit
    left open, so a later task on the same connection finds none.
  - Under `--serve` the region holds one database, and one recording, for every task. Each task
    is its own unit of work, as it is in CICS.
  - EXEC SQL COMMIT and ROLLBACK are refused in a CICS task with -925 and -926: the task's unit of
    work belongs to CICS.
  - A commit the database refuses at SYNCPOINT backs the unit of work out and raises ROLLEDBACK
    (RESP 82); left unhandled, the task abends AEXJ ([CICS TS 6, EXEC
    CICS SYNCPOINT](https://www.ibm.com/docs/en/cics-ts/6.x?topic=summary-syncpoint)).
- **Ordinal 0.** A commit or rollback that no EXEC SQL statement asks for (SYNCPOINT, or the end of
  a run unit or task) names the first program with ordinal 0. It reaches the database only while the
  database holds work or an open cursor.

## 8. Record and replay

- **Recording.** `--sql-record FILE` wraps any backend and writes each call and its outcome.
  `--sql-replay FILE` answers from a recording, with no database at all.
- **Statement identity.** A statement is identified as `PROGRAM:ORDINAL:HASH`: its ordinal among the
  program's EXEC SQL blocks in listing order (INCLUDE is expanded as COPY is, and not counted), and
  the 32-bit FNV-1a hash of its normalised text in eight hex digits. Line numbers are not used,
  because an edit above a statement would change them.
- **The format** is line-oriented text, written and read by ironwork's own code:

      # ironwork sql recording 1
      @ 1 PAYROLL:3:9f2a41c0 SELECT
      > char:"00123"
      < 0 00000 rows=1
      = dec:1234.50 | char:"SMITH" | null
      @ 2 PAYROLL:4:1b77e0d2 FETCH C1
      < 100 02000 rows=0

  - **`@`** starts a call: sequence number, statement identity, verb and cursor. OPEN's text is its
    cursor's declaration, `DECLARE C1 CURSOR [WITH HOLD] FOR query`; FETCH's and CLOSE's are
    `FETCH C1` and `CLOSE C1`.
  - **`>`** gives the input values, in host-variable order.
  - **`<`** gives SQLCODE, SQLSTATE, rows affected, and any message tokens as
    `tokens=char:"…"`. It holds no warning flags: the runtime sets SQLWARN from what assignment
    did, as Db2's precompiled code does.
  - **`=`** gives one output row.
  - **Values** are:
    - `null`, `int:`, `dec:` and `double:`. A decimal's scale is its count of fractional digits, so
      `dec:1234.50` has scale 2; its precision is the column's, not the value's. A double is written
      in shortest round-trip form;
    - `char:"…"`, UTF-8 after CCSID conversion, with `\"`, `\\` and `\xNN` escapes;
    - `hex:` for binary data.
  - DATE, TIME and TIMESTAMP travel as `char:` in ISO form, as they reach a COBOL host variable.
    Typed `date:`, `time:` and `ts:` wait for a backend that sends them.
- **Matching.** Replay is strict by default: call *n* must match record *n*'s statement identity
  and inputs. `--sql-replay-mode keyed` instead matches each call to the next unused record with
  the same identity and inputs, for tests whose order does not matter.
- **A mismatch abends.** It abends `SQLR`, naming the expected and actual statement and inputs.
  Replay never guesses.
- **Where recordings come from:** hand-written fixtures, the PostgreSQL backend in record mode, or
  later an import from Db2 output. A recording is as true as its source, and its header says
  which source that was.

## 9. The PostgreSQL backend

- **Protocol.** Wire protocol version 3 over `std::net::TcpStream`, or
  `std::os::unix::net::UnixStream` for a local socket. It uses the extended query protocol (Parse,
  Describe, Bind, Execute, Sync) with text format values, since the boundary already converts to
  typed values. Each distinct statement text is prepared once per connection, and Describe gives the
  parameter and column types that the text conversions below need.
- **The session** sets `client_encoding` UTF8, `DateStyle` ISO, `TimeZone` UTC and
  `extra_float_digits` 3, so every value's text has one form.
- **Units of work.** The first statement after a COMMIT or ROLLBACK begins a transaction. Each
  statement runs under a savepoint, because Db2 undoes a failed statement and keeps the unit of
  work, where PostgreSQL would abort all of it. A -911 rolls back the whole unit, as Db2 does.
- **Authentication.** SCRAM-SHA-256 (RFC 5802 and RFC 7677), with SHA-256, HMAC and the PBKDF2 `Hi`
  function written in-house and tested against the RFCs' test vectors. A cleartext password is
  answered too; MD5 is refused, naming SCRAM. The client nonce is 18 bytes from `/dev/urandom` on
  Unix; elsewhere it comes from `std`'s OS-seeded `RandomState` keys, the process and the clock
  (assumption SQ6). Neither needs `unsafe`.
- **TLS** is a separate build, so that ironwork's own keeps no dependencies (D2). `tls/` is a
  workspace of its own, as `fuzz/` is. It builds the same command with rustls and the ring
  provider, verifying the server's certificate chain and name (`sslmode=verify-full`) against
  `sslrootcert` or the Mozilla roots. ironwork's own build refuses verify-full, naming tls/. libpq's
  `require`, `verify-ca`, `prefer` and `allow` are refused, since none of them checks both the chain
  and the name.
- **Dialect.** Statement text passes through a short, documented rewrite table (assumption SQ5):
  - `CURRENT DATE` becomes `CURRENT_DATE`; `CURRENT TIME` and `CURRENT TIMESTAMP` (and their
    underscored forms) become `LOCALTIME` and `LOCALTIMESTAMP`, since Db2's special registers
    carry no time zone;
  - `CONCAT` becomes `||`, and `VALUE(…)` becomes `COALESCE(…)`;
  - a statement that is `VALUES expression`, as `SET :hv =` and `VALUES … INTO` give, becomes
    `SELECT expression`;
  - `FOR UPDATE OF columns` becomes `FOR UPDATE`, since PostgreSQL's `OF` names tables;
  - isolation clauses (`WITH UR`, `CS`, `RS`, `RR`, with any `USE AND KEEP … LOCKS`),
    `OPTIMIZE FOR n ROWS`, `FOR FETCH ONLY` and `FOR READ ONLY` are removed;
  - a cursor name with a hyphen is quoted wherever the statement names it.

  Anything else passes through unchanged. `SYSIBM.SYSDUMMY1` needs no rewrite: the test schema
  provides schema `SYSIBM` and a one-row view `SYSDUMMY1`.
- **Values.** Dates and times reach a character host variable in Db2's ISO forms (assumption
  SQ11): `YYYY-MM-DD`, `HH.MM.SS` and `YYYY-MM-DD-HH.MM.SS.NNNNNN`. A character input in those forms
  sent to a time or timestamp parameter is given PostgreSQL's.
- **Errors.** PostgreSQL SQLSTATEs map to Db2 SQLCODEs through a table (assumption SQ4):

  | PostgreSQL | Db2 SQLCODE | Db2 SQLSTATE |
  |---|---|---|
  | 23505 unique violation | -803 | 23505 |
  | 23502 not-null violation | -407 | 23502 |
  | 23503 foreign-key violation | -530 | 23503 |
  | 22001 string too long | -404 | 22001 |
  | 22012 division by zero | -802 | 22012 |
  | 40001 serialization failure, 40P01 deadlock | -911 | 40001 |
  | 42601 syntax error | -104 | 42601 |
  | 42P01 undefined table | -204 | 42704 |
  | 42703 undefined column | -206 | 42703 |

  A SQLSTATE not in the table abends `SQL`, naming it and PostgreSQL's message, rather than
  inventing an SQLCODE. The table grows from what runs meet.
- **Recording.** `--sql-record FILE` wraps the backend, and the header names the server's version,
  host and database.
- **Tests.** `tools/pg-test.sh` starts PostgreSQL 14.19 in a container and runs the live tests,
  which pass without running when `IRONWORK_PG_URL` is unset.

## 10. Assumptions

These are registry entries under the prefix **SQ** (S is SORT's), with the oracle `Oracle::Db2`,
because neither Hercules nor Enterprise COBOL can settle them, and the basis `Observed`. SQ6 stays
in this document only: it is ironwork's own choice, and nothing settles it. *Observed* means Db2
12.1.5 for Linux (Community Edition, 2026-09-30) gave the result, through embedded SQL in C, and Db2
for z/OS documents the same or says nothing to the contrary. Where the two disagree, ironwork
follows z/OS and the row says so. [`tools/db2-probe/`](../tools/db2-probe/run.sh) runs the probes
again, and its `observed-12.1.5.txt` is what Db2 answered.

| ID | Claim | Basis |
|---|---|---|
| SQ1 | A batch run unit commits on normal end and rolls back on abend | Documented: "In all Db2 environments, the normal termination of a process is an implicit commit operation" (Db2 12 for z/OS SQL, COMMIT). Db2 for Linux rolls back instead, on a normal end and on a bad return code alike |
| SQ2 | WHENEVER's tests run in the order and with the conditions §3 gives | Observed: the precompiler tests SQLERROR (`< 0`), then SQLWARNING (`> 0` and not 100, or 0 with SQLWARN0 `W`), then NOT FOUND (100). The three exclude one another, so they take the branches §3's order takes |
| SQ3 | A truncated string's indicator holds its original length | Observed: 18 for an 18-character value cut to 5, into a C string and a VARCHAR alike, with SQLWARN0 and SQLWARN1 `W` and SQLSTATE 01004. Trailing blanks cut from a CHAR count as truncation |
| SQ4 | The PostgreSQL SQLSTATE to Db2 SQLCODE table | Observed for -803, -407, -530, -104, -204 (42704) and -206. Db2 for Linux gives -433 where z/OS documents -404 for a string too long for its column, and -801 where z/OS documents -802 (22012) for division by zero; the table keeps z/OS's. -911 is not provoked |
| SQ5 | The dialect rewrite table | Chosen |
| SQ6 | The SCRAM client nonce, where there is no `/dev/urandom` (Windows), comes from `std`'s OS-seeded `RandomState` keys, the process and the clock | Chosen |
| SQ7 | A name in an INTO list written without its colon is a host variable, as older precompilers assumed. Real programs do it (`FETCH C INTO CSR-ENTITY, CSR-PROJ-ID`) | Recalled |
| SQ8 | WHENEVER and cursor declarations carry on in listing order across nested programs, since the precompiler reads the source in order | Chosen |
| SQ9 | An IEEE double stored into COMP-1 or COMP-2 drops the low-order bits that do not fit, rather than rounding | Chosen |
| SQ10 | A zoned DISPLAY item without SIGN SEPARATE is a DECIMAL host variable, as SIGN LEADING SEPARATE is | Chosen |
| SQ11 | Dates and times reach character host variables in the ISO forms, as DSNHDECP's default DATE(ISO) and TIME(ISO) give | Observed for the forms, under the precompiler's DATETIME(ISO): `2020-01-02`, `13.45.06` and `2020-01-02-03.04.05.500000`. The default is an installation's choice; Db2 for Linux's US territory gives `13:45:06` |
| SQ12 | Character inputs are sent with their trailing blanks. Db2 compares strings as if blank-padded, while PostgreSQL does so only for CHAR(n), so test schemas declare CHAR(n) where Db2's has CHAR | Observed: Db2 matches `'SHORT   '` to a VARCHAR holding `SHORT` |
| SQ13 | A single-row FETCH that returns a row sets SQLERRD(3) to 1 | Observed. Db2 for z/OS documents SQLERRD(3) for a rowset FETCH only |

Also observed, and matching §6 and the runtime: +100 for no row, and for a searched UPDATE or
DELETE that changes none; -811, -305, -304; -501 for FETCH or CLOSE of a cursor never opened, and
after a COMMIT closes it; -502, -507 and -508; a held cursor fetching after COMMIT; and SQLWARN3
with SQLSTATE 01503 when an INTO list is shorter than the select list.

Not settled here: SQ5 and SQ6 are ironwork's own choices; SQ7 to SQ10 need Db2 for z/OS and its
COBOL precompiler; -925 and -926 need CICS. IBM's COBOL for Linux trial cannot serve, as its licence
is for evaluation only.

## 11. Invariants

1. **No executable statement is skipped silently.** It reaches the Database, or the run abends
   naming why.
2. **Replay never guesses.** A mismatch is an abend.
3. **Storage stays EBCDIC.** Conversion happens only at the Database boundary.
4. **No third-party dependency and no `unsafe`.**
5. **The runtime owns cursor-state errors.** -501, -502, -507 and -508 come from the runtime, not
   the backend.
6. **Every SQLCODE the runtime writes is either in §6 or in SQ4's table.**

## 12. Specification (BDD)

### Q1: Single-row statements

- **Given** a recording that answers `SELECT NAME INTO :WS-NAME … WHERE ID = :WS-ID` with one row
  **then** WS-NAME holds the name in EBCDIC, SQLCODE is 0 and SQLSTATE is `00000`.
- **Given** the answer is no row **then** SQLCODE is +100, WS-NAME is unchanged, and a WHENEVER NOT
  FOUND GO TO in force at that statement is taken.
- **Given** the answer is two rows **then** SQLCODE is -811.

### Q2: Indicators and conversion

- **Given** a NULL column into `:WS-X:WS-X-IND` **then** WS-X-IND is -1 and WS-X is unchanged.
- **Given** a NULL column into `:WS-X` with no indicator **then** SQLCODE is -305.
- **Given** `char:"ABCDEFGHIJ"` into a PIC X(5) **then** WS-X holds `ABCDE`, SQLWARN1 is `W`, and
  the indicator is 10.
- **Given** `dec(7,2):-1234.50` into PIC S9(5)V99 COMP-3 **then** storage holds `X'0123450D'`.
- **Given** an input host variable holding invalid packed data **then** the run abends S0C7 at the
  statement.

### Q3: Changes and units of work

- **Given** an INSERT whose recording says one row **then** SQLERRD(3) is 1.
- **Given** a duplicate key **then** SQLCODE is -803 and WHENEVER SQLERROR is taken.
- **Given** a batch run that abends after an UPDATE **then** the backend receives `rollback`.
- **Given** a CICS task that runs SYNCPOINT **then** the backend receives `commit`.

### Q4: Cursors

- **Given** DECLARE, OPEN, a FETCH loop until +100, then CLOSE **then** each row lands in the INTO
  host variables and the loop ends at +100.
- **Given** FETCH on a cursor never opened **then** SQLCODE is -501, without asking the backend.
- **Given** COMMIT with an open cursor not declared WITH HOLD **then** the next FETCH returns -501.
- **Given** OPEN evaluates `:WS-DEPT` **then** the value sent is WS-DEPT's value at OPEN, not at
  DECLARE.

### Q5: Replay

- **Given** a run that makes a call the recording does not hold next **then** the run abends `SQLR`,
  naming both statements.
- **Given** `--sql-record` wrapping PostgreSQL, then `--sql-replay` of the result on the same
  program and input **then** the two runs produce identical storage, output and SQLCA contents.

### Q6: WHENEVER in listing order

- **Given** a WHENEVER SQLERROR GO TO in a paragraph after the one holding a statement, where that
  paragraph is performed first at run time **then** the statement is not governed by it.

## 13. Out of scope

- **Dynamic SQL and SQLDA.**
- **Multi-row FETCH and INSERT** with host-variable arrays.
- **Stored procedures**, and CALL of SQL procedures.
- **LOB types.**
- **Db2 over DRDA.**
- **SQLite**, by the 2026-09-28 decision.

## 14. Decisions for the operator

- **D1.** The recording format of §8, and strict replay as the default.
- **D2.** Settled 2026-09-30: TLS as a separate, optional build on rustls (§9), not in-house TLS,
  and not in ironwork's own build.
- **D3.** Settled 2026-09-30: DISCONNECT is refused at compile time as not Db2 for z/OS. CONNECT is
  Db2 for z/OS ([Db2 13,
  CONNECT](https://www.ibm.com/docs/en/db2-for-zos/13.0.0?topic=statements-connect)), so it passes
  the check and is refused by name at run time (§3).
- **D4.** Done 2026-09-30: the SQ prefix, `Oracle::Db2` and the `Observed` basis are in the
  assumptions registry.

## 15. Execution plan

**Where it stands, 2026-09-30.** Steps 1 to 7 are built.

Step 1: `syntax/src/sql.rs`, a `sql` field on `ExecBlock`, WHENEVER state in the parser, and
`Check` reporting malformed statements. Two things the corpus taught are now assumptions:
- cursor names take hyphens (`PROGRAMS-CSR`);
- an INTO list may omit colons (SQ7).

Checked against a build of `main` over the 3,494 programs in the 500-repository corpus that hold
EXEC SQL, both builds accept the same 362 programs.

Step 2:
- `exec/src/sql/`: `Value`, `HostType` from the layout (including VARCHAR, GRAPHIC and VARGRAPHIC
  from DBCS items, and host structures),
  and `read` and `write` by Db2's assignment rules;
- `exec/src/codec.rs`: the packed and zoned reads, moved out of `Machine`, which now calls them,
  so a host variable sends exactly what a COMPUTE would read.

Step 3:
- `exec/src/sql/database.rs`: the `Database` trait, whose answer is an outcome or an `Abandoned`
  that ends the run;
- `exec/src/machine/sql.rs`: single-row SELECT, INSERT, UPDATE, DELETE, COMMIT and ROLLBACK,
  SQLCA filled by field name, indicators, SQLWARN1 and SQLWARN3, and WHENEVER branching;
- `Compiled::execute_with`, which attaches a database. A run with none abends at its first EXEC
  SQL, as before.

Step 4:
- `exec/src/sql/replay.rs`: the §8 format, `Replay` (strict or keyed) and `Recorder`, which wraps
  any backend;
- `ironwork run --sql-replay FILE [--sql-replay-mode strict|keyed]`. `--sql-record` waits for step
  7, as there is no backend to wrap before it.

Step 5:
- the parser reads cursors against the DECLAREs before them (`Cursors` in `syntax/src/sql.rs`);
- the runtime keeps cursor state in `Session` and answers -501, -502, -507 and -508 itself;
- a normal end commits pending work and an abend rolls it back (SQ1).

Checked over the same 3,494 programs, step 4's build and step 5's accept 450 and 449. The one
refused is an Open-COBOL-ESQL test that opens a cursor it never declares
(`test/ocesql/src/sqlca/open-fetch-close.cbl`), which the Db2 precompiler refuses too.

Step 6:
- `Compiled::execute_cics_with` attaches a database to a CICS task. SYNCPOINT and SYNCPOINT
  ROLLBACK settle the unit of work, and so does the end of the task.
- `ironwork cics --sql-replay` answers a task from a recording. `--serve` refuses it, as one
  recording cannot answer an open-ended conversation.

Step 7, in `exec/src/sql/postgres/`:
- `scram.rs`: SHA-256, HMAC, `Hi`, base64 and the SCRAM client;
- `wire.rs`: the connection string, framing, startup and authentication, and the simple and
  extended query cycles;
- `dialect.rs`: the rewrite table, the SQLSTATE table, and value text in both directions;
- `mod.rs`: the backend, with savepoints and prepared-statement reuse.

The runtime answers a searched UPDATE or DELETE that changes no row with +100, as Db2 does.
`ironwork run --sql-db URL [--sql-record FILE]` runs against PostgreSQL. Q5 passes against
PostgreSQL 14.19: a recorded run covers a cursor loop with a NULL, a timestamp, -803, +100,
`WITH UR`, `SYSIBM.SYSDUMMY1` and -811, and replaying it gives identical output.

Step 8, on Db2 12.1.5 for Linux (Community Edition, whose licence allows internal non-production
development and test): SQ1 to SQ4 and SQ11 to SQ13 are settled or their disagreement recorded
(§10). The runtime now sets SQLSTATE 01004 and 01503 with their warnings, and SQLERRD(3) after a
FETCH, as Db2 does.

The work came after M8 (BMS and the 3270 terminal) and before the VM (operator, 2026-09-29). It
lives in new files (`syntax/src/sql.rs`, `exec/src/machine/sql.rs` and `exec/src/sql/`), with the
smallest hooks in the shared ones.

| Step | Work | Done when |
|---|---|---|
| 1 | Typed statements, host variables with roles and indicators, and WHENEVER attached in listing order. Run the census before and after. | Census counts unchanged or better; parser tests for every form in §3 |
| 2 | Conversion between COBOL items and values, with a test for each row of §5 | Q2 |
| 3 | SQLCA filling, WHENEVER branching, the Database trait, and replay of single-row statements | Q1, Q3 (batch) |
| 4 | The recording format, `--sql-record`, `--sql-replay`, and keyed mode | Q5 |
| 5 | Cursors | Q4 |
| 6 | CICS SYNCPOINT and task-end units of work | Q3 (CICS) |
| 7 | The PostgreSQL backend: wire protocol, SCRAM, rewrites, and the SQLSTATE table | Q5 against a live PostgreSQL |
| 8 | Settle SQ1 to SQ4 on a lawful Db2 | Each assumption's basis updated, or its disagreement recorded |
