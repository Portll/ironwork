# Autofix: `--autofix DIR`

`ironwork check` and `ironwork run` take `--autofix DIR`: ironwork repairs what a message says has
exactly one fix, compiles the result again, and repeats until nothing fixable is left (25 rounds at
most). Then the command checks or runs the repaired source as it would the original. A fix that
would have to guess at what the author meant is not made; the message stays, and `autofix.json`
lists it.

**Status:** built 2026-10-08 (ironwork-roadmap 5.17.20), on the operator's request for a mode that
repairs sources.

## What DIR receives

| File | What it holds |
|---|---|
| The program's file name | The repaired program, every line, whether or not a fix touched it |
| Each COPY member a fix touched, under its own file name | The repaired member. DIR comes first in the copy libraries, so the repaired program reads it |
| `autofix.diff` | A unified diff, three lines of context, of each repaired file against the original |
| `autofix.json` | `fixes`: each fix's `file`, `line`, `col`, the message `id` it answers and `fix`, what it did. `remaining`: each message of severity E or above the repaired source still gives |

Each fix is also written to standard error, `file:line:col: fixed ID: what it did`. A position is
the one the message gave in the round that made the fix, so a fix after a line inserted above it
names the line as it was then.

## The fixes

| Message | Fix |
|---|---|
| IWS0105-E, a period was assumed before PROCEDURE DIVISION | The period, on a line of its own before the header |
| IWS0001-S, a period expected after the PROCEDURE DIVISION header, where the word found begins its line | The period, on a line of its own before that word |
| IWS0104-E, a scope terminator no verb takes, discarded | The terminator removed |
| IWX0061-W, a paragraph header in Area B | The header moved to column 8 |
| IWX0063-W or IWS0106-E, a zero-length literal | `' '` (or `" "`), the space the compiler read for it, where the line keeps within column 72 |
| IWX0058-W, a file read with cobc's tab stops | Every tab in the file expanded to the next column after a multiple of 8 |
| IWX0001-W, a file read in free form because it fails in fixed form, or cannot be fixed form | `>>SOURCE FORMAT FREE` as its first line |

Each is the reading the compiler already gave the source, written into it: a repaired program
behaves as the original did, under the level that read the original. Most of the repairs also bring
the source closer to Enterprise COBOL: a program needing only the first five compiles under strict
afterwards. `>>SOURCE FORMAT FREE` keeps a free-form program for `--compliance extended`.

## What it does not fix

Anything with more than one reading: an undefined name, a function or a usage ironwork does not
have, `GO TO` a paragraph that does not exist (in the bug datasets, a deleted `-EXIT` suffix), a
PICTURE out of range. These stay in `remaining` for a person, or for `--compliance relaxed`, which
compiles a refused statement as a hole.
