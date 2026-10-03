# Compliance level: `--compliance strict|extended`

ironwork's target is IBM Enterprise COBOL for z/OS, and by default it refuses what Enterprise
COBOL refuses. Real programs are also written for Micro Focus and GnuCOBOL. `--compliance
extended` accepts the extensions below, each with a warning that names it and where it is, and
compiles and runs them; anything else stays refused with the message strict gives.

| Level | What it does |
|---|---|
| `strict` (the default) | Today's behaviour, unchanged: a construct Enterprise COBOL does not have is refused |
| `extended` | The six extensions below are read as Micro Focus and GnuCOBOL read them, each with a warning |

The flag is `--compliance strict|extended` (or `--compliance=extended`) on `run`, `check`, `cics`,
`compile`, `job`, `fuzz` and `compare`. It is a compile option: `numeric::Options::compliance`,
carried in the load module's `OPTIONS` section (load-module.md §5.1), named among the options in
force in `--provenance` statements, and kept with its value in an evidence journal's `argv`. A
CALLed program, a class and a job step's program are read under the same level as the program
that names them, and `ironwork run` of a load module reads a source it CALLs under the level the
module's first program was compiled with.

Each warning is severity W, so a program that uses an extension and nothing else checks with
return code 4 and runs; `-warnings-block` refuses it, as it refuses any program with warnings.

## The extensions

All six are read before the parser sees the program, into constructs Enterprise COBOL has, so the
interpreter and the VM run them with the code they run anything else with, and a module records
nothing about them beyond the level. A test runs a program using every one on both executors and
compares the runs (`exec/src/tests/compliance.rs`).

### IWX0001-W free-form source

`IWX0001-W free-form source (Micro Focus and GnuCOBOL; Enterprise COBOL reads fixed form alone)`,
followed by why the file is free form.

A file is read in free form:

- from the line after a source-format directive that selects it: `>>SOURCE [FORMAT] [IS] FREE`
  (GnuCOBOL, the 2002 standard) or `$SET SOURCEFORMAT"FREE"` (also `'FREE'` and `(FREE)`; Micro
  Focus, which GnuCOBOL accepts too), `>>SET SOURCEFORMAT"FREE"` likewise. `FIXED` in the same
  places returns to fixed form. `>>` may start in any column; `$` is the directive indicator in
  column 7 or column 1. The warning is at the directive;
- from its first line, when a line before any such directive cannot be fixed form: its text
  starts in columns 1 to 6 with a character that is not a digit, and column 7 holds something other
  than a space, `*`, `/`, `-`, `D` or `d`. A tab counts as reaching the next column after a multiple
  of 8 for this test, as both compilers place it. Neither compiler guesses the form; ironwork does,
  by a rule no fixed-form line Enterprise COBOL accepts can meet. The warning is at that column 7;
- when it is a COPY member copied from a free-form line: GnuCOBOL and Micro Focus carry the format
  into the member. Such a member has no warning of its own; its own directive or lines can still
  switch it.

In free form there is no sequence area, indicator column or Area A: a line's text runs from column
1 to the end of the line, whatever its length. `*>` begins a comment anywhere outside a literal. A
`*` or `/` in column 1 makes the line a comment, and `D` followed by a space a debugging line, as in
Micro Focus's free format; GnuCOBOL refuses such lines unless they start `*>`, the only place the
two read free form differently. There are no continuation lines: a literal that reaches the end of
a line is refused, as in both compilers (use `&`, below). A comment-entry (AUTHOR. and the like)
ends with its line, as GnuCOBOL ends it. Since Area A does not exist, a paragraph header is a word,
or digits, that comes right after a separator period, is not a reserved word, and is followed by a
period: what the parser takes from Area A in fixed form.

Any other compiler directive (`>>IF`, `>>DEFINE`, `>>TURN`, `>>D`, `$SET` with another directive)
is refused, naming the directive: ironwork evaluates none of them.

### IWX0002-W constant entries

`IWX0002-W constant entry (Micro Focus and GnuCOBOL; Enterprise COBOL has no level 78 and no
CONSTANT clause): NAME stands for its value wherever it is used after this entry`, at the level
number.

