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
| IWC0047 | S | `ALPHABET {name}: {message}` |
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
| IWC0107 | S | `FUNCTION {file}: a user-defined function is not supported here yet` |
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
| IWL0001 | S | `lowering: {table} exceeds the LIR's limit` |
| IWL0002 | S | `lowering: the lowered program is invalid: {why}` |
| IWL0003 | S | `LOCAL-STORAGE exceeds the interpreter's {MAX STORAGE} bytes` |
| IWL0004 | S | `storage exceeds the interpreter's {MAX STORAGE} bytes` |
| IWL0005 | S | `WORKING-STORAGE exceeds the interpreter's {MAX STORAGE} bytes` |
| IWL0006 | S | `an item larger than the interpreter's {MAX STORAGE} bytes` |
| IWO0001 | S | `CBL {option}: {why}` |
| IWO0002 | E | `CBL CURRENCY: code page {codepage} reads its byte as {character}, which cannot be a currency symbol` |
| IWO0003 | W | `CBL NODBCS: NSYMBOL(NATIONAL) requires DBCS, which is in effect` |
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
