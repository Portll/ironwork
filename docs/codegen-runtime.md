# Code generation and the runtime

A specification for compiling COBOL ahead of time, and for the runtime that compiled programs link.

**Status:** steps 0 to 5 of §14 are done: the VM runs programs by default. Step 6, native code, is
under way for 1.1 (§14): `ironwork compile --native` builds a program into an executable, and step
7's text waits on a practitioner's review. It builds on the
operator's rulings of 2026-09-29:

- **The runtime licence.** The runtime is AGPL-3.0-or-later with a runtime exception, so a program
  compiled by ironwork is not bound by the AGPL. The exception is published as
  [RUNTIME-EXCEPTION.md](../RUNTIME-EXCEPTION.md), version 1.0.
- **SQL comes first.** SQL ([sql-runtime.md](sql-runtime.md)) is built before any of this. Steps 1 to 7
  have landed.
- **The interpreter stays.** It remains an option beside the VM, and is not retired.

The detail lives in four companion documents:

| Document | Covers |
|---|---|
| [benchmarks.md](benchmarks.md) | Step 0: the four benchmark programs and the walker and `cobc -O2` baselines |
| [semantics-library.md](semantics-library.md) | Step 1: every `Machine` method classified, the AST leaks, the `rt` layout, the boundary test and the extraction order |
| [lir.md](lir.md) | Step 2: the LIR types, the lowering of every statement, PERFORM exits (V1, V2) and the debug table |
| [load-module.md](load-module.md) | Step 4: the `.iwm` format, its encoding, reproducibility, and loading and CALL |

---

## 1. Why this exists

- **Nothing can be deployed today.** `ironwork run` reads source and interprets it. A shop cannot
  compile once and run without the source, which is how every COBOL estate runs.
- **The interpreter re-resolves at run time.** `Machine::exec` walks `ast::Stmt` and resolves each
  `Ref` through `locate`, memoised in a `HashMap`, on every execution. Nobody has measured the cost,
  and batch volumes will find it.
- **The licence needs a boundary.** A runtime exception only means something if it covers the code a
  compiled program links, and no other code. Today `crates/exec` holds both: the layout and `Check`
  pass a compiler needs, and the storage, file, CICS and terminal services a running program needs.
- **The PERFORM model is implicit.** `PerformProc` recurses in Rust and a range ends when
  `run_paragraphs` reaches its last paragraph. Programs that leave a range by GO TO, or overlap
  ranges, depend on how the compiler records where each range returns, and the walker's answer is
  implicit in Rust recursion. Lowering makes that choice explicit and testable.

## 2. Ubiquitous language

| Term | Meaning |
|---|---|
| Compiler | The crates that read source and produce target code: `syntax`, the resolver and layout, the lowering, the emitters, and the `ironwork` driver |
| Runtime | The crates present when a compiled program runs: `zarch`, `numeric` and `rt` |
| LIR | The lowered, resolved program: every data reference resolved to storage, every PERFORM exit explicit, every service call named |
| Load module | A file holding one source file's programs in LIR, with their layout, compile options, debug table and BMS maps. Its extension is `.iwm` |
| Target code | A load module, or Rust source emitted from LIR together with the object code built from it |
| Semantics library | The functions both executors call: storage access, MOVE, compare, editing, arithmetic stores, abends, and the file, CICS, BMS and SQL services. They are `rt`'s (§6) |
| Executor | What walks a program: the interpreter (`Machine` over the AST, kept as `--interpret`) or the VM (over LIR) |
| Place | The LIR's static description of a data reference: base, constant offset, subscript and reference-modification expressions, ODO, and SSRANGE checks |
| Loc | A Place evaluated at run time: a concrete offset, length and kind. The semantics library takes `Loc`s, never Places or AST types |

## 3. Constraints the tree sets

- **No third-party dependencies.** Every crate in the workspace depends only on path crates. The
  exceptions sit in separate workspaces with their own lockfiles: `libfuzzer-sys` in `fuzz/`, and
  rustls in `tls/`, the optional TLS build of the PostgreSQL backend. The runtime crates stay
  dependency-free. This rules out Cranelift and LLVM, unless the operator relaxes the rule on
  purpose (D3).
- **No `unsafe`.** `[workspace.lints.rust] unsafe_code = "forbid"`. This rules out emitting C that
  calls a Rust runtime, because the FFI boundary needs `unsafe`. Any generated Rust inherits the
  rule.
