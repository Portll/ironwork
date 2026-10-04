//! Reserved words used as user-defined names, which Enterprise COBOL refuses.

use syntax::ast::{DataEntry, Program};
use syntax::{Error, Pos};

/// Pushes an error for each user-defined name in `program` that Enterprise COBOL reserves: data
/// items, conditions, indexes, files, paragraphs and sections. PROGRAM-ID, SPECIAL-NAMES and report
/// names are not checked.
pub(crate) fn check(program: &Program, errors: &mut Vec<Error>) {
    let mut names: Vec<(&str, Pos, &str)> = Vec::new();
    let records = program.files.iter().map(|f| f.records.as_slice());
    for entries in [&program.working_storage, &program.local_storage, &program.linkage].map(Vec::as_slice).into_iter().chain(records) {
        names.extend(entries.iter().flat_map(declared));
    }
    names.extend(program.files.iter().map(|f| (f.name.as_str(), f.pos, "a file")));
    names.extend(program.paragraphs.iter().map(|p| (p.name.as_str(), p.pos, if p.is_section { "a section" } else { "a paragraph" })));
    for (name, pos, what) in names {
        if rt::reserved_words::is_reserved(name) {
            errors.push(syntax::messages::IWC0188.at(pos, format!("{name} is a reserved word, so it cannot name {what}")));
        }
    }
}

/// The names one data entry declares: its own, and its INDEXED BY index-names.
fn declared(entry: &DataEntry) -> impl Iterator<Item = (&str, Pos, &'static str)> {
    let what = if entry.level == 88 { "a condition" } else { "a data item" };
    let own = entry.name.as_deref().map(|n| (n, entry.pos, what));
    own.into_iter().chain(entry.indexed_by.iter().map(|i| (i.as_str(), entry.pos, "an index")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The messages `check` gives for a program with the data and procedure lines given.
    fn refused(data: &[&str], procedure: &[&str]) -> Vec<String> {
        let line = |l: &&str| format!("       {l}\n");
        let src = [
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. P.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n".to_owned(),
            data.iter().map(line).collect(),
            "       PROCEDURE DIVISION.\n".to_owned(),
            procedure.iter().map(line).collect(),
        ]
        .concat();
        let program = syntax::parse(&src).unwrap_or_else(|e| panic!("{e}\n{src}"));
        let mut errors = Vec::new();
        check(&program, &mut errors);
        errors.iter().map(|e| format!("{}: {}", e.pos.line, e.message)).collect()
    }

    #[test]
    fn a_reserved_data_name_is_refused_where_it_is_declared() {
        assert_eq!(refused(&["01 COUNT PIC 9(4)."], &["    GOBACK."]), ["5: COUNT is a reserved word, so it cannot name a data item"]);
        assert_eq!(refused(&["01 count PIC 9(4)."], &["    GOBACK."]), ["5: COUNT is a reserved word, so it cannot name a data item"], "the parser upper-cases names");
    }

    #[test]
    fn standard_only_and_potential_words_and_filler_are_names_a_program_may_use() {
        assert!(refused(&["01 CONTROL PIC 9(4).", "01 ACTIVE-CLASS PIC 9(4).", "01 FILLER PIC X."], &["    GOBACK."]).is_empty());
    }

    #[test]
    fn conditions_and_indexes_are_names_too() {
        let data = ["01 WS-FLAG PIC X.", "   88 TRUE VALUE 'Y'.", "01 WS-TABLE.", "   05 WS-ROW OCCURS 10 INDEXED BY COUNT.", "      10 WS-CELL PIC 9."];
        let got = refused(&data, &["    GOBACK."]);
        assert_eq!(got, ["6: TRUE is a reserved word, so it cannot name a condition", "8: COUNT is a reserved word, so it cannot name an index"]);
    }

    #[test]
    fn paragraph_and_section_names() {
        let got = refused(&[], &["MAIN-LINE SECTION.", "START.", "    GOBACK.", "SORT SECTION.", "EXIT-POINT.", "    EXIT."]);
        assert_eq!(got, ["7: START is a reserved word, so it cannot name a paragraph", "9: SORT is a reserved word, so it cannot name a section"]);
    }
}
