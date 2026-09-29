# Contributing

## The agreement

Every contributor signs the [Contributor Licence Agreement](CLA.md) once, before their first
change is merged. It is a licence, not an assignment: you keep the copyright in what you write.
It is governed by the law of South Australia, and the grantee is John Hancock trading as Portll,
passing to Portll on incorporation under section 9 without contributors needing to be asked again.

It exists because ironwork is to be offered under two licences — AGPL-3.0-or-later, and a
commercial licence for those whose policy or product cannot accept the AGPL. Offering both
requires one party to hold the right to license the whole work under either, and a contribution's
copyright stays with its author unless they grant otherwise. One unagreed contribution ends the
arrangement for the whole project.

Section 8 of the CLA is the reciprocal half: everything merged stays under AGPL-3.0-or-later
permanently, and the open distribution cannot be withdrawn.

A Developer Certificate of Origin sign-off is **not** sufficient here. The DCO certifies that you
had the right to submit the work; it grants no right to relicense it.

## How to sign

On your first pull request, CLA assistant comments with a link. Follow it, sign in with GitHub,
give your name and email address, and agree; the pull request's `license/cla` check then passes.
Nothing is merged until everyone who committed to the pull request has signed.

Contributing on behalf of an employer, or as a company: an authorised signatory should sign
instead, naming the individuals covered. Contact john@portll.net.

Once recorded, it covers everything you send afterwards. If the agreement's text changes, CLA
assistant asks again on your next pull request.

## What a change needs

- **Tests pass.** `cargo test` at the workspace root.
- **No `unsafe`.** `unsafe_code` is forbidden for the whole workspace, in `[workspace.lints.rust]`
  in `Cargo.toml`, and every crate inherits that with `[lints] workspace = true`. A new crate
  carries the same two lines.
- **No dependencies.** The workspace has none outside its own crates: every package in
  `Cargo.lock` is one of the workspace's `ironwork-*` crates. A change that needs one needs a
  reason first. `fuzz/` is a separate workspace that depends on `libfuzzer-sys`, and `tls/` is
  another, which builds the same `ironwork` command with rustls for `--sql-db` over TLS; nothing
  built from `crates/` depends on either.
- **The front end must not panic.** `cargo test` mutates real programs and fails on any panic;
  run it longer with `IRONWORK_FUZZ_ITERATIONS=30000 cargo test -p ironwork-exec mutated`.
- **A question the manuals leave open is an assumption, not a constant.** It goes in
  `numeric::assumptions::ASSUMPTIONS` with its basis — *recalled* (IBM documents it, and it was
  written from memory, so check it against the manual), *chosen* (IBM does not document it) or
  *observed* (seen on a related system the claim names, such as Db2 for Linux) — and the oracle
  that settles it: Hercules for a bare machine instruction, Enterprise COBOL for anything the
  compiler decides, Db2 for z/OS for what embedded SQL sees. The register is append-only: never
  reorder, remove or insert entries, because the C-series numbers of `ironwork assumptions
  --c-series` are positions in it.
- **The real compiler settles a prediction; GnuCOBOL does not.**
  `cargo run -p ironwork-oracle -- smoke <dir>` is a syntax check only: GnuCOBOL is ASCII and
  IEEE, and its results are not IBM's. `cargo run -p ironwork-oracle -- check <dir>` scores saved
  Enterprise COBOL job output against the model.
  An assumption no run has upheld is unsettled, not passing.
- **A code page is ICU's table, unchanged.** A new or updated file in `crates/zarch/ucm/` comes
  from the pinned icu-data commit, and its row in [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md)
  changes in the same commit.
- **No `Co-Authored-By` trailer** and no "Generated with …" line on any commit or pull request.

## Reporting a vulnerability

Do not open a public issue. Email john@portll.net.
