# A verifier for high-assurance builds

**Status:** backlog, 2026-09-30. Nothing here is built. It is written now so that, when it is built,
the evidence a qualification asks for is produced as the work goes rather than reconstructed after
it. Roadmap: E11, P3 #24.

## 1. Why

Where a program controls an aircraft, a medical device or a plant, a miscompilation is a hazard
rather than a defect report. The standards for those domains do not accept a compiler's output on
trust. Either the compiler is qualified, which ties every change to it to a new qualification, or
each compilation's output is checked by a separate tool and that tool is qualified instead. This
plan takes the second route: the verifier is small, changes rarely, and does not share code with the
compiler it checks, so ironwork can keep moving while a project's qualification rests on the
verifier.

## 2. What it checks

Translation validation, one compilation at a time. The verifier reads the source as the front end
resolved it, the compiler options, and the output: the LIR today, the `.iwm` load module once it
exists ([load-module.md](load-module.md)), and native code if P3 #19 is ever built. It decides
whether the output means what the source means under ironwork's semantics.

| Check | What must match |
|---|---|
| Storage | Every place the output reads or writes resolves to the offset, length and usage a layout computed independently of the compiler gives the source |
| Arithmetic | Each arithmetic plan carries the intermediate places the semantics library's rules give (assumption C1), and stores under the options in force (TRUNC, NUMPROC, ARITH) |
| Control flow | Every PERFORM range, exit (V1, V2), GO TO, fall-through and condition is preserved block for block |
| Calls and services | Each CALL, EXEC CICS, EXEC SQL, SORT and Language Environment service is called with the same arguments in the same order |
| Data | Every VALUE, literal and constant appears with the bytes the source gives it under the program's code page |

Each compilation gets one verdict: **accepted**, **rejected** with the first divergence located in
both source and output, or **not decided** where the output uses a construct the verifier does not
cover. A construct it does not check is never reported as accepted.

Each run writes a verification record: the hashes of the source, the COPY members, the options, the
output and the verifier binary; the verdict; and the constructs checked, so a reviewer can see what
the verdict covers.

## 3. Independence

- **Its own crate, `verify`,** with a boundary test in the style of `crates/cli/tests/boundary.rs`.
  The test refuses any dependency on the lowering, the VM or code generation.
- **What it may share with the compiler** is the specification: the AST the front end produces and
  the semantics library as the definition of what a statement means. It may not share the code that
  turns one into the other.
- **A second layout computation,** written separately and checked against the same goldens as the
  compiler's. Storage layout is where a shared bug would hide in both.
- **Deterministic:** no network, no clock and no unordered iteration. The same inputs give the same
  record, byte for byte.

## 4. What it checks against

The verifier is only as right as the semantics it checks against, and ironwork records the basis of
each semantic choice in its assumption register (`numeric::assumptions`).

- A qualification can rest only on entries whose basis is **Documented** or **Observed**.
- Any **Chosen** or **Recalled** entry an accepted verdict relies on is listed in that verdict's
  record as an open assumption.
- The Enterprise COBOL goldens (P0 #5) are what move entries to Observed. Until they exist, every
  record will carry open assumptions, and the kit in §5 says so.

## 5. Built for qualification

A tool is qualified for a use within a project, never in general. ironwork can therefore ship a
qualification kit, not a qualification: the data each project's assessor needs, produced as the
verifier is built. The work builds up the following:

| Item | What it is |
|---|---|
| Tool operational requirements | What the verifier must detect, one requirement per check in §2, each with an id (`VR-n`) |
| Requirements-based tests | Each test names the `VR-n` it covers. A test holds the trace both ways, as cobolwork's build-gate suite holds its spec's scenario ids to its tests |
| Fault seeding | Each `VR-n` carries seeded miscompilations the verifier must reject: a wrong offset, a dropped exit, a lost digit, a swapped argument |
| Structural coverage | Coverage of the verifier's own code, to the level the target standard and level require |
| Configuration record | Reproducible builds of the verifier, and its binary's hash in every verification record |
| Known limitations and problem reports | Append-only, with ids that are never reused, like the assumption register |
| Plan and summary | The tool qualification plan and accomplishment summary, or the equivalents the target standard names |

## 6. Standards to read before building

The project quotes a standard only from its own text, and none of these is held. Each row names what
to read, not a claim about what it says.

| Domain | Standard | What to read for |
|---|---|---|
| Airborne software | RTCA DO-178C / EUROCAE ED-12C, with DO-330 / ED-215 | How a tool that replaces review of compiler output is classified, and what its qualification level asks for |
| Airborne, formal methods | RTCA DO-333 / EUROCAE ED-216 | What is needed if a check in §2 is argued formally rather than by test |
| Medical devices | IEC 62304; ISO 13485 | The software life cycle, and validation of software used in producing the device |
| Automotive | ISO 26262-8 | Confidence in the use of software tools |
| Industrial | IEC 61508-3 | Tool classes, and what a tool whose output becomes executable code needs |
| Railway | EN 50716 (successor to EN 50128) | Tool classes |

## 7. What it needs first

1. **VM step 1** ([roadmap.md](roadmap.md) §6): the semantics library, the specification the
   verifier checks against.
2. **VM step 2:** the LIR. The interpreter translates nothing, so it gives the verifier no output to
   check.
3. **VM step 5:** a reproducible load module, so a record's hash names one artefact.
4. **P0 #5:** the goldens, without which every verdict carries open assumptions.

## 8. Size and what it leaves out

L to XL after VM step 2. The qualification data a project then assembles is that project's own
effort, for each standard and level.

Out of scope:

- qualifying the compiler itself;
- proving the compiler correct once for all inputs, the verified-compiler route (CompCert is the
  known example), which is a larger undertaking than this.
