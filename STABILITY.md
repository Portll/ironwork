# Stability

What a release of ironwork for COBOL keeps working for whoever builds on it, and what it may change.
The promises hold from 1.0. 0.9.0 is the release candidate: what it ships is what 1.0 keeps, and a
change to it before 1.0 is named in the release notes.

Attempted stability is for maintenance and adaptability purposes and is not guaranteed

## What is stable

These change only with a new major version, or, for a file format, a new version of that format,
which a reader of the old one refuses by name:

- **The `ironwork` command.** Its commands (`run`, `check`, `compile`, `cics`, `job`, `dump`,
  `fuzz`, `compare`, `assumptions`), their flags and what each flag means, the defaults, and the
  exit statuses ([README](README.md#exit-status), [docs/run-endings.tsv](docs/run-endings.tsv)).
- **Messages.** The shape of a line on standard error ([README](README.md#compiler-messages)) and
  each message's id ([docs/messages.md](docs/messages.md)): an id keeps its meaning and is never
  given to another message. A message's wording may change.
- **`--diagnostics json`.** The keys each object holds and what they hold.
- **The formats ironwork writes and reads,** each at version 1:

| Format | Version | Defined in |
|---|---|---|
| Load module (`.iwm`) | 1.1, reading every 1.x | [docs/load-module.md](docs/load-module.md) §8.1 |
| Run journal (`--evidence`) | `cobolwork-evidence/v1` | [docs/evidence.md](docs/evidence.md) §1, cobolwork's kinds table |
| Statement list (`--trace-statements`) | 1 | [docs/evidence.md](docs/evidence.md) §1.2 |
| Build provenance and equivalence statements | `check-v1`, `equivalence-v1`, `job-equivalence-v1` | [docs/evidence.md](docs/evidence.md) §2 to §4 |
| Fuzz manifests | `ironwork-fuzz/v1`, `ironwork-fuzz-interface/v1` | [docs/evidence.md](docs/evidence.md) §5, the schemas in `docs/` |
| SQL recording (`--sql-record`, `--sql-replay`) | 1 | [docs/sql-runtime.md](docs/sql-runtime.md) §8 |

A command or flag that a major version renames keeps its old name as an alias through that major,
and one it removes warns where it is used for at least one minor release first.

Each format's document says what a later minor may add without a new version: an optional section
or field at a section's end for a load module, a key for a manifest. A journal's kinds and fields
are cobolwork's table, which cobolwork extends before ironwork writes the addition.

## What a minor release may change

- **Additions:** commands, flags, messages and their ids, and what each format's rules allow.
- **Results that move toward Enterprise COBOL.** Matching IBM's compiler byte for byte is the
  target, so a result found to differ from IBM's, and fixed, is a bug fix even when a program's
  output changes. The fix names the assumption it settles or changes (`ironwork assumptions`), and
  the release notes name the fix.
- **A construct ironwork refused** may compile and run.

## What is not stable

- **The Rust crates.** `ironwork-rt`, `ironwork-syntax`, `ironwork-compile`, `ironwork-exec`,
  `ironwork-numeric`, `ironwork-zarch`, `ironwork-jcl` and `ironwork-oracle` are published so that
  `cargo install ironwork` can build the command. Their Rust API carries no promise: any release may
  change it. Each pins the others at its own exact version, so a release builds only with the
  crates released with it. Use the command and the formats, not the crates.
- **Text for reading.** What `dump` prints, `assumptions`' table and the wording of a message.
- **Anything a document marks as a draft or unbuilt.**

## Supported versions

[SECURITY.md](SECURITY.md#supported-versions) says which releases get security fixes.