`78 name [IS] [GLOBAL] VALUE [IS] value.` and the 2002 standard's `01 name CONSTANT [IS] [GLOBAL]
[AS] value.` define a constant. The value is a literal, a figurative constant, a constant defined
before it, or literals joined by `&`. The entry is no data item: each later use of the name, in the
DATA or PROCEDURE DIVISION, is the value, as a literal written there would be, and `(name)` in a
PICTURE is the value when it is an unsigned integer, so `PIC X(MAX-LEN)` and `OCCURS MAX-LEN TIMES`
work. A constant applies from its entry to the end of the source, nested programs included, which
accepts a contained program's use of a constant that is not GLOBAL; a program that uses one such
name for both a constant and a data item is refused.

A value that is an expression (`78 N VALUE 1 + 2`, `LENGTH OF`, `START OF`, `NEXT`) is refused:
`constant NAME: the value is a literal, a figurative constant, a constant defined before, or
literals joined by &; ironwork computes no expression there`. Strict refuses level 78 as before
(`level 78 is not a data level`) and the CONSTANT clause as an unknown clause.

### IWX0003-W `<>`

`IWX0003-W <> (Micro Focus and GnuCOBOL; Enterprise COBOL writes NOT =) is read as NOT =`, at the
`<`. `<>` written as one token is the relational operator NOT =, in a condition and in an
abbreviated combined condition alike. Strict keeps `<> is not an Enterprise COBOL relational
operator: it writes NOT =`.

### IWX0004-W literal concatenation with `&`

`IWX0004-W literal concatenation with & (Micro Focus and GnuCOBOL; Enterprise COBOL has none): the
literals on either side are one literal`, at the `&`.

`literal & literal` is one literal wherever a literal goes, VALUE clauses and level-78 values
included, and the operands may be on different lines. Alphanumeric literals (with `Z'...'`, whose
X'00' is kept) join into an alphanumeric literal; hexadecimal literals into a hexadecimal one; an
alphanumeric and a hexadecimal literal into an alphanumeric literal holding the hexadecimal
literal's bytes as they are, read in the program's code page; national literals into a national
literal. Either operand may be a constant that stands for such a literal. Anything else beside
`&` is refused: `& joins two alphanumeric or hexadecimal literals, or two national literals, either
of which may be a level-78 constant standing for one`. Strict keeps `literal concatenation with &
is not Enterprise COBOL's`.

### IWX0005-W BINARY-SHORT, BINARY-LONG and BINARY-DOUBLE

`IWX0005-W the COBOL 2002 binary usage (Micro Focus and GnuCOBOL; not Enterprise COBOL's):
BINARY-LONG is read as PIC S9(9) COMP-5`, at the usage word.

`[USAGE [IS]] BINARY-SHORT`, `BINARY-LONG` and `BINARY-DOUBLE`, each `SIGNED` (the default) or
`UNSIGNED`, are `PIC S9(4)`, `S9(9)` and `S9(18)` `COMP-5`, or the same PICTURE without `S`:
two, four and eight bytes of binary whose value is limited by the bytes, not the PICTURE. These are
the equivalences GnuCOBOL documents, and the sizes and ranges Micro Focus gives. The item then
behaves as such a COMP-5 item does under Enterprise COBOL: a DISPLAY of it shows the digits and
sign as Enterprise COBOL shows a COMP-5 item's, where GnuCOBOL shows a separate sign and one more
digit for BINARY-LONG.

`BINARY-CHAR` is refused: `BINARY-CHAR is a one-byte binary item, and ironwork's binary items are
two, four or eight bytes, as Enterprise COBOL's are`.

### IWX0006-W PROGRAM-ID without the IDENTIFICATION DIVISION header

`IWX0006-W PROGRAM-ID with no IDENTIFICATION DIVISION header before it (COBOL 2002, Micro Focus and
GnuCOBOL; Enterprise COBOL requires the header): the program reads as though IDENTIFICATION
DIVISION. came before it`, at PROGRAM-ID. The 2002 standard made the header optional, and GnuCOBOL
under its default, `mf` and `ibm` dialects reads a program, or a contained program, that begins
with PROGRAM-ID. The comment-entries after it are read as after the header.

## How the six were chosen

From the IBM-valid-share census of 2026-10-02 (the local measurement `2026-10-02-ibm-share-030`:
3,000 programs sampled with seed 1 from 3185RecentCobolRepos, each given the copy libraries
`tools/census.py` gives it), the programs ironwork refuses, ranked by how many contain each
construct that is not Enterprise COBOL's, in programs and repositories. A construct was taken where
Micro Focus and GnuCOBOL give it one meaning:

