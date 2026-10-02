#!/bin/sh
# How much of exec's test suite the VM runs: runs exec's tests one at a time with IRONWORK_VM_REPORT
# set, so the test Harness reports each program it ran on both the interpreter and the VM, then
# prints how many lowered programs the VM runs to their end, how many it stops as not run yet and
# why, and how many differ from the interpreter, which must be none (docs/lir.md §12.3).
# usage: sh tools/vm-coverage.sh [TOP]    (TOP reasons shown, default 12)
# env: CARGO_TARGET_DIR is honoured as cargo honours it
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
top=${1:-12}
report=$(mktemp "${TMPDIR:-/tmp}/vm-coverage.XXXXXX")
trap 'rm -f "$report"' EXIT

status=0
(cd "$root" && IRONWORK_VM_REPORT="$report" cargo test --locked -q -p ironwork-exec -- --test-threads=1) >&2 || status=$?

# A program is its PROGRAM-ID in one source run with one set of flags, and fares as its worst run:
# differs, then panicked, then unimplemented, then ran. Fields: test, PROGRAM-ID, source
# fingerprint, ran | unimplemented | differs | panicked, and the reason or first difference.
awk -F '\t' -v top="$top" '
    function rank(o) { return o == "differs" ? 4 : o == "panicked" ? 3 : o == "unimplemented" ? 2 : 1 }
    {
        key = $2 "\t" $3
        runs++
        if (!(key in worst) || rank($4) > rank(worst[key])) {
            worst[key] = $4
            why[key] = $5
            test[key] = $1
            id[key] = $2
        }
    }
    END {
        for (k in worst) {
            programs++
            count[worst[k]]++
            if (worst[k] == "unimplemented") reasons[why[k]]++
            if (worst[k] == "differs" || worst[k] == "panicked") failed[++nfailed] = test[k] " " id[k] ": " why[k]
        }
        printf "vm: %d programs that lower, in %d runs\n", programs, runs
        printf "vm: %d run to their end on both executors and agree\n", count["ran"]
        printf "vm: %d stop at what the VM does not run yet\n", count["unimplemented"]
        printf "vm: %d differ from the interpreter, %d panic\n", count["differs"], count["panicked"]
        n = 0
        for (r in reasons) names[++n] = r
        for (i = 2; i <= n; i++) {
            r = names[i]
            for (j = i - 1; j >= 1 && (reasons[names[j]] < reasons[r] || (reasons[names[j]] == reasons[r] && names[j] > r)); j--) names[j + 1] = names[j]
            names[j + 1] = r
        }
        if (n > 0) print "vm: not run yet, by programs stopped:"
        for (i = 1; i <= n && i <= top; i++) printf "  %5d  %s\n", reasons[names[i]], names[i]
        if (n > top) printf "  (%d more)\n", n - top
        for (i = 1; i <= nfailed; i++) print "vm: FAILED " failed[i]
        if (nfailed > 0) exit 1
    }
' "$report" || status=1
exit "$status"
