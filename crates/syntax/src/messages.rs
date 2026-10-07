//! ironwork's message catalogue: each compile-time message's id, the severity it is given and its
//! text, the parts a site fills in between braces. rt::refusal holds the run-time half, and
//! docs/messages.md is written from both. Once released, an id keeps its meaning and is never given
//! to another message; its wording may change.

use crate::{Error, Pos, Severity};

/// Why something is refused before its place is known: the catalogue's message and its text.
pub type Refused = (Message, String);

/// A refusal as a line shows it: `IWJ0001-S text`.
pub fn labelled((message, text): &Refused) -> String {
    format!("{}-{} {text}", message.id, message.severity.letter())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Message {
    pub id: &'static str,
    pub severity: Severity,
    pub text: &'static str,
}

impl Message {
    /// The message at `pos`, its text as the site words it.
    pub fn at(&self, pos: Pos, text: impl Into<String>) -> Error {
        Error { id: Some(self.id), ..Error::at(pos, text).graded(self.severity) }
    }
}

/// The area an id's third letter names.
pub const AREAS: &[(char, &str)] = &[
    ('S', "Source form, lexing, COPY and REPLACE, and syntax"),
    ('C', "Enterprise COBOL's compile rules"),
    ('O', "CBL and PROCESS options, and compiler flags"),
    ('P', "EXEC SQL, EXEC CICS, EXEC DLI, BMS and CSD"),
    ('R', "An Enterprise COBOL construct ironwork does not run yet, refused by name"),
    ('L', "ironwork's own limits"),
    ('X', "An extension `--compliance extended` reads (docs/compliance.md)"),
    ('J', "JCL a job run refuses"),
];

macro_rules! catalogue {
    ($($id:ident $severity:ident $text:literal;)*) => {
        $(pub const $id: Message = Message { id: stringify!($id), severity: Severity::$severity, text: $text };)*
        /// Every message, in id order.
        pub const CATALOGUE: &[Message] = &[$($id),*];
    };
}

catalogue! {
    IWC0001 Severe "{name} is not defined";
    IWC0002 Severe "{name} is ambiguous; qualify it with OF or IN";
    IWC0003 Severe "no paragraph or section named {name}";
    IWC0004 Severe "{name} names more than one paragraph; qualify it with OF and its section";
    IWC0005 Severe "CLASS {clause}: {message}";
    IWC0006 Severe "{verb} CORRESPONDING {name}: {why}";
    IWC0007 Severe "CORRESPONDING {group ref}: {name} in it cannot be named uniquely";
    IWC0008 Severe "USE AFTER EXCEPTION/ERROR ON {name}: {why}";
    IWC0009 Severe "USE AFTER EXCEPTION/ERROR ON {mode}: another procedure is for the same open mode";
    IWC0010 Severe "USE FOR DEBUGGING is not allowed in a program compiled with THREAD";
    IWC0011 Severe "USE FOR DEBUGGING ON ALL PROCEDURES: it may be written once, and no other USE FOR DEBUGGING may name a procedure";
    IWC0012 Severe "USE FOR DEBUGGING ON {name}: {why}";
    IWC0013 Severe "{name}: a debugging section may refer only to declarative procedures";
    IWC0014 Severe "{name}: only a debugging section may refer to a procedure in a debugging section";
    IWC0015 Severe "PERFORM ... THRU: a declarative procedure and the other end of the range must be in the same declarative section";
    IWC0016 Severe "FUNCTION-ID {name}: {name} is not an 01 or 77 item of the LINKAGE SECTION";
    IWC0017 Severe "FUNCTION-ID {name}: a user-defined function needs PROCEDURE DIVISION RETURNING";
    IWC0018 Severe "EXEC {kind}: SQL and CICS cannot be used with user-defined functions, so neither in one nor in a program after one in its source (assumption C273)";
    IWC0019 Severe "PROCEDURE DIVISION USING BY VALUE {file}: a function's BY VALUE parameter is binary, floating-point, a pointer, or one alphanumeric or national character";
    IWC0020 Severe "FUNCTION-ID {id}: {why} from the prototype at line {line}";
    IWC0021 Severe "FUNCTION {name} takes {params} arguments, not {args}";
    IWC0022 Severe "FUNCTION {name}: a user-defined function's argument is an identifier, a literal or an arithmetic expression";
    IWC0023 Severe "FUNCTION {name}: only an alphanumeric or national function's value can be reference-modified";
    IWC0024 Severe "FUNCTION {name} argument {k}: a function's argument is not a figurative constant";
    IWC0025 Severe "FUNCTION {name} argument {k}: {formal} is numeric, and takes an argument COMPUTE could send it (assumption C272)";
    IWC0026 Severe "FUNCTION {name} argument {k} ({name}): {why}";
    IWC0027 Severe "OCCURS at level {level}: Enterprise COBOL takes OCCURS only at levels 02 to 49";
    IWC0028 Severe "a level-88 entry after a level-66 entry: a RENAMES item cannot be a conditional variable";
    IWC0029 Severe "a level-88 entry with no item before it";
    IWC0030 Severe "a level-88 entry needs a name";
    IWC0031 Severe "RENAMES goes with level 66, and level 66 with RENAMES";
    IWC0032 Severe "a level-66 entry must follow the entries of a level-01 record";
    IWC0033 Severe "a level-66 entry has a name and a RENAMES clause, and nothing else";
    IWC0034 Severe "level {level} is not a data level";
    IWC0035 Severe "level {level} after a level-66 entry: a record's RENAMES entries follow its last entry";
    IWC0036 Severe "level {level} with no group to belong to";
    IWC0037 Severe "REDEFINES {target}: no earlier 01-level item of that name";
    IWC0038 Severe "OCCURS 0 is not a table";
    IWC0039 Severe "RENAMES {name}: {message}";
    IWC0040 Severe "RENAMES {first}: a level-66 entry cannot rename a level-01 record";
    IWC0041 Severe "RENAMES {first} THRU {last}: the last item cannot be within the first";
    IWC0042 Severe "RENAMES {first} THRU {last}: the last item must start and end no earlier than the first";
    IWC0043 Severe "RENAMES {first} THRU {last}: no OCCURS DEPENDING ON between them";
    IWC0044 Severe "REDEFINES {target}: no earlier item of that name at this level";
    IWC0045 Severe "a SYNCHRONIZED item at the start of a REDEFINES would need {slack} slack bytes: the redefined item must be on a {message}-byte boundary";
    IWC0046 Severe "{clause} names {name}, which is not a file";
    IWC0048 Severe "OCCURS DEPENDING ON {object}: the object cannot follow an OCCURS DEPENDING ON table in its record";
    IWC0049 Severe "OCCURS DEPENDING ON {object}: not a numeric data item";
    IWC0050 Severe "PROCEDURE DIVISION USING {param}: not an 01 or 77 item of the LINKAGE SECTION";
    IWC0051 Severe "PROCEDURE DIVISION RETURNING {name}: not an 01 or 77 item of the LINKAGE SECTION";
    IWC0052 Severe "ASSIGN USING or DYNAMIC {name}: a Micro Focus and GnuCOBOL form; --compliance extended reads it";
    IWC0053 Severe "ASSIGN {name}: not a data item";
    IWC0054 Severe "ASSIGN {name}: the item holding the file's name must be alphanumeric or a group";
    IWC0055 Warning "no STOP RUN, GOBACK or EXIT PROGRAM in the program: check that it ends";
    IWC0056 Warning "CALL 'CEECBLDY' under INTDATE(LILIAN): CEECBLDY gives an ANSI integer date, which nothing can use under LILIAN, so the CALL is to CEEDAYS";
    IWC0057 Severe "ENTRY '{item}': a program with PROCEDURE DIVISION RETURNING cannot have ENTRY statements";
    IWC0058 Severe "ENTRY '{item}': the name is already the program's or another ENTRY's";
    IWC0059 Severe "ENTRY '{item}' USING {param}: not an 01 or 77 item of the LINKAGE SECTION";
    IWC0060 Severe "ENTRY '{name}' must be a sentence of its own, not inside another statement";
    IWC0061 Severe "a GO TO with no procedure-name cannot be used in {why}";
    IWC0062 Severe "a GO TO with no procedure-name must be its paragraph's only sentence";
    IWC0063 Severe "ALTER cannot be used in {why}";
    IWC0064 Severe "ALTER {name}: a section, where ALTER names a paragraph";
    IWC0065 Severe "ALTER {name}: the paragraph must hold one sentence, a GO TO without DEPENDING ON";
    IWC0066 Severe "{name}: a DBCS item's VALUE is a DBCS literal of at most {size} characters, SPACE or ALL with a DBCS literal";
    IWC0067 Severe "{name}: a DBCS literal can be the VALUE of a DBCS item only";
    IWC0068 Severe "VALUE of {name}: {what}, where a numeric item's VALUE literal must be numeric";
    IWC0069 Severe "PICTURE {name}: {positions} digit positions, more than the {max} {option} allows";
    IWC0070 Severe "the literal {t} has more than the {max} digits {option} allows";
    IWC0071 Severe "a condition as the WHEN object of a value subject";
    IWC0072 Severe "a value as the WHEN object of a TRUE, FALSE or condition subject";
    IWC0073 Severe "DISPLAY UPON {upon}: {why}";
    IWC0074 Severe "CLOSE {name}: REEL, UNIT and NO REWIND are not valid for an indexed or relative file";
    IWC0075 Severe "{verb} {record}: not a record of a file";
    IWC0076 Severe "START KEY takes =, >, NOT < or >=";
    IWC0077 Severe "INITIALIZE {name}: a level-66 RENAMES item cannot be initialized";
    IWC0078 Severe "INITIALIZE {name}: a variably located item, or a group holding one, cannot be initialized (Language Reference p. 351)";
    IWC0079 Severe "SET {name} TO FALSE: the condition-name has no WHEN SET TO FALSE value";
    IWC0080 Severe "{exit} must be inside an inline PERFORM";
    IWC0081 Severe "PERFORM VARYING {var}: not a numeric elementary item or an index-name";
    IWC0082 Severe "PERFORM VARYING {var} {phrase}: an arithmetic expression, where {phrase} takes an identifier, index-name or literal";
    IWC0083 Severe "no file named {name}";
    IWC0084 Severe "{verb} {name}: not an indexed or relative file";
    IWC0085 Severe "{verb} {name}: the file's ACCESS MODE is RANDOM";
    IWC0086 Severe "class-name {name} tests a data item, not an expression";
    IWC0087 Severe "class-name {name} tests a data item of USAGE DISPLAY, and {name} is not one";
    IWC0088 Severe "{key}: not a key of {file}";
    IWC0089 Severe "{file}: an indexed file needs a RECORD KEY";
    IWC0090 Severe "{name}: a key of {file} must be in its records";
    IWC0091 Severe "{name}: the RELATIVE KEY of {file} must not be in its records";
    IWC0092 Severe "{file}: random or dynamic access needs a RELATIVE KEY";
    IWC0093 Severe "{name}: the DEPENDING ON item of {file} must be an elementary unsigned integer";
    IWC0094 Severe "{name}: the PASSWORD of {file} must be an alphabetic, alphanumeric or alphanumeric-edited item of WORKING-STORAGE";
    IWC0095 Severe "{name} is a condition-name, not a data item";
    IWC0096 Severe "{name}: only a debugging section may reference DEBUG-ITEM";
    IWC0097 Severe "{name} takes {dims} subscripts, not {subscripts}";
    IWC0098 Warning "INITIALIZE {name}: none of its items is of a category REPLACING names ({categories}), so it is not initialized";
    IWC0099 Severe "INSPECT FUNCTION {name}: an integer or numeric function can be used only where an arithmetic expression can, not as the inspected item";
    IWC0100 Severe "INSPECT FUNCTION {name} {phrase}: {phrase} stores into the inspected item, and a function-identifier cannot be a receiving operand";
    IWC0101 Severe "DISPLAY FUNCTION {name}: an integer or numeric function can be used only where an arithmetic expression can, and DISPLAY takes none";
    IWC0102 Severe "MOVE FUNCTION {name}: an integer or numeric function can be used only where an arithmetic expression can, not as a MOVE's sender";
    IWC0103 Severe "INSPECT {name}: {operand} cannot be an operand here, since {why}";
    IWC0104 Severe "SEARCH {table} VARYING {value}: not an index-name, an index data item or an elementary integer item";
    IWC0105 Severe "the literal {t} has more than {min} digits";
    IWC0106 Severe "FUNCTION {file}: neither an intrinsic function nor a user-defined function defined or prototyped before this program";
    // IWC0107 is not given to another message: releases up to 0.8.0 used it for what IWR0075 says.
    IWC0108 Severe "WRITE ... END-OF-PAGE: the FD of {file} has no LINAGE clause";
    IWC0109 Severe "LINAGE-COUNTER can be read, but no statement can change it";
    IWC0110 Severe "NUMCHECK: {name} {why} wherever this statement reads it: its VALUE clauses give it X'{hex}' and no statement changes it, so the test is removed (see {NUMCHECK ALWAYS FAILS})";
    IWC0111 Warning "NORENT conflicts with {and}, which IBM compiles only as RENT (see {OO OPTIONS REQUIRED})";
    IWC0112 Warning "{who} uses object-oriented syntax, which IBM compiles only with THREAD, DLL, RENT and DBCS: {missing} missing from its CBL or PROCESS cards (see {OO OPTIONS REQUIRED} and {OO OPTIONS SEVERITY})";
    IWC0113 Warning "INITIAL conflicts with THREAD, which IBM compiles only as NOINITIAL (see {INITIAL UNDER THREAD})";
    IWC0114 Severe "{who} is compiled with THREAD, which requires RECURSIVE in its PROGRAM-ID paragraph";
    IWC0115 Severe "{who} is INITIAL, which THREAD does not allow";
    IWC0116 Severe "{who} contains program {inner}, and THREAD does not allow nested programs";
    IWC0117 Severe "{verb} is not allowed in a program compiled with THREAD";
    IWC0118 Severe "not a class definition";
    IWC0119 Severe "{inherits}: the class a class INHERITS must be named in its REPOSITORY paragraph";
    IWC0120 Severe "class {def} cannot inherit from itself";
    IWC0121 Severe "{factory} method \"{message}\" has the same parameter types as {factory} method \"{twin}\"";
    IWC0122 Severe "method \"{name}\" receives {param} BY REFERENCE: a method's parameters are BY VALUE";
    IWC0123 Severe "method \"{name}\" parameter {param}: {message}";
    IWC0124 Severe "method \"{name}\" parameter {param}: not a record of the method's own LINKAGE SECTION";
    IWC0125 Severe "SET ADDRESS OF {name}: FACTORY and OBJECT data is WORKING-STORAGE, not LINKAGE";
    IWC0126 Severe "method \"{name}\" RETURNING {name}: {message}";
    IWC0127 Severe "method \"{name}\" RETURNING {name}: not a record of the method's own LINKAGE SECTION";
    IWC0128 Severe "OBJECT REFERENCE {character}: the class must be named in the REPOSITORY paragraph";
    IWC0129 Severe "a class definition cannot contain EXEC statements";
    IWC0130 Severe "EXIT METHOD can be used only in a method";
    IWC0131 Severe "EXIT PROGRAM cannot be used in a method: use EXIT METHOD or GOBACK";
    IWC0132 Severe "SELF can be used only in a method";
    IWC0133 Severe "{name} is {what}: it can be used only in SET, INVOKE, CALL and a relation condition";
    IWC0134 Severe "JNIENVPTR cannot receive a value";
    IWC0135 Severe "SELF cannot receive a value";
    IWC0136 Severe "object references and function-pointers compare only as equal or not equal";
    IWC0137 Severe "an object reference compares with another object reference, SELF or NULL; a function-pointer with another or NULL";
    IWC0138 Severe "SET {name} TO ENTRY: the receiver must be a procedure-pointer or function-pointer";
    IWC0139 Severe "SET {names} TO ENTRY: {fault}";
    IWC0140 Severe "SET {name} TO: {message}";
    IWC0141 Severe "FUNCTION {file}: a figurative constant is an argument only inside an arithmetic expression";
    IWC0142 Severe "FUNCTION {file}: {name} is a pointer or object reference, where an argument is alphabetic, alphanumeric, national or numeric";
    IWC0143 Severe "{first} compared with {second}: an arithmetic expression or a numeric function is compared only with a numeric operand";
    IWC0144 Severe "WRITE ... ADVANCING: {file} is not a sequential file";
    IWC0145 Severe "WRITE ... BEFORE ADVANCING, or ADVANCING a mnemonic-name, is not allowed for the line-sequential file {file}";
    IWC0146 Severe "report {name} is named in no FD's REPORT clause";
    IWC0147 Severe "report {ri}: a line reaches column {end}, beyond the {width} bytes of the report file's record";
    IWC0148 Severe "report {ri}: a line reaches column {end}, beyond LINE LIMIT {limit}";
    IWC0149 Severe "a group entry in a report group cannot have COLUMN, PICTURE, SOURCE, VALUE or SUM";
    IWC0150 Severe "a SUM entry needs a PICTURE";
    IWC0151 Severe "a VALUE entry with a figurative constant or ALL needs a PICTURE";
    IWC0152 Severe "a printed SOURCE entry needs a PICTURE";
    IWC0153 Severe "a report entry with no SOURCE, VALUE or SUM needs a data-name for the program to fill it";
    IWC0154 Severe "BLANK WHEN ZERO needs a numeric PICTURE";
    IWC0155 Severe "a SUM entry needs a numeric PICTURE";
    IWC0156 Severe "a COLUMN with no LINE above it";
    IWC0157 Severe "a report field that starts left of column 1";
    IWC0158 Severe "USE BEFORE REPORTING {group}: the program has no REPORT SECTION";
    IWC0159 Severe "USE BEFORE REPORTING {group}: no report group has that name";
    IWC0160 Severe "USE BEFORE REPORTING {group}: more than one report group has that name; qualify it with IN";
    IWC0161 Severe "an arithmetic SOURCE, or ROUNDED, needs a numeric PICTURE";
    IWC0162 Severe "an absolute LINE needs a PAGE LIMIT";
    IWC0163 Severe "a report group whose first LINE is relative must have only relative LINEs";
    IWC0164 Severe "absolute LINE numbers in a report group must increase";
    IWC0165 Severe "NEXT PAGE needs a PAGE LIMIT";
    IWC0166 Severe "a PAGE HEADING or PAGE FOOTING cannot begin on the NEXT PAGE";
    IWC0167 Severe "report {name}: its report control area could not be laid out";
    IWC0168 Severe "a CONTROL HEADING or FOOTING must name its control when the report has several";
    IWC0169 Severe "a CONTROL HEADING or FOOTING names a control that is not in the report's CONTROL clause";
    IWC0170 Severe "report {name} has two {kind} groups for the same level";
    IWC0171 Severe "a PAGE HEADING or PAGE FOOTING needs a PAGE LIMIT";
    IWC0172 Severe "NEXT GROUP with a line or NEXT PAGE needs a PAGE LIMIT";
    IWC0173 Severe "NEXT GROUP is not allowed in a PAGE HEADING or REPORT FOOTING";
    IWC0174 Severe "report {name} has no CONTROL HEADING, DETAIL or CONTROL FOOTING group";
    IWC0175 Severe "RESET ON names a control that is not in the report's CONTROL clause";
    IWC0176 Severe "SUM ... UPON {name}: not a DETAIL group of report {name}";
    IWC0177 Severe "SUM {operand}: the entry summed must be numeric";
    IWC0178 Severe "SUM {operand}: an entry the program fills itself cannot be summed";
    IWC0179 Severe "SUM {operand}: not a numeric data item";
    IWC0180 Severe "SUM entries of a report group total each other in a circle";
    IWC0181 Severe "HEADING, FIRST DETAIL, LAST DETAIL and FOOTING need a PAGE LIMIT";
    IWC0182 Severe "report {name}: the page regions must run HEADING <= FIRST DETAIL <= LAST DETAIL <= FOOTING <= PAGE LIMIT";
    IWC0183 Severe "{number} is not a report of this program";
    IWC0184 Severe "GENERATE {name}: summary reporting needs a CONTROL HEADING or CONTROL FOOTING group";
    IWC0185 Severe "GENERATE {name}: not a DETAIL group";
    IWC0186 Severe "GENERATE {name}: no report or DETAIL group of that name";
    IWC0187 Severe "GENERATE {name}: more than one report has a DETAIL group of that name; qualify it with IN";
    IWC0188 Severe "{name} is a reserved word, so it cannot name {what}";
    IWC0189 Severe "{name}: EXTERNAL is not allowed in the {section} SECTION";
    IWC0190 Severe "{name}: EXTERNAL goes on a level-01 entry";
    IWC0191 Severe "{name}: EXTERNAL and REDEFINES cannot be in the same entry";
    IWC0192 Severe "an EXTERNAL record needs a data-name, not FILLER";
    IWC0193 Severe "{name}: another EXTERNAL record of the program has the same name";
    IWC0194 Severe "{name}: GLOBAL goes on a level-01 entry";
    IWC0195 Severe "a GLOBAL record needs a data-name, not FILLER";
    IWC0196 Severe "{name}: another GLOBAL record of the DATA DIVISION has the same name";
    IWC0197 Severe "{name}: an item of EXTERNAL record {name} takes no VALUE clause";
    IWC0198 Severe "FD {file}: a record of an EXTERNAL or GLOBAL file needs a data-name, not FILLER";
    IWC0199 Severe "{item}: {size} bytes, larger than the EXTERNAL record {name} it redefines";
    IWC0200 Severe "SET ADDRESS OF {name}: an EXTERNAL or GLOBAL record is not a LINKAGE record of the program";
    IWC0201 Severe "RELEASE {record}: not a record of a sort file (SD)";
    IWC0202 Severe "no file named {file}";
    IWC0203 Severe "RETURN {file}: not a sort or merge file (SD)";
    IWC0204 Severe "COLLATING SEQUENCE {alphabet}: not an alphabet-name of SPECIAL-NAMES";
    IWC0205 Severe "SET {name} TO {OFF}: {name} is not the mnemonic-name of an UPSI switch";
    IWC0206 Severe "{name} is the mnemonic-name of UPSI-{number}: only SET ... TO ON or OFF and a condition-name's qualifier can name it";
    IWC0207 Severe "SET {name} TO TRUE: the UPSI switch's entry has no mnemonic-name, which would be its conditional variable";
    IWC0208 Severe "{verb} {name}: not a sort or merge file (SD)";
    IWC0209 Severe "{verb} {name}: no ASCENDING or DESCENDING KEY";
    IWC0210 Severe "{verb} {name}: KEY needs a data name";
    IWC0211 Severe "{key}: a key of {verb} {name} must be in its records";
    IWC0212 Severe "{key}: a sort key cannot be in a table";
    IWC0213 Severe "{key}: a sort key cannot follow an OCCURS DEPENDING ON table in its record";
    IWC0214 Severe "{key}: a POINTER, INDEX, object reference or function-pointer item cannot be a sort key";
    IWC0215 Severe "{verb} {name}: no {phrase} or {procedure}";
    IWC0216 Severe "{phrase} {file}: a sort or merge file (SD) cannot be one";
    IWC0217 Severe "{phrase} {file}: the file's ACCESS MODE is RANDOM";
    IWC0218 Severe "MERGE {name}: USING names at least two files";
    IWC0219 Severe "MERGE {name}: not a merge file (SD)";
    IWC0220 Severe "SORT {name}: a table SORT takes no USING, GIVING or procedures";
    IWC0221 Severe "SORT {name}: not a table";
    IWC0222 Severe "no file or table named {name}";
    IWC0223 Severe "SORT {name}: not a table (no OCCURS)";
    IWC0224 Severe "SORT {name}: a subscript for each table that contains it, and none for itself";
    IWC0225 Severe "SORT {name}: no KEY phrase, and its OCCURS has none";
    IWC0226 Severe "{key}: a key of SORT {name} must be its element or an item within it";
    IWC0227 Severe "{key}: a table SORT key cannot be in a table within the element";
    IWC0228 Severe "{file}: LINAGE is for a sequential file, not a line-sequential one";
    IWC0229 Severe "{file}: LINAGE is for a sequential file, not an indexed or relative one";
    IWC0230 Severe "{file}: {phrase} {number} is more than the {MOST} lines LINAGE allows";
    IWC0231 Severe "{file}: {phrase} {name} is not an unsigned integer data item";
    IWC0232 Severe "{file}: LINAGE 0: the page body needs at least one line";
    IWC0233 Severe "{file}: FOOTING 0: the footing starts at line 1 or later";
    IWC0234 Severe "{file}: FOOTING {footing} is past the page body of {body} lines";
    IWC0235 Severe "an elementary item needs a PICTURE";
    IWC0236 Severe "a group item cannot have a PICTURE";
    IWC0237 Severe "an object reference, function-pointer or procedure-pointer takes no PICTURE and only VALUE NULL";
    IWC0238 Severe "POINTER and INDEX items take no PICTURE";
    IWC0239 Severe "COMP-1 and COMP-2 items take no PICTURE";
    IWC0240 Severe "BLANK WHEN ZERO needs a numeric or numeric-edited item of USAGE DISPLAY or NATIONAL";
    IWC0241 Severe "a binary item holds at most 18 digits";
    IWC0242 Severe "JUSTIFIED cannot be given for a DBCS item whose PICTURE has B";
    IWC0243 Severe "a PICTURE with G needs USAGE DISPLAY-1 (Language Reference SC27-8713-03, p. 214)";
    IWC0244 Severe "a SIGN clause needs an S in the PICTURE";
    IWC0245 Severe "INVOKE {target}: SELF and SUPER can be used only in a method";
    IWC0246 Severe "INVOKE {target}: not an object reference or a class named in the REPOSITORY paragraph";
    IWC0247 Severe "INVOKE {target} NEW: NEW takes a class-name from the REPOSITORY paragraph";
    IWC0248 Severe "INVOKE ... NEW needs RETURNING an object reference";
    IWC0249 Severe "INVOKE ... NEW RETURNING {name}: not an object reference";
    IWC0250 Severe "INVOKE with an empty method name";
    IWC0251 Severe "INVOKE ... {name}: a method name is held in an alphanumeric or national item";
    IWC0252 Severe "INVOKE {target} {name}: a method named by a data item is invoked on a universal object reference";
    IWC0253 Severe "INVOKE argument: {message}";
    IWC0254 Severe "INVOKE ... RETURNING {name}: not reference-modified";
    IWC0255 Severe "INVOKE ... RETURNING {name}: {message}";
    IWC0256 Severe "ACCEPT ... ON EXCEPTION: of the ACCEPT statements, only ACCEPT ... FROM ARGUMENT-VALUE under --compliance extended has an exception";
    IWC0257 Severe "ACCEPT ... FROM {name}: Micro Focus's and GnuCOBOL's, not Enterprise COBOL's; --compliance extended reads it from the job step's PARM";
    IWC0258 Severe "DISPLAY UPON ARGUMENT-NUMBER: Micro Focus's and GnuCOBOL's, not Enterprise COBOL's; --compliance extended reads it";
    IWC0259 Severe "DISPLAY UPON ARGUMENT-NUMBER: it shows one numeric item or literal, the number of the argument the next ACCEPT ... FROM ARGUMENT-VALUE takes";
    IWC0260 Severe "PICTURE {picture}: P must be one string of scaling positions at the left or right end of the digits";
    IWC0261 Severe "PICTURE {picture}: {character} is not a PICTURE symbol";
    IWC0262 Severe "PICTURE {picture}: more than 134217727 character positions";
    IWC0263 Severe "PICTURE {picture}: more than 31 digits";
    IWC0264 Severe "PICTURE {picture}: mixes symbols of different categories";
    IWC0265 Severe "PICTURE {picture}: {character} cannot be in a PICTURE of {symbol}, which takes {symbol} and B only";
    IWC0266 Severe "PICTURE {picture}: more character positions than a DBCS item holds";
    IWC0267 Severe "BLANK WHEN ZERO cannot be given for a PICTURE with S";
    IWC0268 Severe "PICTURE {picture}: an edited PICTURE longer than 4096 positions";
    IWC0269 Severe "PICTURE {picture}: S and N are not allowed in an edited PICTURE";
    IWC0270 Severe "PICTURE {picture}: an alphanumeric-edited PICTURE takes only X, A, 9, B, 0 and /";
    IWC0271 Severe "PICTURE {picture}: two floating insertion strings";
    IWC0272 Severe "PICTURE {picture}: {character} is not a numeric-edited symbol";
    IWC0273 Severe "PICTURE {picture}: more than one decimal point";
    IWC0274 Severe "PICTURE {picture}: a numeric-edited PICTURE needs 1 to 31 digit positions";
    IWC0275 Severe "PICTURE {picture}: bad repetition ({count})";
    IWC0276 Severe "PICTURE {picture}: a repetition with nothing to repeat";
    IWC0277 Severe "PICTURE {picture}: two different currency symbols";
    IWC0278 Severe "PICTURE {picture}: '$' is not a currency symbol of this program, whose CURRENCY SIGN clauses or CURRENCY option name others";
    IWC0279 Severe "{ALPHABET or PROGRAM COLLATING SEQUENCE and its name}: not an alphabet-name of SPECIAL-NAMES";
    IWC0280 Severe "{ALPHABET or PROGRAM COLLATING SEQUENCE and its name}: the character X'{hex}' is given more than one position";
    IWC0281 Severe "{ALPHABET or PROGRAM COLLATING SEQUENCE and its name}: {number} is not an ordinal position from 1 to 256";
    IWC0282 Severe "{ALPHABET or PROGRAM COLLATING SEQUENCE and its name}: NULL cannot be in an ALPHABET clause";
    IWC0283 Severe "{ALPHABET or PROGRAM COLLATING SEQUENCE and its name}: a national literal cannot be in an ALPHABET clause";
    IWC0284 Severe "{ALPHABET or PROGRAM COLLATING SEQUENCE and its name}: a DBCS literal cannot be in an ALPHABET clause";
    IWC0285 Severe "{ALPHABET or PROGRAM COLLATING SEQUENCE and its name}: ALL cannot be in an ALPHABET clause";
    IWC0286 Severe "{ALPHABET or PROGRAM COLLATING SEQUENCE and its name}: a literal of THROUGH or ALSO must be one character";
    IWC0287 Severe "FUNCTION {name}: an ALL subscript stands for a varying number of arguments, and {name} takes {count}";
    IWC0288 Severe "FUNCTION {name}: {class} and {class} arguments, where all must be of the same class";
    IWC0289 Warning "INITCHECK(STRICT): {item} may be used uninitialized: a path to this statement does not set {it} (see {analysis})";
    IWC0290 Warning "INITCHECK: {item} may be used uninitialized: no path to this statement sets {it} (see {analysis})";
    IWC0291 Severe "{ALPHABET or PROGRAM COLLATING SEQUENCE and its name}: {a character the program's code page does not hold}";
    IWC0292 Severe "VALUE of {name}: a numeric literal, where a numeric-edited item's VALUE is an alphanumeric literal or a figurative constant written in edited form; --compliance extended edits the number into it";
    IWC0293 Severe "BINARY-CHAR: Micro Focus's and GnuCOBOL's one-byte binary, not Enterprise COBOL's; --compliance extended reads it";
    IWC0294 Severe "BINARY-CHAR takes no PICTURE";
    IWC0295 Severe "{name}: two programs of {program} have this name, and the programs of a separately compiled program each need their own (--program-scope=flexible allows it)";
    IWC0296 Severe "CALL '{name}': no program of the compilation has this name, and --unresolved-calls=fail refuses a static CALL the binder could not resolve";
    IWC0297 Severe "FUNCTION {name}: {argument} is numeric, where {name} takes an alphabetic, alphanumeric or national argument";
    IWC0298 Severe "{DISPLAY or ACCEPT on the screen, or the SCREEN SECTION}: Micro Focus's and GnuCOBOL's, not Enterprise COBOL's; --compliance extended reads it";
    IWC0299 Severe "{a locking phrase}: Micro Focus's and GnuCOBOL's, not Enterprise COBOL's; --compliance extended reads it";
    IWC0300 Severe "CALL ... RETURNING {OMITTED, NOTHING or NULL}: GnuCOBOL's, not Enterprise COBOL's, whose RETURNING names a data item; --compliance extended reads it";
    IWC0301 Severe "COMP-X: Micro Focus's binary in the fewest bytes its digits need, not Enterprise COBOL's; --compliance extended reads it";
    IWC0302 Severe "PIC X(n) COMP-5: Micro Focus's and GnuCOBOL's binary of n bytes, where Enterprise COBOL's COMP-5 takes a numeric PICTURE; --compliance extended reads it";
    IWC0303 Severe "PIC X({n}) {COMP-X or COMP-5}: {n} bytes of binary, and ironwork's binary items hold at most eight";
    IWC0304 Severe "{paragraph or section} FOREVER: under --compliance extended PERFORM FOREVER is Micro Focus's and GnuCOBOL's endless loop, not a PERFORM of it; compile the program under strict";
    IWC0305 Severe "FUNCTION MODULE-CALLER-ID: GnuCOBOL's, not Enterprise COBOL's; --compliance extended reads it";
    IWC0306 Severe "{STOP RUN or GOBACK} {RETURNING or GIVING}: GnuCOBOL's and Micro Focus's, not Enterprise COBOL's, where a MOVE to RETURN-CODE comes first; --compliance extended reads it";
    IWC0307 Severe "{name} [NOT] OMITTED: GnuCOBOL's and Micro Focus's, not Enterprise COBOL's, which writes ADDRESS OF {name} = NULL; --compliance extended reads it";
    IWC0308 Severe "ANY LENGTH: GnuCOBOL's and Micro Focus's, not Enterprise COBOL's; --compliance extended reads it";
    IWC0309 Severe "PERFORM UNTIL EXIT: GnuCOBOL's and Micro Focus's, not Enterprise COBOL's; --compliance extended reads it";
    IWC0310 Severe "a file description with no FILE SECTION header: Micro Focus's and GnuCOBOL's, not Enterprise COBOL's; --compliance extended reads it";
    IWC0311 Severe "KEY IS {name} = ...: Micro Focus's split key, not Enterprise COBOL's; --compliance extended reads it";
    IWC0312 Severe "FUNCTION STORED-CHAR-LENGTH: GnuCOBOL's, not Enterprise COBOL's; --compliance extended reads it";
    IWC0313 Severe "DELETE FILE: Micro Focus's and GnuCOBOL's, not Enterprise COBOL's; --compliance extended reads it";
    IWC0314 Severe "PROGRAM-POINTER: GnuCOBOL's and Micro Focus's, not Enterprise COBOL's; --compliance extended reads it as PROCEDURE-POINTER";
    IWC0316 Severe "BASED: GnuCOBOL's and Micro Focus's, not Enterprise COBOL's; --compliance extended reads it";
    IWC0317 Severe "FREE {name}: GnuCOBOL's FREE of a record, not Enterprise COBOL's, which frees through a pointer; --compliance extended reads it";
    IWJ0001 Severe "a quoted value continued onto the next line is not supported yet";
    IWJ0002 Severe "an unbalanced ) in {text}";
    IWJ0003 Severe "unbalanced parentheses or quotes in {text}";
    IWJ0004 Severe "& is not followed by a symbolic parameter name in {text}";
    IWJ0005 Severe "&SYSUID is the user ID the job runs under: USER= on the JOB statement, or the user that submitted it";
    IWJ0006 Severe "symbolic parameter &{name} has no value";
    IWJ0007 Severe "the statement is continued past the end of the job";
    IWJ0008 Severe "a continuation line must start with // and a blank";
    IWJ0009 Severe "a continued operand must start in columns 4 to 16";
    IWJ0010 Severe "IF has no THEN";
    IWJ0011 Severe "no JOB statement";
    IWJ0012 Severe "the first statement is not a JOB statement";
    IWJ0013 Severe "the JOB statement needs a job name of one to eight characters";
    IWJ0014 Severe "a symbolic parameter in the JOB statement's COND is not supported yet";
    IWJ0015 Severe "COND on the JOB statement takes (code,operator) tests only";
    IWJ0016 Severe "RESTART is not supported yet";
    IWJ0017 Severe "TYPRUN is not supported yet";
    IWJ0018 Severe "JOB keyword {k} is not supported yet";
    IWJ0019 Severe "{op} is not NAME=value";
    IWJ0020 Severe "no procedure library holds member {name}";
    IWJ0021 Severe "member {name}: {item}";
    IWJ0022 Severe "procedures and INCLUDE members nest more than 15 deep";
    IWJ0023 Severe "an in-stream PROC needs a name";
    IWJ0024 Severe "{operation} is out of place";
    IWJ0025 Severe "JCLLIB takes ORDER=";
    IWJ0026 Severe "{bad} is not a data set name";
    IWJ0027 Severe "INCLUDE takes MEMBER=name";
    IWJ0028 Severe "a DD statement that follows no EXEC statement";
    IWJ0029 Severe "a second JOB statement; give one job a file";
    IWJ0030 Severe "{op} statements are not supported yet";
    IWJ0031 Severe "{op} is not a JCL statement";
    IWJ0032 Severe "EXEC starts with PGM=, PROC= or a procedure name";
    IWJ0033 Severe "{name} is not a procedure name";
    IWJ0034 Severe "an EXEC operand {op} this reader does not know";
    IWJ0035 Severe "PARMDD is not supported yet";
    IWJ0036 Severe "procedure {name} does not use symbolic parameter {key}";
    IWJ0037 Severe "EXEC keyword {key} is not supported yet";
    IWJ0038 Severe "procedure {name} has no steps";
    IWJ0039 Severe "PARM.{text}: procedure {name} has no step {text}";
    IWJ0040 Severe "COND.{text}: procedure {name} has no step {text}";
    IWJ0041 Severe "{dd} is not a DD name";
    IWJ0042 Severe "an unnamed DD statement with nothing to concatenate to";
    IWJ0043 Severe "{number} is not a step name";
    IWJ0044 Severe "EXEC needs PGM= or a procedure";
    IWJ0045 Severe "PGM=*.stepname.ddname is not supported yet";
    IWJ0046 Severe "{value} is not a program name";
    IWJ0047 Severe "EXEC keyword {k} is not supported yet";
    IWJ0048 Severe "an EXEC operand {value} this reader does not know";
    IWJ0049 Severe "*.{path} is not *.ddname, *.stepname.ddname or *.stepname.procstepname.ddname";
    IWJ0050 Severe "{value} is not a data set name";
    IWJ0051 Severe "{value} is not a generation of a generation data group";
    IWJ0052 Severe "{message} is not a member name";
    IWJ0053 Severe "&&{temp} is not a temporary data set name";
    IWJ0054 Severe "{base} is not a data set name";
    IWJ0055 Severe "DISP={value} has more than three subparameters";
    IWJ0056 Severe "{text} is not a DISP status";
    IWJ0057 Severe "{text} is not a DISP {normal} disposition";
    IWJ0058 Severe "LRECL={value} is not a record length";
    IWJ0059 Severe "a DD operand {value} this reader does not know";
    IWJ0060 Severe "DLM takes two characters";
    IWJ0061 Severe "DD keyword {k} is not supported yet";
    IWJ0062 Severe "DISP applies to a data set";
    IWJ0063 Severe "the DD statement names no data set, in-stream data, DUMMY or SYSOUT";
    IWJ0064 Severe "DD {number} overrides a procedure step, but the EXEC before it runs a program";
    IWJ0065 Severe "{number} is not a DD name";
    IWJ0066 Severe "DD {number} appears twice in the step";
    IWJ0067 Severe "*.{path} names a DD that is no data set";
    IWJ0068 Severe "*.{path} names no earlier DD";
    IWJ0069 Severe "IF statements nest more than 15 deep";
    IWJ0070 Severe "a second ELSE for one IF";
    IWJ0071 Severe "ELSE without IF";
    IWJ0072 Severe "ENDIF without IF";
    IWJ0073 Severe "IF without ENDIF";
    IWJ0074 Severe "{text} is not a return code from 0 to 4095";
    IWJ0075 Severe "{text} is not a COND operator (GT, GE, EQ, NE, LT or LE)";
    IWJ0076 Severe "COND test ({text}) is not (code,operator) or (code,operator,stepname)";
    IWJ0077 Severe "COND={text} is not in parentheses";
    IWJ0078 Severe "{item} must come last in COND";
    IWJ0079 Severe "{t} in COND is not a test in parentheses";
    IWJ0080 Severe "COND takes at most eight tests";
    IWJ0081 Severe "{character} has no meaning in an IF expression";
    IWJ0082 Severe "parentheses in an IF expression nest more than 15 deep";
    IWJ0083 Severe "an IF expression is missing a )";
    IWJ0084 Severe "an IF expression has {t} where a keyword belongs";
    IWJ0085 Severe "an IF expression ends early";
    IWJ0086 Severe "RC needs a comparison operator and a number";
    IWJ0087 Severe "{word} needs a number after it";
    IWJ0088 Severe "{value} is not TRUE or FALSE";
    IWJ0089 Severe "ABENDCC={value} is not Sxxx or Unnnn";
    IWJ0090 Severe "stepname.ABENDCC is not supported yet";
    IWJ0091 Severe "{word} is not RC, ABEND, ABENDCC or a stepname.RC, .ABEND or .RUN";
    IWJ0092 Severe "an IF expression has {at} after its end";
    IWJ0093 Severe "a {statement} statement inside a procedure";
    IWJ0094 Severe "PROC {name} has no PEND";
    IWJ0095 Severe "a {statement} statement in an INCLUDE member is not supported";
    IWJ0096 Severe "procedure {name} has no step {step}";
    IWJ0097 Severe "{generation} is not a relative generation";
    IWJ0098 Severe "a comment is not closed with */";
    IWJ0099 Severe "a parenthesis is not closed";
    IWJ0100 Severe "{character} has no meaning here";
    IWJ0101 Severe "{text} is not a condition code from 0 to 16";
    IWJ0102 Severe "DO has no END";
    IWJ0103 Severe "END without DO";
    IWJ0104 Severe "ELSE inside DO with no IF";
    IWJ0105 Severe "a command, found {word}";
    IWJ0106 Severe "SET takes MAXCC=n or LASTCC=n";
    IWJ0107 Severe "{verb} is out of place";
    IWJ0108 Severe "the IDCAMS command {value} is not supported yet";
    IWJ0109 Severe "IF needs a comparison operator";
    IWJ0110 Severe "IF needs THEN";
    IWJ0111 Severe "DELETE of a generic name ({word}) is not supported yet";
    IWJ0112 Severe "DELETE parameter {k} is not supported yet";
    IWJ0113 Severe "DELETE has {word} where a name belongs";
    IWJ0114 Severe "DELETE names no entry";
    IWJ0115 Severe "REPRO parameter {k} is not supported yet";
    IWJ0116 Severe "REPRO parameter {word} is not supported yet";
    IWJ0117 Severe "REPRO has {word} where a parameter belongs";
    IWJ0118 Severe "REPRO needs INFILE or INDATASET, and OUTFILE or OUTDATASET";
    IWJ0119 Severe "BLDINDEX parameter {k} is not supported yet";
    IWJ0120 Severe "BLDINDEX parameter {word} is not supported yet";
    IWJ0121 Severe "BLDINDEX has {word} where a parameter belongs";
    IWJ0122 Severe "BLDINDEX needs INFILE or INDATASET, and OUTFILE or OUTDATASET";
    IWJ0123 Severe "{value} is not a DD name";
    IWJ0124 Severe "KEYS({value}) has a length that is not from 1 to 255";
    IWJ0125 Severe "RECORDSIZE({value}) needs an average from 1 to the maximum";
    IWJ0126 Severe "{value} is not an even number of hexadecimal digits";
    IWJ0127 Severe "{value} is not a key of 1 to 255 characters";
    IWJ0128 Severe "PRINT writes to OUTFILE, not OUTDATASET";
    IWJ0129 Severe "PRINT parameter {k} is not supported yet";
    IWJ0130 Severe "PRINT parameter {word} is not supported yet";
    IWJ0131 Severe "PRINT has {word} where a parameter belongs";
    IWJ0132 Severe "{value} is not a data set name or a generic name";
    IWJ0133 Severe "LEVEL({value}) must not end with *";
    IWJ0134 Severe "LISTCAT parameter {k} is not supported yet";
    IWJ0135 Severe "LISTCAT parameter {word} is not supported yet";
    IWJ0136 Severe "LISTCAT has {word} where a parameter belongs";
    IWJ0137 Severe "LISTCAT ENTRIES names no entry";
    IWJ0138 Severe "LIMIT({value}) is not from 1 to 255";
    IWJ0139 Severe "DEFINE GDG parameter {k} is not supported yet";
    IWJ0140 Severe "DEFINE GDG has {word} where a parameter belongs";
    IWJ0141 Severe "{name} is not a generation data group name of 35 characters or fewer";
    IWJ0142 Severe "DEFINE GDG needs NAME and LIMIT";
    IWJ0143 Severe "DEFINE CLUSTER {k} is not supported yet";
    IWJ0144 Severe "{name}: KEYS({length} {offset}) does not fit in a record of {maximum} bytes";
    IWJ0145 Severe "DEFINE ALTERNATEINDEX {k} is not supported yet";
    IWJ0146 Severe "DEFINE PATH RECATALOG is not supported yet";
    IWJ0147 Severe "DEFINE {k} is not supported yet; DEFINE CLUSTER, ALTERNATEINDEX, PATH and GDG are";
    IWJ0148 Severe "DEFINE has {word} where a parameter belongs";
    IWJ0149 Severe "DEFINE needs CLUSTER, ALTERNATEINDEX, PATH or GDG";
    IWJ0150 Severe "the field format {file} is not supported yet";
    IWJ0151 Severe "{text} is not a {what}";
    IWJ0152 Severe "X'{text}' is not pairs of hexadecimal digits";
    IWJ0153 Severe "the constant {token} is not supported yet; C'...', X'...' and decimal numbers are";
    IWJ0154 Severe "the edit pattern {text} is longer than 44 characters";
    IWJ0155 Severe "SIGNS=({inner}) has more than four signs";
    IWJ0156 Severe "the sign {part} is not one character";
    IWJ0157 Severe "FIELDS=({inner}) is not position, length{format}, order for each field";
    IWJ0158 Severe "an E order (an exit's own) is not supported yet";
    IWJ0159 Severe "{o} is not A or D";
    IWJ0160 Severe "the field format {format} in a condition is not supported yet; CH, BI, FI, ZD and PD are";
    IWJ0161 Severe "a {format} field of {length} bytes is longer than DFSORT compares";
    IWJ0162 Severe "comparing {position},{length},{format} with {number} is not supported yet";
    IWJ0163 Severe "{extra} in a condition is not AND or OR";
    IWJ0164 Severe "LENGTH={number} is longer than 44";
    IWJ0165 Severe "{key}: the two digit characters must differ";
    IWJ0166 Severe "arithmetic in {what} ({key}) is not supported yet";
    IWJ0167 Severe "a {what} number is either edited or converted, not both";
    IWJ0168 Severe "SIGNS goes with an edit mask, not TO, in {what}";
    IWJ0169 Severe "{character} is not a column or a symbol for one";
    IWJ0170 Severe "the {what} item {token} is not supported yet";
    IWJ0171 Severe "{what} editing of {file} fields is not supported yet";
    IWJ0172 Severe "a {file} field of {length} bytes is not one {what} edits";
    IWJ0173 Severe "arithmetic in {what} ({next}) is not supported yet";
    IWJ0174 Severe "{what} field conversion and editing ({next}) is not supported yet";
    IWJ0175 Severe "{what}=() has no items";
    IWJ0176 Severe "{what}={number} is longer than 15 digits";
    IWJ0177 Severe "the PUSH item {token} is not supported yet; p,m, ID=n and SEQ=n are";
    IWJ0178 Severe "PUSH=() has no items";
    IWJ0179 Severe "KEYBEGIN={value} is not (p,m)";
    IWJ0180 Severe "{verb} IFTHEN {key} is not supported yet";
    IWJ0181 Severe "{k} is not an operand of this {verb} IFTHEN clause";
    IWJ0182 Severe "an IFTHEN clause takes one of BUILD and OVERLAY, in {verb}";
    IWJ0183 Severe "{verb} IFTHEN WHEN=GROUP needs PUSH=";
    IWJ0184 Severe "{verb} IFTHEN WHEN=GROUP needs BEGIN, KEYBEGIN, END or RECORDS";
    IWJ0185 Severe "{verb} IFTHEN WHEN=ANY needs a WHEN=(cond) clause before it";
    IWJ0186 Severe "{verb} has more than one of BUILD, FIELDS, OUTREC and OVERLAY";
    IWJ0187 Severe "{verb} takes IFTHEN clauses or BUILD, FIELDS and OVERLAY, not both";
    IWJ0188 Severe "{verb} IFOUTLEN goes with IFTHEN clauses";
    IWJ0189 Severe "OUTFIL takes one of INCLUDE, OMIT and SAVE";
    IWJ0190 Severe "the OUTFIL parameter {k} is not supported yet";
    IWJ0191 Severe "{bad} is not a ddname";
    IWJ0192 Severe "more than one SORT or MERGE statement";
    IWJ0193 Severe "{verb} operand {word} is not supported yet";
    IWJ0194 Severe "SUM of fields is not supported yet; SUM FIELDS=NONE is";
    IWJ0195 Severe "OPTION {number} is not supported yet";
    IWJ0196 Severe "RECORD TYPE={t} is not supported yet";
    IWJ0197 Severe "more than one INCLUDE or OMIT statement; INCLUDE and OMIT are mutually exclusive";
    IWJ0198 Severe "the {verb} parameter {word} is not supported yet";
    IWJ0199 Severe "{verb} needs FIELDS=, BUILD=, OVERLAY= or IFTHEN=";
    IWJ0200 Severe "more than one {verb} statement";
    IWJ0201 Severe "the DFSORT {value} statement is not supported yet";
    IWJ0202 Severe "{value} is not a DFSORT control statement";
    IWJ0203 Severe "no SORT, MERGE or OPTION COPY statement";
    IWJ0204 Severe "SYMNAMES line {line}: {text} is not a {what} from 1 to 32752";
    IWJ0205 Severe "SYMNAMES line {line}: {statement} is not symbol,value";
    IWJ0206 Severe "SYMNAMES line {line}: ALIGN takes H, F or D, not {value}";
    IWJ0207 Severe "SYMNAMES line {line}: {name} is not a symbol, or is a reserved word";
    IWJ0208 Severe "SYMNAMES line {line}: {name} is defined twice";
    IWJ0209 Severe "SYMNAMES line {line}: {value} is not a closed string";
    IWJ0210 Severe "SYMNAMES line {line}: {value} is not a decimal number";
    IWJ0211 Severe "SYMNAMES line {line}: {file} is not a field format";
    IWJ0212 Severe "SYMNAMES line {line}: {value} is not p,m,f";
    IWJ0213 Severe "the symbol {name} stands for {what}, which these statements do not model yet";
    IWJ0214 Severe "the symbol {token} has no format, and no FORMAT= gives one";
    IWJ0215 Severe "the symbol {token} is a constant, not a field to sort on";
    IWJ0216 Severe "IF takes MAXCC or LASTCC";
    IWJ0217 Severe "IF needs a condition code";
    IWJ0218 Severe "{keyword}({value}) needs {N} whole numbers";
    IWJ0219 Severe "PRINT needs INFILE or INDATASET";
    IWJ0220 Severe "DEFINE CLUSTER needs NAME";
    IWJ0221 Severe "DEFINE ALTERNATEINDEX needs NAME";
    IWJ0222 Severe "DEFINE ALTERNATEINDEX needs RELATE";
    IWJ0223 Severe "DEFINE PATH needs NAME";
    IWJ0224 Severe "DEFINE PATH needs PATHENTRY";
    IWJ0225 Severe "{verb} needs FIELDS=";
    IWJ0226 Severe "{verb} needs COND=";
    IWJ0227 Severe "{token} is not a decimal constant";
    IWJ0228 Severe "the edit pattern {value} is not in parentheses";
    IWJ0229 Severe "SIGNS={value} is not in parentheses";
    IWJ0230 Severe "FIELDS={value} is not in parentheses";
    IWJ0231 Severe "the field {position},{length} has no format, and no FORMAT= gives one";
    IWJ0232 Severe "a comparison has no relation";
    IWJ0233 Severe "{op} is not EQ, NE, GT, GE, LT or LE";
    IWJ0234 Severe "a comparison has nothing after its relation";
    IWJ0235 Severe "a condition ends early";
    IWJ0236 Severe "COND={value} is not in parentheses";
    IWJ0237 Severe "TO={value} is not BI, FI, PD, PDC, PDF, ZD, ZDF, ZDC, CSF or FS";
    IWJ0238 Severe "{what}={value} is not in parentheses";
    IWJ0239 Severe "PUSH={value} is not in parentheses";
    IWJ0240 Severe "IFTHEN={value} is not in parentheses";
    IWJ0241 Severe "IFTHEN=({inner}) does not begin with WHEN=";
    IWJ0242 Severe "SYMNAMES line {line}: = for a position before any position was set";
    IWJ0243 Severe "SYMNAMES line {line}: = for a length before any length was set";
    IWJ0244 Severe "SYMNAMES line {line}: = for a format before any format was set";
    IWJ0245 Severe "a comparison ends early";
    IWJ0246 Severe "PGM={pgm} is not supported yet";
    IWJ0247 Severe "PARM for PGM={pgm} is not supported yet";
    IWJ0248 Severe "DD {dd} concatenates in-stream data with data sets of z/OS records";
    IWJ0249 Severe "IDCAMS SYSIN from data sets of z/OS records is not supported yet";
    IWJ0250 Severe "IEBGENER control statements are not supported yet";
    IWJ0251 Severe "IEBGENER between UTF-8 lines and z/OS records needs a record length, which DCB is not read for yet";
    IWL0001 Severe "lowering: {table} exceeds the LIR's limit";
    IWL0002 Severe "lowering: the lowered program is invalid: {why}";
    IWL0003 Severe "LOCAL-STORAGE exceeds the interpreter's {MAX STORAGE} bytes";
    IWL0004 Severe "storage exceeds the interpreter's {MAX STORAGE} bytes";
    IWL0005 Severe "WORKING-STORAGE exceeds the interpreter's {MAX STORAGE} bytes";
    IWL0006 Severe "an item larger than the interpreter's {MAX STORAGE} bytes";
    IWO0001 Severe "CBL {option}: {why}";
    IWO0002 Error "CBL CURRENCY: code page {codepage} reads its byte as {character}, which cannot be a currency symbol";
    IWO0003 Warning "CBL NODBCS: NSYMBOL(NATIONAL) requires DBCS, which is in effect";
    IWO0004 Severe "SOURCE_DATE_EPOCH={value}: not a whole number of seconds from 0 to 253402300799";
    IWO0005 Severe "{a flag this compiler does not take}";
    IWP0001 Severe "no mapset {to ascii uppercase} among the {sets} in the file";
    IWP0002 Informational "IGYPS2091-W not given: the program ends with EXEC CICS {command}, which the CICS translator turns into a CALL; --cics-return-warning=always gives the warning, =never drops this note";
    IWP0003 Severe "EXEC SQL {command}: {why}";
    IWP0004 Severe "EXEC DLI {command} is not an EXEC DLI command";
    IWP0005 Severe "EXEC DLI {command}: {name} is not one of its options";
    IWP0006 Severe "EXEC DLI {command} WHERE({t}): {why}";
    IWP0007 Severe "EXEC CICS {command} {option}({label}): {message}";
    IWP0008 Severe "a program that uses object-oriented syntax cannot contain EXEC CICS";
    IWP0009 Severe "a continuation line is missing";
    IWP0010 Severe "a label with no macro";
    IWP0011 Severe "a quoted string is not closed";
    IWP0012 Severe "a parenthesised list is not closed";
    IWP0013 Severe "{key} takes one value";
    IWP0014 Severe "{key} takes a quoted string";
    IWP0015 Severe "{key}={word} is not a number from {low} to {high}";
    IWP0016 Severe "{key}={word}: YES or NO";
    IWP0017 Severe "DSATTS={a} is not an extended attribute";
    IWP0018 Severe "EXTATT={word}: NO, MAPONLY or YES";
    IWP0019 Severe "DFHMSD TYPE=FINAL with no mapset open";
    IWP0020 Severe "DFHMDI outside a DFHMSD";
    IWP0021 Severe "DFHMDF outside a DFHMDI map";
    IWP0022 Severe "unknown macro {word}";
    IWP0023 Severe "{what} needs a name";
    IWP0024 Severe "{what} name {name} is longer than {max} characters";
    IWP0025 Severe "TYPE={t}: DSECT, MAP, FINAL or &SYSPARM";
    IWP0026 Severe "MODE={word}: IN, OUT or INOUT";
    IWP0027 Severe "SIZE=(lines,columns), each 1 to 240";
    IWP0028 Severe "{key}={word} is not a number from 1 to 240";
    IWP0029 Severe "ATTRB={word} is not an attribute";
    IWP0030 Severe "XINIT takes an even number of hexadecimal digits";
    IWP0031 Severe "XINIT takes hexadecimal digits";
    IWP0032 Severe "GRPNAME needs a labelled field and does not go with OCCURS";
    IWP0033 Severe "GRPNAME is longer than 30 characters";
    IWP0034 Severe "LENGTH=0 is allowed only on an unlabelled field, where it delimits an input field";
    IWP0035 Severe "LENGTH is missing, or is not from 1 to 256";
    IWP0036 Severe "JUSTIFY={word}: LEFT or RIGHT, BLANK or ZERO";
    IWP0037 Severe "COLOR={character} is not a colour";
    IWP0038 Severe "HILIGHT={h} is not a highlight";
    IWP0039 Severe "POS is an offset within the map, or (line,column) inside it";
    IWP0040 Severe "unbalanced '(' in DEFINE";
    IWP0041 Severe "DEFINE names no KIND(NAME)";
    IWP0042 Severe "{kind}({name}): a {kind} name is at most {limit} characters";
    IWP0043 Severe "attribute value for '{key}' exceeds 256 characters";
    IWP0044 Severe "unbalanced '(' in attribute value";
    IWP0045 Severe "{why the BMS source cannot be read}";
    IWP0046 Severe "EXEC SQL {command}: {name} in the INTO list has no colon, which Db2 requires before every host variable";
    IWR0001 Severe "XML PARSE VALIDATING WITH {schema}: the schema is in IBM's Optimized Schema Representation (OSR), which ironwork does not read";
    IWR0002 Severe "{verb} is not a statement ironwork for COBOL supports yet";
    IWR0003 Severe "{clause} is not a data description clause ironwork for COBOL supports yet";
    IWR0004 Severe "USAGE {usage} is not supported yet";
    IWR0005 Severe "VALUE {literal}: a floating-point VALUE of more than 31 digits in fixed point is not supported yet";
    IWR0006 Severe "the {section} SECTION is not supported yet";
    IWR0007 Severe "ORGANIZATION {organization} is not supported yet";
    IWR0008 Severe "{clause} is not a SELECT clause ironwork for COBOL supports yet";
    IWR0009 Severe "INDEXED BY in FACTORY or OBJECT data is not supported yet";
    IWR0010 Severe "{item}: EXTERNAL in FACTORY or OBJECT WORKING-STORAGE is not supported yet";
    IWR0011 Severe "a method's FILE SECTION can define only EXTERNAL files, which ironwork for COBOL does not support yet";
    IWR0012 Severe "items after an OCCURS DEPENDING ON table in the same record are not supported yet";
    IWR0013 Severe "ADVANCING {mnemonic}: stacker selection ({environment}) on a card punch is not supported yet";
    IWR0014 Severe "WRITE ... ADVANCING {mnemonic} on {file}, whose FD has LINAGE, is not supported yet";
    IWR0015 Severe "{item}: INDEXED BY in a GLOBAL record, in a program that contains others, is not supported yet";
    IWR0016 Severe "FD {file}: LINAGE on an EXTERNAL file is not supported yet";
    IWR0017 Severe "FD {file}: REPORT on an EXTERNAL file is not supported yet";
    IWR0018 Severe "FD {file}: LINAGE or REPORT on a GLOBAL file, in a program that contains others, is not supported yet";
    IWR0019 Severe "{item}: a GLOBAL record of FD {file}, which is not GLOBAL, in a program that contains others, is not supported yet";
    IWR0020 Severe "{file}, a GLOBAL file of {declarer}: its {clause} {item} is not a GLOBAL name of {declarer}, which is not supported yet";
    IWR0021 Severe "SET ADDRESS OF {record}, a GLOBAL LINKAGE record of {program}, in a program it contains is not supported yet";
    IWR0022 Severe "ASSIGN {item}: a file SORT or MERGE reads, writes or describes taking its name from a data item is not supported yet";
    IWR0023 Severe "USE GLOBAL BEFORE REPORTING {group} for a report group of a contained program is not supported yet";
    IWR0024 Severe "report {report} in more than one FD (INITIATE ... UPON) is not supported yet";
    IWR0025 Severe "NEXT PAGE on a LINE other than a group's first (MULTIPLE PAGE) is not supported yet";
    IWR0026 Severe "CONTROL {control}: a subscripted or reference-modified control is not supported yet";
    IWR0027 Severe "GROUP INDICATE outside a DETAIL group is not supported yet";
    IWR0028 Severe "a SUM of an entry in another report is not supported yet";
    IWR0029 Severe "REPORTS ARE ALL is not supported yet";
    IWR0030 Severe "INITIATE ... UPON is not supported yet";
    IWR0031 Severe "a GLOBAL report is not supported yet";
    IWR0032 Severe "CODE with a mnemonic-name or an identifier is not supported yet";
    IWR0033 Severe "LAST DETAIL with an identifier is not supported yet";
    IWR0034 Severe "LINE LIMIT with an identifier is not supported yet";
    IWR0035 Severe "the {clause} clause of an RD is not supported yet";
    IWR0036 Severe "{clause} is not an RD clause ironwork for COBOL supports yet";
    IWR0037 Severe "a level-{level} entry in the REPORT SECTION is not supported yet";
    IWR0038 Severe "multiple SOURCES is not supported yet";
    IWR0039 Severe "multiple VALUES is not supported yet";
    IWR0040 Severe "GROUP LIMIT is not supported yet";
    IWR0041 Severe "USAGE {usage} in a report group is not supported yet";
    IWR0042 Severe "{a report group clause ironwork does not run} is not supported yet";
    IWR0043 Severe "{clause} is not a report group clause ironwork for COBOL supports yet";
    IWR0044 Severe "an entry with more than one SOURCE, VALUE or SUM (a multiple-choice entry) is not supported yet";
    IWR0045 Severe "a SUM or COUNT term in a SOURCE expression is not supported yet";
    IWR0046 Severe "CONTROL FOOTING FOR ALL is not supported yet";
    IWR0047 Severe "CONTROL HEADING ... OR PAGE is not supported yet";
    IWR0048 Severe "a CONTROL FOOTING for more than one control is not supported yet";
    IWR0049 Severe "multiple LINES is not supported yet";
    IWR0050 Severe "multiple COLUMNS is not supported yet";
    IWR0051 Severe "SUM of an arithmetic expression is not supported yet";
    IWR0052 Severe "lowering: {construct} is not lowered yet";
    IWR0053 Severe "{file}: LINAGE on a report file is not supported yet";
    IWR0054 Severe "BLANK WHEN ZERO on a USAGE NATIONAL item is not supported yet";
    IWR0055 Severe "a national-edited PICTURE is not supported yet";
    IWR0056 Severe "a {category} PICTURE with USAGE {usage} is not supported yet";
    IWR0057 Severe "{a SCREEN SECTION form ironwork does not run} is not supported yet";
    IWR0075 Severe "FUNCTION {name}: a user-defined function is not supported here yet";
    IWR0076 Severe "ANY LENGTH on {name}: ironwork reads it on an alphanumeric 01 or 77 parameter, and {why}";
    IWR0077 Severe "BASED on {name}: ironwork reads BASED on an 01 or 77 entry";
    IWS0001 Severe "{what the syntax takes there}, found {the word or token there}";
    IWS0002 Severe "{COPY or a translator's INCLUDE} {name}: no such member in the copy libraries";
    IWS0003 Severe "COPY: {message}";
    IWS0004 Severe "COPY {text}: the name ends in a period; the period that ends a COPY statement is the one followed by a space";
    IWS0005 Severe "{verb}: {message}";
    IWS0006 Severe "{verb} {name}: {display}";
    IWS0007 Severe "more than 65535 copy members";
    IWS0008 Severe "{verb} {name}: copies itself, or nests deeper than {MAX DEPTH}";
    IWS0009 Severe "REPLACE OFF: a period to end the statement";
    IWS0010 Severe "REPLACE ALSO and REPLACE LAST OFF are the 2014 COBOL standard's; Enterprise COBOL has REPLACE pseudo-text BY pseudo-text and REPLACE OFF";
    IWS0011 Severe "REPLACE: a period to end the statement";
    IWS0012 Severe "COPY {display}: {item}";
    IWS0013 Severe "& with no literal after it";
    IWS0014 Severe "constant {name}: {why}";
    IWS0016 Severe "& joins two alphanumeric or hexadecimal literals, or two national literals, either of which may be a level-78 constant standing for one";
    IWS0017 Severe "X'{text}' is not an even number of hex digits";
    IWS0018 Severe "NX'{text}': a national hexadecimal literal is 4 to 320 hex digits, four to each UTF-16 code unit";
    IWS0019 Severe "a sign must be followed by a number";
    IWS0020 Severe "literal concatenation with & is not Enterprise COBOL's";
    IWS0021 Severe "unexpected character {character}";
    IWS0022 Severe "EXEC with no END-EXEC";
    IWS0023 Severe "a DBCS literal holds 1 to {DBCS LITERAL MAX} characters, not {count}";
    IWS0024 Severe "an unterminated literal";
    IWS0025 Error "non-COBOL character {character}: the character was accepted";
    IWS0026 Severe "unexpected '.'";
    IWS0027 Severe "PICTURE with no character-string";
    IWS0028 Severe "{file} has no SELECT ... ASSIGN";
    IWS0029 Severe "a user-defined function or prototype cannot be nested within a program, function, method or class";
    IWS0030 Severe "ENTRY cannot be used in a nested program";
    IWS0031 Severe "FUNCTION-ID {name}: {why}";
    IWS0032 Severe "FUNCTION-ID {name}: a user-defined function cannot be named {name}";
    IWS0033 Severe "a second definition of user-defined function {name}";
    IWS0034 Severe "FUNCTION-ID {name}: a user-defined function contains no programs, but {inner} is inside it";
    IWS0035 Severe "END FUNCTION {end} ends function {name}";
    IWS0036 Severe "a second CURRENCY SIGN clause for the currency symbol {symbol}";
    IWS0037 Severe "UPSI-{number}: a second {which} STATUS phrase";
    IWS0038 Severe "UPSI-{number}: a mnemonic-name or an ON or OFF STATUS phrase must follow it";
    IWS0039 Severe "CURRENCY SIGN needs a nonempty alphanumeric literal";
    IWS0040 Severe "CURRENCY SIGN {bytes} is not one character that can be a PICTURE currency symbol";
    IWS0041 Severe "CURRENCY SIGN {value} is not one character that can be a PICTURE currency symbol";
    IWS0042 Severe "CURRENCY SIGN {value} contains a digit, +, -, . or ,";
    IWS0043 Severe "PICTURE SYMBOL {symbol} is not one character that can be a PICTURE currency symbol";
    IWS0044 Severe "RECORD DELIMITER on {file}: the clause is for a file of ORGANIZATION SEQUENTIAL";
    IWS0045 Severe "{indicator} {name} has no SELECT";
    IWS0046 Severe "SD {name}: a sort or merge file takes no REPORT clause";
    IWS0047 Severe "SD {name}: a sort or merge file takes no EXTERNAL or GLOBAL clause";
    IWS0048 Severe "{indicator} {name}: LINAGE is given twice";
    IWS0049 Severe "{indicator} {name}: EXTERNAL goes on the FD, not on a record of the FILE SECTION";
    IWS0050 Severe "LINAGE: {phrase} {name} takes no subscript or reference modification";
    IWS0051 Severe "a floating-point VALUE literal is for a COMP-1 or COMP-2 item, not a fixed-point one";
    IWS0052 Severe "{written}: a floating-point literal's mantissa has at most 16 digits";
    IWS0053 Severe "{written}: not an exponent";
    IWS0054 Severe "{written}: the literal after ALL is alphanumeric, national or a figurative constant other than ALL";
    IWS0055 Severe "ACCEPT ... FROM ENVIRONMENT is GnuCOBOL's, not Enterprise COBOL's";
    IWS0056 Severe "EXIT FUNCTION: Enterprise COBOL does not yet support the format 4 EXIT statement; GOBACK ends a user-defined function";
    IWS0057 Severe "CORRESPONDING takes one receiving group";
    IWS0058 Severe "an inline PERFORM cannot have AFTER phrases: Enterprise COBOL takes them only when PERFORM names a procedure";
    IWS0059 Severe "INITIALIZE: {word} is named twice in the {phrase} phrase";
    IWS0060 Severe "ACCEPT ... FROM {name}: {why}";
    IWS0061 Severe "SET ENVIRONMENT is GnuCOBOL's, not Enterprise COBOL's";
    IWS0062 Severe "DFHRESP({condition}): not a CICS condition ironwork for COBOL knows";
    IWS0063 Severe "DFHVALUE({name}): not a CVDA ironwork for COBOL knows";
    IWS0064 Severe "NOT cannot follow the left parenthesis that distributes a relational operator";
    IWS0065 Severe "<> is not an Enterprise COBOL relational operator: it writes NOT =";
    IWS0066 Informational "{section} SECTION: no paragraph-name after its USE statement";
    IWS0067 Severe "USE FOR DEBUGGING ON ALL: Enterprise COBOL debugs procedures, by name or as ALL PROCEDURES, and no other items";
    IWS0068 Severe "USE FOR DEBUGGING is not allowed in a method";
    IWS0069 Severe "USE FOR DEBUGGING is not allowed in a RECURSIVE program";
    IWS0070 Severe "USE FOR DEBUGGING in a contained program: debugging sections are allowed only in the outermost program";
    IWS0071 Severe "a REPOSITORY paragraph belongs to the outermost program only";
    IWS0072 Severe "a class definition must be alone in its source file";
    IWS0073 Severe "END CLASS {end} ends class {name}";
    IWS0074 Severe "CLASS {name} IS \"{text}\": not a Java class name";
    IWS0075 Severe "class {name} is named twice in the REPOSITORY paragraph";
    IWS0076 Severe "WHEN-COMPILED is a special register too, so the REPOSITORY paragraph cannot name it";
    IWS0077 Severe "FUNCTION {name} INTRINSIC: {name} is not an intrinsic function ironwork for COBOL knows";
    IWS0078 Severe "FUNCTION {name}: a user-defined function in the REPOSITORY paragraph cannot be named {name}";
    IWS0079 Severe "FUNCTION ALL: INTRINSIC follows ALL, which names every intrinsic function";
    IWS0080 Severe "FUNCTION {name}: the REPOSITORY paragraph lists {name} with INTRINSIC, so no user-defined function takes its name here";
    IWS0081 Severe "{section}: the DATA DIVISION of a {kind} paragraph has only a WORKING-STORAGE SECTION";
    IWS0082 Severe "a class definition cannot contain EXEC statements";
    IWS0083 Severe "method \"{name}\" contains a program: a method cannot contain nested programs";
    IWS0084 Severe "method \"{name}\" has a REPOSITORY paragraph: the class's applies to its methods";
    IWS0085 Severe "INVOKE passes its arguments BY VALUE, not BY {word}";
    IWS0086 Severe "INVOKE passes its arguments BY VALUE: write USING BY VALUE";
    IWS0087 Severe "level {level} is not a data level";
    IWS0088 Severe "TYPE and NEXT GROUP belong on a report group's 01-level entry";
    IWS0089 Severe "a report group entry needs an 01-level entry before it";
    IWS0090 Severe "SUM with SOURCE or VALUE in one entry";
    IWS0091 Severe "COLUMN RIGHT and CENTER take an absolute column";
    IWS0092 Severe "a literal runs to the end of the line with no continuation";
    IWS0093 Severe "a continued literal must resume with its quote";
    IWS0094 Severe "{shown}: the source-format directives >>SOURCE and $SET SOURCEFORMAT, giving FREE or FIXED, are the only compiler directives ironwork reads";
    IWS0095 Severe "CURRENCY SIGN {literal} is {character} in the program's code page, which cannot be a PICTURE currency symbol";
    IWS0096 Severe "CURRENCY SIGN {literal} is {value} in the program's code page, which contains a digit, +, -, . or ,";
    IWS0097 Severe "{END-DISPLAY or END-ACCEPT}: Micro Focus's and GnuCOBOL's scope terminator, a word Enterprise COBOL does not reserve; --compliance extended reads it";
    IWS0098 Severe "VALUES in a level-{level} entry: Enterprise COBOL writes VALUES only in a level-88 entry, and VALUE in any other; --compliance extended reads it as VALUE";
    IWS0099 Error "{word}: a user-defined word has at most 30 characters, and this one has {count}; it is read as its first 30, {the first 30}";
    IWS0100 Error "{word} begins in Area A, where Enterprise COBOL puts no statement: it is read as though it began in Area B";
    IWS0101 Severe "{FLOAT-SHORT or FLOAT-LONG}: GnuCOBOL's and Micro Focus's floating point, not Enterprise COBOL's; --compliance extended reads it as {COMP-1 or COMP-2}";
    IWS0102 Severe "the Communication feature ({a COMMUNICATION SECTION item or statement}) is not part of Enterprise COBOL, which does not compile it";
    IWS0104 Error "{terminator}: an explicit scope terminator with no verb open for it; it was discarded";
    IWS0105 Error "a period was required before {word}: one was assumed";
    IWX0001 Warning "free-form source (Micro Focus and GnuCOBOL; Enterprise COBOL reads fixed form alone): {why the file is read in free form}";
    IWX0002 Warning "constant entry (Micro Focus and GnuCOBOL; Enterprise COBOL has no level 78 and no CONSTANT clause): {name} stands for its value wherever it is used after this entry";
    IWX0003 Warning "<> (Micro Focus and GnuCOBOL; Enterprise COBOL writes NOT =) is read as NOT =";
    IWX0004 Warning "literal concatenation with & (Micro Focus and GnuCOBOL; Enterprise COBOL has none): the literals on either side are one literal";
    IWX0005 Warning "{the COBOL 2002 or GnuCOBOL binary usage}: {usage} is read as PIC {picture} COMP-5";
    IWX0006 Warning "PROGRAM-ID with no IDENTIFICATION DIVISION header before it (COBOL 2002, Micro Focus and GnuCOBOL; Enterprise COBOL requires the header): the program reads as though IDENTIFICATION DIVISION. came before it";
    IWX0007 Warning "ASSIGN to a data item (Micro Focus and GnuCOBOL; Enterprise COBOL's assignment-name is never a data item): each OPEN of {file} takes its DD name from {item}";
    IWX0008 Warning "an integer or numeric function as a MOVE's sender (GnuCOBOL; Enterprise COBOL takes one only where an arithmetic expression can be): FUNCTION {name} is moved as its value";
    IWX0009 Warning "PROCEDURE DIVISION RETURNING OMITTED (GnuCOBOL; Enterprise COBOL's RETURNING names an 01 or 77 item of the LINKAGE SECTION): the program is read with no RETURNING phrase, and returns its RETURN-CODE to its caller as any program does";
    IWX0010 Warning "{ACCEPT ... FROM COMMAND-LINE, ARGUMENT-NUMBER or ARGUMENT-VALUE, or DISPLAY ... UPON ARGUMENT-NUMBER} (Micro Focus and GnuCOBOL; Enterprise COBOL reads no command line): {what the job step's PARM program arguments give}";
    IWX0011 Warning "an INTO name written without its colon (Db2 13 for z/OS requires the colon before every host variable): {name} is read as a host variable";
    IWX0012 Warning "a numeric literal as a numeric-edited item's VALUE (Micro Focus and GnuCOBOL; Enterprise COBOL takes an alphanumeric literal in edited form): {name} starts as the literal moved to it";
    IWX0013 Warning "{END-DISPLAY or END-ACCEPT} (Micro Focus and GnuCOBOL; Enterprise COBOL does not reserve the word): it ends the {DISPLAY or ACCEPT} statement";
    IWX0014 Warning "VALUES outside a level-88 entry (Micro Focus; Enterprise COBOL writes VALUE there): it is read as VALUE";
    IWX0015 Warning "a user-defined word of more than 30 characters (Micro Focus and GnuCOBOL; Enterprise COBOL reads its first 30): {word} is read whole";
    IWX0016 Warning "BINARY-CHAR (Micro Focus and GnuCOBOL; Enterprise COBOL's binary items are two, four or eight bytes): {name} is one byte of binary, {range}";
    IWX0017 Warning "a statement in Area A (Micro Focus and GnuCOBOL; Enterprise COBOL puts statements in Area B): {word} is read as though it began in Area B";
    IWX0018 Warning "a numeric argument to FUNCTION {name} (GnuCOBOL; Enterprise COBOL takes an alphabetic, alphanumeric or national one): {item}'s digits are read as its characters";
    IWX0019 Warning "OCCURS at level {level} (Micro Focus and GnuCOBOL; Enterprise COBOL takes OCCURS only at levels 02 to 49): {name} is read as a table in a record of its own";
    IWX0020 Warning "{DISPLAY or ACCEPT} on the screen (Micro Focus and GnuCOBOL; Enterprise COBOL has none): at {where}";
    IWX0021 Warning "{the environment form} (Micro Focus and GnuCOBOL; Enterprise COBOL reads and sets no environment variable): {what it does}";
    IWX0022 Warning "{a locking phrase} (Micro Focus and GnuCOBOL; Enterprise COBOL has no record locks of its own): the run unit is the file's only user, so nothing it locks waits and the phrase changes nothing";
    IWX0023 Warning "INSPECT ... TRAILING (GnuCOBOL; Enterprise COBOL has ALL, LEADING, FIRST and CHARACTERS): the occurrences that run on to the end of the phrase's region";
    IWX0024 Warning "CALL ... RETURNING {OMITTED, NOTHING or NULL} (GnuCOBOL; Enterprise COBOL's RETURNING names a data item): the CALL leaves the caller's RETURN-CODE as it was";
    IWX0025 Warning "COMP-X (Micro Focus; Enterprise COBOL's binary items are two, four or eight bytes): {name} is {n} bytes of binary, {range}, shown in {digits} digits";
    IWX0026 Warning "PIC X(n) COMP-5 (Micro Focus and GnuCOBOL; Enterprise COBOL's COMP-5 takes a numeric PICTURE): {name} is {n} bytes of binary, {range}";
    IWX0027 Warning "{FLOAT-SHORT or FLOAT-LONG} (GnuCOBOL and Micro Focus; Enterprise COBOL writes COMP-1 and COMP-2): it is read as {COMP-1 or COMP-2}, IBM's hexadecimal floating point";
    IWX0028 Warning "PERFORM ... FOREVER (Micro Focus and GnuCOBOL; Enterprise COBOL has no FOREVER phrase): it repeats until EXIT PERFORM, GO TO, GOBACK or STOP RUN leaves it";
    IWX0029 Warning "ACCEPT ... FROM {LINES, COLUMNS or COLS} (GnuCOBOL; Enterprise COBOL has no screen): the screen's {24 lines or 80 columns}";
    IWX0030 Warning "WRITE ... BEFORE ADVANCING on the line-sequential file {file} (GnuCOBOL and Micro Focus; Enterprise COBOL allows only AFTER there): the line, then the lines or page it names";
    IWX0031 Warning "FUNCTION MODULE-CALLER-ID (GnuCOBOL; Enterprise COBOL has no such function): the PROGRAM-ID of the program that called this one, empty in the main program";
    IWX0032 Warning "a level-66 entry before the end of its record (GnuCOBOL's IBM and Micro Focus dialects; Enterprise COBOL writes a record's RENAMES entries after its last entry): {name} is read as following {record}'s last entry";
    IWX0033 Warning "{STOP RUN or GOBACK} {RETURNING or GIVING} (GnuCOBOL and Micro Focus; Enterprise COBOL moves the value to RETURN-CODE first): the value is moved to RETURN-CODE, then {STOP RUN or GOBACK} ends the program";
    IWX0034 Warning "{name} [NOT] OMITTED (GnuCOBOL and Micro Focus; Enterprise COBOL writes ADDRESS OF {name} = NULL): it is read as ADDRESS OF {name} = NULL, true when the caller passed OMITTED or no argument there";
    IWX0035 Warning "ANY LENGTH (GnuCOBOL and Micro Focus; Enterprise COBOL's parameters have the length their entries give): {name} is as long as the argument each CALL passes for it";
    IWX0036 Warning "START KEY {< or NOT > or <=} (Micro Focus and GnuCOBOL; Enterprise COBOL's START takes =, >, NOT < or >=): the file is positioned at the last record whose key is {that} the value, which READ NEXT or READ PREVIOUS reads first";
    IWX0037 Warning "a file description with no FILE SECTION header (Micro Focus and GnuCOBOL; Enterprise COBOL writes FILE SECTION first): it is read as though FILE SECTION came first";
    IWX0038 Warning "PERFORM UNTIL EXIT (GnuCOBOL and Micro Focus; Enterprise COBOL has no such condition): it repeats until EXIT PERFORM, GO TO, GOBACK or STOP RUN leaves it";
    IWX0039 Warning "ASSIGN TO DISK (Micro Focus and GnuCOBOL; Enterprise COBOL's ASSIGN names a DD): DISK is the device, and what follows names the file";
    IWX0040 Warning "KEY IS {name} = ... (Micro Focus; Enterprise COBOL's key is one data item): the key joins {n} items of the record, in the order written";
    IWX0041 Warning "periods after a period (GnuCOBOL and Micro Focus; Enterprise COBOL ends a sentence with one): the periods after the first are ignored";
    IWX0042 Warning "FUNCTION STORED-CHAR-LENGTH (GnuCOBOL; Enterprise COBOL has no such function): the argument's length in characters without its trailing spaces";
    IWX0043 Warning "DELETE FILE (Micro Focus and GnuCOBOL; Enterprise COBOL has no such statement): each closed file's data set is removed";
    IWX0044 Warning "PROGRAM-POINTER (GnuCOBOL and Micro Focus; Enterprise COBOL writes PROCEDURE-POINTER): it is read as PROCEDURE-POINTER, set by SET ... TO ENTRY and called by CALL";
    IWX0046 Warning "BASED (GnuCOBOL and Micro Focus; Enterprise COBOL describes such an item in the LINKAGE SECTION): {name} has no storage until SET ADDRESS OF gives it some";
    IWX0047 Warning "FREE {name} (GnuCOBOL; Enterprise COBOL frees through a pointer): the storage ADDRESS OF {name} names is released, and the record has none";
}

/// docs/messages.md: the areas and every message, with the severity it is given.
pub fn document() -> String {
    let mut out = String::from(
        "# ironwork's compiler messages\n\n<!-- Generated from crates/syntax/src/messages.rs by its tests; do not edit. -->\n\n\
         Each message a compile gives carries an id: `IW`, an area's letter and four digits, then the\n\
         severity it was given, as in `IWR0001-S`. A site fills in the parts between braces. Once\n\
         released, an id keeps its meaning and is never given to another message; its wording may change.\n\n\
         ## Areas\n\n| Letter | Area |\n|---|---|\n",
    );
    for (letter, area) in AREAS {
        out.push_str(&format!("| {letter} | {area} |\n"));
    }
    out.push_str("\n## Messages\n\n| Id | Severity | Text |\n|---|---|---|\n");
    for m in CATALOGUE {
        out.push_str(&format!("| {} | {} | `{}` |\n", m.id, m.severity.letter(), m.text.replace('|', "\\|")));
    }
    out.push_str(
        "\n## Run-time refusals\n\n\
         A run that reaches a construct ironwork does not run ends with one of these, its id and\n\
         severity S before the text, under the abend code IRONWORK, EXEC or JAVA.\n\n\
         | Id | Severity | Text |\n|---|---|---|\n",
    );
    for r in rt::refusal::RUNTIME {
        out.push_str(&format!("| {} | S | `{}` |\n", r.id, r.text.replace('|', "\\|")));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sources(dir: &std::path::Path, out: &mut Vec<(std::path::PathBuf, String)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n != "tests") {
                    sources(&path, out);
                }
            } else if path.extension().is_some_and(|e| e == "rs") && path.file_name().is_some_and(|n| n != "tests.rs" && n != "messages.rs") {
                let text = std::fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
                let code = text.find("#[cfg(test)]\nmod tests {").map_or(text.as_str(), |at| &text[..at]).to_owned();
                out.push((path, code));
            }
        }
    }

    #[test]
    fn syntax_compile_and_code_generation_build_every_message_from_the_catalogue() {
        let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut files = Vec::new();
        for krate in ["syntax", "compile", "exec"] {
            sources(&crates.join(krate).join("src"), &mut files);
        }
        let uncatalogued: Vec<String> = files
            .iter()
            .flat_map(|(path, code)| code.lines().enumerate().filter(|(_, l)| l.contains("Error::at(") || l.contains("Error::warning(")).map(move |(n, l)| format!("{}:{}: {}", path.display(), n + 1, l.trim())))
            .collect();
        assert!(uncatalogued.is_empty(), "give these an entry in syntax::messages and build them with it:\n{}", uncatalogued.join("\n"));
    }

    #[test]
    fn a_run_refuses_a_construct_by_a_catalogued_id() {
        let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut files = Vec::new();
        for krate in ["rt", "exec"] {
            sources(&crates.join(krate).join("src"), &mut files);
        }
        let uncatalogued: Vec<String> = files
            .iter()
            .flat_map(|(path, code)| {
                code.lines().enumerate().filter(|(_, l)| l.contains("Abend::ironwork(") && (l.contains("not supported") || l.contains("does not run") || l.contains("does not provide"))).map(move |(n, l)| format!("{}:{}: {}", path.display(), n + 1, l.trim()))
            })
            .collect();
        assert!(uncatalogued.is_empty(), "give these an entry in rt::refusal and build them with it:\n{}", uncatalogued.join("\n"));
    }

    #[test]
    fn docs_messages_is_the_catalogue() {
        let committed = include_str!("../../../docs/messages.md").replace('\r', "");
        let written = document();
        assert!(committed == written, "docs/messages.md is not the catalogue; write it as:\n{written}");
    }

    #[test]
    fn ids_are_unique_in_order_and_name_an_area() {
        let ids: Vec<&str> = CATALOGUE.iter().map(|m| m.id).collect();
        assert!(ids.windows(2).all(|w| w[0] < w[1]), "{ids:?}");
        let runtime: Vec<&str> = rt::refusal::RUNTIME.iter().map(|r| r.id).collect();
        assert!(runtime.windows(2).all(|w| w[0] < w[1]), "{runtime:?}");
        let shared: Vec<&&str> = runtime.iter().filter(|id| ids.contains(id)).collect();
        assert!(shared.is_empty(), "both halves of the catalogue give {shared:?}");
        for id in ids.into_iter().chain(runtime) {
            assert!(id.len() == 7 && id.starts_with("IW") && id[3..].bytes().all(|b| b.is_ascii_digit()), "{id}");
            assert!(AREAS.iter().any(|(letter, _)| id.as_bytes()[2] == *letter as u8), "{id}");
        }
    }

    #[test]
    fn a_catalogued_message_carries_its_id_and_severity() {
        let pos = Pos { file: 0, line: 3, col: 8 };
        let warned = IWX0003.at(pos, IWX0003.text);
        assert_eq!((warned.id, warned.severity), (Some("IWX0003"), Severity::Warning));
        assert_eq!(warned.place("p.cbl"), "p.cbl:3:8: warning: IWX0003-W <> (Micro Focus and GnuCOBOL; Enterprise COBOL writes NOT =) is read as NOT =");
        let refused = IWR0001.at(pos, "XML PARSE VALIDATING WITH X: why");
        assert_eq!(refused.place("p.cbl"), "p.cbl:3:8: IWR0001-S XML PARSE VALIDATING WITH X: why");
        assert_eq!(refused.graded(Severity::Error).labelled(), "IWR0001-E XML PARSE VALIDATING WITH X: why");
    }
}
