//! ironwork's message catalogue: each compile-time message's id, the severity it is given and its
//! text, the parts a site fills in between braces. docs/messages.md is written from it. Once
//! released, an id keeps its meaning and is never given to another message; its wording may change.

use crate::{Error, Pos, Severity};

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
    IWL0001 Severe "lowering: {table} exceeds the LIR's limit";
    IWL0002 Severe "lowering: the lowered program is invalid: {why}";
    IWP0001 Severe "no mapset {to ascii uppercase} among the {sets} in the file";
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
    IWS0015 Severe "BINARY-CHAR is a one-byte binary item, and ironwork's binary items are two, four or eight bytes, as Enterprise COBOL's are";
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
    IWS0032 Severe "FUNCTION-ID {name}: {name} is an intrinsic function's name (assumption C271)";
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
    IWS0080 Severe "FUNCTION {name}: an intrinsic function is listed with INTRINSIC, and no user-defined function takes its name (assumption C271)";
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
    IWX0001 Warning "free-form source (Micro Focus and GnuCOBOL; Enterprise COBOL reads fixed form alone): {why the file is read in free form}";
    IWX0002 Warning "constant entry (Micro Focus and GnuCOBOL; Enterprise COBOL has no level 78 and no CONSTANT clause): {name} stands for its value wherever it is used after this entry";
    IWX0003 Warning "<> (Micro Focus and GnuCOBOL; Enterprise COBOL writes NOT =) is read as NOT =";
    IWX0004 Warning "literal concatenation with & (Micro Focus and GnuCOBOL; Enterprise COBOL has none): the literals on either side are one literal";
    IWX0005 Warning "the COBOL 2002 binary usage (Micro Focus and GnuCOBOL; not Enterprise COBOL's): {usage} is read as PIC {picture} COMP-5";
    IWX0006 Warning "PROGRAM-ID with no IDENTIFICATION DIVISION header before it (COBOL 2002, Micro Focus and GnuCOBOL; Enterprise COBOL requires the header): the program reads as though IDENTIFICATION DIVISION. came before it";
    IWX0007 Warning "ASSIGN to a data item (Micro Focus and GnuCOBOL; Enterprise COBOL's assignment-name is never a data item): each OPEN of {file} takes its DD name from {item}";
    IWX0008 Warning "an integer or numeric function as a MOVE's sender (GnuCOBOL; Enterprise COBOL takes one only where an arithmetic expression can be): FUNCTION {name} is moved as its value";
    IWX0009 Warning "PROCEDURE DIVISION RETURNING OMITTED (GnuCOBOL; Enterprise COBOL's RETURNING names an 01 or 77 item of the LINKAGE SECTION): the program is read with no RETURNING phrase, and returns its RETURN-CODE to its caller as any program does";
    IWX0010 Warning "{ACCEPT ... FROM COMMAND-LINE, ARGUMENT-NUMBER or ARGUMENT-VALUE, or DISPLAY ... UPON ARGUMENT-NUMBER} (Micro Focus and GnuCOBOL; Enterprise COBOL reads no command line): {what the job step's PARM program arguments give}";
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
    out
}

#[cfg(test)]
mod tests {
    use super::*;

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
        for id in ids {
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
