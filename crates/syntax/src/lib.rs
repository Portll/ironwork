//! ironwork for COBOL: from fixed-format source text to a syntax tree. Nothing here knows about
//! storage or execution.

pub mod ast;
pub mod bms;
pub mod copy;
pub mod lexer;
pub mod parser;
pub mod source;
pub mod system;

use std::fmt;

/// Where a diagnostic points: 1-based source line and column.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pos {
    /// Index into the program's file table: 0 for the program itself, then each COPY member.
    pub file: u16,
    pub line: u32,
    pub col: u32,
}

impl fmt::Display for Pos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub pos: Pos,
    pub message: String,
    /// The COPY member the position is in, when it is not the program itself.
    pub file: Option<String>,
}

impl Error {
    pub fn at(pos: Pos, message: impl Into<String>) -> Self {
        Self { pos, message: message.into(), file: None }
    }

    /// Names the member a position in a COPY member came from.
    pub fn in_files(mut self, files: &[String]) -> Self {
        self.file = files.get(self.pos.file as usize).filter(|f| !f.is_empty()).cloned();
        self
    }

    /// `file:line:col: message`, with `main` naming the program itself.
    pub fn place(&self, main: &str) -> String {
        match (&self.file, self.pos.line) {
            (Some(f), _) => format!("{f}:{self}"),
            (None, 0) => format!("{main}: {}", self.message),
            (None, _) => format!("{main}:{self}"),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.pos, self.message)
    }
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

/// Every program in the source, in order, nested programs after the one that contains them.
pub fn parse_all_with(text: &str, libraries: &copy::Libraries) -> Result<Vec<ast::Program>, Error> {
    let mut files = vec![String::new()];
    let source = source::read(text).and_then(|s| copy::expand(s, libraries, &mut files)).map_err(|e| e.in_files(&files))?;
    let tokens = lexer::lex(&source).map_err(|e| e.in_files(&files))?;
    let mut programs = parser::parse(&tokens, source.options).map_err(|e| e.in_files(&files))?;
    for p in &mut programs {
        p.sources = files.clone();
    }
    Ok(programs)
}
