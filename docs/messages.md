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
| IWC0005 | S | `CLASS {clause}: {message}` |
| IWC0006 | S | `{verb} CORRESPONDING {name}: {why}` |
| IWC0007 | S | `CORRESPONDING {group ref}: {name} in it cannot be named uniquely` |
| IWC0008 | S | `USE AFTER EXCEPTION/ERROR ON {name}: {why}` |
| IWC0009 | S | `USE AFTER EXCEPTION/ERROR ON {mode}: another procedure is for the same open mode` |
| IWC0010 | S | `USE FOR DEBUGGING is not allowed in a program compiled with THREAD` |
| IWC0011 | S | `USE FOR DEBUGGING ON ALL PROCEDURES: it may be written once, and no other USE FOR DEBUGGING may name a procedure` |
| IWC0012 | S | `USE FOR DEBUGGING ON {name}: {why}` |
| IWC0013 | S | `{name}: a debugging section may refer only to declarative procedures` |
| IWC0014 | S | `{name}: only a debugging section may refer to a procedure in a debugging section` |
| IWC0015 | S | `PERFORM ... THRU: a declarative procedure and the other end of the range must be in the same declarative section` |
| IWC0016 | S | `FUNCTION-ID {name}: {name} is not an 01 or 77 item of the LINKAGE SECTION` |
| IWC0017 | S | `FUNCTION-ID {name}: a user-defined function needs PROCEDURE DIVISION RETURNING` |
| IWC0018 | S | `EXEC {kind}: SQL and CICS cannot be used with user-defined functions, so neither in one nor in a program after one in its source (assumption C273)` |
| IWC0019 | S | `PROCEDURE DIVISION USING BY VALUE {file}: a function's BY VALUE parameter is binary, floating-point, a pointer, or one alphanumeric or national character` |
| IWC0020 | S | `FUNCTION-ID {id}: {why} from the prototype at line {line}` |
| IWC0021 | S | `FUNCTION {name} takes {params} arguments, not {args}` |
| IWC0022 | S | `FUNCTION {name}: a user-defined function's argument is an identifier, a literal or an arithmetic expression` |
| IWC0023 | S | `FUNCTION {name}: only an alphanumeric or national function's value can be reference-modified` |
| IWC0024 | S | `FUNCTION {name} argument {k}: a function's argument is not a figurative constant` |
| IWC0025 | S | `FUNCTION {name} argument {k}: {formal} is numeric, and takes an argument COMPUTE could send it (assumption C272)` |
| IWC0026 | S | `FUNCTION {name} argument {k} ({name}): {why}` |
| IWC0027 | S | `OCCURS at level {level}: Enterprise COBOL takes OCCURS only at levels 02 to 49` |
| IWC0028 | S | `a level-88 entry after a level-66 entry: a RENAMES item cannot be a conditional variable` |
| IWC0029 | S | `a level-88 entry with no item before it` |
| IWC0030 | S | `a level-88 entry needs a name` |
| IWC0031 | S | `RENAMES goes with level 66, and level 66 with RENAMES` |
| IWC0032 | S | `a level-66 entry must follow the entries of a level-01 record` |
| IWC0033 | S | `a level-66 entry has a name and a RENAMES clause, and nothing else` |
| IWC0034 | S | `level {level} is not a data level` |
| IWC0035 | S | `level {level} after a level-66 entry: a record's RENAMES entries follow its last entry` |
| IWC0036 | S | `level {level} with no group to belong to` |
| IWC0037 | S | `REDEFINES {target}: no earlier 01-level item of that name` |
| IWC0038 | S | `OCCURS 0 is not a table` |
| IWC0039 | S | `RENAMES {name}: {message}` |
| IWC0040 | S | `RENAMES {first}: a level-66 entry cannot rename a level-01 record` |
| IWC0041 | S | `RENAMES {first} THRU {last}: the last item cannot be within the first` |
| IWC0042 | S | `RENAMES {first} THRU {last}: the last item must start and end no earlier than the first` |
| IWC0043 | S | `RENAMES {first} THRU {last}: no OCCURS DEPENDING ON between them` |
| IWC0044 | S | `REDEFINES {target}: no earlier item of that name at this level` |
| IWC0045 | S | `a SYNCHRONIZED item at the start of a REDEFINES would need {slack} slack bytes: the redefined item must be on a {message}-byte boundary` |
| IWC0046 | S | `{clause} names {name}, which is not a file` |
| IWC0048 | S | `OCCURS DEPENDING ON {object}: the object cannot follow an OCCURS DEPENDING ON table in its record` |
| IWC0049 | S | `OCCURS DEPENDING ON {object}: not a numeric data item` |
| IWC0050 | S | `PROCEDURE DIVISION USING {param}: not an 01 or 77 item of the LINKAGE SECTION` |
| IWC0051 | S | `PROCEDURE DIVISION RETURNING {name}: not an 01 or 77 item of the LINKAGE SECTION` |
| IWC0052 | S | `ASSIGN USING or DYNAMIC {name}: a Micro Focus and GnuCOBOL form; --compliance extended reads it` |
| IWC0053 | S | `ASSIGN {name}: not a data item` |
| IWC0054 | S | `ASSIGN {name}: the item holding the file's name must be alphanumeric or a group` |
| IWC0055 | W | `no STOP RUN, GOBACK or EXIT PROGRAM in the program: check that it ends` |
| IWC0056 | W | `CALL 'CEECBLDY' under INTDATE(LILIAN): CEECBLDY gives an ANSI integer date, which nothing can use under LILIAN, so the CALL is to CEEDAYS` |
| IWC0057 | S | `ENTRY '{item}': a program with PROCEDURE DIVISION RETURNING cannot have ENTRY statements` |
| IWC0058 | S | `ENTRY '{item}': the name is already the program's or another ENTRY's` |
| IWC0059 | S | `ENTRY '{item}' USING {param}: not an 01 or 77 item of the LINKAGE SECTION` |
| IWC0060 | S | `ENTRY '{name}' must be a sentence of its own, not inside another statement` |
| IWC0061 | S | `a GO TO with no procedure-name cannot be used in {why}` |
| IWC0062 | S | `a GO TO with no procedure-name must be its paragraph's only sentence` |
| IWC0063 | S | `ALTER cannot be used in {why}` |
| IWC0064 | S | `ALTER {name}: a section, where ALTER names a paragraph` |
| IWC0065 | S | `ALTER {name}: the paragraph must hold one sentence, a GO TO without DEPENDING ON` |
| IWC0066 | S | `{name}: a DBCS item's VALUE is a DBCS literal of at most {size} characters, SPACE or ALL with a DBCS literal` |
| IWC0067 | S | `{name}: a DBCS literal can be the VALUE of a DBCS item only` |
| IWC0068 | S | `VALUE of {name}: {what}, where a numeric item's VALUE literal must be numeric` |
| IWC0069 | S | `PICTURE {name}: {positions} digit positions, more than the {max} {option} allows` |
| IWC0070 | S | `the literal {t} has more than the {max} digits {option} allows` |
| IWC0071 | S | `a condition as the WHEN object of a value subject` |
| IWC0072 | S | `a value as the WHEN object of a TRUE, FALSE or condition subject` |
| IWC0073 | S | `DISPLAY UPON {upon}: {why}` |
| IWC0074 | S | `CLOSE {name}: REEL, UNIT and NO REWIND are not valid for an indexed or relative file` |
| IWC0075 | S | `{verb} {record}: not a record of a file` |
| IWC0076 | S | `START KEY takes =, >, NOT < or >=` |
| IWC0077 | S | `INITIALIZE {name}: a level-66 RENAMES item cannot be initialized` |
| IWC0078 | S | `INITIALIZE {name}: a variably located item, or a group holding one, cannot be initialized (Language Reference p. 351)` |
| IWC0079 | S | `SET {name} TO FALSE: the condition-name has no WHEN SET TO FALSE value` |
| IWC0080 | S | `{exit} must be inside an inline PERFORM` |
| IWC0081 | S | `PERFORM VARYING {var}: not a numeric elementary item or an index-name` |
| IWC0082 | S | `PERFORM VARYING {var} {phrase}: an arithmetic expression, where {phrase} takes an identifier, index-name or literal` |
| IWC0083 | S | `no file named {name}` |
| IWC0084 | S | `{verb} {name}: not an indexed or relative file` |
| IWC0085 | S | `{verb} {name}: the file's ACCESS MODE is RANDOM` |
| IWC0086 | S | `class-name {name} tests a data item, not an expression` |
| IWC0087 | S | `class-name {name} tests a data item of USAGE DISPLAY, and {name} is not one` |
| IWC0088 | S | `{key}: not a key of {file}` |
| IWC0089 | S | `{file}: an indexed file needs a RECORD KEY` |
| IWC0090 | S | `{name}: a key of {file} must be in its records` |
| IWC0091 | S | `{name}: the RELATIVE KEY of {file} must not be in its records` |
| IWC0092 | S | `{file}: random or dynamic access needs a RELATIVE KEY` |
| IWC0093 | S | `{name}: the DEPENDING ON item of {file} must be an elementary unsigned integer` |
| IWC0094 | S | `{name}: the PASSWORD of {file} must be an alphabetic, alphanumeric or alphanumeric-edited item of WORKING-STORAGE` |
| IWC0095 | S | `{name} is a condition-name, not a data item` |
| IWC0096 | S | `{name}: only a debugging section may reference DEBUG-ITEM` |
| IWC0097 | S | `{name} takes {dims} subscripts, not {subscripts}` |
| IWC0098 | W | `INITIALIZE {name}: none of its items is of a category REPLACING names ({categories}), so it is not initialized` |
| IWC0099 | S | `INSPECT FUNCTION {name}: an integer or numeric function can be used only where an arithmetic expression can, not as the inspected item` |
| IWC0100 | S | `INSPECT FUNCTION {name} {phrase}: {phrase} stores into the inspected item, and a function-identifier cannot be a receiving operand` |
| IWC0101 | S | `DISPLAY FUNCTION {name}: an integer or numeric function can be used only where an arithmetic expression can, and DISPLAY takes none` |
| IWC0102 | S | `MOVE FUNCTION {name}: an integer or numeric function can be used only where an arithmetic expression can, not as a MOVE's sender` |
| IWC0103 | S | `INSPECT {name}: {operand} cannot be an operand here, since {why}` |
| IWC0104 | S | `SEARCH {table} VARYING {value}: not an index-name, an index data item or an elementary integer item` |
| IWC0105 | S | `the literal {t} has more than {min} digits` |
| IWC0106 | S | `FUNCTION {file}: neither an intrinsic function nor a user-defined function defined or prototyped before this program` |
| IWC0108 | S | `WRITE ... END-OF-PAGE: the FD of {file} has no LINAGE clause` |
| IWC0109 | S | `LINAGE-COUNTER can be read, but no statement can change it` |
| IWC0110 | S | `NUMCHECK: {name} {why} wherever this statement reads it: its VALUE clauses give it X'{hex}' and no statement changes it, so the test is removed (see {NUMCHECK ALWAYS FAILS})` |
| IWC0111 | W | `NORENT conflicts with {and}, which IBM compiles only as RENT (see {OO OPTIONS REQUIRED})` |
| IWC0112 | W | `{who} uses object-oriented syntax, which IBM compiles only with THREAD, DLL, RENT and DBCS: {missing} missing from its CBL or PROCESS cards (see {OO OPTIONS REQUIRED} and {OO OPTIONS SEVERITY})` |
| IWC0113 | W | `INITIAL conflicts with THREAD, which IBM compiles only as NOINITIAL (see {INITIAL UNDER THREAD})` |
| IWC0114 | S | `{who} is compiled with THREAD, which requires RECURSIVE in its PROGRAM-ID paragraph` |
| IWC0115 | S | `{who} is INITIAL, which THREAD does not allow` |
| IWC0116 | S | `{who} contains program {inner}, and THREAD does not allow nested programs` |
| IWC0117 | S | `{verb} is not allowed in a program compiled with THREAD` |
| IWC0118 | S | `not a class definition` |
| IWC0119 | S | `{inherits}: the class a class INHERITS must be named in its REPOSITORY paragraph` |
| IWC0120 | S | `class {def} cannot inherit from itself` |
| IWC0121 | S | `{factory} method "{message}" has the same parameter types as {factory} method "{twin}"` |
| IWC0122 | S | `method "{name}" receives {param} BY REFERENCE: a method's parameters are BY VALUE` |
| IWC0123 | S | `method "{name}" parameter {param}: {message}` |
| IWC0124 | S | `method "{name}" parameter {param}: not a record of the method's own LINKAGE SECTION` |
| IWC0125 | S | `SET ADDRESS OF {name}: FACTORY and OBJECT data is WORKING-STORAGE, not LINKAGE` |
| IWC0126 | S | `method "{name}" RETURNING {name}: {message}` |
| IWC0127 | S | `method "{name}" RETURNING {name}: not a record of the method's own LINKAGE SECTION` |
| IWC0128 | S | `OBJECT REFERENCE {character}: the class must be named in the REPOSITORY paragraph` |
| IWC0129 | S | `a class definition cannot contain EXEC statements` |
| IWC0130 | S | `EXIT METHOD can be used only in a method` |
| IWC0131 | S | `EXIT PROGRAM cannot be used in a method: use EXIT METHOD or GOBACK` |
| IWC0132 | S | `SELF can be used only in a method` |
| IWC0133 | S | `{name} is {what}: it can be used only in SET, INVOKE, CALL and a relation condition` |
| IWC0134 | S | `JNIENVPTR cannot receive a value` |
| IWC0135 | S | `SELF cannot receive a value` |
| IWC0136 | S | `object references and function-pointers compare only as equal or not equal` |
| IWC0137 | S | `an object reference compares with another object reference, SELF or NULL; a function-pointer with another or NULL` |
| IWC0138 | S | `SET {name} TO ENTRY: the receiver must be a procedure-pointer or function-pointer` |
| IWC0139 | S | `SET {names} TO ENTRY: {fault}` |
| IWC0140 | S | `SET {name} TO: {message}` |
| IWC0141 | S | `FUNCTION {file}: a figurative constant is an argument only inside an arithmetic expression` |
| IWC0142 | S | `FUNCTION {file}: {name} is a pointer or object reference, where an argument is alphabetic, alphanumeric, national or numeric` |
| IWC0143 | S | `{first} compared with {second}: an arithmetic expression or a numeric function is compared only with a numeric operand` |
| IWC0144 | S | `WRITE ... ADVANCING: {file} is not a sequential file` |
| IWC0145 | S | `WRITE ... BEFORE ADVANCING, or ADVANCING a mnemonic-name, is not allowed for the line-sequential file {file}` |
| IWC0146 | S | `report {name} is named in no FD's REPORT clause` |
| IWC0147 | S | `report {ri}: a line reaches column {end}, beyond the {width} bytes of the report file's record` |
| IWC0148 | S | `report {ri}: a line reaches column {end}, beyond LINE LIMIT {limit}` |
| IWC0149 | S | `a group entry in a report group cannot have COLUMN, PICTURE, SOURCE, VALUE or SUM` |
| IWC0150 | S | `a SUM entry needs a PICTURE` |
| IWC0151 | S | `a VALUE entry with a figurative constant or ALL needs a PICTURE` |
| IWC0152 | S | `a printed SOURCE entry needs a PICTURE` |
| IWC0153 | S | `a report entry with no SOURCE, VALUE or SUM needs a data-name for the program to fill it` |
| IWC0154 | S | `BLANK WHEN ZERO needs a numeric PICTURE` |
| IWC0155 | S | `a SUM entry needs a numeric PICTURE` |
| IWC0156 | S | `a COLUMN with no LINE above it` |
| IWC0157 | S | `a report field that starts left of column 1` |
| IWC0158 | S | `USE BEFORE REPORTING {group}: the program has no REPORT SECTION` |
| IWC0159 | S | `USE BEFORE REPORTING {group}: no report group has that name` |
| IWC0160 | S | `USE BEFORE REPORTING {group}: more than one report group has that name; qualify it with IN` |
| IWC0161 | S | `an arithmetic SOURCE, or ROUNDED, needs a numeric PICTURE` |
| IWC0162 | S | `an absolute LINE needs a PAGE LIMIT` |
| IWC0163 | S | `a report group whose first LINE is relative must have only relative LINEs` |
| IWC0164 | S | `absolute LINE numbers in a report group must increase` |
| IWC0165 | S | `NEXT PAGE needs a PAGE LIMIT` |
| IWC0166 | S | `a PAGE HEADING or PAGE FOOTING cannot begin on the NEXT PAGE` |
| IWC0167 | S | `report {name}: its report control area could not be laid out` |
| IWC0168 | S | `a CONTROL HEADING or FOOTING must name its control when the report has several` |
| IWC0169 | S | `a CONTROL HEADING or FOOTING names a control that is not in the report's CONTROL clause` |
| IWC0170 | S | `report {name} has two {kind} groups for the same level` |
| IWC0171 | S | `a PAGE HEADING or PAGE FOOTING needs a PAGE LIMIT` |
| IWC0172 | S | `NEXT GROUP with a line or NEXT PAGE needs a PAGE LIMIT` |
| IWC0173 | S | `NEXT GROUP is not allowed in a PAGE HEADING or REPORT FOOTING` |
| IWC0174 | S | `report {name} has no CONTROL HEADING, DETAIL or CONTROL FOOTING group` |
| IWC0175 | S | `RESET ON names a control that is not in the report's CONTROL clause` |
| IWC0176 | S | `SUM ... UPON {name}: not a DETAIL group of report {name}` |
| IWC0177 | S | `SUM {operand}: the entry summed must be numeric` |
| IWC0178 | S | `SUM {operand}: an entry the program fills itself cannot be summed` |
| IWC0179 | S | `SUM {operand}: not a numeric data item` |
| IWC0180 | S | `SUM entries of a report group total each other in a circle` |
| IWC0181 | S | `HEADING, FIRST DETAIL, LAST DETAIL and FOOTING need a PAGE LIMIT` |
| IWC0182 | S | `report {name}: the page regions must run HEADING <= FIRST DETAIL <= LAST DETAIL <= FOOTING <= PAGE LIMIT` |
| IWC0183 | S | `{number} is not a report of this program` |
| IWC0184 | S | `GENERATE {name}: summary reporting needs a CONTROL HEADING or CONTROL FOOTING group` |
| IWC0185 | S | `GENERATE {name}: not a DETAIL group` |
| IWC0186 | S | `GENERATE {name}: no report or DETAIL group of that name` |
| IWC0187 | S | `GENERATE {name}: more than one report has a DETAIL group of that name; qualify it with IN` |
| IWC0188 | S | `{name} is a reserved word, so it cannot name {what}` |
| IWC0189 | S | `{name}: EXTERNAL is not allowed in the {section} SECTION` |
| IWC0190 | S | `{name}: EXTERNAL goes on a level-01 entry` |
| IWC0191 | S | `{name}: EXTERNAL and REDEFINES cannot be in the same entry` |
| IWC0192 | S | `an EXTERNAL record needs a data-name, not FILLER` |
| IWC0193 | S | `{name}: another EXTERNAL record of the program has the same name` |
| IWC0194 | S | `{name}: GLOBAL goes on a level-01 entry` |
| IWC0195 | S | `a GLOBAL record needs a data-name, not FILLER` |
| IWC0196 | S | `{name}: another GLOBAL record of the DATA DIVISION has the same name` |
| IWC0197 | S | `{name}: an item of EXTERNAL record {name} takes no VALUE clause` |
| IWC0198 | S | `FD {file}: a record of an EXTERNAL or GLOBAL file needs a data-name, not FILLER` |
| IWC0199 | S | `{item}: {size} bytes, larger than the EXTERNAL record {name} it redefines` |
| IWC0200 | S | `SET ADDRESS OF {name}: an EXTERNAL or GLOBAL record is not a LINKAGE record of the program` |
| IWC0201 | S | `RELEASE {record}: not a record of a sort file (SD)` |
| IWC0202 | S | `no file named {file}` |
| IWC0203 | S | `RETURN {file}: not a sort or merge file (SD)` |
| IWC0204 | S | `COLLATING SEQUENCE {alphabet}: not an alphabet-name of SPECIAL-NAMES` |
| IWC0205 | S | `SET {name} TO {OFF}: {name} is not the mnemonic-name of an UPSI switch` |
| IWC0206 | S | `{name} is the mnemonic-name of UPSI-{number}: only SET ... TO ON or OFF and a condition-name's qualifier can name it` |
| IWC0207 | S | `SET {name} TO TRUE: the UPSI switch's entry has no mnemonic-name, which would be its conditional variable` |
| IWC0208 | S | `{verb} {name}: not a sort or merge file (SD)` |
| IWC0209 | S | `{verb} {name}: no ASCENDING or DESCENDING KEY` |
| IWC0210 | S | `{verb} {name}: KEY needs a data name` |
| IWC0211 | S | `{key}: a key of {verb} {name} must be in its records` |
| IWC0212 | S | `{key}: a sort key cannot be in a table` |
| IWC0213 | S | `{key}: a sort key cannot follow an OCCURS DEPENDING ON table in its record` |
| IWC0214 | S | `{key}: a POINTER, INDEX, object reference or function-pointer item cannot be a sort key` |
| IWC0215 | S | `{verb} {name}: no {phrase} or {procedure}` |
| IWC0216 | S | `{phrase} {file}: a sort or merge file (SD) cannot be one` |
| IWC0217 | S | `{phrase} {file}: the file's ACCESS MODE is RANDOM` |
| IWC0218 | S | `MERGE {name}: USING names at least two files` |
| IWC0219 | S | `MERGE {name}: not a merge file (SD)` |
| IWC0220 | S | `SORT {name}: a table SORT takes no USING, GIVING or procedures` |
| IWC0221 | S | `SORT {name}: not a table` |
| IWC0222 | S | `no file or table named {name}` |
| IWC0223 | S | `SORT {name}: not a table (no OCCURS)` |
| IWC0224 | S | `SORT {name}: a subscript for each table that contains it, and none for itself` |
| IWC0225 | S | `SORT {name}: no KEY phrase, and its OCCURS has none` |
| IWC0226 | S | `{key}: a key of SORT {name} must be its element or an item within it` |
| IWC0227 | S | `{key}: a table SORT key cannot be in a table within the element` |
| IWC0228 | S | `{file}: LINAGE is for a sequential file, not a line-sequential one` |
| IWC0229 | S | `{file}: LINAGE is for a sequential file, not an indexed or relative one` |
| IWC0230 | S | `{file}: {phrase} {number} is more than the {MOST} lines LINAGE allows` |
| IWC0231 | S | `{file}: {phrase} {name} is not an unsigned integer data item` |
| IWC0232 | S | `{file}: LINAGE 0: the page body needs at least one line` |
| IWC0233 | S | `{file}: FOOTING 0: the footing starts at line 1 or later` |
| IWC0234 | S | `{file}: FOOTING {footing} is past the page body of {body} lines` |
| IWC0235 | S | `an elementary item needs a PICTURE` |
| IWC0236 | S | `a group item cannot have a PICTURE` |
| IWC0237 | S | `an object reference, function-pointer or procedure-pointer takes no PICTURE and only VALUE NULL` |
| IWC0238 | S | `POINTER and INDEX items take no PICTURE` |
| IWC0239 | S | `COMP-1 and COMP-2 items take no PICTURE` |
| IWC0240 | S | `BLANK WHEN ZERO needs a numeric or numeric-edited item of USAGE DISPLAY or NATIONAL` |
| IWC0241 | S | `a binary item holds at most 18 digits` |
| IWC0242 | S | `JUSTIFIED cannot be given for a DBCS item whose PICTURE has B` |
| IWC0243 | S | `a PICTURE with G needs USAGE DISPLAY-1 (Language Reference SC27-8713-03, p. 214)` |
| IWC0244 | S | `a SIGN clause needs an S in the PICTURE` |
| IWC0245 | S | `INVOKE {target}: SELF and SUPER can be used only in a method` |
| IWC0246 | S | `INVOKE {target}: not an object reference or a class named in the REPOSITORY paragraph` |
| IWC0247 | S | `INVOKE {target} NEW: NEW takes a class-name from the REPOSITORY paragraph` |
| IWC0248 | S | `INVOKE ... NEW needs RETURNING an object reference` |
| IWC0249 | S | `INVOKE ... NEW RETURNING {name}: not an object reference` |
| IWC0250 | S | `INVOKE with an empty method name` |
| IWC0251 | S | `INVOKE ... {name}: a method name is held in an alphanumeric or national item` |
| IWC0252 | S | `INVOKE {target} {name}: a method named by a data item is invoked on a universal object reference` |
| IWC0253 | S | `INVOKE argument: {message}` |
| IWC0254 | S | `INVOKE ... RETURNING {name}: not reference-modified` |
| IWC0255 | S | `INVOKE ... RETURNING {name}: {message}` |
| IWC0256 | S | `ACCEPT ... ON EXCEPTION: of the ACCEPT statements, only ACCEPT ... FROM ARGUMENT-VALUE under --compliance extended has an exception` |
| IWC0257 | S | `ACCEPT ... FROM {name}: Micro Focus's and GnuCOBOL's, not Enterprise COBOL's; --compliance extended reads it from the job step's PARM` |
| IWC0258 | S | `DISPLAY UPON ARGUMENT-NUMBER: Micro Focus's and GnuCOBOL's, not Enterprise COBOL's; --compliance extended reads it` |
| IWC0259 | S | `DISPLAY UPON ARGUMENT-NUMBER: it shows one numeric item or literal, the number of the argument the next ACCEPT ... FROM ARGUMENT-VALUE takes` |
| IWC0260 | S | `PICTURE {picture}: P must be one string of scaling positions at the left or right end of the digits` |
| IWC0261 | S | `PICTURE {picture}: {character} is not a PICTURE symbol` |
| IWC0262 | S | `PICTURE {picture}: more than 134217727 character positions` |
| IWC0263 | S | `PICTURE {picture}: more than 31 digits` |
| IWC0264 | S | `PICTURE {picture}: mixes symbols of different categories` |
| IWC0265 | S | `PICTURE {picture}: {character} cannot be in a PICTURE of {symbol}, which takes {symbol} and B only` |
| IWC0266 | S | `PICTURE {picture}: more character positions than a DBCS item holds` |
| IWC0267 | S | `BLANK WHEN ZERO cannot be given for a PICTURE with S` |
| IWC0268 | S | `PICTURE {picture}: an edited PICTURE longer than 4096 positions` |
| IWC0269 | S | `PICTURE {picture}: S and N are not allowed in an edited PICTURE` |
| IWC0270 | S | `PICTURE {picture}: an alphanumeric-edited PICTURE takes only X, A, 9, B, 0 and /` |
| IWC0271 | S | `PICTURE {picture}: two floating insertion strings` |
| IWC0272 | S | `PICTURE {picture}: {character} is not a numeric-edited symbol` |
| IWC0273 | S | `PICTURE {picture}: more than one decimal point` |
| IWC0274 | S | `PICTURE {picture}: a numeric-edited PICTURE needs 1 to 31 digit positions` |
| IWC0275 | S | `PICTURE {picture}: bad repetition ({count})` |
| IWC0276 | S | `PICTURE {picture}: a repetition with nothing to repeat` |
| IWC0277 | S | `PICTURE {picture}: two different currency symbols` |
| IWC0278 | S | `PICTURE {picture}: '$' is not a currency symbol of this program, whose CURRENCY SIGN clauses or CURRENCY option name others` |
| IWC0279 | S | `{ALPHABET or PROGRAM COLLATING SEQUENCE and its name}: not an alphabet-name of SPECIAL-NAMES` |
| IWC0280 | S | `{ALPHABET or PROGRAM COLLATING SEQUENCE and its name}: the character X'{hex}' is given more than one position` |
| IWC0281 | S | `{ALPHABET or PROGRAM COLLATING SEQUENCE and its name}: {number} is not an ordinal position from 1 to 256` |
| IWC0282 | S | `{ALPHABET or PROGRAM COLLATING SEQUENCE and its name}: NULL cannot be in an ALPHABET clause` |
| IWC0283 | S | `{ALPHABET or PROGRAM COLLATING SEQUENCE and its name}: a national literal cannot be in an ALPHABET clause` |
| IWC0284 | S | `{ALPHABET or PROGRAM COLLATING SEQUENCE and its name}: a DBCS literal cannot be in an ALPHABET clause` |
| IWC0285 | S | `{ALPHABET or PROGRAM COLLATING SEQUENCE and its name}: ALL cannot be in an ALPHABET clause` |
| IWC0286 | S | `{ALPHABET or PROGRAM COLLATING SEQUENCE and its name}: a literal of THROUGH or ALSO must be one character` |
| IWC0287 | S | `FUNCTION {name}: an ALL subscript stands for a varying number of arguments, and {name} takes {count}` |
| IWC0288 | S | `FUNCTION {name}: {class} and {class} arguments, where all must be of the same class` |
| IWC0289 | W | `INITCHECK(STRICT): {item} may be used uninitialized: a path to this statement does not set {it} (see {analysis})` |
| IWC0290 | W | `INITCHECK: {item} may be used uninitialized: no path to this statement sets {it} (see {analysis})` |
| IWC0291 | S | `{ALPHABET or PROGRAM COLLATING SEQUENCE and its name}: {a character the program's code page does not hold}` |
| IWC0292 | S | `VALUE of {name}: a numeric literal, where a numeric-edited item's VALUE is an alphanumeric literal or a figurative constant written in edited form; --compliance extended edits the number into it` |
| IWC0293 | S | `BINARY-CHAR: Micro Focus's and GnuCOBOL's one-byte binary, not Enterprise COBOL's; --compliance extended reads it` |
| IWC0294 | S | `BINARY-CHAR takes no PICTURE` |
| IWC0295 | S | `{name}: two programs of {program} have this name, and the programs of a separately compiled program each need their own (--program-scope=flexible allows it)` |
| IWC0296 | S | `CALL '{name}': no program of the compilation has this name, and --unresolved-calls=fail refuses a static CALL the binder could not resolve` |
| IWC0297 | S | `FUNCTION {name}: {argument} is numeric, where {name} takes an alphabetic, alphanumeric or national argument` |
| IWC0298 | S | `{DISPLAY or ACCEPT on the screen, or the SCREEN SECTION}: Micro Focus's and GnuCOBOL's, not Enterprise COBOL's; --compliance extended reads it` |
| IWC0299 | S | `{a locking phrase}: Micro Focus's and GnuCOBOL's, not Enterprise COBOL's; --compliance extended reads it` |
| IWC0300 | S | `CALL ... RETURNING {OMITTED, NOTHING or NULL}: GnuCOBOL's, not Enterprise COBOL's, whose RETURNING names a data item; --compliance extended reads it` |
| IWC0301 | S | `COMP-X: Micro Focus's binary in the fewest bytes its digits need, not Enterprise COBOL's; --compliance extended reads it` |
| IWC0302 | S | `PIC X(n) COMP-5: Micro Focus's and GnuCOBOL's binary of n bytes, where Enterprise COBOL's COMP-5 takes a numeric PICTURE; --compliance extended reads it` |
| IWC0303 | S | `PIC X({n}) {COMP-X or COMP-5}: {n} bytes of binary, and ironwork's binary items hold at most eight` |
| IWC0304 | S | `{paragraph or section} FOREVER: under --compliance extended PERFORM FOREVER is Micro Focus's and GnuCOBOL's endless loop, not a PERFORM of it; compile the program under strict` |
| IWC0305 | S | `FUNCTION MODULE-CALLER-ID: GnuCOBOL's, not Enterprise COBOL's; --compliance extended reads it` |
| IWC0306 | S | `{STOP RUN or GOBACK} {RETURNING or GIVING}: GnuCOBOL's and Micro Focus's, not Enterprise COBOL's, where a MOVE to RETURN-CODE comes first; --compliance extended reads it` |
| IWC0307 | S | `{name} [NOT] OMITTED: GnuCOBOL's and Micro Focus's, not Enterprise COBOL's, which writes ADDRESS OF {name} = NULL; --compliance extended reads it` |
| IWC0308 | S | `ANY LENGTH: GnuCOBOL's and Micro Focus's, not Enterprise COBOL's; --compliance extended reads it` |
| IWC0309 | S | `PERFORM UNTIL EXIT: GnuCOBOL's and Micro Focus's, not Enterprise COBOL's; --compliance extended reads it` |
| IWC0310 | S | `a file description with no FILE SECTION header: Micro Focus's and GnuCOBOL's, not Enterprise COBOL's; --compliance extended reads it` |
| IWJ0001 | S | `a quoted value continued onto the next line is not supported yet` |
| IWJ0002 | S | `an unbalanced ) in {text}` |
| IWJ0003 | S | `unbalanced parentheses or quotes in {text}` |
| IWJ0004 | S | `& is not followed by a symbolic parameter name in {text}` |
| IWJ0005 | S | `&SYSUID is the user ID the job runs under: USER= on the JOB statement, or the user that submitted it` |
| IWJ0006 | S | `symbolic parameter &{name} has no value` |
| IWJ0007 | S | `the statement is continued past the end of the job` |
| IWJ0008 | S | `a continuation line must start with // and a blank` |
| IWJ0009 | S | `a continued operand must start in columns 4 to 16` |
| IWJ0010 | S | `IF has no THEN` |
| IWJ0011 | S | `no JOB statement` |
| IWJ0012 | S | `the first statement is not a JOB statement` |
| IWJ0013 | S | `the JOB statement needs a job name of one to eight characters` |
| IWJ0014 | S | `a symbolic parameter in the JOB statement's COND is not supported yet` |
| IWJ0015 | S | `COND on the JOB statement takes (code,operator) tests only` |
| IWJ0016 | S | `RESTART is not supported yet` |
| IWJ0017 | S | `TYPRUN is not supported yet` |
| IWJ0018 | S | `JOB keyword {k} is not supported yet` |
| IWJ0019 | S | `{op} is not NAME=value` |
| IWJ0020 | S | `no procedure library holds member {name}` |
| IWJ0021 | S | `member {name}: {item}` |
| IWJ0022 | S | `procedures and INCLUDE members nest more than 15 deep` |
| IWJ0023 | S | `an in-stream PROC needs a name` |
| IWJ0024 | S | `{operation} is out of place` |
| IWJ0025 | S | `JCLLIB takes ORDER=` |
| IWJ0026 | S | `{bad} is not a data set name` |
| IWJ0027 | S | `INCLUDE takes MEMBER=name` |
| IWJ0028 | S | `a DD statement that follows no EXEC statement` |
| IWJ0029 | S | `a second JOB statement; give one job a file` |
| IWJ0030 | S | `{op} statements are not supported yet` |
| IWJ0031 | S | `{op} is not a JCL statement` |
| IWJ0032 | S | `EXEC starts with PGM=, PROC= or a procedure name` |
| IWJ0033 | S | `{name} is not a procedure name` |
| IWJ0034 | S | `an EXEC operand {op} this reader does not know` |
| IWJ0035 | S | `PARMDD is not supported yet` |
| IWJ0036 | S | `procedure {name} does not use symbolic parameter {key}` |
| IWJ0037 | S | `EXEC keyword {key} is not supported yet` |
| IWJ0038 | S | `procedure {name} has no steps` |
| IWJ0039 | S | `PARM.{text}: procedure {name} has no step {text}` |
| IWJ0040 | S | `COND.{text}: procedure {name} has no step {text}` |
| IWJ0041 | S | `{dd} is not a DD name` |
| IWJ0042 | S | `an unnamed DD statement with nothing to concatenate to` |
| IWJ0043 | S | `{number} is not a step name` |
| IWJ0044 | S | `EXEC needs PGM= or a procedure` |
| IWJ0045 | S | `PGM=*.stepname.ddname is not supported yet` |
| IWJ0046 | S | `{value} is not a program name` |
| IWJ0047 | S | `EXEC keyword {k} is not supported yet` |
| IWJ0048 | S | `an EXEC operand {value} this reader does not know` |
| IWJ0049 | S | `*.{path} is not *.ddname, *.stepname.ddname or *.stepname.procstepname.ddname` |
| IWJ0050 | S | `{value} is not a data set name` |
| IWJ0051 | S | `{value} is not a generation of a generation data group` |
| IWJ0052 | S | `{message} is not a member name` |
| IWJ0053 | S | `&&{temp} is not a temporary data set name` |
| IWJ0054 | S | `{base} is not a data set name` |
| IWJ0055 | S | `DISP={value} has more than three subparameters` |
| IWJ0056 | S | `{text} is not a DISP status` |
| IWJ0057 | S | `{text} is not a DISP {normal} disposition` |
| IWJ0058 | S | `LRECL={value} is not a record length` |
| IWJ0059 | S | `a DD operand {value} this reader does not know` |
| IWJ0060 | S | `DLM takes two characters` |
| IWJ0061 | S | `DD keyword {k} is not supported yet` |
| IWJ0062 | S | `DISP applies to a data set` |
| IWJ0063 | S | `the DD statement names no data set, in-stream data, DUMMY or SYSOUT` |
| IWJ0064 | S | `DD {number} overrides a procedure step, but the EXEC before it runs a program` |
| IWJ0065 | S | `{number} is not a DD name` |
| IWJ0066 | S | `DD {number} appears twice in the step` |
| IWJ0067 | S | `*.{path} names a DD that is no data set` |
| IWJ0068 | S | `*.{path} names no earlier DD` |
| IWJ0069 | S | `IF statements nest more than 15 deep` |
| IWJ0070 | S | `a second ELSE for one IF` |
| IWJ0071 | S | `ELSE without IF` |
| IWJ0072 | S | `ENDIF without IF` |
| IWJ0073 | S | `IF without ENDIF` |
| IWJ0074 | S | `{text} is not a return code from 0 to 4095` |
| IWJ0075 | S | `{text} is not a COND operator (GT, GE, EQ, NE, LT or LE)` |
| IWJ0076 | S | `COND test ({text}) is not (code,operator) or (code,operator,stepname)` |
| IWJ0077 | S | `COND={text} is not in parentheses` |
| IWJ0078 | S | `{item} must come last in COND` |
| IWJ0079 | S | `{t} in COND is not a test in parentheses` |
| IWJ0080 | S | `COND takes at most eight tests` |
| IWJ0081 | S | `{character} has no meaning in an IF expression` |
| IWJ0082 | S | `parentheses in an IF expression nest more than 15 deep` |
| IWJ0083 | S | `an IF expression is missing a )` |
| IWJ0084 | S | `an IF expression has {t} where a keyword belongs` |
| IWJ0085 | S | `an IF expression ends early` |
| IWJ0086 | S | `RC needs a comparison operator and a number` |
| IWJ0087 | S | `{word} needs a number after it` |
| IWJ0088 | S | `{value} is not TRUE or FALSE` |
| IWJ0089 | S | `ABENDCC={value} is not Sxxx or Unnnn` |
| IWJ0090 | S | `stepname.ABENDCC is not supported yet` |
| IWJ0091 | S | `{word} is not RC, ABEND, ABENDCC or a stepname.RC, .ABEND or .RUN` |
| IWJ0092 | S | `an IF expression has {at} after its end` |
| IWJ0093 | S | `a {statement} statement inside a procedure` |
| IWJ0094 | S | `PROC {name} has no PEND` |
| IWJ0095 | S | `a {statement} statement in an INCLUDE member is not supported` |
| IWJ0096 | S | `procedure {name} has no step {step}` |
| IWJ0097 | S | `{generation} is not a relative generation` |
| IWJ0098 | S | `a comment is not closed with */` |
| IWJ0099 | S | `a parenthesis is not closed` |
| IWJ0100 | S | `{character} has no meaning here` |
| IWJ0101 | S | `{text} is not a condition code from 0 to 16` |
| IWJ0102 | S | `DO has no END` |
| IWJ0103 | S | `END without DO` |
| IWJ0104 | S | `ELSE inside DO with no IF` |
| IWJ0105 | S | `a command, found {word}` |
| IWJ0106 | S | `SET takes MAXCC=n or LASTCC=n` |
| IWJ0107 | S | `{verb} is out of place` |
| IWJ0108 | S | `the IDCAMS command {value} is not supported yet` |
| IWJ0109 | S | `IF needs a comparison operator` |
| IWJ0110 | S | `IF needs THEN` |
| IWJ0111 | S | `DELETE of a generic name ({word}) is not supported yet` |
| IWJ0112 | S | `DELETE parameter {k} is not supported yet` |
| IWJ0113 | S | `DELETE has {word} where a name belongs` |
| IWJ0114 | S | `DELETE names no entry` |
| IWJ0115 | S | `REPRO parameter {k} is not supported yet` |
| IWJ0116 | S | `REPRO parameter {word} is not supported yet` |
| IWJ0117 | S | `REPRO has {word} where a parameter belongs` |
| IWJ0118 | S | `REPRO needs INFILE or INDATASET, and OUTFILE or OUTDATASET` |
| IWJ0119 | S | `BLDINDEX parameter {k} is not supported yet` |
| IWJ0120 | S | `BLDINDEX parameter {word} is not supported yet` |
| IWJ0121 | S | `BLDINDEX has {word} where a parameter belongs` |
| IWJ0122 | S | `BLDINDEX needs INFILE or INDATASET, and OUTFILE or OUTDATASET` |
| IWJ0123 | S | `{value} is not a DD name` |
| IWJ0124 | S | `KEYS({value}) has a length that is not from 1 to 255` |
| IWJ0125 | S | `RECORDSIZE({value}) needs an average from 1 to the maximum` |
| IWJ0126 | S | `{value} is not an even number of hexadecimal digits` |
| IWJ0127 | S | `{value} is not a key of 1 to 255 characters` |
| IWJ0128 | S | `PRINT writes to OUTFILE, not OUTDATASET` |
| IWJ0129 | S | `PRINT parameter {k} is not supported yet` |
| IWJ0130 | S | `PRINT parameter {word} is not supported yet` |
| IWJ0131 | S | `PRINT has {word} where a parameter belongs` |
| IWJ0132 | S | `{value} is not a data set name or a generic name` |
| IWJ0133 | S | `LEVEL({value}) must not end with *` |
| IWJ0134 | S | `LISTCAT parameter {k} is not supported yet` |
| IWJ0135 | S | `LISTCAT parameter {word} is not supported yet` |
| IWJ0136 | S | `LISTCAT has {word} where a parameter belongs` |
| IWJ0137 | S | `LISTCAT ENTRIES names no entry` |
| IWJ0138 | S | `LIMIT({value}) is not from 1 to 255` |
| IWJ0139 | S | `DEFINE GDG parameter {k} is not supported yet` |
| IWJ0140 | S | `DEFINE GDG has {word} where a parameter belongs` |
| IWJ0141 | S | `{name} is not a generation data group name of 35 characters or fewer` |
| IWJ0142 | S | `DEFINE GDG needs NAME and LIMIT` |
| IWJ0143 | S | `DEFINE CLUSTER {k} is not supported yet` |
| IWJ0144 | S | `{name}: KEYS({length} {offset}) does not fit in a record of {maximum} bytes` |
| IWJ0145 | S | `DEFINE ALTERNATEINDEX {k} is not supported yet` |
| IWJ0146 | S | `DEFINE PATH RECATALOG is not supported yet` |
| IWJ0147 | S | `DEFINE {k} is not supported yet; DEFINE CLUSTER, ALTERNATEINDEX, PATH and GDG are` |
| IWJ0148 | S | `DEFINE has {word} where a parameter belongs` |
| IWJ0149 | S | `DEFINE needs CLUSTER, ALTERNATEINDEX, PATH or GDG` |
| IWJ0150 | S | `the field format {file} is not supported yet` |
| IWJ0151 | S | `{text} is not a {what}` |
| IWJ0152 | S | `X'{text}' is not pairs of hexadecimal digits` |
| IWJ0153 | S | `the constant {token} is not supported yet; C'...', X'...' and decimal numbers are` |
| IWJ0154 | S | `the edit pattern {text} is longer than 44 characters` |
| IWJ0155 | S | `SIGNS=({inner}) has more than four signs` |
| IWJ0156 | S | `the sign {part} is not one character` |
| IWJ0157 | S | `FIELDS=({inner}) is not position, length{format}, order for each field` |
| IWJ0158 | S | `an E order (an exit's own) is not supported yet` |
| IWJ0159 | S | `{o} is not A or D` |
| IWJ0160 | S | `the field format {format} in a condition is not supported yet; CH, BI, FI, ZD and PD are` |
| IWJ0161 | S | `a {format} field of {length} bytes is longer than DFSORT compares` |
| IWJ0162 | S | `comparing {position},{length},{format} with {number} is not supported yet` |
| IWJ0163 | S | `{extra} in a condition is not AND or OR` |
| IWJ0164 | S | `LENGTH={number} is longer than 44` |
| IWJ0165 | S | `{key}: the two digit characters must differ` |
| IWJ0166 | S | `arithmetic in {what} ({key}) is not supported yet` |
| IWJ0167 | S | `a {what} number is either edited or converted, not both` |
| IWJ0168 | S | `SIGNS goes with an edit mask, not TO, in {what}` |
| IWJ0169 | S | `{character} is not a column or a symbol for one` |
| IWJ0170 | S | `the {what} item {token} is not supported yet` |
| IWJ0171 | S | `{what} editing of {file} fields is not supported yet` |
| IWJ0172 | S | `a {file} field of {length} bytes is not one {what} edits` |
| IWJ0173 | S | `arithmetic in {what} ({next}) is not supported yet` |
| IWJ0174 | S | `{what} field conversion and editing ({next}) is not supported yet` |
| IWJ0175 | S | `{what}=() has no items` |
| IWJ0176 | S | `{what}={number} is longer than 15 digits` |
| IWJ0177 | S | `the PUSH item {token} is not supported yet; p,m, ID=n and SEQ=n are` |
| IWJ0178 | S | `PUSH=() has no items` |
| IWJ0179 | S | `KEYBEGIN={value} is not (p,m)` |
| IWJ0180 | S | `{verb} IFTHEN {key} is not supported yet` |
| IWJ0181 | S | `{k} is not an operand of this {verb} IFTHEN clause` |
| IWJ0182 | S | `an IFTHEN clause takes one of BUILD and OVERLAY, in {verb}` |
| IWJ0183 | S | `{verb} IFTHEN WHEN=GROUP needs PUSH=` |
| IWJ0184 | S | `{verb} IFTHEN WHEN=GROUP needs BEGIN, KEYBEGIN, END or RECORDS` |
| IWJ0185 | S | `{verb} IFTHEN WHEN=ANY needs a WHEN=(cond) clause before it` |
| IWJ0186 | S | `{verb} has more than one of BUILD, FIELDS, OUTREC and OVERLAY` |
| IWJ0187 | S | `{verb} takes IFTHEN clauses or BUILD, FIELDS and OVERLAY, not both` |
| IWJ0188 | S | `{verb} IFOUTLEN goes with IFTHEN clauses` |
| IWJ0189 | S | `OUTFIL takes one of INCLUDE, OMIT and SAVE` |
| IWJ0190 | S | `the OUTFIL parameter {k} is not supported yet` |
| IWJ0191 | S | `{bad} is not a ddname` |
| IWJ0192 | S | `more than one SORT or MERGE statement` |
| IWJ0193 | S | `{verb} operand {word} is not supported yet` |
| IWJ0194 | S | `SUM of fields is not supported yet; SUM FIELDS=NONE is` |
| IWJ0195 | S | `OPTION {number} is not supported yet` |
| IWJ0196 | S | `RECORD TYPE={t} is not supported yet` |
| IWJ0197 | S | `more than one INCLUDE or OMIT statement; INCLUDE and OMIT are mutually exclusive` |
| IWJ0198 | S | `the {verb} parameter {word} is not supported yet` |
| IWJ0199 | S | `{verb} needs FIELDS=, BUILD=, OVERLAY= or IFTHEN=` |
| IWJ0200 | S | `more than one {verb} statement` |
| IWJ0201 | S | `the DFSORT {value} statement is not supported yet` |
| IWJ0202 | S | `{value} is not a DFSORT control statement` |
| IWJ0203 | S | `no SORT, MERGE or OPTION COPY statement` |
| IWJ0204 | S | `SYMNAMES line {line}: {text} is not a {what} from 1 to 32752` |
| IWJ0205 | S | `SYMNAMES line {line}: {statement} is not symbol,value` |
| IWJ0206 | S | `SYMNAMES line {line}: ALIGN takes H, F or D, not {value}` |
| IWJ0207 | S | `SYMNAMES line {line}: {name} is not a symbol, or is a reserved word` |
| IWJ0208 | S | `SYMNAMES line {line}: {name} is defined twice` |
| IWJ0209 | S | `SYMNAMES line {line}: {value} is not a closed string` |
| IWJ0210 | S | `SYMNAMES line {line}: {value} is not a decimal number` |
| IWJ0211 | S | `SYMNAMES line {line}: {file} is not a field format` |
| IWJ0212 | S | `SYMNAMES line {line}: {value} is not p,m,f` |
| IWJ0213 | S | `the symbol {name} stands for {what}, which these statements do not model yet` |
| IWJ0214 | S | `the symbol {token} has no format, and no FORMAT= gives one` |
| IWJ0215 | S | `the symbol {token} is a constant, not a field to sort on` |
| IWJ0216 | S | `IF takes MAXCC or LASTCC` |
| IWJ0217 | S | `IF needs a condition code` |
| IWJ0218 | S | `{keyword}({value}) needs {N} whole numbers` |
| IWJ0219 | S | `PRINT needs INFILE or INDATASET` |
| IWJ0220 | S | `DEFINE CLUSTER needs NAME` |
| IWJ0221 | S | `DEFINE ALTERNATEINDEX needs NAME` |
| IWJ0222 | S | `DEFINE ALTERNATEINDEX needs RELATE` |
| IWJ0223 | S | `DEFINE PATH needs NAME` |
| IWJ0224 | S | `DEFINE PATH needs PATHENTRY` |
| IWJ0225 | S | `{verb} needs FIELDS=` |
| IWJ0226 | S | `{verb} needs COND=` |
| IWJ0227 | S | `{token} is not a decimal constant` |
| IWJ0228 | S | `the edit pattern {value} is not in parentheses` |
| IWJ0229 | S | `SIGNS={value} is not in parentheses` |
| IWJ0230 | S | `FIELDS={value} is not in parentheses` |
| IWJ0231 | S | `the field {position},{length} has no format, and no FORMAT= gives one` |
| IWJ0232 | S | `a comparison has no relation` |
| IWJ0233 | S | `{op} is not EQ, NE, GT, GE, LT or LE` |
| IWJ0234 | S | `a comparison has nothing after its relation` |
| IWJ0235 | S | `a condition ends early` |
| IWJ0236 | S | `COND={value} is not in parentheses` |
| IWJ0237 | S | `TO={value} is not BI, FI, PD, PDC, PDF, ZD, ZDF, ZDC, CSF or FS` |
| IWJ0238 | S | `{what}={value} is not in parentheses` |
| IWJ0239 | S | `PUSH={value} is not in parentheses` |
| IWJ0240 | S | `IFTHEN={value} is not in parentheses` |
| IWJ0241 | S | `IFTHEN=({inner}) does not begin with WHEN=` |
| IWJ0242 | S | `SYMNAMES line {line}: = for a position before any position was set` |
| IWJ0243 | S | `SYMNAMES line {line}: = for a length before any length was set` |
| IWJ0244 | S | `SYMNAMES line {line}: = for a format before any format was set` |
| IWJ0245 | S | `a comparison ends early` |
| IWJ0246 | S | `PGM={pgm} is not supported yet` |
| IWJ0247 | S | `PARM for PGM={pgm} is not supported yet` |
| IWJ0248 | S | `DD {dd} concatenates in-stream data with data sets of z/OS records` |
| IWJ0249 | S | `IDCAMS SYSIN from data sets of z/OS records is not supported yet` |
| IWJ0250 | S | `IEBGENER control statements are not supported yet` |
| IWJ0251 | S | `IEBGENER between UTF-8 lines and z/OS records needs a record length, which DCB is not read for yet` |
| IWL0001 | S | `lowering: {table} exceeds the LIR's limit` |
| IWL0002 | S | `lowering: the lowered program is invalid: {why}` |
| IWL0003 | S | `LOCAL-STORAGE exceeds the interpreter's {MAX STORAGE} bytes` |
| IWL0004 | S | `storage exceeds the interpreter's {MAX STORAGE} bytes` |
| IWL0005 | S | `WORKING-STORAGE exceeds the interpreter's {MAX STORAGE} bytes` |
| IWL0006 | S | `an item larger than the interpreter's {MAX STORAGE} bytes` |
| IWO0001 | S | `CBL {option}: {why}` |
| IWO0002 | E | `CBL CURRENCY: code page {codepage} reads its byte as {character}, which cannot be a currency symbol` |
| IWO0003 | W | `CBL NODBCS: NSYMBOL(NATIONAL) requires DBCS, which is in effect` |
| IWO0004 | S | `SOURCE_DATE_EPOCH={value}: not a whole number of seconds from 0 to 253402300799` |
| IWO0005 | S | `{a flag this compiler does not take}` |
| IWP0001 | S | `no mapset {to ascii uppercase} among the {sets} in the file` |
| IWP0002 | I | `IGYPS2091-W not given: the program ends with EXEC CICS {command}, which the CICS translator turns into a CALL; --cics-return-warning=always gives the warning, =never drops this note` |
| IWP0003 | S | `EXEC SQL {command}: {why}` |
| IWP0004 | S | `EXEC DLI {command} is not an EXEC DLI command` |
| IWP0005 | S | `EXEC DLI {command}: {name} is not one of its options` |
| IWP0006 | S | `EXEC DLI {command} WHERE({t}): {why}` |
| IWP0007 | S | `EXEC CICS {command} {option}({label}): {message}` |
| IWP0008 | S | `a program that uses object-oriented syntax cannot contain EXEC CICS` |
| IWP0009 | S | `a continuation line is missing` |
| IWP0010 | S | `a label with no macro` |
| IWP0011 | S | `a quoted string is not closed` |
| IWP0012 | S | `a parenthesised list is not closed` |
| IWP0013 | S | `{key} takes one value` |
| IWP0014 | S | `{key} takes a quoted string` |
| IWP0015 | S | `{key}={word} is not a number from {low} to {high}` |
| IWP0016 | S | `{key}={word}: YES or NO` |
| IWP0017 | S | `DSATTS={a} is not an extended attribute` |
| IWP0018 | S | `EXTATT={word}: NO, MAPONLY or YES` |
| IWP0019 | S | `DFHMSD TYPE=FINAL with no mapset open` |
| IWP0020 | S | `DFHMDI outside a DFHMSD` |
| IWP0021 | S | `DFHMDF outside a DFHMDI map` |
| IWP0022 | S | `unknown macro {word}` |
| IWP0023 | S | `{what} needs a name` |
| IWP0024 | S | `{what} name {name} is longer than {max} characters` |
| IWP0025 | S | `TYPE={t}: DSECT, MAP, FINAL or &SYSPARM` |
| IWP0026 | S | `MODE={word}: IN, OUT or INOUT` |
| IWP0027 | S | `SIZE=(lines,columns), each 1 to 240` |
| IWP0028 | S | `{key}={word} is not a number from 1 to 240` |
| IWP0029 | S | `ATTRB={word} is not an attribute` |
| IWP0030 | S | `XINIT takes an even number of hexadecimal digits` |
| IWP0031 | S | `XINIT takes hexadecimal digits` |
| IWP0032 | S | `GRPNAME needs a labelled field and does not go with OCCURS` |
| IWP0033 | S | `GRPNAME is longer than 30 characters` |
| IWP0034 | S | `LENGTH=0 is allowed only on an unlabelled field, where it delimits an input field` |
| IWP0035 | S | `LENGTH is missing, or is not from 1 to 256` |
| IWP0036 | S | `JUSTIFY={word}: LEFT or RIGHT, BLANK or ZERO` |
| IWP0037 | S | `COLOR={character} is not a colour` |
| IWP0038 | S | `HILIGHT={h} is not a highlight` |
| IWP0039 | S | `POS is an offset within the map, or (line,column) inside it` |
| IWP0040 | S | `unbalanced '(' in DEFINE` |
| IWP0041 | S | `DEFINE names no KIND(NAME)` |
| IWP0042 | S | `{kind}({name}): a {kind} name is at most {limit} characters` |
| IWP0043 | S | `attribute value for '{key}' exceeds 256 characters` |
| IWP0044 | S | `unbalanced '(' in attribute value` |
| IWP0045 | S | `{why the BMS source cannot be read}` |
| IWP0046 | S | `EXEC SQL {command}: {name} in the INTO list has no colon, which Db2 requires before every host variable` |
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
| IWR0053 | S | `{file}: LINAGE on a report file is not supported yet` |
| IWR0054 | S | `BLANK WHEN ZERO on a USAGE NATIONAL item is not supported yet` |
| IWR0055 | S | `a national-edited PICTURE is not supported yet` |
| IWR0056 | S | `a {category} PICTURE with USAGE {usage} is not supported yet` |
| IWR0057 | S | `{a SCREEN SECTION form ironwork does not run} is not supported yet` |
| IWR0075 | S | `FUNCTION {name}: a user-defined function is not supported here yet` |
| IWR0076 | S | `ANY LENGTH on {name}: ironwork reads it on an alphanumeric 01 or 77 parameter, and {why}` |
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
| IWS0032 | S | `FUNCTION-ID {name}: a user-defined function cannot be named {name}` |
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
| IWS0080 | S | `FUNCTION {name}: the REPOSITORY paragraph lists {name} with INTRINSIC, so no user-defined function takes its name here` |
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
| IWS0095 | S | `CURRENCY SIGN {literal} is {character} in the program's code page, which cannot be a PICTURE currency symbol` |
| IWS0096 | S | `CURRENCY SIGN {literal} is {value} in the program's code page, which contains a digit, +, -, . or ,` |
| IWS0097 | S | `{END-DISPLAY or END-ACCEPT}: Micro Focus's and GnuCOBOL's scope terminator, a word Enterprise COBOL does not reserve; --compliance extended reads it` |
| IWS0098 | S | `VALUES in a level-{level} entry: Enterprise COBOL writes VALUES only in a level-88 entry, and VALUE in any other; --compliance extended reads it as VALUE` |
| IWS0099 | E | `{word}: a user-defined word has at most 30 characters, and this one has {count}; it is read as its first 30, {the first 30}` |
| IWS0100 | E | `{word} begins in Area A, where Enterprise COBOL puts no statement: it is read as though it began in Area B` |
| IWS0101 | S | `{FLOAT-SHORT or FLOAT-LONG}: GnuCOBOL's and Micro Focus's floating point, not Enterprise COBOL's; --compliance extended reads it as {COMP-1 or COMP-2}` |
| IWS0102 | S | `the Communication feature ({a COMMUNICATION SECTION item or statement}) is not part of Enterprise COBOL, which does not compile it` |
| IWX0001 | W | `free-form source (Micro Focus and GnuCOBOL; Enterprise COBOL reads fixed form alone): {why the file is read in free form}` |
| IWX0002 | W | `constant entry (Micro Focus and GnuCOBOL; Enterprise COBOL has no level 78 and no CONSTANT clause): {name} stands for its value wherever it is used after this entry` |
| IWX0003 | W | `<> (Micro Focus and GnuCOBOL; Enterprise COBOL writes NOT =) is read as NOT =` |
| IWX0004 | W | `literal concatenation with & (Micro Focus and GnuCOBOL; Enterprise COBOL has none): the literals on either side are one literal` |
| IWX0005 | W | `{the COBOL 2002 or GnuCOBOL binary usage}: {usage} is read as PIC {picture} COMP-5` |
| IWX0006 | W | `PROGRAM-ID with no IDENTIFICATION DIVISION header before it (COBOL 2002, Micro Focus and GnuCOBOL; Enterprise COBOL requires the header): the program reads as though IDENTIFICATION DIVISION. came before it` |
| IWX0007 | W | `ASSIGN to a data item (Micro Focus and GnuCOBOL; Enterprise COBOL's assignment-name is never a data item): each OPEN of {file} takes its DD name from {item}` |
| IWX0008 | W | `an integer or numeric function as a MOVE's sender (GnuCOBOL; Enterprise COBOL takes one only where an arithmetic expression can be): FUNCTION {name} is moved as its value` |
| IWX0009 | W | `PROCEDURE DIVISION RETURNING OMITTED (GnuCOBOL; Enterprise COBOL's RETURNING names an 01 or 77 item of the LINKAGE SECTION): the program is read with no RETURNING phrase, and returns its RETURN-CODE to its caller as any program does` |
| IWX0010 | W | `{ACCEPT ... FROM COMMAND-LINE, ARGUMENT-NUMBER or ARGUMENT-VALUE, or DISPLAY ... UPON ARGUMENT-NUMBER} (Micro Focus and GnuCOBOL; Enterprise COBOL reads no command line): {what the job step's PARM program arguments give}` |
| IWX0011 | W | `an INTO name written without its colon (Db2 13 for z/OS requires the colon before every host variable): {name} is read as a host variable` |
| IWX0012 | W | `a numeric literal as a numeric-edited item's VALUE (Micro Focus and GnuCOBOL; Enterprise COBOL takes an alphanumeric literal in edited form): {name} starts as the literal moved to it` |
| IWX0013 | W | `{END-DISPLAY or END-ACCEPT} (Micro Focus and GnuCOBOL; Enterprise COBOL does not reserve the word): it ends the {DISPLAY or ACCEPT} statement` |
| IWX0014 | W | `VALUES outside a level-88 entry (Micro Focus; Enterprise COBOL writes VALUE there): it is read as VALUE` |
| IWX0015 | W | `a user-defined word of more than 30 characters (Micro Focus and GnuCOBOL; Enterprise COBOL reads its first 30): {word} is read whole` |
| IWX0016 | W | `BINARY-CHAR (Micro Focus and GnuCOBOL; Enterprise COBOL's binary items are two, four or eight bytes): {name} is one byte of binary, {range}` |
| IWX0017 | W | `a statement in Area A (Micro Focus and GnuCOBOL; Enterprise COBOL puts statements in Area B): {word} is read as though it began in Area B` |
| IWX0018 | W | `a numeric argument to FUNCTION {name} (GnuCOBOL; Enterprise COBOL takes an alphabetic, alphanumeric or national one): {item}'s digits are read as its characters` |
| IWX0019 | W | `OCCURS at level {level} (Micro Focus and GnuCOBOL; Enterprise COBOL takes OCCURS only at levels 02 to 49): {name} is read as a table in a record of its own` |
| IWX0020 | W | `{DISPLAY or ACCEPT} on the screen (Micro Focus and GnuCOBOL; Enterprise COBOL has none): at {where}` |
| IWX0021 | W | `{the environment form} (Micro Focus and GnuCOBOL; Enterprise COBOL reads and sets no environment variable): {what it does}` |
| IWX0022 | W | `{a locking phrase} (Micro Focus and GnuCOBOL; Enterprise COBOL has no record locks of its own): the run unit is the file's only user, so nothing it locks waits and the phrase changes nothing` |
| IWX0023 | W | `INSPECT ... TRAILING (GnuCOBOL; Enterprise COBOL has ALL, LEADING, FIRST and CHARACTERS): the occurrences that run on to the end of the phrase's region` |
| IWX0024 | W | `CALL ... RETURNING {OMITTED, NOTHING or NULL} (GnuCOBOL; Enterprise COBOL's RETURNING names a data item): the CALL leaves the caller's RETURN-CODE as it was` |
| IWX0025 | W | `COMP-X (Micro Focus; Enterprise COBOL's binary items are two, four or eight bytes): {name} is {n} bytes of binary, {range}, shown in {digits} digits` |
| IWX0026 | W | `PIC X(n) COMP-5 (Micro Focus and GnuCOBOL; Enterprise COBOL's COMP-5 takes a numeric PICTURE): {name} is {n} bytes of binary, {range}` |
| IWX0027 | W | `{FLOAT-SHORT or FLOAT-LONG} (GnuCOBOL and Micro Focus; Enterprise COBOL writes COMP-1 and COMP-2): it is read as {COMP-1 or COMP-2}, IBM's hexadecimal floating point` |
| IWX0028 | W | `PERFORM ... FOREVER (Micro Focus and GnuCOBOL; Enterprise COBOL has no FOREVER phrase): it repeats until EXIT PERFORM, GO TO, GOBACK or STOP RUN leaves it` |
| IWX0029 | W | `ACCEPT ... FROM {LINES, COLUMNS or COLS} (GnuCOBOL; Enterprise COBOL has no screen): the screen's {24 lines or 80 columns}` |
| IWX0030 | W | `WRITE ... BEFORE ADVANCING on the line-sequential file {file} (GnuCOBOL and Micro Focus; Enterprise COBOL allows only AFTER there): the line, then the lines or page it names` |
| IWX0031 | W | `FUNCTION MODULE-CALLER-ID (GnuCOBOL; Enterprise COBOL has no such function): the PROGRAM-ID of the program that called this one, empty in the main program` |
| IWX0032 | W | `a level-66 entry before the end of its record (GnuCOBOL's IBM and Micro Focus dialects; Enterprise COBOL writes a record's RENAMES entries after its last entry): {name} is read as following {record}'s last entry` |
| IWX0033 | W | `{STOP RUN or GOBACK} {RETURNING or GIVING} (GnuCOBOL and Micro Focus; Enterprise COBOL moves the value to RETURN-CODE first): the value is moved to RETURN-CODE, then {STOP RUN or GOBACK} ends the program` |
| IWX0034 | W | `{name} [NOT] OMITTED (GnuCOBOL and Micro Focus; Enterprise COBOL writes ADDRESS OF {name} = NULL): it is read as ADDRESS OF {name} = NULL, true when the caller passed OMITTED or no argument there` |
| IWX0035 | W | `ANY LENGTH (GnuCOBOL and Micro Focus; Enterprise COBOL's parameters have the length their entries give): {name} is as long as the argument each CALL passes for it` |
| IWX0036 | W | `START KEY {< or NOT > or <=} (Micro Focus and GnuCOBOL; Enterprise COBOL's START takes =, >, NOT < or >=): the file is positioned at the last record whose key is {that} the value, which READ NEXT or READ PREVIOUS reads first` |
| IWX0037 | W | `a file description with no FILE SECTION header (Micro Focus and GnuCOBOL; Enterprise COBOL writes FILE SECTION first): it is read as though FILE SECTION came first` |
| IWX0038 | W | `PERFORM UNTIL EXIT (GnuCOBOL and Micro Focus; Enterprise COBOL has no such condition): it repeats until EXIT PERFORM, GO TO, GOBACK or STOP RUN leaves it` |
| IWX0039 | W | `ASSIGN TO DISK (Micro Focus and GnuCOBOL; Enterprise COBOL's ASSIGN names a DD): DISK is the device, and what follows names the file` |

