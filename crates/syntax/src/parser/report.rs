//! The REPORT SECTION, the FD REPORT clause, the report writer statements, and DECLARATIVES
//! holding USE BEFORE REPORTING procedures. Clauses of the report writer this implementation does
//! not run are refused by name.

use super::*;
use crate::report::*;

/// Words that begin a clause of a report group entry, so end an operand list before them.
const ENTRY_WORDS: &[&str] = &[
    "TYPE", "RH", "PH", "CH", "CF", "DE", "DETAIL", "PF", "RF", "REPORT", "PAGE", "CONTROL", "NEXT", "LINE", "LINES", "COLUMN", "COL", "COLUMNS",
    "COLS", "PIC", "PICTURE", "SOURCE", "SOURCES", "VALUE", "VALUES", "SUM", "GROUP", "BLANK", "JUSTIFIED", "JUST", "SIGN", "LEADING", "TRAILING",
    "USAGE", "DISPLAY", "ROUNDED", "UPON", "RESET", "IS", "ARE", "FILLER",
];

/// Report writer clauses that are refused, and how the refusal names them.
fn refused(word: &str) -> Option<&'static str> {
    Some(match word {
        "OCCURS" => "OCCURS in a report group",
        "VARYING" => "VARYING in a report group",
        "PRESENT" | "ABSENT" => "PRESENT and ABSENT clauses",
        "MULTIPLE" => "MULTIPLE PAGE",
        "REPEATED" => "the REPEATED clause",
        "WRAP" | "WITH" | "NO" => "the WRAP clause",
        "STYLE" => "the STYLE clause",
        "FUNCTION" => "the FUNCTION clause of a report group",
        "COUNT" => "the COUNT clause",
        "REDEFINES" => "REDEFINES in a report group",
        "SYNC" | "SYNCHRONIZED" => "SYNCHRONIZED in a report group",
        "GLOBAL" | "EXTERNAL" => "GLOBAL and EXTERNAL in a report group",
        "VALUE-OF" => "VALUE OF in a report group",
        _ => return None,
    })
}

/// Words that begin an RD clause, so end the CONTROL list before them.
const RD_WORDS: &[&str] = &[
    "CODE", "WITH", "CONTROL", "CONTROLS", "PAGE", "HEADING", "FIRST", "LAST", "FOOTING", "LINE", "IS", "GLOBAL", "ALLOW", "OVERFLOW", "SUM", "STYLE",
];

