#!/bin/sh
# Copies the fixtures cobolwork shares with ironwork: CardDemo's maps with the copybooks CICS
# generated from them, and the host-variable table. The copies must be byte for byte the same.
# usage: tools/sync-cobolwork-fixtures.sh <cobolwork checkout>
set -eu
to="$(dirname "$0")/../fixtures/cobolwork"
mkdir -p "$to/bms" "$to/sql"
cp "$1"/test/fixtures/bms/*.bms "$1"/test/fixtures/bms/*.cpy "$to/bms/"
cp "$1/test/fixtures/sql/host-variables.tsv" "$to/sql/"
