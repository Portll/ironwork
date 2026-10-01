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

    cargo run -p ironwork -- run program.cbl [-silent] [-strict-sort-keys] [-warnings-block] [--fastsrt-adv-print=exclude|include] [-debug] [--cics-return-warning=once|always|never] [-I copylib]... [-L proglib]... [--dd NAME=path[:text]]... [--clock 2026-09-27T12:00:00]
    cargo run -p ironwork -- check program.cbl [-warnings-block] [--cics-return-warning=once|always|never] [-I copylib]...

CBL and PROCESS cards set the options. COPY members are found in the program's own directory, then
each `-I` library: a copybook (`.cpy`, `.copy`) in any of them before a program source (`.cbl`,
`.cob`), and either before a file named as the member alone, which a literal name tries first; the
program being compiled is never its own member. CALL finds a program among the others in the same
source, then in the program's directory and each `-L` library, by name; a dynamic CALL can name only
such a member, never a path.
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
PROCESS card choose who does the I/O of USING and GIVING files, as on z/OS. Where DFSORT does it, a
print file's records are written as the SD holds them, with no printer control character
(assumption S15). IBM's rules leave open whether DFSORT may have a print file under ADV, whose data
set's records are a byte longer than its FD's: `--fastsrt-adv-print=exclude` (the default) leaves
it to COBOL, and `--fastsrt-adv-print=include` gives it to DFSORT, which reads the control
character as each record's first byte and pads, cuts or refuses records as its rules for record
lengths say (S16 and S17).

A sequential file that a WRITE ... ADVANCING names, whose FD has LINAGE, or that holds a report is a
print file: each record written to it carries a printer control character, ASA when every WRITE ...
ADVANCING of the file says AFTER and a machine code when one says BEFORE, as Enterprise COBOL
chooses. Under ADV (the default) the character is a byte before the record, so the DD's records are
a byte longer than the FD's; NOADV on a CBL or PROCESS card makes it the record's first byte. A
`:text` DD shows the characters as line feeds, form feeds and carriage returns. Assumptions C40 to
C43 hold what the manuals leave open.

LINAGE gives a print file logical pages: OPEN OUTPUT or EXTEND reads the page body, footing line
and margins from the FD's integers or data items, and each new page reads the data items again.
LINAGE-COUNTER (qualified by the file-name when two FDs have LINAGE) is the line of the page body
the printer is at; a WRITE that would pass the page body, or ADVANCING PAGE, moves the paper in
lines past the bottom and top margins to the next page's first line, and AT END-OF-PAGE and NOT AT
END-OF-PAGE run once the line is written. Assumptions C70 to C76 hold what the manuals leave open.
A print file opened I-O reads past its control characters and keeps them when a record is
rewritten.

The cards also take APOST, which makes QUOTE an apostrophe; CURRENCY(literal), the currency symbol
in place of $ where no CURRENCY SIGN clause gives one; NSYMBOL(DBCS), under which N literals and
PICTURE N items with no USAGE would be DBCS, which ironwork refuses; DISPSIGN(SEP), which puts a
signed binary, packed or overpunched zoned item's sign before its digits on DISPLAY; INTDATE(LILIAN),
which counts the date functions' integer dates from 15 October 1582 and turns CALL 'CEECBLDY' into
CEEDAYS; QUALIFY(EXTEND), under which a complete set of qualifiers names its one item; INITIAL,
which starts every program from its VALUE clauses on each CALL, and which THREAD drops; and
VLR(COMPAT), under which a READ checks a variable-length record only against RECORD VARYING.
VSAMOPENFS is read and kept, but no OPEN in ironwork reaches the verified open it changes.
Assumptions C210 to C220 hold what the manuals leave open.

