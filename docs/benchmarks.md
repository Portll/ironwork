# Benchmarks

Step 0 of [codegen-runtime.md](codegen-runtime.md) §14: four programs timed under the interpreter
(the walker), the VM and GnuCOBOL, as the base for the VM target and B6's native target.

**Status:** the interpreter and cobc measured 2026-09-30 as the base; the VM measured 2026-10-04
with the VM performance commits 6489a92..bbf9941, meeting the [VM target](#vm-target) on all four
programs on an M5 Pro (see VM results). On GitHub's hosted runners the VM missed the target for
`callheavy` and `packed` on Linux x86-64 and for `callheavy` on macOS until the arithmetic and CALL
work of 2026-10-07; at f60f7833 it meets every target on both runners (see
[Hosted runners](#hosted-runners)), which settles decision D-4 for 0.9.0.

## Method

`tools/bench.sh` builds ironwork release, compiles each `bench/*.cbl` with
`cobc -x -O2 -std=ibm-strict` and all four with `ironwork compile --native`, runs each program `RUNS`
times (default five) under the interpreter (`ironwork run --interpret`), the VM (`ironwork run --vm`),
as native code and under cobc, interleaved, and prints the median wall times, the VM's ratio to the
interpreter and to cobc, and native code's ratio to cobc. It reports any difference between the VM's
output and the interpreter's or native code's, and any difference between ironwork's and cobc's (see
Correctness). `NATIVE=0` leaves native code out. The cobc times
below were taken at `-std=ibm`, under which cobc generates the same C for these four programs.

| | |
|---|---|
| Machine | Apple M5 Pro, 51539607552 bytes (48 GiB) |
| ironwork | 79a199e, release build |
| rustc | 1.97.1 (8bab26f4f 2026-07-14) |
| cobc | GnuCOBOL 3.2.0 |

## Results

N is the constant at the top of each program. Times are medians of five runs, in seconds.

| Program | N | Checksum | ironwork | cobc -O2 | Ratio |
|---|---|---|---|---|---|
| `seqio`, write then read fixed records | 1,000,000 | `TOTAL=000500000523754 IDSUM=000500000500000` | 6.89 | 4.23 | 1.6 |
| `packed`, COMP-3 COMPUTE, ADD, MULTIPLY | 500,000 | `ACC= 0003634810833.92`, `D= 0000000777359.78` | 4.58 | 0.206 | 22 |
| `tblsrch`, SEARCH and SEARCH ALL | 20,000 passes over 500 entries | `HITS=000012496 VSUM=000034479236` | 7.25 | 0.031 | 234 |
| `callheavy`, CALL BY REFERENCE | 1,500,000 | `ACC=000007508702455` | 6.21 | 0.439 | 14 |

On 2026-09-30 the checksums were identical in both implementations; `packed` has differed from
cobc since b6c3b48 (see Correctness).

## VM results

One `tools/bench.sh` run, 2026-10-04:

| | |
|---|---|
| Machine | Apple M5 Pro, 18 cores, 51539607552 bytes (48 GiB) |
| ironwork | main 356f2d2 with the VM performance commits (on main as 6489a92..bbf9941), release build |
| rustc | 1.98.1 (48a229cea 2026-09-01) |
| cobc | GnuCOBOL 3.2.0 |
| Runs | `RUNS=5`, interleaved; times are medians, in seconds |
| Load | 1-minute load average 8.1 at the start and 19.1 at the end, from other sessions' ironwork tests and a veld benchmark |

| Program | Interpreter | VM | cobc -O2 | VM / interpreter | VM / cobc |
|---|---|---|---|---|---|
| `seqio` | 4.51 | 1.49 | 2.26 | 0.33 | 0.66 |
| `packed` | 2.83 | 0.831 | 0.156 | 0.29 | 5.3 |
| `tblsrch` | 4.93 | 0.921 | 0.013 | 0.19 | 71 |
| `callheavy` | 6.92 | 1.27 | 0.359 | 0.18 | 3.5 |

Against the [VM target](#vm-target):

| Program | VM | Target | |
|---|---|---|---|
| `tblsrch` | 0.187 of the interpreter | at most 0.20 | met |
| `callheavy` | 0.184 of the interpreter | at most 0.20 | met |
| `packed` | 0.294 of the interpreter | at most 0.33 | met |
| `seqio` | 0.66 times cobc | at most 1.25 | met |

- The VM's output equals the interpreter's on all four programs in every run.
- The interpreter shares the semantics library's decimal reads, stores and arithmetic with the VM,
  and runs faster than in the base table: `packed` 2.83 s against 4.58 s, `tblsrch` 4.93 s against
  7.25 s. Each ratio is against this run's interpreter.
- `tblsrch` and `callheavy` are within 0.02 of their target. `callheavy`'s VM median, 1.27 s, is
  above the 1.24 s reference in the target table, while its ratio is within the target.

## Hosted runners

The Bench workflow (`.github/workflows/bench.yml`) runs `tools/bench.sh` on a hosted Linux x86-64
runner and a hosted macOS arm64 runner, with GnuCOBOL 3.2 on both, and writes each table to the
run's summary and an artifact. It runs on demand, where its `ref` input times any commit with that
commit's own `bench.sh` and programs, and on pushes to `bench/` branches. Each ratio is judged on the
runner it was measured on, as the VM target asks. A hosted runner is small (3 or 4 cores) and its
times move between runs: `callheavy` on macOS gave 0.21 and 0.26 in two runs of one commit, so a
ratio within about 0.03 of its target needs a second run.

`RUNS=5`, 2026-10-07, VM time over interpreter time (`seqio`: VM time over cobc's):

| Runner | Commit | `tblsrch` | `callheavy` | `packed` | `seqio` |
|---|---|---|---|---|---|
| macOS arm64 (Apple M1, virtual, 3 cores) | 2337cd82 | 0.167 | 0.202 | 0.286 | 0.73 |
| macOS arm64 | e74a3fc7 | 0.189 | **0.255** | 0.313 | 0.77 |
| Linux x86-64 (AMD EPYC 9V45, 4 cores) | 2337cd82 | 0.198 | **0.254** | **0.372** | 0.89 |
| Linux x86-64 (AMD EPYC 7763, 4 cores) | e74a3fc7 | **0.207** | **0.285** | **0.378** | 1.17 |
| macOS arm64 (Apple M1, virtual, 3 cores) | 932aff97, on main as f60f7833 | 0.14 | 0.14 | 0.15 | 0.6 |
| Linux x86-64 (AMD EPYC 7763, 4 cores) | 932aff97, on main as f60f7833 | 0.16 | 0.17 | 0.14 | 0.8 |
| Target | | 0.20 | 0.20 | 0.33 | 1.25 |

- On Linux x86-64 the VM missed `callheavy` and `packed` already at 2337cd82, the commit that met
  every target on the M5 Pro: the VM gains less over the interpreter on that platform.
- From 2337cd82 to e74a3fc7 every ratio rose on both runners, by about 5 to 15 percent.
- At 932aff97 (Bench run 37606998129; rustc 1.99.0 and GnuCOBOL 3.2.0 on both runners), after the
  scaled fixed-point arithmetic, the zoned and packed codecs and the CALL path of 15.6.5 to 15.6.7,
  every target is met on both runners, each with room: `callheavy` 0.14 and 0.17 against 0.20,
  `packed` 0.15 and 0.14 against 0.33, `tblsrch` 0.14 and 0.16 against 0.20, and `seqio` 0.6 and
  0.8 times cobc against 1.25. The VM's times there: macOS 2.21, 0.89, 1.77 and 1.32 s; Linux 2.13,
  0.78, 1.61 and 1.83 s for `seqio`, `packed`, `tblsrch` and `callheavy`.
- A profile of the VM at e74a3fc7 (macOS `sample`, release build) puts most of its time in work the
  interpreter does not share: resolving operands and addresses on each execution (50% of
  `tblsrch`, 37 to 38% of `callheavy` and `packed`), instruction dispatch (14 to 19%), and setting
  up each CALL's storage (20% of `callheavy`). Zoned, packed and binary conversion (12 to 32%) is the
  semantics library both executors use, so speeding it lowers both times and moves the ratio little.

## Correctness

- **`packed` differs from cobc.** ironwork gives `ACC= 0003651477000.78`, `D= 0000000779026.52`;
  `cobc -std=ibm-strict` gives `ACC= 0003634810833.92`, `D= 0000000777359.78` (2026-10-04, as
  `-std=ibm` does), and `-std=default` `ACC= 0003652155505.65`, `D= 0000000779033.24`. Since
  b6c3b48 a ROUNDED receiver counts one more decimal place in dmax (assumption C101, chosen because
  CCVS85 NC117A and NC171A expect the digit rounding reads), and GnuCOBOL carries no such place.
  Which matches Enterprise COBOL waits for the goldens. The other three programs agree under both
  dialects. `--dialect gnucobol` gives cobc's checksums ([dialect.md](dialect.md), C101).
- **Level 78 is not IBM.** ironwork refuses it (`level 78 is not a data level`), so N is an ordinary
  `01` item.
- **A 17-digit DISPLAY target abends at 79a199e.** `ADD A TO T` with `T` `PIC 9(15)V99` ends in
  S0C6: a zoned item longer than PACK's 16-byte operand. `feat/core-fixes` (b58af60) packs long
  zoned items correctly. `seqio` keeps its total at 15 digits until that lands, then can return to 17.

## VM target

Fixed per program class before the VM's timing run on a quiet machine (operator, 2026-10-03,
decision D-4). Native code follows in 1.1 whether or not the VM meets it. Each target is a ratio measured in
one interleaved `tools/bench.sh` run on the runner being judged, Linux x86-64 or macOS. The seconds
are the reference on the machine above, from the 2026-09-30 interpreter and cobc times.

| Class | Programs | VM at most | Reference |
|---|---|---|---|
| Call and dispatch | `tblsrch`, `callheavy` | a fifth of the interpreter | 1.45 s, 1.24 s |
| Decimal arithmetic | `packed` | a third of the interpreter | 1.53 s |
| I/O | `seqio` | 1.25 times `cobc -O2` | 5.29 s |

- `tblsrch` and `callheavy` are dispatch- and call-bound, which is what a VM removes.
- `packed` spends its time in decimal arithmetic that the VM shares with the walker through the
  semantics library. The VM removes only the dispatch around it.
- `seqio`: cobc itself takes 4.2 s, 1.8 s of it system time. I/O sets the floor for every
  implementation, and the target is set against cobc's time rather than the interpreter's.

## B6 against these numbers

**Native at most 1.5 times `cobc -O2`.** The limits are 6.3 s for `seqio`, 0.31 s for `packed`,
0.05 s for `tblsrch` and 0.66 s for `callheavy`.

- `seqio` is I/O-bound in both, so within reach.
- `packed` and `callheavy` are within reach if the emitted Rust calls the semantics library without
  per-operation allocation. Not shown by these numbers.
- `tblsrch` runs in 31 ms under cobc, close to the process start-up cost, so the ratio cannot be
  measured at this N. Raise N for cobc, or time the search loop alone, before judging it.