| Construct | Programs | Repositories | Here |
|---|---|---|---|
| Free-form source | 754 | 112 | IWX0001 |
| Level-78 or CONSTANT entries | 89 | 25 | IWX0002 |
| SCREEN SECTION | 86 | 25 | refused |
| `<>` | 61 | 21 | IWX0003 |
| BINARY-SHORT, -LONG, -DOUBLE | 58 | 21 | IWX0005 |
| PROGRAM-ID with no IDENTIFICATION DIVISION header | 57 | 12 | IWX0006 |
| `&` between literals | 43 | 27 | IWX0004 |
| ACCEPT FROM ENVIRONMENT, SET ENVIRONMENT | 42 | 20 | refused |
| OCCURS at level 01 or 77 | 40 | 18 | refused |
| BINARY-CHAR | 32 | 13 | refused |

Of the 1,764 programs strict refuses (294 repositories), many hold more than one; the free-form
count includes non-COBOL files named `.cbl` and generated programs, which no reading compiles.

## What extended compiles

The census samples, run with `ironwork check` at b0bb32e, the commit this change follows, and with
it:

| Sample | Strict | Extended | Newly compiling | Repositories with a program that compiles |
|---|---|---|---|---|
| 3185RecentCobolRepos, 3,000 programs | 1,236 | 1,382 | 146 in 63 repositories | 367 to 405 of 561 |
| 500RandomCobolRepos, 3,000 programs (`2026-09-30-ibm-share`) | 1,436 | 1,485 | 49 in 14 repositories | 86 to 94 of 140 |

No program strict compiles is refused under extended, and strict's output, every message and
return code, is the same as b0bb32e's on all 6,000 programs. In the newly compiling programs of
the first sample, the warnings name free form in 124 (53 repositories; in 116 it is the only
extension), constants in 11, `<>` in 12, `&` in 1, the binary usages in 4 and the missing header in
6; in the second, free form 32, constants 18, `<>` 9, the binary usages 5 and the missing header 17.
Most free-form programs still refused fail on something else first: a member not found, a name
never defined, a non-COBOL file, or a construct below.

## What stays refused, and why

Each is refused under extended with the message strict gives, except BINARY-CHAR and the compiler
directives, whose messages under extended say why. The counts after each are the programs and
repositories in the first sample where it is the first refusal under extended.

- **BINARY-CHAR** (48 programs, 12 repositories): a one-byte binary item, which ironwork's storage,
  like Enterprise COBOL's, does not have. Reading it needs a new binary size in the layout and the
  module format.
- **SCREEN SECTION** (33, 16): Micro Focus and GnuCOBOL screen handling differ in detail, and
  ironwork's terminal is a 3270 driven through BMS.
- **ACCEPT ... FROM ENVIRONMENT, SET ENVIRONMENT** (30, 12): GnuCOBOL's one-statement forms, which
  most of these programs use, are not Micro Focus's (DISPLAY UPON ENVIRONMENT-NAME then ACCEPT FROM
  ENVIRONMENT-VALUE), and the GnuCOBOL programs that set the environment set the screen runtime's
  options.
- **OCCURS at level 01 or 77** (18, 10): a record that is a table needs a change to the storage
  layout; the next candidate by evidence.
- **Split keys** (`RECORD KEY IS name = item item`; 35, 5): an indexed file keyed on items that are
  not contiguous, which the file system does not hold.
- **INSPECT ... TALLYING ... FOR TRAILING** (26, 3, 24 of them in one ACUCOBOL repository):
  GnuCOBOL and Micro Focus both read it, but it needs a new INSPECT mode in the runtime and the
  module format; a candidate after OCCURS at level 01.
- **Compiler directives other than the source format's** (`>>IF`, `>>DEFINE`, `>>TURN`, `>>D`,
  Micro Focus `$SET` options): ironwork evaluates no conditional compilation and sets no option
  from them.
- **Text past column 72 in fixed form** (Micro Focus's and GnuCOBOL's variable format): columns 73
  to 80 hold sequence text in Enterprise COBOL sources, and no rule tells the two apart without a
  directive.
- **Expressions as constant values**, `H'...'` literals, `UNSIGNED-INT` and `BINARY-LONG-LONG`,
  `FLOAT-*`, `READ ... WITH NO LOCK` and file `LOCK MODE`, `FUNCTION-ID` with GnuCOBOL's
  extensions: each found in only a few programs, or with meanings the two compilers do not share.
