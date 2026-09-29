//! SORT, MERGE, RELEASE and RETURN, and the alphabets a COLLATING SEQUENCE names.

use super::*;

/// Words that end a list of keys in SORT and MERGE, beyond the phrase words.
const SORT_PHRASES: &[&str] = &["ASCENDING", "DESCENDING", "DUPLICATES", "COLLATING", "SEQUENCE"];

impl Parser<'_> {
    /// The ENVIRONMENT DIVISION clauses that bear on sorting: SPECIAL-NAMES ALPHABET and
    /// OBJECT-COMPUTER PROGRAM COLLATING SEQUENCE, and SAME RECORD AREA, which is refused. False
    /// when the cursor is at none of them. A literal alphabet is kept as "literal", its literals
    /// left unread.
    pub(super) fn sort_environment(&mut self, collating: &mut Collating) -> R<bool> {
        if self.is_word("SAME") && self.word_at(1) == Some("RECORD") {
            return Err(self.error("SAME RECORD AREA is not supported yet"));
        }
        if self.accept_word("ALPHABET") {
            let name = self.name("an alphabet-name")?;
            if self.accept_word("FOR") {
                self.accept_word("ALPHANUMERIC");
            }
            self.accept_word("IS");
            let kind = match self.word() {
                Some(w @ ("EBCDIC" | "NATIVE" | "STANDARD-1" | "STANDARD-2")) => w.to_owned(),
                _ => "literal".to_owned(),
            };
            if kind != "literal" {
                self.at += 1;
            }
            collating.alphabets.push((name, kind));
            return Ok(true);
        }
        if self.is_word("PROGRAM") && self.word_at(1) == Some("COLLATING") {
            self.at += 1;
        }
        if !(self.is_word("COLLATING") && self.word_at(1) == Some("SEQUENCE")) {
            return Ok(false);
        }
        self.at += 2;
        if self.accept_word("FOR") {
            self.accept_word("ALPHANUMERIC");
        }
        self.accept_word("IS");
        collating.program = Some(self.name("an alphabet-name")?);
        Ok(true)
    }

    pub(super) fn sorting(&mut self, verb: &str, pos: Pos) -> R<Sorting> {
        match verb {
            "RELEASE" => {
                let record = self.reference()?;
                let from = if self.accept_word("FROM") { Some(self.operand()?) } else { None };
                Ok(Sorting::Release { record, from, pos })
            }
            "RETURN" => {
                let file = self.name("a sort or merge file")?;
                self.accept_word("RECORD");
                let into = if self.accept_word("INTO") { Some(self.reference()?) } else { None };
                let mut at_end = Handlers::default();
                loop {
                    let negated = self.is_word("NOT") && matches!(self.word_at(1), Some("AT" | "END"));
                    if !negated && !self.is_word("AT") && !self.is_word("END") {
                        break;
                    }
                    if negated {
                        self.at += 1;
                    }
                    self.accept_word("AT");
                    self.expect_word("END")?;
                    let body = self.block(&["NOT", "END-RETURN"])?;
                    if negated { at_end.not_on = Some(body) } else { at_end.on = Some(body) }
                }
                self.accept_word("END-RETURN");
                Ok(Sorting::Return { file, into, at_end, pos })
            }
            _ => Ok(Sorting::Sort(self.sort_stmt(verb == "MERGE", pos)?)),
        }
    }

    fn sort_stmt(&mut self, merge: bool, pos: Pos) -> R<SortStmt> {
        let subject = self.reference()?;
        let mut keys = Vec::new();
        loop {
            if self.is_word("ON") && matches!(self.word_at(1), Some("ASCENDING" | "DESCENDING")) {
                self.at += 1;
            }
            let Some(order) = self.accept_any(&["ASCENDING", "DESCENDING"]) else { break };
            let ascending = order == "ASCENDING";
            self.accept_word("KEY");
            self.accept_word("IS");
            let before = keys.len();
            while self.starts_ref() && !self.word().is_some_and(|w| SORT_PHRASES.contains(&w)) {
                keys.push((ascending, self.reference()?));
            }
            if keys.len() == before {
                keys.push((ascending, Ref { subscripts: Vec::new(), refmod: None, ..subject.clone() }));
            }
        }
        let duplicates = !merge && (self.is_word("DUPLICATES") || self.is_word("WITH") && self.word_at(1) == Some("DUPLICATES"));
        if duplicates {
            self.accept_word("WITH");
            self.at += 1;
            self.accept_word("IN");
            self.accept_word("ORDER");
        }
        let mut collating = None;
        if self.accept_word("COLLATING") || self.is_word("SEQUENCE") {
            self.expect_word("SEQUENCE")?;
            self.accept_word("IS");
            collating = Some(self.name("an alphabet-name")?);
        }
        let input = if self.accept_word("USING") {
            Some(SortIo::Files(self.sort_files("USING")?))
        } else if self.is_word("INPUT") && self.word_at(1) == Some("PROCEDURE") {
            if merge {
                return Err(self.error("MERGE takes its input from USING files, not an INPUT PROCEDURE"));
            }
            self.at += 2;
            Some(self.sort_procedure()?)
        } else {
            None
        };
        let output = if self.accept_word("GIVING") {
            Some(SortIo::Files(self.sort_files("GIVING")?))
        } else if self.is_word("OUTPUT") && self.word_at(1) == Some("PROCEDURE") {
            self.at += 2;
            Some(self.sort_procedure()?)
        } else {
            None
        };
        Ok(SortStmt { merge, subject, keys, duplicates, collating, input, output, pos })
    }

    fn sort_files(&mut self, phrase: &str) -> R<Vec<String>> {
        let mut files = Vec::new();
        while self.starts_ref() {
            files.push(self.name("a file name")?);
        }
        if files.is_empty() {
            return Err(self.error(format!("a file after {phrase}")));
        }
        Ok(files)
    }

    fn sort_procedure(&mut self) -> R<SortIo> {
        self.accept_word("IS");
        let from = self.proc_name()?;
        let thru = if self.accept_any(&["THRU", "THROUGH"]).is_some() { Some(self.proc_name()?) } else { None };
        Ok(SortIo::Procedure { from, thru })
    }
}

