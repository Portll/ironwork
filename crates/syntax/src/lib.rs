//! ironwork for COBOL: from fixed-format source text to a syntax tree. Nothing here knows about
//! storage or execution.

pub mod ast;
pub mod bms;
pub mod copy;
pub mod csd;
mod debugging;
mod directives;
pub mod dli;
pub mod extended;
pub mod feedback;
pub mod jni;
pub mod lexer;
pub mod messages;
pub mod parser;
pub mod report;
pub mod source;
pub mod sql;
pub mod system;

use std::fmt;

pub use rt::vocab::Pos;

/// The severity of a compiler message, IBM's five levels in order (Programming Guide SC27-8714-03,
/// Table 38, p. 282).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// I: the program runs correctly.
    Informational,
    /// W: a possible error; the program probably runs correctly.
    Warning,
    /// E: an error IBM's compiler corrects, though the program may not run as expected.
    Error,
    /// S: an error the compiler cannot correct; the program should not be run.
    Severe,
    /// U: the compilation ended.
    Unrecoverable,
}

impl Severity {
    /// The return code a compilation ends with when this is its most severe message.
    pub const fn return_code(self) -> u8 {
        match self {
            Self::Informational => 0,
            Self::Warning => 4,
            Self::Error => 8,
            Self::Severe => 12,
            Self::Unrecoverable => 16,
        }
    }

    /// The word a message's line carries after its position; an error's carries none.
    pub const fn label(self) -> Option<&'static str> {
        match self {
            Self::Informational => Some("informational"),
            Self::Warning => Some("warning"),
            Self::Error | Self::Severe | Self::Unrecoverable => None,
        }
    }

    /// The letter after a message id's number: `IWC0101-S`.
    pub const fn letter(self) -> char {
        match self {
            Self::Informational => 'I',
            Self::Warning => 'W',
            Self::Error => 'E',
            Self::Severe => 'S',
            Self::Unrecoverable => 'U',
        }
    }
}

/// A compiler message; [`Error::at`] makes a severe one, and [`messages::Message::at`] one from the
/// catalogue, with its id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub pos: Pos,
    pub message: String,
    /// The COPY member the position is in, when it is not the program itself.
    pub file: Option<String>,
    pub severity: Severity,
    /// The catalogue's id for the message, without its severity letter (docs/messages.md).
    pub id: Option<&'static str>,
}

impl Error {
    pub fn at(pos: Pos, message: impl Into<String>) -> Self {
        Self { pos, message: message.into(), file: None, severity: Severity::Severe, id: None }
    }

    pub fn warning(pos: Pos, message: impl Into<String>) -> Self {
        Self::at(pos, message).graded(Severity::Warning)
    }