- **One storage model.** `RunUnit.mem` is one `Vec<u8>` in z/OS layout: an 8-byte reserved area with
  RETURN-CODE at offset 0, one 8-aligned slab per program, and pointers as `ADDRESS_BASE +
  offset`. Byte-for-byte fidelity is the product, so compiled code keeps exactly this model:
  overlays, S0C4 on a linkage item with no address, and reach anywhere in the run unit without
  SSRANGE.
- **Edition 2024**, with no MSRV declared.

## 4. Options

1. **Emit Rust source and build it with cargo against the runtime crates.** The result is native
   code, safe Rust (each generated crate carries `#![forbid(unsafe_code)]`), and still no
   third-party dependency. It costs a Rust toolchain on the build machine, as `cobc` needs a C
   compiler, and rustc time on large generated sources.
2. **Emit C.** Compilation is fast and a compiler is everywhere, but calling the Rust runtime needs
   `unsafe` FFI, and rewriting the runtime in C would be a second semantics. Rejected.
3. **An own native backend for x86-64 and aarch64.** It needs no toolchain, but means register
   allocation, calling conventions and object formats, with safety argued by construction rather
   than checked by the compiler. Rejected for now.
4. **Cranelift or LLVM.** Both are mature backends, and both break the dependency rule. Rejected
   unless D3 changes the rule.
5. **Compile to LIR and run it on a VM in the runtime.** It needs no toolchain and no dependency,
   compiles fast, and is safe. It is slower than native and much faster than the AST walk, because
   addresses, strides and arithmetic plans are fixed at compile time. The load module is data the
   runtime executes.

## 5. Recommendation

**Lower first; run the LIR on a VM; then emit Rust from the same LIR, in ironwork 1.1.**

- The LIR is needed by every backend, including a future native one, so building it first
  wastes nothing.
- The LIR, the VM and the load module deliver compile-then-run and a deployable artifact with no
  toolchain on the target machine.
- They also give the runtime exception an exact edge, because the VM and its services are the
  runtime.
- **Two executors, one semantics.** The interpreter stays as an option (operator, 2026-09-29).
  Everything that decides a result lives in a semantics library that the interpreter and the VM
  both call, so the executors differ only in how they walk the program. Differential tests of
  the two run in CI permanently.

## 6. The split

Every crate, by module. A module in brackets is private to its crate.

