# Benchmarks

Step 0 of [codegen-runtime.md](codegen-runtime.md) §14: four programs timed under the interpreter
(the walker) and under GnuCOBOL, as the base for the B6 targets.

**Status:** measured 2026-09-30.

## Method

`tools/bench.sh` builds ironwork release, compiles each `bench/*.cbl` with
`cobc -x -O2 -std=ibm`, runs each five times in each implementation and prints the median wall time.
It also compares the two outputs and reports any difference. `-std=ibm` is the flag that matches the
interpreter; `-std=default` gives a different result on `packed` (see Correctness).

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

The checksums are identical in both implementations.

## Correctness

- **`-std=default` differs on `packed`** (`ACC=0003652155505.65`). GnuCOBOL's default dialect
  computes the ROUNDED intermediates differently from IBM; `-std=ibm` agrees with ironwork to the
  digit. The other three agree under both.
- **Level 78 is not IBM.** ironwork refuses it (`level 78 is not a data level`), so N is an ordinary
  `01` item.
- **A 17-digit DISPLAY target abends at 79a199e.** `ADD A TO T` with `T` `PIC 9(15)V99` ends in
  S0C6: a zoned item longer than PACK's 16-byte operand. `feat/core-fixes` (b58af60) packs long
  zoned items correctly. `seqio` keeps its total at 15 digits until that lands, then can return to 17.

## B6 against these numbers

**VM at most a fifth of the walker.** The targets are 1.4 s for `seqio`, 0.9 s for `packed`, 1.5 s
for `tblsrch` and 1.2 s for `callheavy`.

- `tblsrch` and `callheavy` are dispatch- and call-bound, which is what a VM removes. Reachable.
- `packed` spends its time in decimal arithmetic that the VM shares with the walker through the
  semantics library, so the gain is dispatch only. Doubtful at 5 times; not profiled.
- `seqio`: cobc itself takes 4.2 s, 1.8 s of it system time, so I/O sets the floor. The VM cannot
  reach 1.4 s unless the interpreter's own time is mostly not I/O. Not profiled; treat as unconfirmed.

**Native at most 1.5 times `cobc -O2`.** The limits are 6.3 s for `seqio`, 0.31 s for `packed`,
0.05 s for `tblsrch` and 0.66 s for `callheavy`.

- `seqio` is I/O-bound in both, so within reach.
- `packed` and `callheavy` are within reach if the emitted Rust calls the semantics library without
  per-operation allocation. Not shown by these numbers.
- `tblsrch` runs in 31 ms under cobc, close to the process start-up cost, so the ratio cannot be
  measured at this N. Raise N for cobc, or time the search loop alone, before judging it.

**Revision proposed:** keep both targets, but state the VM target per program class, and recheck
`seqio` and `tblsrch` once they are profiled.