    pub fn graded(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// Names the member a position in a COPY member came from.
    pub fn in_files(mut self, files: &[String]) -> Self {
        self.file = files.get(self.pos.file as usize).filter(|f| !f.is_empty()).cloned();
        self
    }

    /// `file:line:col: message`, with `main` naming the program itself; a warning or informational
    /// message puts its label before the message, and an id goes before the message:
    /// `file:line:col: warning: IWX0001-W message`.
    pub fn place(&self, main: &str) -> String {
        match (&self.file, self.pos.line) {
            (Some(f), _) => format!("{f}:{self}"),
            (None, 0) => format!("{main}: {}", self.labelled()),
            (None, _) => format!("{main}:{self}"),
        }
    }

    /// The message after its id, and both after its severity's label when it has one.
    pub fn labelled(&self) -> String {
        let id = self.id.map(|id| format!("{id}-{} ", self.severity.letter())).unwrap_or_default();
        match self.severity.label() {
            Some(label) => format!("{label}: {id}{}", self.message),
            None => format!("{id}{}", self.message),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.pos, self.labelled())
    }
}

/// The return code of a compilation that gave these messages: the highest of theirs, 0 for none.
pub fn return_code(messages: &[Error]) -> u8 {
    messages.iter().map(|m| m.severity.return_code()).max().unwrap_or(0)
}

/// The message a refusal names: the first of the most severe.
pub fn most_severe(messages: &[Error]) -> Option<&Error> {
    messages.iter().rev().max_by_key(|m| m.severity)
}

impl std::error::Error for Error {}

/// The first program in the source.
pub fn parse(text: &str) -> Result<ast::Program, Error> {
    parse_with(text, &copy::Libraries::default())
}

/// The first program in the source, with COPY members resolved from `libraries`.
pub fn parse_with(text: &str, libraries: &copy::Libraries) -> Result<ast::Program, Error> {
    Ok(parse_all_with(text, libraries)?.remove(0))
}

/// The names after each PROGRAM-ID in the source, read without COPY members or a parse; empty for
/// a source that does not lex.
pub fn program_ids(text: &str, compliance: numeric::Compliance) -> Vec<String> {
    let Ok(tokens) = source::read_under(text, 0, false, compliance).and_then(|s| lexer::lex_under(&s, compliance)) else { return Vec::new() };
    let mut names = Vec::new();
    let mut rest = tokens.iter().map(|t| &t.tok);
    while let Some(tok) = rest.next() {
        if !matches!(tok, lexer::Tok::Word(w) if w == "PROGRAM-ID") {
            continue;
        }
        let name = match rest.next() {
            Some(lexer::Tok::Period) => rest.next(),
            other => other,
        };
        if let Some(lexer::Tok::Word(n) | lexer::Tok::Alnum(n)) = name {
            names.push(n.clone());
        }
    }
    names
}

/// Every program in the source, in order, nested programs after the one that contains them. The
/// libraries' compliance level and source format say how the source and its members are read.
/// Debugging lines are program text through COPY and REPLACE, and comments after them outside a
/// program compiled WITH DEBUGGING MODE (Language Reference SC27-8713-03, p. 693). Outside debugging
/// mode Enterprise COBOL accepts a debugging line that does not read as text, such as one holding an
/// unclosed literal; then every debugging line is read as a comment.
///
/// Under `--compliance extended` and `--source-format auto`, a file read in fixed form that does not
/// parse is read again with cobc's tab stops where it holds a tab, and in free form where that
/// parses; so is one whose text fixed form cuts at column 72, in free form.
pub fn parse_all_with(text: &str, libraries: &copy::Libraries) -> Result<Vec<ast::Program>, Error> {
    let mut programs = parse_all_reading(text, libraries)?;
    if libraries.compliance() == numeric::Compliance::Extended {
        functions_elsewhere(&mut programs, libraries);
    }
    Ok(programs)
}

/// Under `--compliance extended`, each function a program's REPOSITORY paragraph names that its
/// source neither defines nor prototypes, as the libraries' program sources define it (IWX0092).
fn functions_elsewhere(programs: &mut [ast::Program], libraries: &copy::Libraries) {
    for p in programs.iter_mut() {
        for name in p.repository_functions.clone() {
            let known = p.prototypes.iter().chain(&p.later_functions).any(|q| q.name.eq_ignore_ascii_case(&name));
            if known || ast::is_intrinsic_name(&name) {
                continue;
            }
            if let Some((prototype, path)) = libraries.function_definition(&name) {
                let file = path.file_name().map_or_else(|| path.display().to_string(), |f| f.to_string_lossy().into_owned());
                p.elsewhere_functions.push((prototype, file));
            }
        }
    }
}

fn parse_all_reading(text: &str, libraries: &copy::Libraries) -> Result<Vec<ast::Program>, Error> {
    use numeric::{Compliance, SourceFormat};
    let start = match (libraries.compliance(), libraries.source_format()) {
        (Compliance::Extended, SourceFormat::Fixed) => source::Start::Fixed,
        (Compliance::Extended, SourceFormat::Free) => source::Start::Free(messages::IWX0001.at(Pos { file: 0, line: 1, col: 1 }, format!("{}: --source-format free reads the file in free form", source::FREE_FORM))),
        _ => source::Start::Detect,
    };
    let detect = libraries.compliance() == Compliance::Extended && libraries.source_format() == SourceFormat::Auto;
    if libraries.loose() {
        return parse_loose(text, libraries, start, detect);
    }
    if !detect {
        return parse_from(text, libraries, start).map(|(programs, _)| programs);
    }
    parse_detecting(text, libraries, start)
}

/// Under `--compliance loose`, the source parsed as [`parse_all_with`] parses it, and again after
/// each directive ironwork does not read, character outside COBOL's set or stray period in the
/// program's own file that stops it is left out, with IWX0065-W; up to 50 times. A source no reading
/// finds IDENTIFICATION DIVISION in is read again as a program named after its file.
fn parse_loose(text: &str, libraries: &copy::Libraries, start: source::Start, detect: bool) -> Result<Vec<ast::Program>, Error> {
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let mut notes = Vec::new();
    let assumed = libraries.assuming_program_id();
    for round in 0..=50 {
        let current = lines.join("\n") + "\n";
        let read = |libraries: &copy::Libraries| if detect { parse_detecting(&current, libraries, start.clone()) } else { parse_from(&current, libraries, start.clone()).map(|(p, _)| p) };
        let (parsed, libraries) = match read(libraries) {
            Err(e) if e.file.is_none() && e.message.starts_with("expected ID,") => (read(&assumed), &assumed),
            parsed => (parsed, libraries),
        };
        let e = match parsed {
            Ok(mut programs) => {
                // A record left out of a COPY member may be one cobc's tab stops read whole.
                let member_holes = |ps: &[ast::Program]| ps.iter().flat_map(|p| &p.messages).filter(|m| m.id == Some("IWX0064") && m.pos.file != 0).count();
                if member_holes(&programs) > 0
                    && let Ok((stopped, _)) = parse_from(&current, &libraries.with_member_tab_stops(), start.clone())
                    && member_holes(&stopped) < member_holes(&programs)
                {
                    programs = stopped;
                }
                if let Some(first) = programs.first_mut() {
                    first.messages.splice(0..0, notes);
                }
                return Ok(programs);
            }
            Err(e) => e,
        };
        let at = (e.pos.line as usize).checked_sub(1).filter(|&l| round < 50 && e.file.is_none() && l < lines.len());
        let Some(line) = at else { return Err(e) };
        let col = (e.pos.col as usize).saturating_sub(1);
        let what = match e.id {
            Some("IWS0094") => {
                lines[line] = String::new();
                "the directive's line is left out"
            }
            Some("IWS0021" | "IWS0026") => {
                let mut chars: Vec<char> = lines[line].chars().collect();
                if col >= chars.len() {
                    return Err(e);
                }
                chars[col] = ' ';
                lines[line] = chars.into_iter().collect();
                "the character is left out"
            }
            _ => return Err(e),
        };
        notes.push(messages::IWX0065.at(e.pos, format!("{} (--compliance loose): {what}", e.labelled())));
    }
    unreachable!("the last round returns")
}

/// [`parse_all_with`]'s reading under `--source-format auto`, given its first reading.
fn parse_detecting(text: &str, libraries: &copy::Libraries, start: source::Start) -> Result<Vec<ast::Program>, Error> {
    let fixed = parse_from(text, libraries, start.clone());
    let (at, why) = match &fixed {
        Ok((_, None)) => return fixed.map(|(programs, _)| programs),
        Ok((_, Some(cut))) => (*cut, format!("line {} runs on past column 72, where fixed form ends, in the middle of a word or literal", cut.line)),
        Err(e) => {
            let stops = format!("it stops {} ({})", if e.pos.file == 0 { format!("at line {}", e.pos.line) } else { "in a COPY member".to_owned() }, e.labelled());
            if let Some((line, col)) = text.lines().enumerate().find_map(|(i, l)| l.find('\t').map(|c| (i as u32 + 1, l[..c].chars().count() as u32 + 1))) {
                let warning = messages::IWX0058.at(Pos { file: 0, line, col }, format!("tab stops (GnuCOBOL and Micro Focus; Enterprise COBOL source holds no tab): read with each tab one column {stops}, and with each reaching the next column after a multiple of 8, as cobc places it, the file parses"));
                if let Ok((programs, None)) = parse_from(text, libraries, source::Start::TabStops(warning)) {
                    return Ok(programs);
                }
            }
            if e.file.is_some() {
                // Read with cobc's tab stops, the members get further or no further.
                match parse_from(text, &libraries.with_member_tab_stops(), start.clone()) {
                    Ok((programs, _)) => return Ok(programs),
                    Err(further) if (further.file.as_ref(), further.pos) != (e.file.as_ref(), e.pos) => return Err(further),
                    Err(_) => {}
                }
            }
            (Pos { file: 0, line: 1, col: 1 }, format!("read in fixed form {stops}"))
        }
    };
    let warning = messages::IWX0001.at(at, format!("{}: {why}, and the file reads in free form, as cobc -free reads it", source::FREE_FORM));
    match parse_from(text, libraries, source::Start::Free(warning)) {
        Ok((programs, _)) => Ok(programs),
        Err(_) => fixed.map(|(programs, _)| programs),
    }
}

/// The programs, and where a file read wholly in fixed form has text cut at column 72.
fn parse_from(text: &str, libraries: &copy::Libraries, start: source::Start) -> Result<(Vec<ast::Program>, Option<Pos>), Error> {
    let compliance = libraries.compliance();
    let mut files = vec![String::new()];
    let read = |debugging: bool, files: &mut Vec<String>| -> Result<(source::Source, Vec<lexer::Token>), Error> {
        let source = source::read_from(text, 0, debugging, compliance, start.clone()).and_then(|s| copy::expand(s, libraries, files)).and_then(copy::replace).map_err(|e| e.in_files(files))?;
        let tokens = lexer::lex_with(&source, compliance, libraries.empty_literal()).map_err(|e| e.in_files(files))?;
        Ok((source, tokens))
    };
    let (source, mut tokens) = match read(true, &mut files) {
        Ok((source, lexed)) => {
            let tokens = debugging::keep(lexed, source.debugging.as_deref().unwrap_or_default());
            (source, tokens)
        }
        Err(e) => {
            files.truncate(1);
            let (source, tokens) = read(false, &mut files)?;
            if debugging::requested(&tokens) {
                return Err(e);
            }
            (source, tokens)
        }
    };
    let cut = source::cut_at_the_margin(text, &source);
    if compliance == numeric::Compliance::Extended {
        tokens = extended::rewrite(tokens, &source.options, &source.constants).map_err(|e| e.in_files(&files))?;
    }
    let mut programs = parser::parse(&tokens, source.options, compliance, libraries.relaxed(), libraries.loose(), libraries.program_stem()).map_err(|e| e.in_files(&files))?;
    for p in &mut programs {
        p.sources = files.clone();
    }
    Ok((programs, cut))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Severity; 5] = [Severity::Informational, Severity::Warning, Severity::Error, Severity::Severe, Severity::Unrecoverable];

