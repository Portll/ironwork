# Evidence: run journals, build provenance and change assurance

What ironwork records about a run, a compile and a change, in a form a third party can verify.
The record format, the ledger, seals and witnesses are cobolwork's (cobolwork
`docs/spec/evidence.md`), so `cobolwork evidence verify`, `seal` and `anchor` work on ironwork's
evidence directory unchanged.

**Status:** built, 2026-09-30. `--evidence` and `--provenance` on `run` and `check`; `compare`.

## 1. Run journal: `--evidence DIR`

`ironwork run`, `check` and `job` write `DIR/runs/<runId>.jsonl` and append the run's tip to
`DIR/ledger.jsonl` (`crates/rt/src/evidence.rs`, `crates/cli/src/evidence.rs`). Each record is
canonical JSON hashed as SHA-256(`"cobolwork-evidence/v1\n"` || the record without `hash`), linked
by `prev` and `seq`.

| kind | fields | written |
|---|---|---|
| `open` | `tool` (`ironwork`), `toolVersion`, `command`, `argv` (option names, `<value>` for values, the program by file name), `roots`, `platform` | first |
| `input` | `root`, `path`, `sha256`, `bytes` | the program and each COPY member it read |
| `dd` | `dd`, `event` (`open`, `close`, `end`), `mode`, `sha256`, `bytes` | a file's digest before each OPEN, after each CLOSE, and as the run left it |
| `call` | `program`, `from`, `sha256` | each program CALL loads from a library, with its source's digest |
| `abend` | `code`, `file`, `line` | the abend the run ended with |
| `step` | `step`, `pgm`, `outcome` | for `job`, each step as the job log shows it: `RC=0004`, an abend, BYPASSED or JCL ERROR, with why |
| `sink` | `sink`, `file`, `line`, `marker`, `reached` | with `--trace-marker`, an operation an input could steer, the first time it is reached with the marker in its operand and the first time without (§1.1) |
| `close` | `exit`, `counts`, `durationMs`, `ledger` | last |

- A job's journal is one run: the JCL as an `input`, then for each step its programs' sources, its
  DDs' `open` and `close` records and CALLs, an `end` record for each data set it was given, and
  its `step` record. The directory is refused inside the JCL's directory, `--datasets`, a library
  or a procedure library.
- A path is relative to the directory that supplied it (the program's, a `-I` library, a `-L`
  library) and otherwise its file name. No record holds a record's data, an option's value, or an
  absolute path.
- The directory is refused inside the program's directory or a library, through a symbolic link,
  and is created owner-only.
- Files are hashed as they stream (`crates/rt/src/digest.rs`), so a large data set is not held in
  memory; hashing an indexed file at OPEN still reads all of it.
- `run --coverage FILE` writes, for each program of the source, every paragraph with its line and
  how often control entered it, from the run unit's `Paragraph` events.
- The run unit tells an observer what it opens, closes and loads, and each paragraph control
  enters (`exec::unit::Observer`); the
  interpreter and, when it lands, the VM raise the same events, so a journal is the same under both.
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
COMMAND-LINE or ENVIRONMENT and PARM.

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
  ADV and SSRANGE as the compile decided them.
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
digests), `sqlRecording`, `closure` (each side's program and COPY members, named relative to their
library, by digest: how a change to a copybook alone is shown to have been run), `results`,
`declared`, `inconclusive`, `unchecked`, `coverage` and `limit`. A program CALL loaded from a
library is in `closure` as `called:<name>`.

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
`verdict`, `job`, `inputs` (each data set and its digest before the run), `sqlRecording`, `steps`
(each step, its program and its outcome as the job log shows it), `results`, `declared`,
`inconclusive`, `coverage` and `limit`. The verdicts and exit statuses are those of `compare`; a
step that reached what ironwork does not model makes the verdict `inconclusive`.

## 5. Abends from generated input: `ironwork fuzz`

`ironwork fuzz PROGRAM.cbl -o DIR` runs a batch program many times on generated input and keeps
each abend an input caused, in the directory cobolwork's abend set reads (`COBOLWORK_ABENDS=DIR
cobolwork scan --only abend ROOT`; `crates/cli/src/fuzz.rs`):

1. The inputs are the sequential and indexed files of fixed-length records the program OPENs INPUT
   or I-O, each on a DD of its own, and SYSIN where it ACCEPTs from SYSIN. An indexed file's
   records are in RECORD KEY order, and no two share the RECORD KEY or an ALTERNATE RECORD KEY
   without DUPLICATES. Records are built field by field, every occurrence of a table included,
   from their level-01 descriptions: mostly values the PICTURE allows, sometimes its boundary, and
   sometimes the bytes that break it (spaces or asterisks in a zoned number, a packed field of
   spaces). A relative file, one whose records have more than one length, and a DD that more than
   one file names are given an empty data set. A file assigned to SYSIN reads the SYSIN lines.
2. Each run is its own `ironwork run` with the given `--clock` (2026-01-01 without it), stopped
   after `--timeout` seconds. Every data set is written inside DIR under a name of its own, never
   under its DD name, since ASSIGN may name a path. `--seed` fixes the inputs, so the same seed
   gives the same runs.
3. An abend the program gives on empty input is not the input's doing and is not kept. Every other
   abend is kept once by code, file and line, with its input made as small as still gives it:
   records and SYSIN lines dropped, then each field outside the keys set to a value that breaks
   nothing, within 200 runs.
4. Each kept input runs once more with `--evidence DIR/evidence` and `--coverage
   DIR/coverage/N.json`, so the abend rests on that run's journal (§1), whose `abend` record names
   the code, the file and the line. DIR is refused inside the program's directory or a library, as
   that run would refuse its evidence directory. A found abend that is not kept is named on
   standard error with what its last run did instead.

`DIR/manifest.json` holds `tool` (`ironwork-fuzz`), `version`, `seed`, `strategy` (`fields`),
`clock`, `program` (`file`, relative to `--root`, the current directory without it, and `id`),
`roots` (the program's directory, then each `-I` and `-L` library, by path from `--root`, `.` for
`--root` itself and null for one outside it: the order a journal's `input` records number them, so
a file named relative to its library is found under that library),
`entry` (`run`), `inputs` (`id`, `kind` `dd` or `sysin`, `name`, `bytes` in base64, `minimized`,
false when the 200 runs ran out first), `counts` (`runs`, `clean`, `abend`, `timeout`, `refused`,
over the generated runs; an abend that says what the surroundings lack counts as refused and is not
kept: IRONWORK, a construct ironwork does not run, and S806, a CALL of a program no `-L` library
holds; so does a run in which ironwork itself panicked, which standard error reports) and `runs`,
one per kept abend (`input` ids, `outcome` `abend`, `abend` with `code`, `file` relative to the
program's directory or the library it came from, `line` and `message`, `journal` the run id, and
`coverage`). A program that takes PROCEDURE DIVISION USING is refused: a CALL would supply its
parameters.