Exit status: RETURN-CODE when the run ends normally; for `check`, and for a run the compile refuses,
the compile's return code (below); 16 an abend, whose message names the system completion code
(S0C7 for a data exception, S0C4 for a LINKAGE item with no address, S806 for a program CALL cannot
find, S0CB, S0C9 or S0CF for a zero divisor no ON SIZE ERROR takes, as the division is decimal,
binary or floating-point, assumption C55), the user completion code (U0999 from CEE3ABD, U4038 for
a Language Environment condition nothing handled) or the file status of an unhandled I/O failure; 2
usage.

    cargo run -p ironwork -- job payroll.jcl --datasets data[:text] [--proclib procs]... [-L proglib]... [-I copylib]... [--clock 2026-09-27T12:00:00] [--sql-replay calls.txt]

`ironwork job` reads one job's JCL and runs its steps in order. Each EXEC PGM= runs a COBOL program
found in a `-L` library as PGM.cbl or PGM.cob, IEFBR14, IEBGENER without control statements (SYSUT1
copied to SYSUT2 as it stands, or return code 12 without either DD), IDCAMS with DELETE, REPRO,
DEFINE CLUSTER and GDG, SET, IF and DO, whose IDC messages go to SYSPRINT, or SORT (and ICEMAN):
SORT, MERGE and COPY with FIELDS in DFSORT's CH, AC, ZD, CLO, CSL, CST, PD, BI and FI formats and
SUM FIELDS=NONE, over `rt::sort`. A DD's RECFM and LRECL (alone or in DCB) give its records; a text
data set's lines are sorted as EBCDIC, so CH keys collate as on z/OS. Data sets live in the
`--datasets` directory: DSN=A.B is the file A.B there and DSN=A.B(M) the file M in the directory
A.B, a partitioned data set being a directory of members. They hold z/OS records, fixed or variable
behind 4-byte RDWs, or UTF-8 lines with `:text`; in-stream data and SYSOUT are always lines. DD
DUMMY and DSN=NULLFILE are an empty input, an unnamed DD concatenates to the one before it, and
&&NAME is a temporary data set that lasts until the job ends. Procedures are expanded: in-stream
ones, and cataloged ones found in the data sets JCLLIB ORDER names and then each `--proclib`
directory, with symbolic parameters (the EXEC's over the PROC's defaults over SET), INCLUDE members,
PARM and COND overrides and DD overrides; a step in a procedure is stepname.procstepname.
DSN=*.stepname.ddname names an earlier DD's data set. A generation data group's base is the file
BASE that DEFINE GDG writes and generation n the file BASE.GnnnnV00: (0), (-1) and (+1) count from
the generations the job began with, DSN=BASE reads them all newest first, and a kept new generation
rolls the oldest off past LIMIT, or all but itself under EMPTY.

DISP=NEW creates the data set when the step starts; OLD and SHR need it to exist; MOD writes after
what it holds, or creates it as NEW would where it is not there; and a data set
that must exist and does not, or that DISP=NEW names and that exists, is a JCL error that ends the
job. As a step ends its normal disposition applies, or its abnormal one after an abend: DELETE
removes the data set, KEEP, CATLG and UNCATLG keep it, and PASS keeps it for later steps, a data
set the job created and only passed being deleted when the job ends. With no disposition stated, a
data set the step created is deleted and one that existed is kept. COND on the JOB statement ends
the job when a test is true, COND on EXEC bypasses the step, and IF/THEN/ELSE/ENDIF nest to 15
levels over RC, stepname.RC, ABEND, ABENDCC=, stepname.ABEND and stepname.RUN. After an abend a step
runs only under COND=EVEN or ONLY, or in the branch of an IF that tests an abend or whether a step
ran. A program no library holds abends S806. A step's DISPLAY output and SYSOUT DDs go to standard
output, and a line per step to standard error: the step, the program and RC=nnnn, ABEND and its
code, BYPASSED and why, or JCL ERROR. PARM, DFSORT's INCLUDE, OMIT, INREC, OUTREC and OUTFIL, and
IBM's other programs are refused by name before any step runs. Exit status: the highest
return code; 16 when a step abended or a JCL error ended the job; 2 for a job refused.
`--expected DATASETS=DIR` runs the job on a copy of the data sets and compares what it leaves with
production's, as [docs/evidence.md](docs/evidence.md) §4 describes.

