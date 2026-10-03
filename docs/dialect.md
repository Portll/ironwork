# Dialect: comparing ironwork with a GnuCOBOL build

`--dialect ibm|gnucobol` chooses whose result ironwork gives where it knowingly computes a different
one from GnuCOBOL's `cobc -std=ibm`. `ibm`, the default, is Enterprise COBOL's result as ironwork's
register of assumptions reads it. `gnucobol` is GnuCOBOL 3.2's, so a migration that runs a program
under ironwork and under a GnuCOBOL build sees only the differences that matter to it.

**Status:** built, 2026-10-03 (ironwork-roadmap 3.11). Four assumptions switch: C101, C14, C95 and
C15.

## 1. What the dialect switches, and what it does not

The dialect switches a chosen assumption (`Basis::Chosen` in `crates/numeric/src/assumptions.rs`): a
place where IBM's manuals leave the result open and ironwork picked one, and where cobc's behaviour
is clear and checked against the installed cobc. Each switched assumption's claim says what
`gnucobol` does instead.

It does not switch:

- **A recalled assumption** (`Basis::Recalled`), which states a rule IBM documents, written from
  memory. Where cobc departs from one, the difference is listed below with the assumption, to be
  checked against the manual: if ironwork recalled the rule wrongly, that is a bug to fix under both
  dialects.
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
lowering reads it where it fixes a result in the LIR: an arithmetic plan's `inner_dmax` (C101), a
binary item's DISPLAY digits (C14) and a numeric literal's DISPLAY text (C95).

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

### C95: DISPLAY of a numeric literal

Under `ibm` DISPLAY writes a numeric literal as the program wrote it, its decimal point the
program's (assumption C95; the Language Reference is silent). cobc shows the literal as a numeric
item of its digits: the sign as written, then every digit, no decimal point. Under `gnucobol`
`rt::display::literal` drops the point, and the lowering bakes the same text into the plan.

| Literal | `ibm` | `gnucobol` and cobc |
|---|---|---|
| `1.5`, `-1.50`, `.5` | `1.5`, `-1.50`, `.5` | `15`, `-150`, `5` |
| `+0.25`, `0.0`, `007` | `+0.25`, `0.0`, `007` | `+025`, `00`, `007` |
| `1,5` under DECIMAL-POINT IS COMMA | `1,5` | `15` |

### C15: ACCEPT at the end of SYSIN

Under `ibm` an ACCEPT that finds SYSIN at its end, before any data, leaves its receiver unchanged
(assumption C15). cobc's `cob_accept` (libcob/termio.c) moves a single space at the end of its input:
a numeric or numeric-edited receiver becomes zero, any other is filled with spaces. Under `gnucobol`
`rt::accept` gives the receiver that value. Both write a line to standard error naming the receiver.

| Receiver, VALUE 7 or 'QQQQ' | `ibm` | `gnucobol` and cobc |
|---|---|---|
| `9(3)`; `S9(3) COMP-3`; `S9(4) COMP` | `007`; `007`; `0007` | `000`; `+000`; `+00000` |
| `X(4)`; `ZZ9` holding 5 | `QQQQ`; `  5` | four spaces; `  0` |

The rest of C261 stays: IBM fills a receiver longer than a record from the records after it
(Language Reference, ACCEPT, "each input record is concatenated with the previous input record")
and checks nothing, where cobc moves one line as a MOVE would; that difference is listed below.

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

Each difference below shows under both dialects. Found on 2026-10-03 by running, under ironwork and
under `cobc -x -std=ibm` (GnuCOBOL 3.2.0): `bench/`; the 403 CCVS85 routines both run, prepared as
`tools/nist.py` prepares them; the 161 of ironwork's own test programs both compile (those without
option cards, files, EXEC, object-oriented COBOL, the clock or random numbers); and 300 programs
drawn from the corpus that need no files, of which 141 ran under both. Where a cobc option removes a
difference, it is named.

### 5.1 The platform

| Difference | Seen in |
|---|---|
| Collating order: EBCDIC under ironwork, ASCII under cobc, in comparisons, SORT and MERGE keys, and table SORT | CCVS85 ST137A, ST144A, ST147A, whose X-cards 063 and 064 give EBCDIC order (`-fdefault-colseq=EBCDIC` passes them); test programs; corpus |
| Character codes: hexadecimal literals, ORD and CHAR, HIGH-VALUE and LOW-VALUE shown, an overpunched sign seen through an alphanumeric item, the card images ACCEPT transfers | test programs; corpus |
| JSON and XML GENERATE write UTF-8 into an alphanumeric receiver; DISPLAY then reads the bytes in the program's EBCDIC code page | corpus |
| Floating point: IBM hexadecimal under ironwork, IEEE under cobc, in COMP-1 and COMP-2 values and the floating-point functions | test programs |

