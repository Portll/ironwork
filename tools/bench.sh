#!/usr/bin/env bash
# Times the bench/ programs under the ironwork interpreter and under GnuCOBOL (cobc -O2).
# usage: tools/bench.sh [program...]    (default: all four; RUNS sets the repeat count, default 5)
# env: COBC (default cobc on PATH); CARGO_TARGET_DIR is honoured as cargo honours it
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
runs=${RUNS:-5}
cobc=${COBC:-cobc}
programs=("$@")
[ ${#programs[@]} -gt 0 ] || programs=(seqio packed tblsrch callheavy)

(cd "$root" && cargo build --release --locked -p ironwork) >&2
iw=${CARGO_TARGET_DIR:-$root/target}/release/ironwork
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# median SECONDS... : the middle of the sorted values
median() { printf '%s\n' "$@" | sort -n | sed -n "$(( ($# + 1) / 2 ))p"; }

# timed OUTFILE CMD... : runs CMD, sends its stdout to OUTFILE, prints wall seconds
timed() {
    local out=$1 t; shift
    TIMEFORMAT=%R
    t=$( { time "$@" >"$out" 2>"$out.err"; } 2>&1 ) || { cat "$out.err" >&2; return 1; }
    echo "$t"
}

printf '%-10s %-40s %9s %9s %7s\n' program checksum ironwork cobc ratio
for p in "${programs[@]}"; do
    src=$root/bench/$p.cbl
    "$cobc" -x -O2 -std=ibm -o "$work/$p" "$src"
    export DD_BENCHF=$work/$p.dat
    iw_t=(); cb_t=()
    for _ in $(seq "$runs"); do
        iw_t+=("$(timed "$work/iw.out" "$iw" run "$src" --dd "BENCHF=$DD_BENCHF:fixed")")
        cb_t+=("$(timed "$work/cb.out" "$work/$p")")
        cmp -s "$work/iw.out" "$work/cb.out" || { echo "$p: CHECKSUM MISMATCH" >&2; cat "$work/iw.out" "$work/cb.out" >&2; }
    done
    mi=$(median "${iw_t[@]}"); mc=$(median "${cb_t[@]}")
    printf '%-10s %-40s %8ss %8ss %6.1fx\n' "$p" "$(head -1 "$work/iw.out")" "$mi" "$mc" "$(echo "$mi / $mc" | bc -l)"
done