## Compiler messages

Each message has one of IBM's five severities, and a compile's return code is the highest of its
messages', 0 when there is none (Enterprise COBOL Programming Guide SC27-8714-03, Table 38, p. 282):

| Severity | Return code | `run` and `cics` |
|---|---|---|
| I, informational | 0 | run the program |
| W, warning | 4 | run the program; refuse under `-warnings-block` |
| E, error | 8 | run the program |
| S, severe | 12 | refuse |
| U, unrecoverable | 16 | refuse |

Every refusal ironwork makes is S (assumption C45). A class definition, or a program with INVOKE or
object references, compiled without THREAD, DLL, RENT or DBCS, or with NORENT beside THREAD or DLL,
is W (J19). `ironwork check` exits with the return code. `ironwork run` and `ironwork cics` print
the messages, then run the program at 0, 4 or 8, and otherwise exit with the return code without
running anything, as IBM's IGYWCLG procedure runs its GO step only up to 8 and the default
NOCOMPILE(S) produces object code after E-level messages (C46).

A CBL or PROCESS card's COMPILE option moves the refusal: NOCOMPILE(W), NOCOMPILE(E) or
NOCOMPILE(S) (abbreviated NOC) refuses from the first message of that severity, COMPILE (C) from S
as NOCOMPILE(S) does, since IGYWCLG would bypass its GO step above 8 whatever the object code, and
NOCOMPILE alone is a syntax check that runs nothing. `-warnings-block` is ironwork's command-line NOCOMPILE(W), and a card's COMPILE or
NOCOMPILE wins over it, as IBM's PROCESS statements outrank the compiler's invocation. Neither
changes the return code (C47).

A program with no STOP RUN, GOBACK or EXIT PROGRAM gets IBM's IGYPS2091-W, a warning that it may
run past its end. One that leaves by EXEC CICS RETURN or XCTL, which the CICS translator turns into
a CALL, is exempt unless asked: `--cics-return-warning=once` (the default) gives an informational
note in place of the warning, once in a run; `=always` gives the warning, return code 4; `=never`
gives nothing. Whether Enterprise COBOL warns such a program is open until an IBM listing settles it
(C124).

Messages go to standard error, one to a line: errors first, then warnings, then informational
messages, each in the order ironwork found them.

    path:line:col: message                   E, S or U
    path:line:col: warning: message          W
    path:line:col: informational: message    I
    path: message                            the same three, for a message with no position
    path: warning: message
    path: informational: message

An error's line carries no severity: E, S and U lines look alike, and the exit status is the
highest. `path` is the program as given, or the COPY member the position is in. An error's message
never begins with `warning:` or `informational:`, so a parser can take the word after the position
as the severity when it is one of those two. For example:

    client.cbl:12:17: Y is not defined
    client.cbl: warning: program CLIENT uses object-oriented syntax, which IBM compiles only with THREAD, DLL, RENT and DBCS: THREAD, DLL missing from its CBL or PROCESS cards (see J13 and J19)

## Crates

| Crate | What it models | Settled by |
|---|---|---|
| `zarch` | The machine. 21 single-byte EBCDIC code pages; PACK, UNPK, ZAP, AP, SP, MP, DP, CP, SRP, CVB, CVD, TP; hexadecimal floating point (short, long, extended) with the guard digit, truncation and exponent exceptions. | *z/Architecture Principles of Operation*; Hercules for anything in doubt |
| `numeric` | IBM's compiler. The option vector, binary stores under each TRUNC, NUMPROC sign handling, ARITH intermediate precision, float conversions. | Enterprise COBOL, through the oracle |
| `oracle` | The test harness: COBOL programs that pin their options on a CBL card, DISPLAY each case's storage in hex, and are scored against the model's predictions. | — |
| `syntax` | Fixed-format source (sequence area, indicators, continuation, CBL and PROCESS cards), the lexer and the parser. | — |
| `exec` | WORKING-STORAGE laid out byte for byte (USAGE, PICTURE, REDEFINES, OCCURS), and an interpreter over it. | The oracle programs, which it runs |
| `jcl` | Job control language: JOB, EXEC and DD statements, dispositions, in-stream data, COND and IF, procedures and symbolic parameters, and IDCAMS commands. | *z/OS MVS JCL Reference*; *DFSMS Access Method Services Commands* |
| `ironwork` | The driver. | — |

