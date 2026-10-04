# Compliance level: `--compliance strict|extended`

ironwork's target is IBM Enterprise COBOL for z/OS, and by default it refuses what Enterprise
COBOL refuses. Real programs are also written for Micro Focus and GnuCOBOL. `--compliance
extended` accepts the extensions below, each with a warning that names it and where it is, and
compiles and runs them; anything else stays refused with the message strict gives.

| Level | What it does |
|---|---|
| `strict` (the default) | Today's behaviour, unchanged: a construct Enterprise COBOL does not have is refused |
| `extended` | The extensions below are read as Micro Focus and GnuCOBOL read them, each with a warning |

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

The first six are read before the parser sees the program, into constructs Enterprise COBOL has,
so the interpreter and the VM run them with the code they run anything else with, and a module
records nothing about them beyond the level. A test runs a program using every one on both
executors and compares the runs (`exec/src/tests/compliance.rs`). The seventh, ASSIGN to a data
item, has no Enterprise COBOL form: both executors run it through `rt::fileio`, and a module
records the item with its file (`lir::FileDesc::assign_item`; `exec/src/tests/assign.rs`). The
eighth, IWX0008-W, is the compiler's check on a MOVE's sender. The ninth, IWX0009-W, is read
before the parser as the first six are, and a test runs a caller and a program that uses it on
both executors and compares the runs. The tenth, IWX0010-W, the command line, has no Enterprise
COBOL form: both executors read it from the run unit's PARM arguments (`rt::le::parm::Arguments`),
and the LIR carries its ACCEPT sources and the op DISPLAY UPON ARGUMENT-NUMBER becomes.

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

### IWX0007-W ASSIGN to a data item

`IWX0007-W ASSIGN to a data item (Micro Focus and GnuCOBOL; Enterprise COBOL's assignment-name is
never a data item): each OPEN of FILE takes its DD name from ITEM`, at the item's name.

`SELECT file ASSIGN TO name` names a data item when `name` is an alphanumeric or group item's, as
GnuCOBOL's default assign clause and Micro Focus's ASSIGN(DYNAMIC) read it; `ASSIGN [TO] DYNAMIC
data-name` and `ASSIGN USING data-name` always name one, and `ASSIGN TO EXTERNAL name` never does. A
name no data item has stays a DD name. Enterprise COBOL's assignment-name "is not the name of a data
item, and cannot be contained in a data item" (Language Reference, ASSIGN clause), so under strict
the name is a DD name, as before, and DYNAMIC and USING are refused (C361).

At each OPEN the item's value, its blanks taken off, is the DD name, folded to upper case: GnuCOBOL
and Micro Focus map a name with no directory to a file through `DD_name`, and a DD maps one here. A
value that cannot be a DD name, such as a path, names no DD, nor does a name the run was not given,
and the OPEN fails as it does for a missing DD, with status 35 for a file that must exist. ironwork
never opens a host file a program names (C360). CLOSE closes the DD the OPEN found. A file that
SORT or MERGE reads, writes or describes cannot take its name from a data item: the sort opens its
files by their DD names.

With `--evidence --trace-marker`, each OPEN records the item's value as a `dynamic-file-path` sink
at the SELECT, where cobolwork places the finding, with the input of that file's item alone
(evidence.md §1.1).

### IWX0008-W an integer or numeric function as a MOVE's sender

`IWX0008-W an integer or numeric function as a MOVE's sender (GnuCOBOL; Enterprise COBOL takes one
only where an arithmetic expression can be): FUNCTION NUMVAL is moved as its value`, at the
function.

Enterprise COBOL refuses `MOVE FUNCTION NUMVAL(X) TO N` and `MOVE FUNCTION MAX(N M) TO A`, whatever
the receiver: "numeric functions are not valid as senders in MOVE statements" (Programming Guide
SC27-8714-03, p. 119). An integer or numeric function can be used only where an arithmetic
expression can (Language Reference SC27-8713-03, p. 499), MOVE sends an identifier or a literal
(p. 400), and no numeric function is among the valid operands of an elementary move (p. 402).
Strict refuses it under either dialect, `MOVE FUNCTION NUMVAL: an integer or numeric function can be
used only where an arithmetic expression can, not as a MOVE's sender` (S), as it refuses DISPLAY of
one (C332). MAX and MIN count as numeric when their first argument is; CONTENT-OF and user-defined
functions are not refused (C394). `COMPUTE N = FUNCTION NUMVAL(X)` is what IBM allows.

