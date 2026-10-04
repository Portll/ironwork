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

    cargo run -p ironwork -- run program.cbl [-silent] [-strict-sort-keys] [-warnings-block] [--fastsrt-adv-print=exclude|include] [-debug] [--cics-return-warning=once|always|never] [--compliance strict|extended] [--dialect ibm|gnucobol] [--assume ID=VALUE]... [-I copylib]... [-L proglib]... [--dd NAME=path[:text]]... [--clock 2026-09-27T12:00:00]
    cargo run -p ironwork -- check program.cbl [-warnings-block] [--cics-return-warning=once|always|never] [--compliance strict|extended] [--dialect ibm|gnucobol] [--assume ID=VALUE]... [-I copylib]...
    cargo run -p ironwork -- compile program.cbl... [-o dir] [--bundle NAME] [--source-prefix DIR] [run's compile flags] [-I copylib]...
    cargo run -p ironwork -- dump [--section NAME]... [--strings] [--no-check] program.iwm
    cargo run -p ironwork -- run program.iwm [-I copylib]... [-L proglib]... [--dd NAME=path[:text]]... [--coverage FILE] [--evidence DIR]
    cargo run -p ironwork -- cics program.iwm [-L proglib]... [--transid T] [--commarea path[:text]] [--screens script] [--coverage FILE] [--evidence DIR]

`compile` lowers each source's programs and writes them as one load module,
[docs/load-module.md](docs/load-module.md): PAYROLL.cbl gives PAYROLL.iwm in `-o`'s directory, or
every source's programs go into NAME.iwm under `--bundle NAME`. A program lowering refuses is
named with the construct and its position, and its source writes nothing. The same source,
libraries and options give the same bytes from any process or directory; a program that uses
FUNCTION WHEN-COMPILED holds the compile time, SOURCE_DATE_EPOCH's when it is set. `dump` prints a
module one fact per line, in section order, and exits 1 for a damaged one; each program's generated
code prints as a listing of its blocks, one op to a line with data names and source positions
([docs/lir.md](docs/lir.md) §13). `run program.iwm` runs a module's first program on the VM of
[docs/codegen-runtime.md](docs/codegen-runtime.md) with the options it was compiled with, CALL
finding programs in the module first, then as `NAME.iwm` or source in the program libraries;
`cics program.iwm` runs it as the first program of a CICS task. Each writes the `--coverage` report
and the `--evidence` journal a run of the source writes, from the paragraphs, lines, source files
and digests the module records ([docs/load-module.md](docs/load-module.md) §8.2, §9.2); a module
refuses the compile flags and `--provenance`, which describe a compile, with 246 (usage).

CBL and PROCESS cards set the options. COPY members are found in the program's own directory, then
each `-I` library: a copybook (`.cpy`, `.copy`) in any of them before a program source (`.cbl`,
`.cob`), and either before a file named as the member alone, which a literal name tries first; the
program being compiled is never its own member. CALL finds a program among the others in the same
source, then in the program's directory and each `-L` library, by name, and failing that in the
`.cbl` or `.cob` file there whose PROGRAM-ID it is (assumption C441); a dynamic CALL can name only
such a member, never a path. A user-defined function's definition is found the same way, by its
external name. `run` and `check` compile a source's functions and function prototypes with its first
program, which is the one a run enters even when functions come before it (assumption C270).
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
PICTURE N items with no USAGE are DBCS (C212); DISPSIGN(SEP), which puts a
signed binary, packed or overpunched zoned item's sign before its digits on DISPLAY; INTDATE(LILIAN),
which counts the date functions' integer dates from 15 October 1582 and turns CALL 'CEECBLDY' into
CEEDAYS; QUALIFY(EXTEND), under which a complete set of qualifiers names its one item; INITIAL,
which starts every program from its VALUE clauses on each CALL, and which THREAD drops; and
VLR(COMPAT), under which a READ checks a variable-length record only against RECORD VARYING.
VSAMOPENFS(SUCC) makes 00 the status of an OPEN that verifies an indexed or relative data set a
run left open for output, 97 under COMPAT; a run leaves one open only when an abend that TRAP(OFF)
in its PARM keeps from Language Environment ends it (C152).
Assumptions C210 to C220 hold what the manuals leave open. INITCHECK (or IC) warns at compile time,
return code 4, of each statement that uses a WORKING-STORAGE or LOCAL-STORAGE item no path to the
statement sets, and INITCHECK(STRICT) of each that some path leaves unset, following PERFORM, GO TO
and fall-through, and changes nothing at run time (C224, C225). NUMCHECK, with ZONECHECK as its
zoned check, tests each zoned, packed or binary item a statement reads as a sender, and reports
invalid data on the error stream (MSG) or ends the run with U4038 (ABD) (C228, C229). ZON(LAX)
tolerates the two redefinitions IBM lists, an unsigned item over a signed one and leading spaces
over an edited item's Z positions (C280), and a test the compiler finds always fails is an
error-level message when compiled and is removed (C281).
PARMCHECK(MSG|ABD,n) puts n bytes, 100 by default, after the WORKING-STORAGE a program declares and
sets them to X'AA' before each CALL; when the called program has written into them, a warning on
standard error names the parameter, the CALL's line and the program, and under ABD the run ends
with U4038 (C226, C227).

ironwork runs no operating-system command: a CALL of SYSTEM or C$SYSTEM that no library answers
abends S806, unless it prints. In a run given DD PRINTER, the virtual printer, a command that is lp
or lpr with options CUPS documents and at least one file appends each file to DD PRINTER, byte for
byte, and returns 0. Each file is a DD, as an ASSIGN literal names one (report.txt is DD
REPORT.TXT), so the command reaches only what the run was given; a file with no DD prints nothing
and returns 1, as lp does. A command the shell would do more with (quotes, `;`, `|`, `$`, a
redirection) is not a print, except that an option's value may be a double-quoted variable,
`"$NAME"`, which the shell passes as one word: the form a program uses to keep a printer's name out
of the command text. The destination, copies and title are read and not acted on. The status goes
to the CALL's RETURNING item, or else RETURN-CODE. `compare` compares DD PRINTER as it does any DD,
so a change that stops a program printing diverges.

Exit status: the program's RETURN-CODE up to 238, and 239 and above as [Exit status](#exit-status)
says.

    cargo run -p ironwork -- fuzz src/PAYROLL.cbl -o fuzz-run [--runs 200] [--seed 1] [--timeout 10] [--root .] [-I copylib]... [-L proglib]...

`ironwork fuzz` runs a batch program on generated input: the sequential and indexed files of
fixed-length records it reads, built field by field from their descriptions, and SYSIN. Each abend an input
causes is kept once by code and place, with the smallest input found that still causes it, the
journal of a run on that input and its coverage; `COBOLWORK_ABENDS=fuzz-run cobolwork scan --only
abend .` reports them as findings ([docs/evidence.md](docs/evidence.md) §5). `fuzz --cics` runs a
CICS program as a task instead, on a generated COMMAREA and an operator's generated typing into the
maps it RECEIVEs (§5.1). `fuzz --interface` runs a subprogram as a caller would, on generated
arguments for its PROCEDURE DIVISION USING items, shaped by the CALLs that pass them where the
`-L` libraries hold any (§5.2). `fuzz --differential` runs each generated input on the interpreter
and on the VM under one statement limit, and keeps each input on which they differ in
`divergence-N/`, with what each wrote and the command that repeats it; its exit status is 1 when
any input differs.

    cargo run -p ironwork -- job payroll.jcl --datasets data[:text] [--proclib procs]... [--user ID] [-L proglib]... [-I copylib]... [--clock 2026-09-27T12:00:00] [--sql-replay calls.txt]

`ironwork job` reads one job's JCL and runs its steps in order. Each EXEC PGM= runs a COBOL program
found in a `-L` library as PGM.cbl or PGM.cob, IEFBR14, IEBGENER without control statements (SYSUT1
copied to SYSUT2 as it stands, or return code 12 without either DD), IDCAMS with DELETE, REPRO,
DEFINE CLUSTER, ALTERNATEINDEX, PATH and GDG, BLDINDEX, LISTCAT, PRINT, SET, IF and DO, whose IDC
messages go to SYSPRINT, or SORT (and ICEMAN):
SORT, MERGE and COPY with FIELDS in DFSORT's CH, AC, ZD, CLO, CSL, CST, PD, BI and FI formats and
SUM FIELDS=NONE, over `rt::sort`; INCLUDE and OMIT, comparing CH, BI, FI, ZD and PD fields with
each other or with C'...', X'...' and decimal constants, joined by AND and OR; INREC and OUTREC
with BUILD, FIELDS or OVERLAY of columns, fields, blanks, binary zeros and C'...' and X'...'
strings, numeric fields edited by M0 to M26, EDIT and EDxy patterns with SIGNS and LENGTH, or
converted by TO= or a target format, and IFTHEN with WHEN=INIT, GROUP (BEGIN, KEYBEGIN, END,
RECORDS and PUSH), conditions with HIT=NEXT, and NONE; OUTFIL groups with FNAMES or FILES,
INCLUDE, OMIT or SAVE, and the same reformatting; and SYMNAMES, whose symbols stand for fields and
constants in these statements and whose table goes to SYMNOUT. An invalid digit in a field a
statement reads as a number abends the step S0C7 (assumption C340).
A DD's RECFM and LRECL (alone or in DCB) give its records; a text data set's lines are sorted as
EBCDIC, so CH keys collate as on z/OS. A COBOL program gets the step's PARM as Language Environment
passes it: its first PROCEDURE DIVISION USING item addresses a halfword length and the program
arguments, what precedes the last slash when runtime options follow it (CBLOPTS(ON); assumptions
C250 and C251), and a step with no PARM passes a length of zero; the runtime option
UPSI(nnnnnnnn) after the slash sets the UPSI switches (C411). JOBLIB and STEPLIB are not
allocated, since programs come from `-L`. Data sets live in the
`--datasets` directory: DSN=A.B is the file A.B there and DSN=A.B(M) the file M in the directory
A.B, a partitioned data set being a directory of members. They hold z/OS records, fixed or variable
behind 4-byte RDWs, or UTF-8 lines with `:text`; in-stream data and SYSOUT are always lines. DD
DUMMY and DSN=NULLFILE are an empty input, an unnamed DD concatenates to the one before it,
&&NAME is a temporary data set that lasts until the job ends, and a DD that names no data set but
asks for one (UNIT=, SPACE=) gets a new temporary one of its own. Data with no DD before it is
SYSIN, as z/OS supplies a `//SYSIN DD *` for it, and &SYSUID is the JOB statement's USER= or the
`--user` that submitted the job. Procedures are expanded: in-stream ones, and cataloged ones found
in the data sets JCLLIB ORDER names and then each `--proclib` directory, with symbolic parameters
(the EXEC's over the PROC's defaults over SET), INCLUDE members,
PARM and COND overrides and DD overrides; a step in a procedure is stepname.procstepname.
DSN=*.stepname.ddname names an earlier DD's data set. A generation data group's base is the file
BASE that DEFINE GDG writes and generation n the file BASE.GnnnnV00: (0), (-1) and (+1) count from
the generations the job began with, DSN=BASE reads them all newest first, and a kept new generation
rolls the oldest off past LIMIT, or all but itself under EMPTY.

A VSAM cluster, alternate index or path that IDCAMS defines has a catalog entry beside its name, the
file NAME.catalog-entry holding the DEFINE that made it: KEYS, RECORDSIZE and the organization for a
cluster, RELATE, KEYS, UNIQUEKEY and UPGRADE for an alternate index, PATHENTRY and UPDATE for a
path. A data set already in the directory joins the catalog through DEFINE CLUSTER with RECATALOG,
which keeps its records. A cluster's records are fixed-length when RECORDSIZE's average is its
maximum and behind RDWs otherwise, or lines with `:text`. BLDINDEX builds an alternate index's
records from its base cluster's, a five-byte header, the alternate key and the prime keys in
ascending order, with IDC1644I, IDC1645I and IDC1646I for the records it leaves out or cuts short
(C353). When a step changes a base cluster, the alternate indexes in its upgrade set are rebuilt as
the step ends (C355). A DD that names a path reads the base cluster's records in the alternate
index's order, and what a program writes there goes back to the base cluster (C351). A program's
ALTERNATE RECORD KEY reads use the base cluster's records as they stand, and the DDs the Programming
Guide asks for, the base ddname with 1, 2 and on, are not opened (C350). DELETE of a cluster removes
its alternate indexes and paths, and LISTCAT lists the directory as a catalog: by name, the
components and paths of each cluster and alternate index beneath it, or with ALL the attributes the
entries keep (C354). PRINT lists a cluster, an alternate index, a path or a sequential data set in
DUMP, HEX or CHARACTER format as the Access Method Services samples lay them out, a key-sequenced
cluster in key order, from SKIP or FROMKEY to COUNT or TOKEY (C358, C359). The listings of LISTCAT
and PRINT go to OUTFILE when it is given, and their messages to SYSPRINT.

DISP=NEW creates the data set when the step starts; OLD and SHR need it to exist; MOD writes after
what it holds, a generation's included, or creates it as NEW would where it is not there; and a
data set that must exist and does not, or that DISP=NEW names and that exists, is a JCL error that
ends the job. As a step ends its normal disposition applies, or its abnormal one after an abend:
DELETE removes the data set, KEEP, CATLG and UNCATLG keep it, and PASS keeps it for later steps, a data
set the job created and only passed being deleted when the job ends. A data set that cannot be
deleted stays, and standard error says `dsname NOT DELETED` and why, as z/OS's IEF283I does; the
step's return code is unchanged. With no disposition stated, a
data set the step created is deleted and one that existed is kept. COND on the JOB statement ends
the job when a test is true, COND on EXEC bypasses the step, and IF/THEN/ELSE/ENDIF nest to 15
levels over RC, stepname.RC, ABEND, ABENDCC=, stepname.ABEND and stepname.RUN. After an abend a step
runs only under COND=EVEN or ONLY, or in the branch of an IF that tests an abend or whether a step
ran. A program no library holds abends S806. A step's DISPLAY output and SYSOUT DDs go to standard
output, and a line per step to standard error: the step, the program and RC=nnnn, ABEND and its
code, BYPASSED and why, or JCL ERROR. IBM's other programs, PARM to a utility, DFSORT's FINDREP,
PARSE, arithmetic, date formats and SEQNUM, and the statements and parameters not named here are
refused by name before any step runs. Exit status: the highest return code, or as the first step that
ended without one, or a JCL error, says ([Exit status](#exit-status)).
`--expected DATASETS=DIR` runs the job on a copy of the data sets and compares what it leaves with
production's, as [docs/evidence.md](docs/evidence.md) §4 describes.

## Exit status

`check` and `compile` exit with the compile's return code (below), and 2 for usage. `run`, `cics`
and `job` keep 239 and above for the ends ironwork gives a run, so a CI step can tell a program's
RETURN-CODE from a run ironwork refused, stopped or could not finish:

| Status | How the run ended |
|---|---|
| 0–238 | It ran to its end: the program's RETURN-CODE, a job's highest step return code, or 0 for a CICS task. |
| 239 | It ran to its end with a RETURN-CODE outside 0–238, or of 239. Standard error gives the value (`ironwork: RETURN-CODE 1000 exits 239`), and an `--evidence` journal's `close` record holds it as `exit`. |
| 240 | An abend, which the message names: the system completion code (S0C7 for a data exception, S0C4 for a LINKAGE item with no address, S806 for a program CALL cannot find, S0CB, S0C9 or S0CF for a zero divisor no ON SIZE ERROR takes, as the division is decimal, binary or floating-point, assumption C55), the user completion code (U0999 from CEE3ABD, U4038 for a Language Environment condition nothing handled), a CICS abend code, the file status of an unhandled I/O failure, or SQL and SQLR from the database or its recording. For a job, a step's abend or a JCL error that ended it. |
| 241 | The compile gave no program to run: its return code, which standard error gives, reached the refusal level (below), a card's NOCOMPILE asked for a syntax check, or the source or module holds only user-defined functions. |
| 242 | Code generation refused a construct, named with where it is (`--vm`). |
| 243 | The VM stopped at a construct it does not run yet (`--vm`, or a module). |
| 244 | The run reached a construct ironwork does not run: an abend with one of ironwork's own codes, IRONWORK (INVOKE in a CICS task among them), EXEC (EXEC DLI) or JAVA. For a job, also JCL ironwork refuses before any step runs. |
| 245 | The source, JCL or load module cannot be read, or the reader refuses the module (damaged, or a format version it does not read). |
| 246 | Usage, or a file, directory, address or database a flag names cannot be used. |
| 255 | An internal error: ironwork panicked, or could not make a scratch directory. |

A job exits as its first step that ended without a return code says: 240 for an abend, 241 for a
program the compile refused, 244 for an IRONWORK, EXEC or JAVA abend, 245 for a source that cannot
be read. Its COND and IF tests see each of these as the abend the job log names. `job --expected`
exits with its equivalence verdict ([docs/evidence.md](docs/evidence.md) §4) and 2 for usage.

[docs/run-endings.tsv](docs/run-endings.tsv) lists the statuses from 240 up and the abend codes that
exit 244, for a tool that reads how a run ended; cobolwork vendors it, and a test holds it to
`crates/cli/src/exit.rs`.

With `--exit-code`, `run`, `cics` and `job` exit with a verdict instead, the convention cobolwork's
`--exit-code` follows:

| Status | Verdict |
|---|---|
| 0 | It ran to its end with RETURN-CODE 0. |
| 1 | It ran to its end with another RETURN-CODE, which standard error and the journal give. |
| 2 | Usage, or an input that cannot be read (246, 245). |
| 3 | An abend (240). |
| 4 | Refused: by the compile, by code generation, or a construct ironwork does not run (241, 242, 244). |
| 5 | Stopped: the VM does not run a construct yet (243). |
| 70 | An internal error (255). |

An `--evidence` journal records the same `exit` whichever table the run exits by.

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

Every refusal ironwork makes is S (assumption C45). A message from ironwork's catalogue opens with
its id: `IW`, the area's letter, four digits and the severity it was given, as `IWR0001-S` refuses
XML PARSE VALIDATING. IWX names an extension `--compliance extended` reads and IWR an Enterprise
COBOL construct ironwork does not run yet; [docs/messages.md](docs/messages.md) lists the areas and
the messages catalogued so far, and the rest carry no id yet. A class definition, or a program with INVOKE or
object references, compiled without THREAD, DLL, RENT or DBCS, or with NORENT beside THREAD or DLL,
is W (J19). `ironwork check` exits with the return code. `ironwork run` and `ironwork cics` print
the messages, then run the program at 0, 4 or 8, and otherwise exit 241 without running anything,
standard error giving the return code, as IBM's IGYWCLG procedure runs its GO step only up to 8
and the default NOCOMPILE(S) produces object code after E-level messages (C46).

A CBL or PROCESS card's COMPILE option moves the refusal: NOCOMPILE(W), NOCOMPILE(E) or
NOCOMPILE(S) (abbreviated NOC) refuses from the first message of that severity, COMPILE (C) from S
as NOCOMPILE(S) does, since IGYWCLG would bypass its GO step above 8 whatever the object code, and
NOCOMPILE alone is a syntax check that runs nothing. `-warnings-block` is ironwork's command-line NOCOMPILE(W), and a card's COMPILE or
NOCOMPILE wins over it, as IBM's PROCESS statements outrank the compiler's invocation. Neither
changes the return code (C47).

`--compliance extended` reads seven extensions Micro Focus and GnuCOBOL share, which Enterprise
COBOL refuses and `--compliance strict`, the default, still refuses: free-form source, level-78 and
CONSTANT entries, `<>`, literal concatenation with `&`, BINARY-SHORT, BINARY-LONG and BINARY-DOUBLE,
PROGRAM-ID with no IDENTIFICATION DIVISION header, and ASSIGN to a data item, whose value names the
file's DD at each OPEN and never a host file. Each use is a warning, IWX0001-W to IWX0007-W, naming
the extension and where it is, so `check` returns 4, and the program runs on the interpreter and the
VM alike. It also accepts an integer or numeric function as a MOVE's sender, as GnuCOBOL does, with
IWX0008-W; strict refuses it under either dialect, as Enterprise COBOL does. GnuCOBOL's PROCEDURE
DIVISION RETURNING OMITTED, a program that returns no item, is read as a header with no RETURNING
phrase, with IWX0009-W. ACCEPT FROM COMMAND-LINE, ARGUMENT-NUMBER and ARGUMENT-VALUE, and DISPLAY
UPON ARGUMENT-NUMBER, read the job step's PARM program arguments as a command line, with IWX0010-W
(C442).
[docs/compliance.md](docs/compliance.md) gives each one's meaning, the census that chose them, and
what stays refused and why.

A program with no STOP RUN, GOBACK or EXIT PROGRAM gets IBM's IGYPS2091-W, a warning that it may
run past its end. One that leaves by EXEC CICS RETURN or XCTL, which the CICS translator turns into
a CALL, is exempt unless asked: `--cics-return-warning=once` (the default) gives an informational
note in place of the warning, once in a run; `=always` gives the warning, return code 4; `=never`
gives nothing. Whether Enterprise COBOL warns such a program is open until an IBM listing settles it
(C124).

`--dialect gnucobol` gives GnuCOBOL's `cobc -std=ibm` result in place of Enterprise COBOL's where
ironwork's register of assumptions chose one and cobc chose another: a ROUNDED receiver's extra
decimal place reaches only a statement's last operation (C101); DISPLAY shows packed and binary
items (C14) and numeric literals (C95) as cobc does; ACCEPT at the end of SYSIN moves a space (C15);
an ENTRY name shares its program's storage (C51); a shorter EXTERNAL record shares the run unit's
(C180); and two unsigned zoned items of one length compare by their bytes at every OPTIMIZE level
(C262). `--dialect ibm` is the default. `--assume ID=VALUE` switches one of these seven alone,
whatever the dialect: `ibm` or `gnucobol`, and for C101 also `off`, the extra place counted in no
operation. It is repeatable, an assumption with no alternative is refused by name, and the choice is
kept in the load module, the provenance statement and the journal. [docs/dialect.md](docs/dialect.md)
lists these and every other difference found from cobc, which the dialect leaves alone: the
platform, what IBM documents and cobc does differently, and bugs.

Messages go to standard error, one to a line: errors first, then warnings, then informational
messages, each in the order ironwork found them.

    path:line:col: [ID-S ]message                   E, S or U
    path:line:col: warning: [ID-W ]message          W
    path:line:col: informational: [ID-I ]message    I
    path: [ID-S ]message                            the same three, for a message with no position
    path: warning: [ID-W ]message
    path: informational: [ID-I ]message

An error's line carries no severity word: E, S and U lines look alike, and the exit status is the
highest; an id's last letter is the message's severity. `path` is the program as given, or the COPY
member the position is in. An error's message never begins with `warning:` or `informational:`, so
a parser can take the word after the position as the severity when it is one of those two, and an
id, when one follows, matches `IW[A-Z][0-9]{4}-[IWESU]`. For example:

    client.cbl:12:17: Y is not defined
    client.cbl:8:26: IWR0001-S XML PARSE VALIDATING WITH OSR: the schema is in IBM's Optimized Schema Representation (OSR), which ironwork does not read
    client.cbl: warning: program CLIENT uses object-oriented syntax, which IBM compiles only with THREAD, DLL, RENT and DBCS: THREAD, DLL missing from its CBL or PROCESS cards (see J13 and J19)

`--diagnostics json` on `check`, `run`, `cics` and `compile` writes each message as one JSON object
a line instead, its keys sorted: `col`, `file` (the program as given), `id` (null for a message the
catalogue does not list yet), `line`, `member` (the COPY member, or null), `message` and `severity`
(`I`, `W`, `E`, `S` or `U`); `line` and `col` are null for a message with no position. Every other
line on standard error is written as before.

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
  TRAILING, and identifiers with their qualifiers, subscripts and reference modification), nested.
  A debugging line takes part in COPY and REPLACE matching, and is a comment after them outside
  debugging mode.
- **Data:** WORKING-STORAGE, LOCAL-STORAGE, FILE SECTION and LINKAGE SECTION items in DISPLAY, BINARY, COMP-5,
  PACKED-DECIMAL, COMP-1, COMP-2, NATIONAL, DISPLAY-1, POINTER and INDEX; numeric-edited and
  alphanumeric-edited PICTUREs (zero suppression, `*`, floating `$ + -`, CR, DB, insertion, BLANK
  WHEN ZERO); scaling positions P at either end of the digits; VALUE, REDEFINES, OCCURS with KEY,
  INDEXED BY and DEPENDING ON, SIGN, SYNCHRONIZED with IBM's slack bytes before an item and after
  each occurrence of a table, level-66 RENAMES of one item or a THRU range, and level-88
  conditions with THRU ranges and WHEN SET TO FALSE. SPECIAL-NAMES DECIMAL-POINT IS COMMA
  exchanges the comma and the period in PICTUREs, numeric literals and NUMVAL and NUMVAL-C, and
  CURRENCY SIGN clauses, with or without PICTURE SYMBOL, give the currency symbols and the values
  editing inserts (assumption C102), a hexadecimal literal in the program's code page (C141), for the program and the programs it contains. SPECIAL-NAMES UPSI-0 to UPSI-7
  entries give switch-status conditions, which the mnemonic-name qualifies, and SET ... TO ON and
  OFF; the eight switches are one copy for the run unit, off unless the PARM's runtime option
  UPSI(nnnnnnnn) sets them (C410 to C412). SPECIAL-NAMES CLASS clauses name sets of characters,
  given as characters or as ordinal numbers in the code page (C430), alone or in THROUGH ranges,
  and a class condition tests a USAGE DISPLAY item against one. Numeric PICTUREs and literals hold at most 18 digits
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
  up to six AFTER phrases on a performed procedure, inline; a COMP-1 or COMP-2 variable steps in
  floating point), EXIT PARAGRAPH/SECTION/PERFORM
  [CYCLE], NEXT SENTENCE, STRING, UNSTRING, INSPECT (TALLYING, also of a function's value: C190;

  REPLACING, CONVERTING, BEFORE/AFTER INITIAL; a national item in national characters: C230), SEARCH and SEARCH ALL (a binary search on the table's keys, as IBM's is, so an unsorted
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
  procedure resets XML-CODE (C118); VALIDATING is refused as IWR0001-S, since ironwork does not read
  the Optimized Schema Representation the schema is in. XML GENERATE, with COUNT, ENCODING,
  XML-DECLARATION, ATTRIBUTES, NAMESPACE and its prefix, NAME, TYPE, SUPPRESS and ON EXCEPTION, sets
  XML-CODE (C119). JSON PARSE, with NAME and OMITTED, SUPPRESS, CONVERTING, INDICATING, IGNORING and
  ENCODING, moves each matched value by MOVE's rules and sets JSON-CODE and JSON-STATUS (C200). What is not Enterprise COBOL is refused as such:
  `<>`, literals joined with `&`, SET ENVIRONMENT and ACCEPT ... FROM ENVIRONMENT. As Enterprise
  COBOL does, the compile refuses ALL with a numeric literal, a condition-name used as data, an
  arithmetic expression or numeric function compared with an operand that is not numeric, a
  figurative constant as a function argument outside an expression, ALL subscripts where the
  function takes a fixed number of arguments, MAX, MIN, ORD-MAX and ORD-MIN arguments of different
  classes, PERFORM VARYING of an item that is not numeric or FROM or BY an arithmetic expression,
  SEARCH VARYING an item that is neither an index nor an integer, PROCEDURE DIVISION RETURNING an
  item outside the LINKAGE SECTION, and an EXEC CICS HANDLE label that names no paragraph or
  section.
- **Subprograms:** several and nested programs per source; CALL (static and dynamic) USING BY
  REFERENCE, BY CONTENT, BY VALUE and OMITTED, RETURNING, ON EXCEPTION; PROCEDURE DIVISION USING
  and RETURNING; ENTRY [USING], whose name a CALL begins at and whose USING list alone gives LINKAGE
  addresses, a static CALL entering the program's one copy and a dynamic CALL (an identifier, or a
  literal under DYNAM) a copy of its own for each entry name (assumptions C50 and C51); SET of a
  PROCEDURE-POINTER or FUNCTION-POINTER TO ENTRY a literal or identifier, which loads the program
  when the SET runs, and CALL through the pointer, which enters it as a CALL of the name would
  (C140); CANCEL; IS
  INITIAL and IS RECURSIVE; EXIT PROGRAM; RETURN-CODE. Every program in
  a run shares one memory, as on z/OS, and a called program keeps its WORKING-STORAGE and open files
  between CALLs until it is cancelled; its LOCAL-STORAGE starts afresh on every CALL. PERFORMs and
  CALLs nest at most 100 deep. EXTERNAL records and files are the run unit's, one of each name for
  every program that describes it (C180); the GLOBAL records and files of a program reach the
  programs it contains, a name declared again nearer hiding it (C181). Not yet: LINAGE or REPORT
  on an EXTERNAL file, or on a GLOBAL file of a program that contains others; INDEXED BY in such a
  GLOBAL record; a GLOBAL file whose FILE STATUS or keys are not GLOBAL names; SET ADDRESS OF a
  GLOBAL LINKAGE record from a contained program.
- **User-defined functions:** FUNCTION-ID definitions and prototypes (AS, IS PROTOTYPE, ENTRY-NAME,
  ENTRY-INTERFACE) to END FUNCTION, invoked wherever an intrinsic function can be, as FUNCTION
  name(arguments) or, when the REPOSITORY paragraph lists FUNCTION name, by the name alone. An
  invocation is checked against the definition or prototype before it in the source: the number of
  arguments, and each data item passed BY REFERENCE against its parameter's PICTURE, USAGE, SIGN,
  JUSTIFIED and BLANK WHEN ZERO, a group by its length; a literal or expression is stored as the
  parameter describes it (C272), and a prototype and the definition of its name must agree. The
  RETURNING item's value is the function's, reference-modifiable when alphanumeric or national.
  Functions are recursive, each activation with its own LOCAL-STORAGE, and STOP RUN in one ends the
  run (C274). EXIT FUNCTION, a nested definition, an intrinsic function's name (C271), BY VALUE
  parameters other than binary, floating-point, pointers and single characters, and SQL or CICS with
  functions (C273) are refused. Lowering refuses an invocation, so `compile` writes no module for a
  program that invokes a function.
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
  after each ALTER of one. A file statement of a contained program with no procedure of its own
  runs the first USE GLOBAL procedure of the programs containing it, innermost out, for the file
  and then for its open mode, as a procedure of the program that declares it; USE GLOBAL BEFORE
  REPORTING a group of a contained program is refused. Assumptions C60 to C69 and C98 hold what
  the manuals leave open.
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
  and ROLLBACK, CICS SYNCPOINT, dynamic SQL (PREPARE, EXECUTE, EXECUTE IMMEDIATE, DESCRIBE, the
  SQLDA and cursors for prepared statements), host variables and indicators converted by Db2's
  rules, the SQLCA and WHENEVER. A normal end commits and an abend rolls back. TLS to PostgreSQL is
  in a separate build, [tls/](tls/README.md), so that this one keeps no dependencies.
  [docs/sql-runtime.md](docs/sql-runtime.md) specifies it, with what Db2 12.1 for Linux settled.
- **CICS, run as a harness** (`ironwork cics`): one task, with the transaction ID, terminal, user
  and COMMAREA the command line gives, and the EXEC interface block in IBM's layout. Program
  control (RETURN with TRANSID and COMMAREA, LINK, XCTL, ABEND); exception conditions (RESP, RESP2,
  NOHANDLE, HANDLE CONDITION with ERROR, IGNORE CONDITION, PUSH and POP HANDLE, and the AEIx abend
  IBM documents for a condition nothing handles; a program check is ASRA); HANDLE ABEND PROGRAM,
  LABEL, CANCEL and RESET, one exit per logical level, which an abend in the program or a level
  below it reaches (C142); ASKTIME,
  FORMATTIME, ASSIGN, GETMAIN, FREEMAIN, ADDRESS, SYNCPOINT, ENQ, DEQ, DELAY, SEND TEXT and WRITE
  OPERATOR; temporary-storage and transient-data queues; and file control over VSAM KSDS and RRDS
  files (READ with GENERIC, GTEQ and UPDATE, WRITE, REWRITE, DELETE, UNLOCK, and browsing with
  STARTBR, READNEXT, READPREV, RESETBR and ENDBR). Without a screen script the task ends with
  RETURN TRANSID's COMMAREA written out, so a pseudo-conversation runs one task at a time.
  `ironwork cics program.iwm` runs a load module's first program as the task's first program, on
  the VM, with every flag but `--serve` and `--serve-public`, which serve a source's tasks.
- **BMS maps and a 3270 terminal.** COPY of a mapset reads `NAME.bms` (DFHMSD, DFHMDI, DFHMDF) from
  the copy libraries and gives the symbolic map the BMS assembly would; DFHAID and DFHBMSCA carry
  their values. SEND MAP (ERASE, MAPONLY, DATAONLY, CURSOR, symbolic cursor, FREEKB, ALARM, FRSET),
  RECEIVE MAP (MAPFAIL, JUSTIFY, EIBAID, EIBCPOSN), SEND CONTROL and RECEIVE work on a 3270
  display that speaks the 3270 data stream. `--screens FILE` plays an operator from a script
  (`type ROW COL text`, `eof`, `cursor`, `home`, `tab`, `string text`, then an AID key) and prints
  every screen; `--serve HOST:PORT` is a TN3270 server a 3270 emulator such as c3270 or x3270
  connects to. It asks for no credentials, so it serves only a loopback address unless
  `--serve-public` is given. Both run
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

With `--vm`, differ.py runs each program under `ironwork run --vm` in gcobol's place and reports
where the VM and the interpreter differ, and what stops the VM where it stops.

With `--cobc` it compiles with GnuCOBOL's `cobc -x -std=ibm` instead and runs ironwork with
`--dialect gnucobol`; `fixtures/dialect` holds a program for each switched assumption, which should
agree:

    tools/differ.py target/release/ironwork fixtures/dialect bench/packed.cbl --cobc

`tools/nist.py` runs NIST's CCVS85 audit routines, one program to a file as in
[z390development/nistcobol85](https://github.com/z390development/nistcobol85)'s `src/`, after
EXEC85's default option switches and X-cards, and classes each program as clean, failed (a FAIL*
line in its report), refused or abended. Subprograms run only when called, and the flagging tests
are compiled and not run, as the CCVS85 User Guide says. A program that names an UPSI switch runs
with the PARM `/UPSI(10000000)`, the settings its tests expect. `--baseline` names an earlier results file
and lists every program whose class changed; the exit status is 1 when one that was clean is no
longer. `--vm` runs each program again on the VM from the same files and compares the exit status,
standard output, standard error and every file the two runs leave; the exit status is 1 when any
program differs. CI runs it so on every push:

    tools/nist.py target/release/ironwork ../nistcobol85/src --out nist.tsv --baseline before.tsv
    tools/nist.py target/release/ironwork ../nistcobol85/src --vm

`ironwork assumptions` lists the register of assumptions (`numeric::assumptions::ASSUMPTIONS`), one
per line; `--c-series` puts each entry's number in a single C series first, with its own id beside it.

## Code pages

`zarch/ucm/` holds IBM's tables as ICU publishes them, pinned to
[unicode-org/icu-data@8d9eb3e2](https://github.com/unicode-org/icu-data/tree/8d9eb3e27e79f59dd76e278e58d68b4668835027/charset/data/ucm):
CCSIDs 037, 273, 277, 278, 280, 284, 285, 297, 500, 871, 1047 and 1140–1149, and the mixed pages DBCS
programs compile with (the Programming Guide's Table 47): 930, 939, 1390, 1399, 5026 and 5035
(Japanese), 933 and 1364 (Korean), 935 and 1388 (Simplified Chinese) and 937 (Traditional
Chinese). `build.rs` turns them into tables at build time and refuses a single-byte table that does
not map all 256 bytes. A mixed page's single bytes with no character read as U+001A, as IBM's
conversions substitute, and its two-byte characters become one table for each distinct DBCS
component, seven in all. The ICU data is under the Unicode License v3.

DBCS data runs on both executors: PICTURE G, N under NSYMBOL(DBCS) or with USAGE DISPLAY-1, and B
for a DBCS space; G and N literals; MOVE, padded with DBCS spaces, to DBCS, national and group
items; comparison in binary order, or with a national item through the code page; the DBCS and KANJI
classes; reference modification, STRING, UNSTRING and INSPECT in DBCS characters; INITIALIZE;
LENGTH and NATIONAL-OF; JSON and XML GENERATE; and GRAPHIC and VARGRAPHIC host variables. Under a
mixed CODEPAGE an alphanumeric literal or item may hold DBCS characters between shift-out and
shift-in (C284), which DISPLAY and NATIONAL-OF convert. Under a single-byte CODEPAGE DBCS data shows
the DBCS space as U+3000 and other characters as U+FFFD, and a DBCS literal ends the run (C282).

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
Each mutated program that compiles also runs on the interpreter and on the VM under a statement
limit, and the two must agree; `IRONWORK_DIFFERENTIAL_ITERATIONS` sets how many are tried.
`fuzz/` is a coverage-guided cargo-fuzz target over the same path, seeded with the oracle programs;
it needs a nightly toolchain and `cargo install cargo-fuzz`, then `cargo +nightly fuzz run front_end`.