The subset the interpreter runs today:

- **Source:** fixed format with sequence numbers, continuation, `*>` comments, CBL and PROCESS
  cards, and COPY with REPLACING (whole words, pseudo-text, `==:TAG:==` inside words, LEADING,
  TRAILING), nested.
- **Data:** WORKING-STORAGE, LOCAL-STORAGE, FILE SECTION and LINKAGE SECTION items in DISPLAY, BINARY, COMP-5,
  PACKED-DECIMAL, COMP-1, COMP-2, NATIONAL, POINTER and INDEX; numeric-edited and
  alphanumeric-edited PICTUREs (zero suppression, `*`, floating `$ + -`, CR, DB, insertion, BLANK
  WHEN ZERO); scaling positions P at either end of the digits; VALUE, REDEFINES, OCCURS with KEY,
  INDEXED BY and DEPENDING ON, SIGN, SYNCHRONIZED with IBM's slack bytes before an item and after
  each occurrence of a table, level-66 RENAMES of one item or a THRU range, and level-88
  conditions with THRU ranges and WHEN SET TO FALSE. SPECIAL-NAMES DECIMAL-POINT IS COMMA
  exchanges the comma and the period in PICTUREs, numeric literals and NUMVAL and NUMVAL-C, and
  CURRENCY SIGN clauses, with or without PICTURE SYMBOL, give the currency symbols and the values
  editing inserts (assumption C102), for the program and the programs it contains. Numeric PICTUREs and literals hold at most 18 digits
  under ARITH(COMPAT) and 31 under ARITH(EXTEND). A zoned item longer than one PACK takes, up to
  31 digits, is packed in parts (assumption C34). A group that holds the object of its own OCCURS
  DEPENDING ON receives data at its maximum length, as IBM lists for MOVE, ACCEPT, STRING,
  UNSTRING, READ and RETURN INTO, and WRITE, REWRITE and RELEASE FROM. Assumptions C90 to C97
  hold what the manuals leave open about these.
- **Collating sequences:** SPECIAL-NAMES ALPHABET (EBCDIC, NATIVE, STANDARD-1, STANDARD-2, or
  literals with THROUGH and ALSO) and OBJECT-COMPUTER PROGRAM COLLATING SEQUENCE, which a contained
  program shares. The program's sequence orders alphanumeric relation and condition-name
  conditions, EVALUATE, SEARCH ALL, MAX and MIN, and gives HIGH-VALUE, LOW-VALUE, CHAR and ORD;
  national and numeric comparisons keep their own order. The choices are assumptions C35 to C37.
