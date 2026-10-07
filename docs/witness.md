# The witness deck

Four jobs, one program each, that a site with Enterprise COBOL compiles and runs, and a record of
what the compiler did that can be published. ironwork's model of Enterprise COBOL rests on the
assumptions in `crates/numeric/src/assumptions.rs`. Most are *Chosen*: IBM's documentation leaves
the behaviour to the generated code, and only a run on the real compiler settles them. The deck is
how a site that holds a licence settles them without the compiler's output leaving the site.

## What the deck is

- `ORAC01` to `ORAC04`, written by `ironwork-oracle generate`. Each pins TRUNC, NUMPROC and ARITH
  on its CBL card, runs its cases, and DISPLAYs each case's storage in hex. No data sets, no Db2,
  no CICS. Each job compiles, links and runs through IGYWCLG, and takes seconds.
- `expected.tsv`: the model's prediction for each case, with the assumption ids it rests on.
- The record, written by `ironwork-oracle witness` from the saved job output. The table below says
  what it holds.

## Running it

1. Write the deck: `cargo run -p ironwork-oracle -- generate deck/`, or ask Portll for the four
   files and `expected.tsv`.
2. Edit each `.jcl` JOB card: accounting, CLASS, MSGCLASS, and the procedure name if the site's
   compile-link-go procedure is not IGYWCLG. Nothing else needs changing. ARCH, OPT and the other
   installation defaults are part of what the record reports.
3. Upload the four JCL files as text and submit them. The source uses only EBCDIC-invariant
   characters, so the transfer code page does not change it.
4. Save each job's complete output as one text file per job, the compile listing (SYSPRINT) and
   the program's output (SYSOUT) included:

       zowe zos-jobs submit local-file deck/ORAC01.jcl --view-all-spool-content > spool/ORAC01.txt

   From SDSF, print the whole job to a data set and download it as text.
5. Either send the four spool files to Portll under the site's own terms, or write the record
   on site and send only that:

       ironwork-oracle witness spool/ --runner "Example Bank, z/OS 3.1" --out witness.json

   `--runner` is free text, or leave it out. The command reads every file in the directory and
   writes one record for the set.

## What the record holds

| In the record | Not in the record |
|---|---|
| The deck's version and each program's source hash | Any line of the listing |
| The compiler's product, version and service level, and the date, from the listing header | Any line of SYSOUT |
| The options in effect for each program | The bytes the compiler produced for any case |
| Each spool file's SHA-256 and size | Data set names, job names, user ids, accounting fields |
| Per case: match, mismatch or missing, with each mismatch named by its case id | |
| Per assumption: cases held and broken, and its basis before the run | |
| `runner`, as given, or nothing | |
| A SHA-256 of the record | |

`ironwork-oracle check spool/` prints the compiler's bytes beside the model's for each mismatch.
That is for whoever holds the spool, and it writes nothing.

Spool files sent to Portll go to a private repository and are not published. Records are
published under `witness/` in this repository and listed below.

## Terms

The site runs its own licensed compiler on its own system and keeps the output. What ironwork
publishes is the count of cases on which the compiler and the model agreed, and the ids of those
on which they did not. Read the licence's terms on comparison results before sending anything; the
record carries none of the compiler's output, and the site decides whether to send the spool.

## Records received

| Date | Deck | Compiler | ARCH, OPT | Runner | Match | Mismatch | Missing | Record |
|---|---|---|---|---|---|---|---|---|

None yet. Each record received adds a row and a file under `witness/`, and moves the assumptions
its cases held from Chosen to Observed in `assumptions.rs`, naming the record.
