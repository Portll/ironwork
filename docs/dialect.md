# Dialect: comparing ironwork with a GnuCOBOL build

`--dialect ibm|gnucobol` chooses whose result ironwork gives where it knowingly computes a different
one from GnuCOBOL's `cobc -std=ibm-strict`. `ibm`, the default, is Enterprise COBOL's result as
ironwork's register of assumptions reads it. `gnucobol` is GnuCOBOL 3.2's, so a migration that runs
a program under ironwork and under a GnuCOBOL build sees only the differences that matter to it.

**Status:** built, 2026-10-03 (ironwork-roadmap 3.11); checked against `cobc -std=ibm-strict`
2026-10-04 (3.11.1). Seven assumptions switch: C101, C14, C95, C15, C51, C180 and C262.
`--assume ID=VALUE` switches one of them alone (§2.1).

## 1. What the dialect switches, and what it does not

The dialect switches a chosen assumption (`Basis::Chosen` in `crates/numeric/src/assumptions.rs`): a
place where IBM's manuals leave the result open and ironwork picked one, and where cobc's behaviour
is clear and checked against the installed cobc. Each switched assumption's claim says what
`gnucobol` does instead.

Since 2026-10-08 the dialect is one part of a target ([targets.md](targets.md)), and the operator's
ruling that a GnuCOBOL target emulates every behaviour the documentation references takes in what
IBM documents too: C456 (§5.2) follows the dialect, and the rest of §5.2 follows one difference at
a time. What follows describes the switches as first built.

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

### 2.1 `--assume ID=VALUE`: one assumption at a time

`--assume` switches one of the seven, whatever `--dialect` says, so a comparison can find which
assumption a difference comes from. Each takes `ibm`, the result the register states, or `gnucobol`,
cobc's; C101 also takes `off`, a ROUNDED receiver's extra decimal place counted in no operation
(§3). A value names whose result it is or what it does.

| | |
|---|---|
| Flag | `--assume ID=VALUE` or `--assume=ID=VALUE`, repeatable; for one ID the last wins, and it wins over `--dialect` in either order. The same commands as `--dialect` take it, and `dump` refuses it |
| Refused | by name, as a usage error: an ID the register lacks; an assumption with no alternative, its basis named (`--assume C1=off: assumption C1 (documented) has no alternative; --assume switches C101, C14, C95, C15, C51, C180 and C262`); a value its switch does not take (`C14 takes ibm or gnucobol`) |
| Options | `Options::assumed`, set by `Options::apply_flag("--assume=ID=VALUE")`; `Options::dialect_of` and `Options::extra_place` give the value in force. `numeric::options::SWITCHES` lists the IDs and their values |
| Load module | after the `OPTIONS` section's records, for each program compiled with one ([load-module.md](load-module.md) §5.1); `ironwork dump` prints each as `assume C101=off` |
| Provenance | the flag in `externalParameters.flags`, and in `optionsInForce.assumed` each switched assumption whose value in force is not `ibm`, with its value ([evidence.md](evidence.md) §2) |
| Evidence | the journal's `open` record keeps `--assume` and its ID=VALUE in `argv` |

| Flags | `COMPUTE D ROUNDED = D + E / 3`, then `DISPLAY` of an `S9(3)V99 COMP-3` -1.25 and of `1.5` |
|---|---|
| none | `000051B`, `0012N 1.5` |
| `--assume C14=gnucobol` | `000051B`, `-00125 1.5` |
| `--dialect gnucobol --assume C14=ibm` | `000051A`, `0012N 15` |
| `--assume C101=off` | `000051A`, `0012N 1.5` |

The interpreter and the VM read the dialect from the same options and give the same result. The
lowering reads it where it fixes a result in the LIR: an arithmetic plan's `inner_dmax` (C101), a
binary item's DISPLAY digits (C14), a numeric literal's DISPLAY text (C95) and which zoned
comparisons read bytes (C262).

## 3. Switched

### C101: a ROUNDED receiver's extra decimal place

Under `ibm` a ROUNDED receiver counts in dmax with one decimal place more than it holds, in every
operation of the statement (assumption C101, chosen so that CCVS85 NC117A and NC171A pass). cobc's
arithmetic-osvs, which `-std=ibm-strict` sets, truncates each intermediate result to dmax and
computes the statement's last operation exactly. Under `gnucobol` the extra place counts in the last
operation alone, the one whose result the receivers take, and every operation below it carries dmax
with each receiver's own places.

