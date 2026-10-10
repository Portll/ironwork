# Compliance level: `--compliance strict|extended|relaxed|loose`

ironwork's target is IBM Enterprise COBOL for z/OS, and by default it refuses what Enterprise
COBOL refuses. Real programs are also written for Micro Focus and GnuCOBOL. `--compliance
extended` accepts the extensions below, each with a warning that names it and where it is, and
compiles and runs them; anything else stays refused with the message strict gives.

| Level | What it does |
|---|---|
| `strict` (the default) | A construct Enterprise COBOL does not have is refused, and a form IBM's compiler flags gets IBM's message at IBM's severity |
| `extended` | The extensions below are read as Micro Focus and GnuCOBOL read them, each with a warning |
| `relaxed` | `extended`, and a PROCEDURE DIVISION sentence or statement extended refuses compiles as a hole that ends a run reaching it ([below](#relaxed)) |
| `loose` | `relaxed`, and a data record, file or report extended refuses is left out, its items' statements holes; a statement a later check refuses is a hole; an unread directive, a character outside COBOL's set and other forms are left out; severity E is a warning ([below](#loose)) |

The flag is `--compliance strict|extended` (or `--compliance=extended`) on `run`, `check`, `cics`,
`compile`, `job`, `fuzz` and `compare`. It is a compile option: `numeric::Options::compliance`,
carried in the load module's `OPTIONS` section (load-module.md §5.1), named among the options in
force in `--provenance` statements, and kept with its value in an evidence journal's `argv`. A
CALLed program, a class and a job step's program are read under the same level as the program
that names them, and `ironwork run` of a load module reads a source it CALLs under the level the
module's first program was compiled with.

Each warning is severity W, so a program that uses an extension and nothing else checks with
return code 4 and runs; `-warnings-block` refuses it, as it refuses any program with warnings.

## How DISPLAY shows numbers

The compliance level also chooses how DISPLAY shows a zoned, packed or binary item (operator
2026-10-08). `--numeric-display ibm|cobc-ibm-strict|cobc` chooses it outright, and a CBL or PROCESS
card naming DISPSIGN outranks both. With neither:

| Level and dialect | Form | `S9(3)V99` holding -12.5, `S9(3)` 12, `9(3)V9` 1.5, `9(3)PP` 12300 |
|---|---|---|
| strict | Enterprise COBOL's, the digits as stored, the last overpunched (Programming Guide, DISPSIGN) | `0125}` `01B` `0015` `123` |
| extended or relaxed | cobc's default dialect's and Micro Focus's: a sign first, or where a separate one is declared; the program's decimal point at the item's scale; PICTURE P positions as zeros | `-012.50` `+012` `001.5` `12300` |
| `--dialect gnucobol`, either level | cobc -std=ibm-strict's: an overpunched item's digits, then its sign | `01250-` `012+` `0015` `123` |

Packed and binary items show their PICTURE's digits under cobc's default form, and COMP-5,
BINARY-CHAR, COMP-X and PIC X(n) COMP-5 items every digit their bytes hold, with a sign first when
signed, as cobc shows them (C14). A zoned item holding a byte other than a digit shows its bytes.
Both executors and a load module (`DispSign` tags 2 and 3) give these forms, checked against cobc
3.2.

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
and the LIR carries its ACCEPT sources and the op DISPLAY UPON ARGUMENT-NUMBER becomes. The eleventh,
IWX0011-W, is the compiler's check on an EXEC SQL INTO list: the name it reads is a host variable as
any other. IWX0012-W to IWX0015-W and IWX0017-W are forms IBM's compiler flags itself:
strict gives a message where IBM's does, at the severity IBM's has, and extended reads each as Micro
Focus and GnuCOBOL do, into a program Enterprise COBOL could hold. A test runs a program using the
five on both executors and compares the runs, and another gives each one's message under each level.
The sixteenth, IWX0016-W, BINARY-CHAR, has no Enterprise COBOL form: it is a binary item of
one byte, which a module records as its `Native` (load-module.md §5.2), and a test runs a program
using it on both executors and compares the runs with cobc's.
IWX0018-W, a numeric argument to LOWER-CASE, REVERSE, TRIM or UPPER-CASE, is a form Enterprise COBOL
refuses: both executors evaluate the argument as the item reference-modified from its first
character (`compile::as_characters`), and a test runs it on both and gives its message under each
level. IWX0019-W, a table at level 01 or 77, is read before the layout into an unnamed record
holding the table one level down, which both executors run as any table. IWX0020-W, the screen, has
no Enterprise COBOL form: both executors write and read it through `rt::crt`, and the LIR carries
its ops `ScreenDisplay` and `ScreenAccept`. IWX0021-W, the environment, has none either: both
executors read and set the run unit's variables (`rt::environment`), and the LIR carries its
`Environment` op and the ACCEPT source ENVIRONMENT-VALUE. IWX0022-W, record locking, is read by the
parser and changes nothing either executor runs. IWX0023-W, INSPECT ... TRAILING, is a mode of
INSPECT's scan that both executors run (`rt::strings::inspect`, `InspectMode` tag 4). IWX0024-W,
CALL ... RETURNING OMITTED, NOTHING or NULL, is rewritten by the compiler into statements Enterprise
COBOL has, which both executors run as any others.

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
- from its first line, when read in fixed form a literal runs past column 72, where fixed form
  ends, on a line with no continuation after it: its author compiles it with `cobc -free`. No file
  that reads in fixed form holds such a line. The warning is at column 73 of the first line longer
  than 72 columns;
- from its first line, when read in fixed form it does not parse (with a tab as one column or at
  cobc's tab stops, IWX0058-W), or a line's word or literal runs on from column 72 into column 73,
  where fixed form cuts it, and read in free form it parses: its author compiles it with `cobc
  -free`. A fixed-form file rarely parses in free form, since its comment lines (`*` in column 7)
  and sequence numbers are program text there. The warning says which, at line 1 or at column 73
  of the cut line;
- from its first line, under `--source-format free`, as `cobc -free` reads every file. The warning is
  at line 1;
- when it is a COPY member copied from a free-form line: GnuCOBOL and Micro Focus carry the format
  into the member. Such a member has no warning of its own; its own directive or lines can still
  switch it.

The rules from the first line apply under `--source-format auto`, the default. `--source-format
fixed` keeps every file and member in fixed form unless a directive switches it, and `free` reads
every file in free form from its first line. Both are for `--compliance extended`; strict reads fixed
form alone and refuses the flag.

In free form there is no sequence area, indicator column or Area A: a line's text runs from column
1 to the end of the line, whatever its length. `*>` begins a comment anywhere outside a literal. A
`*` or `/` in column 1 makes the line a comment, and `D` followed by a space a debugging line, as in
Micro Focus's free format; GnuCOBOL refuses such lines unless they start `*>`, the only place the
two read free form differently. There are no continuation lines: a literal that reaches the end of
a line is refused, as in both compilers (use `&`, below). A comment-entry (AUTHOR. and the like)
ends with its line, as GnuCOBOL ends it. Since Area A does not exist, a paragraph header is a word,
or digits, that comes right after a separator period, is not a reserved word, and is followed by a
period: what the parser takes from Area A in fixed form.

The conditional compilation directives, `>>TURN`, `>>LISTING`, `>>PAGE` and `>>D` are read as
IWX0048-W to IWX0050-W say. Any other compiler directive (`$SET` with another directive, `>>` with
a word none of these is) is refused, naming the directive.

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

### IWX0005-W BINARY-SHORT, BINARY-LONG, BINARY-DOUBLE and GnuCOBOL's binary usages

`IWX0005-W the COBOL 2002 binary usage (Micro Focus and GnuCOBOL; not Enterprise COBOL's):
BINARY-LONG is read as PIC S9(9) COMP-5`, at the usage word.

`[USAGE [IS]] BINARY-SHORT`, `BINARY-LONG` and `BINARY-DOUBLE`, each `SIGNED` (the default) or
`UNSIGNED`, are `PIC S9(4)`, `S9(9)` and `S9(18)` `COMP-5`, or the same PICTURE without `S`:
two, four and eight bytes of binary whose value is limited by the bytes, not the PICTURE. These are
the equivalences GnuCOBOL documents, and the sizes and ranges Micro Focus gives. The item then
behaves as such a COMP-5 item does under Enterprise COBOL: a DISPLAY of it shows the digits and
sign as Enterprise COBOL shows a COMP-5 item's, where GnuCOBOL shows a separate sign and one more
digit for BINARY-LONG.

GnuCOBOL's own binary usages are read the same way, with `GnuCOBOL's binary usage (not Enterprise
COBOL's)` in the warning: `BINARY-LONG-LONG [SIGNED|UNSIGNED]` as BINARY-DOUBLE; `SIGNED-SHORT` and
`UNSIGNED-SHORT` as `PIC S9(4)` and `9(4)` `COMP-5`; `SIGNED-INT` and `UNSIGNED-INT` as `S9(9)` and
`9(9)`; `SIGNED-LONG` and `UNSIGNED-LONG` as `S9(18)` and `9(18)`. These take no SIGNED or UNSIGNED
after them. `BINARY-INT [SIGNED|UNSIGNED]` is BINARY-LONG, as cobc's default dialect reads it. They are cobc 3.2's sizes, two, four and eight bytes, and its results: MOVE 70000 to a
SIGNED-SHORT gives 4464, and MOVE -1 to an UNSIGNED-INT gives 1. `BINARY-C-LONG`, whose size is the
C compiler's `long`, stays refused.

`BINARY-CHAR` is one byte, not a COMP-5 PICTURE: IWX0016, below.

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
never opens a host file a program names (C360). CLOSE closes the DD the OPEN found. A file SORT or
MERGE reads or writes takes its DD name from the item when the SORT or MERGE opens it, and FASTSRT
never gives its I/O to DFSORT, which finds a data set by its DD; an SD's ASSIGN names nothing a run
opens, and its item is not read (C500).

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

### IWX0011-W an INTO name without its colon

`IWX0011-W an INTO name written without its colon (Db2 13 for z/OS requires the colon before every
host variable): CSR-ENTITY is read as a host variable`, at the name.

Db2 13 for z/OS says every reference to a host variable is preceded by a colon, and that its
precompiler issues an error for a missing one, or reads the name as an unqualified column name where
a column name can stand (SQL Reference, References to host variables, db2z_refs2hostvars). An INTO
list is no such place, and in Db2 13 a name there without a colon writes a global variable, SQL
variable or SQL parameter target (SELECT INTO, db2z_sql_selectinto). Programs in the corpus write
host variables so (`FETCH C INTO CSR-ENTITY, CSR-PROJ-ID`), and under extended an INTO name without
its colon is the host variable of that name, in a SELECT INTO, FETCH or rowset FETCH. Strict refuses
it (S), as Db2's precompiler does (assumption SQ7).

### IWX0012-W a numeric literal as a numeric-edited item's VALUE

`IWX0012-W a numeric literal as a numeric-edited item's VALUE (Micro Focus and GnuCOBOL; Enterprise
COBOL takes an alphanumeric literal in edited form): AMOUNT starts as the literal moved to it`, at
the item's level-number.

Enterprise COBOL's VALUE clause for a numeric-edited item of USAGE DISPLAY "must be an alphanumeric
literal or a figurative constant", and for one of USAGE NATIONAL a national or alphanumeric literal
or a figurative constant, the editing characters written into the literal (Language Reference
SC27-8713-03, pp. 245-247). The rule is the one for alphanumeric items, for which IBM documents
IGYGR1080-S, "A "VALUE" clause literal was not compatible with the data category of the subject data
item. The "VALUE" clause was discarded." (Migration Guide, COBOL source code differences in
Enterprise COBOL 5 and 6), and which OS/VS COBOL alone relaxed (Migration Guide, VALUE clause
condition names). Strict refuses `PIC ZZ9.99 VALUE 12.5` with `IWC0292-S VALUE of AMOUNT: a numeric
literal, where a numeric-edited item's VALUE is an alphanumeric literal or a figurative constant
written in edited form; --compliance extended edits the number into it`.

Micro Focus takes a numeric literal there, and "the value contained in the item will be the same as
if the numeric literal were moved to the numeric edited item" (Visual COBOL Language Reference, The
VALUE Clause). cobc 3.2 does the same under `-std=default`, and under `-std=mf` with a warning, and
both give ` 12.50`; extended does the same. A figurative constant, ZERO among them, is no numeric
literal and is taken under either level.

### IWX0013-W END-DISPLAY and END-ACCEPT

`IWX0013-W END-DISPLAY (Micro Focus and GnuCOBOL; Enterprise COBOL does not reserve the word): it
ends the DISPLAY statement`, at the word, and the same for END-ACCEPT after ACCEPT.

Enterprise COBOL's DISPLAY and ACCEPT have no scope terminator (Language Reference SC27-8713-03,
pp. 307, 333). Its reserved words list END-DISPLAY and END-ACCEPT only under "Potential reserved
words", which "might be reserved in a future release" and are flagged with an I-level message where
a program uses them as names (Appendix E, pp. 761, 766). IBM's compiler therefore reads `DISPLAY X
END-DISPLAY` as displaying a data item named END-DISPLAY, and refuses it as undefined (IGYPS2121-S,
"... was not defined as a data-name. The statement was discarded."), and END-ACCEPT after a complete
ACCEPT as a word no statement takes. Strict refuses either word after its statement with
`IWS0097-S END-DISPLAY: Micro Focus's and GnuCOBOL's scope terminator, a word Enterprise COBOL does
not reserve; --compliance extended reads it`. A program that names a data item END-DISPLAY and
displays it there, which Enterprise COBOL compiles, is refused the same way.

Micro Focus and GnuCOBOL reserve both words and read each as its statement's terminator, which
matters where ON EXCEPTION precedes it; extended does the same.

### IWX0014-W VALUES outside a level-88 entry

`IWX0014-W VALUES outside a level-88 entry (Micro Focus; Enterprise COBOL writes VALUE there): it is
read as VALUE`, at VALUES.

Enterprise COBOL writes VALUES ARE only in format 2 of the VALUE clause, a condition-name's entry;
format 1, a data item's initial value, is VALUE IS (Language Reference SC27-8713-03, pp. 245, 248).
No IBM listing of VALUES in format 1 has been found, so the severity is the syntax's: a word a data
description entry does not take is S, as in IGYDS1089-S, "... was invalid. Scanning was resumed at
the next area "A" item, level-number, or the start of the next clause.". Strict refuses `01 FLAG PIC
X VALUES 'Y'.` with `IWS0098-S VALUES in a level-01 entry: Enterprise COBOL writes VALUES only in a
level-88 entry, and VALUE in any other; --compliance extended reads it as VALUE`.

Micro Focus documents that "VALUES ARE can be used with Format 1", marked as an OS/VS COBOL form
(Visual COBOL Language Reference, The VALUE Clause), and cobc 3.2 reads it as VALUE under `-std=mf`
with a warning; cobc refuses it under `-std=default` and `-std=ibm-strict`, giving it no other
meaning. Extended reads it as VALUE. A list of values after VALUES, the 2002 standard's table form,
stays refused under either level.

### IWX0015-W user-defined words of more than 30 characters

`IWX0015-W a user-defined word of more than 30 characters (Micro Focus and GnuCOBOL; Enterprise COBOL
reads its first 30): NAME is read whole`, at the word.

"The maximum length of a user-defined word is 30 bytes" (Language Reference SC27-8713-03, p. 13).
IBM's compiler gives IGYDS0023-E, "The COBOL word starting in column 23 contained more than 30
characters. The word was truncated to 30 characters." (IBM APAR PI67249, for two data-names of 31
characters), and under its default NOCOMPILE(S) the program compiles and runs with return code 8. Strict does the same: `IWS0099-E NAME: a user-defined word has at most 30
characters, and this one has 32; it is read as its first 30, ...` at each such word, which is read
as its first 30 characters, so `check` returns 8 and `run` runs the program. A name in EXEC SQL or
EXEC CICS is cut the same way, and two names alike in their first 30 characters are one name,
refused as ambiguous where a reference cannot tell them apart.

Micro Focus and GnuCOBOL (63 characters under `-std=default` and `-std=mf`) read the word whole;
extended does the same.

### IWX0017-W a statement in Area A

`IWX0017-W a statement in Area A (Micro Focus and GnuCOBOL; Enterprise COBOL puts statements in Area
B): DISPLAY is read as though it began in Area B`, at each word of the statement that begins there.

Area A holds division, section and paragraph headers, the level indicators, level-numbers 01 and
77, DECLARATIVES and the end markers; "entries, sentences, statements, and clauses" begin in Area B
(Language Reference SC27-8713-03, pp. 55-57). IBM's compiler gives IGYPS0009-E, ""DISPLAY" should
not begin in area "A". It was processed as if found in area "B"." (a listing quoted in Tek-Tips
thread 1544944), and compiles the program with return code 8. A reserved word in Area A before a
period, as in `GOBACK.` or `EXIT.`, names no paragraph and is read as its statement. Strict gives
`IWS0100-E DISPLAY begins in Area A, where Enterprise COBOL puts no statement: it is read as though it
began in Area B` at each such word, so `check` returns 8 and `run` runs the program. Micro Focus and
GnuCOBOL check no area (`areacheck: no` under cobc's `-std=default` and `-std=mf`); extended reads the
statement the same way, with the warning.

In the census samples below, measured with the five on acc3642, 116 of the 1,161 programs strict
compiled cleanly in the 3185-repository sample (34 repositories) and 106 of 1,425 in the
500-repository sample (14) now compile at return code 8, most of them whole programs indented from
column 8; they still run. Reading `GOBACK.` and `EXIT.` in Area A as statements compiles 7 programs
under extended that it refused before.

### IWX0018-W a numeric argument to LOWER-CASE, REVERSE, TRIM or UPPER-CASE

`IWX0018-W a numeric argument to FUNCTION TRIM (GnuCOBOL; Enterprise COBOL takes an alphabetic,
alphanumeric or national one): N's digits are read as its characters`, for an unsigned integer
item of USAGE DISPLAY.

The argument of each of the four "must be" of class alphabetic, alphanumeric, national or UTF-8
(Language Reference SC27-8713-03, pp. 589, 627, 657, 663), and a numeric item, a numeric literal or
an arithmetic expression is none of them. Strict refuses it with `IWC0297-S FUNCTION TRIM: N is
numeric, where TRIM takes an alphabetic, alphanumeric or national argument`. A numeric-edited item
is of class alphanumeric and is taken, as is a reference-modified numeric item (p. 75).

cobc 3.2 accepts each of the four with a numeric argument, under `-std=ibm-strict` too, and reads
it as characters in a way that depends on the item: an unsigned integer DISPLAY item as its digits
(`005` for `PIC 9(3) VALUE 5`), a signed one with a leading sign (`-005`), one with decimal places
with a point (`01.5`), and a packed item as neither its bytes nor its value. Only the first is the
item's own characters, the ones reference modification gives, so extended reads that one, with the
warning, on both executors as cobc does, and refuses the rest with IWC0297-S. Before this,
ironwork compiled all of them silently and ended the run at the function with an IRONWORK abend,
and an IF comparing the result with an alphanumeric literal was refused with IWC0143-S, which named
the function as numeric.

In the census samples (3,000 programs of each corpus, seed 1, at b51ae02 and with this change on
it), strict compiles the same 1,027 and 1,314 programs: no program either sample compiles holds the
form. Extended compiles the same 1,382 and 1,485. Three of them, copies of one training program in
three repositories, now carry IWX0018-W, and they run to the end on both executors with cobc's
output, where they ended with the abend before.

### What the four change

The census samples of the next section, 3,000 programs from each corpus with seed 1, run with
`ironwork check` at ef82e8c, and with this change on that commit. Strict compiled 1,161
programs of the 3185-repository sample (365 repositories) and 1,425 of the 500-repository sample (86);
with the four it compiles 1,139 (362) and 1,418 (86). No program is refused by more than one:

| Form | Strict's message | First sample: compiled, now not (repositories) | Second sample | Programs holding it, first and second |
|---|---|---|---|---|
| A numeric VALUE for a numeric-edited item | IWC0292-S | 1 (1) | 0 | 2, 0 |
| END-DISPLAY or END-ACCEPT | IWS0097-S | 15 (3), 11 of them GnuCOBOL tests copied into one repository | 5 (2) | 21, 17 |
| VALUES outside a level-88 entry | IWS0098-S | 4 (2) | 1 (1) | 4, 1 |
| A user-defined word of more than 30 characters | IWS0099-E | 2 (2), at return code 8, and they still run | 1 (1), the same | 3, 2 |

Under extended both samples compile what they compiled before, 1,376 and 1,484 programs, and no
more: a program using one of the four that compiled before now carries its warning, and one refused
before stops on something else. The NIST CCVS85 routines give the same class and first message
under strict before and after (383 clean of 458): COBOL-85 has none of the four.

### IWX0016-W BINARY-CHAR

`IWX0016-W BINARY-CHAR (Micro Focus and GnuCOBOL; Enterprise COBOL's binary items are two, four or
eight bytes): U is one byte of binary, 0 to 255`, at the data entry.

`[USAGE [IS]] BINARY-CHAR [SIGNED|UNSIGNED]` is one byte of binary holding -128 to 127, or 0 to 255
when UNSIGNED; SIGNED is the default. It takes no PICTURE (IWC0294 refuses one). A value it receives
keeps its low-order byte, whatever TRUNC says, as a COMP-5 item keeps its bytes: MOVE 300 to an
unsigned one gives 44, ADD 1 to one holding 255 gives 0, and ADD 50 to a signed one holding 100
gives -106, as cobc 3.2 gives them. Where a PICTURE's digits are asked for it has three integer
digits: DISPLAY shows three, `00J` for -1 under `--dialect ibm` as a COMP-5 item's whole value is
shown, `-001` under `--dialect gnucobol` as cobc shows it. SYNCHRONIZED leaves it where it is, a
JSON or XML number holds three digits, and EXEC SQL refuses it as a host variable, as a binary item
of one byte has no SQL type. Strict refuses it (IWC0293). Assumption C460.

In the 3,000-program census of v0.7.0 BINARY-CHAR was the first refusal of 48 programs in 12
repositories under extended, cobc accepting all 48, 27 of them programs of ACAS, a GnuCOBOL
accounting system; every one of the 48 then stops at a further extension (DISPLAY and ACCEPT AT 17,
LOCK MODE 8, SET ENVIRONMENT 4, among others), so BINARY-CHAR alone compiles none of them.

### IWX0019-W OCCURS at level 01 or 77

`IWX0019-W OCCURS at level 01 (Micro Focus and GnuCOBOL; Enterprise COBOL takes OCCURS only at
levels 02 to 49): T is read as a table in a record of its own`, at the entry.

An entry of level 01 or 77 in WORKING-STORAGE or LOCAL-STORAGE with OCCURS is read as an unnamed 01
record holding the table at level 02, each entry under it a level lower too. Its occurrences follow
one another as a level-02 table's do, the records before and after it untouched, as cobc 3.2 lays
it out. A reference to it takes a subscript, and a VALUE on it gives every occurrence that value.
An entry with REDEFINES, EXTERNAL or GLOBAL, one in the LINKAGE SECTION or a file's records, and one
with a level-49 entry under it stay refused with IWC0027, as Enterprise COBOL refuses OCCURS at
levels 01 and 77. Assumption C461.

In the census, after BINARY-CHAR, it was the first refusal of 23 distinct sources in 7 directories
that cobc compiles, all in WORKING-STORAGE at level 01.

### IWX0020-W DISPLAY and ACCEPT on the screen

`IWX0020-W DISPLAY on the screen (Micro Focus and GnuCOBOL; Enterprise COBOL has none): at the line
and column AT gives`, at the first screen phrase.

A DISPLAY or ACCEPT that names a place on the screen, clears part of it, or gives a field's
behaviour writes or reads Micro Focus's and GnuCOBOL's screen instead of a device:

    DISPLAY item... [AT {LLCC | LLLCCC | item} | LINE [NUMBER] n | COL[UMN] [NUMBER] n | POSITION n]...
            [UPON CRT] [WITH] [BLANK {SCREEN | LINE} | ERASE [EOL | EOS | SCREEN | LINE] | attribute]...
    ACCEPT item [FROM CRT] [AT ... | LINE ... | COL ...] [WITH] [UPDATE | SECURE | attribute]...
            [ON EXCEPTION ...] [NOT ON EXCEPTION ...] [END-ACCEPT]

`DISPLAY ... UPON CRT` and `ACCEPT ... FROM CRT` with no other phrase use the cursor. The
attributes (HIGHLIGHT, LOWLIGHT, REVERSE-VIDEO, BLINK, UNDERLINE, BELL, BEEP, AUTO, FULL, REQUIRED,
PROMPT, FOREGROUND-COLOR n, BACKGROUND-COLOR n, TIMEOUT n and the like) are read and kept by name, and
change nothing a run shows.

The run unit has one screen of 24 lines of 80 characters, blank at the start. A DISPLAY writes its
items one after another from its position, after clearing what BLANK or ERASE names, and leaves the
cursor after them; text past a line's end goes on at the next line, and past the last line is lost.
An ACCEPT's field is as long as DISPLAY shows the item and holds its value with UPDATE, spaces
without. `ironwork run --screens path` gives the operator a script, as for a CICS task: `string
text` typed at the cursor, `type ROW COL text`, `eof ROW COL`, `cursor ROW COL`, `home`, `tab` and a
key (ENTER, PF1-PF24, PA1-PA3, CLEAR) that ends the ACCEPT. Text typed replaces the field from where
it is typed to its end. The item takes the field as typed if it is alphanumeric, as NUMVAL reads it
if numeric (zero where it is no number), and the field then shows the item as stored; SECURE shows
each character as `*`. A key other than ENTER takes ON EXCEPTION. An ACCEPT the script has no key
left for ends the run (`ACCEPT: the screen has no more operator input`). After the run, `ironwork
run` prints each screen an ACCEPT showed, before the operator typed, and the last screen.

This is a clean model of the screen, not cobc's (operator ruling 2026-10-07): cobc 3.2 positions
only a DISPLAY's last item, overtypes an UPDATE field from the left, reads `3.5` typed into
`S9(3)V99` as 35.00, and writes a plain DISPLAY on the screen once it is used. Assumption C462.
Strict refuses each screen phrase with IWC0298.

The SCREEN SECTION describes screens to DISPLAY and ACCEPT by name. Each entry is

    level [name | FILLER] [LINE [NUMBER] [IS] [PLUS | MINUS] n] [COL[UMN] [NUMBER] [IS] [PLUS | MINUS] n]
          [VALUE literal | PIC[TURE] picture [FROM item | TO item | USING item]]
          [BLANK {SCREEN | LINE} | ERASE [EOL | EOS] | SECURE | attribute]...

An entry is at the LINE and COLUMN it gives, PLUS and MINUS counting from the entry before; without
LINE it is on the line of the entry before, and without COLUMN at column 1 when LINE is given, else in
the column after the entry before. A field is as long as its VALUE or its PICTURE, and a group entry
places what follows it. `DISPLAY name` writes each entry the screen holds, in order, at its place: a
VALUE as written, a FROM or USING field as its PICTURE edits the item (the compiler gives each field
with a PICTURE an item of that PICTURE in WORKING-STORAGE and MOVEs to it), a TO field as spaces or
zeros. The item has the field's name where no data item or other entry has it, so a field with no
FROM, TO or USING shows what the program last moved to it by name, as cobc 3.2 shows it.
`ACCEPT name` displays the screen and reads its TO and USING fields in one turn of the operator, `tab`
moving to the next field, each target taking its field as a positioned ACCEPT's does; a USING field
starts holding the item's value. `DISPLAY name AT LLCC` or `LINE n COL n` moves the whole screen from
line 1, column 1 to there. Assumption C463. The warning names the screen:
`IWX0020-W ACCEPT ORDER-SCREEN on the screen (Micro Focus and GnuCOBOL; Enterprise COBOL has none): at
the lines and columns its SCREEN SECTION entries give`. Strict refuses the SCREEN SECTION with
IWC0298. OCCURS in it, a LINE or COLUMN that is not an integer literal, and a screen displayed at a
place an item gives are refused with IWR0057.

In the census, after the level-01 tables, positioned DISPLAY and ACCEPT were the first refusal of
19 distinct sources that cobc compiles, and the SCREEN SECTION of 17.

### IWX0021-W the environment

`IWX0021-W ACCEPT ... FROM ENVIRONMENT (Micro Focus and GnuCOBOL; Enterprise COBOL reads and sets no
environment variable): V receives the value of the environment variable named, or spaces and the
exception when it is not set`, at the statement.

    ACCEPT item FROM ENVIRONMENT {literal | item} [ON EXCEPTION ...] [NOT ON EXCEPTION ...]
    ACCEPT item FROM ENVIRONMENT-VALUE [ON EXCEPTION ...] [NOT ON EXCEPTION ...]
    DISPLAY {literal | item} UPON ENVIRONMENT-NAME
    DISPLAY {literal | item} UPON ENVIRONMENT-VALUE
    SET ENVIRONMENT {literal | item} TO {literal | item}

DISPLAY UPON ENVIRONMENT-NAME names the variable the next ENVIRONMENT-VALUE reads or sets, and
DISPLAY UPON ENVIRONMENT-VALUE sets it; a name and a value lose their trailing spaces. ACCEPT ... FROM
ENVIRONMENT name is DISPLAY name UPON ENVIRONMENT-NAME then ACCEPT ... FROM ENVIRONMENT-VALUE, and
SET ENVIRONMENT name TO value the two DISPLAYs, as GnuCOBOL documents them. A variable that is not
set gives the receiver spaces and takes ON EXCEPTION; cobc 3.2 then runs NOT ON EXCEPTION too, which
ironwork does not. The variables are those `ironwork run --env NAME=VALUE` gives and those the run
sets, never the process's own, so a run gives what its command line says (assumption C464). Each
value ACCEPT reads is the run's input, as a SYSIN record is. Strict refuses each as before: IWS0055,
IWS0060, IWS0061, and IWC0073 for DISPLAY UPON.

### IWX0022-W record locking

`IWX0022-W LOCK MODE MANUAL (Micro Focus and GnuCOBOL; Enterprise COBOL has no record locks of its
own): the run unit is the file's only user, so nothing it locks waits and the phrase changes
nothing`, at the phrase.

    SELECT ... [LOCK MODE [IS] {MANUAL | AUTOMATIC | EXCLUSIVE} [WITH LOCK ON [MULTIPLE] {RECORD | RECORDS}]]
               [SHARING WITH {ALL OTHER | NO OTHER | READ ONLY}]
    OPEN mode file WITH LOCK
    READ file ... [WITH {LOCK | NO LOCK | KEPT LOCK | WAIT} | IGNORING LOCK] ...
    WRITE record ... [WITH [NO] LOCK] ...
    REWRITE record ... [WITH [NO] LOCK] ...
    UNLOCK file [RECORD | RECORDS]

A run unit is the only user of its files, so no lock it takes is ever met by another and none of its
I/O waits or fails for one: the phrases are read, each with the warning, and change nothing;
UNLOCK does nothing. cobc 3.2 refuses a READ lock phrase on a file of LOCK MODE AUTOMATIC, which
ironwork reads. Assumption C465. Strict refuses LOCK MODE and SHARING as before (IWR0008), the
statement phrases with IWC0299, and UNLOCK is no statement there.

### IWX0023-W INSPECT ... TRAILING

`IWX0023-W INSPECT ... TRAILING (GnuCOBOL; Enterprise COBOL has ALL, LEADING, FIRST and CHARACTERS):
the occurrences that run on to the end of the phrase's region`, at TRAILING.

    INSPECT item TALLYING counter FOR TRAILING operand [BEFORE | AFTER INITIAL operand]...
    INSPECT item REPLACING TRAILING operand BY operand [BEFORE | AFTER INITIAL operand]...

A TRAILING phrase takes the occurrences of its operand that run on to the end of its region, after
its BEFORE and AFTER bounds: `INSPECT T TALLYING N FOR TRAILING SPACES` counts the spaces at the end
of T. In INSPECT's left-to-right scan it matches at a position when the data, as it was before the
scan, holds its operand from there to the region's end, over and over; among the other phrases it
fits as LEADING does, the first phrase that matches at a position taking it. cobc 3.2 gives the
same results on the probes checked. Assumption C466. Strict refuses TRAILING as before.

### IWX0024-W CALL ... RETURNING OMITTED, NOTHING or NULL

`IWX0024-W CALL ... RETURNING OMITTED (GnuCOBOL; Enterprise COBOL's RETURNING names a data item): the
CALL leaves the caller's RETURN-CODE as it was`, at the word after RETURNING.

    CALL program [USING ...] RETURNING {OMITTED | NOTHING | NULL} [ON EXCEPTION ...] [NOT ON EXCEPTION ...]

cobc 3.2 reads the three alike: the CALL returns nothing, and the caller's RETURN-CODE is what it was
before the CALL, whatever the called program set; a plain CALL gives the caller the called program's
RETURN-CODE. In ironwork RETURN-CODE is one location the whole run unit shares, so the compiler
rewrites each such CALL before the layout is built (`compile::omitted`): a MOVE of RETURN-CODE to an
item no source can name, `RETURN-CODE SAVED`, PIC S9(4) BINARY as RETURN-CODE is, goes before the
CALL, and a MOVE of it back to RETURN-CODE first in NOT ON EXCEPTION. A CALL that fails leaves
RETURN-CODE as it was in both compilers. The item is in WORKING-STORAGE, or in LOCAL-STORAGE in a
RECURSIVE program, where each level of a recursion keeps its own. NOTHING is not reserved in
Enterprise COBOL: in a program that declares an item named NOTHING, `RETURNING NOTHING` names it.
OMITTED and NULL are reserved, and are read as the form wherever they are written, with no
qualifier, subscript or reference modification. Strict refuses the three with IWC0300.

Probes under `/tmp/callret2` gave the same output under `ironwork run --compliance extended`, with
and without `--vm`, as under `cobc -x`, byte for byte: each word after `MOVE 7 TO RETURN-CODE` and
a CALL of a program that sets 3; the form in IF and inline PERFORM; ON EXCEPTION and NOT ON
EXCEPTION bodies; a CALL of a missing program with ON EXCEPTION; and a called program using the
form in its own CALL. They differ in a RECURSIVE program that CALLs itself: cobc keeps a
RETURN-CODE for each program, which the levels of a recursion share, so the caller's RETURN-CODE
is what the deeper level last set; in ironwork each level gets back its own.

### IWX0025-W COMP-X and IWX0026-W PIC X(n) COMP-5

`IWX0025-W COMP-X (Micro Focus; Enterprise COBOL's binary items are two, four or eight bytes): N7 is
3 bytes of binary, 0 to 16777215, shown in 7 digits` and `IWX0026-W PIC X(n) COMP-5 (Micro Focus
and GnuCOBOL; Enterprise COBOL's COMP-5 takes a numeric PICTURE): F2 is 2 bytes of binary, 0 to
65535`, at the data entry.

`PIC 9(n) COMP-X` is binary in the fewest bytes that hold n digits: one byte for 1 or 2 digits, two
for 3 or 4, three for 5 to 7, four for 8 or 9, and so on to eight bytes for 17 or 18, a signed
PICTURE taking the same bytes, as cobc 3.2 gives them. `PIC X(n) COMP-X` is n bytes of unsigned
binary, one to eight, with the 2, 4, 7, 9, 12, 14, 16 or 19 digit positions Micro Focus documents.
A value either receives keeps its low-order bytes whatever TRUNC says, and ON SIZE ERROR is taken
where a result does not fit them: `PIC 99 COMP-X` holds 250, and ADD 10 to it is a size error.
DISPLAY and a MOVE to an alphanumeric item show the item's digits, the value's low-order ones, so
that 250 in `PIC 99 COMP-X` shows as `50`, as cobc gives it. Assumption C467.

`PIC X(n) COMP-5` is n bytes of unsigned binary, one to eight, limited by its bytes as any COMP-5
item is. DISPLAY shows its whole value in the 3, 5, 8, 10, 13, 15, 17 or 20 digits its bytes hold,
and a MOVE to an alphanumeric item its COMP-X digit positions, as cobc does. Its bytes are
big-endian, z/Architecture's order; cobc on x86-64 writes them little-endian. Assumption C468.

SYNCHRONIZED aligns either on two or four bytes for an item of that size, on four for eight bytes,
and leaves the others where they are. An alphanumeric PICTURE of more than eight bytes is refused
(IWC0303); Micro Focus takes up to sixteen. Strict refuses COMP-X (IWC0301) and an alphanumeric
PICTURE with COMP-5 (IWC0302).

### IWX0027-W FLOAT-SHORT and FLOAT-LONG

`IWX0027-W FLOAT-SHORT (GnuCOBOL and Micro Focus; Enterprise COBOL writes COMP-1 and COMP-2): it is
read as COMP-1, IBM's hexadecimal floating point`, at the usage word.

`[USAGE [IS]] FLOAT-SHORT` is read as COMP-1 and `FLOAT-LONG` as COMP-2, by the operator's ruling of
2026-10-07 that they behave as IBM's do. cobc and Micro Focus hold them in IEEE binary floating
point, so a value with no exact form, such as 0.001, can differ in its last digits: as COMP-1 it
holds 0.00099999993. DISPLAY shows them as it shows any COMP-1 or COMP-2 item, `-.99999993E-03`
where cobc shows `-0.001`. The other floating-point usages, FLOAT-DECIMAL-16 and -34,
FLOAT-BINARY-32, -64 and -128, FLOAT-EXTENDED and z390's FLOAT-HEX-30, stay refused.
Assumption C469. Strict refuses FLOAT-SHORT and FLOAT-LONG (IWS0101).

### IWX0066-W FLOAT-HEX-7 and FLOAT-HEX-15

`IWX0066-W FLOAT-HEX-7 (z390's zCOBOL; Enterprise COBOL writes COMP-1): it is read as COMP-1, the
same hexadecimal floating point`, at the usage word.

z390's zCOBOL names IBM's short and long hexadecimal floating point FLOAT-HEX-7 and FLOAT-HEX-15,
and they are read as COMP-1 and COMP-2 with no change of value. cobc refuses both. FLOAT-HEX-30,
z390's extended hexadecimal floating point, has no Enterprise COBOL usage and stays refused, as do
z390's FLOAT-BINARY and FLOAT-DECIMAL forms. Strict refuses FLOAT-HEX-7 and -15 (IWS0107).

### IWX0028-W PERFORM ... FOREVER

`IWX0028-W PERFORM ... FOREVER (Micro Focus and GnuCOBOL; Enterprise COBOL has no FOREVER phrase): it
repeats until EXIT PERFORM, GO TO, GOBACK or STOP RUN leaves it`, at FOREVER.

    PERFORM FOREVER statements END-PERFORM
    PERFORM procedure-name [THRU procedure-name] FOREVER

FOREVER takes the place of TIMES, UNTIL or VARYING: the body, or the procedures, run again and
again, and EXIT PERFORM leaves an inline one as it leaves any inline PERFORM. ACAS reads a file
this way, `PERFORM FOREVER`, `READ ... AT END EXIT PERFORM`. cobc 3.2 gives the same output for
both forms on the probes checked. In Enterprise COBOL FOREVER is not reserved, and `PERFORM
FOREVER` performs a procedure of that name; strict reads it so. Under extended a program that both
performs FOREVER and names a paragraph or section FOREVER is refused (IWC0304), as the two readings
differ there.

### IWX0029-W ACCEPT ... FROM LINES and FROM COLUMNS or COLS

`IWX0029-W ACCEPT ... FROM LINES (GnuCOBOL; Enterprise COBOL has no screen): the screen's 24 lines`,
at LINES or COLUMNS.

The item receives the size of the one screen of 24 lines of 80 characters that positioned DISPLAY
and ACCEPT use (assumption C462), as a MOVE of 24 or 80 gives it. cobc asks curses for the
terminal's size, which the clean screen model does not have; ACAS reads it to lay out its
screens, taking at least 24. Strict refuses both (IWS0060), as before.

### IWX0030-W WRITE ... BEFORE ADVANCING on a line-sequential file

`IWX0030-W WRITE ... BEFORE ADVANCING on the line-sequential file P (GnuCOBOL and Micro Focus;
Enterprise COBOL allows only AFTER there): the line, then the lines or page it names`, at the WRITE.

Enterprise COBOL takes only AFTER ADVANCING for a LINE SEQUENTIAL file. Under extended BEFORE
ADVANCING n LINES and BEFORE ADVANCING PAGE are taken too, and the file is written as a text DD shows
any print file: the line, then n line feeds, or a line feed and a form feed for PAGE. A line written
BEFORE straight after one written AFTER prints over it, shown with a carriage return, as a printer
prints it; cobc writes the two lines one after the other on one line. ACAS writes its first page's
headings BEFORE 1 and the others AFTER. ADVANCING a mnemonic-name stays refused there (IWC0145), as
does BEFORE under strict.

### IWX0031-W FUNCTION MODULE-CALLER-ID

`IWX0031-W FUNCTION MODULE-CALLER-ID (GnuCOBOL; Enterprise COBOL has no such function): the
PROGRAM-ID of the program that called this one, empty in the main program`, at the function.

The value is alphanumeric, as long as the caller's name: SUBP when SUBP called the running program,
and an empty value, of length zero, in the main program, as cobc 3.2 gives it. The caller is the
program whose CALL, user-defined function reference or INVOKE entered the latest activation of the
running one. ACAS compares it with "ACAS" to learn whether its menu program called it. Strict
refuses it (IWC0305).

### IWX0032-W A level-66 entry inside its record

`IWX0032-W a level-66 entry before the end of its record (GnuCOBOL's IBM and Micro Focus dialects;
Enterprise COBOL writes a record's RENAMES entries after its last entry): DW-BP-YYMM is read as
following DW's last entry`, at the level-66 entry.

Enterprise COBOL's RENAMES entries for a record "must immediately follow the last data description
entry of that record" (Language Reference, RENAMES clause). No IBM listing of the message its
compiler gives for one inside the record has been found, in IBM's Messages and Codes (SC27-4648-02
lists no IGYDS texts) or in the corpus's listings, so strict keeps the layout's refusal, IWC0035-S.
cobc 3.2 takes it with a warning under `-std=ibm`, `ibm-strict` and `mf`, and refuses it under
`-std=default`. Under extended a run of level-66 entries followed by an entry of levels 02 to 49 is
read as following the record's last entry: the entries after it continue the record, and each
RENAMES covers what it names, as cobc lays it out. One followed by a level-88 entry stays refused
(IWC0028). Assumption C471.

In the 500-repository corpus it is the first refusal of 137 of the 194 batch programs of
Pavansai0522_CABS-MAINFRAME-DEMO-TIER5, through one shared copybook.

### IWX0033-W STOP RUN and GOBACK with RETURNING or GIVING

`IWX0033-W STOP RUN RETURNING (GnuCOBOL and Micro Focus; Enterprise COBOL moves the value to
RETURN-CODE first): the value is moved to RETURN-CODE, then STOP RUN ends the program`, at RETURNING
or GIVING.

    STOP RUN {RETURNING | GIVING} identifier-or-literal
    GOBACK {RETURNING | GIVING} identifier-or-literal

The statement runs as a MOVE of the value to RETURN-CODE and then the STOP RUN or GOBACK, as cobc
3.2 runs it, and a caller receives the value as its RETURN-CODE. RETURN-CODE stays Enterprise
COBOL's binary halfword, where cobc's holds nine digits, so a value beyond four digits differs; the
exit status is ironwork's band of RETURN-CODE, where cobc exits with the value modulo 256. ACAS ends
two programs with `GOBACK RETURNING 4`. Assumption C480. Strict refuses the phrase (IWC0306).

### IWX0034-W name [NOT] OMITTED

`IWX0034-W P1 OMITTED (GnuCOBOL and Micro Focus; Enterprise COBOL writes ADDRESS OF P1 = NULL): it is
read as ADDRESS OF P1 = NULL, true when the caller passed OMITTED or no argument there`, at OMITTED.

    identifier IS [NOT] OMITTED

The condition is ADDRESS OF identifier = NULL, Enterprise COBOL's way of asking whether a parameter
came: true when the CALL passed OMITTED in its place or passed fewer arguments, and cobc 3.2 gives
the same answers for both. cobcurses tests its optional parameters this way. Strict refuses it
(IWC0307).

### IWX0035-W ANY LENGTH

`IWX0035-W ANY LENGTH (GnuCOBOL and Micro Focus; Enterprise COBOL's parameters have the length their
entries give): L is as long as the argument each CALL passes for it`, at the entry.

    01 identifier PIC X ANY LENGTH.
    01 identifier ANY LENGTH.

An alphanumeric 01 or 77 item of the LINKAGE SECTION named in PROCEDURE DIVISION USING takes the
length of the argument in its position on each CALL or function reference: a data item's length,
or the bytes BY CONTENT or BY VALUE gives, and 0 for one omitted or not passed. It is read as a group
holding that many single characters, a table whose count is set as the procedure starts, so it
moves, compares, displays and is reference-modified as an alphanumeric group of the argument's
length, and FUNCTION LENGTH gives that length; cobc 3.2 gives the same output on the probes
checked. CobolCraft passes every packet buffer this way. Refused with IWR0076: an ANY LENGTH entry
anywhere else, one that is not alphanumeric, one with OCCURS or REDEFINES, a RETURNING item written
ANY LENGTH, one an ENTRY statement names, and a function argument that is neither a data item nor
an alphanumeric or hexadecimal literal, which would have no length of its own; such a literal is
passed in a temporary of its bytes, as long as they are. Assumption C481. Strict refuses the clause
(IWC0308).

### IWX0036-W START KEY <, NOT > and <=

`IWX0036-W START KEY NOT > or <= (Micro Focus and GnuCOBOL; Enterprise COBOL's START takes =, >, NOT <
or >=): the file is positioned at the last record whose key is NOT > or <= the value, which READ NEXT
or READ PREVIOUS reads first`, at the START.

START ... KEY IS LESS THAN, <, NOT GREATER THAN, NOT > and <= position an indexed or relative file
at the last record whose key, compared over the value's length, is less than the value, or not
greater. The READ NEXT or READ PREVIOUS that follows reads that record first and goes on in its
direction, as cobc 3.2 does with its BDB handler: COBSOFT reads its tables backwards this way, START
NOT GREATER and then READ PREVIOUS. No such record gives INVALID KEY, status 23. Assumption C483.
Strict keeps IWC0076.

### IWX0037-W A file description with no FILE SECTION header

`IWX0037-W a file description with no FILE SECTION header (Micro Focus and GnuCOBOL; Enterprise COBOL
writes FILE SECTION first): it is read as though FILE SECTION came first`, at the FD or SD.

An FD or SD that opens the DATA DIVISION, or follows another section, is read as the start of the
FILE SECTION, as cobc 3.2 reads it with a warning. Strict refuses it (IWC0310).

### IWX0038-W PERFORM UNTIL EXIT

`IWX0038-W PERFORM UNTIL EXIT (GnuCOBOL and Micro Focus; Enterprise COBOL has no such condition): it
repeats until EXIT PERFORM, GO TO, GOBACK or STOP RUN leaves it`, at UNTIL.

UNTIL EXIT is PERFORM ... FOREVER (IWX0028) under another name, and runs the same way. Strict
refuses it (IWC0309).

### IWX0039-W ASSIGN TO DISK

`IWX0039-W ASSIGN TO DISK (Micro Focus and GnuCOBOL; Enterprise COBOL's ASSIGN names a DD): DISK is
the device, and what follows names the file`, at DISK.

    SELECT file ASSIGN TO DISK {data-name | literal}

DISK is read as the device, and what follows as the file's name: a literal names the DD, and a name
is a data item each OPEN takes the DD name from, as ASSIGN TO an item does (IWX0007). Where neither
the program nor a program containing it declares the item, it is declared for the program (IWX0103);
COBSOFT builds a path in it before each OPEN. Assumption C482. Strict reads DISK as the
assignment-name, as Enterprise COBOL does, and the name after it has no effect.

### IWX0040-W Split keys

`IWX0040-W KEY IS F00100-CHAVE = ... (Micro Focus; Enterprise COBOL's key is one data item): the key
joins 4 items of the record, in the order written`, at the equals sign.

    RECORD KEY IS key-name = data-name-1 data-name-2 ...
    ALTERNATE RECORD KEY IS key-name = data-name-1 data-name-2 ... [WITH DUPLICATES]

The key is the items' bytes joined in the order written, wherever they lie in the record and in
whatever order: records are held, read in sequence and found by that joined value, as cobc 3.2
holds them with its BDB handler. START and READ ... KEY IS name the key by key-name, and take its
value from the items as the record area holds them. Each item must be in the file's records
(IWC0090). COBSOFT keys all 26 of its files this way. A load module holding a split key is format
1.2, the pieces following the `LIR` section's records (load-module.md §8.1). Assumption C484.
Strict refuses the form (IWC0311).

### IWX0041-W Periods after a period

`IWX0041-W periods after a period (GnuCOBOL and Micro Focus; Enterprise COBOL ends a sentence with
one): the periods after the first are ignored`, at the first.

`DISPLAY 'BAD'..` and `VALUE 0..` end the sentence or entry once, as cobc 3.2 reads them with a
warning; the z390 test programs copied into the bug datasets end many sentences so. Strict refuses
the second period (IWS0026).

### IWX0042-W FUNCTION STORED-CHAR-LENGTH

`IWX0042-W FUNCTION STORED-CHAR-LENGTH (GnuCOBOL; Enterprise COBOL has no such function): the
argument's length in characters without its trailing spaces`, at the function.

An integer: the characters of an alphanumeric argument up to its last that is not a space, 0 when
all are spaces, as cobc 3.2 gives it. A national argument counts its characters up to the last that
is not a national space, where cobc 3.2, whose national handling it calls unfinished, counts more.
CobolCraft measures names and channels with it. Strict refuses it (IWC0312).

### IWX0043-W DELETE FILE

`IWX0043-W DELETE FILE (Micro Focus and GnuCOBOL; Enterprise COBOL has no such statement): each closed
file's data set is removed`, at FILE.

    DELETE FILE file-name ...

Each file must be closed. The data set of the DD it is assigned to, by its ASSIGN or by its ASSIGN
item's value at the statement, is removed, and FILE STATUS is 00, as cobc 3.2 removes the file. An
open file gives 41, and a missing DD or data set 35, each taking the file's error path as any I/O
status does. Tangram removes its work files this way. Assumption C485. Strict refuses the statement
(IWC0313).

### IWX0044-W PROGRAM-POINTER

`IWX0044-W PROGRAM-POINTER (GnuCOBOL and Micro Focus; Enterprise COBOL writes PROCEDURE-POINTER): it is
read as PROCEDURE-POINTER, set by SET ... TO ENTRY and called by CALL`, at the word.

The item is Enterprise COBOL's PROCEDURE-POINTER: SET ... TO ENTRY sets it, CALL calls the program
it holds, and it compares and moves as one, as cobc 3.2 gives the results. CobolCraft keeps its
callbacks in such items. Assumption C486. Strict refuses it (IWC0314).

### IWX0046-W BASED

`IWX0046-W BASED (GnuCOBOL and Micro Focus; Enterprise COBOL describes such an item in the LINKAGE
SECTION): REC has no storage until SET ADDRESS OF gives it some`, at the entry.

A WORKING-STORAGE or LOCAL-STORAGE 01 or 77 entry written BASED is read, with what is subordinate
to it, as a record of the LINKAGE SECTION no USING names: ADDRESS OF it is NULL until SET ADDRESS
OF gives it storage, as cobc 3.2 gives it. BASED on another level is refused (IWR0077). Assumption
C486. Strict refuses the clause (IWC0316).

### IWX0047-W FREE of a record

`IWX0047-W FREE REC (GnuCOBOL; Enterprise COBOL frees through a pointer): the storage ADDRESS OF REC
names is released, and the record has none`, at the record's name.

Enterprise COBOL's FREE names pointers (C487). GnuCOBOL's also takes a LINKAGE or BASED record:
the storage ALLOCATE gave it is released and ADDRESS OF it becomes NULL, as cobc 3.2 does. Strict
refuses it (IWC0317).

### IWX0048-W Conditional compilation

`IWX0048-W >>IF (COBOL 2002 and GnuCOBOL; Enterprise COBOL 6.3 has it too): the lines it chooses are
compiled and the others are not`, at each >>DEFINE and >>IF on a compiled line.

    >>DEFINE name [AS] {literal | OFF | PARAMETER} [OVERRIDE]
    >>IF condition ... [>>ELIF condition ...] [>>ELSE ...] >>END-IF

The lines an >>IF, >>ELIF or >>ELSE chooses are read and the others are not, nested as written. A
condition is `name [IS] [NOT] DEFINED` (or `SET`, which cobc reads the same way; no name is
predefined, P64 included), or a defined name compared with a literal or another
defined name by =, <, >, <=, >= or their words, as numbers where both are numbers, and terms may
be joined by AND and OR, read left to right; a comparison of a name not defined is false.
PARAMETER, a value from the compiler's options, leaves the name undefined, as ironwork takes no
such option. cobc 3.2 gives the same lines on the probes checked. An >>IF with no >>END-IF, an
>>ELSE, >>ELIF or >>END-IF with no >>IF, and a condition of another form are refused (IWC0318).
GnuCOBOL's >>DEFINE CONSTANT, a constant for the program text, is IWX0067. >>EVALUATE and the
directives no section here names stay refused (IWS0094). Enterprise COBOL 6.3 has >>DEFINE, >>IF, >>ELSE and
>>END-IF (Language Reference, Conditional compilation), and strict reads them the same way with no
message; >>ELIF, which IBM has not, is read under extended alone. IBM's arithmetic expressions in
>>DEFINE, its predefined compilation variables and its DEFINE compiler option are not read yet.

### IWX0049-W >>TURN, >>LISTING and >>PAGE

`IWX0049-W >>TURN (COBOL 2002 and GnuCOBOL; Enterprise COBOL has no such directive): it is read and
has no effect`, at the directive.

>>TURN switches exception checks on and off, and >>LISTING and >>PAGE control the listing; ironwork
reads them and changes nothing. cobc 3.2 ignores a word after >>D that is no directive, as
>>DMOVE, as an invalid directive, and ironwork reads it the same way, with this warning.

### IWX0050-W >>D

`IWX0050-W >>D (GnuCOBOL; Enterprise COBOL marks a debugging line with D in column 7): the line is a
debugging line, compiled only WITH DEBUGGING MODE`, at >>D.

The text after `>>D ` is a debugging line in fixed or free form, read only where SOURCE-COMPUTER
says WITH DEBUGGING MODE, as a D in column 7 is.

### IWX0051-W An inline PERFORM with AFTER phrases

`IWX0051-W an inline PERFORM with AFTER phrases (GnuCOBOL and Micro Focus; Enterprise COBOL takes them
only when PERFORM names a procedure): the body runs for each combination, the last AFTER varying
fastest`, at the PERFORM.

The inline body runs as a performed procedure would under the same VARYING and AFTER phrases, as
cobc 3.2 runs it. Strict keeps IWS0058.

### IWX0052-W INITIALISE

`IWX0052-W INITIALISE (Micro Focus and GnuCOBOL; Enterprise COBOL spells it INITIALIZE): it is read
as INITIALIZE`, at the verb. ACAS writes it so. Under strict INITIALISE is no verb.

### IWX0053-W FUNCTION CONCATENATE

`IWX0053-W FUNCTION CONCATENATE (GnuCOBOL; Enterprise COBOL has no such function): its arguments'
characters joined, a number's as its digits`, at the function.

An alphanumeric value of the arguments' characters in order, a numeric argument giving its digits
unsigned, as a MOVE to an alphanumeric item shows them, as cobc 3.2 gives it. Strict refuses it
(IWC0319).

### IWX0054-W COPY name..

`IWX0054-W COPY CSTMT.. (GnuCOBOL and Micro Focus; Enterprise COBOL reads the name as CSTMT.): the
member is CSTMT, and the periods after it end the statement`, at COPY.

Enterprise COBOL takes the second period as the one that ends the statement, so the name is `X.`
(C88), and strict keeps IWS0004. cobc 3.2 copies member X and leaves the extra period in the text,
which ends an empty sentence in the PROCEDURE DIVISION and is refused elsewhere; ironwork copies
member X wherever the statement stands.

### IWX0055-W PROCEDURE DIVISION CHAINING

`IWX0055-W PROCEDURE DIVISION CHAINING (GnuCOBOL and Micro Focus; Enterprise COBOL's main program
takes its PARM through USING): each item takes the run's argument in its position, its bytes
left-justified, where one is given`, at CHAINING.

The job step's PARM program arguments, split at blanks, stand for the command line, as they do for
ACCEPT ... FROM COMMAND-LINE. Each word's bytes go into its item as the procedure starts,
left-justified, padded with spaces and cut to the item's length whatever its class, as cobc 3.2
copies them: `123` into a PIC 9 item gives 1. An item with no word keeps its value, and so does
every item of a program another one called. ACAS's posting programs take their date range this
way. Assumption C488. Strict refuses it (IWC0320).

### IWX0056-W DISPLAY UPON SYSERR or STDERR

`IWX0056-W DISPLAY UPON SYSERR (GnuCOBOL and Micro Focus; Enterprise COBOL has no such device): the line
is written to the run's standard error`, at the DISPLAY. GnuCOBOL's STDERR is the same device, with
its own name in the warning.

The line goes to the run's standard error, with or without NO ADVANCING, as cobc 3.2 writes it; it
reaches the VM as `Op::DisplayError`, tag 40. Strict keeps IWC0073.

### IWX0057-W COB-CRT-STATUS

`IWX0057-W COB-CRT-STATUS (GnuCOBOL's special register; Enterprise COBOL has no screen ACCEPT): it holds
the key that ended the last screen ACCEPT, as GnuCOBOL's screenio.cpy numbers the keys`, at the
first reference.

A program that names COB-CRT-STATUS and declares no item of that name gets `77 COB-CRT-STATUS PIC
9(4) VALUE 0`. Each screen ACCEPT then sets it to the code of the key that ended it, before its ON
EXCEPTION or NOT ON EXCEPTION phrase runs: the screen script's ENTER is 0, PF1 to PF24 the
function keys 1001 to 1024, CLEAR Esc 2005, PA1 and PA2 page up and page down 2001 and 2002, and PA3
print 2006, GnuCOBOL's screenio.cpy codes, which ACAS compares it with. Assumption C489.

### IWX0058-W Tab stops

`IWX0058-W tab stops (GnuCOBOL and Micro Focus; Enterprise COBOL source holds no tab)`, followed
by why, at the file's first tab.

A tab is one column, as under strict. Under `--source-format auto`, a file that does not parse that
way and holds a tab is read again with each tab reaching the next column after a multiple of 8, as
cobc places it (its default `-ftab-width`), so `<tab><tab>IDENTIFICATION DIVISION.` starts in column
17; where it parses then, it is read so. Files indented with narrower tabs, such as CardDemo's
CUSTREC copybook, which runs past column 72 at cobc's stops and which cobc refuses, keep reading a
tab as one column. A COPY member is read with a tab as one column. Enterprise COBOL's source comes
from fixed-length records with no tab characters.

### IWX0060-W PICTUREs of 19 to 31 digits

`IWX0060-W PICTURE {picture} (GnuCOBOL and Micro Focus; Enterprise COBOL's ARITH(COMPAT) allows 18
digits): the program is compiled with ARITH(EXTEND), which allows 31`, at the first such PICTURE.

GnuCOBOL and Micro Focus accept numeric PICTUREs of up to 38 digits; Enterprise COBOL takes up to
18 under ARITH(COMPAT), its default, and 31 under ARITH(EXTEND). Under extended, a program with a
numeric or numeric-edited PICTURE of 19 to 31 digit positions, and no CBL or PROCESS card naming
ARITH, is compiled as though a card said ARITH(EXTEND): its literals may have 31 digits too, and its
arithmetic has ARITH(EXTEND)'s intermediate precision, not cobc's (assumption C490). A card that
names ARITH keeps its choice, and a PICTURE of more than 31 digits stays refused.

### IWX0061-W A paragraph header in Area B

`IWX0061-W {name}. in Area B (Micro Focus and GnuCOBOL; Enterprise COBOL puts a paragraph header in
Area A): a name and a period after a separator period is read as a paragraph header`, at the name.

In fixed form, a user-defined word or digits that follows a separator period and is followed by a
period is a paragraph header wherever it begins, as cobc reads it: `           100-STEP.` names a
paragraph. Enterprise COBOL takes a header from Area A alone, and in Area B such a sentence is no
statement, so no program Enterprise COBOL compiles reads differently. In free form every such word is
a header already (IWX0001).

### IWX0062-W FUNCTION SUBSTITUTE and SUBSTITUTE-CASE

`IWX0062-W FUNCTION {name} (GnuCOBOL; Enterprise COBOL has no such function): each text found is
replaced, the pairs tried in order at each position`, at the function.

`FUNCTION SUBSTITUTE(text from-1 to-1 [from-2 to-2]...)` scans `text` from the left; at each position
the first pair whose `from` is there puts its `to` in its place and the scan goes on after it, the
replacement not scanned again. Otherwise the character is kept. The value is alphanumeric, its
length what the replacements make it. `SUBSTITUTE-CASE` compares without regard to case. A number
argument stands for its digits, unsigned, as in CONCATENATE. Both executors give cobc 3.2's output.
One difference: cobc reads a zero-length literal `''` as a space, with a warning, where ironwork
reads it as no characters, so `SUBSTITUTE(T 'x' '')` deletes each `x` where cobc puts a space. Strict
refuses either function with IWC0321-S.

### IWX0063-W A zero-length literal

`IWX0063-W '' (GnuCOBOL and Micro Focus; Enterprise COBOL's literals hold at least one character):
a space is assumed, as cobc assumes it`, at the literal.

cobc reads `''` or `""` as one space, with a warning, in every dialect. Enterprise COBOL's
alphanumeric literals are 1 to 160 bytes (Language Reference, Basic alphanumeric literals), so strict
gives `IWS0106-E` and assumes the space too: the program compiles at return code 8, as Enterprise
COBOL compiles past an E-level message. `--empty-literal empty` reads it as no characters instead,
the reading ironwork gave before, under either level and with the same message.

### IWX0067-W A directive's constant

`IWX0067-W {directive} (GnuCOBOL and Micro Focus; Enterprise COBOL has no compile-time constant):
{name} stands for {literal} in the program, as a level-78 constant does`, at the directive.

    >>DEFINE CONSTANT name [AS] literal [OVERRIDE]
    $SET CONSTANT name literal

From the line after the directive, the name in the program text stands for the literal, as a
level-78 constant's name does: a quoted literal's characters, or a number as written. A second
definition of the name is ignored, as cobc ignores it, unless it says OVERRIDE. `$SET` starts in
column 1 or 7, or wherever it begins a line, and `>>` anywhere first on a line, including after a
sequence number in fixed form, as cobc reads `000100 >>DEFINE ...`. A directive with no name or no
literal is refused (IWC0318).

### IWX0068-W ALPHABET IS ASCII

`IWX0068-W ALPHABET {name} IS ASCII (GnuCOBOL and Micro Focus; Enterprise COBOL writes STANDARD-1):
it is read as STANDARD-1, the ASCII collating sequence`, at ASCII.

### IWX0069-W REPOSITORY PROGRAM

`IWX0069-W REPOSITORY PROGRAM {name} (COBOL 2014 and GnuCOBOL; Enterprise COBOL names classes and
functions there): CALL {name}, unquoted, calls the program so named`, at PROGRAM.

A REPOSITORY entry `PROGRAM name [AS literal]` lets the program write `CALL name` with the name
unquoted; that CALL calls the program the literal names, or `NAME` with no AS phrase, as a quoted
name does. The entry outranks a data item of the same name. An empty REPOSITORY paragraph is read
too.

### IWX0070-W The header's last parameter in Area A

`IWX0070-W {name} in Area A (Micro Focus and GnuCOBOL; Enterprise COBOL puts the header's parameters
in Area B): it is read as the PROCEDURE DIVISION header's last parameter`, at the name.

A PROCEDURE DIVISION USING list continued on lines of its own may put its last name in Area A,
followed by the header's period. Under strict that reads as a paragraph header and the header has
no period; under extended, after at least one parameter, it is the last parameter.

### IWX0071-W A DATA DIVISION section with no division header

`IWX0071-W {section} SECTION with no DATA DIVISION header (Micro Focus and GnuCOBOL; Enterprise COBOL
writes DATA DIVISION first): it is read as though DATA DIVISION came first`, at the section header.

WORKING-STORAGE, LOCAL-STORAGE, LINKAGE or FILE SECTION after the IDENTIFICATION or ENVIRONMENT
DIVISION with no DATA DIVISION header begins the DATA DIVISION.

### IWX0072-W OPTIONAL parameters

`IWX0072-W OPTIONAL (GnuCOBOL and Micro Focus; Enterprise COBOL has no optional parameter): it is
read and has no effect, an argument the caller leaves out reading as OMITTED, as one does under
Enterprise COBOL`, at OPTIONAL.

`PROCEDURE DIVISION USING a OPTIONAL b` reads `b` as any parameter. A CALL that passes fewer
arguments leaves `b` with no storage, and `IF b OMITTED` is true for it (IWX0034).

### IWX0073-W FUNCTION-ID with no IDENTIFICATION DIVISION header

`IWX0073-W FUNCTION-ID with no IDENTIFICATION DIVISION header before it (COBOL 2002 and GnuCOBOL;
Enterprise COBOL requires the header): the function reads as though IDENTIFICATION DIVISION. came
before it`, at FUNCTION-ID. The same as IWX0006 for PROGRAM-ID.

### IWX0074-W A constant entry AS an arithmetic expression

`IWX0074-W constant {name} AS an arithmetic expression (COBOL 2002, Micro Focus and GnuCOBOL;
Enterprise COBOL has no constant entry): it stands for {value}, the expression's value truncated to
an integer, as the standard gives it`, at the level number.

`01 name CONSTANT AS expression` or `78 name VALUE expression`, where the expression joins numeric
literals and numeric constants defined before it with +, -, *, / and parentheses: ironwork works it
exactly, as fractions, and truncates the result toward zero (ISO 2002 7.3.6.3). Any other operand is
refused, naming what the expression may hold.

### IWX0076-W CALL STATIC

`IWX0076-W CALL STATIC (GnuCOBOL; Enterprise COBOL's CALL names no call convention): it is read as
CALL '{literal}', which calls the same program`, at STATIC.

GnuCOBOL's `CALL STATIC literal` links the program statically. Which program runs is the same as
for `CALL literal`, so ironwork reads the CALL without the word. Strict refuses it with IWC0326-S,
as cobc -std=ibm-strict does.

### IWX0077-W CALL ... GIVING

`IWX0077-W CALL ... GIVING (Micro Focus and GnuCOBOL; Enterprise COBOL writes RETURNING): it is read
as RETURNING`, at GIVING. Strict refuses it with IWC0327-S. A COBOL program called with no
PROCEDURE DIVISION RETURNING phrase gives the item its RETURN-CODE under extended, as cobc does
(C491, [targets.md](targets.md)).

### IWX0078-W NUMBER-OF-CALL-PARAMETERS

`IWX0078-W NUMBER-OF-CALL-PARAMETERS (GnuCOBOL's special register; Enterprise COBOL has none): it
holds the number of arguments the program was called with, or of the run's arguments in the main
program`, where a program names it and declares no item of the name.

The register is `PIC S9(9) BINARY`, shown in nine digits as cobc shows it. It takes its value where
the program is entered, first in the PROCEDURE DIVISION and after each ENTRY: the number of
arguments the CALL passed, OMITTED ones included, or in the main program the number of words of the
run's PARM, as cobc counts a main program's command-line arguments. Func 92 `CALL PARAMETERS` reads
it on both executors.

### IWX0079-W An inline PERFORM a period ends

`IWX0079-W an inline PERFORM ended by a period (Micro Focus; Enterprise COBOL ends it with
END-PERFORM): the period ends the PERFORM and the sentence, as cobc -std=mf reads it`, at the period.

Micro Focus lets a separator period end every open statement, an inline PERFORM included; cobc
accepts it under -std=mf and refuses it under its default dialect. Strict refuses it with IWS0108-S.

### IWX0080-W A MOVE of a procedure-pointer

`IWX0080-W MOVE {name} of a procedure-pointer or function-pointer (GnuCOBOL and Micro Focus;
Enterprise COBOL writes SET): it is read as SET ... TO {name}`, at the MOVE.

A MOVE whose sender and receivers are all procedure-pointers or function-pointers (including
PROGRAM-POINTER, IWX0044) is the SET Enterprise COBOL writes for it. Strict keeps IWC0133.

### IWX0090-W EXIT FUNCTION

`IWX0090-W EXIT FUNCTION (COBOL 2002 and GnuCOBOL; Enterprise COBOL ends a user-defined function with
GOBACK): it ends the function as GOBACK does`, at the statement.

Inside a user-defined function's definition, EXIT FUNCTION returns to the invoking statement with the
RETURNING item as it stands, as GOBACK does there. Outside one, and under strict, it stays refused
with IWS0056-S.

### IWX0091-W A function defined after the program

`IWX0091-W FUNCTION {name}, defined after this program in its source (GnuCOBOL; Enterprise COBOL
takes a user-defined function only defined or prototyped before the program): it is invoked as that
definition describes it`, at the invocation.

A program whose REPOSITORY paragraph names a function the same source defines or prototypes after it
invokes it as that definition gives its parameters and RETURNING item, which cobc allows.

### IWX0092-W A function defined in another source

`IWX0092-W FUNCTION {name}, defined in {file} (GnuCOBOL; Enterprise COBOL takes a user-defined
function only defined or prototyped before the program): it is invoked as that definition describes
it`, at the invocation.

GnuCOBOL compiles an invocation of a function its REPOSITORY paragraph names without its definition,
and takes the function's result as the called module returns it. ironwork needs the RETURNING item's
class and size when it compiles the program, so it reads the definition from the program sources
(`.cbl`, `.cob`) of the directories the compile reads copybooks from: the program's own directory
first, then each `-I` directory, each one's files in name order. The first definition or prototype of
that name is the function's interface. A function no such source defines stays refused with
IWC0106-S. A run finds the function's code as for any user-defined function: in the module, or by
its name in a program library.

### IWX0093-W A period missing before PROCEDURE DIVISION

`IWX0093-W a period was required before PROCEDURE DIVISION (GnuCOBOL warns and assumes it;
Enterprise COBOL assumes it at E): one was assumed`, at PROCEDURE.

The last data description entry before PROCEDURE DIVISION ends there, as both compilers end it.
Enterprise COBOL gives IGYDS1082-E, which ends a compile at return code 8, and strict keeps that as
IWS0105-E; cobc 3.2 warns ("optional period used") and compiles on, as extended does.

### IWX0094-W Letters outside COBOL's set in a word

`IWX0094-W the word {word} has letters outside COBOL's character set (GnuCOBOL reads a user-defined
word's letters as UTF-8; Enterprise COBOL writes such a word in DBCS characters): it is read as a
user-defined word`, at the word's first use.

A word may hold letters beyond ASCII, such as ñ and é from a Latin-1 or UTF-8 source, or kanji and
kana, as GnuCOBOL's sources hold them in paragraph and data names. Such a letter is not folded to a
capital, as cobc 3.2 does not fold it, and a word matches only a word spelled with the same letters.
Enterprise COBOL's DBCS user-defined words are written between shift-out and shift-in in a DBCS
source, which ironwork does not read; strict refuses a letter beyond Latin-1 with IWS0021-S, and
takes a Latin-1 letter with IWS0025-E.

### IWX0095-W A period missing before a level number

`IWX0095-W a period was required before level number {level} (cobc under -std=ibm warns and assumes
it; Enterprise COBOL assumes it at E): one was assumed`, at the level number.

A data description entry ends where the next entry's level number begins, as both compilers end it.
Strict gives IWS0120-E, as Enterprise COBOL gives IGYDS1082-E, and compiles on at return code 8.

### IWX0096-W A VALUE with no PICTURE

`IWX0096-W {name} has no PICTURE (GnuCOBOL takes one from its VALUE; Enterprise COBOL requires one):
it is read as PIC X({length})`, at the entry.

An elementary item with no PICTURE or USAGE whose VALUE is an alphanumeric or hexadecimal literal is
alphanumeric and as long as the literal, a figurative constant's one character, and ALL a literal's
the literal's own length, as cobc 3.2 lays them out. A numeric literal, which cobc refuses there too,
leaves the entry refused with IWC0235-S.

### IWX0097-W A word GnuCOBOL reserves only in its own clause

`IWX0097-W {name} is a reserved word of Enterprise COBOL's that GnuCOBOL reserves only in its own
clause: it names {what}`, at the name.

Twenty of Enterprise COBOL's reserved words are words cobc 3.2 reserves only where its own clause
takes them (`cobc --list-reserved` marks them context sensitive): APPLY, AUTHOR, BYTE-LENGTH, COBOL,
DATE-COMPILED, DATE-WRITTEN, EVERY, INSTALLATION, MEMORY, MODULES, PASSWORD, PROCESSING, RECURSIVE,
RERUN, SECURITY, TAPE, TITLE, UTF-8, WRITE-ONLY and XML-SCHEMA. Under extended such a word may name
a data item, a file, a paragraph or a section. Any other reserved word stays refused with IWC0188-S,
as strict refuses them all.

### IWX0098-W ACCEPT FROM ESCAPE KEY

`IWX0098-W ACCEPT ... FROM ESCAPE KEY (GnuCOBOL; Enterprise COBOL has no screen ACCEPT): the code of
the key that ended the last screen ACCEPT, as COB-CRT-STATUS holds it`, at ESCAPE.

The receiver takes the code COB-CRT-STATUS takes after a screen ACCEPT (assumption C489): 0 for
ENTER, 1000 plus n for PFn, 2005 for CLEAR, as GnuCOBOL's `cob_accept_escape_key` gives the last
ACCEPT's status. Strict refuses it with IWS0060-S.

### IWX0099-W INSPECT of a literal

`IWX0099-W INSPECT {literal} (GnuCOBOL; Enterprise COBOL inspects a data item): its characters are
tallied`, at INSPECT.

INSPECT TALLYING counts in an alphanumeric or hexadecimal literal as in a data item holding it.
REPLACING and CONVERTING would store into the literal and are refused with IWS0121-S.

### IWX0100-W PROGRAM-ID.NAME

`IWX0100-W a period with no space after it ends {paragraph} (GnuCOBOL; Enterprise COBOL follows a
separator period with a space): it is read as a separator period`, at the period.

A period written right before the program's name, as in `PROGRAM-ID.SALESREPORT.`, ends
PROGRAM-ID, FUNCTION-ID, CLASS-ID or METHOD-ID as a separator period would. Elsewhere a period needs
the space after it, as under strict.

### IWX0101-W FUNCTION CONCAT

`IWX0101-W FUNCTION CONCAT (GnuCOBOL's name for CONCATENATE; Enterprise COBOL has neither): it is
read as FUNCTION CONCATENATE`, at the name.

cobc 3.2 knows the function by both names; FUNCTION ALL INTRINSIC lets it be invoked as CONCAT
without the word FUNCTION too. It is CONCATENATE (IWX0053) in every other respect.

### IWX0102-W DISPLAY and ACCEPT (line, column)

`IWX0102-W {verb} (line, column) (Micro Focus's and RM/COBOL's; Enterprise COBOL has no screen): it
is read as {verb} ... AT LINE line COLUMN column`, at the parenthesis.

`DISPLAY (23, 40) 'TEXT'` and `ACCEPT (23, 57) ITEM` position the screen I/O as AT LINE 23
COLUMN 40 does (IWX0020), with any further screen phrases after it.

### IWX0103-W An undeclared ASSIGN name

`IWX0103-W {name} is not declared (GnuCOBOL and Micro Focus declare the name ASSIGN gives a file;
Enterprise COBOL's assignment-name is never a data item): it is read as 01 {name} PIC X(4095) VALUE
'{spelled}'`, at the name.

The name after ASSIGN TO DISK (IWX0039), or after a plain ASSIGN TO where the PROCEDURE DIVISION
uses it and no file has it, is declared for the program in WORKING-STORAGE where neither the program
nor a program containing it declares it: 4,095 bytes holding the name as the source spells it, as
cobc 3.2 declares it. Each OPEN takes the DD name from the item (IWX0007), so a program that sets
the item first, as `ACCEPT name FROM ARGUMENT-VALUE` does, opens the DD it names, and one that does
not opens the DD the name itself gives. Assumption C482. Strict refuses a name no entry declares
with IWC0001-S, and a plain ASSIGN name nothing uses stays the DD name under extended too.

### IWX0104-W ASSIGN names a later constant

`IWX0104-W ASSIGN {name}: a constant defined after the ASSIGN (GnuCOBOL; Enterprise COBOL has no
constant entry): the file's name is its value`, at the name.

A level-78 or CONSTANT entry (IWX0002) stands for its value only after the entry, but the name
ASSIGN gives a file, after TO, USING or DYNAMIC, may be a constant the DATA DIVISION defines later,
as cobc 3.2 reads it: `SELECT F ASSIGN DYNAMIC LIST-NAME` with `78 LIST-NAME VALUE
"./feeds/list.dat"` is read as `ASSIGN "./feeds/list.dat"`, a literal naming the DD.

### IWX0105-W A key named in and outside the file's records

`IWX0105-W {clause} {name} names an item in the records of {file} and another outside them
(GnuCOBOL; Enterprise COBOL requires the name qualified): it is read as {name} OF {record}`, at the
name.

Enterprise COBOL's RECORD KEY and ALTERNATE RECORD KEY name an item in the file's records and may be
qualified, so a name that WORKING-STORAGE or another file declares too is refused as ambiguous
(IWC0002-S). cobc 3.2 takes the item in the file's records, under every dialect; extended does the
same where exactly one of the file's records holds the name. ACAS's `COPY "plwspay.cob" REPLACING
Pay-Record BY WS-Pay-Record` leaves PAY-KEY in WORKING-STORAGE and in the payments file's record.

### IWX0106-W ACCEPT OMITTED

`IWX0106-W ACCEPT OMITTED (GnuCOBOL; Enterprise COBOL has no screen ACCEPT): a screen ACCEPT with no
field, which waits for the next key`, at OMITTED.

`ACCEPT OMITTED`, with any screen phrases and ON EXCEPTION, waits for the operator's next key as a
screen ACCEPT (IWX0020) with no field would: the `--screens` script plays to its next key, text
typed before it goes nowhere, and COB-CRT-STATUS (IWX0057) takes the key. Strict refuses it, as
OMITTED names no data item (IWC0001-S).

### IWX0107-W A sort key outside the sort file's records

`IWX0107-W {name} is not in the records of {file} (GnuCOBOL; Enterprise COBOL takes a {verb} key
from the file's records): each record is keyed on bytes {from} to {to}, where {name} lies in its own
record`, at the key.

Enterprise COBOL's SORT and MERGE keys are items of the SD's records (IWC0211-S). cobc 3.2 takes a
key from another record, a WORKING-STORAGE record or another file's, at the offset and length it has
in its own record, applied to each record sorted, under every dialect; extended does the same where
those bytes fall within the SD's record area, the key read as its own item's class and usage.

### IWX0108-W DISPLAY with several lists of items

`IWX0108-W DISPLAY with items after its screen phrases (GnuCOBOL; Enterprise COBOL has no screen):
each list of items is displayed in turn, at the place its own phrases give or at the cursor`, at the
first item after the phrases.

    DISPLAY ' Operation: ' AT 0124 WITH FOREGROUND-COLOR 7  OPERATION AT 0136

is read as one positioned DISPLAY (IWX0020) for each list of items and the phrases after it, in
order, as cobc 3.2 shows them: a list with no AT of its own starts at the cursor, where the list
before it ended. Within a list the items go one after another from the list's AT, as a single
DISPLAY's do (C462), where cobc places only the last of them there.

### IWX0109-W SWITCH-n

`IWX0109-W SWITCH-{n} (GnuCOBOL's name for its switch {n}; Enterprise COBOL writes UPSI-{n}): it is
read as UPSI-{n}, which the UPSI run-time option sets`, at the name.

cobc 3.2 spells a SPECIAL-NAMES switch SWITCH-n under its default dialect and UPSI-n under
`-std=ibm`, and both are its run-time switch n. Extended reads SWITCH-0 to SWITCH-7 as UPSI-0 to
UPSI-7, with the mnemonic-name and the ON and OFF STATUS condition-names after it. SWITCH-8 to
SWITCH-36 have no UPSI switch and are refused with IWS0122-S. Strict, as Enterprise COBOL, knows
UPSI-0 to UPSI-7 only.

## Relaxed

`--compliance relaxed` (or `--compliance=relaxed`) is `extended` for every program it compiles, and
more programs compile: what extended refuses in the PROCEDURE DIVISION becomes a hole, with
`IWX0059-W {construct} (--compliance relaxed): {why}; it compiles as a hole, and a run that reaches
it ends with IWR0078`, where `{why}` is the message extended gives.

- A sentence that does not parse is a hole from its first token to its period, or to the next
  paragraph or section header. The statements before it and after its period compile as ever.
- A statement whose check gives a severe message (an undefined name, a GO TO of no paragraph, a
  function ironwork does not have) is a hole. The innermost such statement is the hole, so an IF
  whose THEN branch names no item keeps its condition and ELSE; the IF is a hole itself only where
  its own part is refused.
- A run that reaches a hole ends there with `ABEND IRONWORK: IWR0078-S {construct} was reached:
  under --compliance relaxed it compiled as a hole, since {why}`, exit status 244, on the
  interpreter and the VM alike; a load module carries the hole as an abend like any other. A run
  that never reaches one runs as it would under extended.

Relaxed changes nothing a program that compiles under extended does: such a program has no hole,
and check, run and the load module are the same. What it does not make holes of: the IDENTIFICATION,
ENVIRONMENT and DATA DIVISIONs (a refused data description, PICTURE or USAGE stays refused, with the
message extended gives), the PROCEDURE DIVISION header, and messages of severity E, which compile as
they do under every level (return code 8). The flag is kept with its value in an evidence journal's
`argv`; a load module records `extended`, so a source a module CALLs when it runs is read under
extended.

## Loose

`--compliance loose` (or `--compliance=loose`) is `relaxed`, and goes on where relaxed stops:

- A data record extended refuses, a level-01 or level-77 entry with its subordinates, is left out,
  with `IWX0064-W {record} (--compliance loose): {why}; it is left out, and a statement naming one
  of its items compiles as a hole`. That covers an entry that does not parse (TYPEDEF, a clause
  ironwork does not read) and one the compile refuses (a PICTURE of more than 31 digits, a reserved
  word as its name). A record of a file, or a refusal naming the file (`FD F: ...`), leaves the file
  out. Only the whole record goes: an item left out of a group would move the items after it.
- A statement naming an item that was left out then compiles as a hole, as relaxed compiles any
  statement that names no item (IWX0059-W), and a run that reaches it ends with IWR0078.
- In the program's own file, a directive ironwork does not read (`>>CALL-CONVENTION`,
  `>>COBOL-WORDS`), a character outside COBOL's set, or a stray period that stops the parse is left
  out, the directive's line or the one character, with `IWX0065-W`, and the source is read again.
- A statement a check across the program refuses, such as PERFORM ... THRU from a declarative
  procedure into the rest of the program (IWC0015), compiles as a hole, with IWX0059-W.
- A report whose description the compile refuses, such as a subscripted CONTROL (IWR0026), is left
  out with its name in the file's REPORT clause, with `IWX0065-W`; INITIATE, GENERATE and TERMINATE
  naming it or its groups then compile as holes.
- A PROCEDURE DIVISION USING name whose record was left out (ANY NUMERIC, say) keeps its
  parameter's place, so the arguments after it still reach their parameters, with `IWX0065-W`; a
  statement naming it compiles as a hole.
- The Communication feature: the COMMUNICATION SECTION's CD areas are declared as data, as under
  every level, and ENABLE, DISABLE, RECEIVE, SEND, PURGE and ACCEPT MESSAGE COUNT compile as holes,
  each refusal given as `IWX0065-W` or IWX0059-W.
- `USE FOR DEBUGGING ON ALL [REFERENCES OF] item`: the item is left out of the USE statement, with
  `IWX0065-W`, and the section runs for the procedures it names. ALPHABET name FOR NATIONAL, which
  ironwork has no collating sequence for, is left out the same way.
- A source with no IDENTIFICATION DIVISION or PROGRAM-ID is a program named after its file, as cobc
  names it under -std=mf and -std=ibm, with `IWX0075-W no IDENTIFICATION DIVISION or PROGRAM-ID
  (GnuCOBOL under -std=mf or -std=ibm assumes them; Enterprise COBOL requires them): the program is
  named {name}, after its file`. When it begins with statements, they are its PROCEDURE DIVISION.
- Messages of severity E are given as warnings, so a program whose worst message is an E-level
  recovery checks with return code 4.

What loose still refuses: a conditional compilation directive it cannot evaluate (leaving `>>IF`
out would choose a branch), a refusal inside a COPY member that is no data record, and a severe
message placed in none of the constructs above. A load module records `extended`.
`--remediate DIR` repairs with `--autofix DIR` and compiles under loose
([docs/autofix.md](autofix.md)).

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
  `-std=default`, `012-` under `-std=ibm-strict`). The rest are a student's
  `FUNCTION ORD(X) >= "A"`, three copies of a GnuCOBOL test that continues a literal with `-` after
  it, and a conformance test. cobc compares an expression with an alphanumeric operand by rules no
  manual gives (`N + 1 = X` is false where N is 5 and X is `"6"` padded with spaces,
  `N + 1 = "6"` true), and stops with an internal compiler error at `IF N + 1 = SPACE`.
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
