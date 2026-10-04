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
| IWC0001 | S | `{name} is not defined` |
| IWC0002 | S | `{name} is ambiguous; qualify it with OF or IN` |
| IWC0003 | S | `no paragraph or section named {name}` |
| IWC0004 | S | `{name} names more than one paragraph; qualify it with OF and its section` |
| IWR0001 | S | `XML PARSE VALIDATING WITH {schema}: the schema is in IBM's Optimized Schema Representation (OSR), which ironwork does not read` |
| IWR0002 | S | `{verb} is not a statement ironwork for COBOL supports yet` |
| IWR0003 | S | `{clause} is not a data description clause ironwork for COBOL supports yet` |
| IWR0004 | S | `USAGE {usage} is not supported yet` |
| IWR0005 | S | `VALUE {literal}: a floating-point VALUE of more than 31 digits in fixed point is not supported yet` |
| IWR0006 | S | `the {section} SECTION is not supported yet` |
| IWR0007 | S | `ORGANIZATION {organization} is not supported yet` |
| IWR0008 | S | `{clause} is not a SELECT clause ironwork for COBOL supports yet` |
| IWR0009 | S | `INDEXED BY in FACTORY or OBJECT data is not supported yet` |
| IWR0010 | S | `{item}: EXTERNAL in FACTORY or OBJECT WORKING-STORAGE is not supported yet` |
| IWR0011 | S | `a method's FILE SECTION can define only EXTERNAL files, which ironwork for COBOL does not support yet` |
| IWR0012 | S | `items after an OCCURS DEPENDING ON table in the same record are not supported yet` |
| IWR0013 | S | `ADVANCING {mnemonic}: stacker selection ({environment}) on a card punch is not supported yet` |
| IWR0014 | S | `WRITE ... ADVANCING {mnemonic} on {file}, whose FD has LINAGE, is not supported yet` |
| IWR0015 | S | `{item}: INDEXED BY in a GLOBAL record, in a program that contains others, is not supported yet` |
| IWR0016 | S | `FD {file}: LINAGE on an EXTERNAL file is not supported yet` |
| IWR0017 | S | `FD {file}: REPORT on an EXTERNAL file is not supported yet` |
| IWR0018 | S | `FD {file}: LINAGE or REPORT on a GLOBAL file, in a program that contains others, is not supported yet` |
| IWR0019 | S | `{item}: a GLOBAL record of FD {file}, which is not GLOBAL, in a program that contains others, is not supported yet` |
| IWR0020 | S | `{file}, a GLOBAL file of {declarer}: its {clause} {item} is not a GLOBAL name of {declarer}, which is not supported yet` |
| IWR0021 | S | `SET ADDRESS OF {record}, a GLOBAL LINKAGE record of {program}, in a program it contains is not supported yet` |
| IWR0022 | S | `ASSIGN {item}: a file SORT or MERGE reads, writes or describes taking its name from a data item is not supported yet` |
| IWR0023 | S | `USE GLOBAL BEFORE REPORTING {group} for a report group of a contained program is not supported yet` |
| IWR0024 | S | `report {report} in more than one FD (INITIATE ... UPON) is not supported yet` |
| IWR0025 | S | `NEXT PAGE on a LINE other than a group's first (MULTIPLE PAGE) is not supported yet` |
| IWR0026 | S | `CONTROL {control}: a subscripted or reference-modified control is not supported yet` |
| IWR0027 | S | `GROUP INDICATE outside a DETAIL group is not supported yet` |
| IWR0028 | S | `a SUM of an entry in another report is not supported yet` |
| IWR0029 | S | `REPORTS ARE ALL is not supported yet` |
| IWR0030 | S | `INITIATE ... UPON is not supported yet` |
| IWR0031 | S | `a GLOBAL report is not supported yet` |
| IWR0032 | S | `CODE with a mnemonic-name or an identifier is not supported yet` |
| IWR0033 | S | `LAST DETAIL with an identifier is not supported yet` |
| IWR0034 | S | `LINE LIMIT with an identifier is not supported yet` |
| IWR0035 | S | `the {clause} clause of an RD is not supported yet` |
| IWR0036 | S | `{clause} is not an RD clause ironwork for COBOL supports yet` |
| IWR0037 | S | `a level-{level} entry in the REPORT SECTION is not supported yet` |
| IWR0038 | S | `multiple SOURCES is not supported yet` |
| IWR0039 | S | `multiple VALUES is not supported yet` |
| IWR0040 | S | `GROUP LIMIT is not supported yet` |
| IWR0041 | S | `USAGE {usage} in a report group is not supported yet` |
| IWR0042 | S | `{a report group clause ironwork does not run} is not supported yet` |
| IWR0043 | S | `{clause} is not a report group clause ironwork for COBOL supports yet` |
| IWR0044 | S | `an entry with more than one SOURCE, VALUE or SUM (a multiple-choice entry) is not supported yet` |
| IWR0045 | S | `a SUM or COUNT term in a SOURCE expression is not supported yet` |
| IWR0046 | S | `CONTROL FOOTING FOR ALL is not supported yet` |
| IWR0047 | S | `CONTROL HEADING ... OR PAGE is not supported yet` |
| IWR0048 | S | `a CONTROL FOOTING for more than one control is not supported yet` |
| IWR0049 | S | `multiple LINES is not supported yet` |
| IWR0050 | S | `multiple COLUMNS is not supported yet` |
| IWR0051 | S | `SUM of an arithmetic expression is not supported yet` |
| IWS0001 | S | `{what the syntax takes there}, found {the word or token there}` |
| IWS0002 | S | `{COPY or a translator's INCLUDE} {name}: no such member in the copy libraries` |
| IWX0001 | W | `free-form source (Micro Focus and GnuCOBOL; Enterprise COBOL reads fixed form alone): {why the file is read in free form}` |
| IWX0002 | W | `constant entry (Micro Focus and GnuCOBOL; Enterprise COBOL has no level 78 and no CONSTANT clause): {name} stands for its value wherever it is used after this entry` |
| IWX0003 | W | `<> (Micro Focus and GnuCOBOL; Enterprise COBOL writes NOT =) is read as NOT =` |
| IWX0004 | W | `literal concatenation with & (Micro Focus and GnuCOBOL; Enterprise COBOL has none): the literals on either side are one literal` |
| IWX0005 | W | `the COBOL 2002 binary usage (Micro Focus and GnuCOBOL; not Enterprise COBOL's): {usage} is read as PIC {picture} COMP-5` |
| IWX0006 | W | `PROGRAM-ID with no IDENTIFICATION DIVISION header before it (COBOL 2002, Micro Focus and GnuCOBOL; Enterprise COBOL requires the header): the program reads as though IDENTIFICATION DIVISION. came before it` |
| IWX0007 | W | `ASSIGN to a data item (Micro Focus and GnuCOBOL; Enterprise COBOL's assignment-name is never a data item): each OPEN of {file} takes its DD name from {item}` |
| IWX0008 | W | `an integer or numeric function as a MOVE's sender (GnuCOBOL; Enterprise COBOL takes one only where an arithmetic expression can be): FUNCTION {name} is moved as its value` |
| IWX0009 | W | `PROCEDURE DIVISION RETURNING OMITTED (GnuCOBOL; Enterprise COBOL's RETURNING names an 01 or 77 item of the LINKAGE SECTION): the program is read with no RETURNING phrase, and returns its RETURN-CODE to its caller as any program does` |
| IWX0010 | W | `{ACCEPT ... FROM COMMAND-LINE, ARGUMENT-NUMBER or ARGUMENT-VALUE, or DISPLAY ... UPON ARGUMENT-NUMBER} (Micro Focus and GnuCOBOL; Enterprise COBOL reads no command line): {what the job step's PARM program arguments give}` |
