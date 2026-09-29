# cobolwork and ironwork: boundary, expansion and priorities

What each product owns, where they overlap, what is done, what becomes an alternate, and what
needs doing, in priority order with relative sizes.

**Status:** draft, 2026-09-29, for the operator's review. It reflects the operator's rulings of the
same day:

- the two are "two sides of the same coin";
- SQL is next, ahead of the VM;
- the interpreter stays as an option;
- the runtime is AGPL with a runtime exception.

**Sizes** are relative, calibrated on ironwork's milestones.

| Size | Meaning |
|---|---|
| XS | A one-file change |
| S | About half a milestone |
| M | One milestone, the size of M6 (indexed and relative files) |
| L | Two to three milestones |
| XL | More than three milestones, or blocked by an outside party |

## 1. The boundary

**cobolwork reads estates.** It covers every dialect an estate holds (IBM, Micro Focus, GnuCOBOL,
ACUCOBOL and others) and every artefact beside the programs: copybooks, JCL, the CICS CSD, BMS
maps, and compiler options across installation defaults, PARM and CBL cards. It says what is wrong
and how sure it is: findings, evidence kinds, the build gate, the remediation gate, and compliance.
It is zero-dependency JavaScript.

**ironwork defines and executes IBM Enterprise COBOL.** It holds:

- the machine and compiler models, and the assumptions that record their choices;
- the oracle;
- the front end, the storage layout, the interpreter, and later the VM and code generation;
- the runtime services a program meets: files, CICS, BMS and the 3270 terminal, SQL, SORT, and
  Language Environment services.

It is zero-dependency Rust.

**Neither links the other.** They meet in three ways:

- **Shared data.** Tables and fixtures, copied into each repository and held equal by a drift test.
- **Knowledge as rules.** What ironwork learns about IBM's semantics becomes cobolwork analysis
  rules, as the ironwork README's boundary section says.
- **A process boundary.** cobolwork may run `ironwork` as a separate program, as it runs `cobc`.

| Capability | Owner | Note |
|---|---|---|
| Reading dialects other than IBM | cobolwork | ironwork refuses what IBM refuses |
| Security meaning: sources, sinks, CWE, evidence, severity | cobolwork | ironwork reports what happened, never what it means |
| Executing COBOL, and tracing what executed | ironwork | Interpreter, VM and runtime services |
| IBM storage layout | ironwork | The reference for IBM rules. cobolwork keeps its own, graded against GnuCOBOL, for flow |
| Compiler options | Both | cobolwork resolves where options come from; ironwork applies them. One option table as shared data |
| EXEC SQL | Both | cobolwork translates for flow and `cobc`; ironwork runs it. Shared statement fixtures |
| CICS commands | Both | cobolwork finds defects; ironwork runs them. One command table (`provenance/precompile.json`) as shared data |
| BMS maps | Both | Two parsers for two purposes. ironwork's symbolic maps are the reference; shared fixtures |
| JCL | Both | cobolwork reads jobs. Running them belongs to ironwork (proposed: E7) |
| Db2 DDL | cobolwork | On cobolwork's language plan. ironwork's SQL backend takes schemas as fixtures |
| Oracles | Each its own | cobolwork grades against GnuCOBOL's listing and its bench; ironwork against Enterprise COBOL and Hercules |

**Placements to rule on:**

- JCL execution and utilities go to ironwork.
- Dynamic confirmation of findings is orchestrated by cobolwork and traced by ironwork.
- The shared-data mechanism is vendored copies held equal by a drift test in each repository.

## 2. Where they overlap today

