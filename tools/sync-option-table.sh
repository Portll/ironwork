#!/bin/sh
# Copies cobolwork's compiler-option table (generated from IBM's Programming Guide, Table 45) into
# ironwork, which reads option spellings from it. The two must be byte for byte the same.
# usage: tools/sync-option-table.sh <cobolwork checkout>
set -eu
cp "$1/provenance/enterprise-options.tsv" "$(dirname "$0")/../crates/numeric/data/enterprise-options.tsv"
