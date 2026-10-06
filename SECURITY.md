# Security policy

## Reporting a vulnerability

Report privately through [GitHub's advisory form](https://github.com/Portll/ironwork/security/advisories/new),
or by email to <john@portll.net>. Please do not open a public issue for a vulnerability.

Expect an acknowledgement within three working days and an assessment within ten. If a report is
valid, the fix and the advisory are published together, and you will be credited unless you ask
not to be.

## Supported versions

ironwork for COBOL is before 1.0 and is in public preview. The `main` branch is the supported
version, and fixes are not backported.

## Checking a download

Each GitHub release carries `SHA256SUMS`, a CycloneDX bill of materials for each build, and
attestations GitHub signs for the release run that built them.

- **Checksums.** `sha256sum -c SHA256SUMS --ignore-missing`, in the directory holding the
  downloads.
- **Provenance.** `gh attestation verify <file> -R Portll/ironwork` checks that the file was
  built by this repository's release workflow from the tagged commit.
- **Bill of materials.** `ironwork-<version>.cdx.json` and `ironwork-tls-<version>.cdx.json` list
  the crates each build is made from. `gh attestation verify <archive> -R Portll/ironwork
  --predicate-type https://cyclonedx.org/bom` checks the one attested for that archive.
- **Rebuilding.** The release notes name the Rust version every build used. At the tag, with that
  version, `cargo build --release --locked -p ironwork --target x86_64-unknown-linux-musl` gives
  the bytes of the Linux x86-64 archive's binary. CI checks on every commit that two such builds,
  from checkouts at different paths, are the same bytes.

## What counts as a vulnerability here

ironwork compiles and runs COBOL programs, which may come from anywhere. A running program is
confined; anything that lets it act outside those bounds is scoped for vulnerabilities:

- **Reaching outside run-unit storage.** Without SSRANGE a subscript, a reference modification or a
  pointer can reach anywhere in the run unit's storage, as on z/OS, but never outside it. A way to
  read or write host memory beyond that is a vulnerability.
- **Opening a file no DD maps.** A program reaches only the files its DDs are given, by `--dd` or
  `DD_NAME` in the environment; `ASSIGN` names a DD, not a host path. A way to open, create or
  delete any other file at run time is in scope.
- **Execution.** Anything that makes a CLI command start a host process, load native code or open
  a network connection outside the bounds described below. The table shows per-command properties:

| Command | Reads Files | Writes Files | Starts Host Process | Opens Network | Listens |
|---------|:-----------:|:------------:|:-------------------:|:--------------:|:-------:|
| run | ✓ | ✓ | ✗ (✓ --evidence) | ✓ (--sql-db) | ✗ |
| check | ✓ | ✗ | ✗ | ✗ | ✗ |
| cics | ✓ | ✓ | ✗ (✓ --evidence) | ✓ (--sql-db) | ✓ (--serve) |
| compile | ✓ | ✓ | ✗ | ✗ | ✗ |
| dump | ✓ | ✗ | ✗ | ✗ | ✗ |
| job | ✓ | ✓ | ✗ (✓ --evidence) | ✓ (--sql-db) | ✗ |
| fuzz | ✓ | ✓ | ✓ (ironwork itself) | ✗ | ✗ |
| assumptions | ✗ | ✗ | ✗ | ✗ | ✗ |
| compare | ✓ | ✓ (copies of each DD, --statement) | ✗ | ✗ | ✗ |
| --version | ✗ | ✗ | ✗ | ✗ | ✗ |

The processes these start are fixed: `fuzz` runs each input in a new copy of the running
`ironwork`, and with `--evidence`, on macOS and Windows, `ps` or `tasklist` is asked whether the
process holding a stale journal lock still runs, with fixed arguments and that process's id. A
program never chooses a process to start: `CALL 'SYSTEM'` and the other routines that would run a
command on z/OS or under GnuCOBOL load a program of that name or fail. With DD PRINTER, `SYSTEM` or
`C$SYSTEM` given an `lp` or `lpr` command appends the files it names to that DD and runs nothing.

`unsafe` code is forbidden across the workspace.
- **A crash or unbounded cost from crafted source.** The front end must refuse bad input with an
  error, never panic, and never take memory or time without bound. It is fuzzed for this; an input
  that gets through is in scope.

## Limits

A run has no limit by default: an endless loop or growing storage is the program's behaviour, as
on z/OS. `run` and `job` take three, each checked as a statement starts:

| Flag | Default | When it is reached |
|---|---|---|
| `--statement-limit N` | none | S322 at the start of the loop the run is in, the same statement on every run |
| `--time-limit SECONDS` | none | S322 at a statement that starts once SECONDS have passed |
| `--storage-limit BYTES[K\|M\|G]` | none | the run ends at the next statement once the run unit's storage passes BYTES |

Each job step gets the whole of each limit. Without a storage limit, a single CICS GETMAIN or
Language Environment CEEGTST grants at most 256 MiB, and objects at most 1 GiB in all. A program
waiting on ACCEPT from standard input starts no statement, so none of these ends it. `fuzz`
always runs each input under a statement limit and a time limit of its own.

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