| Crate | Holds | Depends on | Licence |
|---|---|---|---|
| `zarch` | The machine: `decimal` (packed and zoned decimal, and the decimal instructions), `hfp` (hexadecimal floating point), `wide` (256-bit products), `check` (program checks, the condition code and the program mask), `ebcdic` (the code pages) | none | AGPL + exception (runtime). The ICU tables stay under the Unicode License v3 |
| `numeric` | IBM's numeric rules: `precision` (intermediate results), `binary`, `float`, `sign`, `zoned`, `options` (the compiler options), `assumptions` (the register of assumptions) | `zarch` | AGPL + exception (runtime) |
| `rt` | Every function that decides a result, every service, and the compiled form. **Vocabulary and storage:** `vocab` (`Pos` and the enums the front end shares), `storage` (`Kind`, `Loc`, `Val`), `picture` (`Sym`), `abend` (`Abend`, `AbendCode`, `Signal`, `FileStatus`, `Ending`), `fixed`, `codec`, `loc` (the location checks), `store` (reads, MOVE, compare and the numeric stores), `host` (what a statement asks of the executor running it). **Statements:** `arith`, `edit`, `strings`, `text` (STRING, UNSTRING and INSPECT), `set`, `accept`, `display`, `intrinsic` (the functions), `calendar`, `json`, `xml`, `parmcheck`. **The run unit:** `unit` (`RunUnit`, `Loader`), `callee` (the one callee sequence and the CALL USING addresses), `taint`, `evidence` (run journals), `digest`. **Services:** `files`, `fileio` (the file verbs), `linage`, `printer`, `virtual_printer`, `sort`, `report`, `le`, `feedback`, `oo`, `jni`, `cics`, `cics_tables`, `bms` (the map model and its slots), `terminal`, `tn3270`, `sql` (see [sql-runtime.md](sql-runtime.md)), `reserved_words`. **The compiled form:** `lir`, `vm`, `module` (the load-module codec, reader and writer, and `Modules`, the modules a run reads programs from) | `numeric`, `zarch` | AGPL + exception (runtime) |
| `syntax` | Source to syntax tree: `source` (reference format), `copy` (COPY and REPLACING), `lexer`, `parser`, `ast`, `report` (the REPORT SECTION), `sql` (EXEC SQL, typed), `dli`, `bms` (BMS map source), `csd`, `system` (the copy members IBM's products supply), `feedback`, `jni`, (`debugging`). It re-exports the vocabulary it shares with `rt` | `numeric`, `rt` | AGPL (compiler) |
| `compile` | `Compiled`, `compile` and `Check` in lib.rs; `layout`, `picture`, `collating`, `declaratives`, `function` (FUNCTION-ID), `linage`, `markup`, `numcheck`, `oo`, `printer`, `report`, `sort`, `sql` (host types), (`corresponding`, `initcheck`, `reserved`, `scope`) | `numeric`, `rt`, `syntax`, `zarch` | AGPL (compiler) |
| `exec` | The interpreter and lowering. `machine` is the walker: `Machine` over the AST, with its control flow (`perform`, `declaratives`, `scope`) and name resolution, and the adapters that build `rt`'s inputs from the AST and answer `rt`'s host traits (`facts`, `cics`, `cics_bind`, `sql`, `file_io`, `sort`, `report`, `oo`, `le_services`, `function`, `intrinsic`, `json`, `xml`, `parmcheck`). `lower` lowers a `Compiled` to the LIR. `loader` (`Library`, which CALL loads from), `unit` (the interpreter's `RunUnit`), `vm` (a program or a load module run on `rt::vm`, and `VmLibrary`, CALL's loader there), `oo` (where a class definition is found). `abend`, `le`, `printer`, `report`, `sql` and `terminal` re-export `rt` and `compile` modules, some with tests that compile COBOL against them, and lib.rs re-exports the `rt` and `compile` modules the driver names. (`testing`) is the test harness, which runs a program on both executors | `compile`, `numeric`, `rt`, `syntax`, `zarch` | AGPL (compiler) |
| `cli` (`ironwork`) | The driver: `main.rs` with `run`, `check`, `cics` and `assumptions`; `compile`, `dump`, `job`, `compare`, `fuzz` and `fuzz_cics`, `dfsort` and `dfsort_number`, and the `--evidence`, `--provenance` and coverage outputs (`evidence`, `provenance`, `coverage`). `tests/boundary.rs` is the boundary test | `exec`, `jcl`, `numeric`, `rt`, `syntax`, `zarch` | AGPL (compiler) |
| `jcl` | JCL read and procedures expanded (lib.rs), `cond`, `idcams`, `sort` (DFSORT control statements), `symnames` | none | AGPL (compiler) |
| `oracle` | Test programs whose results settle `numeric::assumptions` on a real compiler: lib.rs, `families`, `hercules`, and the binary that writes them. `exec` takes it as a dev-dependency | `numeric`, `zarch` | AGPL (compiler) |
| `tls` | A workspace of its own: the same driver with rustls, which supplies the `Tls` of `rt::sql` for `--sql-db` | `exec`, `jcl`, `numeric`, `rt`, `syntax`, `zarch`, rustls, webpki-roots | AGPL (compiler); rustls and webpki-roots under their own licences |
| `tools/ddl` (`ironwork-ddl`) | A development tool, not built into `ironwork` or published: Db2 for z/OS DDL as PostgreSQL DDL for the SQL backend's tests | none | AGPL (compiler) |

**BMS.** The map parser, `syntax::bms`, is the compiler's. The map model, `rt::bms`, and SEND MAP
and RECEIVE MAP, in `rt::cics`, are the runtime's (D4), and a load module has a `BMS` section for
the models.

**Dependency direction.** `rt` depends on `numeric` and `zarch` only, as `cargo tree -p
ironwork-rt` shows, and `syntax` depends on `rt`, never the reverse. `crates/cli/tests/boundary.rs`
reads the manifests of `zarch`, `numeric` and `rt`, and fails if one names `syntax`, `compile`,
`exec`, the driver, or any crate outside its rule ([semantics-library.md](semantics-library.md)
§6).

## 7. The LIR

Lowering consumes `Compiled` after `Check` has passed, and produces per program:

- **Data references** as `Place { base, offset, len, kind, dims, odo, refmod }`. `base` is one of
  WORKING-STORAGE, LOCAL-STORAGE, linkage ordinal or pointer, with the same meanings `locate` gives
  them now. Subscript and reference-modification arithmetic become LIR expressions, and the SSRANGE
  checks are emitted only when the program was compiled with SSRANGE.
- **Arithmetic plans.** Each COMPUTE, ADD, SUBTRACT, MULTIPLY and DIVIDE carries `dmax`, the
  ARITH mode, whether it is fixed or float, and its receivers' store plans. These are the values
  `Machine::arithmetic` now works out on every execution.
- **Control flow** as basic blocks per paragraph, with explicit `PerformEnter(range, loop)` and an
  exit check at the end of every paragraph that can close an active range. How an overlapping range
  or a GO TO out of a range resolves is a new assumption, V1, and how a SORT input or output
  procedure exits is V2. Both are in a new V series for lowering and the VM, recorded with basis
  `Chosen` until the oracle settles them. The walker's current behaviour is the baseline.
- **Service calls**, named and typed: file verbs with their FD, ACCEPT and DISPLAY, CALL and CANCEL,
  EXEC CICS commands as `Machine::cics` dispatches them, and EXEC SQL statements as
  `sql-runtime.md` types them.
- **A debug table** mapping each LIR instruction to its source file, line and column, so that every
  abend names the COBOL position it names today.

## 8. The load module

- **Contents.** One file per source file: the format version, compile options, layout, the LIR of
  each program, the BMS map models, and the debug table.
- **Encoding.** A fixed binary encoding written and read by ironwork's own code, with no
  serialisation crate.
- **Reproducible.** The same source, libraries and options give a byte-identical module.
- **Loading.** `RunUnit::load` looks up a program in the modules already loaded, then in the
  library directories: `NAME.iwm` first, then source compiled in memory. This keeps today's
  `-L` behaviour for callers that ship source.
- **Static and dynamic CALL.** A static CALL (NODYNAM) resolves within the module, or within a
  bundle linked at build time. A dynamic CALL loads by name at run time. Either way a missing
  program still raises ON EXCEPTION or ends as C450 says: CEE3501S and U4038 for a dynamic CALL,
  the binder's IEW2456E as ironwork's refusal for a static one.

## 9. The runtime exception

- **Form.** The exception is an additional permission under AGPLv3 §7, on the model of GCC's Runtime
  Library Exception 3.1. It will sit in a `RUNTIME-EXCEPTION` file and be named in the SPDX header
  of every file in `zarch`, `numeric` and `rt`.
- **What it permits.** Anyone may distribute a work combining target code with the runtime on terms
  of their choice, provided ironwork's compiler produced that target code.
- **What it does not cover.** The compiler crates stay AGPL without exception. Embedding the
  compiler in a product still needs the AGPL, or a PolyForm or negotiated grant.
- **Plugins.** ironwork has no plugin or IR-export interface. If one is added, the eligibility
  condition must stop target code produced through a non-free pass from qualifying, which is the
  hole GCC's condition closes.
- **Wording.** Published as [RUNTIME-EXCEPTION.md](../RUNTIME-EXCEPTION.md), version 1.0, with a
  copy and SPDX naming in each crate. Its "Compiled Program" includes intermediate code, so a load
  module qualifies.

## 10. Invariants

1. **One semantics.** For every test and oracle case, the interpreter, the VM and native code
   produce the same storage bytes, files, DISPLAY output, RETURN-CODE and abend (code and position).
   Every case runs in every executor it has, in CI.
2. **The storage model is unchanged**, as §3 states it.
3. **Every abend names its COBOL position.**
4. **The runtime never depends on the compiler.** The manifest test enforces it.
5. **No `unsafe` and no third-party dependency** in the runtime or in any generated code.
6. **Load modules are reproducible.**
7. **Compile options are fixed at compile time**, as IBM's are. A load module carries the options
   it was compiled with.

## 11. Specification (BDD)

### B1: Compiling and running

- **Given** `PAYROLL.cbl` **when** `ironwork compile PAYROLL.cbl -o out/` **then** `out/PAYROLL.iwm`
  exists, **and** `ironwork run out/PAYROLL.iwm` produces the same output and exit status as
  `ironwork run PAYROLL.cbl` does today.
- **Given** a load module whose source has been deleted **when** it is run **then** it runs, and an
  abend still prints `PAYROLL.cbl:LINE:COL: ABEND S0C7: …`.
- **Given** the same source and options compiled twice **then** the two modules are byte-identical.

### B2: One semantics

- **Given** every test in `exec/src/tests.rs` and every oracle case **when** run by the walker and
  by the VM **then** storage, output, RETURN-CODE and abend agree.
- **Given** the front-end fuzz target extended to run what it compiles with a step limit **when** a
  program runs in both modes **then** the modes agree, or both stop at the step limit.

### B3: PERFORM exits (V1)

- **Given** `PERFORM A THRU C` where B does `GO TO D` **when** it runs **then** the result is what
  V1 records. The walker's current result is recorded beside it, and any difference is reported as
  a change of semantics, not hidden.
- **Given** two overlapping ranges **then** the same applies.

### B4: CALL and the run unit

- **Given** `MAIN.iwm` calling `SUB` dynamically, with `SUB.iwm` in a `-L` directory **then** SUB
  runs, shares memory, keeps its WORKING-STORAGE between CALLs, and CANCEL resets it.
- **Given** no SUB anywhere **then** ON EXCEPTION runs, or the run ends as C450 says.

### B5: The boundary

- **Given** the workspace manifests **when** the boundary test runs **then** it fails if `rt`,
  `numeric` or `zarch` depends on `syntax`, `compile` or `cli`.
- **Given** Rust emitted from any test program **then** it contains `#![forbid(unsafe_code)]` and
  depends only on `rt`, `numeric` and `zarch`.

### B6: Performance

- **Given** the benchmark programs of step 0 **when** run by the VM **then** each meets its
  class's target in [benchmarks.md](benchmarks.md#vm-target): at most a fifth of the walker's time
  for call- and dispatch-bound programs, a third for decimal arithmetic, and 1.25 times `cobc -O2`
  for I/O-bound programs. **Given** native code **then** it is within 1.5 times the time of
  `cobc -O2` on the same program.

## 12. Out of scope

- **z/OS output.** Load modules or s390x object code for z/OS. The target is IBM's behaviour, run
  off the mainframe.
- **Language Environment run-time options** beyond those the interpreter already models.
- **ALTER.** It is not parsed today.
- **A debugger interface.** The debug table makes one possible later.

SORT and MERGE, Language Environment callable services, Report Writer and object-oriented COBOL are
integrated in the interpreter and landing on main. The LIR lowers each of them ([lir.md](lir.md)).

## 13. Decisions for the operator

- **D1.** Build the LIR VM first, then native Rust emission. Settled by the operator on 2026-09-29:
  SQL first, then the VM, with the interpreter kept. On 2026-10-07 the operator scheduled native
  code for 1.1, whether or not the VM meets B6.
- **D2.** Accept a Rust toolchain as a build-time requirement for native output.
- **D3.** Keep the no-dependencies rule, which excludes Cranelift and LLVM.
- **D4.** Keep the BMS parser in the compiler, with map models in the runtime.
- **D5.** Have a practitioner review the published exception text and its eligibility condition.

The companion documents carry their own open questions for the operator, listed in §15.

## 14. Execution plan

| Step | Work | Done when |
|---|---|---|
| 0 | **Measure.** Write four benchmark programs: sequential file read and write, packed arithmetic, table search, and CALL-heavy code. Time the walker and `cobc -O2` on them. | Done: [benchmarks.md](benchmarks.md). The walker takes 1.6 to 234 times `cobc -O2`; the VM target is stated per program class |
| 1 | **Extract the semantics library and split `rt`** out of `exec`, with no change of behaviour. This waits for M8, the SORT, LE, Report Writer and OO integration, and SQL (whose runtime is written as a library service from the start). Add the boundary test. | Done: [semantics-library.md](semantics-library.md) §8, E1 to E12. All tests pass, the boundary test passes, and `rt` depends on `numeric` and `zarch` only (§6) |
| 2 | **Build the LIR and the lowering** from `Compiled`, covering everything the interpreter runs by then, including SORT and MERGE, LE services, Report Writer, OO COBOL and EXEC SQL. Add assumptions V1 and V2. | Every test program lowers |
| 3 | **Build the VM** in `rt` on the semantics library. Run the interpreter and the VM on every test and oracle case as a permanent CI job. Extend the fuzz target. | B2 passes |
| 4 | **Add the load module**, `ironwork compile`, and module loading in `RunUnit`. | B1 and B4 pass for programs that lower: cli/tests/iwm_run.rs runs modules against their sources ([load-module.md](load-module.md) §12, L5 and L8) |
| 5 | **Make the VM the default.** `ironwork run file.cbl` compiles in memory and runs the VM; `--interpret` keeps the interpreter. | Tests pass in both executors |
| 6 | **Emit Rust**, in 1.1 (ironwork-roadmap 25). Under way: `ironwork compile --native` writes each module as a crate that runs it from the runtime alone (`rt::native`), its programs' blocks as generated Rust (`cli/src/codegen.rs`) that the VM's dispatch hands control to, and builds it with cargo. See §14.1. | B5, and B6 native |
| 7 | **Publish the exception.** `RUNTIME-EXCEPTION.md`, SPDX headers, and README and NOTICE are on main (6a6da25). | Text reviewed by a practitioner (D5) |

### 14.1 Native code

- **One semantics by construction.** A program's generated code is the VM's dispatch loop over its
  blocks, written out. An op or a branch the generator has a fast path for runs in generated code
  over the activation's storage (`rt::fast`); any other, and a fast path that declines, runs as the
  VM runs it (`rt::vm::Machine`). A fast path computes what the VM's own count path computes, from
  the same functions (`rt::count`, `store::digits`, `store::count_bytes`), and writes nothing where
  it declines. A run anything watches (taint, statement tracing, limits, an observer, NUMCHECK) is
  the VM's alone.
- **Places at code generation.** Each value's places follow from its PICTURE, its literal, or its
  operands' places, dmax and ARITH (`count::result_places`), so generated code carries bare `i64`
  counts and the places as constants.
- **The run.** A native executable runs as `ironwork run module.iwm` does: the same flags for a
  batch run, messages and exit statuses (`rt::exit`, `rt::batch`). With no compiler at hand, CALL
  finds a program in its module or as `NAME.iwm` in its `-L` directories, never from source.
- **Testing.** `ironwork compile --native-harness` builds every module written into one test
  executable, and `tools/native-diff.py` runs each NIST CCVS85 program as `ironwork run X.iwm` and
  natively, comparing exit status, standard output, standard error and files.
- **B6, M5 Pro cycles against `cobc -O2`:** packed, callheavy and seqio are within it (0.7, 1.0 and
  0.4 times); tblsrch's linear SEARCH is 3.8 times.

**Verification.** Every step keeps today's tests and oracle cases passing. V1, V2, and any assumption
lowering forces, are recorded in `numeric::assumptions::ASSUMPTIONS` with their basis, and settled
only by an Enterprise COBOL run, as the numeric model's are.

## 15. Open questions from the companion documents

Each is argued in the document named, and none blocks step 1.

| # | Doc | Question |
|---|---|---|
| Q1 | [lir](lir.md) | Which oracle settles V1 (PERFORM-range exits), and must it be settled before step 5 makes the VM the default? |
| Q2 | lir | Does the VM keep Rust recursion for CALL, INVOKE, SORT procedures and USE BEFORE REPORTING, or keep its own stack? |
| Q3 | lir | Are the walker's four remaining known divergences from IBM fixed in step 2, or only once an oracle confirms them? (Condition-names finding their variable by name was fixed in both executors.) |
| Q4 | lir | Does an abend carry its program, so that its file name is right in a multi-program run? |
| Q5 | lir | Answered: MOVE CORRESPONDING, PERFORM VARYING … AFTER and GO TO … DEPENDING ON are all parsed and run; the compiler expands CORRESPONDING before lowering. |
| Q6 | [sem](semantics-library.md) | Answered: `syntax` depends on `rt` and re-exports the shared vocabulary. |
| Q7 | sem | Does the runtime exception cover `tn3270.rs`, the TN3270 server? |
| Q8 | [lm](load-module.md) | Answered: no reader is kept for a previous major version; a module is compiled again. |
| Q9 | lm | Answered: CRC-32 only; a module carries no keyed signature. |
| Q10 | lm | Answered: no `--strip-debug`; every module has its debug table. |
| Q11 | lm | Answered: a `NAME.iwm` beside newer source runs, and a compile that fails leaves it in place. |
| Q12 | lm | Answered: a run-time CALL by default; `--unresolved-calls fail` and `--le-services bind` give the other readings. |
| Q13 | lm | Answered: IBM's program scope by default, COMMON as the Language Reference gives it; `--program-scope flexible` reaches every program by name. |
| Q14 | lm | Answered: CANCEL acts only on a program a dynamic CALL entered, as the Language Reference documents. |
| Q15 | lm | Answered: abend lines name the bare file; `--source-prefix` gives a path. |
