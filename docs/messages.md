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
| IWL0001 | S | `lowering: {table} exceeds the LIR's limit` |
| IWL0002 | S | `lowering: the lowered program is invalid: {why}` |
| IWP0001 | S | `no mapset {to ascii uppercase} among the {sets} in the file` |
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
| IWR0052 | S | `lowering: {construct} is not lowered yet` |
| IWS0001 | S | `{what the syntax takes there}, found {the word or token there}` |
| IWS0002 | S | `{COPY or a translator's INCLUDE} {name}: no such member in the copy libraries` |
| IWS0003 | S | `COPY: {message}` |
| IWS0004 | S | `COPY {text}: the name ends in a period; the period that ends a COPY statement is the one followed by a space` |
| IWS0005 | S | `{verb}: {message}` |
| IWS0006 | S | `{verb} {name}: {display}` |
| IWS0007 | S | `more than 65535 copy members` |
| IWS0008 | S | `{verb} {name}: copies itself, or nests deeper than {MAX DEPTH}` |
| IWS0009 | S | `REPLACE OFF: a period to end the statement` |
| IWS0010 | S | `REPLACE ALSO and REPLACE LAST OFF are the 2014 COBOL standard's; Enterprise COBOL has REPLACE pseudo-text BY pseudo-text and REPLACE OFF` |
| IWS0011 | S | `REPLACE: a period to end the statement` |
| IWS0012 | S | `COPY {display}: {item}` |
| IWS0013 | S | `& with no literal after it` |
| IWS0014 | S | `constant {name}: {why}` |
| IWS0015 | S | `BINARY-CHAR is a one-byte binary item, and ironwork's binary items are two, four or eight bytes, as Enterprise COBOL's are` |
| IWS0016 | S | `& joins two alphanumeric or hexadecimal literals, or two national literals, either of which may be a level-78 constant standing for one` |
| IWS0017 | S | `X'{text}' is not an even number of hex digits` |
| IWS0018 | S | `NX'{text}': a national hexadecimal literal is 4 to 320 hex digits, four to each UTF-16 code unit` |
| IWS0019 | S | `a sign must be followed by a number` |
| IWS0020 | S | `literal concatenation with & is not Enterprise COBOL's` |
| IWS0021 | S | `unexpected character {character}` |
| IWS0022 | S | `EXEC with no END-EXEC` |
| IWS0023 | S | `a DBCS literal holds 1 to {DBCS LITERAL MAX} characters, not {count}` |
| IWS0024 | S | `an unterminated literal` |
| IWS0025 | E | `non-COBOL character {character}: the character was accepted` |
| IWS0026 | S | `unexpected '.'` |
| IWS0027 | S | `PICTURE with no character-string` |
| IWS0028 | S | `{file} has no SELECT ... ASSIGN` |
| IWS0029 | S | `a user-defined function or prototype cannot be nested within a program, function, method or class` |
| IWS0030 | S | `ENTRY cannot be used in a nested program` |
| IWS0031 | S | `FUNCTION-ID {name}: {why}` |
| IWS0032 | S | `FUNCTION-ID {name}: {name} is an intrinsic function's name (assumption C271)` |
| IWS0033 | S | `a second definition of user-defined function {name}` |
| IWS0034 | S | `FUNCTION-ID {name}: a user-defined function contains no programs, but {inner} is inside it` |
| IWS0035 | S | `END FUNCTION {end} ends function {name}` |
| IWS0036 | S | `a second CURRENCY SIGN clause for the currency symbol {symbol}` |
| IWS0037 | S | `UPSI-{number}: a second {which} STATUS phrase` |
| IWS0038 | S | `UPSI-{number}: a mnemonic-name or an ON or OFF STATUS phrase must follow it` |
| IWS0039 | S | `CURRENCY SIGN needs a nonempty alphanumeric literal` |
| IWS0040 | S | `CURRENCY SIGN {bytes} is not one character that can be a PICTURE currency symbol` |
| IWS0041 | S | `CURRENCY SIGN {value} is not one character that can be a PICTURE currency symbol` |
| IWS0042 | S | `CURRENCY SIGN {value} contains a digit, +, -, . or ,` |
| IWS0043 | S | `PICTURE SYMBOL {symbol} is not one character that can be a PICTURE currency symbol` |
| IWS0044 | S | `RECORD DELIMITER on {file}: the clause is for a file of ORGANIZATION SEQUENTIAL` |
| IWS0045 | S | `{indicator} {name} has no SELECT` |
| IWS0046 | S | `SD {name}: a sort or merge file takes no REPORT clause` |
| IWS0047 | S | `SD {name}: a sort or merge file takes no EXTERNAL or GLOBAL clause` |
| IWS0048 | S | `{indicator} {name}: LINAGE is given twice` |
| IWS0049 | S | `{indicator} {name}: EXTERNAL goes on the FD, not on a record of the FILE SECTION` |
| IWS0050 | S | `LINAGE: {phrase} {name} takes no subscript or reference modification` |
| IWS0051 | S | `a floating-point VALUE literal is for a COMP-1 or COMP-2 item, not a fixed-point one` |
| IWS0052 | S | `{written}: a floating-point literal's mantissa has at most 16 digits` |
| IWS0053 | S | `{written}: not an exponent` |
| IWS0054 | S | `{written}: the literal after ALL is alphanumeric, national or a figurative constant other than ALL` |
| IWS0055 | S | `ACCEPT ... FROM ENVIRONMENT is GnuCOBOL's, not Enterprise COBOL's` |
| IWS0056 | S | `EXIT FUNCTION: Enterprise COBOL does not yet support the format 4 EXIT statement; GOBACK ends a user-defined function` |
| IWS0057 | S | `CORRESPONDING takes one receiving group` |
| IWS0058 | S | `an inline PERFORM cannot have AFTER phrases: Enterprise COBOL takes them only when PERFORM names a procedure` |
| IWS0059 | S | `INITIALIZE: {word} is named twice in the {phrase} phrase` |
| IWS0060 | S | `ACCEPT ... FROM {name}: {why}` |
| IWS0061 | S | `SET ENVIRONMENT is GnuCOBOL's, not Enterprise COBOL's` |
| IWS0062 | S | `DFHRESP({condition}): not a CICS condition ironwork for COBOL knows` |
| IWS0063 | S | `DFHVALUE({name}): not a CVDA ironwork for COBOL knows` |
| IWS0064 | S | `NOT cannot follow the left parenthesis that distributes a relational operator` |
| IWS0065 | S | `<> is not an Enterprise COBOL relational operator: it writes NOT =` |
| IWS0066 | I | `{section} SECTION: no paragraph-name after its USE statement` |
| IWS0067 | S | `USE FOR DEBUGGING ON ALL: Enterprise COBOL debugs procedures, by name or as ALL PROCEDURES, and no other items` |
| IWS0068 | S | `USE FOR DEBUGGING is not allowed in a method` |
| IWS0069 | S | `USE FOR DEBUGGING is not allowed in a RECURSIVE program` |
| IWS0070 | S | `USE FOR DEBUGGING in a contained program: debugging sections are allowed only in the outermost program` |
| IWS0071 | S | `a REPOSITORY paragraph belongs to the outermost program only` |
| IWS0072 | S | `a class definition must be alone in its source file` |
| IWS0073 | S | `END CLASS {end} ends class {name}` |
| IWS0074 | S | `CLASS {name} IS "{text}": not a Java class name` |
| IWS0075 | S | `class {name} is named twice in the REPOSITORY paragraph` |
| IWS0076 | S | `WHEN-COMPILED is a special register too, so the REPOSITORY paragraph cannot name it` |
| IWS0077 | S | `FUNCTION {name} INTRINSIC: {name} is not an intrinsic function ironwork for COBOL knows` |
| IWS0078 | S | `FUNCTION {name}: a user-defined function in the REPOSITORY paragraph cannot be named {name}` |
| IWS0079 | S | `FUNCTION ALL: INTRINSIC follows ALL, which names every intrinsic function` |
| IWS0080 | S | `FUNCTION {name}: an intrinsic function is listed with INTRINSIC, and no user-defined function takes its name (assumption C271)` |
| IWS0081 | S | `{section}: the DATA DIVISION of a {kind} paragraph has only a WORKING-STORAGE SECTION` |
| IWS0082 | S | `a class definition cannot contain EXEC statements` |
| IWS0083 | S | `method "{name}" contains a program: a method cannot contain nested programs` |
| IWS0084 | S | `method "{name}" has a REPOSITORY paragraph: the class's applies to its methods` |
| IWS0085 | S | `INVOKE passes its arguments BY VALUE, not BY {word}` |
| IWS0086 | S | `INVOKE passes its arguments BY VALUE: write USING BY VALUE` |
| IWS0087 | S | `level {level} is not a data level` |
| IWS0088 | S | `TYPE and NEXT GROUP belong on a report group's 01-level entry` |
| IWS0089 | S | `a report group entry needs an 01-level entry before it` |
| IWS0090 | S | `SUM with SOURCE or VALUE in one entry` |
| IWS0091 | S | `COLUMN RIGHT and CENTER take an absolute column` |
| IWS0092 | S | `a literal runs to the end of the line with no continuation` |
| IWS0093 | S | `a continued literal must resume with its quote` |
| IWS0094 | S | `{shown}: the source-format directives >>SOURCE and $SET SOURCEFORMAT, giving FREE or FIXED, are the only compiler directives ironwork reads` |
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
