# Dialect: comparing ironwork with a GnuCOBOL build

`--dialect ibm|gnucobol` chooses whose result ironwork gives where it knowingly computes a different
one from GnuCOBOL's `cobc -std=ibm`. `ibm`, the default, is Enterprise COBOL's result as ironwork's
register of assumptions reads it. `gnucobol` is GnuCOBOL 3.2's, so a migration that runs a program
under ironwork and under a GnuCOBOL build sees only the differences that matter to it.

**Status:** built, 2026-10-03 (ironwork-roadmap 3.11). Seven assumptions switch: C101, C14, C95,
C15, C51, C180 and C262.

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
| Flag | `--dialect ibm`, `--dialect gnucobol`, or `--dialect=ibm`, `--dialect=gnucobol`; anything else is a usage error (exit status 246 for `run`, `cics` and `job`, 2 for the others) |
| Commands | `run`, `check`, `cics`, `job`, `compile`, `fuzz` and `compare`; `dump` refuses it |
| Options | `numeric::options::Options::dialect`, a `Dialect` (`Ibm`, `Gnucobol`), set by `Options::apply_flag("--dialect=...")`. A CALLed program, a class's methods and a job step compile with it too |
| Load module | the last field of the `OPTIONS` section's `Options`, tag `Ibm` 0, `Gnucobol` 1 ([load-module.md](load-module.md) §4.7, §5.1); `ironwork dump` prints it as `dialect` |
| Provenance | `--provenance` records the flag in `externalParameters.flags` and the dialect in `internalParameters.optionsInForce.dialect` ([evidence.md](evidence.md) §2) |
| Evidence | the journal's `open` record keeps `--dialect` and its value in `argv`, as it keeps `--compliance`'s and `--statement-limit`'s ([evidence.md](evidence.md) §1) |

The interpreter and the VM read the dialect from the same options and give the same result. The
lowering reads it where it fixes a result in the LIR: an arithmetic plan's `inner_dmax` (C101), a
binary item's DISPLAY digits (C14), a numeric literal's DISPLAY text (C95) and which zoned
comparisons read bytes (C262).

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

### C51: a dynamic CALL of an ENTRY name

Under `ibm` a dynamic CALL of an entry name gets a copy of the program with WORKING-STORAGE of its
own, which a CANCEL of that name resets (assumption C51). cobc's entry points share the program's
one WORKING-STORAGE; CANCEL of an entry name does nothing, and CANCEL of the PROGRAM-ID resets it.
Under `gnucobol` `rt::callee::entry_copy` makes no copy, so `RunUnit::load_entry` enters the one
program for every entry name.

| CALL `SUBPROG`, then CALL PGM (`PAYMASTR`, an ENTRY of SUBPROG) twice, CANCEL PGM, CALL PGM, CALL `SUBPROG`, CANCEL `SUBPROG`, CALL PGM, each adding 1 to one counter | `ibm` | `gnucobol` and cobc |
|---|---|---|
| The counter each CALL shows | 1, 1, 2, 1, 2, 2 | 1, 2, 3, 4, 5, 1 |

### C180: an EXTERNAL record of another size

Under `ibm` a program describing an EXTERNAL record with another size than the run unit's ends the
run (assumption C180). cobc's `cob_external_addr` (libcob/common.c) lets a shorter description share
the storage with a warning, and ends the run on a longer one. Under `gnucobol` `RunUnit::external`
does the same for records; an EXTERNAL file's record area must still match.

### C262: zoned comparisons under NOINVDATA

Under `ibm`, with NOINVDATA, IBM's default, an unsigned zoned integer compared with zero or with
another of its own length is compared by its bytes at OPTIMIZE(1) and OPTIMIZE(2) and as numbers at
OPTIMIZE(0) (assumption C262). cobc compares the two items with `memcmp` at every level and compares
with zero by value. Under `gnucobol` `Options::zones_compared_between_items` and
`Options::zones_compared_with_zero` decide them so; the walker, the lowering and the VM read them.
They differ only where the bytes are not all digits.

| `9(4)` holding X'F0F040F0', compared with ZERO, and with a `9(4)` holding zero | `ibm`, OPT(0) | `ibm`, OPT(2) | `gnucobol` and cobc |
|---|---|---|---|
| With ZERO; with the item | equal; equal | not equal; not equal | equal; not equal |

