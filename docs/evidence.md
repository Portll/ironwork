# Evidence: run journals, build provenance and change assurance

What ironwork records about a run, a compile and a change, in a form a third party can verify.
The record format, the ledger, seals and witnesses are cobolwork's (cobolwork
`docs/spec/evidence.md`), so `cobolwork evidence verify`, `seal` and `anchor` work on ironwork's
evidence directory unchanged.

**Status:** built, 2026-09-30. `--evidence` and `--provenance` on `run` and `check`; `compare`.

## 1. Run journal: `--evidence DIR`

`ironwork run` and `check` write `DIR/runs/<runId>.jsonl` and append the run's tip to
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
| `close` | `exit`, `counts`, `durationMs`, `ledger` | last |

- A path is relative to the directory that supplied it (the program's, a `-I` library, a `-L`
  library) and otherwise its file name. No record holds a record's data, an option's value, or an
  absolute path.
- The directory is refused inside the program's directory or a library, through a symbolic link,
  and is created owner-only.
- Files are hashed as they stream (`crates/rt/src/digest.rs`), so a large data set is not held in
  memory; hashing an indexed file at OPEN still reads all of it.
- The run unit tells an observer what it opens, closes and loads (`exec::unit::Observer`); the
  interpreter and, when it lands, the VM raise the same events, so a journal is the same under both.

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

1. Each side runs in its own temporary directory holding copies of every input DD, so neither side
   can change the caller's files or the other's inputs. The clock is `--clock` or 2026-01-01; SYSIN
   is the SYSIN DD or empty; a `--sql-replay` recording is replayed strictly, so a change that issues
   different SQL fails its replay.
2. RETURN-CODE, the abend code, the DISPLAY output and every DD are compared byte for byte; each
   difference is located by line and offset.
3. `--declare FILE` lists intended divergences, one a line: `DD NAME [lines A-B] reason`,
   `DISPLAY reason`, `RETURN-CODE reason`.
4. `--expected NAME=path` compares the head's output with a given file instead of a base run: the
   check for a program translated to another language, whose outputs are the files.

The statement's `predicateType` is
`https://github.com/Portll/ironwork/blob/main/docs/evidence.md#equivalence-v1`; its subjects are
`base:<file>` and `head:<file>` by digest, and its predicate holds `verdict`, `inputs` (DD names and
digests), `sqlRecording`, `closure` (each side's program and COPY members, named relative to their
library, by digest: how a change to a copybook alone is shown to have been run), `results`,
`declared`, `inconclusive`, `coverage` and `limit`.

| verdict | when | exit |
|---|---|---|
| `equivalent` | no difference | 0 |
| `equivalent-as-declared` | every difference declared | 0 |
| `diverged` | a difference not declared | 1 |
| `inconclusive` | a side could not run, or reached what ironwork does not model | 3 |

**Limits, stated in every statement.** Equivalence is under ironwork's model of Enterprise COBOL,
on the inputs given. The oracle holds no Enterprise COBOL goldens yet. Paragraph coverage is not
measured yet (E9), so `coverage` is `null`, and cobolwork's build treats a statement without
coverage as inconclusive where its policy requires equivalence.

## 4. Migration equivalence for a job: `ironwork job --expected`

<a id="job-equivalence-v1"></a>`ironwork job JOB.jcl --datasets DIR --expected DATASETS=PROD` runs
the job against production's recorded inputs and compares what it leaves with what production left
(`crates/cli/src/job.rs`):

1. The job runs on a temporary copy of `--datasets`, so DISP=DELETE and every write leave the given
   data sets as they were. The clock is `--clock` or 2026-01-01; a `--sql-replay` recording is
   replayed strictly across the job's steps in order.
2. Each file under PROD, laid out as `--datasets` is (A.B, or A.B/M for a member), is compared byte
   for byte with the data set of that name the job left; each difference is located by line and
   offset, and a data set the job did not leave is a difference.
3. `--expected STEPS=FILE` adds production's step outcomes from its job log, one a line
   (`STEP RC=0004`, `CALLER.PSTEP ABEND S0C7`), each compared with the step's outcome in the job.
4. `--declare FILE` lists intended divergences, one a line: `DATASET DSN [lines A-B] reason` or
   `STEP NAME reason`.

The statement's `predicateType` is
`https://github.com/Portll/ironwork/blob/main/docs/evidence.md#job-equivalence-v1`, apart from
`equivalence-v1` because its subjects are not a base and a head program: they are `job:<file>`, the
JCL, and `program:<file>` for each COBOL program the job ran, by digest. The predicate holds
`verdict`, `job`, `inputs` (each data set and its digest before the run), `sqlRecording`, `steps`
(each step, its program and its outcome as the job log shows it), `results`, `declared`,
`inconclusive`, `coverage` and `limit`. The verdicts and exit statuses are those of `compare`; a
step that reached what ironwork does not model makes the verdict `inconclusive`.
