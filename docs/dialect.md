# Dialect: comparing ironwork with a GnuCOBOL build

`--dialect ibm|gnucobol` chooses whose result ironwork gives where it knowingly computes a different
one from GnuCOBOL's `cobc -std=ibm`. `ibm`, the default, is Enterprise COBOL's result as ironwork's
register of assumptions reads it. `gnucobol` is GnuCOBOL 3.2's, so a migration that runs a program
under ironwork and under a GnuCOBOL build sees only the differences that matter to it.

**Status:** built, 2026-10-03 (ironwork-roadmap 3.11). Two assumptions switch: C101 and C14.

## 1. What the dialect switches, and what it does not

The dialect switches an assumption: a place where IBM's manuals leave the result open, or where
ironwork wrote IBM's rule from memory (`Basis::Chosen` or `Basis::Recalled` in
`crates/numeric/src/assumptions.rs`), and where cobc's behaviour is clear and checked against the
installed cobc. Each switched assumption's claim says what `gnucobol` does instead.

It does not switch:

- **The platform.** Storage stays EBCDIC under both dialects; cobc's is ASCII. Collating order,
  hexadecimal literals, character codes and sign nibbles in storage differ by design.
- **What IBM documents and cobc does differently.** These are the differences a migration has to
  find, so they show in a comparison.
- **Bugs**, in either implementation. Each one found is listed below.

## 2. The option

| | |
|---|---|
| Flag | `--dialect ibm`, `--dialect gnucobol`, or `--dialect=ibm`, `--dialect=gnucobol`; anything else is a usage error (exit status 2) |
| Commands | `run`, `check`, `cics`, `job`, `compile`, `fuzz` and `compare`; `dump` refuses it |
| Options | `numeric::options::Options::dialect`, a `Dialect` (`Ibm`, `Gnucobol`), set by `Options::apply_flag("--dialect=...")`. A CALLed program, a class's methods and a job step compile with it too |
| Load module | the last field of the `OPTIONS` section's `Options`, tag `Ibm` 0, `Gnucobol` 1 ([load-module.md](load-module.md) §4.7, §5.1); `ironwork dump` prints it as `dialect` |
| Provenance | `--provenance` records the flag in `externalParameters.flags` and the dialect in `internalParameters.optionsInForce.dialect` ([evidence.md](evidence.md) §2) |
| Evidence | the journal's `open` record keeps `--dialect` and its value in `argv`, the one option value it records ([evidence.md](evidence.md) §1) |

The interpreter and the VM read the dialect from the same options and give the same result. The
lowering reads it where it fixes a result in the LIR: an arithmetic plan's `inner_dmax` (C101) and a
binary item's DISPLAY digits (C14).

## 3. Switched

### C101: a ROUNDED receiver's extra decimal place

Under `ibm` a ROUNDED receiver counts in dmax with one decimal place more than it holds, in every
operation of the statement (assumption C101, chosen so that CCVS85 NC117A and NC171A pass). cobc's
arithmetic-osvs, which `-std=ibm` sets, truncates each intermediate result to dmax and computes the
statement's last operation exactly. Under `gnucobol` the extra place counts in the last operation
alone, the one whose result the receivers take, and every operation below it carries dmax with each
receiver's own places.

`ArithPlan.inner_dmax` carries the second dmax; `numeric::precision::Dmax::receiver` decides both, and
`eval_fixed_at` in the walker (`crates/exec/src/machine.rs`) and the VM (`crates/rt/src/vm/value.rs`)
applies them.

| Statement | `ibm` | `gnucobol` and cobc |
|---|---|---|
| `COMPUTE D ROUNDED = D + E / 3`, D 1.00, E 12.35, both `9(5)V99` | 5.12 | 5.11 |
| `COMPUTE Y ROUNDED = A / B * C`, A 1 and B 3 `9(5)V99`, C 100 `9(3)` | 33.30 | 33.00 |
| `COMPUTE S ROUNDED = 1 + 1661.7 / DIV2`, DIV2 44.1, S `99V9` | 38.7 | 38.6 |
| `COMPUTE S ROUNDED = 1661.7 / DIV2`, and `DIVIDE DIV2 INTO DIV3 ROUNDED` | 37.7 | 37.7 |
| `bench/packed.cbl`, 500,000 turns | `ACC= 0003651477000.78`, `D= 0000000779026.52` | `ACC= 0003634810833.92`, `D= 0000000777359.78` |

NC117A and NC171A pass under both dialects: their quotients are each statement's last operation.

### C14: DISPLAY of a packed or binary item

Under `ibm` DISPLAY shows a packed or binary item's PICTURE digits, a negative value's sign
overpunched on the last digit, and a COMP-5 item, or any binary item under TRUNC(BIN), in 5, 10 or
19 (signed) or 20 digits (assumption C14; DISPSIGN(SEP) puts a separate sign first, C213). cobc
shows a signed item's sign, + or -, before its digits, a packed item's PICTURE digits, and any
binary item's whole value in 5, 10 or 20 digits by its size (`display_numeric` and
`cob_print_realbin` in libcob). Under `gnucobol` `rt::display::place` shows them as cobc does,
whatever DISPSIGN says.