INVDATA, which cobc lacks, keeps C223 under both dialects.

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
option cards, files, EXEC, object-oriented COBOL, the clock or random numbers), of which 89 differed;
and 300 programs drawn from the corpus that need no files, of which 141 ran under both and 26
differed. Every difference was reduced to a small program run under both. Where a cobc option
removes one, it is named.

### 5.1 The platform

| Difference | Seen in |
|---|---|
| Collating order: EBCDIC under ironwork, ASCII under cobc, in comparisons, SORT and MERGE keys, and table SORT | CCVS85 ST137A, ST144A, ST147A, whose X-cards 063 and 064 give EBCDIC order (`-fdefault-colseq=EBCDIC` passes them); test programs; corpus |
| Character codes: hexadecimal literals, ORD and CHAR, HIGH-VALUE and LOW-VALUE shown, an overpunched sign seen through an alphanumeric item, the card images ACCEPT transfers, a character CCSID 1140 has no byte for | test programs; corpus |
| JSON and XML GENERATE write UTF-8 into an alphanumeric receiver; DISPLAY then reads the bytes in the program's EBCDIC code page | corpus |
| Floating point: IBM hexadecimal under ironwork, IEEE under cobc, in COMP-1 and COMP-2 values, the floating-point functions, and an exponent beyond HFP's range (S0CC) | test programs |
| The same failure in another form: S0C4 for a LINKAGE item with no address where cobc takes SIGSEGV, S806 where cobc says `module not found` | test programs |

### 5.2 IBM documents it, and cobc -std=ibm does otherwise

