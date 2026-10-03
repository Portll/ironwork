# Evidence: run journals, build provenance and change assurance

What ironwork records about a run, a compile and a change, in a form a third party can verify.
The record format, the ledger, seals and witnesses are cobolwork's (cobolwork
`docs/spec/evidence.md`), so `cobolwork evidence verify`, `seal` and `anchor` work on ironwork's
evidence directory unchanged.

**Status:** built, 2026-09-30. `--evidence` on `run`, `check`, `job` and `cics`, of a source or,
for `run` and `cics`, a load module; `--provenance` on `run` and `check`; `compare`.

## 1. Run journal: `--evidence DIR`

`ironwork run`, `check`, `job` and `cics` write `DIR/runs/<runId>.jsonl` and append the run's tip to
`DIR/ledger.jsonl` (`crates/rt/src/evidence.rs`, `crates/cli/src/evidence.rs`). Each record is
canonical JSON hashed as SHA-256(`"cobolwork-evidence/v1\n"` || the record without `hash`), linked
by `prev` and `seq`.

| kind | fields | written |
|---|---|---|
| `open` | `tool` (`ironwork`), `toolVersion`, `command`, `argv` (option names, `<value>` for values but `--compliance`'s and `--dialect`'s, which are kept, the program by file name), `roots`, `platform` | first |
| `input` | `root`, `path`, `sha256`, `bytes` | the program and each COPY member it read |
| `dd` | `dd`, `event` (`open`, `close`, `end`), `mode`, `sha256`, `bytes` | a file's digest before each OPEN, after each CLOSE, and as the run left it |
| `call` | `program`, `from`, `sha256` | each program CALL loads from a library, with its source's digest |
| `abend` | `code`, `file`, `line` | the abend the run ended with |
| `step` | `step`, `pgm`, `outcome` | for `job`, each step as the job log shows it: `RC=0004`, an abend, BYPASSED or JCL ERROR, with why |
| `sink` | `sink`, `file`, `line`, `marker`, `reached`, `input` | with `--trace-marker`, an operation an input could steer, the first time it is reached with the marker in its operand and the first time without (§1.1); with `--trace-input`, `input` true, false or null, and a record for each value it first takes (§1.3). `marker` and `reached` only with a marker |
| `statement` | `file`, `line`, `capped` | with `--trace-statements`, each start of a listed statement, in the order the run made them, up to 100 per statement, the 100th with `capped` true (§1.2) |
| `close` | `exit`, `counts`, `durationMs`, `ledger` | last |

