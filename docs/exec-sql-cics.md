# EXEC SQL and EXEC CICS in ironwork for COBOL

## Why it matters

In a sample of 8,000 programs from the local corpus, 1,890 (24%) contain EXEC CICS and 685 (8.6%)
contain EXEC SQL. A compiler that refuses both cannot even check those programs: in the
3,000-program census, taken while ironwork still refused them, EXEC CICS was the first refusal for
291 programs and EXEC SQL for 140. Nothing else blocked as many programs that are genuinely IBM
programs.

## What IBM does

Neither is COBOL. A translation step, which runs before the compiler, turns each EXEC block into
ordinary COBOL.

- **EXEC SQL.** The Db2 precompiler, or the Db2 coprocessor behind the SQL compiler option, replaces
  each statement with a CALL to Db2's language interface. The CALL passes a parameter list
  describing the host variables (`:NAME`), their types and their indicator variables. The SQL itself
  goes into a DBRM, which is bound into a package. `INCLUDE SQLCA` and the DCLGEN members are copied
  in like COPY members, and `WHENEVER` becomes GO TO after each statement.
- **EXEC CICS.** The CICS translator, or the integrated translator behind the CICS option, replaces
  each command with MOVEs into argument fields and a CALL to CICS's command interface. It adds the
  EIB (`DFHEIBLK`) and `DFHCOMMAREA` to the LINKAGE SECTION, and the program then runs inside a CICS
  region: pseudo-conversational `RETURN TRANSID ... COMMAREA`, LINK and XCTL, file control over VSAM,
  temporary storage and transient data queues, BMS maps on 3270 terminals, and HANDLE CONDITION or
  RESP for outcomes.

## What the corpus uses

| EXEC SQL | Occurrences | EXEC CICS | Occurrences |
|---|---|---|---|
| SELECT ... INTO | 767 | ASKTIME, FORMATTIME | 3,132, 3,130 |
| INCLUDE SQLCA | 613 | RETURN | 2,912 |
| INSERT | 588 | LINK | 2,753 |
| UPDATE | 542 | SEND (maps and text) | 1,410 |
| BEGIN/END DECLARE SECTION | 142/130 | ABEND | 1,069 |
| COMMIT, ROLLBACK | 115, 49 | READ, WRITE, REWRITE, DELETE | 790, 364, 396, 366 |
| CONNECT, DISCONNECT | 86, 77 | STARTBR, READNEXT, READPREV, ENDBR | 332, 296, 92, 385 |
| DELETE | 77 | ASSIGN, RECEIVE, XCTL | 627, 611, 421 |
| WHENEVER | 109 | HANDLE, SYNCPOINT, WRITEQ | 280, 136, 135 |
| cursors (DECLARE, OPEN, FETCH, CLOSE) | 36 each | QUERY, DEFINE, GET, PUT | 180, 111, 80, 56 |

CONNECT and DISCONNECT, and BEGIN DECLARE SECTION outside Db2 idiom, come from programs written for
other precompilers (GnuCOBOL with ocesql or esqlOC, for PostgreSQL and ODBC). They are not Db2
programs.

## Options

1. **Keep refusing.** Honest and costs nothing, but a quarter of the programs stay out entirely,
   including from `check`.
2. **Parse, don't run.** Read each block into a typed statement (SQL statement kind and host
   variables; CICS command and its options) instead of refusing it. `check` then accepts the
   program: everything else in it is compiled and checked, and every host variable and CICS
   argument is resolved. Reaching an EXEC statement at run time ends the run with a clear message
   that names the command. This is cheap, and it is also what analysis needs.
3. **An SQL runtime behind an interface.** Translate each statement into a call on a connection
   interface that binds host variables (EBCDIC, packed, zoned and binary converted to SQL types
   and back), fills the SQLCA with SQLCODE and SQLSTATE, and handles indicator variables, cursors
   and WHENEVER. The backends differ in cost:
   - **Record and replay.** A test double that answers from recorded Db2 results. It needs no
     database and no dependency, and it is what migration testing needs: run a batch program
     against the answers production gave.
   - **PostgreSQL over its wire protocol,** written in Rust. The protocol is small, but SCRAM
     authentication needs SHA-256 and HMAC written in-house to keep "no dependencies". The SQL
     dialect differs from Db2 (date and time functions, FETCH FIRST, special registers).
   - **SQLite, embedded.** The quickest to get running, but it brings a C library into the build.
     That breaks the "no dependencies" rule and brings `unsafe` code in through the dependency.
   - **Db2 over DRDA.** Faithful but heavy. It only makes sense for a client with Db2.
4. **A CICS runtime, in tiers.**
   - *Tier 1, program control:* RETURN (including TRANSID and COMMAREA), LINK, XCTL, ABEND,
     ASKTIME, FORMATTIME, ASSIGN, HANDLE CONDITION and RESP, SYNCPOINT, GETMAIN and FREEMAIN,
     ADDRESS. Almost all of this is logic ironwork already has: the run unit, CALL, LINKAGE and
     pointers.
   - *Tier 2, data:* file control over VSAM (READ, WRITE, REWRITE, DELETE, browse) and temporary
     storage queues. It needs indexed files, which batch programs need too (141 census refusals
     are ORGANIZATION INDEXED).
   - *Tier 3, terminals:* SEND and RECEIVE MAP through BMS. cobolwork already parses BMS maps.
     The screen could be a 3270 emulator or a structured harness that feeds and captures fields.
   - *Tier 4:* web, MQ, channels and containers. Only for a client that needs them.

## Recommendation

1. **Now: option 2 for both.** It turns about 430 census refusals into checked programs, and it is
   the foundation every later option builds on.
2. **Next: indexed files** (VSAM KSDS, ESDS, RRDS). Batch programs need them on their own, and
   CICS file control needs them.
3. **Then CICS Tiers 1 and 2 as an API-level harness.** A test run supplies the transaction ID,
   the COMMAREA, the EIB fields and file fixtures, and asserts on the COMMAREA, the files and the
   RETURN. Being able to test CICS programs without a region is the main value, and it needs no
   terminal.
4. **SQL: record and replay first; PostgreSQL second, if live data matters.** Avoid SQLite unless
   the dependency rule is relaxed on purpose.

## Decisions, 2026-09-28

1. **Parse, don't run: adopted now.**
2. **SQL backends keep the no-dependencies rule.** Record and replay first; the PostgreSQL wire
   protocol, including SCRAM, written in-house.
3. **CICS includes a 3270 terminal** as well as the API-level harness.
4. **Order: indexed files, then CICS, then SQL.**

## Where it stands

Parse, don't run is built: EXEC SQL and EXEC CICS are read and checked. Indexed and relative files
are built. CICS Tiers 1 and 2 run as a harness (`ironwork cics`): program control, exception
conditions, time and storage services, temporary-storage and transient-data queues, and file
control over VSAM, with the choices recorded as assumptions C22 to C27. Tier 3 runs too: BMS maps
(COPY of a mapset gives its symbolic map), SEND MAP and RECEIVE MAP on a 3270 display, played from
a script or served over TN3270 to a real emulator (assumptions C28 to C33). SQL is next, record and
replay first. Reaching an EXEC SQL statement at run time still ends the run, naming the command.

## Verification

CICS and SQL behaviour is settled from IBM's documentation, and then from a z/OS system whose terms
of use permit it, or a client's test region. Until one exists, every choice is recorded as an
assumption, as the numeric model's are.