Unlike IWX0001-W to IWX0006-W, this one is not read before the parser: the compiler's check gives
the warning, and the MOVE moves the function's value at the precision IBM gives the function (C390
to C392), as IBM moves a numeric item of that precision. A numeric or numeric-edited receiver takes the
value. An alphanumeric receiver takes an integer's digits, `00005` for MAX(N M) with N `999` 5 and M
`9(5)` 4; a value with decimal places, or a floating-point one such as NUMVAL's, is refused at run
time, as a MOVE of such an item is (p. 404). `--dialect gnucobol` gives the same: cobc moves the
digits of the field its function returns, `005` there and `000000008` for INTEGER(8.25)
([dialect.md](dialect.md) 5.2), which ironwork does not reproduce.

### IWX0009-W PROCEDURE DIVISION RETURNING OMITTED

`IWX0009-W PROCEDURE DIVISION RETURNING OMITTED (GnuCOBOL; Enterprise COBOL's RETURNING names an 01
or 77 item of the LINKAGE SECTION): the program is read with no RETURNING phrase, and returns its
RETURN-CODE to its caller as any program does`, at RETURNING.

GnuCOBOL 3 writes a program that returns no item as `PROCEDURE DIVISION [USING ...] RETURNING
OMITTED.`, which cobc compiles to a C function returning `void` (GnuCOBOL 3.2 NEWS: "PROCEDURE
DIVISION RETURNING OMITTED -> callable as void function"; `_procedure_returning` in
`cobc/parser.y`). Under extended, `RETURNING OMITTED` before the header's period is taken out of a
program's header, which leaves a header Enterprise COBOL has; nothing else in the program changes.
Strict keeps `PROCEDURE DIVISION RETURNING OMITTED: not an 01 or 77 item of the LINKAGE SECTION`
(S): Enterprise COBOL's RETURNING names such an item (Language Reference SC27-8713-03, pp. 262-263).

What a caller then receives:

- A CALL with no RETURNING phrase: the caller's RETURN-CODE is set from the program's, as after the
  CALL of any program. cobc calls the `void` function through an `int` pointer and stores whatever
  the return register holds, a value C leaves undefined; cobc 3.2 on arm64 stored the program's
  RETURN-CODE.
- `CALL ... RETURNING item`: the item is left as it was, as for any program with no RETURNING
  phrase. cobc stores the same undefined value in it.
- `CALL ... RETURNING OMITTED` or `RETURNING NOTHING`, the one case GnuCOBOL's testsuite fixes (the
  caller's RETURN-CODE is left as it was: `run_misc.at`, "void PROCEDURE" and "void PROCEDURE,
  NOTHING return"), is refused under either level (`OMITTED is not defined`). Nearly every program
  in the two corpora that has it calls a C library with it (raylib, Agar, the C runtime), which
  ironwork cannot call, or is one of GnuCOBOL's tests.

cobc refuses the phrase in a program compiled as the main program (`-x`: "RETURNING clause cannot
be OMITTED for main program") and in a function. ironwork compiles every program alike, so such a
program run as the first of a run unit ends with its RETURN-CODE as any program does. A FUNCTION-ID's
or method's header keeps the phrase, and is refused as under strict. Micro Focus's header takes a
data-name after RETURNING, and its documentation gives no OMITTED there.

The programs that use it are GnuCOBOL programs over C libraries, OlegKunitsyn's
`gnucobol-examples/microservice.cbl` and gnucobol-contrib's `cobweb-agar.cob` among them, each
copied into several repositories, and GnuCOBOL's own tests of it (copied in
infinityabundance_gnucobol-rs). The examples still stop on something else first: a COPY member
their repositories do not hold, `>>IF`, and past those `ANY LENGTH`, BINARY-INT and `EXTERN`; see
"What extended compiles".

### IWX0010-W the command line: ACCEPT FROM COMMAND-LINE, ARGUMENT-NUMBER and ARGUMENT-VALUE