#[cfg(test)]
mod tests {
    use crate::ast::*;

    const HEAD: &str = concat!(
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. T.\n       ENVIRONMENT DIVISION.\n       CONFIGURATION SECTION.\n",
        "       OBJECT-COMPUTER. IBM-370 PROGRAM COLLATING SEQUENCE IS EB.\n",
        "       SPECIAL-NAMES. ALPHABET EB IS EBCDIC ALPHABET AS IS STANDARD-1\n",
        "           ALPHABET MINE IS 'A' THRU 'Z'.\n",
        "       INPUT-OUTPUT SECTION.\n       FILE-CONTROL.\n           SELECT W ASSIGN TO SORTWK1.\n",
        "           SELECT A ASSIGN TO ADD.\n           SELECT B ASSIGN TO BDD.\n",
        "       DATA DIVISION.\n       FILE SECTION.\n       SD  W RECORD CONTAINS 2 TO 4 CHARACTERS\n           DATA RECORD IS W-REC.\n",
        "       01  W-REC.\n           05 W-K1 PIC XX.\n           05 W-K2 PIC S9(3) COMP-3.\n",
        "       FD  A RECORD IS VARYING IN SIZE.\n       01  A-REC PIC X(4).\n       FD  B.\n       01  B-REC PIC X(4).\n",
        "       PROCEDURE DIVISION.\n",
    );

    fn statements(body: &str) -> Vec<Stmt> {
        let p = crate::parse(&format!("{HEAD}{body}")).unwrap_or_else(|e| panic!("{e}"));
        p.paragraphs.into_iter().flat_map(|p| p.statements).filter(|s| *s != Stmt::SentenceEnd).collect()
    }