- **Procedure:** sections and paragraphs; MOVE (with editing and de-editing), COMPUTE, ADD,
  SUBTRACT, MULTIPLY, DIVIDE (GIVING, REMAINDER, ROUNDED, ON SIZE ERROR), IF, EVALUATE (ALSO,
  THRU, ANY, TRUE/FALSE, OTHER), PERFORM (procedures, sections, THRU, TIMES, UNTIL, VARYING with
  up to six AFTER phrases on a performed procedure, inline), EXIT PARAGRAPH/SECTION/PERFORM
  [CYCLE], NEXT SENTENCE, STRING, UNSTRING, INSPECT (TALLYING, also of a function's value: C190;
  REPLACING, CONVERTING, BEFORE/AFTER INITIAL), SEARCH and SEARCH ALL (a binary search on the table's keys, as IBM's is, so an unsorted
  table misses what a serial search finds), DISPLAY [UPON] [WITH] NO ADVANCING (to standard output,
  as a z/OS UNIX program writes it: assumption C53), ACCEPT (SYSIN, DATE, DAY,
  DAY-OF-WEEK, TIME), INITIALIZE, SET (condition TO TRUE or FALSE, index TO/UP BY/DOWN BY,
  pointer TO ADDRESS OF/NULL, ADDRESS OF TO pointer), GO TO [DEPENDING ON], ALTER and the altered GO TO (put
  back by CANCEL, IS INITIAL and entry to an independent segment: assumption C52), GOBACK, STOP
  RUN; subscripts, reference modification, LENGTH OF, ADDRESS OF, and the intrinsic functions of
  Enterprise COBOL 6.4, with table arguments written with ALL subscripts; the Unicode functions
  read an alphanumeric argument as UTF-8 and a national one as UTF-16, and WHEN-COMPILED gives the
  build's SOURCE_DATE_EPOCH when it is set and the time of the compile otherwise. The
  floating-point functions (SQRT, LOG, SIN and the rest, assumption C111) are
  computed to 128 bits and rounded to long or extended HFP (C110); RANDOM is a generator of
  ironwork's choosing (C54). JSON GENERATE, with COUNT, NAME, SUPPRESS, CONVERTING, INDICATING,
  ENCODING and ON EXCEPTION, sets JSON-CODE (C117). XML PARSE reports z/OS XML System Services'
  events to its processing procedure, in segments through END-OF-INPUT, with XML-TEXT, XML-NTEXT and
  the namespace registers, XMLSS's codes in XML-CODE, and past an undeclared prefix when the
  procedure resets XML-CODE (C118). XML GENERATE, with COUNT, ENCODING,
  XML-DECLARATION, ATTRIBUTES, NAMESPACE and its prefix, NAME, TYPE, SUPPRESS and ON EXCEPTION, sets
  XML-CODE (C119). JSON PARSE, with NAME and OMITTED, SUPPRESS, CONVERTING, INDICATING, IGNORING and
  ENCODING, moves each matched value by MOVE's rules and sets JSON-CODE and JSON-STATUS (C200). What is not Enterprise COBOL is refused as such:
  `<>`, literals joined with `&`, SET ENVIRONMENT and ACCEPT ... FROM ENVIRONMENT.
- **Subprograms:** several and nested programs per source; CALL (static and dynamic) USING BY
  REFERENCE, BY CONTENT, BY VALUE and OMITTED, RETURNING, ON EXCEPTION; PROCEDURE DIVISION USING
  and RETURNING; ENTRY [USING], whose name a CALL begins at and whose USING list alone gives LINKAGE
  addresses, a static CALL entering the program's one copy and a dynamic CALL (an identifier, or a
  literal under DYNAM) a copy of its own for each entry name (assumptions C50 and C51); CANCEL; IS
  INITIAL and IS RECURSIVE; EXIT PROGRAM; RETURN-CODE. Every program in
  a run shares one memory, as on z/OS, and a called program keeps its WORKING-STORAGE and open files
  between CALLs until it is cancelled; its LOCAL-STORAGE starts afresh on every CALL. PERFORMs and
  CALLs nest at most 100 deep.