| Difference | ironwork, as IBM documents | cobc | Seen in |
|---|---|---|---|
| DISPLAY of a signed zoned item | the last digit overpunched: -12 in `S9(3)` shows `01K` (Programming Guide, DISPSIGN) | a separate trailing sign, `012-` (`display_numeric`, libcob/termio.c); `-fpretty-display` gives `-012`, which is ironwork under `CBL DISPSIGN(SEP)` | corpus, 7 of 26 differing programs; test programs |
| DISPLAY of a zoned item holding other characters than digits | the bytes as stored | each one rewritten as 0 (termio.c) | corpus |
| The RETURN-CODE special register | `S9(4) BINARY`: DISPLAY shows `+00007` under `gnucobol` | a fullword, `+000000007` | test programs |
| A binary item larger than its PICTURE | cut to the PICTURE under TRUNC(STD), IBM's default: `MOVE 123456` to `9(4) COMP` keeps 3456 | keeps the halfword's value, 57920: `-std=ibm` sets `binary-truncate: no`, which is TRUNC(BIN); `-fbinary-truncate` cuts. Give ironwork `CBL TRUNC(BIN)` to compare | CCVS85 NC105A; corpus; test programs |
| ACCEPT into an item longer than a line | the next records fill it, each concatenated with the one before (Language Reference, ACCEPT) | one line, the rest of the item spaces (termio.c `cob_accept`) | CCVS85 NC204M |
| ACCEPT into a numeric item | the characters as they come, with no editing or checking (Language Reference, ACCEPT) | the line moved as a MOVE of an alphanumeric item does: `12` gives `012`, `-3.5` gives -3.5 | test programs |
| MOVE of an alphanumeric item to a numeric one | the sender read as an unsigned integer; a character other than a digit gives the low half of its byte (C240, recalled) and can be a data exception when the item is next read as a number | a sign, decimal point and spaces read as such, any other character giving zero (`cob_move_alphanum_to_display`, libcob/move.c) | corpus, 1 output and 4 S0C7 abends under ironwork; test programs |
| MOVE of a zoned item to a zoned one | each digit the low half of the sender's byte, the zone F (C260, recalled) | the bytes copied (move.c) | test programs |
| A receiving group holding its own OCCURS DEPENDING ON object | received at its maximum length (Language Reference, OCCURS DEPENDING ON), by MOVE, READ INTO, RETURN INTO, STRING, UNSTRING and ACCEPT | at the object's current value: `-std=ibm` sets `odoslide: yes`; `-fno-odoslide` passes MOVE, but stops sliding the items after the table | CCVS85 NC247A, SQ214A, ST146A; test programs |
| `ADD X TO X Y` | the sum of the operands before TO kept in a temporary for every receiver (Language Reference, ADD) | X read again for Y (cobc/typeck.c) | test programs |
| A signed zoned item holding spaces, compared with SPACES under ZWB | its sign removed first, so not equal (Programming Guide, ZWB) | the space left in the sign position, so equal (libcob/common.c) | test programs |
| ORD, CHAR and a contained program under a PROGRAM COLLATING SEQUENCE | ordinals in the program's sequence; a contained program takes its container's | ORD and CHAR native; a contained program has its own (libcob/intrinsic.c, cobc/codegen.c) | test programs |
| CANCEL of a program CALLed by a literal | no action under NODYNAM, IBM's default (Language Reference, CANCEL) | the program is reset; `-fstatic-call` does not change it. Give ironwork `CBL DYNAM` to compare | CCVS85 IC203A; corpus |
| ALTER in an independent segment | the altered GO TOs are put back each time the segment is entered from one of another priority (Language Reference, Procedures) | never put back: `-std=ibm` sets `section-segments: ignore`; `-fsection-segments=ok` passes | CCVS85 SG102A, SG103A, SG201A, SG203A |
| The rest of a JSON GENERATE receiver | kept as it was (Language Reference, JSON GENERATE) | filled with spaces (libcob/mlio.c) | corpus |
| INSPECT of a national item | counts national characters | counts bytes (libcob/strings.c) | corpus |
| An intermediate result of more than 30 digits | cut to 30 (31 under ARITH(EXTEND)) (Programming Guide, Truncated intermediate results; C1) | every digit kept | probe |
| MEAN, MEDIAN, NUMVAL and COMBINED-DATETIME | floating-point results, rounded into the receiver (C111, recalled; Programming Guide on NUMVAL; C6) | exact decimal, truncated | corpus; test programs |
| MAX, MIN, RANGE, REM and SUM of fixed-point arguments | as many decimal places as the arguments have at most, counted in the expression's dmax (Programming Guide SC27-8714-03, pp. 794, 799; C390): `COMPUTE R = FUNCTION MAX(A B) / 3 * 3`, A `9V99` 1.00, B `9` 5, R `99V9`, gives 4.9 | MAX and MIN return the winning argument's own field (`cob_intr_max`, libcob/intrinsic.c), and intermediates around a function value carry other places: `10 / 3 * FUNCTION SUM(D B)`, D `9V99` 1.01, gives 19.8 where IBM's dmax gives 20.0 | probe |
| INTEGER, INTEGER-PART and MOD of fixed-point arguments | INTEGER one digit more than its argument, INTEGER-PART as many, MOD as many as its shorter argument (Language Reference SC27-8713-03, p. 601; Programming Guide SC27-8714-03, pp. 798-799; C392): `FUNCTION MOD(N H)`, N `S9` -3, H `999` 100, is 7 | a field as large as the value: 97 | probe |
| MOVE of an integer or numeric function to an alphanumeric item, which Enterprise COBOL refuses (Programming Guide SC27-8714-03, p. 119) | refused when compiled under `--compliance strict` (C394); under `extended`, with IWX0008-W, the value at the function's precision, moved as a numeric item of that precision is: an integer's digits, `00005` for MAX(N M) with N `999` 5 and M `9(5)` 4, and a value with decimal places refused at run time (Language Reference SC27-8713-03, p. 404) | the digits of the field cobc's function returns: the winning argument's own for MAX and MIN (`005`, and `825` for 8.25), nine for most others (`000000008` for INTEGER(8.25)) | probe |
| Invalid decimal data, and a zero divisor outside ON SIZE ERROR | the program check: S0C7, S0CB or S0C9 | runs on, the receiver unchanged by the division; under `-debug` invalid data stops the run | test programs; corpus |

### 5.3 cobc bugs

