# Fixtures shared with cobolwork

Copies of cobolwork's `test/fixtures/bms/`, `test/fixtures/sql/` and `test/fixtures/evidence/`, and in
`csd/` five of its region definitions, named for where cobolwork keeps them. Both projects' tests
read them. cobolwork holds the originals; change them there, then run
`tools/sync-cobolwork-fixtures.sh <cobolwork checkout>`.
[`crates/compile/tests/cobolwork_fixtures.rs`](../../crates/compile/tests/cobolwork_fixtures.rs) checks:

- that the symbolic map ironwork builds from each map lays out as the copybook CICS generated
  from it;
- that `syntax::sql` reads and writes each statement's host variables as the table says;
- that `syntax::csd` reads each region definition as cobolwork's `parseCsd` does;
- with `IRONWORK_COBOLWORK_DIR` naming a cobolwork checkout, as CI sets it, that these copies
  equal cobolwork's.

`evidence/kinds.tsv` is generated from the table cobolwork's evidence writer and verifier hold each
record to. `crates/rt/src/evidence.rs` checks each kind ironwork's run journal writes against it.
With `IRONWORK_COBOLWORK_DIR` set, `crates/cli/tests/evidence.rs` also has cobolwork's verifier read
the evidence directory of real runs.

`bms/` is AWS CardDemo's `COSGN00` and `COCRDSL` maps with the copybooks CICS generated from them,
unchanged below a header naming where each came from. They are under the Apache License 2.0; see
[THIRD-PARTY-NOTICES.md](../../THIRD-PARTY-NOTICES.md).