Under `--assume C101=off` a ROUNDED receiver counts with its own places in every operation, so a
quotient that is the statement's last operation keeps no digit for rounding to read either:
`COMPUTE S ROUNDED = 1661.7 / DIV2` gives 37.6, where `ibm` and `gnucobol` give the 37.7 CCVS85
NC117A and NC171A expect. The Programming Guide's "might be carried" (p. 794) allows it.

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

cobc compiles at `-std=ibm-strict`, GnuCOBOL's strict reading of Enterprise COBOL. `-std=ibm` adds
GnuCOBOL's relaxed syntax, limits and reserved words to the same settings (its `ibm.conf` includes
`ibm-strict.conf`), and gives each switched assumption the same result.

`crates/exec/src/tests/dialect.rs` runs each switched difference under both dialects on the
interpreter, in the differential run, and on the VM alone; the expected `gnucobol` output is what
GnuCOBOL 3.2 printed. `crates/cli/tests/dialect_flag.rs` checks the flag on each command and where it
is recorded.

`fixtures/dialect` holds a program for each switched assumption. `tools/differ.py --cobc` compiles
each with the installed `cobc -x -std=ibm-strict`, runs it under ironwork with `--dialect gnucobol`,
and reports any difference; every one agrees, and so does `bench/packed.cbl`:

    tools/differ.py target/release/ironwork fixtures/dialect bench/packed.cbl --cobc

## 5. Found, not switched

Each difference below shows under both dialects. Found by running programs under ironwork, on
2026-10-03, and under `cobc -x -std=ibm-strict` (GnuCOBOL 3.2.0), on 2026-10-04: `bench/`; the 403
CCVS85 routines, prepared as `tools/nist.py` prepares them, of which both run 375; the 157 of
ironwork's own test programs both compile (those without option cards, files, EXEC, object-oriented
COBOL, the clock or random numbers), of which 86 differed; and 300 programs drawn from the corpus
that need no files, of which 114 ran under both and 20 differed. `-std=ibm` gives each of these
programs that both standards compile the same result, FUNCTION RANDOM and the clock aside; 5.6 says
which programs each refuses. Every difference was reduced to a small program run under both. Where a
cobc option removes one, it is named.

### 5.1 The platform

| Difference | Seen in |
|---|---|
| Collating order: EBCDIC under ironwork, ASCII under cobc, in comparisons, SORT and MERGE keys, and table SORT | CCVS85 ST137A, ST144A, ST147A, whose X-cards 063 and 064 give EBCDIC order (`-fdefault-colseq=EBCDIC` passes them); test programs; corpus |
| Character codes: hexadecimal literals, ORD and CHAR, HIGH-VALUE and LOW-VALUE shown, an overpunched sign seen through an alphanumeric item, the card images ACCEPT transfers, a character CCSID 1140 has no byte for | test programs; corpus |
| JSON and XML GENERATE write UTF-8 into an alphanumeric receiver; DISPLAY then reads the bytes in the program's EBCDIC code page | corpus |
| Floating point: IBM hexadecimal under ironwork, IEEE under cobc, in COMP-1 and COMP-2 values, the floating-point functions, and an exponent beyond HFP's range (S0CC) | test programs |
| The same failure in another form: S0C4 for a LINKAGE item with no address, or a reference beyond the run unit's storage (C458), where cobc takes SIGSEGV, `attempt to reference invalid memory address`; CEE3501S and U4038, or ironwork's IEW2456E refusal, where cobc says `module not found`; U4038 for an I/O failure no FILE STATUS, declarative or AT END or INVALID KEY phrase takes, with IGZ0035S, IGZ0020S, IGZ0197S or IGZ0002S where IBM names a message (C451), where libcob names the status, `file does not exist (status = 35)`, `end of file (status = 10)`, `record key does not exist (status = 23)` | test programs; probe |
| Setting the UPSI switches: the runtime option UPSI(nnnnnnnn) in the PARM under ironwork (C411), `COB_SWITCH_0` to `COB_SWITCH_7` set to `ON` in the environment under cobc | CCVS85 NC108M, NC211A, NC254A; probe |

### 5.2 IBM documents it, and cobc -std=ibm-strict does otherwise