    fn message(severity: Severity, line: u32) -> Error {
        Error::at(Pos { file: 0, line, col: 8 }, format!("m{line}")).graded(severity)
    }

    #[test]
    fn severities_rise_with_ibms_return_codes() {
        assert_eq!(ALL.map(Severity::return_code), [0, 4, 8, 12, 16]);
        assert!(ALL.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(Error::at(Pos::default(), "m").severity, Severity::Severe);
        assert_eq!(Error::warning(Pos::default(), "m").severity, Severity::Warning);
    }

    #[test]
    fn a_compilation_returns_the_highest_code_of_its_messages() {
        let codes = |severities: &[Severity]| return_code(&severities.iter().enumerate().map(|(i, &s)| message(s, i as u32 + 1)).collect::<Vec<_>>());
        use Severity::*;
        assert_eq!(codes(&[]), 0);
        assert_eq!(codes(&[Informational, Informational]), 0);
        assert_eq!(codes(&[Informational, Warning]), 4);
        assert_eq!(codes(&[Warning, Error, Informational]), 8);
        assert_eq!(codes(&[Error, Severe, Warning]), 12);
        assert_eq!(codes(&[Informational, Warning, Error, Severe]), 12);
        assert_eq!(codes(&[Severe, Unrecoverable]), 16);
    }

    #[test]
    fn the_message_a_refusal_names_is_the_first_of_the_most_severe() {
        let messages = [message(Severity::Warning, 1), message(Severity::Severe, 2), message(Severity::Error, 3), message(Severity::Severe, 4)];
        assert_eq!(most_severe(&messages).map(|m| m.pos.line), Some(2));
        assert!(most_severe(&[]).is_none());
    }

    #[test]
    fn an_error_line_carries_no_label_and_a_warning_or_informational_one_says_which_it_is() {
        let lines: Vec<String> = ALL.map(|s| message(s, 7).place("p.cbl")).to_vec();
        assert_eq!(lines, ["p.cbl:7:8: informational: m7", "p.cbl:7:8: warning: m7", "p.cbl:7:8: m7", "p.cbl:7:8: m7", "p.cbl:7:8: m7"]);
        let unplaced = |s: Severity| Error::at(Pos::default(), "m").graded(s).place("p.cbl");
        assert_eq!((unplaced(Severity::Severe), unplaced(Severity::Warning)), ("p.cbl: m".to_string(), "p.cbl: warning: m".to_string()));
        let in_member = Error::warning(Pos { file: 1, line: 3, col: 8 }, "m3").in_files(&[String::new(), "COPYA".into()]);
        assert_eq!(in_member.place("p.cbl"), "COPYA:3:8: warning: m3");
        assert_eq!(message(Severity::Severe, 3).to_string(), "3:8: m3");
    }

    fn free_form_note(program: &ast::Program) -> Option<String> {
        program.messages.iter().find(|m| m.id == Some("IWX0001")).map(|m| m.message.clone())
    }

    fn under(text: &str, format: &str) -> Result<ast::Program, Error> {
        let flags = ["--compliance=extended".to_owned(), format!("--source-format={format}")];
        parse_with(text, &copy::Libraries::default().with_flags(&flags))
    }

    const HEAD: &str = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. P.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n";

    #[test]
    fn auto_reads_free_form_where_fixed_form_cuts_a_word_at_column_72_and_free_form_parses() {
        let text = format!("{HEAD}       01 A-LONG-NAME PIC X.\n       PROCEDURE DIVISION.\n{:<66}A-LONG-NAME.\n           GOBACK.\n", "           MOVE 'X' TO");
        let note = free_form_note(&under(&text, "auto").unwrap()).unwrap();
        assert!(note.contains("line 7 runs on past column 72") && note.ends_with("as cobc -free reads it"), "{note}");
        assert_eq!(free_form_note(&under(&text, "fixed").unwrap()), None);
        let strict = parse_with(&text, &copy::Libraries::default());
        assert!(strict.is_ok_and(|p| free_form_note(&p).is_none()));
    }

    #[test]
    fn auto_reads_free_form_where_fixed_form_does_not_parse_and_free_form_does() {
        let text = format!("{HEAD}01 B PIC X.\n       PROCEDURE DIVISION.\n           GOBACK.\n");
        let note = free_form_note(&under(&text, "auto").unwrap()).unwrap();
        assert!(note.contains("read in fixed form it stops at line 5 (IWS"), "{note}");
        assert!(under(&text, "fixed").is_err());
        let neither = format!("{HEAD}01 B PIC X VALUE.\n       PROCEDURE DIVISION.\n");
        assert!(under(&neither, "auto").is_err());
    }

    #[test]
    fn auto_reads_cobcs_tab_stops_where_a_tab_as_one_column_does_not_parse() {
        let text = "\t\tIDENTIFICATION DIVISION.\n\t\tPROGRAM-ID. P.\n\t\tPROCEDURE DIVISION.\n\t\t    GOBACK.\n";
        let program = under(text, "auto").unwrap();
        let note = program.messages.iter().find(|m| m.id == Some("IWX0058")).unwrap();
        assert_eq!((note.pos.line, note.pos.col), (1, 1));
        assert!(note.message.contains("read with each tab one column it stops at line 1"), "{}", note.message);
        assert!(free_form_note(&program).is_none());
        assert!(under(text, "fixed").is_err());
        let tabbed = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. P.\n       PROCEDURE DIVISION.\n\t    GOBACK.\n";
        assert!(under(tabbed, "auto").unwrap().messages.iter().all(|m| m.id != Some("IWX0058")));
    }

    #[test]
    fn relaxed_makes_a_sentence_that_does_not_parse_a_hole_up_to_its_period() {
        let text = format!("{HEAD}       PROCEDURE DIVISION.\n           DISPLAY 'A'.\n           FROB X\n               WIBBLE.\n           DISPLAY 'B'.\n");
        let flags = ["--compliance=extended".to_owned(), numeric::RELAXED.to_owned()];
        let program = parse_with(&text, &copy::Libraries::default().with_flags(&flags)).unwrap();
        let statements = &program.paragraphs[0].statements;
        assert!(matches!(&statements[2], ast::Stmt::Hole { construct, why, pos } if construct == "the sentence at line 7" && why == "IWS0001-S a statement, found FROB" && pos.line == 7), "{statements:?}");
        assert!(matches!(&statements[4], ast::Stmt::Display { pos, .. } if pos.line == 9), "{statements:?}");
        assert_eq!(program.messages.iter().filter(|m| m.id == Some("IWX0059")).count(), 1);
        assert!(parse_with(&text, &copy::Libraries::default().with_flags(&flags[..1])).is_err());
    }

    #[test]
    fn free_reads_every_line_in_free_form_and_says_the_option_did() {
        let text = "IDENTIFICATION DIVISION.\nPROGRAM-ID. P.\nPROCEDURE DIVISION.\nGOBACK.\n";
        let note = free_form_note(&under(text, "free").unwrap()).unwrap();
        assert!(note.ends_with("--source-format free reads the file in free form"), "{note}");
    }
}
