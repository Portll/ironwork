#!/usr/bin/env bash
# Times the bench/ programs under the ironwork interpreter, the ironwork VM, ironwork native code and
# GnuCOBOL (cobc -O2).
# usage: tools/bench.sh [program...]    (default: all four; RUNS sets the repeat count, default 5)
# env: COBC (default cobc on PATH); NATIVE=0 leaves native code out; CARGO_TARGET_DIR is honoured as
# cargo honours it
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
runs=${RUNS:-5}
cobc=${COBC:-cobc}
native=${NATIVE:-1}
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

if [ "$native" = 1 ]; then
    sources=()
    for p in "${programs[@]}"; do sources+=("$root/bench/$p.cbl"); done
    "$iw" compile "${sources[@]}" -o "$work/native" --native --runtime "$root/crates" >&2
fi

printf '%-10s %-40s %9s %9s %9s %9s %7s %7s %8s\n' program checksum interp vm native cobc vm/int vm/cobc nat/cobc
for p in "${programs[@]}"; do
    src=$root/bench/$p.cbl
    "$cobc" -x -O2 -std=ibm-strict -o "$work/$p" "$src"
    export DD_BENCHF=$work/$p.dat
    iw_t=(); vm_t=(); nt_t=(); cb_t=()
    for _ in $(seq "$runs"); do
        iw_t+=("$(timed "$work/iw.out" "$iw" run --interpret "$src" --dd "BENCHF=$DD_BENCHF:fixed")")
        vm_t+=("$(timed "$work/vm.out" "$iw" run --vm "$src" --dd "BENCHF=$DD_BENCHF:fixed")")
        if [ "$native" = 1 ]; then
            nt_t+=("$(timed "$work/nt.out" "$work/native/$p" --dd "BENCHF=$DD_BENCHF:fixed")")
            cmp -s "$work/nt.out" "$work/vm.out" || { echo "$p: NATIVE CHECKSUM MISMATCH" >&2; cat "$work/nt.out" "$work/nt.out.err" >&2; }
        fi
        cb_t+=("$(timed "$work/cb.out" "$work/$p")")
        cmp -s "$work/iw.out" "$work/cb.out" || { echo "$p: ironwork and cobc differ (see docs/benchmarks.md, Correctness)" >&2; cat "$work/iw.out" "$work/cb.out" >&2; }
        cmp -s "$work/vm.out" "$work/iw.out" || { echo "$p: VM CHECKSUM MISMATCH" >&2; cat "$work/vm.out" "$work/vm.out.err" >&2; }
    done
    mi=$(median "${iw_t[@]}"); mv=$(median "${vm_t[@]}"); mc=$(median "${cb_t[@]}")
    if [ "$native" = 1 ]; then
        mn=$(median "${nt_t[@]}")
        printf '%-10s %-40s %8ss %8ss %8ss %8ss %6.2fx %6.1fx %7.2fx\n' "$p" "$(head -1 "$work/iw.out")" "$mi" "$mv" "$mn" "$mc" "$(echo "$mv / $mi" | bc -l)" "$(echo "$mv / $mc" | bc -l)" "$(echo "$mn / $mc" | bc -l)"
    else
        printf '%-10s %-40s %8ss %8ss %9s %8ss %6.2fx %6.1fx %8s\n' "$p" "$(head -1 "$work/iw.out")" "$mi" "$mv" - "$mc" "$(echo "$mv / $mi" | bc -l)" "$(echo "$mv / $mc" | bc -l)" -
    fi
done
