# Security policy

## Reporting a vulnerability

Report privately through [GitHub's advisory form](https://github.com/Portll/ironwork/security/advisories/new),
or by email to <john@portll.net>. Please do not open a public issue for a vulnerability.

Expect an acknowledgement within three working days and an assessment within ten. If a report is
valid, the fix and the advisory are published together, and you will be credited unless you ask not to be.

## Supported versions

ironwork for COBOL is before 1.0 and is in public preview.
The `main` branch is the supported version; 
Fixes are not backported.

## What counts as a vulnerability here

ironwork compiles and runs COBOL programs, which may come from anywhere. A running program is
confined; anything that lets it act outside those bounds is scoped for vulnerabilities:

- **Reaching outside run-unit storage.** Without SSRANGE a subscript, a reference modification or a
  pointer can reach anywhere in the run unit's storage, as on z/OS, but never outside it. A way to
  read or write host memory beyond that is a vulnerability.
- **Opening a file no DD maps.** A program reaches only the files its DDs are given, by `--dd` or
  `DD_NAME` in the environment; `ASSIGN` names a DD, not a host path. A way to open, create or
  delete any other file at run time is in scope.
- **Execution.** Anything that makes `ironwork check` or `ironwork run` start a host process, load
  native code or open a network connection. They do none of these, and `unsafe` code is forbidden
  across the workspace.
- **A crash or unbounded cost from crafted source.** The front end must refuse bad input with an
  error, never panic, and never take memory or time without bound. It is fuzzed for this; an input
  that gets through is in scope.

## What doesn't count

- **Compile-time reads of the files a program names.** A COPY member can be named by path,
  including an absolute path or one containing `..`, and the compiler reads it, as a C compiler
  reads an `#include`.
- **What the program itself does.** A loop that never ends, storage it fills, an abend such as
  S0C7: these are the program's behaviour, as they would be on z/OS.
- **A result that differs from IBM Enterprise COBOL.** That is a conformance bug. Open an issue
  with the program, its options and, if you have it, the output IBM's compiler produced.

## Scope

This repository: the `ironwork` driver, the library crates, the oracle and the fuzz target. The
oracle's `smoke` and `hercules` commands run GnuCOBOL and Hercules by design; vulnerabilities in
those, in the ICU code-page data, or in Zowe belong with those projects.
