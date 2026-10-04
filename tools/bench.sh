#!/usr/bin/env bash
# Times the bench/ programs under the ironwork interpreter, the ironwork VM and GnuCOBOL (cobc -O2).
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

printf '%-10s %-40s %9s %9s %9s %7s %7s\n' program checksum interp vm cobc vm/int vm/cobc
for p in "${programs[@]}"; do
    src=$root/bench/$p.cbl
    "$cobc" -x -O2 -std=ibm-strict -o "$work/$p" "$src"
    export DD_BENCHF=$work/$p.dat
    iw_t=(); vm_t=(); cb_t=()
    for _ in $(seq "$runs"); do
        iw_t+=("$(timed "$work/iw.out" "$iw" run "$src" --dd "BENCHF=$DD_BENCHF:fixed")")
        vm_t+=("$(timed "$work/vm.out" "$iw" run --vm "$src" --dd "BENCHF=$DD_BENCHF:fixed")")
        cb_t+=("$(timed "$work/cb.out" "$work/$p")")
        cmp -s "$work/iw.out" "$work/cb.out" || { echo "$p: ironwork and cobc differ (see docs/benchmarks.md, Correctness)" >&2; cat "$work/iw.out" "$work/cb.out" >&2; }
        cmp -s "$work/vm.out" "$work/iw.out" || { echo "$p: VM CHECKSUM MISMATCH" >&2; cat "$work/vm.out" "$work/vm.out.err" >&2; }
    done
    mi=$(median "${iw_t[@]}"); mv=$(median "${vm_t[@]}"); mc=$(median "${cb_t[@]}")
    printf '%-10s %-40s %8ss %8ss %8ss %6.2fx %6.1fx\n' "$p" "$(head -1 "$work/iw.out")" "$mi" "$mv" "$mc" "$(echo "$mv / $mi" | bc -l)" "$(echo "$mv / $mc" | bc -l)"
done
