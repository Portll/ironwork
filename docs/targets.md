# Target compilers: `--target NAME`

`--target` names the compiler ironwork emulates. Each target stands for a compliance level, a dialect
and a DISPLAY form, and every behaviour below follows from them: a target's results are derived,
not set one by one. A flag given beside `--target` overrides the target's own, so `--target gnucobol
--dialect ibm` reads GnuCOBOL's forms and gives IBM's results.

**Status:** built 2026-10-08 (ironwork-roadmap 29), on the operator's request for derived emulation of
every compiler behaviour the documentation references.

## 1. The targets

| `--target` | Compiler | Stands for | Checked against |
|---|---|---|---|
| `ibm` (the default) | IBM Enterprise COBOL 6.4 for z/OS | `--compliance strict --dialect ibm --numeric-display ibm` | the manuals, through the register of assumptions; no IBM compiler run |
| `gnucobol` | GnuCOBOL 3.2, cobc's default dialect | `--compliance extended --dialect gnucobol --numeric-display cobc` | cobc 3.2 on this machine |
| `gnucobol-ibm-strict` | GnuCOBOL 3.2, `cobc -std=ibm-strict` | `--compliance strict --dialect gnucobol --numeric-display cobc-ibm-strict` | cobc 3.2 on this machine |

Listed, not yet targets:

| Compiler | Why not yet |
|---|---|
| Micro Focus Visual COBOL | No Micro Focus compiler to check against. Its forms are read under `--compliance extended` as cobc `-std=mf` reads them ([compliance.md](compliance.md)); its results are not emulated |
| GCC COBOL (gcobol 16) | Runs in Docker here (`gcobol:16`), but its results have not been surveyed against ironwork's |

A "GnuCOBOL target" below is `gnucobol` or `gnucobol-ibm-strict`: any compile under `--dialect
gnucobol`, or under `--compliance extended`, `relaxed` or `loose`.

## 2. What each target sets

| Behaviour | `ibm` | GnuCOBOL targets | Emulated |
|---|---|---|---|
| Forms read: Micro Focus and GnuCOBOL extensions | refused, each by name | read under `gnucobol` (extended); refused under `gnucobol-ibm-strict`, as cobc -std=ibm-strict refuses them | yes ([compliance.md](compliance.md)) |
| DISPLAY of a number | IBM's form, the sign overpunched | cobc's default form under `gnucobol`, -std=ibm-strict's under `gnucobol-ibm-strict` | yes (`--numeric-display`) |
| A zero-length literal `''` | a space, IWS0106-E | a space, IWX0063-W | yes |
| C101, a ROUNDED receiver's extra place | every operation | the last operation | yes ([dialect.md](dialect.md) §3) |
| C14, DISPLAY of a packed or binary item | IBM's | cobc's | yes |
| C95, DISPLAY of a numeric literal under DECIMAL-POINT IS COMMA | IBM's | cobc's | yes |
| C15, ACCEPT at the end of SYSIN | IBM's | cobc's | yes |
| C51, a dynamic CALL of an ENTRY name | IBM's | cobc's | yes |
| C180, an EXTERNAL record of another size | IBM's | cobc's | yes |
| C262, zoned comparisons under NOINVDATA | IBM's | cobc's | yes |
| C456, a main program that runs past its last statement | U4038 with IGZ0037S | the run ends as GOBACK ends it, return code RETURN-CODE | yes |
| C491, CALL ... RETURNING of a program with no RETURNING phrase | the item kept as it was | the item takes the called program's RETURN-CODE | yes |
| RETURN-CODE after a CALL with RETURNING | kept as it was (Language Reference, CALL) | the called program's | yes, but after a program whose header says RETURNING OMITTED, where cobc keeps it |
| The RETURN-CODE special register's size | `S9(4) BINARY` | a fullword | not yet |
| DISPLAY of a zoned item holding other characters than digits | the bytes | under `gnucobol` a sign and the decimal point among the characters, each after the point shown as 0; under `gnucobol-ibm-strict` the sign after them, a space shown as 0 | yes |
| A binary item larger than its PICTURE | cut under TRUNC(STD) | kept, as TRUNC(BIN); give `CBL TRUNC(BIN)` | by card |
| ACCEPT into an item longer than a line (C261) | the next records fill it | one line, the rest spaces | yes |
| ACCEPT into a numeric item | the characters as they come | the line moved as an alphanumeric MOVE moves it | yes |
| MOVE of an alphanumeric item to a numeric one (C240) | an unsigned integer, each character's low half | one sign and the digits aligned on the decimal point; another character before the receiver is full gives zero | yes |
| MOVE of a zoned item holding other characters than digits to a zoned one (C260) | each digit's low half, zone F | each byte copied, aligned on the decimal point, a space as 0 and the sign byte read as a comparison reads it | yes |
| A receiving group holding its own OCCURS DEPENDING ON object | its maximum length | the object's current value | not yet |
| `ADD X TO X Y`, and SUBTRACT and MULTIPLY with one sending item | the sending item read once for every receiver | read again for each receiver, after the one before is stored; two or more sending operands summed once, as IBM sums them | yes |
| A signed zoned item compared with an alphanumeric operand (C221) | the sign byte's zone made F under ZWB, so a space reads as 0 | a sign removed from a digit; a space kept; another character read as 0 | yes |
| ORD, CHAR and a contained program under a PROGRAM COLLATING SEQUENCE | the program's sequence | native; a contained program its own | not yet |
| CANCEL of a program CALLed by a literal | no action under NODYNAM | the program is reset; give `CBL DYNAM` | by card |
| ALTER in an independent segment | put back on entry | never put back | not yet |
| The rest of a JSON or XML GENERATE receiver | kept | spaces, in the document's encoding | yes |
| INSPECT of a national item | national characters | bytes | not yet |
| An intermediate result of more than 30 digits | cut to 30 (31) | every digit kept | not yet |
| MEAN, MEDIAN, NUMVAL and COMBINED-DATETIME | floating point, rounded | exact decimal, truncated | not yet |
| MAX, MIN, RANGE, REM, SUM; INTEGER, INTEGER-PART, MOD of fixed-point arguments | IBM's places | cobc's fields | not yet |
| A zero divisor outside ON SIZE ERROR | S0CB, S0C9 or S0CF | the receiver kept as it was, and the run goes on | yes |
| Invalid decimal data in arithmetic, comparisons, MOVE and DISPLAY | S0C7 | read as libcob reads it: a zoned character worth the low half of its ASCII code, a packed digit its half-byte's value, HIGH-VALUE and LOW-VALUE as plus and minus 10 to the item's size; the run goes on | yes |
| An OPEN or CLOSE of an indexed or relative file that fails, with no FILE STATUS and no declarative (C451) | control returns; the next statement on the file is a logic error | the run ends at the OPEN or CLOSE, U4038 with IGZ0035S | yes |
| Zero to a negative power, with or without ON SIZE ERROR (C334) | a size error; without the phrase U4038 with IGZ0050S | zero, and the run goes on | yes |
| A function argument outside what it takes, such as `CHAR(0)` (C452) | U4038 with LE's message | CHAR the sequence's first character, a function of characters none, any other zero, RANDOM the seed's magnitude; the run goes on | yes |
| C16, C54, C99, C112, C181, C333, C391 ([dialect.md](dialect.md) §5.5) | ironwork's chosen result | cobc's | not yet |

"Not yet" rows give IBM's result under every target. Each is a task under ironwork-roadmap 29.3,
taken cheapest first and checked against cobc 3.2; [dialect.md](dialect.md) §5 gives each one's
detail and where it was seen. "By card" rows follow a compiler option a CBL card sets under every
target. A load module records the compliance level and dialect a target set, not the target.
