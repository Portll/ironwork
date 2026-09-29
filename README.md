# ironwork for COBOL

ironwork for COBOL is a COBOL compiler in Rust whose target is IBM Enterprise COBOL for z/OS, byte
for byte: EBCDIC storage, packed and zoned decimal as the z/Architecture decimal instructions treat
them, hexadecimal floating point, and the ARITH, TRUNC, NUMPROC and CODEPAGE options honoured as
IBM's compiler honours them.

What exists: a model of what the machine and IBM's compiler do with the bytes, an oracle that tests
that model against the real compiler, and a front end and interpreter that run a first subset of
COBOL on EBCDIC storage through that model. Code generation does not exist yet.

Install it from whichever registry you already use; each gives you the `ironwork` command:

    cargo install ironwork
    pip install ironwork
    npm install -g @portll/ironwork

The PyPI and npm packages carry builds for Linux (static, x64 and arm64), macOS (arm64 and x64) and
Windows (x64). The same builds are attached to each [release](https://github.com/Portll/ironwork/releases).
From a checkout:

    cargo run -p ironwork -- run program.cbl [-silent] [-strict-sort-keys] [-I copylib]... [-L proglib]... [--dd NAME=path[:text]]... [--clock 2026-09-27T12:00:00]
    cargo run -p ironwork -- check program.cbl [-I copylib]...

CBL and PROCESS cards set the options. COPY members are found in the program's own directory, then
each `-I` library. CALL finds a program among the others in the same source, then in the program's
directory and each `-L` library, by name; a dynamic CALL can name only such a member, never a path.
`ASSIGN` names a DD, and a program reaches only the files its DDs are given, by `--dd` or `DD_NAME`
in the environment, as JCL gives them on z/OS; DD SYSIN is what ACCEPT reads, standard input
otherwise. An indexed or relative file's DD holds its records in key order, as an IDCAMS REPRO
unload of the cluster does (a relative file's empty slot is a record of zero bytes); the file is
held in memory from OPEN to CLOSE, and CLOSE writes it back when it changed. Any DD can be
`:text`, UTF-8 lines converted through the code page, which suits fixtures written by hand. `--clock` fixes the time ACCEPT FROM DATE/TIME and FUNCTION CURRENT-DATE report, which is
otherwise the system clock in UTC.

A SORT or MERGE compares zoned and packed keys as DFSORT compares ZD and PD fields, so no bytes
in a key are a data exception; `-strict-sort-keys` reads each key as the program would instead, so
a key that is not a valid number abends S0C7. FASTSRT and NOFASTSRT (the default) on a CBL or
PROCESS card choose who does the I/O of USING and GIVING files, as on z/OS.

Exit status: RETURN-CODE when the run ends normally; 12 compile errors; 16 an abend, whose message
names the system completion code (S0C7 for a data exception, S0C4 for a LINKAGE item with no
address, S806 for a program CALL cannot find), the user completion code (U0999 from CEE3ABD, U4038
for a Language Environment condition nothing handled) or the file status of an unhandled I/O
failure; 2 usage.

## Crates

| Crate | What it models | Settled by |
|---|---|---|
| `zarch` | The machine. 21 single-byte EBCDIC code pages; PACK, UNPK, ZAP, AP, SP, MP, DP, CP, SRP, CVB, CVD, TP; hexadecimal floating point (short, long, extended) with the guard digit, truncation and exponent exceptions. | *z/Architecture Principles of Operation*; Hercules for anything in doubt |
| `numeric` | IBM's compiler. The option vector, binary stores under each TRUNC, NUMPROC sign handling, ARITH intermediate precision, float conversions. | Enterprise COBOL, through the oracle |
| `oracle` | The test harness: COBOL programs that pin their options on a CBL card, DISPLAY each case's storage in hex, and are scored against the model's predictions. | — |
| `syntax` | Fixed-format source (sequence area, indicators, continuation, CBL and PROCESS cards), the lexer and the parser. | — |
| `exec` | WORKING-STORAGE laid out byte for byte (USAGE, PICTURE, REDEFINES, OCCURS), and an interpreter over it. | The oracle programs, which it runs |
| `ironwork` | The driver. | — |

The subset the interpreter runs today:

- **Source:** fixed format with sequence numbers, continuation, `*>` comments, CBL and PROCESS
  cards, and COPY with REPLACING (whole words, pseudo-text, `==:TAG:==` inside words, LEADING,
  TRAILING), nested.
- **Data:** WORKING-STORAGE, LOCAL-STORAGE, FILE SECTION and LINKAGE SECTION items in DISPLAY, BINARY, COMP-5,
  PACKED-DECIMAL, COMP-1, COMP-2, NATIONAL, POINTER and INDEX; numeric-edited and
  alphanumeric-edited PICTUREs (zero suppression, `*`, floating `$ + -`, CR, DB, insertion, BLANK
  WHEN ZERO); VALUE, REDEFINES, OCCURS with KEY, INDEXED BY and DEPENDING ON, SIGN, and level-88
  conditions with THRU ranges.
- **Procedure:** sections and paragraphs; MOVE (with editing and de-editing), COMPUTE, ADD,
  SUBTRACT, MULTIPLY, DIVIDE (GIVING, REMAINDER, ROUNDED, ON SIZE ERROR), IF, EVALUATE (ALSO,
  THRU, ANY, TRUE/FALSE, OTHER), PERFORM (procedures, sections, THRU, TIMES, UNTIL, VARYING,
  inline), EXIT PARAGRAPH/SECTION/PERFORM [CYCLE], NEXT SENTENCE, STRING, UNSTRING, INSPECT
  (TALLYING, REPLACING, CONVERTING, BEFORE/AFTER INITIAL), SEARCH and SEARCH ALL (a binary search
  on the table's keys, as IBM's is, so an unsorted table misses what a serial search finds),
  DISPLAY, ACCEPT (SYSIN, DATE, DAY,
  DAY-OF-WEEK, TIME), INITIALIZE, SET (condition TO TRUE, index TO/UP BY/DOWN BY, pointer TO
  ADDRESS OF/NULL, ADDRESS OF TO pointer), GO TO, GOBACK, STOP RUN; subscripts, reference
  modification, LENGTH OF, ADDRESS OF, and the functions ABS, CHAR, CURRENT-DATE,
  DATE-OF-INTEGER, INTEGER, INTEGER-OF-DATE, INTEGER-PART, LENGTH, LOWER-CASE, MAX, MIN, MOD,
  NATIONAL-OF, NUMVAL, NUMVAL-C, ORD, REM, REVERSE, TRIM and UPPER-CASE.
- **Subprograms:** several and nested programs per source; CALL (static and dynamic) USING BY
  REFERENCE, BY CONTENT, BY VALUE and OMITTED, RETURNING, ON EXCEPTION; PROCEDURE DIVISION USING
  and RETURNING; CANCEL; IS INITIAL and IS RECURSIVE; EXIT PROGRAM; RETURN-CODE. Every program in
  a run shares one memory, as on z/OS, and a called program keeps its WORKING-STORAGE and open files
  between CALLs until it is cancelled; its LOCAL-STORAGE starts afresh on every CALL. PERFORMs and
  CALLs nest at most 100 deep.
- **Files:** sequential, line-sequential, indexed (VSAM KSDS) and relative (RRDS):
  SELECT/ASSIGN/FILE STATUS, ORGANIZATION, ACCESS SEQUENTIAL/RANDOM/DYNAMIC, RECORD KEY, ALTERNATE
  RECORD KEY [WITH DUPLICATES], RELATIVE KEY; FD with RECORDING MODE F or V and RECORD
  CONTAINS/VARYING; OPEN INPUT/OUTPUT/EXTEND/I-O; READ [NEXT|PREVIOUS] [INTO] [KEY IS] with AT END
  or INVALID KEY; WRITE [FROM] with ADVANCING or INVALID KEY; REWRITE, DELETE and START with
  INVALID KEY; CLOSE; OPTIONAL files, and the file status codes for each outcome. A sequential file
  opened I-O can be REWRITTEN in place. The files of a SAME RECORD AREA clause share one record
  area, and so do the VSAM files of a SAME AREA clause.
- **Sort and merge:** SD files; SORT and MERGE on ascending and descending keys anywhere in the
  record (alphanumeric keys in EBCDIC order, zoned and packed keys as DFSORT compares them, other
  numeric keys by value), WITH DUPLICATES IN ORDER, USING and GIVING files or INPUT and OUTPUT
  PROCEDURE with RELEASE and RETURN, and FASTSRT; SORT of a table by its keys; SORT-RETURN and the
  other sort special registers. Records are sorted in memory,
  and records with equal keys keep their input order. A COLLATING SEQUENCE other than EBCDIC or
  NATIVE is refused, and a DD holding sort control statements (IGZSRTCD) stops the run.
- **Report Writer**, run as the output of IBM's COBOL Report Writer Precompiler would run, since
  Enterprise COBOL takes a REPORT SECTION only through that precompiler: FD REPORT IS; RD with
  CONTROLS (FINAL included), PAGE LIMIT, HEADING, FIRST DETAIL, LAST DETAIL, FOOTING, LINE LIMIT
  and a literal CODE; report groups of every TYPE with LINE (absolute, PLUS, NEXT PAGE), NEXT
  GROUP, COLUMN (absolute, PLUS, RIGHT, CENTER), PICTURE with editing, SOURCE (an identifier or an
  arithmetic expression, ROUNDED), VALUE, SUM with UPON and RESET ON, GROUP INDICATE, BLANK WHEN
  ZERO, JUSTIFIED and SIGN; PAGE-COUNTER and LINE-COUNTER; INITIATE, GENERATE of a DETAIL group or
  of the report (summary reporting), and TERMINATE, with control footings minor to major and
  headings major to minor, and page footing and heading on each new page; DECLARATIVES holding USE
  BEFORE REPORTING, with SUPPRESS PRINTING and PRINT-SWITCH. Each line is a WRITE AFTER ADVANCING
  to the report's file, whose record carries no printer control byte. The precompiler's extensions
  (OCCURS, PRESENT WHEN, multiple LINES and COLUMNS, OR PAGE, STYLE, FUNCTION and the rest) are
  refused by name; assumptions RW1 to RW13 hold what the manuals leave open.
- **Object-oriented COBOL,** as Enterprise COBOL has it for Java interoperability: class
  definitions (CLASS-ID ... INHERITS, the REPOSITORY paragraph, FACTORY and OBJECT paragraphs with
  their WORKING-STORAGE, METHOD-ID with PROCEDURE DIVISION USING BY VALUE and RETURNING), USAGE
  OBJECT REFERENCE, INVOKE (NEW, a method named by a literal or a data item, SELF, SUPER, USING BY
  VALUE, RETURNING, ON EXCEPTION), SET and = or NOT = on object references, EXIT METHOD,
  FUNCTION-POINTER and PROCEDURE-POINTER items, JNIENVPTR, COPY JNI and Z'...' literals. Classes
  written in COBOL run in the run unit: NEW gives an object its instance data from the VALUE
  clauses, factory data is one copy per class, a method's WORKING-STORAGE persists between
  invocations, and INVOKE finds a method by its name and Java signature up the INHERITS chain, as
  the JNI does. A class is found as a CALLed program is, among the programs read and in the program
  libraries (Account.cbl for Account or com.acme.Account). Java classes are checked, not run:
  reaching one ends the run with abend JAVA naming the class and method, while java.lang.Object's
  NEW and equals, and the JNI's reference services such as NewGlobalRef and IsSameObject, run
  without a JVM. The run-time choices are assumptions J1 to J14.

- **EXEC SQL and EXEC CICS** are read and checked: every SQL host variable and every CICS argument
  that names data must resolve; EXEC SQL INCLUDE works as COPY; a program with EXEC CICS gets
  DFHEIBLK and DFHCOMMAREA as the translator adds them; `DFHRESP(condition)` is its EIBRESP number;
  SQLCA, SQLDA, DFHEIBLK, DFHAID and DFHBMSCA are built in when no library holds them.
- **EXEC SQL runs** against PostgreSQL (`--sql-db`) or a recording of a run (`--sql-replay`, made
  with `--sql-record`): single-row statements, cursors with WITH HOLD and positioned changes, COMMIT
  and ROLLBACK, CICS SYNCPOINT, host variables and indicators converted by Db2's rules, the SQLCA
  and WHENEVER. A normal end commits and an abend rolls back. TLS to PostgreSQL is in a separate
  build, [tls/](tls/README.md), so that this one keeps no dependencies.
  [docs/sql-runtime.md](docs/sql-runtime.md) specifies it, with what Db2 12.1 for Linux settled.
- **CICS, run as a harness** (`ironwork cics`): one task, with the transaction ID, terminal, user
  and COMMAREA the command line gives, and the EXEC interface block in IBM's layout. Program
  control (RETURN with TRANSID and COMMAREA, LINK, XCTL, ABEND); exception conditions (RESP, RESP2,
  NOHANDLE, HANDLE CONDITION with ERROR, IGNORE CONDITION, PUSH and POP HANDLE, HANDLE ABEND, and
  the AEIx abend IBM documents for a condition nothing handles; a program check is ASRA); ASKTIME,
  FORMATTIME, ASSIGN, GETMAIN, FREEMAIN, ADDRESS, SYNCPOINT, ENQ, DEQ, DELAY, SEND TEXT and WRITE
  OPERATOR; temporary-storage and transient-data queues; and file control over VSAM KSDS and RRDS
  files (READ with GENERIC, GTEQ and UPDATE, WRITE, REWRITE, DELETE, UNLOCK, and browsing with
  STARTBR, READNEXT, READPREV, RESETBR and ENDBR). The task ends with RETURN TRANSID's COMMAREA
  written out, so a pseudo-conversation runs one task at a time.
- **BMS maps and a 3270 terminal.** COPY of a mapset reads `NAME.bms` (DFHMSD, DFHMDI, DFHMDF) from
  the copy libraries and gives the symbolic map the BMS assembly would; DFHAID and DFHBMSCA carry
  their values. SEND MAP (ERASE, MAPONLY, DATAONLY, CURSOR, symbolic cursor, FREEKB, ALARM, FRSET),
  RECEIVE MAP (MAPFAIL, JUSTIFY, EIBAID, EIBCPOSN), SEND CONTROL and RECEIVE work on a 3270
  display that speaks the 3270 data stream. `--screens FILE` plays an operator from a script
  (`type ROW COL text`, `eof`, `cursor`, then an AID key) and prints every screen; `--serve
  HOST:PORT` is a TN3270 server a 3270 emulator such as c3270 or x3270 connects to, running
  pseudo-conversations task after task (`--transaction TRAN=PROGRAM` names the programs RETURN
  TRANSID leads to). The choices made without a z/OS to observe are assumptions C28 to C33.
- **Language Environment callable services:** a CALL that finds no program of the name reaches
  the service. CEE3ABD ends the run with user abend U*abcode*; CEEDAYS, CEEDATE, CEEDATM,
  CEESECS and CEEDYWK convert between text, Lilian days and Lilian seconds (a COMP-2, in HFP) by
  picture strings of years, months and month names, days, day of year and weekday names, hours,
  minutes, seconds, fractions and AM/PM; CEELOCT, CEEGMT, CEEUTC and CEEGMTO read the `--clock`,
  taking local time as UTC; CEEMOUT writes to DD SYSOUT and CEE3DMP to DD CEEDUMP, or both to
  standard error; CEEGTST and CEEFRST get and free heap storage. Each returns its 12-byte
  feedback code, and with the feedback code OMITTED a failure ends the run with U4038. Any other
  LE service ends the run S806, which names it as one ironwork does not provide yet. In a CICS
  task, CEE3ABD is a transaction abend with *abcode* as its four-digit ABCODE, and CEEMOUT and
  CEE3DMP write to transient data queue CESE instead of any DD. The choices are assumptions L1
  to L15.

Anything else is refused by name at compile time.
SSRANGE is honoured, including for OCCURS DEPENDING ON counts; without it a subscript can reach
anywhere in the run unit's storage, as on z/OS, but never outside it.

`tools/census.py` runs `ironwork check` over a sample of a COBOL corpus and tallies why programs are
refused, which is how the next gaps are chosen.

`ironwork assumptions` lists the register of assumptions (`numeric::assumptions::ASSUMPTIONS`), one
per line; `--c-series` puts each entry's number in a single C series first, with its own id beside it.

## Code pages

`zarch/ucm/` holds IBM's tables as ICU publishes them, pinned to
[unicode-org/icu-data@8d9eb3e2](https://github.com/unicode-org/icu-data/tree/8d9eb3e27e79f59dd76e278e58d68b4668835027/charset/data/ucm):
CCSIDs 037, 273, 277, 278, 280, 284, 285, 297, 500, 871, 1047 and 1140–1149. `build.rs` turns them into
tables at build time and refuses a table that does not map all 256 bytes. The ICU data is under the
Unicode License v3.

Storage stays in EBCDIC; conversion happens only at I/O. Line feed is X'25' and next line X'15' in
these tables, which is right for record-oriented data sets. z/OS UNIX text files swap the two.

## The oracle

    cargo run -p ironwork-oracle -- generate out/     # ORAC01..04 .cbl and .jcl, and expected.tsv
    cargo run -p ironwork-oracle -- check goldens/    # score saved job output against the predictions
    cargo run -p ironwork-oracle -- smoke /tmp/smoke  # compile and run with GnuCOBOL: a syntax check only

Each program pins TRUNC, NUMPROC and ARITH on its CBL card rather than trusting the installation's
defaults. Generated source uses only EBCDIC-invariant characters, so the transfer code page does not
change it.

To produce goldens on a z/OS with Enterprise COBOL whose terms of use permit it, edit the JOB
card's accounting fields, then submit each job and save its complete output (the compile listing
names the compiler and its level):

    zowe zos-jobs submit local-file out/ORAC01.jcl --view-all-spool-content > goldens/<target>/ORAC01.txt

Keep one directory per target: compiler level, `ARCH` and `OPT` all change the generated code, and
with it the answers to chosen assumptions.

### Hercules, a second reading of the machine

    cargo run -p ironwork-oracle -- hercules /tmp/herc   # needs hercules (4.9.1) on PATH

This builds a bare-metal program that runs each of about two thousand cases once: PACK, UNPK, ZAP,
AP, SP, MP, DP, CP, SRP, CVB, CVD, TP and the HFP add, subtract, multiply, divide, compare, halve
and load-rounded instructions, on edge and random operands. Hercules runs it, and each result,
condition code and program interruption is compared with `zarch`. Nothing from Hercules is copied
into ironwork; it only runs.

Agreement is evidence, not proof: Hercules implements the same manual. Where the two disagree, the
manual decides. On Hercules 4.9.1 every decimal case agrees. Two HFP instructions disagree, and in
both the *Principles of Operation* (SA22-7832-14) sides with `zarch`: LOAD ROUNDED extended to long
(LDXR) rounds on the leftmost fraction bit dropped, not on the low-order characteristic (p. 18-17),
and MULTIPLY long to extended (MXDR) writes the low-order half of the product, with its
characteristic 14 below the high-order one (pp. 18-4, 18-18).

## Repository boundary

**ironwork** owns everything that defines or executes COBOL semantics: the machine and compiler
models, the conformance oracle, the front end (with its own CBL/PROCESS parsing, since that is
compiler input), the storage layout, the interpreter, and later code generation and the runtime.

**cobolwork** stays a zero-dependency analysis tool and never links ironwork. It keeps estate reading
(COBOL, JCL, CICS, BMS), option resolution across installation defaults, PARM and CBL cards for
the build gate, and its findings. What ironwork learns reaches it as analysis rules, not code:

- the static counterpart of TRUNC(OPT) checking: binary receivers whose operands can exceed the
  receiver's PICTURE, in a program compiled TRUNC(OPT);
- EBCDIC dependence: hex literals, comparisons and SORT keys whose order differs between EBCDIC and
  ASCII, and zoned items redefined as alphanumeric. All of these change meaning when a program
  moves to an ASCII compiler;
- intermediates wider than 30 digits (31 under ARITH(EXTEND)), where IBM drops digits and GnuCOBOL
  does not.

The two repositories share data only: the compiler-option table (names, abbreviations, where each
may appear) and conformance fixtures.

## Checked mode

A binary store under TRUNC(OPT) whose value exceeds the PICTURE is reported, because decimal and
binary truncation give different results and the program depends on which one the generated code
uses. So is a SORT whose outcome FASTSRT changes: a USING or GIVING file whose I/O DFSORT does, or
would do, under FASTSRT, and whose FILE STATUS (or a GIVING relative file's RELATIVE KEY) the SORT
then leaves alone; and, under FASTSRT, each USING or GIVING file IBM's rules keep from DFSORT, with
the reason. The flag `-silent` suppresses the reports and changes nothing else.

## Licence

ironwork for COBOL is published under AGPL-3.0-or-later ([LICENSE](LICENSE)) with the
[Runtime Exception](RUNTIME-EXCEPTION.md): programs you compile, check or run with ironwork are not
covered by the AGPL. PolyForm Internal Use is available for a fee, and a negotiated licence for
other uses ([LICENSING.md](LICENSING.md)). Contributions need the [CLA](CLA.md). The code-page tables
are ICU data under the Unicode License v3 ([THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md)).

## Tests

    cargo test

The instruction and HFP emulators run against random operands to show they cannot panic. The
interpreter runs the four oracle programs and must reproduce every predicted byte.

The front end is fuzzed two ways. `cargo test` mutates real programs (the oracle's, plus fragments
chosen to break the reader, lexer, parser and layout) and fails on any panic, leaving the input in
the temp directory; `IRONWORK_FUZZ_ITERATIONS=30000 cargo test -p ironwork-exec mutated` runs longer.
`fuzz/` is a coverage-guided cargo-fuzz target over the same path, seeded with the oracle programs;
it needs a nightly toolchain and `cargo install cargo-fuzz`, then `cargo +nightly fuzz run front_end`.