### 5.2 IBM documents it, and cobc -std=ibm does otherwise

| Difference | ironwork, as IBM documents | cobc | Seen in |
|---|---|---|---|
| DISPLAY of a signed zoned item | the last digit overpunched: -12 in `S9(3)` shows `01K` (Programming Guide, DISPSIGN) | a separate trailing sign, `012-` (`display_numeric`, libcob/termio.c); `-fpretty-display` gives `-012`, which is ironwork under `CBL DISPSIGN(SEP)` | corpus, 7 of 26 differing programs |
| DISPLAY of a zoned item holding other characters than digits | the bytes as stored | each one rewritten as 0 (termio.c) | corpus |
| A binary item larger than its PICTURE | cut to the PICTURE under TRUNC(STD), IBM's default: `MOVE 123456` to `9(4) COMP` keeps 3456 | keeps the halfword's value, 57920: `-std=ibm` sets `binary-truncate: no`, which is TRUNC(BIN); `-fbinary-truncate` cuts. Give ironwork `CBL TRUNC(BIN)` to compare | CCVS85 NC105A; corpus |
| ACCEPT into an item longer than a line | the next records fill it, each concatenated with the one before (Language Reference, ACCEPT) | one line, the rest of the item spaces (termio.c `cob_accept`) | CCVS85 NC204M |
| ACCEPT into a numeric item | the characters as they come, with no editing or checking (Language Reference, ACCEPT) | the line moved as a MOVE of an alphanumeric item does: `12` gives `012`, `-3.5` gives -3.5 | probe |
| MOVE of an alphanumeric item to a numeric one | the sender read as an unsigned integer; a character other than a digit gives the low half of its byte (C240, recalled) and can be a data exception when the item is next read as a number | a sign, decimal point and spaces read as such, any other character giving zero (`cob_move_alphanum_to_display`, libcob/move.c) | corpus, 1 output and 4 S0C7 abends under ironwork |
| A receiving group holding its own OCCURS DEPENDING ON object | received at its maximum length (Language Reference, OCCURS DEPENDING ON) | at the object's current value: `-std=ibm` sets `odoslide: yes`; `-fno-odoslide` passes, but stops sliding the items after the table | CCVS85 NC247A, SQ214A, ST146A |
| CANCEL of a program CALLed by a literal | no action under NODYNAM, IBM's default (Language Reference, CANCEL) | the program is reset; `-fstatic-call` does not change it. Give ironwork `CBL DYNAM` to compare | CCVS85 IC203A; corpus |
| ALTER in an independent segment | the altered GO TOs are put back each time the segment is entered from one of another priority (Language Reference, Procedures) | never put back: `-std=ibm` sets `section-segments: ignore`; `-fsection-segments=ok` passes | CCVS85 SG102A, SG103A, SG201A, SG203A |
| The rest of a JSON GENERATE receiver | kept as it was (Language Reference, JSON GENERATE) | filled with spaces (libcob/mlio.c) | corpus |
| INSPECT of a national item | counts national characters | counts bytes (libcob/strings.c) | corpus |
| An intermediate result of more than 30 digits | cut to 30 (31 under ARITH(EXTEND)) (Programming Guide, Truncated intermediate results; C1) | every digit kept | probe |
| MEAN and MEDIAN | floating-point functions, their result rounded into the receiver (C111, recalled; C6) | exact decimal, truncated | corpus |
| Invalid decimal data, and a zero divisor outside ON SIZE ERROR | the program check: S0C7, S0CB or S0C9 | runs on, the receiver unchanged by the division; under `-debug` invalid data stops the run | test programs; corpus |

### 5.3 cobc bugs

| Difference | ironwork | cobc | Cause |
|---|---|---|---|
| Aligning an intermediate to more decimal places | `SUBTRACT A 2 B .04 FROM X` with A 10, B .3, X 12.34 gives 0.00; `X * (1 + OT)` with OT `V99` is not zero | 12.00; zero | `cob_decimal_align` (libcob/numeric.c:2286) divides where it should multiply. With the next two rows, arithmetic-osvs fails CCVS85 tests that pass under `-fno-arithmetic-osvs`: 39 of NC106A, 8 of NC175A, 6 of NC119A, 3 of NC118A, 3 of NC177A and NC250A's IF--TEST-37 |
| An intermediate whose next operand is a literal | `COMPUTE X ROUNDED = A / B * 100`, A 1, B 3, X `9(5)V99`: A / B is cut to dmax, 33.00 (33.30 under `ibm`) | 33.33 | a literal right operand is not loaded through `decimal_expand`, so `decimal_align` never cuts the result before it (cobc/typeck.c); IBM's table cuts every intermediate |
| `* 1`, `/ 1`, `+ 0` and `- 0` | the operation is carried out, its operand cut to dmax: `COMPUTE S ROUNDED = (1661.7 / DIV2) * 1` gives 37.6 under `gnucobol` | removed at compile time, so the quotient is the last operation: 37.7 | constant folding (cobc/tree.c, `cb_build_binary_op`) |