| Difference | ironwork, as IBM documents | cobc | Seen in |
|---|---|---|---|
| DISPLAY of a signed zoned item | the last digit overpunched: -12 in `S9(3)` shows `01K` (Programming Guide, DISPSIGN). Under `gnucobol`, with no DISPSIGN card or `--numeric-display`, ironwork shows cobc's form (operator 2026-10-08) | a separate trailing sign, `012-` (`display_numeric`, libcob/termio.c); `-fpretty-display` gives `-012`, which is ironwork under `--numeric-display cobc` | corpus, 4 of 20 differing programs; test programs |
| DISPLAY of a zoned item holding other characters than digits | the bytes as stored; under `gnucobol` cobc's, where a letter in the sign position reads as IBM's overpunched digit, ironwork holding the item in EBCDIC | the default dialect moves it to a numeric-edited copy: a sign first, the characters kept before the decimal point and shown as 0 after it, `B2.00` for `B200` in `9(2)V99` (`pretty_display_numeric`, libcob/termio.c; `cob_move_display_to_edited`, libcob/move.c). -std=ibm-strict moves it to a copy with a separate sign, a space shown as 0: `1204+` for `12 4` in `S9(4)` (`display_numeric`) | probe; test programs |
| The RETURN-CODE special register | `S9(4) BINARY` (Language Reference, RETURN-CODE); under `gnucobol` cobc's fullword, the run's report and exit status taken from its last halfword (C457) | a fullword, never cut to its nine digits: DISPLAY shows `+000000007`, and 70000 is kept | test programs |
| A binary item larger than its PICTURE | cut to the PICTURE under TRUNC(STD), IBM's default: `MOVE 123456` to `9(4) COMP` keeps 3456 | keeps the halfword's value, 57920: `-std=ibm-strict` sets `binary-truncate: no`, which is TRUNC(BIN); `-fbinary-truncate` cuts. Give ironwork `CBL TRUNC(BIN)` to compare | CCVS85 NC105A; corpus; test programs |
| ACCEPT into an item longer than a line | the next records fill it, each concatenated with the one before (Language Reference, ACCEPT) | one line, the rest of the item spaces (termio.c `cob_accept`); ironwork's under `gnucobol` | CCVS85 NC204M |
| ACCEPT into a numeric item | the characters as they come, with no editing or checking (Language Reference, ACCEPT) | the line moved as a MOVE of an alphanumeric item does: `12` gives `012`, `-3.5` gives -3.5; ironwork's under `gnucobol` | test programs |
| MOVE of an alphanumeric item to a numeric one | the sender read as an unsigned integer; a character other than a digit gives the low half of its byte (C240, chosen) and can be a data exception when the item is next read as a number | a sign, decimal point and spaces read as such, any other character before the receiver is full making the whole value zero (`cob_move_alphanum_to_display`, libcob/move.c); ironwork's under `gnucobol` | corpus, 1 output and 2 S0C7 abends under ironwork; test programs |
| MOVE of a zoned item to a zoned one | each digit the low half of the sender's byte, the zone F (C260, chosen); under `gnucobol` the bytes, where one is no digit character, as cobc's | the bytes copied, a space as 0 and the sign byte read as `cob_real_get_sign` reads it (`store_common_region`, libcob/move.c) | test programs |
| A receiving group holding its own OCCURS DEPENDING ON object | received at its maximum length (Language Reference, OCCURS DEPENDING ON), by MOVE, READ INTO, RETURN INTO, STRING, UNSTRING and ACCEPT | at the object's current value: `-std=ibm-strict` sets `odoslide: yes`; `-fno-odoslide` passes MOVE, but stops sliding the items after the table | CCVS85 NC247A, SQ214A, ST146A; test programs |
| `ADD X TO X Y` | the sum of the operands before TO kept in a temporary for every receiver (Language Reference, ADD); under `gnucobol` one sending item read again for each receiver, as cobc's | X read again for Y (cobc/typeck.c), where there is one sending item; two or more summed once | test programs |
| A signed zoned item compared with an alphanumeric operand | the sign byte's zone made F under ZWB (Programming Guide, ZWB; C221), so an item of spaces does not equal SPACES; under `gnucobol` cobc's | a sign removed from a digit, a space left in the sign position, so an item of spaces equals SPACES, and another character read as 0 (`cob_real_get_sign`, libcob/common.c) | test programs; probe |
| ORD, CHAR and a contained program under a PROGRAM COLLATING SEQUENCE | ordinals in the program's sequence; a contained program takes its container's | ORD and CHAR native; a contained program has its own (libcob/intrinsic.c, cobc/codegen.c) | test programs |
| CANCEL of a program CALLed by a literal | no action under NODYNAM, IBM's default (Language Reference, CANCEL) | the program is reset; `-fstatic-call` does not change it. Give ironwork `CBL DYNAM` to compare | CCVS85 IC203A; corpus |
| ALTER in an independent segment | the altered GO TOs are put back each time the segment is entered from one of another priority (Language Reference, Procedures) | never put back: `-std=ibm-strict` sets `section-segments: ignore`; `-fsection-segments=ok` passes | CCVS85 SG102A, SG103A, SG201A, SG203A |
| The rest of a JSON GENERATE receiver | kept as it was (Language Reference, JSON GENERATE); under `gnucobol` spaces in the document's encoding, as cobc's | filled with spaces (libcob/mlio.c) | corpus |
| INSPECT of a national item | counts national characters (C230); under `gnucobol` TALLYING FOR CHARACTERS counts bytes, as cobc's | TALLYING FOR CHARACTERS counts bytes, 12 for `N(6)` (libcob/strings.c); ALL, LEADING, REPLACING ALL and CONVERTING work by character; REPLACING CHARACTERS BY a national literal is refused, `operand has wrong size` | corpus; probe |
| An intermediate result of more than 30 digits | cut to 30 (31 under ARITH(EXTEND)) (Programming Guide, Truncated intermediate results; C1) | every digit kept | probe |
| MEAN, MEDIAN, NUMVAL and COMBINED-DATETIME | floating-point results, rounded into the receiver (C111, chosen; Programming Guide on NUMVAL; C6) | exact decimal, truncated | corpus; test programs |
| MAX, MIN, RANGE, REM and SUM of fixed-point arguments | as many decimal places as the arguments have at most, counted in the expression's dmax (Programming Guide SC27-8714-03, pp. 794, 799; C390): `COMPUTE R = FUNCTION MAX(A B) / 3 * 3`, A `9V99` 1.00, B `9` 5, R `99V9`, gives 4.9 | MAX and MIN return the winning argument's own field (`cob_intr_max`, libcob/intrinsic.c), and intermediates around a function value carry other places: `10 / 3 * FUNCTION SUM(D B)`, D `9V99` 1.01, gives 19.8 where IBM's dmax gives 20.0 | probe |
| INTEGER, INTEGER-PART and MOD of fixed-point arguments | INTEGER one digit more than its argument, INTEGER-PART as many, MOD as many as its shorter argument (Language Reference SC27-8713-03, p. 601; Programming Guide SC27-8714-03, pp. 798-799; C392): `FUNCTION MOD(N H)`, N `S9` -3, H `999` 100, is 7 | a field as large as the value: 97 | probe |
| MOVE of an integer or numeric function to an alphanumeric item, which Enterprise COBOL refuses (Programming Guide SC27-8714-03, p. 119) | refused when compiled under `--compliance strict` (C394); under `extended`, with IWX0008-W, the value at the function's precision, moved as a numeric item of that precision is: an integer's digits, `00005` for MAX(N M) with N `999` 5 and M `9(5)` 4, and a value with decimal places refused at run time (Language Reference SC27-8713-03, p. 404) | the digits of the field cobc's function returns: the winning argument's own for MAX and MIN (`005`, and `825` for 8.25), nine for most others (`000000008` for INTEGER(8.25)) | probe |
| Invalid decimal data, and a zero divisor outside ON SIZE ERROR | the program check: S0C7, S0CB or S0C9; under `gnucobol` both as cobc's, where a letter in an overpunched sign position reads as IBM's overpunch, ironwork holding the item in EBCDIC | runs on. Invalid data is read with no check (`cob_decimal_set_display`, `cob_decimal_set_packed`, libcob/numeric.c): `'1 #4'` in `9(4)` is 1034, `'*'` worth 10; HIGH-VALUES in `9(4)` is 10000 and LOW-VALUES -10000; a packed sign B positive. A MOVE of a packed sender copies its half-bytes (`cob_move_bcd`) and DISPLAY shows each as '0' plus its value, `1<3`. A zero divisor leaves the receiver unchanged. Under `-debug` invalid data stops the run | test programs; corpus; probe |
| A main program whose control runs past its last statement | U4038 with IGZ0037S, placed at the last paragraph (Language Reference, Transfer of control; C456); under `gnucobol` the run ends as GOBACK ends it, as cobc's does | ends normally, return code RETURN-CODE | probe; 4.6% of corpus programs have no STOP RUN, GOBACK or EXIT PROGRAM |
| An OPEN or CLOSE of an indexed or relative file that fails, with no FILE STATUS and no declarative | control returns, and the next statement on the file is a logic error, U4038 with IGZ0020S (Programming Guide, Handling errors in VSAM files; C451); under `gnucobol` the run ends at the OPEN or CLOSE, U4038 with IGZ0035S, as cobc's does | the run ends at the OPEN or CLOSE: `libcob: error: file does not exist (status = 35)`, `file not open (status = 42)` | probe |
| An intrinsic function's argument outside what it takes, such as `FUNCTION CHAR(0)` and `FUNCTION RANDOM(-1)` | U4038 with Language Environment's message, IGZ0162S and IGZ0163S for these (C452); under `gnucobol` cobc's value | a value, and the run goes on: CHAR(0) gives X'00', the first character of the sequence, a numeric function zero and a function of characters none | probe |
| Zero to a negative power, with no ON SIZE ERROR | the run ends abnormally (Language Reference, SIZE ERROR phrases), U4038 with IGZ0050S, in fixed point as in floating point (C334); under `gnucobol` zero, as cobc's | 0, and the run goes on, with no size error even under ON SIZE ERROR | probe |

