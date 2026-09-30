# Fixtures shared with cobolwork

Copies of cobolwork's `test/fixtures/bms/` and `test/fixtures/sql/`, and in `csd/` five of its region
definitions, named for where cobolwork keeps them. Both projects' tests read them. cobolwork holds
the originals; change them there, then run `tools/sync-cobolwork-fixtures.sh <cobolwork checkout>`.
[`crates/exec/tests/cobolwork_fixtures.rs`](../../crates/exec/tests/cobolwork_fixtures.rs) checks:

- that the symbolic map ironwork builds from each map lays out as the copybook CICS generated
  from it;
- that `syntax::sql` reads and writes each statement's host variables as the table says;
- that `syntax::csd` reads each region definition as cobolwork's `parseCsd` does;
- with `IRONWORK_COBOLWORK_DIR` naming a cobolwork checkout, as CI sets it, that these copies
  equal cobolwork's.

`bms/` is AWS CardDemo's `COSGN00` and `COCRDSL` maps with the copybooks CICS generated from them,
unchanged below a header naming where each came from. They are under the Apache License 2.0; see
[THIRD-PARTY-NOTICES.md](../../THIRD-PARTY-NOTICES.md).