- **Files:** sequential, line-sequential, indexed (VSAM KSDS) and relative (RRDS):
  SELECT/ASSIGN/FILE STATUS, ORGANIZATION, ACCESS SEQUENTIAL/RANDOM/DYNAMIC, RECORD KEY, ALTERNATE
  RECORD KEY [WITH DUPLICATES], RELATIVE KEY; FD with RECORDING MODE F or V and RECORD
  CONTAINS/VARYING; OPEN INPUT/OUTPUT/EXTEND/I-O; READ [NEXT|PREVIOUS] [INTO] [KEY IS] with AT END
  or INVALID KEY; WRITE [FROM] with ADVANCING (lines, PAGE, or a mnemonic-name for C01 to C12, CSP or
  AFP-5A), AT END-OF-PAGE or INVALID KEY; REWRITE, DELETE and START with INVALID KEY; CLOSE;
  OPTIONAL files, and the file status codes for each outcome. FD LINAGE with FOOTING, TOP and
  BOTTOM, integers or data items, and LINAGE-COUNTER; a mnemonic-name ADVANCING on a LINAGE file,
  and LINAGE on a report file, are not supported yet. A record can be qualified by its file-name. A
  sequential file opened I-O can be REWRITTEN in place.
  The files of a SAME RECORD AREA clause share one record area, and so do the VSAM files of a SAME
  AREA clause.
- **Declaratives:** USE AFTER STANDARD EXCEPTION/ERROR PROCEDURE on files or on INPUT, OUTPUT, I-O
  or EXTEND. When a statement on a file fails, or meets AT END or INVALID KEY with no phrase for
  it, the file's own procedure runs, else the one for the mode it is open in, once FILE STATUS
  holds the status; control then returns after the statement, and the failure no longer ends the
  run. They serve the files of a SORT's USING and GIVING too, and keep those files from FASTSRT.
  USE FOR DEBUGGING on procedures or ALL PROCEDURES, with DEBUG-ITEM, and debugging lines (D in
  column 7), under SOURCE-COMPUTER ... WITH DEBUGGING MODE; without it both are comments. The
  debugging sections run only under `-debug`, standing for the Language Environment option DEBUG,
  as on z/OS, where NODEBUG is the default; a section runs before each procedure it serves and
  after each ALTER of one. USE GLOBAL serves its own program only, so GLOBAL for an open mode, or
  before reporting a group of a contained program, is refused in a program that contains others.
  Assumptions C60 to C69 and C98 hold what the manuals leave open.
- **Sort and merge:** SD files; SORT and MERGE on ascending and descending keys anywhere in the
  record (alphanumeric keys by the COLLATING SEQUENCE phrase, else for a file by the program
  collating sequence, else in EBCDIC order; zoned and packed keys as DFSORT compares them; other
  numeric keys by value), WITH DUPLICATES IN ORDER, USING and GIVING files or INPUT and OUTPUT
  PROCEDURE with RELEASE and RETURN, and FASTSRT; SORT of a table by its keys; SORT-RETURN and the
  other sort special registers. Records are sorted in memory,
  and records with equal keys keep their input order. A DD holding sort control statements
  (IGZSRTCD) stops the run.
- **Report Writer**, run as the output of IBM's COBOL Report Writer Precompiler would run, since
  Enterprise COBOL takes a REPORT SECTION only through that precompiler: FD REPORT IS; RD with
  CONTROLS (FINAL included), PAGE LIMIT, HEADING, FIRST DETAIL, LAST DETAIL, FOOTING, LINE LIMIT
  and a literal CODE; report groups of every TYPE with LINE (absolute, PLUS, NEXT PAGE), NEXT
  GROUP, COLUMN (absolute, PLUS, RIGHT, CENTER), PICTURE with editing, SOURCE (an identifier or an
  arithmetic expression, ROUNDED), VALUE, SUM with UPON and RESET ON, GROUP INDICATE, BLANK WHEN
  ZERO, JUSTIFIED and SIGN; PAGE-COUNTER and LINE-COUNTER; INITIATE, GENERATE of a DETAIL group or
  of the report (summary reporting), and TERMINATE, with control footings minor to major and
  headings major to minor, and page footing and heading on each new page; DECLARATIVES holding USE
  [GLOBAL] BEFORE REPORTING, with SUPPRESS PRINTING and PRINT-SWITCH. Each line is a WRITE AFTER ADVANCING
  to the report's file, so its records carry ASA control characters, the CODE after the character.
  The precompiler's extensions (OCCURS, PRESENT WHEN, multiple LINES and COLUMNS, OR PAGE, STYLE,
  FUNCTION and the rest) are refused by name; assumptions RW1 to RW13 hold what the manuals leave
  open.