### 5.3 cobc bugs

| Difference | ironwork | cobc | Cause |
|---|---|---|---|
| Aligning an intermediate to more decimal places | `SUBTRACT A 2 B .04 FROM X` with A 10, B .3, X 12.34 gives 0.00; `X * (1 + OT)` with OT `V99` is not zero | 12.00; zero | `cob_decimal_align` (libcob/numeric.c:2286) divides where it should multiply. With the next two rows, arithmetic-osvs fails CCVS85 tests that pass under `-fno-arithmetic-osvs`: 39 of NC106A, 8 of NC175A, 6 of NC119A, 3 of NC118A, 3 of NC177A and NC250A's IF--TEST-37 |
| An intermediate whose next operand is a literal | `COMPUTE X ROUNDED = A / B * 100`, A 1, B 3, X `9(5)V99`: A / B is cut to dmax, 33.00 (33.30 under `ibm`) | 33.33 | a literal right operand is not loaded through `decimal_expand`, so `decimal_align` never cuts the result before it (cobc/typeck.c); IBM's table cuts every intermediate |
| `* 1`, `/ 1`, `+ 0` and `- 0` | the operation is carried out, its operand cut to dmax: `COMPUTE S ROUNDED = (1661.7 / DIV2) * 1` gives 37.6 under `gnucobol` | removed at compile time, so the quotient is the last operation: 37.7 | constant folding (cobc/tree.c, `cb_build_binary_op`) |
| ACCEPT FROM ARGUMENT-VALUE with ON EXCEPTION and NOT ON EXCEPTION, no argument left (`--compliance extended`, IWX0010) | the ON EXCEPTION phrase alone (C442) | both phrases, ON EXCEPTION first | seen in a probe of cobc 3.2; the receiver is unchanged in both |
| `'0' = '9'` under an alphabet with ALSO | compared in the program's sequence | folded at compile time; `-fno-constant-folding` fixes it | cobc/tree.c |
| SEARCH ... VARYING an item | the item stepped with the index | set to the index's value (cobc/codegen.c) | |
| A signed zoned item whose sign byte holds neither a digit nor a space, compared with an alphanumeric operand, moved or shown | the item unchanged | the byte left as 0: `MOVE '12$'` to the item's bytes, then `IF W = '120'`, a MOVE from W or DISPLAY W, leaves `120` | `cob_get_sign_ascii` writes 0 over the byte, and `cob_cmp`, `cob_move` and DISPLAY put back only the sign (libcob/common.c) |
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
| C112, an argument outside a function's domain | U4038 with the math services' CEE2010E, CEE2012E, CEE2016E or CEE2017E for SQRT, LOG, LOG10, ASIN and ACOS outside their domain and SIN, COS and TAN from pi*(2**50); U4038 with IGZ0152S for a character HEX-TO-CHAR or BIT-TO-CHAR does not take; for MOD and REM by zero the divide program check, S0CB, or S0CF for a floating-point REM, which ON SIZE ERROR takes as a size error | EC-ARGUMENT-FUNCTION set and never raised, and the run goes on: SQRT(-1), LOG(0), ACOS(2), MOD(7 0) and REM(7 0) give 0 without running ON SIZE ERROR, SIN(4000000000000000) gives 0.83381, HEX-TO-CHAR('1G') gives X'10' | Language Environment signals a condition for each, and MOD and REM divide by argument-2, so a value would hide what z/OS does |
| C181, a contained program CALLed from outside its container | found in ironwork's flat library, and the run ends when it uses a GLOBAL name | the CALL finds no program, as IBM's scope rules say | IBM documents the scope; a scope check belongs under both dialects |
| C333, a BY VALUE argument to a parameter received BY REFERENCE | the parameter gets storage of its own holding the value, and the called program runs | the value is read as an address, and the program faults | the Language Reference requires BY VALUE on both sides (p. 322) and gives no result; what the value addresses has no counterpart in ironwork's storage |
| C391, the integer places of MAX and MIN | as many as the widest argument's: MAX(N M), N `999` 5, M `9(5)` 4, moved to an alphanumeric item gives `00005` | the winning argument's own field: `005` | that field keeps the winning argument's decimal places too, which IBM documents (C390), so switching the integer places alone gives cobc's digits only where the arguments' decimal places agree |
| No assumption: DISPLAY of LENGTH OF | its value's 9 digits | a fixed size's LENGTH OF folded to a literal (`4`), a variable one `+0000000004` | unrecorded; it needs an assumption first |
| No assumption: the rest of an XML GENERATE receiver | kept as it was; under `gnucobol` spaces, as cobc's (with JSON GENERATE's, [targets.md](targets.md)) | filled with spaces (libcob/mlio.c) | unrecorded; C119 does not say it |