| Overlap | cobolwork | ironwork | Proposal |
|---|---|---|---|
| Compiler option table | `provenance/enterprise-options.json` (IBM's Table 45, each row cited), which generates `lib/enterprise-options.mjs` | `numeric::options` reads its spellings from the vendored `crates/numeric/data/enterprise-options.tsv` | Shared since 2026-09-30: cobolwork generates the table, ironwork vendors it byte for byte, and each side tests its option reading against it |
| CICS command table | `provenance/precompile.json` (each command cites IBM), `lib/cics-commands.mjs` | Hand-written in `machine/cics*.rs` | ironwork checks its dispatch against the shared table |
| Reserved words | `provenance/words.json` (each word sourced) | `syntax/src/lexer.rs` | ironwork's lexer tested against the shared list |
| BMS | `lib/bms.mjs` (453 lines) | `syntax/src/bms.rs` (914 lines) | Keep both. Shared map fixtures, with ironwork's symbolic maps as the expected output |
| Embedded SQL | `lib/precompile.mjs`: host-variable direction, WHENEVER, cursor inputs | Typed statements, run against PostgreSQL or a recording (`exec/src/sql/`) | Keep both. Shared statement fixtures; WHENEVER already specified alike |
| Storage layout | `lib/parser.mjs` sizes, graded against GnuCOBOL | `exec/src/layout.rs` | Keep both. ironwork's layouts become cobolwork fixtures for IBM-only rules once goldens exist |
| Corpus census | `diag/` corpus runners | `tools/census.py` | Share the corpus lists |

## 3. Done

**cobolwork**
- **Commands:** `scan`, `flow`, `diff`, `build`, `inventory`, `parse`, `baseline` and `gate`.
  Fourteen rule sets report into SARIF with fingerprints, evidence kinds, impact and remedy.
- **Parser accuracy:** graded against GnuCOBOL on 100, 300 and 500 held-out repository sets. On the
  500 set, data items reach 99.98% recall at 99.998% precision.
- **Benchmark:** 93 CWE-labelled bench cases, each with a near-miss negative.
- **Build gate:** policy, SSRANGE, and the compile step.
- **Precompiler:** SQL and CICS translation, built 2026-09-27.
- **Remediation gate:** built, but nothing calls it yet.
- **Compliance:** mappings to DORA, FFIEC and NIST SP 800-53.
- **CI:** 824 tests. On Windows, 798 pass, one fails on a path bug, and the rest record skips.
- **History:** squashed for publication.

**ironwork**
- **M1:** the front end and interpreter on EBCDIC storage, with the `zarch` machine and `numeric`
  compiler models, and the oracle generator.
- **M2:** COPY, sections, EVALUATE, edited pictures and sequential files.
- **M3:** subprograms, pointers, SET, ACCEPT and OCCURS DEPENDING ON.
- **M4:** STRING, UNSTRING, INSPECT, SEARCH, LOCAL-STORAGE and intrinsic functions.
- **M5:** EXEC SQL and EXEC CICS parsed and checked.
- **M6:** indexed and relative files.
- **M7:** CICS Tiers 1 and 2 as a harness.
- **Hercules:** every decimal case agrees; the two HFP disagreements were settled by the manual in
  `zarch`'s favour.
- **M8:** BMS maps and symbolic maps, SEND and RECEIVE MAP, the terminal model, scripted screens
  and the TN3270 server.
- **SQL, steps 1 to 7 of [sql-runtime.md](sql-runtime.md):** typed statements, conversion, the
  SQLCA and WHENEVER, record and replay, cursors, CICS SYNCPOINT, and the PostgreSQL backend.
- **CI:** exists.

**In flight**
- **Four branches:** SORT/MERGE, LE callable services, Report Writer and OO COBOL, integrated and
  landing (cobolwork-c2).
- **Specs:** [codegen-runtime.md](codegen-runtime.md), a draft, with its detail in
  [lir.md](lir.md), [semantics-library.md](semantics-library.md) and
  [load-module.md](load-module.md). Step 0 is measured in [benchmarks.md](benchmarks.md).

## 4. What becomes an alternate

| Primary | Alternate | When |
|---|---|---|
| The VM over LIR | The interpreter (`--interpret`), kept by ruling, differential-tested against the VM in CI | When the VM becomes the default |
| The VM | Native code emitted as Rust | Only if the VM misses its performance target |
| SQL record and replay | The PostgreSQL backend, for live data | When SQL step 7 lands |
| Scripted screens (`--screens`) | The TN3270 server, for a person at an emulator | Both stay; scripts for tests, TN3270 for people |
| `--dd NAME=path` | DD allocation from a job's JCL | If the JCL runner (E7) is built; `--dd` stays for single programs |
| `cobc` as the build gate's compiler | `ironwork check`, and later `ironwork compile`, for estates that target IBM | E1 |
| GnuCOBOL's listing as cobolwork's only external witness | ironwork layouts, for IBM-only size rules | After Enterprise COBOL goldens exist; before that, ironwork is not an independent witness |
| cobolwork's grading stand-in (`diag/precompiler.mjs`) | cobolwork's translation (`lib/precompile.mjs`) | Already both; the stand-in stays for grading only |

## 5. To do, in priority order

### P0: now

| # | Item | Repo | Size | Note |
|---|---|---|---|---|
| 1 | Fix the Windows test that builds `D:\D:\…` from a URL | cobolwork | XS | Done: cobolwork's CI is green |
| 2 | Commit approval: the cobolwork-web split | Operator | XS | M8, SQL and these specs have landed; the four-branch integration is landing |
| 3 | M8: the TN3270 server | ironwork | M | Done |
| 4 | Integrate SORT/MERGE, LE, Report Writer and OO | ironwork | M | In flight |
| 5 | Choose an Enterprise COBOL route for goldens: HDISV, ZD&T Enterprise Edition, or a client's system | Operator | XL to carry out | ironwork's central claim is unwitnessed until this happens |

### P1: next

| # | Item | Repo | Size | Note |
|---|---|---|---|---|
| 6 | SQL, [sql-runtime.md](sql-runtime.md) steps 1–6: typed statements (S), conversion (M), SQLCA, WHENEVER and single-row statements (M), the recording format and `--sql-replay` (S), cursors (S), CICS SYNCPOINT (XS), all six built | ironwork | L | Next by ruling. Written as a library service for both executors |
| 7 | Publish cobolwork: public repository, tagged release, npm decision, the PolyForm links fixed | cobolwork | S | Needs the operator's go-ahead |
| 8 | Shared data (E2): option table, CICS command table, reserved words, BMS and SQL fixtures, with drift tests | Both | M | The option table is shared (2026-09-30); the CICS command table, reserved words and fixtures remain |
| 9 | Rules from ironwork's model (E3): TRUNC(OPT) binary overflow, EBCDIC-dependent order and comparison, intermediates over 30 digits (31 under ARITH(EXTEND)) | cobolwork | M | None exists yet |
| 10 | A hand-labelled flow corpus: the independent witness for cobolwork's precision | cobolwork | L | Banks will ask for it |

### P2: after SQL

| # | Item | Repo | Size | Note |
|---|---|---|---|---|
| 11 | The VM (§6) | ironwork | XL | After SQL, by ruling |
| 12 | The PostgreSQL backend (SQL step 7) | ironwork | L | Built; TLS in the separate `tls/` build (D2); Q5 passes against PostgreSQL 14.19 |
| 13 | The build gate running `ironwork check` (E1) | cobolwork | S | `ironwork check` exists now, exiting 12 on a compile error |
| 14 | A CICS region defined by a CSD, for the TN3270 server (E6) | ironwork | S | cobolwork already parses CSDs; share fixtures |
| 15 | Crash-fuzzing programs through ironwork (E5) | Both | L | S0C7, S0C4 and SSRANGE become findings, each with its input |
| 16 | Dynamic witness (E4): `cobolwork confirm` runs ironwork with a payload | Both | L | Needs an ironwork trace of which input bytes reached which operation |
| 17 | A caller for cobolwork's remediation gate | cobolwork | M | BACKLOG item |
| 18 | JCL runner (E7) and migration equivalence testing (E8) | ironwork | XL, then L | E8 builds on SQL replay and E7 |

### P3: later

| # | Item | Repo | Size | Note |
|---|---|---|---|---|
| 19 | Native code from LIR | ironwork | L | Only if the VM misses its target |
| 20 | cobolwork language coverage: PL/I, HLASM, IMS, Db2 DDL | cobolwork | XL | A decision, per BACKLOG |
| 21 | Dynamic SQL, multi-row FETCH, LOBs, DRDA | ironwork | L to XL | Out of scope in sql-runtime.md |
| 22 | Coverage reports from ironwork runs (E9); DDL to PostgreSQL schemas (E10) | Both | M each | |
| 23 | cobolwork BACKLOG items: git-ref source tree, the three extractions, utility knowledge-base rows, PCI and COBIT mappings, the gitleaks pull request | cobolwork | S to L each | As BACKLOG.md lists them |

## 6. What the VM needs

These are the steps of [codegen-runtime.md](codegen-runtime.md), with the interpreter kept.

| Step | Work | Size |
|---|---|---|
| 0 | Benchmarks (file I/O, packed arithmetic, table search, CALL-heavy code) and baseline times for the interpreter and `cobc -O2` | S |
| 1 | Extract the semantics library from `Machine` (storage access, MOVE, compare, editing, arithmetic stores, abends and every service) and split out `rt`. The boundary test. SQL, built first, is already library-shaped | L |
| 2 | The LIR: resolved places, arithmetic plans, basic blocks with explicit PERFORM exits (V1, V2), typed service calls and a debug table. The lowering from `Compiled`, covering everything the interpreter runs by then, including SORT, LE, Report Writer, OO and SQL | L |
| 3 | The VM executor in `rt` | L |
| 4 | A permanent differential CI job running every test and oracle case in both executors, plus differential fuzzing | M |
| 5 | The load module: binary format, writer and reader, reproducibility | M |
| 6 | `RunUnit` loading modules; static and dynamic CALL | M |
| 7 | The VM as default, with `--interpret` kept | XS |
| 8 | Oracle cases for V1 and V2, settled when goldens exist | S, blocked by P0 #5 |

Total: XL, about seven to nine milestones. Step 1 decides the rest. If every result is decided in
the semantics library, the VM is a second walker of the same calls, and keeping the interpreter
costs nothing but CI time.

## 7. Expansion

| ID | Capability | Priority | Size |
|---|---|---|---|
| E1 | The build gate compiles IBM estates with ironwork | P2 | S |
| E2 | Shared data between the repositories | P1 | M |
| E3 | cobolwork rules from ironwork's numeric and code-page model | P1 | M |
| E4 | Confirmed findings: an ironwork run shows the input reaching the sink. This would be a new evidence kind, and it needs the operator's ruling, because cobolwork has so far claimed only what it read | P2 | L |
| E5 | Fuzzing COBOL programs for abends, reported as findings with their inputs | P2 | L |
| E6 | CSD-defined CICS regions for the TN3270 server | P2 | S |
| E7 | A JCL runner: steps, DD allocation, COND and IF, in-stream data, and the utilities cobolwork's `lib/utilities.mjs` documents with IBM citations | P2 | XL |
| E8 | Migration equivalence: run a job under ironwork against recorded SQL and files, and compare with production's outputs | P2 | L |
| E9 | Execution coverage feeding cobolwork, to mark findings in code a test reached | P3 | M |
| E10 | Db2 DDL to PostgreSQL schemas for SQL tests | P3 | M |

## 8. Outside the code

| Item | Who | Size |
|---|---|---|
| Runtime exception text and its eligibility condition | A practitioner | S |
| LICENSING.md for both products, with the uncapped prices of 2026-09-29 | Drafted on the operator's go-ahead | S |
| Title opinion on cobolwork's word lists; the ACL addendum; trade marks | A practitioner | S each |
| Making the repositories public | Operator | XS, after P0 #1 and P1 #7 |
| The goldens route, P0 #5 | Operator | Decision |