    fn sort(s: &Stmt) -> &SortStmt {
        match s {
            Stmt::Sorting(b) => match b.as_ref() {
                Sorting::Sort(s) => s,
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn sort_with_keys_duplicates_and_files() {
        let s = statements(
            "           SORT W ON ASCENDING KEY W-K1 DESCENDING W-K2\n               WITH DUPLICATES IN ORDER COLLATING SEQUENCE IS EB\n               USING A B GIVING B.\n",
        );
        let s = sort(&s[0]);
        assert!(!s.merge && s.duplicates);
        assert_eq!(s.keys.iter().map(|(a, r)| (*a, r.name.as_str())).collect::<Vec<_>>(), [(true, "W-K1"), (false, "W-K2")]);
        assert_eq!(s.collating.as_deref(), Some("EB"));
        assert_eq!(s.input, Some(SortIo::Files(vec!["A".into(), "B".into()])));
        assert_eq!(s.output, Some(SortIo::Files(vec!["B".into()])));
    }

    #[test]
    fn procedures_release_and_return() {
        let s = statements(concat!(
            "           SORT W ASCENDING W-K1 INPUT PROCEDURE IS P-IN\n               OUTPUT PROCEDURE P-OUT THRU P-END.\n",
            "       P-IN.\n           RELEASE W-REC FROM A-REC.\n",
            "       P-OUT.\n           RETURN W RECORD INTO B-REC AT END CONTINUE\n               NOT AT END DISPLAY B-REC END-RETURN.\n",
            "       P-END.\n           EXIT.\n",
        ));
        let sorted = sort(&s[0]);
        let Some(SortIo::Procedure { from, thru: Some(thru) }) = &sorted.output else { panic!("{sorted:?}") };
        assert_eq!((from.name.as_str(), thru.name.as_str()), ("P-OUT", "P-END"));
        assert!(matches!(&s[1], Stmt::Sorting(b) if matches!(b.as_ref(), Sorting::Release { from: Some(_), .. })));
        let Stmt::Sorting(r) = &s[2] else { panic!() };
        let Sorting::Return { file, into: Some(_), at_end, .. } = r.as_ref() else { panic!("{r:?}") };
        assert!(file == "W" && at_end.on.is_some() && at_end.not_on.is_some());
    }

    #[test]
    fn merge_and_the_alphabets() {
        let p = crate::parse(&format!("{HEAD}           MERGE W ON DESCENDING KEY W-K2 USING A B OUTPUT PROCEDURE P.\n       P.\n           EXIT.\n")).unwrap();
        assert!(p.files[0].sort && !p.files[1].sort);
        assert_eq!((p.files[0].record_min, p.files[0].record_max), (Some(2), Some(4)));
        assert_eq!((p.files[1].recording, p.files[2].recording), (Some('V'), None));
        assert_eq!(p.collating.program.as_deref(), Some("EB"));
        let alphabets: Vec<(&str, &str)> = p.collating.alphabets.iter().map(|(n, k)| (n.as_str(), k.as_str())).collect();
        assert_eq!(alphabets, [("EB", "EBCDIC"), ("AS", "STANDARD-1"), ("MINE", "literal")]);
        assert!(sort(&p.paragraphs[0].statements[0]).merge);
        let refused = crate::parse(&format!("{HEAD}           MERGE W ON ASCENDING KEY W-K1 INPUT PROCEDURE P.\n")).unwrap_err();
        assert!(refused.message.contains("not an INPUT PROCEDURE"), "{}", refused.message);
        let shared = HEAD.replace("       DATA DIVISION.", "       I-O-CONTROL.\n           SAME RECORD AREA FOR W A.\n       DATA DIVISION.");
        assert!(crate::parse(&shared).unwrap_err().message.contains("SAME RECORD AREA is not supported yet"));
    }

    #[test]
    fn a_table_sort_without_key_names_sorts_on_the_element() {
        let s = statements("           SORT W-TAB ON DESCENDING KEY.\n           SORT W-TAB(1).\n");
        let first = sort(&s[0]);
        assert_eq!((first.keys.len(), first.keys[0].0, first.keys[0].1.name.as_str()), (1, false, "W-TAB"));
        assert!(first.input.is_none() && first.output.is_none());
        assert!(sort(&s[1]).keys.is_empty());
    }
}
