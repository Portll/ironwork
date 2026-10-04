//! DECLARATIVES: the sections that open the PROCEDURE DIVISION, each headed by a USE statement that
//! says when it runs (Language Reference SC27-8713-03, pp. 264-265 and 713-716).

use super::*;
use crate::report::{ReportWriter, UseBeforeReporting};

/// What a USE statement makes of its section.
enum Use {
    Reporting(UseBeforeReporting),
    Error(UseAfterError),
    Debugging(UseForDebugging),
    /// USE FOR DEBUGGING without WITH DEBUGGING MODE: the whole section is a comment (p. 772).
    Comment,
}

impl Parser<'_> {
    /// The PROCEDURE DIVISION's paragraphs, DECLARATIVES first when there are any.
    pub(super) fn procedure_paragraphs(&mut self, writer: &mut ReportWriter, declaratives: &mut Declaratives) -> R<Vec<Paragraph>> {
        if !self.is_word("DECLARATIVES") {
            return self.paragraphs();
        }
        self.at += 1;
        self.expect(&Tok::Period, "a period after DECLARATIVES")?;
        let mut paragraphs = Vec::new();
        loop {
            if self.at_end_declaratives() {
                self.at += 2;
                self.expect(&Tok::Period, "a period after END DECLARATIVES")?;
                break;
            }
            if self.peek().is_none() || self.at_end_program() || self.at_division(&["IDENTIFICATION", "ID"]) {
                return Err(self.error("END DECLARATIVES"));
            }
            if paragraphs.is_empty() && !self.section_header() {
                return Err(self.error("a section in DECLARATIVES"));
            }
            if !self.procedure_item(&mut paragraphs)? || !self.is_word("USE") {
                continue;
            }
            match self.use_statement(paragraphs.len() - 1)? {
                Use::Reporting(u) => writer.uses.push(u),
                Use::Error(u) => declaratives.errors.push(u),
                Use::Debugging(u) => declaratives.debugging.push(u),
                Use::Comment => {
                    paragraphs.pop();
                    self.skip_debugging_section();
                    continue;
                }
            }
            if self.at_end_declaratives() || self.section_header() {
                let section = &paragraphs[paragraphs.len() - 1].name;
                self.messages.push(Error::at(self.pos(), format!("{section} SECTION: no paragraph-name after its USE statement")).graded(crate::Severity::Informational));
            }
        }
        writer.procedure_start = paragraphs.len();
        paragraphs.extend(self.paragraphs()?);
        Ok(paragraphs)
    }

    fn at_end_declaratives(&self) -> bool {
        self.is_word("END") && self.word_at(1) == Some("DECLARATIVES")
    }

    /// Past a debugging section read as a comment, to the next section header or END DECLARATIVES.
    fn skip_debugging_section(&mut self) {
        while self.peek().is_some() && !self.at_end_declaratives() && !self.at_end_program() && !(self.section_header() && self.tokens[self.at].area_a) {
            self.at += 1;
        }
    }

    fn use_statement(&mut self, section: usize) -> R<Use> {
        let pos = self.pos();
        self.at += 1;
        let global = self.accept_word("GLOBAL");
        if self.is_word("BEFORE") && self.word_at(1) == Some("REPORTING") {
            return Ok(Use::Reporting(self.use_before_reporting(section, global, pos)?));
        }
        if self.accept_word("AFTER") {
            self.accept_word("STANDARD");
            if self.accept_any(&["EXCEPTION", "ERROR"]).is_none() {
                return Err(self.error("EXCEPTION or ERROR"));
            }
            self.expect_word("PROCEDURE")?;
            self.accept_word("ON");
            let on = match self.accept_any(&["INPUT", "OUTPUT", "I-O", "EXTEND"]).as_deref() {
                Some("INPUT") => ErrorUse::Mode(OpenMode::Input),
                Some("OUTPUT") => ErrorUse::Mode(OpenMode::Output),
                Some("I-O") => ErrorUse::Mode(OpenMode::InputOutput),
                Some(_) => ErrorUse::Mode(OpenMode::Extend),
                None => {
                    let mut files = Vec::new();
                    while self.word().is_some() {
                        files.push(self.name("a file name")?);
                    }
                    if files.is_empty() {
                        return Err(self.error("a file name, INPUT, OUTPUT, I-O or EXTEND"));
                    }
                    ErrorUse::Files(files)
                }
            };
            self.expect(&Tok::Period, "a period after the USE statement")?;
            return Ok(Use::Error(UseAfterError { section, global, on, pos }));
        }
        if !global && (self.is_word("FOR") || self.is_word("DEBUGGING")) {
            self.accept_word("FOR");
            self.expect_word("DEBUGGING")?;
            if !self.debugging {
                return Ok(Use::Comment);
            }
            self.accept_word("ON");
            let mut procedures = Vec::new();
            if self.accept_word("ALL") {
                if !self.accept_word("PROCEDURES") {
                    return Err(Error::at(pos, "USE FOR DEBUGGING ON ALL: Enterprise COBOL debugs procedures, by name or as ALL PROCEDURES, and no other items"));
                }
            } else {
                while self.peek().is_some_and(|t| *t != Tok::Period) {
                    procedures.push(self.proc_name()?);
                }
                if procedures.is_empty() {
                    return Err(self.error("ALL PROCEDURES or a procedure name"));
                }
            }
            self.expect(&Tok::Period, "a period after the USE statement")?;
            return Ok(Use::Debugging(UseForDebugging { section, procedures, pos }));
        }
        Err(self.error("AFTER, FOR DEBUGGING or BEFORE REPORTING after USE"))
    }
}