### 5.6 Programs one compiler refuses

Not results, so not the dialect's: what ironwork refuses that cobc accepts is `--compliance`'s
ground ([compliance.md](compliance.md)), and so is what ironwork accepts that IBM's rules refuse.

- **CCVS85.** Both refuse the communication routines (CM101M to CM105M, CM201M and CM202M, and
  DB205A, which has a COMMUNICATION SECTION) and IX110A. cobc alone refuses an ALL subscript in an
  intrinsic function's argument (11 routines from IF119A to IF141A), OBNC1M, CALL ... BY CONTENT in
  IC224A, IC225A and IC227A, and FUNCTION COS in IF106A. ironwork alone refuses the debugging
  routines DB201A to DB203A. cobc also refuses an UPSI switch's condition-name qualified by its
  mnemonic-name, and SET TO TRUE of one, which ironwork runs (C412).
- **Corpus.** Of 300, ironwork refused 10 cobc ran: 5 for syntax IBM does not have, 2 for IBM's
  limits, 2 for layout or punctuation, and `FUNCTION ALL INTRINSIC` in REPOSITORY, which IBM 6.4
  accepts and ironwork does not. cobc refused 31 ironwork ran: 17 for its area check (a statement or
  separator period in Area A, or a header or level-01 entry outside it), 6 for END-DISPLAY, which
  Enterprise COBOL does not reserve (Language Reference, Reserved words), 2 for VALUES outside a
  level-88 entry, 1 for a name longer than 30 characters, 1 for a numeric VALUE on a numeric-edited
  item (Language Reference, VALUE clause, which asks for an alphanumeric literal), and 4 others.
  ironwork, under its default `--compliance strict`, accepts what cobc refuses in each.
- **Test programs.** cobc refused 34 that ironwork runs, among them ENTRY in Area B, CALL ... BY
  CONTENT, FUNCTION COS and END-DISPLAY.

Two kinds of cobc refusal depart from IBM's rules. GnuCOBOL 3.2's IBM word list lacks CONTENT and
COS, which Enterprise COBOL has: `-freserved=CONTENT` passes CALL ... BY CONTENT, and no option
restores COS short of another word list. Its area check wants ENTRY in Area A, where IBM's reference
format puts every statement in Area B (Language Reference, Area B); `fixtures/dialect/ENTRIES.cbl`
starts its ENTRY in Area A, which both compilers accept. `-std=ibm` makes the area check warn and
takes GnuCOBOL's own word list in place of IBM's, so it compiles most of what ibm-strict refuses,
and it refuses NC211A and two test programs that ibm-strict compiles, each for a word that list
reserves (NOTHING, TAB and FULL).

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