### 5.4 ironwork bugs

Each is a separate fix; under `ibm` it changes results, so none is made here.

| Difference | ironwork | IBM, and cobc | Seen in |
|---|---|---|---|
| An intermediate quotient's decimal places | the dividend's or dmax, whichever is more: `COMPUTE X = (A * B / C) * K`, A and B 1.11, C 0.7, K 1000, gives 1760.10 | the dividend's less the divisor's, or dmax (Programming Guide, Fixed-point data and intermediate results; C1 recalled it otherwise): 1760.00 | probe |
| FUNCTION MOD with a negative divisor | MOD(5, -3) is 2: `div_euclid` in `rt/src/intrinsic/function.rs` | -1, FUNCTION INTEGER's floor | CCVS85 IF124A, 5 tests |
| A LINKAGE level-01 item that REDEFINES another | no address: S0C4 | the address of the item it redefines | CCVS85 IC237A |
| The SIGN clause of a subordinate group | the level-01 item's clause wins | the subordinate entry's (Language Reference, SIGN) | CCVS85 NC116A |
| INSPECT of a signed zoned item | read with its sign | as if moved to an unsigned item (Language Reference, INSPECT, Table 1) | CCVS85 NC216A |
| SEARCH ... VARYING the table's own index | searches with the first index and steps the named one beside it | uses the named index for the search (Language Reference, SEARCH) | CCVS85 NC235A |
| An ALL literal longer than the item compared with it | not cut to the item's length | cut (Language Reference, figurative constants) | CCVS85 NC250A |
| DISPLAY of a national item to SYSOUT | converted from UTF-16 | written as its UTF-16 bytes; only UPON CONSOLE converts (Programming Guide, Displaying values) | corpus |
| `PIC 99 VALUE "7"` | accepted, 7 stored | refused by IBM; cobc warns and stores 70 | corpus |
| An unknown environment-name in DISPLAY UPON (SYSERR, or any word) | accepted, written to standard output | refused by IBM; cobc writes SYSERR to standard error | corpus |
| Floating-point exponentiation, `FUNCTION SQRT(10) ** 2` | not run yet: abend IRONWORK | runs it | CCVS85 IF136A |

### 5.5 Chosen, and not switched

| Assumption | ironwork | cobc | Why not switched |
|---|---|---|---|
| C16, a numeric literal passed BY CONTENT | zoned decimal of its digits | an 8-byte buffer holding the value as a binary integer in the host's byte order (cobc/codegen.c) | the host's byte order has no counterpart in z/OS storage, where every binary item is big-endian |
| C54, FUNCTION RANDOM | the Park-Miller generator | GMP's Mersenne Twister, seeded from the clock and the module's address when no seed is given (libcob/intrinsic.c) | without a seed cobc's sequence does not repeat; with one, matching it means GMP's seeding |
| No assumption: DISPLAY of a function's value | the digits of the value's places: LENGTH 9, ORD 3, MOD and INTEGER 31, INTEGER-OF-DATE 7 | by each function's result field: LENGTH of a fixed item as a literal (`10`), an integer result in 10 digits with a sign when signed, ABS as its argument's PICTURE | unrecorded; it needs an assumption first, and a rule per function |
| No assumption: the rest of an XML GENERATE receiver | kept as it was | filled with spaces (libcob/mlio.c) | unrecorded; C119 does not say it |

### 5.6 Programs one compiler refuses

Not results, so not the dialect's: what ironwork refuses that cobc accepts is the compliance option's
ground (ironwork-roadmap 3.12). CCVS85: both refuse the communication routines (CM), IX110A and
NC211A; cobc alone refuses an ALL subscript in an intrinsic function's argument (11 routines from
IF119A to IF141A) and OBNC1M; ironwork alone refuses the debugging routines DB201A to DB205A, NC108M,
NC174A, NC254A, SM201A, SM202A and SM206A. Corpus: of 300, ironwork refused 30 cobc ran (18 for syntax IBM does not have, 3 for IBM's
limits, 5 for layout, 4 others, and `FUNCTION ALL INTRINSIC` in REPOSITORY, which IBM 6.4 accepts and
ironwork does not), and cobc refused 3 ironwork ran.

A RETURN-CODE outside 0 to 255 ends `ironwork run` with exit status 255, and a cobc program with the
value modulo 256.
