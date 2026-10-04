# ironwork's compiler messages

<!-- Generated from crates/syntax/src/messages.rs by its tests; do not edit. -->

Each message a compile gives carries an id: `IW`, an area's letter and four digits, then the
severity it was given, as in `IWR0001-S`. A site fills in the parts between braces. Once
released, an id keeps its meaning and is never given to another message; its wording may change.

## Areas

| Letter | Area |
|---|---|
| S | Source form, lexing, COPY and REPLACE, and syntax |
| C | Enterprise COBOL's compile rules |
| O | CBL and PROCESS options, and compiler flags |
| P | EXEC SQL, EXEC CICS, EXEC DLI, BMS and CSD |
| R | An Enterprise COBOL construct ironwork does not run yet, refused by name |
| L | ironwork's own limits |
| X | An extension `--compliance extended` reads (docs/compliance.md) |
| J | JCL a job run refuses |

## Messages

| Id | Severity | Text |
|---|---|---|
| IWR0001 | S | `XML PARSE VALIDATING WITH {schema}: the schema is in IBM's Optimized Schema Representation (OSR), which ironwork does not read` |
| IWX0001 | W | `free-form source (Micro Focus and GnuCOBOL; Enterprise COBOL reads fixed form alone): {why the file is read in free form}` |
| IWX0002 | W | `constant entry (Micro Focus and GnuCOBOL; Enterprise COBOL has no level 78 and no CONSTANT clause): {name} stands for its value wherever it is used after this entry` |
| IWX0003 | W | `<> (Micro Focus and GnuCOBOL; Enterprise COBOL writes NOT =) is read as NOT =` |
| IWX0004 | W | `literal concatenation with & (Micro Focus and GnuCOBOL; Enterprise COBOL has none): the literals on either side are one literal` |
| IWX0005 | W | `the COBOL 2002 binary usage (Micro Focus and GnuCOBOL; not Enterprise COBOL's): {usage} is read as PIC {picture} COMP-5` |
| IWX0006 | W | `PROGRAM-ID with no IDENTIFICATION DIVISION header before it (COBOL 2002, Micro Focus and GnuCOBOL; Enterprise COBOL requires the header): the program reads as though IDENTIFICATION DIVISION. came before it` |
| IWX0007 | W | `ASSIGN to a data item (Micro Focus and GnuCOBOL; Enterprise COBOL's assignment-name is never a data item): each OPEN of {file} takes its DD name from {item}` |
| IWX0008 | W | `an integer or numeric function as a MOVE's sender (GnuCOBOL; Enterprise COBOL takes one only where an arithmetic expression can be): FUNCTION {name} is moved as its value` |
| IWX0009 | W | `PROCEDURE DIVISION RETURNING OMITTED (GnuCOBOL; Enterprise COBOL's RETURNING names an 01 or 77 item of the LINKAGE SECTION): the program is read with no RETURNING phrase, and returns its RETURN-CODE to its caller as any program does` |
