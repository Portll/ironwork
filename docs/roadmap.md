# cobolwork and ironwork: boundary, expansion and priorities

What each product owns, where they overlap, and what becomes an alternate.

**Status:** 2026-10-03. The boundary and overlaps reflect the operator's rulings.

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
| JCL | Both | cobolwork reads jobs. Running them belongs to ironwork (E7) |
| Db2 DDL | cobolwork | On cobolwork's language plan. ironwork's SQL backend takes schemas as fixtures |
| Oracles | Each its own | cobolwork grades against GnuCOBOL's listing and its bench; ironwork against Enterprise COBOL and Hercules |

**Placements, ruled by the operator on 2026-09-30:**

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

## Releases and priorities

Release contents and their order live in the SPINE plans `cobolwork-roadmap` and `ironwork-roadmap` and on the site's roadmap pages (https://ironwork.commitwork.online/ironwork/roadmap/ and https://cobolwork.commitwork.online/cobolwork/roadmap/). The steps toward the VM are detailed in [codegen-runtime.md](codegen-runtime.md). This file holds only the boundary between the two products.