## Run-time refusals

A run that reaches a construct ironwork does not run ends with one of these, its id and
severity S before the text, under the abend code IRONWORK, EXEC or JAVA.

| Id | Severity | Text |
|---|---|---|
| IWR0058 | S | `EXEC CICS {command} is not supported yet` |
| IWR0059 | S | `EXEC CICS FORMATTIME {option} is not supported` |
| IWR0060 | S | `EXEC {kind} {command} was reached: ironwork for COBOL checks EXEC statements but does not run them yet` |
| IWR0061 | S | `EXEC SQL {verb} was reached: ironwork for COBOL does not run {statement}` |
| IWR0062 | S | `{what} was reached: {class} is a Java class, and ironwork for COBOL checks Java classes but has no JVM to run them` |
| IWR0063 | S | `CALL {name} was reached: {service} is a JNI service, and ironwork for COBOL has no JVM to run it` |
| IWR0064 | S | `FUNCTION {name} is not supported yet` |
| IWR0065 | S | `FUNCTION LENGTH of this argument is not supported yet` |
| IWR0066 | S | `DISPLAY of a floating-point value is not supported yet` |
| IWR0067 | S | `DISPLAY of a pointer, index or object reference is not supported` |
| IWR0068 | S | `ADVANCING {name} on {file}, whose FD has LINAGE, is not supported yet` |
| IWR0070 | S | `this BY VALUE argument is not supported` |
| IWR0071 | S | `this INVOKE argument is not supported` |
| IWR0072 | S | `{statement}: {name} is a Language Environment callable service that ironwork for COBOL does not provide yet` |
| IWR0073 | S | `the VM does not run {construct} yet; run it with --interpret` |
| IWR0074 | S | `{file}, a GLOBAL file of {declarer}, is written as a print file in one of {declarer} and {program} and not the other, which is not supported yet` |
