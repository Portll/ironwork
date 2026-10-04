//! ironwork for COBOL: from fixed-format source text to a syntax tree. Nothing here knows about
//! storage or execution.

pub mod ast;
pub mod bms;
pub mod copy;
pub mod csd;
mod debugging;
pub mod dli;
pub mod extended;
pub mod feedback;
pub mod jni;
pub mod lexer;
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
}

/// A compiler message; [`Error::at`] makes a severe one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub pos: Pos,
    pub message: String,
    /// The COPY member the position is in, when it is not the program itself.
    pub file: Option<String>,
    pub severity: Severity,
}

impl Error {
    pub fn at(pos: Pos, message: impl Into<String>) -> Self {
        Self { pos, message: message.into(), file: None, severity: Severity::Severe }
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
    /// message puts its label before the message: `file:line:col: warning: message`.
    pub fn place(&self, main: &str) -> String {
        match (&self.file, self.pos.line) {
            (Some(f), _) => format!("{f}:{self}"),
            (None, 0) => format!("{main}: {}", self.labelled()),
            (None, _) => format!("{main}:{self}"),
        }
    }

    /// The message, after its severity's label when it has one.
    pub fn labelled(&self) -> String {
        match self.severity.label() {
            Some(label) => format!("{label}: {}", self.message),
            None => self.message.clone(),
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

/// Every program in the source, in order, nested programs after the one that contains them. The
/// libraries' compliance level says how the source and its members are read. Debugging lines are
/// program text through COPY and REPLACE, and comments after them outside a program compiled WITH
/// DEBUGGING MODE (Language Reference SC27-8713-03, p. 693). Outside debugging mode Enterprise
/// COBOL accepts a debugging line that does not read as text, such as one holding an unclosed
/// literal; then every debugging line is read as a comment.
pub fn parse_all_with(text: &str, libraries: &copy::Libraries) -> Result<Vec<ast::Program>, Error> {
    let compliance = libraries.compliance();
    let mut files = vec![String::new()];
    let read = |debugging: bool, files: &mut Vec<String>| -> Result<(source::Source, Vec<lexer::Token>), Error> {
        let source = source::read_under(text, 0, debugging, compliance).and_then(|s| copy::expand(s, libraries, files)).and_then(copy::replace).map_err(|e| e.in_files(files))?;
        let tokens = lexer::lex_under(&source, compliance).map_err(|e| e.in_files(files))?;
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
    if compliance == numeric::Compliance::Extended {
        tokens = extended::rewrite(tokens, &source.options).map_err(|e| e.in_files(&files))?;
    }
    let mut programs = parser::parse(&tokens, source.options).map_err(|e| e.in_files(&files))?;
    for p in &mut programs {
        p.sources = files.clone();
    }
    Ok(programs)
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
}