`IWX0010-W ACCEPT ... FROM COMMAND-LINE (Micro Focus and GnuCOBOL; Enterprise COBOL reads no command
line): CL receives the job step's PARM program arguments`, at the statement, and the same for
ARGUMENT-NUMBER, ARGUMENT-VALUE and `DISPLAY ... UPON ARGUMENT-NUMBER`.

Micro Focus and GnuCOBOL give a program its command line: `ACCEPT x FROM COMMAND-LINE` the whole
line after the command, `ACCEPT n FROM ARGUMENT-NUMBER` how many arguments it holds, `ACCEPT x FROM
ARGUMENT-VALUE [[ON] EXCEPTION ...] [NOT [ON] EXCEPTION ...] [END-ACCEPT]` the next argument each
time, the exception taken and `x` left as it was once none is left, and `DISPLAY n UPON
ARGUMENT-NUMBER` which argument comes next. A z/OS batch program's command line is its job step's
PARM, so under extended these read the PARM's program arguments, what precedes its last slash when
runtime options follow it (CBLOPTS(ON), C250), split at blanks (assumption C442):

| PARM `alpha be 0042/RPTOPTS(ON)` | ironwork | cobc 3.2, `prog alpha be 0042` |
|---|---|---|
| COMMAND-LINE into `X(20)` | `alpha be 0042` and spaces | the same |
| ARGUMENT-NUMBER | 3 | 3 |
| ARGUMENT-VALUE, four times | `alpha`, `be`, `0042`, then the exception | the same |
| `DISPLAY 9 UPON ARGUMENT-NUMBER`, then ARGUMENT-VALUE | `0042`, the last | the same |
| `DISPLAY 0 UPON ARGUMENT-NUMBER`, then ARGUMENT-VALUE | the exception: a PARM has no word 0 | the command's own name |
| NOT ON EXCEPTION when the exception is taken | not run | run as well as ON EXCEPTION |

A run with no `--parm`, and a job step with no PARM, have an empty command line: COMMAND-LINE gives
spaces, ARGUMENT-NUMBER zero, and ARGUMENT-VALUE the exception. The shell's quotes, which keep words
together on a command line, mean nothing in a PARM. The value moves as an alphanumeric sender and
the count as a numeric one, and the receiver is input to the trace when a PARM gave it. Strict
refuses each form (S), and under extended ON EXCEPTION goes with ARGUMENT-VALUE alone, and
`DISPLAY UPON ARGUMENT-NUMBER` shows one numeric item or literal.

The 3185-repository corpus has COMMAND-LINE in 628 files, ARGUMENT-VALUE in 333, ARGUMENT-NUMBER in
215 and `DISPLAY UPON ARGUMENT-NUMBER` 98 times, in 119 repositories; ENVIRONMENT-VALUE, which pairs
with `DISPLAY UPON ENVIRONMENT-NAME`, stays refused (below).

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

IWX0007, ASSIGN to a data item, came later and from elsewhere: cobolwork's `dynamic-file-path`
findings, 689 of them unlabelled in its 500-repository corpus until ironwork could run such a
program and trace the name it opens.

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

IWX0009-W changes nothing in either sample: at 978e0af and with IWX0009-W on it, strict compiles
1,166 programs of the first and 1,432 of the second, and extended 1,375 and 1,483, every message and
return code the same at both commits under either level. Over every program of both corpora (133,343
and 45,354) under extended, the 9 programs RETURNING OMITTED alone refused now compile, all copies
of three GnuCOBOL tests in one repository; the other 224 programs that hold `RETURNING OMITTED`, in
a header or a CALL, give what they gave before.

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
  options. ACCEPT reads SYSIN, SYSIPT and CONSOLE, or a mnemonic-name for one; extended adds
  COMMAND-LINE, ARGUMENT-NUMBER and ARGUMENT-VALUE (IWX0010), and any other FROM operand is refused:
  ENVIRONMENT-VALUE, the screen sources (ESCAPE, EXCEPTION, LINES, COLUMNS, CRT), USER, and any other
  name. Assumption C440 says how ironwork reads the console.
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

### Operands and phrases the Language Reference bars

The compiler refuses at severity S, under either level, the operands and phrases Enterprise COBOL
bars that it once accepted or left to code generation. Each was measured over every program of
both corpora under extended at 978e0af; the counts are the programs whose messages hold the refusal
and, in brackets, those it alone refuses. Only RETURNING OMITTED, above, met both tests: one meaning
in GnuCOBOL and Micro Focus, and programs written to be run that use it.

