#!/bin/sh
# Copies the fixtures cobolwork shares with ironwork: CardDemo's maps with the copybooks CICS
# generated from them, the host-variable table, region definitions, and the evidence record kinds. The copies must be byte for
# byte the same.
# usage: tools/sync-cobolwork-fixtures.sh <cobolwork checkout>
set -eu
to="$(dirname "$0")/../fixtures/cobolwork"
mkdir -p "$to/bms" "$to/sql" "$to/csd" "$to/evidence"
cp "$1"/test/fixtures/bms/*.bms "$1"/test/fixtures/bms/*.cpy "$to/bms/"
cp "$1/test/fixtures/sql/host-variables.tsv" "$to/sql/"
cp "$1/test/fixtures/evidence/kinds.tsv" "$to/evidence/"
for pair in entry/REGION.csd:entry intrdr/declared/REGION.CSD:intrdr-declared outbound/REGION.csd:outbound priv/REGION.csd:priv web-csd/REGION.csd:web-csd; do
    cp "$1/test/fixtures/${pair%%:*}" "$to/csd/${pair##*:}.csd"
done