| Item | `ibm` | `gnucobol` and cobc |
|---|---|---|
| `S9(5)V99 COMP-3` -123.45, and +123.45 | `001234N`, `0012345` | `-0012345`, `+0012345` |
| `S9(4) COMP` -12; `9(4) COMP` 12 | `001K`; `0012` | `-00012`; `00012` |
| `S9(18) COMP` -5 | `00000000000000000N` | `-00000000000000000005` |
| `S9(4) COMP-5` -3 | `0000L` | `-00003` |

A signed zoned item is not C14: IBM documents its overpunched sign (Programming Guide, DISPSIGN), and
cobc's separate trailing sign is listed below.

## 4. Checking against cobc

`crates/exec/src/tests/dialect.rs` runs each switched difference under both dialects on the
interpreter, in the differential run, and on the VM alone; the expected `gnucobol` output is what
GnuCOBOL 3.2 printed. `crates/cli/tests/dialect_flag.rs` checks the flag on each command and where it
is recorded.

`fixtures/dialect` holds a program for each switched assumption. `tools/differ.py --cobc` compiles
each with the installed `cobc -x -std=ibm`, runs it under ironwork with `--dialect gnucobol`, and
reports any difference; every one agrees, and so does `bench/packed.cbl`:

    tools/differ.py target/release/ironwork fixtures/dialect bench/packed.cbl --cobc

## 5. Found, not switched

Each difference below shows under both dialects. Found by running `bench/`, the CCVS85 routines
(`tools/nist.py`'s preparation) and ironwork's own test programs under ironwork and under
`cobc -x -std=ibm`, GnuCOBOL 3.2.0, on 2026-10-03.

| Difference | ironwork | cobc | Cause |
|---|---|---|---|
| DISPLAY of a signed zoned item | the last digit overpunched: -12 in `S9(3)` shows `01K` | a separate trailing sign: `012-` | IBM documents the overpunch under DISPSIGN(COMPAT) (Programming Guide, DISPSIGN); cobc's `display_numeric` (libcob/termio.c) moves the item to SIGN SEPARATE |
| A binary item larger than its PICTURE | TRUNC(STD), IBM's default, cuts it to the PICTURE: `MOVE 123456` to `9(4) COMP` keeps 3456 | keeps the halfword's value, 57920 | `-std=ibm` sets `binary-truncate: no` (ibm-strict.conf), which is TRUNC(BIN); give ironwork `CBL TRUNC(BIN)` to compare |
| Aligning an intermediate to more decimal places | `SUBTRACT A 2 B .04 FROM X` with A 10, B .3, X 12.34 gives 0.00 | 12.00 | cobc bug: `cob_decimal_align` (libcob/numeric.c:2286) divides where it should multiply. With the next two rows, arithmetic-osvs fails CCVS85 tests that pass under `-fno-arithmetic-osvs`: 39 of NC106A, 8 of NC175A, 6 of NC119A, 3 of NC118A and 3 of NC177A |
| An intermediate whose next operand is a literal | `COMPUTE X ROUNDED = A / B * 100`, A 1, B 3, X `9(5)V99`: A / B is cut to dmax, 33.00 (33.30 under `ibm`) | 33.33 | cobc bug: a literal right operand is not loaded through `decimal_expand`, so `decimal_align` never cuts the result before it (cobc/typeck.c); IBM's table cuts every intermediate |
| `* 1`, `/ 1`, `+ 0` and `- 0` | the operation is carried out, its operand cut to dmax: `COMPUTE S ROUNDED = (1661.7 / DIV2) * 1` gives 37.6 under `gnucobol` | removed at compile time, so the quotient is the last operation: 37.7 | cobc's constant folding (cobc/tree.c, `cb_build_binary_op`) |
| An intermediate quotient's decimal places | the dividend's or dmax, whichever is more: `COMPUTE X = (A * B / C) * K`, A B 1.11, C 0.7, K 1000 gives 1760.10 | the dividend's less the divisor's, or dmax: 1760.00 | ironwork bug: IBM's table (Programming Guide, Fixed-point data and intermediate results) gives (d2 − d1) or dmax; assumption C1 is Recalled, and cobc follows the table |
| More than 30 digits in an intermediate result | cut to 30 digits (31 under ARITH(EXTEND)) | every digit kept | IBM documents the cut (Programming Guide, Truncated intermediate results; C1) |
| FUNCTION MOD with a negative divisor | MOD(5, -3) is 2 | -1 | ironwork bug: `div_euclid` in `rt/src/intrinsic/function.rs` is not FUNCTION INTEGER's floor; CCVS85 IF124A fails 5 tests |
| Invalid decimal data, and a zero divisor outside ON SIZE ERROR | the program check: S0C7, S0CB or S0C9 | runs on, the receiver unchanged by the division; under `-debug` invalid data stops the run | IBM documents the checks |
