#!/bin/sh
# Copies cobolwork's CICS tables (the commands and their options, DFHRESP and DFHVALUE, generated
# from IBM's CICS TS references) into ironwork's runtime. The two must be byte for byte the same.
# usage: tools/sync-cics-tables.sh <cobolwork checkout>
set -eu
for table in cics-commands dfhresp dfhvalue; do
    cp "$1/provenance/$table.tsv" "$(dirname "$0")/../crates/rt/data/$table.tsv"
done