| Difference | ironwork | cobc | Cause |
|---|---|---|---|
| Aligning an intermediate to more decimal places | `SUBTRACT A 2 B .04 FROM X` with A 10, B .3, X 12.34 gives 0.00; `X * (1 + OT)` with OT `V99` is not zero | 12.00; zero | `cob_decimal_align` (libcob/numeric.c:2286) divides where it should multiply. With the next two rows, arithmetic-osvs fails CCVS85 tests that pass under `-fno-arithmetic-osvs`: 39 of NC106A, 8 of NC175A, 6 of NC119A, 3 of NC118A, 3 of NC177A and NC250A's IF--TEST-37 |
| An intermediate whose next operand is a literal | `COMPUTE X ROUNDED = A / B * 100`, A 1, B 3, X `9(5)V99`: A / B is cut to dmax, 33.00 (33.30 under `ibm`) | 33.33 | a literal right operand is not loaded through `decimal_expand`, so `decimal_align` never cuts the result before it (cobc/typeck.c); IBM's table cuts every intermediate |
| `* 1`, `/ 1`, `+ 0` and `- 0` | the operation is carried out, its operand cut to dmax: `COMPUTE S ROUNDED = (1661.7 / DIV2) * 1` gives 37.6 under `gnucobol` | removed at compile time, so the quotient is the last operation: 37.7 | constant folding (cobc/tree.c, `cb_build_binary_op`) |
| `'0' = '9'` under an alphabet with ALSO | compared in the program's sequence | folded at compile time; `-fno-constant-folding` fixes it | cobc/tree.c |
| SEARCH ... VARYING an item | the item stepped with the index | set to the index's value (cobc/codegen.c) | |
| `A = B AND (C OR < D) OR 2` | A = C and A = 2 (C150) | A < C and A < 2 | a global operator in `build_expr_shift` (cobc/typeck.c) |
| TEST-NUMVAL-C with a currency string of more than one character | 0 for `('CHF 12', 'CHF')` | 2 | libcob/intrinsic.c |
| ANNUITY at a very small rate | 0.1 for ANNUITY(e^-100, 10) | 0.000000372 | decimal power precision (libcob/intrinsic.c) |
| BLANK WHEN ZERO on `9(3)V9`, INITIALIZE ... TO VALUE, a national QUOTE, RETURNING, CURRENCY ... WITH PICTURE SYMBOL | as IBM documents | a byte too many for the V; every category initialized and REPLACING ignored; four raw bytes; a crash; `Y` shown: unfinished or wrong in cobc 3.2 (`-Wunfinished`, `-Wpending`) | |

### 5.4 ironwork bugs

The survey's ironwork bugs each changed results under `ibm`. All are fixed, and listed in 5.7.

### 5.5 Chosen, and not switched

| Assumption | ironwork | cobc | Why not switched |
|---|---|---|---|
| C16, a numeric literal passed BY CONTENT | zoned decimal of its digits | an 8-byte buffer holding the value as a binary integer in the host's byte order (cobc/codegen.c) | the host's byte order has no counterpart in z/OS storage, where every binary item is big-endian |
| C54, FUNCTION RANDOM | the Park-Miller generator | GMP's Mersenne Twister, seeded from the clock and the module's address when no seed is given (libcob/intrinsic.c) | without a seed cobc's sequence does not repeat; with one, matching it means GMP's seeding |
| C99, control passing the end of a paragraph armed to return to a PERFORM that repeats, left by GO TO | refused at run time, abend IRONWORK | returns, and the PERFORM goes on with its iterations | ironwork has no model of that return in either executor |
| C112, an argument outside a function's domain | abend IRONWORK | EC-ARGUMENT-FUNCTION set and never raised, the result 0; FACTORIAL exact past 28 | Language Environment's math services signal a condition there, so zero would hide what z/OS does |
| C181, a contained program CALLed from outside its container | found in ironwork's flat library, and the run ends when it uses a GLOBAL name | the CALL finds no program, as IBM's scope rules say | IBM documents the scope; a scope check belongs under both dialects |
| C333, a BY VALUE argument to a parameter received BY REFERENCE | the parameter gets storage of its own holding the value, and the called program runs | the value is read as an address, and the program faults | the Language Reference requires BY VALUE on both sides (p. 322) and gives no result; what the value addresses has no counterpart in ironwork's storage |
| C391, the integer places of MAX and MIN | as many as the widest argument's: MAX(N M), N `999` 5, M `9(5)` 4, moved to an alphanumeric item gives `00005` | the winning argument's own field: `005` | that field keeps the winning argument's decimal places too, which IBM documents (C390), so switching the integer places alone gives cobc's digits only where the arguments' decimal places agree |
| No assumption: DISPLAY of LENGTH OF | its value's 9 digits | a fixed size's LENGTH OF folded to a literal (`4`), a variable one `+0000000004` | unrecorded; it needs an assumption first |
| No assumption: the rest of an XML GENERATE receiver | kept as it was | filled with spaces (libcob/mlio.c) | unrecorded; C119 does not say it |

### 5.6 Programs one compiler refuses

