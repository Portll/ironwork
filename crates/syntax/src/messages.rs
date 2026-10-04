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
    IWR0001 Severe "XML PARSE VALIDATING WITH {schema}: the schema is in IBM's Optimized Schema Representation (OSR), which ironwork does not read";
    IWS0001 Severe "{what the syntax takes there}, found {the word or token there}";
    IWS0002 Severe "{COPY or a translator's INCLUDE} {name}: no such member in the copy libraries";
    IWX0001 Warning "free-form source (Micro Focus and GnuCOBOL; Enterprise COBOL reads fixed form alone): {why the file is read in free form}";
    IWX0002 Warning "constant entry (Micro Focus and GnuCOBOL; Enterprise COBOL has no level 78 and no CONSTANT clause): {name} stands for its value wherever it is used after this entry";
    IWX0003 Warning "<> (Micro Focus and GnuCOBOL; Enterprise COBOL writes NOT =) is read as NOT =";
    IWX0004 Warning "literal concatenation with & (Micro Focus and GnuCOBOL; Enterprise COBOL has none): the literals on either side are one literal";
    IWX0005 Warning "the COBOL 2002 binary usage (Micro Focus and GnuCOBOL; not Enterprise COBOL's): {usage} is read as PIC {picture} COMP-5";
    IWX0006 Warning "PROGRAM-ID with no IDENTIFICATION DIVISION header before it (COBOL 2002, Micro Focus and GnuCOBOL; Enterprise COBOL requires the header): the program reads as though IDENTIFICATION DIVISION. came before it";
    IWX0007 Warning "ASSIGN to a data item (Micro Focus and GnuCOBOL; Enterprise COBOL's assignment-name is never a data item): each OPEN of {file} takes its DD name from {item}";
    IWX0008 Warning "an integer or numeric function as a MOVE's sender (GnuCOBOL; Enterprise COBOL takes one only where an arithmetic expression can be): FUNCTION {name} is moved as its value";
    IWX0009 Warning "PROCEDURE DIVISION RETURNING OMITTED (GnuCOBOL; Enterprise COBOL's RETURNING names an 01 or 77 item of the LINKAGE SECTION): the program is read with no RETURNING phrase, and returns its RETURN-CODE to its caller as any program does";
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
