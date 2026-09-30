#!/bin/sh
# How much of exec's test suite lowers: runs exec's tests one at a time with IRONWORK_LOWER_REPORT
# set, so the test Harness and the bench test (bench/*.cbl) report each program they lower, then
# prints how many lower and what the rest lack (docs/lir.md §12.2).
# usage: sh tools/lower-coverage.sh [TOP]    (TOP reasons shown, default 12)
# env: CARGO_TARGET_DIR is honoured as cargo honours it
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
top=${1:-12}
report=$(mktemp "${TMPDIR:-/tmp}/lower-coverage.XXXXXX")
trap 'rm -f "$report"' EXIT

status=0
(cd "$root" && IRONWORK_LOWER_REPORT="$report" cargo test --locked -q -p ironwork-exec -- --test-threads=1) >&2 || status=$?

# A program is its PROGRAM-ID in one source run with one set of flags; the report has a line per run.
# Fields: test, PROGRAM-ID, source fingerprint, ok | unsupported | error, and the construct or fault.
awk -F '\t' -v top="$top" '
    !(($2 "\t" $3) in seen) {
        seen[$2 "\t" $3] = 1
        bench = ($1 ~ /^bench\//)
        programs[bench]++
        if ($4 == "ok") lowered[bench]++
        if ($4 == "unsupported") reasons[$5]++
        if ($4 == "error") errors[++nerrors] = $1 " " $2 ": " $5
    }
    END {
        printf "lowering: the tests'\'' programs: %d of %d lower\n", lowered[0], programs[0]
        printf "lowering: bench/*.cbl: %d of %d lower\n", lowered[1], programs[1]
        n = 0
        for (r in reasons) names[++n] = r
        for (i = 2; i <= n; i++) {
            r = names[i]
            for (j = i - 1; j >= 1 && (reasons[names[j]] < reasons[r] || (reasons[names[j]] == reasons[r] && names[j] > r)); j--) names[j + 1] = names[j]
            names[j + 1] = r
        }
        if (n > 0) print "lowering: unsupported constructs, by programs refused:"
        for (i = 1; i <= n && i <= top; i++) printf "  %5d  %s\n", reasons[names[i]], names[i]
        if (n > top) printf "  (%d more)\n", n - top
        for (i = 1; i <= nerrors; i++) print "lowering: FAILED " errors[i]
    }
' "$report"
exit "$status"
