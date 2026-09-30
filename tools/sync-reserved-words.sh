#!/bin/sh
# Copies cobolwork's table of IBM's reserved words, each with the column Enterprise COBOL's
# Reserved words appendix marks it in, into ironwork's runtime. The two must be byte for byte the same.
# usage: tools/sync-reserved-words.sh <cobolwork checkout>
set -eu
cp "$1/provenance/reserved-words.tsv" "$(dirname "$0")/../crates/rt/data/reserved-words.tsv"