/// Debugging sections are not allowed in a method or a RECURSIVE program (p. 715).
pub(super) fn debugging_sections_allowed(declaratives: &Declaratives, recursive: bool, method: bool) -> R<()> {
    match declaratives.debugging.first() {
        Some(u) if method => Err(Error::at(u.pos, "USE FOR DEBUGGING is not allowed in a method")),
        Some(u) if recursive => Err(Error::at(u.pos, "USE FOR DEBUGGING is not allowed in a RECURSIVE program")),
        _ => Ok(()),
    }
}

/// The rules the programs a program contains bring: debugging sections belong only to the
/// outermost program (p. 715), and USE GLOBAL BEFORE REPORTING for a contained program's report
/// group is refused (assumption C69).
pub(super) fn contained_programs(writer: &ReportWriter, nested: &[Program]) -> R<()> {
    if let Some(u) = nested.iter().find_map(|p| p.declaratives.debugging.first()) {
        return Err(Error::at(u.pos, "USE FOR DEBUGGING in a contained program: debugging sections are allowed only in the outermost program"));
    }
    if nested.is_empty() {
        return Ok(());
    }
    for u in writer.uses.iter().filter(|u| u.global) {
        let own = |p: &Program| p.report_writer.uses.iter().any(|v| v.group == u.group);
        let has_group = |p: &Program| p.report_writer.reports.iter().flat_map(|r| &r.groups).any(|g| g.name.as_deref() == Some(u.group.as_str()));
        if nested.iter().any(|p| has_group(p) && !own(p)) {
            return Err(crate::messages::IWR0023.at(u.pos, format!("USE GLOBAL BEFORE REPORTING {} for a report group of a contained program is not supported yet", u.group)));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::ast::*;

    fn parse(text: &str) -> Result<Vec<Program>, crate::Error> {
        crate::parse_all_with(text, &crate::copy::Libraries::default())
    }

    fn program(mode: &str, declaratives: &str) -> String {
        [
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n",
            &format!("       SOURCE-COMPUTER. IBM-370{mode}.\n"),
            "       INPUT-OUTPUT SECTION.\n       FILE-CONTROL.\n           SELECT F ASSIGN TO FDD.\n",
            "       DATA DIVISION.\n       FILE SECTION.\n       FD  F.\n       01  R PIC X.\n",
            "       PROCEDURE DIVISION.\n       DECLARATIVES.\n",
            declaratives,
            "       END DECLARATIVES.\n       MAIN SECTION.\n       M.\n           GOBACK.\n",
        ]
        .concat()
    }

    #[test]
    fn each_form_of_use_after_exception_or_error() {
        let text = program(
            "",
            "       E1 SECTION.\n           USE AFTER STANDARD ERROR PROCEDURE ON F.\n       E1-A.\n           DISPLAY 'E1'.\n       E2 SECTION.\n           USE GLOBAL AFTER EXCEPTION PROCEDURE I-O.\n       E3 SECTION.\n           USE AFTER ERROR PROCEDURE EXTEND.\n",
        );
        let p = &parse(&text).unwrap_or_else(|e| panic!("{e}"))[0];
        let uses: Vec<(usize, bool, ErrorUse)> = p.declaratives.errors.iter().map(|u| (u.section, u.global, u.on.clone())).collect();
        assert_eq!(
            uses,
            [(0, false, ErrorUse::Files(vec!["F".into()])), (2, true, ErrorUse::Mode(OpenMode::InputOutput)), (3, false, ErrorUse::Mode(OpenMode::Extend))]
        );
        assert_eq!(p.report_writer.procedure_start, 4);
        assert!(parse(&program("", "       E SECTION.\n           USE AFTER EXCEPTION CONDITION EC-SIZE.\n")).unwrap_err().message.contains("expected PROCEDURE"));
    }

    #[test]
    fn a_section_that_ends_right_after_its_use_statement_is_noted() {
        let text = program(
            " WITH DEBUGGING MODE",
            "       E1 SECTION.\n           USE AFTER ERROR PROCEDURE ON F.\n       E2 SECTION.\n           USE FOR DEBUGGING ON M.\n       E2-A.\n           DISPLAY 'E2'.\n       E3 SECTION.\n           USE AFTER ERROR PROCEDURE INPUT.\n",
        );
        let p = &parse(&text).unwrap_or_else(|e| panic!("{e}"))[0];
        let messages: Vec<(u32, &str, crate::Severity)> = p.messages.iter().map(|m| (m.pos.line, m.message.as_str(), m.severity)).collect();
        assert_eq!(
            messages,
            [
                (17, "E1 SECTION: no paragraph-name after its USE statement", crate::Severity::Informational),
                (23, "E3 SECTION: no paragraph-name after its USE statement", crate::Severity::Informational),
            ]
        );
        let comment = program("", "       D SECTION.\n           USE FOR DEBUGGING ON M.\n");
        assert!(parse(&comment).unwrap_or_else(|e| panic!("{e}"))[0].messages.is_empty());
    }

    #[test]
    fn a_debugging_section_is_a_comment_without_debugging_mode() {
        let section = "       D SECTION.\n           USE FOR DEBUGGING ON ALL REFERENCES OF R.\n       D-1.\n           ALTER X TO PROCEED TO Y.\n       E SECTION.\n           USE AFTER ERROR PROCEDURE INPUT.\n";
        let p = &parse(&program("", section)).unwrap_or_else(|e| panic!("{e}"))[0];
        assert!(p.declaratives.debugging.is_empty());
        assert_eq!(p.paragraphs[0].name, "E");
        assert_eq!(p.declaratives.errors[0].section, 0);
        let refused = parse(&program(" WITH DEBUGGING MODE", section)).unwrap_err();
        assert!(refused.message.contains("debugs procedures"), "{refused}");
    }

    #[test]
    fn use_for_debugging_names_procedures_or_all_of_them() {
        let text = program(" WITH DEBUGGING MODE", "       D1 SECTION.\n           USE FOR DEBUGGING ON M OF MAIN MAIN.\n       D2 SECTION.\n           USE FOR DEBUGGING ALL PROCEDURES.\n");
        let p = &parse(&text).unwrap_or_else(|e| panic!("{e}"))[0];
        let named: Vec<(&str, Option<&str>)> = p.declaratives.debugging[0].procedures.iter().map(|n| (n.name.as_str(), n.section.as_deref())).collect();
        assert_eq!(named, [("M", Some("MAIN")), ("MAIN", None)]);
        assert!(p.declaratives.debugging[1].procedures.is_empty());
    }

    #[test]
    fn debugging_sections_only_in_the_outermost_program() {
        let inner = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. INNER.\n       PROCEDURE DIVISION.\n       DECLARATIVES.\n       D SECTION.\n           USE FOR DEBUGGING ON P.\n       END DECLARATIVES.\n       P.\n           GOBACK.\n       END PROGRAM INNER.\n";
        let text = [program(" WITH DEBUGGING MODE", ""), inner.into(), "       END PROGRAM T.\n".into()].concat();
        assert!(parse(&text).unwrap_err().message.contains("only in the outermost program"));
        let recursive = program(" WITH DEBUGGING MODE", "       D SECTION.\n           USE FOR DEBUGGING ON M.\n").replace("PROGRAM-ID. T.", "PROGRAM-ID. T RECURSIVE.");
        assert!(parse(&recursive).unwrap_err().message.contains("RECURSIVE"));
    }

    #[test]
    fn global_reporting_declaratives_for_a_contained_program_are_refused() {
        let inner = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. INNER.\n       PROCEDURE DIVISION.\n           GOBACK.\n       END PROGRAM INNER.\n";
        let with = |declaratives: &str| [program("", declaratives), inner.into(), "       END PROGRAM T.\n".into()].concat();
        assert!(parse(&with("       E SECTION.\n           USE GLOBAL AFTER ERROR PROCEDURE ON INPUT.\n")).is_ok());
        assert!(parse(&with("       E SECTION.\n           USE GLOBAL AFTER ERROR PROCEDURE ON F.\n")).is_ok());
        let reporting = [
            program("", "       U SECTION.\n           USE GLOBAL BEFORE REPORTING ROW.\n"),
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. INNER.\n       DATA DIVISION.\n       REPORT SECTION.\n       RD  R.\n".into(),
            "       01  ROW TYPE DE LINE PLUS 1 COLUMN 1 VALUE 'X'.\n       PROCEDURE DIVISION.\n           GOBACK.\n       END PROGRAM INNER.\n       END PROGRAM T.\n".into(),
        ]
        .concat();
        assert!(parse(&reporting).unwrap_err().message.contains("USE GLOBAL BEFORE REPORTING ROW"));
    }
}