- **Object-oriented COBOL,** as Enterprise COBOL has it for Java interoperability: class
  definitions (CLASS-ID ... INHERITS, the REPOSITORY paragraph, FACTORY and OBJECT paragraphs with
  their WORKING-STORAGE, METHOD-ID with PROCEDURE DIVISION USING BY VALUE and RETURNING), USAGE
  OBJECT REFERENCE, INVOKE (NEW, a method named by a literal or a data item, SELF, SUPER, USING BY
  VALUE, RETURNING, ON EXCEPTION), SET and = or NOT = on object references, EXIT METHOD,
  FUNCTION-POINTER and PROCEDURE-POINTER items, JNIENVPTR, COPY JNI and Z'...' literals. Classes
  written in COBOL run in the run unit: NEW gives an object its instance data from the VALUE
  clauses, factory data is one copy per class, a method's WORKING-STORAGE persists between
  invocations, and INVOKE finds a method by its name and Java signature up the INHERITS chain, as
  the JNI does. Object references are the JNI's local and global references: those a method
  receives, gets back, makes with NEW or holds as SELF are freed when it returns, unless
  NewGlobalRef makes a global one; DeleteLocalRef, DeleteGlobalRef, PushLocalFrame and
  PopLocalFrame free them as the JNI does; and using a freed one ends the run with an abend that
  says where it was made and where it was freed. A class is found as a CALLed program is, among the
  programs read and in the program libraries (Account.cbl for Account or com.acme.Account). Java
  classes are checked, not run: reaching one ends the run with abend JAVA naming the class and
  method, while java.lang.Object's NEW and equals, and the JNI's reference services, run without a
  JVM. IBM compiles a class definition, or a program with INVOKE or object references, with THREAD
  and DLL on a CBL or PROCESS card (RENT and DBCS are the defaults); without them it compiles with a
  warning and runs. A program compiled with THREAD is RECURSIVE and has no INITIAL, nested program,
  or SORT or MERGE of a file; otherwise it is refused. The choices are assumptions J1 to J20.

- **EXEC SQL, EXEC CICS and EXEC DLI** are read and checked: every SQL host variable and every CICS
  or DL/I argument that names data must resolve; EXEC SQL INCLUDE works as COPY; a program with EXEC
  CICS gets DFHEIBLK and DFHCOMMAREA as the translator adds them; `DFHRESP(condition)` is its
  EIBRESP number; SQLCA, SQLDA, DFHEIBLK, DFHAID and DFHBMSCA are built in when no library holds
  them. An EXEC DLI command and its options are checked against IMS's table, a WHERE qualification's
  form too, and the program gets the DL/I interface block, DIBSTAT and the rest (C201); a run ends
  when it reaches one.
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
  STARTBR, READNEXT, READPREV, RESETBR and ENDBR). Without a screen script the task ends with
  RETURN TRANSID's COMMAREA written out, so a pseudo-conversation runs one task at a time.
- **BMS maps and a 3270 terminal.** COPY of a mapset reads `NAME.bms` (DFHMSD, DFHMDI, DFHMDF) from
  the copy libraries and gives the symbolic map the BMS assembly would; DFHAID and DFHBMSCA carry
  their values. SEND MAP (ERASE, MAPONLY, DATAONLY, CURSOR, symbolic cursor, FREEKB, ALARM, FRSET),
  RECEIVE MAP (MAPFAIL, JUSTIFY, EIBAID, EIBCPOSN), SEND CONTROL and RECEIVE work on a 3270
  display that speaks the 3270 data stream. `--screens FILE` plays an operator from a script
  (`type ROW COL text`, `eof`, `cursor`, then an AID key) and prints every screen; `--serve
  HOST:PORT` is a TN3270 server a 3270 emulator such as c3270 or x3270 connects to. Both run
  pseudo-conversations task after task on one screen (`--transaction TRAN=PROGRAM` names the
  programs RETURN TRANSID leads to, and `--csd FILE` takes them from the region's DEFINE
  TRANSACTIONs); a script's next AID key starts the next task.
  The choices made without a z/OS to observe are assumptions C28 to C33.