Not results, so not the dialect's: what ironwork refuses that cobc accepts is `--compliance`'s
ground ([compliance.md](compliance.md)). CCVS85: both refuse the communication routines (CM), IX110A and
NC211A; cobc alone refuses an ALL subscript in an intrinsic function's argument (11 routines from
IF119A to IF141A) and OBNC1M; ironwork alone refuses the debugging routines DB201A to DB205A, NC108M,
NC174A, NC254A, SM201A, SM202A and SM206A. Corpus: of 300, ironwork refused 30 cobc ran (18 for
syntax IBM does not have, 3 for IBM's limits, 5 for layout, 4 others, and `FUNCTION ALL INTRINSIC`
in REPOSITORY, which IBM 6.4 accepts and ironwork does not), and cobc refused 3 ironwork ran.
Test programs: cobc refused 30 that ironwork runs.

A RETURN-CODE of 239 or outside 0 to 238 ends `ironwork run` with exit status 239, its value named
on standard error, and a cobc program with the value modulo 256.

### 5.7 Fixed since the survey

The survey found these as ironwork bugs. ironwork gives the result IBM's manuals state, under both
dialects and on both executors (ironwork-roadmap 3.14).

| Difference | ironwork and IBM | cobc | Seen in |
|---|---|---|---|
| An intermediate quotient's decimal places | the dividend's less the divisor's, or dmax, whichever is more (Programming Guide SC27-8714-03, p. 795; C1): `COMPUTE X = (A * B / C) * K`, A and B 1.11, C 0.7, K 1000, gives 1760.00 | the same | probe |
| FUNCTION MOD with a negative divisor | argument-1 less argument-2 times FUNCTION INTEGER of their quotient (Language Reference SC27-8713-03, p. 601): MOD(11, -5) is -4 | the same | CCVS85 IF124A, which now passes |
| A LINKAGE level-01 item that REDEFINES another | at the address of the item it redefines, whose storage it describes again (Language Reference SC27-8713-03, p. 225) | the same | CCVS85 IC237A, which now passes |
| The SIGN clause of a group and of an entry below it | a group's clause applies to its signed zoned items, and a subordinate entry's own clause takes precedence for that entry (p. 231) | the same | CCVS85 NC116A, which now passes |
| INSPECT of a signed zoned item | examined as if moved to an unsigned item of its length, a separate sign not examined (p. 359, Table 40); REPLACING and CONVERTING keep the sign (C330) | the same | CCVS85 NC216A, which now passes |
| SEARCH ... VARYING one of the table's own indexes | the search uses that index, and the table's first is left alone (p. 437) | the same | CCVS85 NC235A, which now passes |
| An ALL literal compared with an item | as long as the item, so cut when it is longer (p. 17): ALL '00' compared with a `PIC 9` item is '0' | the same | CCVS85 NC250A, which now passes |
| DISPLAY of an integer or numeric intrinsic function, FUNCTION INTEGER(-2.5) | refused when compiled: such a function can be used only where an arithmetic expression can (Language Reference SC27-8713-03, p. 499; Programming Guide SC27-8714-03, p. 56; C332), and COMPUTE gives its value to an item DISPLAY shows | shows the value and its sign | test programs |
| Floating-point exponentiation, and an exponent with decimal places or a division | `FUNCTION SQRT(10) ** 2` is 10, `A ** 0.5` and `8 ** (1 / 3)` with dmax above zero are floating point (Programming Guide SC27-8714-03, pp. 796, 800), zero to a negative power a size error (Language Reference SC27-8713-03, pp. 296-297, Table 32); zero to the power zero and a negative base to a fractional power take Table 32's values without running ON SIZE ERROR (C334) | `8 ** (1 / 3)` cuts the quotient first and gives 1.999999; zero to a negative power gives 0, and zero to the power zero is a size error | CCVS85 IF136A, which now passes |
| `PIC 99 VALUE "7"` | refused when compiled: a numeric item's VALUE literal must be numeric (Language Reference SC27-8713-03, p. 246) | warns and stores 70 | corpus |
| An environment-name in DISPLAY UPON other than SYSOUT, SYSLIST, SYSLST, SYSPUNCH, SYSPCH or CONSOLE, or a mnemonic-name for another (SYSERR, or any word) | refused when compiled (Language Reference SC27-8713-03, pp. 126, 334) | SYSERR written to standard error | corpus |
| DISPLAY of national data elsewhere than the console | written as its UTF-16 bytes, unconverted; only UPON CONSOLE converts to the code page, a character it lacks as X'3F' (Language Reference SC27-8713-03, p. 333; Programming Guide SC27-8714-03, p. 36) | the same | corpus |