impl Parser<'_> {
    pub(super) fn report_section(&mut self) -> R<Vec<Report>> {
        let mut reports = Vec::new();
        while self.is_word("RD") {
            reports.push(self.report_description()?);
        }
        Ok(reports)
    }

    /// FD ... REPORT IS / REPORTS ARE: the report names, after the clause word.
    pub(super) fn report_names(&mut self) -> R<Vec<String>> {
        self.accept_any(&["IS", "ARE"]);
        if self.is_word("ALL") {
            return Err(Error::at(self.pos(), "REPORTS ARE ALL is not supported yet"));
        }
        let mut names = Vec::new();
        while let Some(w) = self.word().filter(|w| !FD_WORDS.contains(w)) {
            names.push(w.to_owned());
            self.at += 1;
        }
        if names.is_empty() {
            return Err(self.error("a report name"));
        }
        Ok(names)
    }

    pub(super) fn report_statement(&mut self, verb: &str, pos: Pos) -> R<ReportStmt> {
        Ok(match verb {
            "INITIATE" | "TERMINATE" => {
                let mut reports = Vec::new();
                while self.starts_ref() {
                    reports.push(self.name("a report name")?);
                }
                if reports.is_empty() {
                    return Err(self.error("a report name"));
                }
                if self.is_word("UPON") {
                    return Err(Error::at(pos, "INITIATE ... UPON is not supported yet"));
                }
                if verb == "INITIATE" { ReportStmt::Initiate { reports, pos } } else { ReportStmt::Terminate { reports, pos } }
            }
            "GENERATE" => {
                let name = self.name("a DETAIL group or a report name")?;
                let qualifier = if self.accept_any(&["IN", "OF"]).is_some() { Some(self.name("a report name")?) } else { None };
                ReportStmt::Generate { name, qualifier, pos }
            }
            _ => {
                self.accept_word("PRINTING");
                ReportStmt::Suppress { pos }
            }
        })
    }

    /// The PROCEDURE DIVISION's paragraphs, DECLARATIVES first when there are any.
    pub(super) fn procedure_paragraphs(&mut self, writer: &mut ReportWriter) -> R<Vec<Paragraph>> {
        if !self.is_word("DECLARATIVES") {
            return self.paragraphs();
        }
        self.at += 1;
        self.expect(&Tok::Period, "a period after DECLARATIVES")?;
        let mut paragraphs = Vec::new();
        loop {
            if self.is_word("END") && self.word_at(1) == Some("DECLARATIVES") {
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
            if self.procedure_item(&mut paragraphs)? && self.is_word("USE") {
                let used = self.use_statement(paragraphs.len() - 1)?;
                writer.uses.push(used);
            }
        }
        writer.procedure_start = paragraphs.len();
        paragraphs.extend(self.paragraphs()?);
        Ok(paragraphs)
    }

    fn use_statement(&mut self, section: usize) -> R<UseBeforeReporting> {
        let pos = self.pos();
        self.at += 1;
        if self.is_word("GLOBAL") {
            return Err(Error::at(pos, "USE GLOBAL BEFORE REPORTING is not supported yet"));
        }
        if !self.is_word("BEFORE") || self.word_at(1) != Some("REPORTING") {
            let what = if self.is_word("FOR") { "USE FOR DEBUGGING" } else { "USE AFTER STANDARD ERROR or EXCEPTION" };
            return Err(Error::at(pos, format!("{what} is not supported yet")));
        }
        self.at += 2;
        let group = self.name("a report group name")?;
        let qualifier = if self.accept_any(&["IN", "OF"]).is_some() { Some(self.name("a report name")?) } else { None };
        self.expect(&Tok::Period, "a period after USE BEFORE REPORTING")?;
        Ok(UseBeforeReporting { section, group, qualifier, pos })
    }

    fn report_description(&mut self) -> R<Report> {
        let pos = self.pos();
        self.at += 1;
        let name = self.name("a report name")?;
        let mut r = Report {
            name,
            code: None,
            controls: Vec::new(),
            page: None,
            heading: None,
            first_detail: None,
            last_detail: None,
            footing: None,
            line_limit: None,
            groups: Vec::new(),
            pos,
        };
        while !self.accept(&Tok::Period) {
            let clause_pos = self.pos();
            let clause = self.name("an RD clause or a period")?;
            match clause.as_str() {
                "IS" | "GLOBAL" => return Err(Error::at(clause_pos, "a GLOBAL report is not supported yet")),
                "WITH" | "CODE" => {
                    if clause == "WITH" {
                        self.expect_word("CODE")?;
                    }
                    self.accept_word("IS");
                    if !matches!(self.peek(), Some(Tok::Alnum(_) | Tok::Hex(_))) {
                        return Err(Error::at(clause_pos, "CODE with a mnemonic-name or an identifier is not supported yet"));
                    }
                    r.code = Some(self.literal()?);
                }
                "CONTROL" | "CONTROLS" => {
                    self.accept_any(&["IS", "ARE"]);
                    self.accept_any(&["FINAL", "REPORT"]);
                    while self.starts_ref() && !self.word().is_some_and(|w| RD_WORDS.contains(&w)) {
                        r.controls.push(self.reference()?);
                    }
                }
                "PAGE" => {
                    self.accept_any(&["LIMIT", "LIMITS"]);
                    self.accept_any(&["IS", "ARE"]);
                    r.page = Some(self.report_integer("the PAGE LIMIT")?);
                    if self.word_at(1) != Some("LIMIT") {
                        self.accept_any(&["LINE", "LINES"]);
                    }
                    if self.absolute_integer_ahead() && matches!(self.word_at(1), Some("COLUMN" | "COLUMNS")) {
                        r.line_limit = Some(self.report_integer("the line width")?);
                        self.at += 1;
                    }
                }
                "HEADING" => {
                    self.accept_word("IS");
                    r.heading = Some(self.report_integer("the HEADING line")?);
                }
                "FIRST" => {
                    if self.accept_word("BODY") {
                        self.expect_word("GROUP")?;
                    } else if self.accept_any(&["DETAIL", "DE"]).is_none() {
                        return Err(self.error("DETAIL after FIRST"));
                    }
                    self.accept_word("IS");
                    r.first_detail = Some(self.report_integer("the FIRST DETAIL line")?);
                }
                "LAST" => {
                    if self.accept_any(&["DETAIL", "DE"]).is_some() {
                        if self.accept_word("OR") && !self.accept_word("CH") {
                            self.expect_word("CONTROL")?;
                            self.expect_word("HEADING")?;
                        }
                        self.accept_word("IS");
                        if !self.absolute_integer_ahead() {
                            return Err(Error::at(clause_pos, "LAST DETAIL with an identifier is not supported yet"));
                        }
                        r.last_detail = Some(self.report_integer("the LAST DETAIL line")?);
                    } else {
                        if self.accept_word("BODY") {
                            self.expect_word("GROUP")?;
                        } else if !self.accept_word("CF") {
                            self.expect_word("CONTROL")?;
                            self.expect_word("FOOTING")?;
                        }
                        self.accept_word("IS");
                        r.footing = Some(match self.plus_integer()? {
                            Some(n) => Footing::Plus(n),
                            None => Footing::Line(self.report_integer("the LAST CONTROL FOOTING line")?),
                        });
                    }
                }
                "FOOTING" => {
                    self.accept_word("IS");
                    r.footing = Some(Footing::Line(self.report_integer("the FOOTING line")?));
                }
                "LINE" => {
                    self.expect_word("LIMIT")?;
                    self.accept_word("IS");
                    if !self.absolute_integer_ahead() {
                        return Err(Error::at(clause_pos, "LINE LIMIT with an identifier is not supported yet"));
                    }
                    r.line_limit = Some(self.report_integer("the LINE LIMIT")?);
                }
                "ALLOW" | "OVERFLOW" | "SUM" | "STYLE" => return Err(Error::at(clause_pos, format!("the {clause} clause of an RD is not supported yet"))),
                other => return Err(Error::at(clause_pos, format!("{other} is not an RD clause ironwork for COBOL supports yet"))),
            }
        }
        while let Some((level, pos)) = self.level_number()? {
            if level == 88 || level == 66 || level == 77 {
                return Err(Error::at(pos, format!("a level-{level} entry in the REPORT SECTION is not supported yet")));
            }
            if !(1..=49).contains(&level) {
                return Err(Error::at(pos, format!("level {level} is not a data level")));
            }
            let (entry, kind, next_group) = self.report_entry(level, pos)?;
            if level == 1 {
                r.groups.push(Group { name: entry.name.clone(), kind: kind.unwrap_or(GroupType::Detail), next_group, entries: vec![entry], pos });
                continue;
            }
            if kind.is_some() || next_group.is_some() {
                return Err(Error::at(pos, "TYPE and NEXT GROUP belong on a report group's 01-level entry"));
            }
            match r.groups.last_mut() {
                Some(g) => g.entries.push(entry),
                None => return Err(Error::at(pos, "a report group entry needs an 01-level entry before it")),
            }
        }
        Ok(r)
    }

    /// One report group entry, after its level number: the entry, and the TYPE and NEXT GROUP
    /// clauses that belong to the whole group.
    fn report_entry(&mut self, level: u8, pos: Pos) -> R<(Entry, Option<GroupType>, Option<NextGroup>)> {
        let mut e = Entry {
            level,
            name: None,
            line: None,
            column: None,
            picture: None,
            content: None,
            rounded: false,
            group_indicate: false,
            blank_when_zero: false,
            justified: false,
            sign: None,
            pos,
        };
        let (mut kind, mut next_group) = (None, None);
        if let Some(w) = self.word()
            && (!ENTRY_WORDS.contains(&w) || w == "FILLER")
            && refused(w).is_none()
        {
            if w != "FILLER" {
                e.name = Some(w.to_owned());
            }
            self.at += 1;
        }
        loop {
            let clause_pos = self.pos();
            let word = match self.peek() {
                Some(Tok::Period) => {
                    self.at += 1;
                    break;
                }
                Some(Tok::Alnum(_) | Tok::Hex(_) | Tok::National(_)) => {
                    let lit = self.literal()?;
                    self.report_content(&mut e, Content::Value(lit), clause_pos)?;
                    continue;
                }
                Some(Tok::Word(w)) => w.clone(),
                _ => return Err(self.error("a report group clause or a period")),
            };
            match word.as_str() {
                "TYPE" => {
                    self.at += 1;
                    self.accept_word("IS");
                    let first = self.name("a report group type")?;
                    kind = Some(self.group_type(&first)?);
                }
                "RH" | "PH" | "CH" | "CF" | "DE" | "DETAIL" | "PF" | "RF" | "CONTROL" => {
                    self.at += 1;
                    kind = Some(self.group_type(&word)?);
                }
                "REPORT" | "PAGE" if matches!(self.word_at(1), Some("HEADING" | "FOOTING")) => {
                    self.at += 1;
                    kind = Some(self.group_type(&word)?);
                }
                "NEXT" => {
                    self.at += 1;
                    next_group = Some(self.next_group_clause()?);
                }
                "LINE" | "LINES" => {
                    self.at += 1;
                    e.line = Some(self.line_clause(clause_pos)?);
                }
                "COLUMN" | "COL" | "COLUMNS" | "COLS" => {
                    self.at += 1;
                    e.column = Some(self.column_clause(clause_pos)?);
                }
                "PIC" | "PICTURE" => {
                    self.at += 1;
                    e.picture = Some(self.picture()?);
                }
                "SOURCE" | "SOURCES" => {
                    self.at += 1;
                    self.accept_any(&["IS", "ARE"]);
                    let source = self.source_operand()?;
                    self.report_content(&mut e, Content::Source(source), clause_pos)?;
                    if word == "SOURCES" && self.starts_operand() && !self.word().is_some_and(|w| ENTRY_WORDS.contains(&w)) {
                        return Err(Error::at(clause_pos, "multiple SOURCES is not supported yet"));
                    }
                }
                "VALUE" | "VALUES" => {
                    self.at += 1;
                    self.accept_any(&["IS", "ARE"]);
                    let lit = self.literal()?;
                    self.report_content(&mut e, Content::Value(lit), clause_pos)?;
                    if matches!(self.peek(), Some(Tok::Alnum(_) | Tok::Hex(_) | Tok::National(_) | Tok::Number(_))) {
                        return Err(Error::at(clause_pos, "multiple VALUES is not supported yet"));
                    }
                }
                "SUM" => {
                    self.at += 1;
                    let sum = self.sum_clause()?;
                    match &mut e.content {
                        None => e.content = Some(Content::Sum(vec![sum])),
                        Some(Content::Sum(sums)) => sums.push(sum),
                        Some(_) => return Err(Error::at(clause_pos, "SUM with SOURCE or VALUE in one entry")),
                    }
                }
                "GROUP" => {
                    self.at += 1;
                    if self.is_word("LIMIT") {
                        return Err(Error::at(clause_pos, "GROUP LIMIT is not supported yet"));
                    }
                    self.accept_word("INDICATE");
                    e.group_indicate = true;
                }
                "BLANK" => {
                    self.at += 1;
                    self.blank_when_zero()?;
                    e.blank_when_zero = true;
                }
                "JUSTIFIED" | "JUST" => {
                    self.at += 1;
                    self.accept_word("RIGHT");
                    e.justified = true;
                }
                "SIGN" | "LEADING" | "TRAILING" => {
                    self.at += 1;
                    e.sign = Some(self.sign_clause(&word)?);
                }
                "USAGE" | "DISPLAY" => {
                    self.at += 1;
                    if word == "USAGE" {
                        self.accept_word("IS");
                        let usage = self.name("a usage")?;
                        if usage != "DISPLAY" {
                            return Err(Error::at(clause_pos, format!("USAGE {usage} in a report group is not supported yet")));
                        }
                    }
                }
                "ROUNDED" => {
                    self.at += 1;
                    e.rounded = true;
                }
                w => {
                    if let Some(what) = refused(w) {
                        return Err(Error::at(clause_pos, format!("{what} is not supported yet")));
                    }
                    if ENTRY_WORDS.contains(&w) || !self.starts_operand() {
                        return Err(Error::at(clause_pos, format!("{w} is not a report group clause ironwork for COBOL supports yet")));
                    }
                    let source = self.source_operand()?;
                    self.report_content(&mut e, Content::Source(source), clause_pos)?;
                }
            }
        }
        Ok((e, kind, next_group))
    }

    fn report_content(&mut self, e: &mut Entry, content: Content, pos: Pos) -> R<()> {
        if e.content.is_some() {
            return Err(Error::at(pos, "an entry with more than one SOURCE, VALUE or SUM (a multiple-choice entry) is not supported yet"));
        }
        e.content = Some(content);
        Ok(())
    }

    fn source_operand(&mut self) -> R<Expr> {
        if self.word().is_some_and(|w| w == "SUM" || w == "COUNT") {
            return Err(Error::at(self.pos(), "a SUM or COUNT term in a SOURCE expression is not supported yet"));
        }
        self.expr()
    }

    fn group_type(&mut self, first: &str) -> R<GroupType> {
        Ok(match first {
            "RH" => GroupType::ReportHeading,
            "PH" => GroupType::PageHeading,
            "PF" => GroupType::PageFooting,
            "RF" => GroupType::ReportFooting,
            "DE" | "DETAIL" => GroupType::Detail,
            "REPORT" | "PAGE" => {
                let heading = match self.accept_any(&["HEADING", "FOOTING"]) {
                    Some(w) => w == "HEADING",
                    None => return Err(self.error("HEADING or FOOTING")),
                };
                match (first, heading) {
                    ("REPORT", true) => GroupType::ReportHeading,
                    ("REPORT", false) => GroupType::ReportFooting,
                    (_, true) => GroupType::PageHeading,
                    (_, false) => GroupType::PageFooting,
                }
            }
            "CH" | "CF" | "CONTROL" => {
                let heading = match first {
                    "CONTROL" => match self.accept_any(&["HEADING", "FOOTING"]) {
                        Some(w) => w == "HEADING",
                        None => return Err(self.error("HEADING or FOOTING")),
                    },
                    other => other == "CH",
                };
                let pos = self.pos();
                self.accept_any(&["FOR", "ON"]);
                if self.is_word("ALL") {
                    return Err(Error::at(pos, "CONTROL FOOTING FOR ALL is not supported yet"));
                }
                if heading && self.is_word("PAGE") {
                    return Err(Error::at(pos, "CONTROL HEADING ... OR PAGE is not supported yet"));
                }
                let control = self.control_name()?;
                if heading && self.is_word("OR") {
                    return Err(Error::at(pos, "CONTROL HEADING ... OR PAGE is not supported yet"));
                }
                if !heading && control.is_some() && self.control_name_ahead() {
                    return Err(Error::at(pos, "a CONTROL FOOTING for more than one control is not supported yet"));
                }
                if heading { GroupType::ControlHeading(control) } else { GroupType::ControlFooting(control) }
            }
            other => return Err(self.error(format!("{other} is not a report group type"))),
        })
    }

    fn control_name_ahead(&self) -> bool {
        self.is_word("FINAL") || self.is_word("REPORT") && !matches!(self.word_at(1), Some("HEADING" | "FOOTING")) || self.sum_operand_ahead()
    }

    fn control_name(&mut self) -> R<Option<ControlName>> {
        if self.is_word("FINAL") || self.is_word("REPORT") && !matches!(self.word_at(1), Some("HEADING" | "FOOTING")) {
            self.at += 1;
            return Ok(Some(ControlName::Final));
        }
        if self.sum_operand_ahead() {
            return Ok(Some(ControlName::Item(self.reference()?)));
        }
        Ok(None)
    }

    /// A data name that is neither a report group clause word nor a refused one.
    fn sum_operand_ahead(&self) -> bool {
        self.starts_ref() && self.word().is_some_and(|w| !ENTRY_WORDS.contains(&w) && refused(w).is_none())
    }

    fn next_group_clause(&mut self) -> R<NextGroup> {
        while self.word().is_some_and(|w| matches!(w, "BODY" | "DE" | "DETAIL" | "OR" | "CH" | "CONTROL" | "HEADING")) {
            self.at += 1;
        }
        self.expect_word("GROUP")?;
        self.accept_word("IS");
        if let Some(n) = self.plus_integer()? {
            return Ok(NextGroup::Plus(n));
        }
        if self.next_page_phrase()? {
            return Ok(NextGroup::NextPage);
        }
        Ok(NextGroup::Line(self.report_integer("a line number after NEXT GROUP")?))
    }

    fn line_clause(&mut self, pos: Pos) -> R<LineNumber> {
        self.accept_any(&["NUMBER", "NUMBERS"]);
        self.accept_any(&["IS", "ARE"]);
        let number = if let Some(n) = self.plus_integer()? {
            LineNumber::Plus(n)
        } else if self.absolute_integer_ahead() {
            let n = self.report_integer("a line number")?;
            if self.next_page_phrase()? { LineNumber::NextPage(Some(n)) } else { LineNumber::Line(n) }
        } else if self.next_page_phrase()? {
            LineNumber::NextPage(None)
        } else {
            LineNumber::Plus(1)
        };
        if self.absolute_integer_ahead() || self.plus_integer_ahead() {
            return Err(Error::at(pos, "multiple LINES is not supported yet"));
        }
        Ok(number)
    }

    fn column_clause(&mut self, pos: Pos) -> R<ColumnNumber> {
        self.accept_any(&["NUMBER", "NUMBERS"]);
        self.accept_any(&["IS", "ARE"]);
        let align = self.accept_any(&["LEFT", "RIGHT", "CENTER", "CENTRE"]);
        let column = if let Some(n) = self.plus_integer()? {
            if matches!(align.as_deref(), Some("RIGHT" | "CENTER" | "CENTRE")) {
                return Err(Error::at(pos, "COLUMN RIGHT and CENTER take an absolute column"));
            }
            ColumnNumber::Plus(n)
        } else if self.absolute_integer_ahead() {
            let n = self.report_integer("a column number")?;
            match align.as_deref() {
                Some("RIGHT") => ColumnNumber::Right(n),
                Some("CENTER" | "CENTRE") => ColumnNumber::Center(n),
                _ => ColumnNumber::Left(n),
            }
        } else if align.is_some() {
            return Err(self.error("a column number"));
        } else {
            ColumnNumber::Plus(1)
        };
        if self.absolute_integer_ahead() || self.plus_integer_ahead() {
            return Err(Error::at(pos, "multiple COLUMNS is not supported yet"));
        }
        Ok(column)
    }

    fn sum_clause(&mut self) -> R<SumClause> {
        let pos = self.pos();
        self.accept_word("OF");
        let expression = |p: &Self| matches!(p.peek(), Some(Tok::LParen | Tok::Number(_) | Tok::Plus | Tok::Minus | Tok::Star | Tok::Slash | Tok::Power));
        let mut operands = Vec::new();
        while self.sum_operand_ahead() {
            operands.push(self.reference()?);
            if expression(self) {
                return Err(Error::at(pos, "SUM of an arithmetic expression is not supported yet"));
            }
        }
        if operands.is_empty() {
            if expression(self) {
                return Err(Error::at(pos, "SUM of an arithmetic expression is not supported yet"));
            }
            return Err(self.error("a SUM operand"));
        }
        let mut upon = Vec::new();
        if self.accept_word("UPON") {
            while self.sum_operand_ahead() {
                upon.push(self.reference()?);
            }
            if upon.is_empty() {
                return Err(self.error("a DETAIL group after UPON"));
            }
        }
        let reset = if self.accept_word("RESET") {
            self.accept_word("ON");
            match self.control_name()? {
                Some(c) => Some(c),
                None => return Err(self.error("a control after RESET ON")),
            }
        } else {
            None
        };
        Ok(SumClause { operands, upon, reset })
    }

    fn absolute_integer_ahead(&self) -> bool {
        matches!(self.peek(), Some(Tok::Number(n)) if n.bytes().all(|b| b.is_ascii_digit()))
    }

    fn plus_integer_ahead(&self) -> bool {
        match self.peek() {
            Some(Tok::Number(n)) => n.starts_with('+'),
            Some(Tok::Plus) => true,
            Some(Tok::Word(w)) => w == "PLUS",
            _ => false,
        }
    }

    /// `+ n`, `+n` or `PLUS n`, consumed; None when none is at the cursor.
    fn plus_integer(&mut self) -> R<Option<u32>> {
        match self.peek() {
            Some(Tok::Number(n)) if n.starts_with('+') && n.len() > 1 && n[1..].bytes().all(|b| b.is_ascii_digit()) => {
                let value = n[1..].parse().map_err(|_| self.error("an integer"))?;
                self.at += 1;
                Ok(Some(value))
            }
            Some(Tok::Plus) => {
                self.at += 1;
                Ok(Some(self.report_integer("an integer after +")?))
            }
            Some(Tok::Word(w)) if w == "PLUS" => {
                self.at += 1;
                Ok(Some(self.report_integer("an integer after PLUS")?))
            }
            _ => Ok(None),
        }
    }

    /// ON NEXT PAGE or NEXT PAGE, consumed.
    fn next_page_phrase(&mut self) -> R<bool> {
        let on = self.is_word("ON") && self.word_at(1) == Some("NEXT") && self.word_at(2) == Some("PAGE");
        if on || self.is_word("NEXT") && self.word_at(1) == Some("PAGE") {
            self.at += if on { 3 } else { 2 };
            return Ok(true);
        }
        Ok(false)
    }

    fn report_integer(&mut self, what: &str) -> R<u32> {
        if self.accept_any(&["ZERO", "ZEROS", "ZEROES"]).is_some() {
            return Ok(0);
        }
        match self.peek() {
            Some(Tok::Number(n)) if n.bytes().all(|b| b.is_ascii_digit()) => {
                let value = n.parse().ok().filter(|&v| v <= 9999).ok_or_else(|| self.error(format!("{what} from 0 to 9999")))?;
                self.at += 1;
                Ok(value)
            }
            _ => Err(self.error(format!("expected {what}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ast::*;
    use crate::report::*;

    fn parse(lines: &[&str]) -> Program {
        let text: String = lines.iter().map(|l| format!("       {l}\n")).collect();
        crate::parse(&text).unwrap_or_else(|e| panic!("{e}"))
    }

    const HEAD: &[&str] = &[
        "IDENTIFICATION DIVISION.",
        "PROGRAM-ID. T.",
        "ENVIRONMENT DIVISION.",
        "INPUT-OUTPUT SECTION.",
        "FILE-CONTROL.",
        "    SELECT P ASSIGN TO PDD.",
        "DATA DIVISION.",
        "FILE SECTION.",
        "FD  P LABEL RECORDS ARE OMITTED",
        "    REPORTS ARE R1 R2 RECORD CONTAINS 80 CHARACTERS.",
        "WORKING-STORAGE SECTION.",
        "01  K PIC X.",
        "01  A PIC 99.",
        "REPORT SECTION.",
    ];

    #[test]
    fn rd_clauses_and_report_groups() {
        let body = [
            "RD  R1 CONTROLS ARE FINAL K PAGE LIMIT IS 60 LINES HEADING 2",
            "    FIRST DETAIL 5 LAST DETAIL 50 LAST CONTROL FOOTING +3.",
            "01  TYPE IS PAGE HEADING LINE 2 COLUMN RIGHT 40 VALUE 'TITLE'.",
            "01  ROW DE LINE PLUS 1.",
            "    05 COL 3 PIC X SOURCE K GROUP INDICATE.",
            "    05 R-A COLUMN + 2 PIC Z9 A.",
            "01  CF K NEXT GROUP NEXT PAGE.",
            "    05 LINE NUMBER IS PLUS 2 COLUMN 1 PIC ZZ9",
            "       SUM R-A UPON ROW RESET ON FINAL.",
            "01  TYPE RF LINE 3 ON NEXT PAGE COL 1 'END'.",
            "RD  R2 CODE 'X'.",
            "01  D TYPE DETAIL LINE COLUMN VALUE 'Y'.",
            "PROCEDURE DIVISION.",
            "    INITIATE R1 R2 GENERATE ROW IN R1 GENERATE R1",
            "    TERMINATE R1 R2 GOBACK.",
        ];
        let p = parse(&[HEAD, &body[..]].concat());
        assert_eq!(p.files[0].reports, ["R1", "R2"]);
        let rw = &p.report_writer;
        let r1 = &rw.reports[0];
        assert_eq!((r1.page, r1.heading, r1.first_detail, r1.last_detail, r1.footing), (Some(60), Some(2), Some(5), Some(50), Some(Footing::Plus(3))));
        assert_eq!(r1.controls.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["K"]);
        assert_eq!(r1.groups.iter().map(|g| g.kind.clone()).collect::<Vec<_>>()[..2], [GroupType::PageHeading, GroupType::Detail]);
        let ph = &r1.groups[0].entries[0];
        assert_eq!((ph.line, ph.column), (Some(LineNumber::Line(2)), Some(ColumnNumber::Right(40))));
        let row = &r1.groups[1];
        assert_eq!(row.name.as_deref(), Some("ROW"));
        assert!(row.entries[1].group_indicate);
        assert_eq!((row.entries[2].name.as_deref(), row.entries[2].column), (Some("R-A"), Some(ColumnNumber::Plus(2))));
        assert!(matches!(&row.entries[2].content, Some(Content::Source(Expr::Operand(Operand::Ref(r)))) if r.name == "A"));
        let cf = &r1.groups[2];
        assert_eq!(cf.next_group, Some(NextGroup::NextPage));
        assert!(matches!(&cf.kind, GroupType::ControlFooting(Some(ControlName::Item(k))) if k.name == "K"));
        let Some(Content::Sum(sums)) = &cf.entries[1].content else { panic!() };
        assert_eq!((sums[0].operands[0].name.as_str(), sums[0].upon[0].name.as_str(), &sums[0].reset), ("R-A", "ROW", &Some(ControlName::Final)));
        assert_eq!(r1.groups[3].entries[0].line, Some(LineNumber::NextPage(Some(3))));
        let r2 = &rw.reports[1];
        assert_eq!(r2.code, Some(Literal::Alnum("X".into())));
        assert_eq!((r2.groups[0].entries[0].line, r2.groups[0].entries[0].column), (Some(LineNumber::Plus(1)), Some(ColumnNumber::Plus(1))));
        let s = &p.paragraphs[0].statements;
        assert!(matches!(&s[0], Stmt::Report(r) if matches!(&**r, ReportStmt::Initiate { reports, .. } if reports == &["R1", "R2"])));
        assert!(matches!(&s[1], Stmt::Report(r) if matches!(&**r, ReportStmt::Generate { name, qualifier: Some(q), .. } if name == "ROW" && q == "R1")));
        assert!(matches!(&s[2], Stmt::Report(r) if matches!(&**r, ReportStmt::Generate { name, qualifier: None, .. } if name == "R1")));
    }

    #[test]
    fn declaratives_come_first_and_the_procedure_starts_after_them() {
        let body = [
            "RD  R1.",
            "01  ROW TYPE DE LINE PLUS 1 COLUMN 1 PIC X SOURCE K.",
            "RD  R2.",
            "01  D2 TYPE DE LINE PLUS 1 COLUMN 1 PIC X SOURCE K.",
            "PROCEDURE DIVISION.",
            "DECLARATIVES.",
            "U1 SECTION.",
            "    USE BEFORE REPORTING ROW.",
            "U1-A.",
            "    SUPPRESS PRINTING.",
            "HELPER SECTION.",
            "    MOVE 'Z' TO K.",
            "END DECLARATIVES.",
            "MAIN SECTION.",
            "    GOBACK.",
        ];
        let p = parse(&[HEAD, &body[..]].concat());
        let rw = &p.report_writer;
        assert_eq!(rw.procedure_start, 3);
        assert_eq!((rw.uses[0].section, rw.uses[0].group.as_str()), (0, "ROW"));
        assert_eq!(p.paragraphs.iter().map(|q| q.name.as_str()).collect::<Vec<_>>(), ["U1", "U1-A", "HELPER", "MAIN"]);
        assert!(matches!(&p.paragraphs[1].statements[0], Stmt::Report(r) if matches!(**r, ReportStmt::Suppress { .. })));
    }
}