- **Language Environment callable services:** a CALL that finds no program of the name reaches
  the service. CEE3ABD ends the run with user abend U*abcode*; CEEDAYS, CEEDATE, CEEDATM,
  CEESECS and CEEDYWK convert between text, Lilian days and Lilian seconds (a COMP-2, in HFP) by
  picture strings of years, months and month names, days, day of year and weekday names, hours,
  minutes, seconds, fractions and AM/PM; CEELOCT, CEEGMT, CEEUTC and CEEGMTO read the `--clock`,
  taking local time as UTC; CEEMOUT writes to DD SYSOUT and CEE3DMP to DD CEEDUMP, or both to
  standard error; CEEGTST and CEEFRST get and free heap storage. Each returns its 12-byte
  feedback code, and with the feedback code OMITTED a failure ends the run with U4038. A CALL
  that passes fewer arguments than the service takes, as `CALL 'CEE3ABD'` with no USING does,
  ends the run with ironwork's own abend, since what z/OS does then is unpredictable. COPY
  CEEIGZCT, when no library holds it, names the 723 symbolic feedback codes of the Language
  Environment Runtime Messages, written from IBM's manuals rather than taken from IBM's member.
  Any other LE service ends the run S806, which names it as one ironwork does not provide yet. In
  a CICS task, CEE3ABD is a transaction abend with *abcode* as its four-digit ABCODE, and CEEMOUT
  and CEE3DMP write to transient data queue CESE instead of any DD. The choices are assumptions
  L1 to L18.

Anything else is refused by name at compile time.
SSRANGE is honoured, including for OCCURS DEPENDING ON counts; without it a subscript can reach
anywhere in the run unit's storage, as on z/OS, but never outside it.

`tools/census.py` runs `ironwork check` over a sample of a COBOL corpus and tallies why programs are
refused, which is how the next gaps are chosen. A program checked with warnings alone, return code 4,
counts as compiling.

`tools/differ.py` runs each program under `ironwork run` and compiled by GCC's gcobol, and reports
where what DISPLAY wrote, the return code or an abend differ. gcobol keeps storage in ASCII and has
its own numeric model, so it is no oracle: a difference is behaviour that changes when a program
leaves z/OS, or an ironwork bug, and Enterprise COBOL settles which. `tools/gcobol/` builds gcobol 16
in Docker for arm64 or x86-64 Linux, natively on an Apple silicon Mac, with a wrapper that runs it
at the host's paths; install it as `gcobol`, and as `gcobol-exec` to run what it links:

    docker build -t gcobol:16 tools/gcobol
    ln -s "$PWD/tools/gcobol/gcobol" ~/.local/bin/gcobol; ln -s gcobol ~/.local/bin/gcobol-exec
    tools/differ.py target/release/ironwork programs/ --exec gcobol-exec --stdin sysin.txt

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

Goldens are IBM's outputs, so they live in a private repository cloned here as `goldens/`, which
`.gitignore` keeps out of this one. They come from IBM Test Accelerator for Z's On-Demand
Environments (Enterprise COBOL 6.4, Db2 13.1, CICS 6.2) on a Linux x86-64 host.

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
then leaves alone; a print file whose records DFSORT reads or writes otherwise than COBOL, naming
the `--fastsrt-adv-print` choice for one under ADV; and, under FASTSRT, each USING or GIVING file
IBM's rules keep from DFSORT, with the reason. The flag `-silent` suppresses the reports and changes
nothing else.

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