- `exit` is how the run ended: for `check`, the compile's return code; for `run`, `cics` and `job`,
  the RETURN-CODE of a run that ran to its end, its own value even where the exit status gives it
  as 239 (a job's highest step return code; 0 for a task), and otherwise the code of the reserved
  band ([README](../README.md#exit-status)), such as 240 for an abend or 241 for a program the
  compile refused. `--exit-code` does not change it. A RETURN-CODE from 240 up is told from an
  abend by the `abend` record an abend writes.

- A job's journal is one run: the JCL as an `input`, then for each step its programs' sources, its
  DDs' `open` and `close` records and CALLs, an `end` record for each data set it was given, and
  its `step` record. A COBOL step's abend gives the `abend` record its file and line, and standard
  error the line `run` gives. The directory is refused inside the JCL's directory, `--datasets`, a
  library or a procedure library.
- A path is relative to the directory that supplied it (the program's, a `-I` library, a `-L`
  library), the innermost where one lies inside another, and otherwise its file name. No record
  holds a record's data, an option's value, or an absolute path.
- A run of a load module (`run x.iwm`, `cics x.iwm`) reads no source, and records what its module
  records of the compile (load-module.md §9.2): its `input` records are the source and each COPY
  member the compile read, by the library index and path the compile found them under and the
  digests they had then; a program CALL loads from another module is a `call` record with its
  source's path and digest as that module records them; and a sink, statement or abend in a
  module's program names its file by the path its module records. The journal is the one a run of
  the source writes when the module runs with the source's libraries in the same order from a
  directory of the source directory's name, but for the module's file name in `argv`.
- The directory is refused inside the program's directory or a library, through a symbolic link,
  and is created owner-only.
- Files are hashed as they stream (`crates/rt/src/digest.rs`), so a large data set is not held in
  memory; hashing an indexed file at OPEN still reads all of it.
- `run --coverage FILE` writes, for each program of the source but a function prototype, every
  paragraph with its line and how often control entered it, from the run unit's `Paragraph`
  events, and the paragraphs reached in each program CALL loaded from a library; `job --coverage
  FILE` the same for every program the job's steps ran, each also naming its `source`, so steps
  that run one source add up and programs of two sources that share a PROGRAM-ID stay apart. A
  load module's run reports each program of the module compiled from its first program's source,
  its paragraphs from the LIR and their lines from the debug table, which is the report a run of
  the source writes.
- The run unit tells an observer what it opens, closes and loads, and each paragraph control
  enters (`exec::unit::Observer`); the interpreter and the VM raise the same events, so a journal
  is the same under both.
- `cics` keeps a journal for one task; `--serve` does not.

### 1.1 Input trace: `--trace-marker TEXT`

With `--evidence`, `run`, `job` and `cics` record whether a marker entered at an input reached each
operation an input could steer: cobolwork's execution label for a path finding (cobolwork
`docs/spec/reach.md` §9.7, marker `CWVRFY01`). The marker goes in where the input comes in (SYSIN, a
DD, the COMMAREA, a replayed row), so no source is instrumented; at each sink the interpreter
decodes the operand through the program's code page and the journal records whether the marker is
in it (`exec::unit::Event::Sink`). The operand itself is never recorded.

| `sink` | operation, where its operand is a data item |
|---|---|
| `dynamic-program-load` | CALL of a program named by a data item |
| `os-command` | CALL SYSTEM, C$SYSTEM, CBL_EXEC_RUN_UNIT, CBL_GC_HOSTED or BXPSYSTM: the arguments. ironwork runs no operating-system command; the CALL loads a program of that name or fails, or, for an lp or lpr command in a run given DD PRINTER, prints on the virtual printer (README), whose DD and each printed DD are journalled as opened and closed |
| `log` | DISPLAY (literals included); WRITEQ TD FROM; WRITE OPERATOR TEXT; WRITE JOURNALNAME FROM |
| `cics-dynamic-transfer` | LINK or XCTL PROGRAM; START TRANSID |
| `queue-name` | QUEUE or QNAME of WRITEQ, READQ, DELETEQ |
| `record-key`, `record-update` | RIDFLD of READ, STARTBR, RESETBR; of DELETE |
| `screen` | SEND TEXT or SEND MAP FROM |
| `web-response`, `http-header`, `outbound-host`, `outbound-http` | WEB SEND FROM; WEB WRITE VALUE; WEB OPEN HOST or URL, WEB CONVERSE PATH; WEB CONVERSE FROM |
| `cics-sysid` | SYSID of any command |

The names are cobolwork's sink kinds (`lib/dataflow.mjs`), so a label joins a finding by sink, file
and line. A CICS operand is recorded before the command runs, so a command ironwork does not carry
out yet (START, WEB) is still traced before it stops the task; an operand that cannot be read is
left to the command, so tracing never changes how a run ends. Not traced, because ironwork does not
run them yet: MQPUT, dynamic SQL, sockets, ASSIGN to a data item, and the sources ACCEPT FROM
COMMAND-LINE or ENVIRONMENT. PARM reaches a program only through `ironwork job`.

### 1.2 Statement trace: `--trace-statements FILE`

With `--evidence`, `run` and `cics` record each time a statement FILE lists starts: whether one run
executed a finding's route in order, which cobolwork needs before coverage may refute the finding
(cobolwork `docs/spec/reach.md` §9.8, fact 2). FILE holds one `FILE:LINE` per line, split at the
last colon, blank lines left out: the statements of cobolwork's `flow --all-routes`
`routes.statements`. A statement is matched by its file's name and its line, as cobolwork joins a
sink to a finding, so a path recorded from another directory still matches.

- **Where.** A statement starts as the walker's `exec` meets it (lir.md §10): every statement but
  NEXT SENTENCE, a separator period, CONTINUE and EXIT, each time control reaches it, in a COPY
  member or a program CALL loaded as well as in the program itself.
- **How many.** The first 100 starts of each listed statement are recorded; the 100th carries
  `capped: true`, and later ones are left out, so a loop cannot fill the journal. A route whose
  order shows only after a statement's cap is not shown.
- **Cost.** The run unit tells the observer only of statements on a listed line, and nothing
  without the flag. The table of statement starts is in every lowered program, so the VM raises the
  same events as the walker.

### 1.3 Input trace by taint: `--trace-input`

With `--evidence`, `run` and `cics` follow which bytes of run-unit memory may hold input, with no
marker. At each operation §1.1 names, the sink record says whether an input byte may be in its
operand: `input` true, false, or null. cobolwork needs this before coverage may refute a finding
(cobolwork `docs/spec/reach.md` §9.8, fact 3): a step that changes bytes (a numeric MOVE, a COMPUTE,
a FUNCTION) loses the marker, but it does not lose the taint.

- **Input.** A READ's record and its INTO item, ACCEPT from SYSIN or the console, the host
  variables and SQLCA a row EXEC SQL fetched fills, a job step's PARM and a CICS task's COMMAREA.
  ACCEPT FROM DATE, DAY or TIME is not input. In a CICS task, EIBCALEN when there is a COMMAREA,
  the AID that started the task, and whatever a command writes once it has taken in data from
  outside the program: RECEIVE and RECEIVE MAP from the terminal (EIBAID and EIBCPOSN included),
  READQ TS and READQ TD, and a file READ, READNEXT or READPREV (RIDFLD included).
- **Through statements.** Each statement's writes may hold input when the statement has read a
  byte that may, since it started: the bytes of every item it locates, which includes a subscript's
  item, a BY CONTENT or BY VALUE copy, and the key a random READ, START or DELETE finds its record
  by. OPEN reads none of a file's keys. A receiver a statement only writes is not read: MOVE's,
  SET's, ACCEPT's, INITIALIZE's, PERFORM VARYING's FROM, CALL's RETURNING, a file's FILE STATUS,
  and the options an EXEC CICS command stores into. So `MOVE SPACES TO X` clears X. A program's
  initial values and LOCAL-STORAGE hold none. What CICS keeps outside the program between commands (queues, files,
  RETURN's COMMAREA) comes back only through one of the commands above, and is input then.
- **What it does not say.** A condition on input steers which constant is stored, but it puts no
  input byte in the receiver, and taint does not follow it. A whole receiver may hold input when any
  operand did, so taint over-approximates. It never under-approximates where it follows the run.
- **Not followed yet.** SORT and MERGE, the Report Writer, XML and JSON statements,
  object-oriented COBOL, calls through pointers, and Language Environment services. After the first
  of these, a sink is null where it would be false, and true stays true.
- **Equal under both executors.** The rt write funnel (`RunUnit::write`) and the locate of each
  executor carry it, and the differential compares the taint of every byte and each sink's `input`
  between the interpreter and the VM (lir.md §12.3).

## 2. Build provenance: `--provenance FILE`

<a id="check-v1"></a>`check` and `run` write an in-toto statement with the SLSA Provenance v1
predicate (`crates/cli/src/provenance.rs`), `buildType`
`https://github.com/Portll/ironwork/blob/main/docs/evidence.md#check-v1`:

- `subject`: the program source by digest.
- `resolvedDependencies`: the source and every COPY member by digest, `uri file:<path relative to
  its library>`, `annotations.library` the index of that library; a member the compiler supplies
  itself (`(system member X)`) by name only.
- `externalParameters`: the `CBL`/`PROCESS` cards as written, the flags, the library names.
- `internalParameters.optionsInForce`: ARITH, TRUNC, NUMPROC, CODEPAGE, the TRUNC check, FASTSRT,
  ADV, the compliance level (`strict` or `extended`) and SSRANGE as the compile decided them, and
  the dialect (`ibm` or `gnucobol`, [dialect.md](dialect.md)).
- `runDetails.builder`: `IRONWORK_BUILDER_ID` or `https://github.com/Portll/ironwork/local`, the
  version, and the ironwork executable's digest; `byproducts` the run journal's tip when `--evidence`
  is on.

It is unsigned. `cobolwork evidence seal` signs, or the pipeline does; the platform that signs earns
any SLSA level. The verifier of [verifier.md](verifier.md) can record its verdict against the same
digests.

## 3. Change assurance: `ironwork compare`

<a id="equivalence-v1"></a>`ironwork compare --base OLD.cbl --head NEW.cbl --dd ...` runs both
versions (`crates/cli/src/compare.rs`):

1. The inputs and both sources are read once, when the comparison begins; each side runs in its
   own new temporary directory holding copies of those inputs, so neither side can change the
   caller's files or the other's inputs, and a file changed during the run changes neither. The clock is `--clock` or 2026-01-01; SYSIN
   is the SYSIN DD or empty; a `--sql-replay` recording is replayed strictly, so a change that issues
   different SQL fails its replay.
2. RETURN-CODE, the abend code, the DISPLAY output and every DD are compared byte for byte, line
   by line.
3. `--declare FILE` lists intended divergences, one a line: `DD NAME [lines A-B] reason`,
   `DISPLAY reason`, `RETURN-CODE reason`. Every line that differs must fall in a declaration of
   its output, and the first that does not is the one reported; a declaration without lines
   covers the whole output.
4. `--expected NAME=path` compares the head's output with a given file instead of a base run: the
   check for a program translated to another language, whose outputs are the files. Each file
   must exist, and the DDs no file is given for are listed as `unchecked`.

The statement's `predicateType` is
`https://github.com/Portll/ironwork/blob/main/docs/evidence.md#equivalence-v1`; its subjects are
`base:<file>` and `head:<file>` by digest, and its predicate holds `verdict`, `inputs` (DD names and
digests), `sqlRecording`, `clock` (the fixed time both sides ran at, ISO 8601 UTC), `closure` (each
side's program and COPY members, named relative to their library, by digest: how a change to a
copybook alone is shown to have been run), `results`, `declared`, `inconclusive`, `unchecked`,
`coverage` and `limit`. A program CALL loaded from a library is in `closure` as `called:<name>`.

| verdict | when | exit |
|---|---|---|
| `equivalent` | no difference | 0 |
| `equivalent-as-declared` | every difference declared | 0 |
| `diverged` | a difference not declared | 1 |
| `inconclusive` | a side could not run, or reached what ironwork does not model | 3 |

`coverage` is measured on the head run: `paragraphs` and `reached` count the head program's
paragraphs and those control entered; `changed` names the paragraphs whose statements differ from
the base's (positions aside), or every paragraph when the change touched none, as a change to data
or to a copybook in the DATA DIVISION does (`scope` `all`; so too with `--expected`); and
`unreached` names the changed paragraphs the inputs never entered. cobolwork's build refuses a
statement with unreached changed paragraphs where its policy requires equivalence. `coverage` is
`null` when the head did not run.

**Limits, stated in every statement.** Equivalence is under ironwork's model of Enterprise COBOL,
on the inputs given. The oracle holds no Enterprise COBOL goldens yet. Coverage is of paragraphs
entered, not of the statements or branches within them.

## 4. Migration equivalence for a job: `ironwork job --expected`

<a id="job-equivalence-v1"></a>`ironwork job JOB.jcl --datasets DIR --expected DATASETS=PROD` runs
the job against production's recorded inputs and compares what it leaves with what production left
(`crates/cli/src/job.rs`):

1. The job runs on a temporary copy of `--datasets`, so DISP=DELETE and every write leave the given
   data sets as they were. The clock is `--clock` or 2026-01-01; a `--sql-replay` recording is
   replayed strictly across the job's steps in order.
2. Each file under PROD, laid out as `--datasets` is (A.B, or A.B/M for a member), is compared byte
   for byte with the data set of that name the job left; each difference is located by line and
   offset, and a data set the job did not leave is a difference. Declarations cover lines as they
   do for `compare`.
3. `--expected STEPS=FILE` adds production's step outcomes from its job log, one a line
   (`STEP RC=0004`, `CALLER.PSTEP ABEND S0C7`), each compared with the step's outcome in the job.
4. `--declare FILE` lists intended divergences, one a line: `DATASET DSN [lines A-B] reason` or
   `STEP NAME reason`.

The statement's `predicateType` is
`https://github.com/Portll/ironwork/blob/main/docs/evidence.md#job-equivalence-v1`, apart from
`equivalence-v1` because its subjects are not a base and a head program: they are `job:<file>`, the
JCL, and `program:<file>` for each COBOL program the job ran or CALLed, by digest. The predicate holds
`verdict`, `job`, `inputs` (each data set and its digest before the run), `sqlRecording`, `clock`,
`steps` (each step, its program and its outcome as the job log shows it), `results`, `declared`,
`inconclusive`, `coverage` and `limit`. The verdicts and exit statuses are those of `compare`; a
step that reached what ironwork does not model makes the verdict `inconclusive`.

## 5. Abends from generated input: `ironwork fuzz`

`ironwork fuzz PROGRAM.cbl -o DIR` runs a batch program many times on generated input and keeps
each abend an input caused, in the directory cobolwork's abend set reads (`COBOLWORK_ABENDS=DIR
cobolwork scan --only abend ROOT`; `crates/cli/src/fuzz.rs`):

1. The inputs are the sequential, indexed and relative files the program OPENs INPUT or I-O, each
   on a DD of its own, and SYSIN where it ACCEPTs from SYSIN. An indexed file's records are in
   RECORD KEY order, and no two share the RECORD KEY or an ALTERNATE RECORD KEY without
   DUPLICATES. A relative file's data set holds a record per slot, some slots left empty. Records
   are built field by field, every occurrence of a table included, from their level-01
   descriptions: mostly values the PICTURE allows (a packed field's pad nibble zero, a national
   field UTF-16 text), sometimes its boundary, and sometimes the bytes that break it (spaces or
   asterisks in a zoned number, a packed field of spaces). A file whose records have more than one
   length gets each record behind an RDW, at its level-01 record's length, at a length READ allows
   (VLR decides which), or now and then shorter than READ allows. A line-sequential file, an
   indexed file whose keys lie past its shortest record, a contained program's file and a DD that
   more than one file names are given an empty data set. A file assigned to SYSIN reads the SYSIN
   lines. A blank SYSIN card is 80 spaces, never an empty line, and no card starts `/*` or `//`,
   where the reader would end in-stream data. A program whose one USING item is
   the parameter Language Environment gives a job step (a group led by a halfword binary length)
   gets a PARM of up to 100 characters through `run --parm`, which the run's journal does not
   record; the manifest holds it.
   With `--job`, the inputs are those of the job's COBOL steps: each data set a step reads before
   any step creates it, built from the first reading program's file description and named by its
   data set name (a generation data group's base or a relative generation by the generation it
   reads, the newest for the base, so the group's own entry is never written); each in-stream DD
   a step reads as SYSIN lines or as its file, named `STEP.DD`;
   and each step whose program takes a PARM, named by the step as the job log names it. They reach
   `ironwork job` through `--datasets` (a fresh copy per run of `--datasets` given to fuzz, with
   the fed data sets written in), `--instream` and `--step-parm`. A data set the job reads that is
   neither fed nor given is empty. A step that abends at no COBOL statement, and a JCL error, are
   counted as refused.
2. Each run is its own `ironwork run`, or `ironwork job` with `--job`, with the given `--clock`
   (2026-01-01 without it), stopped after `--timeout` seconds. Every data set is written inside DIR
   under a name of its own, never under its DD name, since ASSIGN may name a path. `--seed` fixes
   the inputs, so the same seed gives the same runs.
3. An abend the program gives on empty input is not the input's doing and is not kept. Every other
   abend is kept once by code, file and line, with its input made as small as still gives it:
   records and SYSIN lines dropped, the PARM cut short, then each field outside the keys set to a
   value that breaks nothing, within 200 runs.
4. Each kept input runs once more with `--evidence DIR/evidence` and `--coverage
   DIR/coverage/N.json`, so the abend rests on that run's journal (§1), whose `abend` record names
   the code, the file and the line. DIR is refused inside the program's directory or a library, as
   that run would refuse its evidence directory. A found abend that is not kept is named on
   standard error with what its last run did instead.
5. Two outcomes that are counted and never kept become findings under strict conditions. A
   timeout, up to three per fuzz run, is run again on the same input under `--statement-limit`
   (`--hang-limit`, 10,000,000 statements without it) for six times `--timeout`; if that run ends
   in S322 it is kept as an input that keeps the program running past the limit, at the first
   statement of the loop it is in (assumption C241), which the message names with the loop's
   lines. It is not kept when the empty input's run ends in S322 in the same loop (in the same
   file, sharing a line), nor a second time for a loop already kept. A run that had ACCEPT find
   SYSIN at its end, stopped at its timeout or its limit, is a program waiting for input, not
   looping on it: it is counted as a timeout and not run again. A kept S322 says the run passed
   the limit; it does not show the loop would never end. An S806, up to five per fuzz run, each
   at a CALL of its own, is kept only where the program name its message gives is in the input
   and a run with every occurrence of that name replaced by a marker of `@`, `#` and `$`, traced
   with `--trace-marker`,
   ends in S806 at the same CALL naming the marker and its journal records the marker reaching
   that CALL's `dynamic-program-load` sink. A static CALL raises no such sink. A kept hang's input
   is made smaller within 10 runs; a kept S806's input is the marked one.
6. Each kept input other than an S322 or an S806 runs once more, with no evidence, compiled with `--optimize=2`, the compiler
   invocation's OPTIMIZE(2), which a CBL or PROCESS card's OPTIMIZE outranks. IBM leaves what invalid
   data does to the generated code, and at OPTIMIZE(1) and (2) it may compare an unsigned zoned item
   with zero by its bytes where OPTIMIZE(0), its default, reads it as a number and ends in a data
   exception (assumption C262). Whether that run ends in the same abend at the same place is the
   abend's `optimized`. OPTIMIZE changes neither where a loop passes the statement limit nor the
   name a CALL takes, so a kept S322 or S806 is `optimized` without that run.

`DIR/manifest.json` holds `tool` (`ironwork-fuzz`), `format`, `version` (the ironwork release that
wrote it), `seed`, `strategy` (`fields`),
`clock`, `program` (`file`, relative to `--root`, the current directory without it, and `id`),
`roots` (the program's directory, then each `-I` and `-L` library, by path from `--root`, `.` for
`--root` itself and null for one outside it: the order a journal's `input` records number them, so
a file named relative to its library is found under that library; for a job, the JCL's directory,
`.work` standing for each run's own data sets, the libraries, then the procedure libraries),
`entry` (`run`, `job`, or `cics` for §5.1; for a job `program` is the JCL and its id the job's
name), `inputs` (`id`, `kind` `dd`, `sysin`, `parm`, `commarea` or `terminal`, `name`, `bytes` in
base64, `minimized`,
false when the 200 runs ran out first), `counts` (`runs`, `clean`, `abend`, `timeout`, `refused`,
over the generated runs; an abend that says what the surroundings lack counts as refused and is not
kept: IRONWORK, a construct ironwork does not run, S806, a CALL of a program no `-L` library holds,
EXEC, an EXEC statement with no database or region behind it, and IO-35, an OPEN of a file no DD
gives; so do a run ironwork refused, told by its exit status from 241 up, and a run in which
ironwork itself panicked, 255; standard error gives the first refusal's reason and the first
panic) and `runs`,
one per kept abend (`input` ids, `outcome` `abend`, `abend` with `code`, `file` relative to the
program's directory or the library it came from, `line`, `message` and `optimized` (item 6),
`journal` the run id, `coverage`, and `limit`, the statement limit a kept S322's runs were given,
which its place depends on and its journal's `argv` also records). `optimized` rests on the manifest's word: the run it comes
from keeps no journal. A program that takes any other PROCEDURE DIVISION USING is refused: a CALL would
supply its parameters.

`format` is `ironwork-fuzz/v1`, the shape [fuzz-manifest.schema.json](fuzz-manifest.schema.json)
describes, apart from `version` so a reader checks the shape and not the release. A key added to
the manifest keeps the format, and a reader skips keys it does not know; a key removed or renamed,
or a value given another meaning, takes a new format, which a reader of the old one refuses. So
does a new value of `entry` or of an input's `kind` that changes what a kept run shows, since a
reader that does not look at the value would read the run as one it knows.
A manifest from ironwork 0.3.0 or earlier has the v1 shape without `format` and `optimized`, and one
from before 0.3.0 has no `roots` either.

### 5.1 A CICS task: `ironwork fuzz --cics`

`ironwork fuzz --cics PROGRAM.cbl -o DIR` runs the program as the first program of a CICS task,
each run an `ironwork cics`, and writes the same directory with `entry` `cics`
(`crates/cli/src/fuzz_cics.rs`):

1. The COMMAREA is built from DFHCOMMAREA's fields, or, where DFHCOMMAREA is a table EIBCALEN
   sizes, from the fields of the item the program MOVEs it into (up to 256 bytes of the table where
   there is no such MOVE). Three tasks in ten get none, as a terminal's first task does, and now
   and then one is shorter than the program describes. The one-byte DFHCOMMAREA the CICS
   translator declares for a program that declares none is not varied. A task whose program reads
   past the COMMAREA it was given ends refused: ironwork does not model the storage beyond it.
2. A program that RECEIVEs or CONVERSEs, or RETURNs with a TRANSID, gets an operator at a scripted
   terminal (`--screens`) for one to four turns. Each turn types text into some of the unprotected
   fields of a map the program RECEIVEs (its mapset read from the copy libraries as NAME.bms, or
   fields of no map where no library holds it), each field reached by `home` and `tab`, then
   presses an AID key: ENTER most often, otherwise one the program names (DFHPFn, DFHCLEAR, HANDLE
   AID) or any. A program that only SENDs gets a terminal with no turns. RETURN TRANSID starts the
   next task on the script's next key; without `--transid` or `--csd`, the one transaction RETURN
   TRANSID names, by a literal or a data item's VALUE, runs the program.
3. `--transid`, `--termid`, `--userid`, `--applid`, `--sysid`, `--transaction`, `--csd`, `--file`
   and `--td` go to every task. Each task gets its own copy of each `--file` data set, which a task
   writes back when it ends, and its own `--td` queues.
4. The empty input is a task with no COMMAREA and no turns. Minimizing drops turns, then the
   COMMAREA, then puts each COMMAREA field back to a value that breaks nothing, then leaves each
   typed field untyped. Besides the abends above, AEI0 (PGMIDERR), AEIL (FILENOTFOUND), AEYQ
   (SYSIDERR) and AEI1 (TRANSIDERR) say what the region lacks and count as refused. A data or
   protection exception in a task is ASRA, its message naming S0C7 or S0C4.
5. A kept run's inputs are `kind` `commarea` (`name` `DFHCOMMAREA`, the EBCDIC bytes `--commarea`
   takes) and `terminal` (`name` the terminal id, `--termid` or `TERM`, and the screen script as
   UTF-8). Its journal and coverage take in every task of its pseudo-conversation. Its input runs
   once more compiled with `--optimize=2`, which gives its abend's `optimized`, as in §5.

### 5.2 A subprogram at its interface: `ironwork fuzz --interface`

`ironwork fuzz --interface PROGRAM.cbl -o DIR` runs a subprogram as a caller would, each run an
`ironwork run --argument`, and writes the same directory in its own format
(`crates/cli/src/fuzz/interface.rs`):

1. The subprogram must take PROCEDURE DIVISION USING items. Refused, because the arguments are not
   data a caller makes: one with a pointer among them, an IMS program (ENTRY 'DLITCBL' or
   'DLITPLI', EXEC DLI, a CALL of CBLTDLI, AIBTDLI or CEETDLI by name or by a data item's VALUE,
   and an argument passed on to a CALL whose target no literal or VALUE names) and a CICS program,
   which `--cics` runs.
2. Each argument is built field by field from its LINKAGE record, as a record is (§5 item 1), with
   each OCCURS DEPENDING ON object in the record kept within its table's bounds.
3. Where a source in the subprogram's directory or an `-L` library CALLs it by name, a run takes
   the shape of one such CALL, drawn per run: an OMITTED position stays OMITTED, a literal is passed
   as written, padded with spaces, and of an item the caller passes, only as much as it holds is
   varied. With no such CALL, every field of every argument is varied.
4. `ironwork run --argument` gives the program the arguments in USING order, each pushed as input,
   OMITTED as a null address, and runs it as a subprogram: EXIT PROGRAM returns. An abend on
   arguments that break nothing is not kept; every other is kept once, its arguments made as small
   as still give it within 200 runs, then run with `--evidence` and `--coverage` and once more with
   `--optimize=2`, as in §5. A timeout and an S806 are counted, never kept.
5. The manifest's `format` is `ironwork-fuzz-interface/v1`
   ([fuzz-interface-manifest.schema.json](fuzz-interface-manifest.schema.json)): `entry`
   `interface`; `callers`, each CALL a run may take its shape from (`file` from `--root`, `line`);
   and inputs of `kind` `argument`, `name` the USING item, `position` its place in the USING list,
   and `omitted` true for an OMITTED one. Its kept runs are described as §5's are. An abend found
   this way shows that a caller passing those bytes ends the subprogram, not that any caller does.
