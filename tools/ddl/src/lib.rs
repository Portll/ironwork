//! Converts Db2 for z/OS DDL to PostgreSQL DDL for the ironwork test backend.

use std::fmt::Write;
use std::fs;
use std::path::Path;

#[derive(Debug)]
pub enum DdlError {
    Parse { line: usize, message: String },
    Io(std::io::Error),
}

impl std::fmt::Display for DdlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DdlError::Parse { line, message } => write!(f, "line {}: {}", line, message),
            DdlError::Io(e) => write!(f, "I/O error: {}", e),
        }
    }
}

impl std::error::Error for DdlError {}

impl From<std::io::Error> for DdlError {
    fn from(e: std::io::Error) -> Self {
        DdlError::Io(e)
    }
}

/// Converts Db2 DDL text to PostgreSQL DDL text.
pub fn convert(input: &str) -> Result<String, DdlError> {
    let tokens = lex(input)?;
    let statements = parse_statements(&tokens)?;
    let mut out = String::new();
    for stmt in &statements {
        match stmt {
            Statement::CreateTable(ct) => emit_create_table(ct, &mut out),
            Statement::CreateIndex(ci) => emit_create_index(ci, &mut out),
            Statement::Dropped { kind, name } => {
                let _ = writeln!(out, "-- DROPPED: {} {}", kind, name);
            }
        }
    }
    Ok(out)
}

/// Reads a Db2 DDL file and converts it to PostgreSQL DDL.
pub fn convert_file(path: &Path) -> Result<String, DdlError> {
    let input = fs::read_to_string(path)?;
    convert(&input)
}


#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    QuotedIdent(String),
    Number(String),
    StringLit(String),
    LParen,
    RParen,
    Comma,
    Semicolon,
    Equals,
    Dot,
    Eof,
}

impl Tok {
    fn is_keyword(&self, kw: &str) -> bool {
        match self {
            Tok::Ident(s) => s.eq_ignore_ascii_case(kw),
            _ => false,
        }
    }
}

struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
    line: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Lexer { src: src.as_bytes(), pos: 0, line: 1 }
    }

    fn next(&mut self) -> Result<Tok, DdlError> {
        self.skip_ws_and_comments();
        if self.pos >= self.src.len() {
            return Ok(Tok::Eof);
        }
        let c = self.src[self.pos] as char;
        match c {
            '(' => { self.pos += 1; Ok(Tok::LParen) }
            ')' => { self.pos += 1; Ok(Tok::RParen) }
            ',' => { self.pos += 1; Ok(Tok::Comma) }
            ';' => { self.pos += 1; Ok(Tok::Semicolon) }
            '=' => { self.pos += 1; Ok(Tok::Equals) }
            '.' => { self.pos += 1; Ok(Tok::Dot) }
            '"' => self.read_quoted_ident(),
            '\'' => self.read_string_lit(),
            _ if c.is_ascii_digit() => self.read_number(),
            _ if c.is_ascii_alphabetic() || c == '_' || c == '$' || c == '#' => self.read_ident(),
            _ => Err(DdlError::Parse {
                line: self.line,
                message: format!("unexpected character '{}'", c),
            }),
        }
    }

    fn skip_ws_and_comments(&mut self) {
        while self.pos < self.src.len() {
            let c = self.src[self.pos] as char;
            if c == '\n' {
                self.line += 1;
                self.pos += 1;
            } else if c == ' ' || c == '\t' || c == '\r' {
                self.pos += 1;
            } else if c == '-' && self.pos + 1 < self.src.len() && self.src[self.pos + 1] as char == '-' {
                while self.pos < self.src.len() && self.src[self.pos] as char != '\n' {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    fn read_ident(&mut self) -> Result<Tok, DdlError> {
        let start = self.pos;
        while self.pos < self.src.len() {
            let c = self.src[self.pos] as char;
            if c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '#' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let text = std::str::from_utf8(&self.src[start..self.pos]).unwrap().to_string();
        Ok(Tok::Ident(text))
    }

    fn read_quoted_ident(&mut self) -> Result<Tok, DdlError> {
        self.pos += 1;
        let start = self.pos;
        while self.pos < self.src.len() && self.src[self.pos] as char != '"' {
            if self.src[self.pos] as char == '\n' {
                self.line += 1;
            }
            self.pos += 1;
        }
        if self.pos >= self.src.len() {
            return Err(DdlError::Parse {
                line: self.line,
                message: "unterminated quoted identifier".into(),
            });
        }
        let text = std::str::from_utf8(&self.src[start..self.pos]).unwrap().to_string();
        self.pos += 1;
        Ok(Tok::QuotedIdent(text))
    }

    fn read_string_lit(&mut self) -> Result<Tok, DdlError> {
        self.pos += 1;
        let mut s = String::new();
        while self.pos < self.src.len() {
            let c = self.src[self.pos] as char;
            if c == '\'' {
                if self.pos + 1 < self.src.len() && self.src[self.pos + 1] as char == '\'' {
                    s.push('\'');
                    self.pos += 2;
                } else {
                    self.pos += 1;
                    return Ok(Tok::StringLit(s));
                }
            } else {
                if c == '\n' {
                    self.line += 1;
                }
                s.push(c);
                self.pos += 1;
            }
        }
        Err(DdlError::Parse {
            line: self.line,
            message: "unterminated string literal".into(),
        })
    }

    fn read_number(&mut self) -> Result<Tok, DdlError> {
        let start = self.pos;
        while self.pos < self.src.len() {
            let c = self.src[self.pos] as char;
            if c.is_ascii_digit() || c == '.' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let text = std::str::from_utf8(&self.src[start..self.pos]).unwrap().to_string();
        Ok(Tok::Number(text))
    }
}

fn lex(input: &str) -> Result<Vec<Tok>, DdlError> {
    let mut lx = Lexer::new(input);
    let mut tokens = Vec::new();
    loop {
        let tok = lx.next()?;
        if tok == Tok::Eof {
            break;
        }
        tokens.push(tok);
    }
    Ok(tokens)
}


#[derive(Debug, Clone)]
enum ColumnType {
    Char(u32),
    Varchar(u32),
    CharForBitData,
    VarcharForBitData,
    Graphic(u32),
    Vargraphic(u32),
    Decimal(u32, u32),
    Smallint,
    Integer,
    Bigint,
    Real,
    Double,
    Float,
    Date,
    Time,
    Timestamp,
    Clob,
    Blob,
}

impl ColumnType {
    fn to_pg(&self) -> String {
        match self {
            ColumnType::Char(n) => format!("CHAR({})", n),
            ColumnType::Varchar(n) => format!("VARCHAR({})", n),
            ColumnType::CharForBitData => "BYTEA".into(),
            ColumnType::VarcharForBitData => "BYTEA".into(),
            ColumnType::Graphic(n) => format!("CHAR({})", n),
            ColumnType::Vargraphic(n) => format!("VARCHAR({})", n),
            ColumnType::Decimal(p, s) => format!("NUMERIC({}, {})", p, s),
            ColumnType::Smallint => "SMALLINT".into(),
            ColumnType::Integer => "INTEGER".into(),
            ColumnType::Bigint => "BIGINT".into(),
            ColumnType::Real => "REAL".into(),
            ColumnType::Double => "DOUBLE PRECISION".into(),
            ColumnType::Float => "DOUBLE PRECISION".into(),
            ColumnType::Date => "DATE".into(),
            ColumnType::Time => "TIME".into(),
            ColumnType::Timestamp => "TIMESTAMP".into(),
            ColumnType::Clob => "TEXT".into(),
            ColumnType::Blob => "BYTEA".into(),
        }
    }

    fn implicit_default(&self) -> String {
        match self {
            ColumnType::Char(n) => {
                let blanks = " ".repeat(*n as usize);
                format!("'{}'", blanks)
            }
            ColumnType::Varchar(_) => "''".into(),
            ColumnType::CharForBitData => "X''".into(),
            ColumnType::VarcharForBitData => "X''".into(),
            ColumnType::Graphic(_) => "''".into(),
            ColumnType::Vargraphic(_) => "''".into(),
            ColumnType::Decimal(_, _) => "0".into(),
            ColumnType::Smallint => "0".into(),
            ColumnType::Integer => "0".into(),
            ColumnType::Bigint => "0".into(),
            ColumnType::Real => "0".into(),
            ColumnType::Double => "0".into(),
            ColumnType::Float => "0".into(),
            ColumnType::Date => "CURRENT_DATE".into(),
            ColumnType::Time => "CURRENT_TIME".into(),
            ColumnType::Timestamp => "CURRENT_TIMESTAMP".into(),
            ColumnType::Clob => "''".into(),
            ColumnType::Blob => "X''".into(),
        }
    }
}

#[derive(Debug, Clone)]
struct Column {
    name: String,
    data_type: ColumnType,
    not_null: bool,
    default: Option<String>,
    inline_pk: bool,
    inline_unique: bool,
}

#[derive(Debug, Clone)]
enum TableConstraint {
    PrimaryKey { name: Option<String>, columns: Vec<String> },
    Unique { name: Option<String>, columns: Vec<String> },
    ForeignKey {
        name: Option<String>,
        columns: Vec<String>,
        ref_table: String,
        ref_columns: Vec<String>,
    },
}

#[derive(Debug, Clone)]
struct CreateTable {
    name: String,
    columns: Vec<Column>,
    constraints: Vec<TableConstraint>,
    dropped: Vec<String>,
}

#[derive(Debug, Clone)]
struct CreateIndex {
    name: String,
    unique: bool,
    table: String,
    columns: Vec<String>,
    dropped: Vec<String>,
}

#[derive(Debug, Clone)]
enum Statement {
    CreateTable(CreateTable),
    CreateIndex(CreateIndex),
    Dropped { kind: String, name: String },
}


struct Parser {
    tokens: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Tok>) -> Self {
        Parser { tokens, pos: 0 }
    }

    fn peek(&self) -> &Tok {
        self.tokens.get(self.pos).unwrap_or(&Tok::Eof)
    }

    fn advance(&mut self) -> &Tok {
        let tok = self.tokens.get(self.pos).unwrap_or(&Tok::Eof);
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    fn expect(&mut self, expected: &Tok) -> Result<(), DdlError> {
        let tok = self.advance().clone();
        if &tok == expected {
            Ok(())
        } else {
            Err(DdlError::Parse {
                line: 0,
                message: format!("expected {:?}, got {:?}", expected, tok),
            })
        }
    }

    fn expect_keyword(&mut self, kw: &str) -> Result<(), DdlError> {
        let tok = self.advance().clone();
        if tok.is_keyword(kw) {
            Ok(())
        } else {
            Err(DdlError::Parse {
                line: 0,
                message: format!("expected keyword '{}', got {:?}", kw, tok),
            })
        }
    }

    fn parse_ident(&mut self) -> Result<String, DdlError> {
        let tok = self.advance().clone();
        match tok {
            Tok::Ident(s) | Tok::QuotedIdent(s) => Ok(s),
            other => Err(DdlError::Parse {
                line: 0,
                message: format!("expected identifier, got {:?}", other),
            }),
        }
    }

    fn parse_qualified_name(&mut self) -> Result<String, DdlError> {
        let first = self.parse_ident()?;
        if self.peek().clone() == Tok::Dot {
            self.advance();
            let second = self.parse_ident()?;
            Ok(format!("{}.{}", first, second))
        } else {
            Ok(first)
        }
    }

    fn parse_number(&mut self) -> Result<u32, DdlError> {
        let tok = self.advance().clone();
        match tok {
            Tok::Number(s) => s.parse::<u32>().map_err(|_| DdlError::Parse {
                line: 0,
                message: format!("invalid number '{}'", s),
            }),
            other => Err(DdlError::Parse {
                line: 0,
                message: format!("expected number, got {:?}", other),
            }),
        }
    }

    fn parse_ident_list(&mut self) -> Result<Vec<String>, DdlError> {
        self.expect(&Tok::LParen)?;
        let mut ids = Vec::new();
        ids.push(self.parse_ident()?);
        while self.peek().clone() == Tok::Comma {
            self.advance();
            ids.push(self.parse_ident()?);
        }
        self.expect(&Tok::RParen)?;
        Ok(ids)
    }

    /// An index's columns, each optionally ASC or DESC, which PostgreSQL writes the same way.
    fn parse_index_columns(&mut self) -> Result<Vec<String>, DdlError> {
        self.expect(&Tok::LParen)?;
        let mut cols = Vec::new();
        loop {
            let mut col = self.parse_ident()?;
            if (self.peek().is_keyword("ASC") || self.peek().is_keyword("DESC"))
                && let Tok::Ident(order) = self.advance().clone()
            {
                col = format!("{col} {}", order.to_ascii_uppercase());
            }
            cols.push(col);
            if self.peek().clone() != Tok::Comma {
                break;
            }
            self.advance();
        }
        self.expect(&Tok::RParen)?;
        Ok(cols)
    }

    fn parse_data_type(&mut self) -> Result<ColumnType, DdlError> {
        let tok = self.advance().clone();
        if !tok.is_keyword("CHAR") && !tok.is_keyword("VARCHAR") && !tok.is_keyword("GRAPHIC")
            && !tok.is_keyword("VARGRAPHIC") && !tok.is_keyword("DECIMAL")
            && !tok.is_keyword("DEC") && !tok.is_keyword("NUMERIC")
            && !tok.is_keyword("SMALLINT") && !tok.is_keyword("INTEGER")
            && !tok.is_keyword("INT") && !tok.is_keyword("BIGINT")
            && !tok.is_keyword("REAL") && !tok.is_keyword("DOUBLE")
            && !tok.is_keyword("FLOAT") && !tok.is_keyword("DATE")
            && !tok.is_keyword("TIME") && !tok.is_keyword("TIMESTAMP")
            && !tok.is_keyword("CLOB") && !tok.is_keyword("BLOB")
        {
            return Err(DdlError::Parse {
                line: 0,
                message: format!("unrecognized data type keyword {:?}", tok),
            });
        }

        let kw = match &tok {
            Tok::Ident(s) => s.to_uppercase(),
            _ => unreachable!(),
        };

        match kw.as_str() {
            "CHAR" => {
                let n = self.parse_char_length()?;
                if self.peek().is_keyword("FOR") {
                    self.advance();
                    self.expect_keyword("BIT")?;
                    self.expect_keyword("DATA")?;
                    Ok(ColumnType::CharForBitData)
                } else {
                    Ok(ColumnType::Char(n))
                }
            }
            "VARCHAR" => {
                let n = self.parse_char_length()?;
                if self.peek().is_keyword("FOR") {
                    self.advance();
                    self.expect_keyword("BIT")?;
                    self.expect_keyword("DATA")?;
                    Ok(ColumnType::VarcharForBitData)
                } else {
                    Ok(ColumnType::Varchar(n))
                }
            }
            "GRAPHIC" => {
                let n = self.parse_char_length()?;
                Ok(ColumnType::Graphic(n))
            }
            "VARGRAPHIC" => {
                let n = self.parse_char_length()?;
                Ok(ColumnType::Vargraphic(n))
            }
            "DECIMAL" | "DEC" | "NUMERIC" => {
                self.expect(&Tok::LParen)?;
                let p = self.parse_number()?;
                self.expect(&Tok::Comma)?;
                let s = self.parse_number()?;
                self.expect(&Tok::RParen)?;
                Ok(ColumnType::Decimal(p, s))
            }
            "SMALLINT" => Ok(ColumnType::Smallint),
            "INTEGER" | "INT" => Ok(ColumnType::Integer),
            "BIGINT" => Ok(ColumnType::Bigint),
            "REAL" => Ok(ColumnType::Real),
            "DOUBLE" => Ok(ColumnType::Double),
            "FLOAT" => {
                if self.peek().clone() == Tok::LParen {
                    self.advance();
                    let p = self.parse_number()?;
                    self.expect(&Tok::RParen)?;
                    if p <= 24 {
                        Ok(ColumnType::Real)
                    } else {
                        Ok(ColumnType::Double)
                    }
                } else {
                    Ok(ColumnType::Float)
                }
            }
            "DATE" => Ok(ColumnType::Date),
            "TIME" => Ok(ColumnType::Time),
            "TIMESTAMP" => Ok(ColumnType::Timestamp),
            "CLOB" => {
                // A LOB's length (32K, 1M, 2G) has no PostgreSQL counterpart and is read past.
                if self.peek().clone() == Tok::LParen {
                    while !matches!(self.peek(), Tok::RParen | Tok::Semicolon | Tok::Eof) {
                        self.advance();
                    }
                    self.expect(&Tok::RParen)?;
                }
                Ok(ColumnType::Clob)
            }
            "BLOB" => {
                if self.peek().clone() == Tok::LParen {
                    while !matches!(self.peek(), Tok::RParen | Tok::Semicolon | Tok::Eof) {
                        self.advance();
                    }
                    self.expect(&Tok::RParen)?;
                }
                Ok(ColumnType::Blob)
            }
            _ => unreachable!(),
        }
    }

    fn parse_char_length(&mut self) -> Result<u32, DdlError> {
        self.expect(&Tok::LParen)?;
        let n = self.parse_number()?;
        self.expect(&Tok::RParen)?;
        Ok(n)
    }

    fn parse_column(&mut self) -> Result<Column, DdlError> {
        let name = self.parse_ident()?;
        let data_type = self.parse_data_type()?;
        let mut not_null = false;
        let mut default: Option<String> = None;
        let mut inline_pk = false;
        let mut inline_unique = false;

        loop {
            let tok = self.peek().clone();
            if tok.is_keyword("NOT") {
                self.advance();
                self.expect_keyword("NULL")?;
                not_null = true;
            } else if tok.is_keyword("WITH") {
                self.advance();
                self.expect_keyword("DEFAULT")?;
                if self.peek().is_keyword("NOT")
                    || self.peek().is_keyword("PRIMARY")
                    || self.peek().is_keyword("UNIQUE")
                    || self.peek().clone() == Tok::Comma
                    || self.peek().clone() == Tok::RParen
                {
                    default = Some(data_type.implicit_default());
                } else {
                    default = Some(self.parse_default_value()?);
                }
            } else if tok.is_keyword("DEFAULT") {
                self.advance();
                default = Some(self.parse_default_value()?);
            } else if tok.is_keyword("PRIMARY") {
                self.advance();
                self.expect_keyword("KEY")?;
                inline_pk = true;
            } else if tok.is_keyword("UNIQUE") {
                self.advance();
                inline_unique = true;
            } else {
                break;
            }
        }

        Ok(Column { name, data_type, not_null, default, inline_pk, inline_unique })
    }

    fn parse_default_value(&mut self) -> Result<String, DdlError> {
        let tok = self.peek().clone();
        if tok.is_keyword("CURRENT") {
            self.advance();
            let sub = self.parse_ident()?;
            match sub.to_uppercase().as_str() {
                "DATE" => Ok("CURRENT_DATE".into()),
                "TIME" => Ok("CURRENT_TIME".into()),
                "TIMESTAMP" => Ok("CURRENT_TIMESTAMP".into()),
                other => Err(DdlError::Parse {
                    line: 0,
                    message: format!("unrecognized CURRENT {} in DEFAULT", other),
                }),
            }
        } else if tok.is_keyword("NULL") {
            self.advance();
            Ok("NULL".into())
        } else if matches!(tok, Tok::StringLit(_)) {
            let t = self.advance().clone();
            if let Tok::StringLit(s) = t {
                Ok(format!("'{}'", s.replace('\'', "''")))
            } else {
                unreachable!()
            }
        } else if matches!(tok, Tok::Number(_)) {
            let t = self.advance().clone();
            if let Tok::Number(s) = t {
                Ok(s)
            } else {
                unreachable!()
            }
        } else if tok.is_keyword("X") {
            self.advance();
            let t = self.advance().clone();
            if let Tok::StringLit(hex) = t {
                Ok(format!("X'{}'", hex))
            } else {
                Err(DdlError::Parse {
                    line: 0,
                    message: "expected string literal after X".into(),
                })
            }
        } else {
            let t = self.advance().clone();
            if let Tok::Ident(s) = t {
                Ok(s)
            } else {
                Err(DdlError::Parse {
                    line: 0,
                    message: format!("unexpected token in DEFAULT: {:?}", t),
                })
            }
        }
    }

    fn parse_table_constraint(&mut self) -> Result<TableConstraint, DdlError> {
        let mut name = None;
        if self.peek().is_keyword("CONSTRAINT") {
            self.advance();
            name = Some(self.parse_ident()?);
        }

        let tok = self.peek().clone();
        if tok.is_keyword("PRIMARY") {
            self.advance();
            self.expect_keyword("KEY")?;
            let columns = self.parse_ident_list()?;
            Ok(TableConstraint::PrimaryKey { name, columns })
        } else if tok.is_keyword("UNIQUE") {
            self.advance();
            let columns = self.parse_ident_list()?;
            Ok(TableConstraint::Unique { name, columns })
        } else if tok.is_keyword("FOREIGN") {
            self.advance();
            self.expect_keyword("KEY")?;
            let columns = self.parse_ident_list()?;
            self.expect_keyword("REFERENCES")?;
            let ref_table = self.parse_qualified_name()?;
            let ref_columns = if self.peek().clone() == Tok::LParen {
                self.parse_ident_list()?
            } else {
                Vec::new()
            };
            Ok(TableConstraint::ForeignKey { name, columns, ref_table, ref_columns })
        } else {
            Err(DdlError::Parse {
                line: 0,
                message: format!("expected table constraint, got {:?}", tok),
            })
        }
    }

    fn is_constraint_start(&self) -> bool {
        let tok = self.peek();
        tok.is_keyword("PRIMARY") || tok.is_keyword("UNIQUE") || tok.is_keyword("FOREIGN")
            || tok.is_keyword("CONSTRAINT")
    }

    fn parse_create_table(&mut self) -> Result<CreateTable, DdlError> {
        self.expect_keyword("CREATE")?;
        self.expect_keyword("TABLE")?;
        let name = self.parse_qualified_name()?;
        self.expect(&Tok::LParen)?;

        let mut columns = Vec::new();
        let mut constraints = Vec::new();

        loop {
            if self.is_constraint_start() {
                constraints.push(self.parse_table_constraint()?);
            } else {
                columns.push(self.parse_column()?);
            }

            if self.peek().clone() == Tok::Comma {
                self.advance();
            } else {
                break;
            }
        }
        self.expect(&Tok::RParen)?;

        let dropped = self.parse_table_options()?;

        Ok(CreateTable { name, columns, constraints, dropped })
    }

    fn parse_create_index(&mut self) -> Result<CreateIndex, DdlError> {
        self.expect_keyword("CREATE")?;
        let unique = if self.peek().is_keyword("UNIQUE") {
            self.advance();
            true
        } else {
            false
        };
        self.expect_keyword("INDEX")?;
        let name = self.parse_qualified_name()?;
        self.expect_keyword("ON")?;
        let table = self.parse_qualified_name()?;
        let columns = self.parse_index_columns()?;
        let dropped = self.parse_table_options()?;

        Ok(CreateIndex { name, unique, table, columns, dropped })
    }

    fn parse_table_options(&mut self) -> Result<Vec<String>, DdlError> {
        let mut dropped = Vec::new();
        let droppable_keywords = [
            "IN", "CCSID", "EDITPROC", "VALIDPROC", "AUDIT", "DATA",
            "BUFFERPOOL", "STOGROUP", "USING", "PRIQTY", "SECQTY",
        ];

        loop {
            let tok = self.peek().clone();
            if tok == Tok::Semicolon || tok == Tok::Eof {
                break;
            }
            if let Some(kw) = droppable_keywords.iter().find(|k| tok.is_keyword(k)) {
                let clause_name = kw.to_string();
                self.advance();
                self.skip_clause_args(&droppable_keywords);
                dropped.push(clause_name);
            } else {
                return Err(DdlError::Parse {
                    line: 0,
                    message: format!("unrecognized table option {:?}", tok),
                });
            }
        }
        Ok(dropped)
    }

    fn skip_clause_args(&mut self, droppable: &[&str]) {
        loop {
            let tok = self.peek().clone();
            if tok == Tok::Semicolon || tok == Tok::Eof {
                break;
            }
            if droppable.iter().any(|k| tok.is_keyword(k)) {
                break;
            }
            self.advance();
        }
    }

    fn parse_statements(&mut self) -> Result<Vec<Statement>, DdlError> {
        let mut statements = Vec::new();
        loop {
            let tok = self.peek().clone();
            if tok == Tok::Eof {
                break;
            }
            if tok.is_keyword("CREATE") {
                let next = self.tokens.get(self.pos + 1).cloned().unwrap_or(Tok::Eof);
                if next.is_keyword("TABLE") {
                    let ct = self.parse_create_table()?;
                    self.expect(&Tok::Semicolon)?;
                    statements.push(Statement::CreateTable(ct));
                } else if next.is_keyword("INDEX") || next.is_keyword("UNIQUE") {
                    let ci = self.parse_create_index()?;
                    self.expect(&Tok::Semicolon)?;
                    statements.push(Statement::CreateIndex(ci));
                } else if next.is_keyword("DATABASE") {
                    self.advance();
                    self.advance();
                    let name = self.parse_ident()?;
                    self.skip_to_semicolon();
                    statements.push(Statement::Dropped { kind: "CREATE DATABASE".into(), name });
                } else if next.is_keyword("TABLESPACE") {
                    self.advance();
                    self.advance();
                    let name = self.parse_ident()?;
                    self.skip_to_semicolon();
                    statements.push(Statement::Dropped { kind: "CREATE TABLESPACE".into(), name });
                } else if next.is_keyword("STOGROUP") {
                    self.advance();
                    self.advance();
                    let name = self.parse_ident()?;
                    self.skip_to_semicolon();
                    statements.push(Statement::Dropped { kind: "CREATE STOGROUP".into(), name });
                } else {
                    return Err(DdlError::Parse {
                        line: 0,
                        message: format!("unrecognized CREATE statement: CREATE {:?}", next),
                    });
                }
            } else {
                return Err(DdlError::Parse {
                    line: 0,
                    message: format!("expected CREATE, got {:?}", tok),
                });
            }
        }
        Ok(statements)
    }

    fn skip_to_semicolon(&mut self) {
        loop {
            let tok = self.peek().clone();
            if tok == Tok::Semicolon || tok == Tok::Eof {
                if tok == Tok::Semicolon {
                    self.advance();
                }
                break;
            }
            self.advance();
        }
    }
}

fn parse_statements(tokens: &[Tok]) -> Result<Vec<Statement>, DdlError> {
    let mut parser = Parser::new(tokens.to_vec());
    parser.parse_statements()
}


fn emit_create_table(ct: &CreateTable, out: &mut String) {
    for d in &ct.dropped {
        let _ = writeln!(out, "-- DROPPED: {}", d);
    }
    let _ = writeln!(out, "CREATE TABLE {} (", ct.name);

    for (i, col) in ct.columns.iter().enumerate() {
        let sep = if i + 1 < ct.columns.len() + ct.constraints.len() { "," } else { "" };
        let _ = write!(out, "    {} {}", col.name, col.data_type.to_pg());
        if col.not_null {
            let _ = write!(out, " NOT NULL");
        }
        if let Some(ref d) = col.default {
            let _ = write!(out, " DEFAULT {}", d);
        }
        if col.inline_pk {
            let _ = write!(out, " PRIMARY KEY");
        }
        if col.inline_unique {
            let _ = write!(out, " UNIQUE");
        }
        let _ = writeln!(out, "{}", sep);
    }

    for (i, constraint) in ct.constraints.iter().enumerate() {
        let sep = if i + 1 < ct.constraints.len() { "," } else { "" };
        match constraint {
            TableConstraint::PrimaryKey { name, columns } => {
                if let Some(n) = name {
                    let _ = write!(out, "    CONSTRAINT {} PRIMARY KEY (", n);
                } else {
                    let _ = write!(out, "    PRIMARY KEY (");
                }
                let _ = write!(out, "{})", columns.join(", "));
                let _ = writeln!(out, "{}", sep);
            }
            TableConstraint::Unique { name, columns } => {
                if let Some(n) = name {
                    let _ = write!(out, "    CONSTRAINT {} UNIQUE (", n);
                } else {
                    let _ = write!(out, "    UNIQUE (");
                }
                let _ = write!(out, "{})", columns.join(", "));
                let _ = writeln!(out, "{}", sep);
            }
            TableConstraint::ForeignKey { name, columns, ref_table, ref_columns } => {
                if let Some(n) = name {
                    let _ = write!(out, "    CONSTRAINT {} FOREIGN KEY (", n);
                } else {
                    let _ = write!(out, "    FOREIGN KEY (");
                }
                let _ = write!(out, "{}) REFERENCES {}", columns.join(", "), ref_table);
                if !ref_columns.is_empty() {
                    let _ = write!(out, " ({})", ref_columns.join(", "));
                }
                let _ = writeln!(out, "{}", sep);
            }
        }
    }

    let _ = writeln!(out, ");");
}

fn emit_create_index(ci: &CreateIndex, out: &mut String) {
    for d in &ci.dropped {
        let _ = writeln!(out, "-- DROPPED: {}", d);
    }
    let unique = if ci.unique { "UNIQUE " } else { "" };
    let _ = writeln!(out, "CREATE {unique}INDEX {} ON {} ({});", ci.name, ci.table, ci.columns.join(", "));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_char_and_int() {
        let input = "CREATE TABLE T1 (\n  A CHAR(10) NOT NULL,\n  B INTEGER\n);";
        let out = convert(input).unwrap();
        assert!(out.contains("CREATE TABLE T1 ("));
        assert!(out.contains("A CHAR(10) NOT NULL"));
        assert!(out.contains("B INTEGER"));
        assert!(out.contains(");"));
    }

    #[test]
    fn varchar_for_bit_data_maps_to_bytea() {
        let input = "CREATE TABLE T2 (\n  A VARCHAR(255) FOR BIT DATA\n);";
        let out = convert(input).unwrap();
        assert!(out.contains("A BYTEA"));
    }

    #[test]
    fn decimal_maps_to_numeric() {
        let input = "CREATE TABLE T3 (\n  A DECIMAL(10,2),\n  B DEC(5,0),\n  C NUMERIC(18,4)\n);";
        let out = convert(input).unwrap();
        assert!(out.contains("A NUMERIC(10, 2)"));
        assert!(out.contains("B NUMERIC(5, 0)"));
        assert!(out.contains("C NUMERIC(18, 4)"));
    }

    #[test]
    fn not_null_with_default_implicit() {
        let input = "CREATE TABLE T4 (\n  A CHAR(5) NOT NULL WITH DEFAULT,\n  B INTEGER NOT NULL WITH DEFAULT,\n  C DATE NOT NULL WITH DEFAULT\n);";
        let out = convert(input).unwrap();
        assert!(out.contains("A CHAR(5) NOT NULL DEFAULT '     '"));
        assert!(out.contains("B INTEGER NOT NULL DEFAULT 0"));
        assert!(out.contains("C DATE NOT NULL DEFAULT CURRENT_DATE"));
    }

    #[test]
    fn explicit_default_value() {
        let input = "CREATE TABLE T5 (\n  A INTEGER DEFAULT 42,\n  B CHAR(3) DEFAULT 'abc'\n);";
        let out = convert(input).unwrap();
        assert!(out.contains("A INTEGER DEFAULT 42"));
        assert!(out.contains("B CHAR(3) DEFAULT 'abc'"));
    }

    #[test]
    fn primary_key_table_constraint() {
        let input = "CREATE TABLE T6 (\n  A INTEGER,\n  B CHAR(10),\n  PRIMARY KEY (A, B)\n);";
        let out = convert(input).unwrap();
        assert!(out.contains("PRIMARY KEY (A, B)"));
    }

    #[test]
    fn inline_primary_key() {
        let input = "CREATE TABLE T7 (\n  A INTEGER PRIMARY KEY,\n  B CHAR(10)\n);";
        let out = convert(input).unwrap();
        assert!(out.contains("A INTEGER PRIMARY KEY"));
    }

    #[test]
    fn foreign_key_constraint() {
        let input = "CREATE TABLE T8 (\n  A INTEGER,\n  B INTEGER,\n  FOREIGN KEY (B) REFERENCES T6 (A)\n);";
        let out = convert(input).unwrap();
        assert!(out.contains("FOREIGN KEY (B) REFERENCES T6 (A)"));
    }

    #[test]
    fn create_unique_index() {
        let input = "CREATE UNIQUE INDEX IDX1 ON T1 (A, B);";
        let out = convert(input).unwrap();
        assert!(out.contains("CREATE UNIQUE INDEX IDX1 ON T1 (A, B)"));
    }

    #[test]
    fn a_qualified_index_with_ordered_columns() {
        let out = convert("CREATE UNIQUE INDEX PAY.XEMP ON PAY.EMP (EMPNO ASC, HIRED DESC) BUFFERPOOL BP1;").unwrap();
        assert!(out.contains("CREATE UNIQUE INDEX PAY.XEMP ON PAY.EMP (EMPNO ASC, HIRED DESC)"), "{out}");
        assert!(out.contains("BUFFERPOOL"), "the dropped clause is kept as a comment: {out}");
    }

    #[test]
    fn a_lob_length_with_a_unit_is_read_past() {
        let out = convert("CREATE TABLE T (P BLOB(1M), D CLOB(32K));").unwrap();
        assert!(out.contains("P BYTEA") && out.contains("D TEXT"), "{out}");
    }

    #[test]
    fn create_plain_index() {
        let input = "CREATE INDEX IDX2 ON T1 (B);";
        let out = convert(input).unwrap();
        assert!(out.contains("CREATE INDEX IDX2 ON T1 (B)"));
    }

    #[test]
    fn dropped_clauses_listed_as_comments() {
        let input = "CREATE TABLE T9 (\n  A INTEGER\n) IN MYDB.MYSTS\n  CCSID 1208\n  BUFFERPOOL BP0;";
        let out = convert(input).unwrap();
        assert!(out.contains("-- DROPPED: IN"));
        assert!(out.contains("-- DROPPED: CCSID"));
        assert!(out.contains("-- DROPPED: BUFFERPOOL"));
        assert!(out.contains("CREATE TABLE T9 ("));
    }

    #[test]
    fn create_database_dropped() {
        let input = "CREATE DATABASE MYDB;";
        let out = convert(input).unwrap();
        assert!(out.contains("-- DROPPED: CREATE DATABASE MYDB"));
    }

    #[test]
    fn create_tablespace_dropped() {
        let input = "CREATE TABLESPACE MYTS;";
        let out = convert(input).unwrap();
        assert!(out.contains("-- DROPPED: CREATE TABLESPACE MYTS"));
    }

    #[test]
    fn comments_are_ignored() {
        let input = "-- this is a comment\nCREATE TABLE T10 (\n  A INTEGER -- inline comment\n);";
        let out = convert(input).unwrap();
        assert!(out.contains("CREATE TABLE T10 ("));
        assert!(out.contains("A INTEGER"));
        assert!(!out.contains("this is a comment"));
    }

    #[test]
    fn float_precision_mapping() {
        let input = "CREATE TABLE T11 (\n  A FLOAT,\n  B FLOAT(24),\n  C FLOAT(53)\n);";
        let out = convert(input).unwrap();
        assert!(out.contains("A DOUBLE PRECISION"));
        assert!(out.contains("B REAL"));
        assert!(out.contains("C DOUBLE PRECISION"));
    }

    #[test]
    fn clob_and_blob_mapping() {
        let input = "CREATE TABLE T12 (\n  A CLOB(10000),\n  B BLOB(4096)\n);";
        let out = convert(input).unwrap();
        assert!(out.contains("A TEXT"));
        assert!(out.contains("B BYTEA"));
    }

    #[test]
    fn graphic_and_vargraphic_mapping() {
        let input = "CREATE TABLE T13 (\n  A GRAPHIC(20),\n  B VARGRAPHIC(100)\n);";
        let out = convert(input).unwrap();
        assert!(out.contains("A CHAR(20)"), "GRAPHIC is fixed length");
        assert!(out.contains("B VARCHAR(100)"));
    }

    #[test]
    fn multiple_statements() {
        let input = "CREATE TABLE A (X INTEGER);\nCREATE TABLE B (Y CHAR(5));";
        let out = convert(input).unwrap();
        assert!(out.contains("CREATE TABLE A ("));
        assert!(out.contains("CREATE TABLE B ("));
    }

    #[test]
    fn named_constraint_preserved() {
        let input = "CREATE TABLE T14 (\n  A INTEGER,\n  CONSTRAINT PK_T14 PRIMARY KEY (A)\n);";
        let out = convert(input).unwrap();
        assert!(out.contains("CONSTRAINT PK_T14 PRIMARY KEY (A)"));
    }

    #[test]
    fn with_default_nullable() {
        let input = "CREATE TABLE T15 (\n  A INTEGER WITH DEFAULT\n);";
        let out = convert(input).unwrap();
        assert!(out.contains("A INTEGER DEFAULT 0"));
        assert!(!out.contains("NOT NULL"));
    }

    #[test]
    fn schema_qualified_table_name() {
        let input = "CREATE TABLE MYSCHEMA.MYTABLE (\n  A INTEGER\n);";
        let out = convert(input).unwrap();
        assert!(out.contains("CREATE TABLE MYSCHEMA.MYTABLE ("));
    }

    #[test]
    fn error_on_unrecognized_token() {
        let input = "CREATE TABLE T16 (\n  A FOOBAR\n);";
        let result = convert(input);
        assert!(result.is_err());
    }
}