- **An arithmetic expression or numeric function compared with an operand that is not numeric**
  (9 programs, 6 repositories [8]). The four such programs in three repositories that were written
  to be run (`login.cob` and `registration.cob` of a course project, and one `SCR3USR.cbl` copied
  into two training repositories) compare `FUNCTION TRIM` or `UPPER-CASE` of a `PIC 9(4)` or
  `PIC 9(10)` item with an alphanumeric operand: the check takes TRIM of a numeric item to be
  numeric. Enterprise COBOL refuses these programs too, as TRIM's and UPPER-CASE's argument is
  alphabetic, alphanumeric, national or UTF-8 (Language Reference SC27-8713-03, pp. 657 and 663),
  and Micro Focus documents the same classes for TRIM; cobc gives the item's digits for an
  unsigned integer but text that varies with its dialect for a signed or decimal one (`-012` under
  `-std=default`, `012-` under `-std=ibm`). The rest are a student's `FUNCTION ORD(X) >= "A"`,
  three copies of a GnuCOBOL test that continues a literal with `-` after it, and a conformance
  test. cobc compares an expression with an alphanumeric operand by rules no manual gives (`N + 1 =
  X` is false where N is 5 and X is `"6"` padded with spaces, `N + 1 = "6"` true), and stops with
  an internal compiler error at `IF N + 1 = SPACE`.
- **A condition-name used as data** (16 programs, 7 repositories [5]; 2 in the second corpus [2]):
  `UNTIL WS-EOF = 'Y'` where WS-EOF is the 88, `IF CUST-STATUS = ACTIVE`, `INSPECT ... FOR ALL`
  an 88, `EVALUATE item WHEN` its 88, `MOVE 1 TO` or `DISPLAY` of an 88, and GnuCOBOL's own
  syntax tests. cobc
  refuses each: "condition-name not allowed here", "invalid use of 88 level in WHEN expression".
- **PERFORM VARYING FROM or BY an arithmetic expression** (14, 3 [10]): 13 programs written by
  language models (`FROM I + 1`) and a negative test. cobc refuses it: "syntax error, unexpected
  +, expecting UNTIL", under `-std=default`, `mf`, `ibm` and `cobol2014`.
- **PERFORM VARYING an item that is not numeric** (6, 5 [5]): two negative tests, a model's
  program, and two programs that vary a `PIC X(2)` item or start one `FROM 'BSL'`. cobc refuses it:
  "PERFORM VARYING ... is not a numeric field".
- **A figurative constant as an intrinsic function's argument** (19, 2 [17]): a conformance
  suite's tests and copies of two GnuCOBOL tests. cobc takes `SQRT(ZERO)` and refuses
  `ABS(SPACE)` ("FUNCTION 'ABS' has invalid argument").
- **ALL with a numeric literal** (13, 12 [0]): copies of ProLeap's parser and interpreter tests of
  `VALUE ALL 2`, which also fail on something else. cobc refuses it: "invalid VALUE clause", "invalid MOVE
  statement".
- **An EXEC CICS HANDLE label that names no paragraph** (6, 2 [0]): copies of IBM example programs
  with bugs planted for a study. GnuCOBOL and Micro Focus have no CICS translator to give it a
  meaning.
- **ALL subscripts in a function of a fixed number of arguments** (5, 1 [5]), **MAX or MIN of
  arguments of different classes** (1 [1]) and **SEARCH VARYING an item that is neither an index
  nor an integer** (1 [1]): negative tests of one conformance suite alone. cobc 3.2 refuses `E(ALL)`
  as an argument ("syntax error, unexpected ALL"), and takes the other two.
- **PROCEDURE DIVISION RETURNING an item outside the LINKAGE SECTION**: no program in either
  corpus but those with OMITTED, and cobc refuses it ("RETURNING item is not defined in LINKAGE
  SECTION").

`CALL ... RETURNING OMITTED`, `NOTHING` or `NULL`, which these refusals do not include, was refused
before them (`OMITTED is not defined`), and stays so: of the 237 files in the two corpora that hold
RETURNING OMITTED, NOTHING or NULL, nearly all call C libraries or are GnuCOBOL's tests.
